#!/usr/bin/env python3
"""Source wiring only. Does not execute Rust, SQL, or homeserver requests."""
from __future__ import annotations
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLLER = 'apps/matrix-bot-poller/src/main.rs'
SCOPE = 'apps/matrix-bot-poller/src/stream_scope.rs'
SQL = 'services/matrix-entry-adapter/migrations/0004_stream_scope_binding.sql'
TEST = 'scripts/test-matrix-stream-scope-postgres.sql'
DRIVER = 'scripts/matrix_postgres_regression.py'
REPLY = 'apps/matrix-bot-relay/src/response_contract.rs'


def validate(sources: dict[str, str]) -> None:
    spec = importlib.util.spec_from_file_location('matrix_wiring_scope', ROOT / 'scripts/check-matrix-runtime-wiring.py')
    assert spec and spec.loader
    wiring = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(wiring)
    fn, ordered = wiring.function_source, wiring.require_order
    p, s, migration = sources[POLLER], sources[SCOPE], sources[SQL]
    def require(text, *markers):
        for marker in markers:
            if marker not in text:
                raise AssertionError('missing stream-scope source contract: ' + marker)
    ordered(p, 'main', ['verify_schema(', 'verify_homeserver_account(', 'run(config, pool, http)'])
    ordered(p, 'poll_once', ['pool.begin()', 'bind_stream_scope(', 'scope_tx.commit()', 'build_sync_url(', 'request.send()'])
    ordered(p, 'persist_batch', ['pool.begin()', 'bind_stream_scope(', 'cex_matrix_accept_source_event_v1',
                                 'cex_matrix_advance_cursor_v1', 'tx.commit()'])
    require(fn(p, 'verify_homeserver_account'), 'Duration::from_secs(10)', 'read_bounded_body(response, 4096)',
            'response.status() != reqwest::StatusCode::OK', 'stream_scope::verify_account')
    require(fn(p, 'bind_stream_scope'), 'cex_matrix_bind_stream_scope_v1', '.bind(lease.lease_fence)',
            '.bind(lease.cursor_revision)', '"bound" | "replay"', 'matrix_stream_binding_unverified')
    require(fn(p, 'verify_schema'), 'cex_matrix_bind_stream_scope_v1(text,text,bigint,bigint,jsonb)')
    require(p, 'stream_scope::configured_filter(env::var("MATRIX_SYNC_FILTER"))')
    require(s, 'Err(VarError::NotPresent) => Ok(None)', '=> Ok(Some(value))',
            'configured_zero_is_an_id_and_only_absence_disables_filter')
    require(s, '"cex.matrix.stream-scope.v1"', '"kind": "inline", "value": raw',
            'body.get("user_id").and_then(Value::as_str) != Some(expected)',
            '"matrix_stream_account_unverified"', 'raw.len() > 4096')
    require(migration, 'for update', 'matrix_stream_scope_legacy_review_required',
            'matrix_stream_scope_active_lease', 'current_user <> scope_owner',
            'cursor_row.opaque_cursor is distinct from p_expected_cursor',
            'new.approved_by is distinct from current_user::text',
            'before update or delete', 'cex_matrix_reject_immutable_mutation_v1()',
            'revoke all on function', "scope->'schema' = '\"cex.matrix.stream-scope.v1\"'::jsonb",
            "scope->'filter'->'kind' in ('\"inline\"'::jsonb, '\"id\"'::jsonb)")
    for name in ['cex_matrix_guard_stream_scope_insert_v1', 'cex_matrix_approve_legacy_stream_scope_v1']:
        prefix = 'create or replace function public.' + name + '('
        if migration.count(prefix) != 1:
            raise AssertionError('scope SQL function identity changed')
        body = migration.split(prefix, 1)[1].split('\n$$;', 1)[0]
        require(body, 'current_user <> scope_owner')
    for forbidden in ['security definer', 'disable trigger', 'grant all', 'update public.matrix_transport_cursors']:
        if forbidden in migration.lower():
            raise AssertionError('forbidden scope mutation/authority: ' + forbidden)
    if not migration.startswith('begin;') or not migration.rstrip().endswith('commit;'):
        raise AssertionError('scope migration must be transactional')
    require(sources[DRIVER], Path(SQL).name, Path(TEST).as_posix())
    require(sources[TEST], 'legacy cursor was auto-approved', 'live worker scope was approved',
            'wrong legacy cursor was approved', 'changed filter reused cursor',
            'runtime role approved a legacy scope', 'NULL schema bypassed scope shape', 'rollback;')
    reply_source = ''.join(fn(sources[REPLY], 'bound_reply').split())
    require(reply_source, 'object.keys().all', '"msgtype"|"body"')


def sources() -> dict[str, str]:
    return {name: (ROOT / name).read_text(encoding='utf-8') for name in [POLLER, SCOPE, SQL, TEST, DRIVER, REPLY]}


def main() -> int:
    try:
        validate(sources())
    except (AssertionError, OSError, ValueError) as error:
        print('Matrix stream scope source contract FAILED: ' + str(error))
        return 1
    print(json.dumps({'schema':'cex.matrix-stream-scope-source-check.v1', 'status':'ok',
                      'runtime_execution_proven':False, 'production_authorization':'not_granted'}))
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
