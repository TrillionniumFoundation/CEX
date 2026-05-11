#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

python3 - <<'PY'
import ast
import json
import os
import pathlib
import re
import urllib.error
import urllib.request

root = pathlib.Path.cwd()
env_path = pathlib.Path(os.environ.get('CEX_ENV_FILE', 'run/local-production/.env'))
base = os.environ.get('CONSUMER_ENTRY_BASE_URL', 'http://127.0.0.1:8090').rstrip('/')

def env_value(name, default=None):
    if os.environ.get(name):
        return os.environ[name]
    path = root / env_path if not env_path.is_absolute() else env_path
    if path.exists():
        for raw in path.read_text().splitlines():
            line = raw.strip()
            if not line or line.startswith('#') or '=' not in line:
                continue
            key, value = line.split('=', 1)
            if key.strip() == name:
                return value.strip().strip('"').strip("'")
    return default

snapshot_path = pathlib.Path(env_value(
    'CONSUMER_ENTRY_LEAGUE_SQL_SNAPSHOT_PATH',
    'run/linux-runtime/entry-config/league-state-snapshot.sql',
))
if not snapshot_path.is_absolute():
    snapshot_path = root / snapshot_path
assert snapshot_path.exists(), f'missing SQL snapshot: {snapshot_path}'
raw = snapshot_path.read_text()
assert 'league_state_snapshots' in raw, 'snapshot table insert missing'
assert 'league_state_repository_snapshots' in raw, 'repository audit table insert missing'
assert 'league_state_repository_write_set_audits' in raw, 'repository write-set audit table insert missing'
raw_normalized_world_shadow_checked = all(marker in raw for marker in [
    'Normalized WorldState shadow upserts',
    'insert into world_map_nodes',
    'insert into world_work_acceptances',
])
assert 'consumer_entry_json_v1' in raw, 'snapshot kind missing'
assert 'sha256:' in raw, 'state hash missing'
assert '0025_add_trillionnium_combat_numerics_runtime_column.sql' in raw, 'repository migration floor missing'
assert 'region_story_unlock_state' in raw, 'region/story unlock runtime column missing from SQL snapshot'
assert 'combat_numerics_state' in raw, 'combat numerics runtime column missing from SQL snapshot'
assert 'trillionnium_repository_cutover_v1' in raw, 'repository cutover contract missing'
assert 'trillionnium_sql_shadow_validation_v1' in raw, 'sql shadow validation contract missing'
assert 'on conflict (state_hash)' in raw, 'snapshot upsert clause missing'
assert 'on conflict (state_hash, cutover_phase)' in raw, 'repository audit upsert clause missing'
assert 'on conflict (state_hash, cutover_phase, command)' in raw, 'repository write-set audit upsert clause missing'
match = re.search(
    r"insert into league_state_snapshots\s*\(snapshot_kind, state_hash, state\)\s*"
    r"values \('consumer_entry_json_v1', '([^']+)', '((?:''|[^'])*)'::jsonb\)\s*"
    r"on conflict \(state_hash\)",
    raw,
    re.S,
)
assert match, 'snapshot insert format changed'
state_hash, sql_json = match.groups()
json_text = sql_json.replace("''", "'")
state = json.loads(json_text)
repo_match = re.search(
    r"insert into league_state_repository_snapshots\s*\([\s\S]*?\)\s*values\s*\(\s*"
    r"'([^']+)',\s*'consumer_entry_json_v1',\s*'final_cutover',\s*"
    r"'json_file_with_sql_snapshot',\s*'normalized_sql_dual_write',\s*"
    r"'([^']+)',\s*'((?:''|[^'])*)'::jsonb,\s*'((?:''|[^'])*)'::jsonb\s*\)\s*"
    r"on conflict \(state_hash, cutover_phase\)",
    raw,
    re.S,
)
assert repo_match, 'repository audit insert format changed'
repo_state_hash, migration_floor, cutover_plan_sql, shadow_validation_sql = repo_match.groups()
assert repo_state_hash == state_hash, 'repository audit state_hash mismatch'
assert migration_floor == '0025_add_trillionnium_combat_numerics_runtime_column.sql', migration_floor
cutover_plan = json.loads(cutover_plan_sql.replace("''", "'"))
shadow_validation = json.loads(shadow_validation_sql.replace("''", "'"))
assert cutover_plan.get('next_repository') == 'normalized_sql_dual_write', cutover_plan
assert cutover_plan.get('migration_floor') == migration_floor, cutover_plan
assert cutover_plan.get('repository_contract', {}).get('contract_version') == 'trillionnium_repository_cutover_v1', cutover_plan
repository_contract = cutover_plan.get('repository_contract', {})
write_set_audit = repository_contract.get('write_set_audit') or {}
dual_write_plan = repository_contract.get('dual_write_plan') or {}
runtime_validation = repository_contract.get('runtime_validation') or {}
state_boundary = repository_contract.get('state_boundary') or {}
direct_write_contract = repository_contract.get('direct_write_contract') or {}
read_model_contract = repository_contract.get('read_model_contract') or {}
expected_direct_commands = [
    'world_action',
    'world_contract_completion',
    'world_map_move',
    'world_tactics_command',
    'world_asset_upgrade',
    'world_company',
    'world_listing',
    'world_buy',
    'world_work_deliver',
    'world_work_accept',
    'world_work_reject',
    'world_work_reopen',
    'world_work_cancel',
]
raw_dual_write_plan_checked = dual_write_plan.get('plan_version') == 'trillionnium_repository_dual_write_plan_v1'
raw_runtime_validation_checked = runtime_validation.get('script') == 'scripts/check-trillionnium-league-normalized-runtime-dual-write.sh'
raw_direct_write_contract_checked = False
assert 'normalized_repository_command_shadow_sql' in (state_boundary.get('runtime_command_write_sql_helper') or ''), repository_contract
assert 'verify_command_scoped_world_table_upserts' in (runtime_validation.get('checks') or []), repository_contract
if state_boundary.get('runtime_direct_write_helper') is not None:
    assert 'execute_normalized_repository_direct_command_write' in (state_boundary.get('runtime_direct_write_helper') or ''), repository_contract
if direct_write_contract:
    assert direct_write_contract.get('contract_version') == 'trillionnium_normalized_repository_direct_write_v1', repository_contract
    supported = direct_write_contract.get('supported_commands') or []
    assert 'world_action' in supported and 'world_map_move' in supported, repository_contract
    if all(command in supported for command in expected_direct_commands):
        raw_direct_write_contract_checked = True
    if direct_write_contract.get('transaction_mode') is not None:
        assert direct_write_contract.get('transaction_mode') == 'single_pg_transaction_direct_sql_primary_plus_snapshot_export', repository_contract
        assert 'atomically' in (direct_write_contract.get('transaction_boundary') or ''), repository_contract
raw_read_model_contract_checked = False
if state_boundary.get('runtime_read_model_sql_helper') is not None:
    assert 'normalized_repository_world_home_read_model_sql' in (state_boundary.get('runtime_read_model_sql_helper') or ''), repository_contract
    assert 'normalized_repository_client_feed_read_model_sql' in (state_boundary.get('runtime_read_model_sql_helper') or ''), repository_contract
if (
    'verify_normalized_world_home_read_model_sql' in (runtime_validation.get('checks') or [])
    and 'verify_normalized_client_feed_read_model_sql' in (runtime_validation.get('checks') or [])
):
    raw_read_model_contract_checked = True
if read_model_contract:
    assert read_model_contract.get('contract_version') == 'trillionnium_normalized_repository_read_model_v1', repository_contract
    assert ((read_model_contract.get('world_home') or {}).get('read_model_version') == 'trillionnium_normalized_world_home_read_model_v1'), repository_contract
    assert ((read_model_contract.get('world_home') or {}).get('startup_gate') in (None, 'normalized_read_model_startup_gate_green')), repository_contract
    assert ((read_model_contract.get('client_feed') or {}).get('read_model_version') == 'trillionnium_normalized_client_feed_read_model_v1'), repository_contract
    assert ((read_model_contract.get('client_feed') or {}).get('startup_gate') in (None, 'normalized_client_feed_read_model_startup_gate_green')), repository_contract
    raw_read_model_contract_checked = True
if raw_dual_write_plan_checked:
    assert any(
        write_set.get('command') == 'world_work_accept'
        and 'world_work_acceptances' in (write_set.get('tables') or [])
        for write_set in (dual_write_plan.get('write_sets') or [])
    ), cutover_plan
    assert any(
        write_set.get('command') == 'world_map_move'
        and 'world_player_positions' in (write_set.get('tables') or [])
        and 'world_economy_events' in (write_set.get('tables') or [])
        for write_set in (dual_write_plan.get('write_sets') or [])
    ), cutover_plan
    assert any(
        write_set.get('command') == 'world_tactics_command'
        and 'world_trillionnium_characters' in (write_set.get('tables') or [])
        and 'world_tactics_sessions' in (write_set.get('tables') or [])
        and 'world_tactics_simulation_ticks' in (write_set.get('tables') or [])
        for write_set in (dual_write_plan.get('write_sets') or [])
    ), cutover_plan
    assert any(
        write_set.get('command') == 'world_work_deliver'
        and 'world_work_deliveries' in (write_set.get('tables') or [])
        and 'world_economy_events' in (write_set.get('tables') or [])
        and 'world_faction_standings' in (write_set.get('tables') or [])
        for write_set in (dual_write_plan.get('write_sets') or [])
    ), cutover_plan
    requirements = dual_write_plan.get('read_switch_requirements') or []
    assert 'repository_write_set_audit_green' in requirements, cutover_plan
    assert 'normalized_runtime_dual_write_gate_green' in requirements, cutover_plan
    assert 'normalized_runtime_read_switch_gate_green' in requirements, cutover_plan
    if 'normalized_world_home_read_model_green' in requirements and 'normalized_client_feed_read_model_green' in requirements:
        raw_read_model_contract_checked = True
assert write_set_audit.get('audit_version') == 'trillionnium_repository_write_set_audit_v1', cutover_plan
assert write_set_audit.get('table') == 'league_state_repository_write_set_audits', cutover_plan
raw_runtime_gates_checked = (
    'repository_write_set_audit_green' in (repository_contract.get('read_switch_gates') or [])
    and 'normalized_runtime_dual_write_gate_green' in (repository_contract.get('read_switch_gates') or [])
    and 'normalized_runtime_read_switch_gate_green' in (repository_contract.get('read_switch_gates') or [])
    and 'normalized_world_home_read_model_green' in (repository_contract.get('read_switch_gates') or [])
    and 'normalized_client_feed_read_model_green' in (repository_contract.get('read_switch_gates') or [])
)
assert shadow_validation.get('validation_version') == 'trillionnium_sql_shadow_validation_v1', shadow_validation
assert shadow_validation.get('mode') == 'row_count_parity', shadow_validation
for key in [
    'matches',
    'players_by_matrix_user',
    'entries',
    'submissions',
    'rewards',
    'league_skills',
    'league_tools',
    'league_skins',
    'world_companies',
    'world_shops',
    'world_listings',
    'world_economy_events',
    'world_purchases',
    'world_work_orders',
    'world_work_deliveries',
    'world_work_acceptances',
    'world_work_rejections',
    'world_work_reopens',
    'world_work_cancellations',
    'world_factions',
    'world_faction_standings',
    'world_map_nodes',
    'world_player_positions',
    'world_trillionnium_characters',
    'world_tactics_sessions',
    'world_tactics_simulation_ticks',
]:
    assert key in state, f'missing state key {key}'
assert state['matches'], 'expected seeded league matches'
assert state['world_factions'], 'expected seeded world factions'
assert state['league_skills'], 'expected seeded league skills'
assert state['league_tools'], 'expected seeded league tools'
assert state['league_skins'], 'expected seeded league skins'

token = env_value('CONSUMER_ENTRY_INGRESS_TOKEN')
endpoint = None
endpoint_dual_write_plan = {}
endpoint_runtime_validation = {}
endpoint_normalized_world_shadow = {}
if token:
    req = urllib.request.Request(base + '/v1/league/state/snapshot', headers={'x-entry-token': token})
    try:
        with urllib.request.urlopen(req, timeout=20) as resp:
            endpoint = json.loads(resp.read().decode())
    except urllib.error.URLError:
        endpoint = None

if endpoint is not None:
    assert endpoint.get('kind') == 'league_state_snapshot', endpoint
    assert endpoint.get('state_hash', '').startswith('sha256:'), endpoint
    assert endpoint.get('sql_snapshot_path_configured') is True, endpoint
    endpoint_plan = endpoint.get('sql_cutover_plan') or {}
    endpoint_validation = endpoint.get('sql_shadow_validation') or {}
    endpoint_audit = endpoint.get('repository_cutover_audit') or {}
    endpoint_write_set_audit = endpoint.get('repository_write_set_audit') or {}
    endpoint_repository = endpoint.get('normalized_repository') or {}
    endpoint_normalized_world_shadow = endpoint.get('normalized_world_shadow_sql') or {}
    endpoint_contract = endpoint_plan.get('repository_contract') or {}
    endpoint_dual_write_plan = endpoint_contract.get('dual_write_plan') or {}
    endpoint_runtime_validation = endpoint_contract.get('runtime_validation') or {}
    endpoint_state_boundary = endpoint_contract.get('state_boundary') or {}
    endpoint_direct_write_contract = endpoint_contract.get('direct_write_contract') or {}
    endpoint_read_model_contract = endpoint_contract.get('read_model_contract') or {}
    assert endpoint_plan.get('next_repository') == 'normalized_sql_dual_write', endpoint
    assert endpoint_plan.get('migration_floor') == migration_floor, endpoint
    assert endpoint_dual_write_plan.get('plan_version') == 'trillionnium_repository_dual_write_plan_v1', endpoint
    assert any(
        write_set.get('command') == 'world_work_accept'
        and 'world_work_acceptances' in (write_set.get('tables') or [])
        for write_set in (endpoint_dual_write_plan.get('write_sets') or [])
    ), endpoint
    assert any(
        write_set.get('command') == 'world_map_move'
        and 'world_player_positions' in (write_set.get('tables') or [])
        and 'world_economy_events' in (write_set.get('tables') or [])
        for write_set in (endpoint_dual_write_plan.get('write_sets') or [])
    ), endpoint
    assert any(
        write_set.get('command') == 'world_tactics_command'
        and 'world_trillionnium_characters' in (write_set.get('tables') or [])
        and 'world_tactics_sessions' in (write_set.get('tables') or [])
        and 'world_tactics_simulation_ticks' in (write_set.get('tables') or [])
        for write_set in (endpoint_dual_write_plan.get('write_sets') or [])
    ), endpoint
    assert any(
        write_set.get('command') == 'world_work_deliver'
        and 'world_work_deliveries' in (write_set.get('tables') or [])
        and 'world_economy_events' in (write_set.get('tables') or [])
        and 'world_faction_standings' in (write_set.get('tables') or [])
        for write_set in (endpoint_dual_write_plan.get('write_sets') or [])
    ), endpoint
    endpoint_requirements = endpoint_dual_write_plan.get('read_switch_requirements') or []
    assert 'repository_write_set_audit_green' in endpoint_requirements, endpoint
    assert 'normalized_runtime_dual_write_gate_green' in endpoint_requirements, endpoint
    assert 'normalized_runtime_read_switch_gate_green' in endpoint_requirements, endpoint
    assert 'normalized_world_home_read_model_green' in endpoint_requirements, endpoint
    assert 'normalized_client_feed_read_model_green' in endpoint_requirements, endpoint
    assert endpoint_write_set_audit.get('audit_version') == 'trillionnium_repository_write_set_audit_v1', endpoint
    assert endpoint_runtime_validation.get('script') == 'scripts/check-trillionnium-league-normalized-runtime-dual-write.sh', endpoint
    assert 'verify_command_scoped_world_table_upserts' in (endpoint_runtime_validation.get('checks') or []), endpoint
    endpoint_supported_direct_commands = endpoint_direct_write_contract.get('supported_commands') or []
    endpoint_direct_write_contract_current = all(command in endpoint_supported_direct_commands for command in expected_direct_commands)
    endpoint_runtime_validation_checks = endpoint_runtime_validation.get('checks') or []
    endpoint_runtime_validation_current = all(
        check in endpoint_runtime_validation_checks for check in [
            'verify_direct_world_action_write_helper',
            'verify_direct_world_contract_completion_write_helper',
            'verify_direct_world_map_move_write_helper',
            'verify_direct_world_tactics_command_write_helper',
            'verify_direct_world_asset_upgrade_write_helper',
            'verify_direct_world_company_write_helper',
            'verify_direct_world_listing_write_helper',
            'verify_direct_world_buy_write_helper',
            'verify_direct_world_work_deliver_write_helper',
            'verify_direct_world_work_accept_write_helper',
            'verify_direct_world_work_reject_write_helper',
            'verify_direct_world_work_reopen_write_helper',
            'verify_direct_world_work_cancel_write_helper',
        ]
    )
    if endpoint_direct_write_contract_current or endpoint_runtime_validation_current:
        assert 'verify_direct_world_action_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_contract_completion_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_map_move_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_tactics_command_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_asset_upgrade_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_company_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_listing_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_buy_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_work_deliver_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_work_accept_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_work_reject_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_work_reopen_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
        assert 'verify_direct_world_work_cancel_write_helper' in (endpoint_runtime_validation.get('checks') or []), endpoint
    assert 'verify_normalized_world_home_read_model_sql' in (endpoint_runtime_validation.get('checks') or []), endpoint
    assert 'verify_normalized_client_feed_read_model_sql' in (endpoint_runtime_validation.get('checks') or []), endpoint
    assert 'normalized_repository_command_shadow_sql' in (endpoint_state_boundary.get('runtime_command_write_sql_helper') or ''), endpoint
    if endpoint_state_boundary.get('runtime_direct_write_helper') is not None:
        assert 'execute_normalized_repository_direct_command_write' in (endpoint_state_boundary.get('runtime_direct_write_helper') or ''), endpoint
    assert 'normalized_repository_world_home_read_model_sql' in (endpoint_state_boundary.get('runtime_read_model_sql_helper') or ''), endpoint
    assert 'normalized_repository_client_feed_read_model_sql' in (endpoint_state_boundary.get('runtime_read_model_sql_helper') or ''), endpoint
    if endpoint_direct_write_contract:
        assert endpoint_direct_write_contract.get('contract_version') == 'trillionnium_normalized_repository_direct_write_v1', endpoint
        assert 'world_action' in endpoint_supported_direct_commands and 'world_map_move' in endpoint_supported_direct_commands, endpoint
        if endpoint_direct_write_contract_current:
            for command in expected_direct_commands:
                assert command in endpoint_supported_direct_commands, endpoint
        if endpoint_direct_write_contract.get('transaction_mode') is not None:
            assert endpoint_direct_write_contract.get('transaction_mode') == 'single_pg_transaction_direct_sql_primary_plus_snapshot_export', endpoint
            assert 'atomically' in (endpoint_direct_write_contract.get('transaction_boundary') or ''), endpoint
        if endpoint_direct_write_contract.get('index_reuse') is not None:
            assert 'one WorldIndexes snapshot' in endpoint_direct_write_contract.get('index_reuse'), endpoint
    assert endpoint_read_model_contract.get('contract_version') == 'trillionnium_normalized_repository_read_model_v1', endpoint
    assert (endpoint_read_model_contract.get('world_home') or {}).get('read_model_version') == 'trillionnium_normalized_world_home_read_model_v1', endpoint
    if (endpoint_read_model_contract.get('world_home') or {}).get('startup_gate') is not None:
        assert (endpoint_read_model_contract.get('world_home') or {}).get('startup_gate') == 'normalized_read_model_startup_gate_green', endpoint
    assert (endpoint_read_model_contract.get('client_feed') or {}).get('read_model_version') == 'trillionnium_normalized_client_feed_read_model_v1', endpoint
    if (endpoint_read_model_contract.get('client_feed') or {}).get('startup_gate') is not None:
        assert (endpoint_read_model_contract.get('client_feed') or {}).get('startup_gate') == 'normalized_client_feed_read_model_startup_gate_green', endpoint
    assert endpoint_validation.get('validation_version') == 'trillionnium_sql_shadow_validation_v1', endpoint
    assert endpoint_normalized_world_shadow.get('contract_version') == 'trillionnium_normalized_world_shadow_sql_v1', endpoint
    assert endpoint_normalized_world_shadow.get('index_layer') == 'WorldIndexes::normalized_shadow_sorted_ids_v1', endpoint
    if endpoint_normalized_world_shadow.get('sorted_vector_index_layer') is not None:
        assert endpoint_normalized_world_shadow.get('sorted_vector_index_layer') == 'WorldIndexes::normalized_shadow_sorted_vector_indices_v1', endpoint
    assert 'world_work_acceptances' in (endpoint_normalized_world_shadow.get('tables') or []), endpoint
    assert 'world_tactics_sessions' in (endpoint_normalized_world_shadow.get('tables') or []), endpoint
    assert 'world_tactics_simulation_ticks' in (endpoint_normalized_world_shadow.get('tables') or []), endpoint
    assert endpoint_audit.get('audit_version') == 'trillionnium_repository_cutover_audit_v1', endpoint
    assert endpoint_audit.get('cutover_phase') == 'final_cutover', endpoint
    assert endpoint_audit.get('next_repository') == 'normalized_sql_dual_write', endpoint
    assert endpoint_audit.get('dual_write_plan_version') == 'trillionnium_repository_dual_write_plan_v1', endpoint
    assert 'dual_write_active' in endpoint_repository, endpoint
    assert 'read_switch_active' in endpoint_repository, endpoint
    if endpoint_repository.get('write_mode') is not None:
        assert endpoint_repository.get('write_mode') == 'command_scoped_normalized_world_upserts_with_full_snapshot_export', endpoint
    if endpoint_repository.get('command_scoped_helper') is not None:
        assert endpoint_repository.get('command_scoped_helper') == 'normalized_repository_command_shadow_sql', endpoint
    if endpoint_repository.get('direct_write_helper') is not None:
        assert endpoint_repository.get('direct_write_helper') == 'execute_normalized_repository_direct_command_write', endpoint
    if endpoint_repository.get('direct_write_contract') is not None:
        assert endpoint_repository.get('direct_write_contract', {}).get('contract_version') == 'trillionnium_normalized_repository_direct_write_v1', endpoint
    if endpoint_repository.get('unknown_command_mode') is not None:
        assert endpoint_repository.get('unknown_command_mode') == 'audit_only_no_full_world_snapshot_fallback', endpoint
    if endpoint_repository.get('read_switch_gate') is not None:
        assert endpoint_repository.get('read_switch_gate') == 'latest_snapshot_requires_repository_audit_and_write_set_audit', endpoint
    if endpoint_repository.get('read_switch_source_of_truth_gate') is not None:
        assert endpoint_repository.get('read_switch_source_of_truth_gate') == 'latest_snapshot_requires_repository_audit_write_set_audit_and_normalized_world_home_and_client_feed_read_models', endpoint
    if endpoint_repository.get('read_model_contract') is not None:
        assert endpoint_repository.get('read_model_contract', {}).get('contract_version') == 'trillionnium_normalized_repository_read_model_v1', endpoint
    assert endpoint_repository.get('read_boundary', '').startswith('startup can hydrate LeagueState'), endpoint
elif not raw_dual_write_plan_checked:
    raise AssertionError('repository dual-write plan missing from raw snapshot and endpoint unavailable')

if not raw_normalized_world_shadow_checked and not endpoint_normalized_world_shadow:
    raise AssertionError('normalized world shadow SQL missing from raw snapshot and endpoint unavailable')

effective_dual_write_plan = endpoint_dual_write_plan or dual_write_plan
effective_contract = endpoint_contract if endpoint is not None else repository_contract
effective_runtime_validation = (endpoint_runtime_validation if endpoint is not None else None) or runtime_validation
assert effective_runtime_validation.get('script') == 'scripts/check-trillionnium-league-normalized-runtime-dual-write.sh', effective_runtime_validation
assert 'verify_command_scoped_world_table_upserts' in (effective_runtime_validation.get('checks') or []), effective_runtime_validation
assert 'verify_normalized_world_home_read_model_sql' in (effective_runtime_validation.get('checks') or []), effective_runtime_validation
assert 'verify_normalized_client_feed_read_model_sql' in (effective_runtime_validation.get('checks') or []), effective_runtime_validation
assert 'repository_write_set_audit_green' in (effective_contract.get('read_switch_gates') or []), effective_contract
if 'normalized_read_model_startup_gate_green' in (effective_contract.get('read_switch_gates') or []):
    assert ((effective_contract.get('read_model_contract') or {}).get('world_home') or {}).get('startup_gate') == 'normalized_read_model_startup_gate_green', effective_contract
if 'normalized_client_feed_read_model_startup_gate_green' in (effective_contract.get('read_switch_gates') or []):
    assert ((effective_contract.get('read_model_contract') or {}).get('client_feed') or {}).get('startup_gate') == 'normalized_client_feed_read_model_startup_gate_green', effective_contract
assert 'normalized_world_home_read_model_green' in (effective_contract.get('read_switch_gates') or []), effective_contract
assert 'normalized_client_feed_read_model_green' in (effective_contract.get('read_switch_gates') or []), effective_contract
assert 'normalized_runtime_dual_write_gate_green' in (effective_contract.get('read_switch_gates') or []), effective_contract
assert 'normalized_runtime_read_switch_gate_green' in (effective_contract.get('read_switch_gates') or []), effective_contract

summary = {
    'ok': True,
    'snapshot_path': str(snapshot_path),
    'state_hash': state_hash,
    'matches': len(state.get('matches') or {}),
    'players': len(state.get('players_by_matrix_user') or {}),
    'submissions': len(state.get('submissions') or {}),
    'rewards': len(state.get('rewards') or []),
    'league_skills': len(state.get('league_skills') or {}),
    'league_tools': len(state.get('league_tools') or {}),
    'league_skins': len(state.get('league_skins') or {}),
    'world_companies': len(state.get('world_companies') or []),
    'world_listings': len(state.get('world_listings') or []),
    'world_purchases': len(state.get('world_purchases') or []),
    'world_work_orders': len(state.get('world_work_orders') or []),
    'world_work_deliveries': len(state.get('world_work_deliveries') or []),
    'world_work_acceptances': len(state.get('world_work_acceptances') or []),
    'world_work_rejections': len(state.get('world_work_rejections') or []),
    'world_work_reopens': len(state.get('world_work_reopens') or []),
    'world_work_cancellations': len(state.get('world_work_cancellations') or []),
    'world_factions': len(state.get('world_factions') or {}),
    'world_faction_standings': len(state.get('world_faction_standings') or []),
    'world_map_nodes': len(state.get('world_map_nodes') or {}),
    'world_player_positions': len(state.get('world_player_positions') or {}),
    'world_trillionnium_characters': len(state.get('world_trillionnium_characters') or {}),
    'world_tactics_sessions': len(state.get('world_tactics_sessions') or {}),
    'world_tactics_simulation_ticks': len(state.get('world_tactics_simulation_ticks') or []),
    'repository_audit_checked': True,
    'repository_write_set_audit_checked': True,
    'repository_migration_floor': migration_floor,
    'raw_normalized_world_shadow_checked': raw_normalized_world_shadow_checked,
    'normalized_world_shadow_upserts_checked': raw_normalized_world_shadow_checked or bool(endpoint_normalized_world_shadow),
    'raw_dual_write_plan_checked': raw_dual_write_plan_checked,
    'raw_runtime_validation_checked': raw_runtime_validation_checked,
    'raw_runtime_gates_checked': raw_runtime_gates_checked,
    'raw_direct_write_contract_checked': raw_direct_write_contract_checked,
    'runtime_command_scoped_write_contract_checked': True,
    'normalized_world_home_read_model_contract_checked': True,
    'normalized_client_feed_read_model_contract_checked': True,
    'dual_write_write_sets': len(effective_dual_write_plan.get('write_sets') or []),
    'runtime_validation_script': effective_runtime_validation.get('script'),
    'shadow_validation_checks': shadow_validation.get('check_count'),
    'endpoint_checked': endpoint is not None,
    'endpoint_repository_audit_checked': endpoint is not None,
    'endpoint_normalized_repository_checked': endpoint is not None,
}
print(json.dumps(summary, ensure_ascii=False, indent=2))
PY
