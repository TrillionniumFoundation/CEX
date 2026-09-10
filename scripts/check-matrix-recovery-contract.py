#!/usr/bin/env python3
"""Reject drift in Matrix recovery source contracts; not runtime qualification.

This intentionally checks source, catalog and test wiring only. Cargo, PostgreSQL
and a real homeserver must still execute before a candidate is qualified.
"""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLLER = 'apps/matrix-bot-poller/src/main.rs'
RELAY = 'apps/matrix-bot-relay/src/main.rs'
PAGER = 'apps/matrix-bot-poller/src/sync_recovery.rs'
RESPONSE = 'apps/matrix-bot-relay/src/response_contract.rs'
MIGRATION = 'services/matrix-entry-adapter/migrations/0003_sync_recovery_and_send_receipts.sql'


def require(text: str, *markers: str) -> None:
    for marker in markers:
        if marker not in text:
            raise AssertionError(f'missing recovery contract: {marker}')


def reject(text: str, *markers: str) -> None:
    for marker in markers:
        if marker in text:
            raise AssertionError(f'forbidden recovery contract: {marker}')


def ordered(text: str, *markers: str) -> None:
    end = 0
    for marker in markers:
        position = text.find(marker, end)
        if position < 0:
            raise AssertionError(f'missing ordered recovery operation: {marker}')
        end = position + len(marker)


def ordered_ignoring_whitespace(text: str, *markers: str) -> None:
    """Preserve operation order while tolerating rustfmt-only line wrapping."""
    ordered(
        ''.join(text.split()),
        *(''.join(marker.split()) for marker in markers),
    )


def read_sources(root: Path) -> dict[str, str]:
    names = [POLLER, RELAY, PAGER, RESPONSE, MIGRATION,
             'apps/matrix-bot-poller/src/wire_response.rs',
             'scripts/check-matrix-source-observation-postgres.sh',
             'scripts/matrix_postgres_regression.py',
             'scripts/test-matrix-sync-recovery-postgres.sql',
             '.github/workflows/matrix-review-repair-regression.yml',
             'docs/matrix-recovery-and-receipt-contract-v2.md',
             'crates/shared-config/src/runtime_guard.rs',
             'crates/shared-config/src/runtime_guard/matrix_profile.rs',
             'services/matrix-entry-adapter/src/runtime_profile.rs',
             'apps/matrix-bot-poller/src/runtime_profile.rs',
             'apps/matrix-bot-relay/src/runtime_profile.rs',
             'docs/module-catalog-v1.json',
             'docs/modules/matrix-bot-poller.md', 'docs/modules/matrix-bot-relay.md',
             'docs/modules/matrix-entry-adapter.md']
    return {name: (root / name).read_text(encoding='utf-8') for name in names}


def validate(sources: dict[str, str]) -> None:
    spec = importlib.util.spec_from_file_location('matrix_wiring', ROOT / 'scripts/check-matrix-runtime-wiring.py')
    assert spec and spec.loader
    wiring = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(wiring)
    function = wiring.function_source
    p, r, pager, response, sql = (sources[name] for name in [POLLER, RELAY, PAGER, RESPONSE, MIGRATION])
    ordered(function(p, 'poll_once'), 'recover_limited_timelines(', 'prepare_admissions(',
            'persist_poison_observations(', 'persist_batch(')
    require(function(p, 'poll_once'), 'matrix_initial_cursor_required', 'Vec::new()', 'bootstrap_start_now')
    sync = function(p, 'poll_once')
    ordered(sync, 'response.status() != reqwest::StatusCode::OK',
            'wire_response::decode(&bytes)', 'validate_token(&body.next_batch)',
            'if lease.opaque_cursor.is_none()')
    reject(sync, 'serde_json::from_slice(&bytes)', 'response.status().is_success()')
    wire = sources['apps/matrix-bot-poller/src/wire_response.rs']
    require(wire, 'CheckedJson { envelope: true }', 'parser.end()',
            'self.envelope && key == "errcode"', '!keys.insert(key)',
            'map.next_value_seed(CheckedJson { envelope: false })',
            '.next_element_seed(CheckedJson { envelope: false })',
            'String::deserialize(parser).map(Some)', 'MAX_RESPONSE_BYTES')
    require(pager, 'deserialize_with = "super::wire_response::optional_string"')
    require(p, 'deserialize_with = "wire_response::optional_string"',
            'failure_code: "invalid_event_type"')
    recovery = function(p, 'recover_limited_timelines')
    ordered_ignoring_whitespace(
        recovery,
        'renew_cursor_lease(',
        'http.get(url)',
        'wire_response::decode(&bytes)',
        'pager.accept(',
        'backwards.reverse()',
    )
    reject(recovery, '.begin()', 'chunk.is_empty()', 'chunk.len() <')
    require(recovery, 'GAP_PAGE_BUDGET', 'GAP_EVENT_BUDGET', 'GAP_BYTE_BUDGET',
            'GAP_DEADLINE_SECONDS', 'matrix_gap_room_identity_mismatch')
    require(pager, 'page.start != self.cursor', 'page.chunk.len() > limit',
            'None => self.complete = true', 'next == self.stop', 'self.seen.insert',
            'empty_page_with_next_cursor_must_continue', 'wrong_start_and_oversized_page_are_rejected')
    ordered(function(p, 'persist_poison_observations'), 'pool.begin()',
            'cex_matrix_renew_cursor_lease_v1', 'cex_matrix_record_poison_event_v1',
            'cex_matrix_store_poison_payload_v1', 'tx.commit()')
    ordered(function(p, 'persist_batch'), 'acknowledged_at is null',
            'matrix_poison_requires_operator_quarantine', 'cex_matrix_accept_source_event_v1',
            'cex_matrix_advance_cursor_v1', 'tx.commit()')
    reject(function(p, 'persist_batch'), '.send()')
    require(function(p, 'prepare_admissions'), 'm.replace', 'source_payload', 'object.remove("unsigned")')
    ordered(function(r, 'call_matrix_homeserver'), 'cex_matrix_bind_send_attempt_v1',
            '.fetch_one(&state.pool).await', 'request.send()', 'classify_matrix_response')
    require(function(r, 'call_matrix_homeserver'), 'matrix_access_token.as_bytes()',
            'matrix_homeserver_base_url', 'matrix_send_binding_unverified')
    ordered(function(r, 'process_matrix_delivery'), 'state.pool.begin()',
            'cex_matrix_record_send_receipt_v1', 'cex_matrix_finish_delivery_v1', 'tx.commit()')
    reject(function(r, 'call_adapter'), 'RemoteOutcome::Retryable')
    require(function(r, 'call_adapter'), 'adapter_response_unknown_timeout',
            'adapter_response_unknown_network', 'BodyFailure::Interrupted')
    require(function(r, 'complete_adapter_success'), 'response_contract::bound_reply',
            'let room_id = event.room_id.as_str()')
    ordered(function(r, 'complete_adapter_success'), 'response_contract::validate_adapter_response',
            'response_contract::bound_reply', 'state.pool.begin()')
    require(function(response, 'validate_adapter_response'), 'adapter_response_identity_mismatch', 'adapter_response_event_mismatch',
            'adapter_duplicate_outcome_unknown', 'adapter_status_request_mismatch',
            'accepted == non_business_action')
    require(function(response, 'bound_reply'), 'None | Some(Value::Null) => Ok(None)')
    require(function(r, 'matrix_send_url'), "url.path().trim_end_matches('/')",
            'url.username()', 'url.password()', 'url.query()', 'url.fragment()')

    require(response, 'BodyFailure::Interrupted', 'BodyFailure::TooLarge', 'status == 200',
            'receipt.get("event_id")', "id.starts_with('$')", 'receipt.get("errcode").is_none()',
            'room.as_str() != Some(original_room)', 'matrix_response_unknown_missing_receipt')
    reject(response, 'unwrap_or(original_room)')
    require(sql, 'matrix_transport_cursor_history', 'matrix_transport_poison_payloads',
            'matrix_transport_send_bindings', 'matrix_transport_send_receipts',
            'matrix_poison_requires_operator_quarantine', 'matrix_send_receipt_required',
            'matrix_send_idempotency_scope_changed', "o.destination <> 'matrix-relay-adapter-v1'",
            'stale.previous_owner', 'adapter_response_unknown_expired_claim',
            'cex_matrix_reject_immutable_mutation_v1()', 'revoke all on function')
    reject(sql.lower(), 'security definer', 'grant all', 'disable trigger')
    if not sql.startswith('begin;') or not sql.rstrip().endswith('commit;'):
        raise AssertionError('migration must remain transactional')
    profiles = [sources[name] for name in [
        'services/matrix-entry-adapter/src/runtime_profile.rs',
        'apps/matrix-bot-poller/src/runtime_profile.rs',
        'apps/matrix-bot-relay/src/runtime_profile.rs']]
    expected_import = ("// Compatibility import only; parsing and tests are owned by shared-config.\n"
                       "pub use shared_config::runtime_guard::matrix_profile::resolve_profiles;\n")
    if any(profile != expected_import for profile in profiles):
        raise AssertionError('Matrix profile wrappers must only re-export the shared parser')
    require(sources['crates/shared-config/src/runtime_guard.rs'], 'pub mod matrix_profile;')
    shared_profile = sources['crates/shared-config/src/runtime_guard/matrix_profile.rs']
    profile_spec = importlib.util.spec_from_file_location(
        'matrix_profile_functions', ROOT / 'scripts/check-matrix-filter-recovery.py')
    assert profile_spec and profile_spec.loader
    profile_wiring = importlib.util.module_from_spec(profile_spec)
    profile_spec.loader.exec_module(profile_wiring)
    shared_function = profile_wiring.function_body
    require(shared_function(shared_profile, 'parse'), '"stage" | "staging"',
            '"prod" | "production" | "trnm-economy" | "trnm_economy"',
            'Err("invalid_matrix_runtime_profile")')
    require(shared_function(shared_profile, 'resolve_profiles'), 'AdapterProfile::parse(raw)?',
            'previous != parsed', 'Err("conflicting_matrix_runtime_profiles")',
            'selected.unwrap_or(AdapterProfile::Local)')
    require(shared_function(shared_profile, 'legacy_value'),
            'Self::Staging | Self::Production => "production"', 'Self::Beta => "beta"')
    reject(shared_profile, 'set_var(', 'unsafe ', 'std::env::', 'include!')
    for main in [p, r]:
        require(function(main, 'is_production_like'), 'resolve_profiles(&values)', 'NotUnicode',
                'PROFILE_ENV_NAME', '"CEX_RUNTIME_PROFILE"', '"APP_ENV"')
    wrapper = sources['scripts/check-matrix-source-observation-postgres.sh']
    require(wrapper, 'exec python3', 'matrix_postgres_regression.py')
    reject(wrapper, 'eval ', 'psql ', 'truncate ', 'docker ')
    driver = sources['scripts/matrix_postgres_regression.py']
    require(driver, 'MATRIX_TEST_ALLOW_SCHEMA_RESET', 'check-matrix-transport-postgres.sh',
            '0002_source_observation_replay.sql', '0003_sync_recovery_and_send_receipts.sql',
            'test-matrix-source-observation-replay.sql', 'test-matrix-sync-recovery-postgres.sql',
            'foreign-table-guard', 'server-identity', 'baseline_sql(inputs[BASELINE])',
            'for iteration in (1, 2)', 'subprocess.Popen', 'ON_ERROR_STOP=1',
            'single_psql_session', 'pg_try_advisory_lock', 'test-matrix-transport-baseline.sql')
    reject(driver, 'shell=True', 'eval(', 'check=False)\n    return 0')
    regression = sources['scripts/test-matrix-sync-recovery-postgres.sql']
    require(regression, 'expired adapter claim was blindly resent', 'send succeeded without receipt',
            'credential rotation silently changed retry identity', 'poison bytes lost',
            'old owner acknowledged recovered send', 'rollback;')
    workflow = sources['.github/workflows/matrix-review-repair-regression.yml']
    require(workflow, 'contents: read', 'check-matrix-recovery-contract.py',
            'test-matrix-recovery-contract.py', 'cargo test --locked -p matrix-bot-poller -p matrix-bot-relay --all-targets',
            'cargo clippy --locked -p matrix-bot-poller -p matrix-bot-relay --all-targets -- -D warnings',
            'check-matrix-source-observation-postgres.sh')
    reject(workflow, 'continue-on-error: true', 'git push', 'contents: write')
    catalog = json.loads(sources['docs/module-catalog-v1.json'])
    for name in ['matrix-entry-adapter', 'matrix-bot-poller', 'matrix-bot-relay']:
        item = next(row for row in catalog['modules'] if row['package'] == name)
        document = sources[item['documentation']]
        for source in item['source_entrypoints']:
            require(document, '`' + source + '`')
        for command in item['verification']:
            require(document, command)
    require(sources['docs/matrix-recovery-and-receipt-contract-v2.md'],
            'not_granted', 'not repository qualification', 'Independent', '0001 -> 0002 -> 0003')


def main() -> int:
    try:
        validate(read_sources(ROOT))
    except (AssertionError, OSError, ValueError, StopIteration) as error:
        print(f'Matrix recovery source contract: FAILED: {error}')
        return 1
    print(json.dumps({'schema': 'cex.matrix-recovery-source-check.v1', 'status': 'ok',
                      'runtime_execution_proven': False, 'production_authorization': 'not_granted'}))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
