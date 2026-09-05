#!/usr/bin/env python3
"""Lexical/source wiring for pinned Matrix filters; never runtime qualification."""
from __future__ import annotations
import importlib.util
import json
from pathlib import Path
import re
from rust_route_contract import tokenize

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('filter_projection_guard', ROOT/'scripts/check-matrix-filter-recovery.py')
assert SPEC and SPEC.loader
G = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(G)
POLLER = 'apps/matrix-bot-poller/src/main.rs'
RESOLVER = 'apps/matrix-bot-poller/src/filter_definition.rs'
SCOPE = 'apps/matrix-bot-poller/src/stream_scope.rs'
SQL = 'services/matrix-entry-adapter/migrations/0005_filter_definition_pins.sql'
REGRESSION = 'scripts/test-matrix-filter-definition-postgres.sql'
DRIVER = 'scripts/matrix_postgres_regression.py'
WORKFLOWS = ('.github/workflows/matrix-review-repair-regression.yml', '.github/workflows/rust-service-gate.yml')
PATHS = (POLLER, RESOLVER, SCOPE, SQL, REGRESSION, DRIVER, *WORKFLOWS)


def tokens(text: str) -> list[tuple[str, str]]:
    return [(t.kind, t.text) for t in tokenize(text)]


def ordered(body: str, *needles: str) -> None:
    haystack = tokens(body)
    offset = 0
    for needle in needles:
        want = tokens(needle)
        found = next((i for i in range(offset, len(haystack) - len(want) + 1)
                      if haystack[i:i+len(want)] == want), None)
        if found is None:
            raise AssertionError('missing ordered executable token sequence: ' + needle)
        offset = found + len(want)


def function(source: str, name: str) -> str:
    return G.function_body(source, name)


def validate(s: dict[str, str]) -> None:
    p, d, scope = s[POLLER], s[RESOLVER], s[SCOPE]
    ordered(function(p, 'main'), 'PollerConfig::from_env()', 'verify_homeserver_account(',
            'config.resolved_filter = filter_definition::resolve(', 'run(config, pool, http)')
    ordered(function(p, 'from_env'), 'stream_scope::configured_filter(', 'stream_scope::describe(',
            'filter_definition::configured_digest(', 'env::var("MATRIX_SYNC_FILTER_DEFINITION_SHA256")')
    ordered(function(d, 'resolve'), 'valid_digest(expected)', 'definition_url(base, user, id)',
            'timeout(Duration::from_secs(10)', 'http.get(url).bearer_auth(token).send()',
            'response.status() != StatusCode::OK', 'crate::read_bounded_body(response, DEFINITION_MAX_BYTES)',
            'verify_definition(&bytes, expected)')
    ordered(function(d, 'verify_definition'), 'bytes.len() > DEFINITION_MAX_BYTES',
            'Sha256::digest(bytes)', 'if digest != expected', 'std::str::from_utf8(bytes)',
            'stream_scope::validate_inline(raw)?')
    ordered(function(d, 'effective_filter'), '(None, None) => Ok(None)',
            'stream_scope::validate_inline(raw)?', 'Ok(Some(definition.bytes()))',
            'Err("matrix_filter_definition_unresolved")')
    ordered(function(p, 'build_sync_url'), 'filter_definition::effective_filter(',
            'config.sync_filter.as_deref(), config.resolved_filter.as_ref()', 'query.append_pair("filter", filter)')
    ordered(function(p, 'recover_limited_timelines'), 'let effective = filter_definition::effective_filter(',
            'stream_scope::backfill_filter(effective, room_id)', 'renew_cursor_lease(', 'http.get(url)')
    ordered(function(p, 'bind_stream_scope'), '"select public.cex_matrix_bind_stream_scope_v1($1,$2,$3,$4,$5)"',
            'filter_definition::effective_filter(', '"select public.cex_matrix_bind_filter_definition_v1($1,$2,$3,$4,$5,$6,$7)"',
            '.bind(lease.lease_fence).bind(lease.cursor_revision).bind(filter_id)',
            '.bind(definition.bytes()).bind(definition.sha256())', '.fetch_one(&mut **tx)')
    ordered(function(p, 'poll_once'), 'bind_stream_scope(', 'scope_tx.commit()', 'build_sync_url(', 'request.send()')
    ordered(function(p, 'persist_batch'), 'bind_stream_scope(', '"select public.cex_matrix_accept_source_event_v1($1, $2, $3, $4)"')
    G.require(function(p, 'verify_schema'),
              "cex_matrix_bind_filter_definition_v1(text,text,bigint,bigint,text,text,text)")
    ordered(function(scope, 'present_non_null'), 'Option::<T>::deserialize(deserializer)?',
            '.map(Some)', '.ok_or_else(')
    # Check each typed optional field, not a marker elsewhere in tests.
    for name in ('SyncFilter', 'RoomFilter', 'RoomEventFilter'):
        body = scope.split('struct ' + name + ' {', 1)[1].split('\n}', 1)[0]
        for match in re.finditer(r'(?m)^    \w+: Option<', body):
            previous = body[:match.start()].rstrip().splitlines()[-1]
            if previous.strip() != '#[serde(default, deserialize_with = "present_non_null")]':
                raise AssertionError('nullable selector: ' + name)
    sql = s[SQL]
    G.require(sql, 'for update', 'matrix_filter_definition_legacy_review_required',
              "stream_scope->'filter'->>'value' is distinct from p_filter_id",
              'bound.definition is distinct from p_definition',
              'bound.definition_sha256 is distinct from p_sha256',
              "encode(sha256(convert_to(definition, 'UTF8')), 'hex')",
              'cursor_row.opaque_cursor is distinct from p_expected_cursor',
              'new.approved_by is distinct from current_user::text',
              'matrix_filter_definition_active_lease', 'before update or delete',
              'cex_matrix_reject_immutable_mutation_v1()', 'revoke all on function')
    for name in ('cex_matrix_guard_filter_definition_insert_v1', 'cex_matrix_approve_legacy_filter_definition_v1'):
        body = sql.split('create or replace function public.' + name + '(', 1)[1].split('\n$$;', 1)[0]
        G.require(body, 'current_user <> table_owner')
    for forbidden in ('security definer', 'disable trigger', 'grant all', 'update public.matrix_transport_cursors'):
        if forbidden in sql.lower():
            raise AssertionError('forbidden filter-pin migration operation')
    if not sql.startswith('begin;') or not sql.rstrip().endswith('commit;'):
        raise AssertionError('nontransactional filter-pin migration')
    G.require(s[DRIVER], Path(SQL).name, REGRESSION, '"matrix_transport_filter_definitions"')
    G.require(s[REGRESSION], 'legacy definition was auto-approved', 'database accepted digest without matching bytes',
              'runtime role approved a legacy definition', 'wrong legacy cursor was approved for definition', 'rollback;')
    for path in WORKFLOWS:
        G.require(s[path], 'python3 scripts/test-matrix-filter-definition.py',
                  'python3 scripts/check-matrix-filter-definition.py')


def sources() -> dict[str, str]:
    return {p: (ROOT/p).read_text(encoding='utf-8') for p in PATHS}


def main() -> int:
    try:
        validate(sources())
    except (AssertionError, ValueError, IndexError, OSError) as e:
        print('Matrix filter definition source FAILED: ' + str(e))
        return 1
    print(json.dumps({'schema':'cex.matrix-filter-definition-source.v1','status':'ok',
                      'rust_executed':False,'postgres_executed':False,'production_authorization':'not_granted'}))
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
