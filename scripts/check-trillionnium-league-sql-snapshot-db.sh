#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"

cex_load_env

SNAPSHOT_PATH="${1:-$PROJECT_ROOT/run/linux-runtime/entry-config/league-state-snapshot.sql}"
if [[ ! -f "$SNAPSHOT_PATH" ]]; then
  echo "snapshot SQL not found: $SNAPSHOT_PATH" >&2
  exit 1
fi

TMP_DB="${CEX_SQL_SNAPSHOT_TMP_DB:-cex_snapshot_check_$(date +%s)_$$}"
if [[ ! "$TMP_DB" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
  echo "unsafe temp database name: $TMP_DB" >&2
  exit 1
fi

BASE_URL="$(cex_effective_database_url)"
if [[ "$BASE_URL" != *"127.0.0.1"* && "$BASE_URL" != *"localhost"* && "${CEX_ALLOW_NONLOCAL_SNAPSHOT_DB_CHECK:-0}" != "1" ]]; then
  if ! cex_can_use_docker_postgres; then
    echo "refusing non-local DATABASE_URL for snapshot DB check: $BASE_URL" >&2
    echo "set CEX_ALLOW_NONLOCAL_SNAPSHOT_DB_CHECK=1 only for an isolated disposable database" >&2
    exit 1
  fi
fi

cex_wait_postgres 60 1 >/dev/null

run_admin_sql() {
  local sql="$1"
  if cex_has_local_psql; then
    local admin_url="${BASE_URL%/*}/postgres"
    PGPASSWORD="$CEX_POSTGRES_PASSWORD" psql "$admin_url" -v ON_ERROR_STOP=1 -c "$sql"
    return 0
  fi
  if cex_can_use_docker_postgres; then
    cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" \
      psql -U "$CEX_POSTGRES_USER" -d postgres -v ON_ERROR_STOP=1 -c "$sql"
    return 0
  fi
  echo "no usable postgres client found" >&2
  return 1
}

run_tmp_file() {
  local file="$1"
  if cex_has_local_psql; then
    local tmp_url="${BASE_URL%/*}/$TMP_DB"
    PGPASSWORD="$CEX_POSTGRES_PASSWORD" psql "$tmp_url" -v ON_ERROR_STOP=1 -f "$file"
    return 0
  fi
  if cex_can_use_docker_postgres; then
    cex_docker exec -i "$CEX_POSTGRES_CONTAINER_NAME" \
      psql -U "$CEX_POSTGRES_USER" -d "$TMP_DB" -v ON_ERROR_STOP=1 -f - < "$file"
    return 0
  fi
  echo "no usable postgres client found" >&2
  return 1
}

run_tmp_sql() {
  local sql="$1"
  if cex_has_local_psql; then
    local tmp_url="${BASE_URL%/*}/$TMP_DB"
    PGPASSWORD="$CEX_POSTGRES_PASSWORD" psql "$tmp_url" -v ON_ERROR_STOP=1 -c "$sql"
    return 0
  fi
  if cex_can_use_docker_postgres; then
    cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" \
      psql -U "$CEX_POSTGRES_USER" -d "$TMP_DB" -v ON_ERROR_STOP=1 -c "$sql"
    return 0
  fi
  echo "no usable postgres client found" >&2
  return 1
}

cleanup() {
  run_admin_sql "drop database if exists \"$TMP_DB\" with (force);" >/dev/null 2>&1 || \
    run_admin_sql "drop database if exists \"$TMP_DB\";" >/dev/null 2>&1 || true
}
trap cleanup EXIT

cleanup
run_admin_sql "create database \"$TMP_DB\";" >/dev/null

for migration in "$PROJECT_ROOT"/migrations/*.sql; do
  [[ -f "$migration" ]] || continue
  echo "==> applying $(basename "$migration") to $TMP_DB"
  run_tmp_file "$migration" >/dev/null
done

echo "==> applying $(basename "$SNAPSHOT_PATH") to $TMP_DB"
run_tmp_file "$SNAPSHOT_PATH" >/dev/null

echo "==> validating normalized world row-count parity in $TMP_DB"
run_tmp_sql "
do \$\$
declare
  mismatches jsonb;
begin
  with plan_tables as (
    select
      table_entry->>'table_name' as table_name,
      (table_entry->>'row_count')::bigint as expected_count
    from league_state_repository_snapshots snapshot
    cross join lateral jsonb_array_elements(snapshot.cutover_plan->'tables') as table_entry
    where snapshot.cutover_phase = 'shadow_snapshot'
      and table_entry->>'table_name' like 'world_%'
  ), actual_counts as (
    select 'world_zones' as table_name, count(*)::bigint as actual_count from world_zones union all
    select 'world_locations', count(*)::bigint from world_locations union all
    select 'world_entities', count(*)::bigint from world_entities union all
    select 'world_map_nodes', count(*)::bigint from world_map_nodes union all
    select 'world_player_positions', count(*)::bigint from world_player_positions union all
    select 'world_assets', count(*)::bigint from world_assets union all
    select 'world_events', count(*)::bigint from world_events union all
    select 'world_relationships', count(*)::bigint from world_relationships union all
    select 'world_contracts', count(*)::bigint from world_contracts union all
    select 'world_contract_completions', count(*)::bigint from world_contract_completions union all
    select 'world_asset_upgrades', count(*)::bigint from world_asset_upgrades union all
    select 'world_companies', count(*)::bigint from world_companies union all
    select 'world_shops', count(*)::bigint from world_shops union all
    select 'world_listings', count(*)::bigint from world_listings union all
    select 'world_economy_events', count(*)::bigint from world_economy_events union all
    select 'world_purchases', count(*)::bigint from world_purchases union all
    select 'world_work_orders', count(*)::bigint from world_work_orders union all
    select 'world_factions', count(*)::bigint from world_factions union all
    select 'world_faction_standings', count(*)::bigint from world_faction_standings union all
    select 'world_work_deliveries', count(*)::bigint from world_work_deliveries union all
    select 'world_work_acceptances', count(*)::bigint from world_work_acceptances union all
    select 'world_work_rejections', count(*)::bigint from world_work_rejections union all
    select 'world_work_reopens', count(*)::bigint from world_work_reopens union all
    select 'world_work_cancellations', count(*)::bigint from world_work_cancellations
  ), bad_counts as (
    select
      coalesce(plan_tables.table_name, actual_counts.table_name) as table_name,
      plan_tables.expected_count,
      actual_counts.actual_count
    from plan_tables
    full join actual_counts using (table_name)
    where coalesce(plan_tables.expected_count, -1) <> coalesce(actual_counts.actual_count, -1)
  )
  select jsonb_agg(jsonb_build_object(
    'table_name', table_name,
    'expected', expected_count,
    'actual', actual_count
  )) into mismatches
  from bad_counts;

  if mismatches is not null then
    raise exception 'snapshot DB row-count mismatches: %', mismatches;
  end if;
end
\$\$;
" >/dev/null

echo "==> validating repository write-set audit in $TMP_DB"
run_tmp_sql "
do \$\$
begin
  if (select count(*) from league_state_repository_write_set_audits where cutover_phase = 'shadow_snapshot') < 12 then
    raise exception 'repository write-set audit rows missing';
  end if;
  if (select count(*) from league_state_repository_write_set_audits where command = 'world_work_accept' and 'world_work_acceptances' = any(tables)) < 1 then
    raise exception 'repository write-set audit missing world_work_accept acceptance table';
  end if;
  if (select count(*) from league_state_repository_write_set_audits where command = 'world_map_move' and 'world_player_positions' = any(tables) and 'world_economy_events' = any(tables)) < 1 then
    raise exception 'repository write-set audit missing world_map_move movement/economy tables';
  end if;
  if (select count(*) from league_state_repository_write_set_audits where command = 'world_work_deliver' and 'world_work_deliveries' = any(tables) and 'world_economy_events' = any(tables) and 'world_faction_standings' = any(tables)) < 1 then
    raise exception 'repository write-set audit missing world_work_deliver delivery/economy/faction tables';
  end if;
end
\$\$;
" >/dev/null

echo "==> validating normalized world home read model in $TMP_DB"
run_tmp_sql "
do \$\$
declare
  read_model jsonb;
begin
  select jsonb_build_object(
    'read_model_version', 'trillionnium_normalized_world_home_read_model_v1',
    'source_tables', jsonb_build_array('world_events', 'world_relationships', 'world_map_nodes', 'world_contracts', 'world_work_orders', 'world_faction_standings'),
    'world_event_count', (select count(*) from world_events),
    'world_relationship_count', (select count(*) from world_relationships),
    'world_map_node_count', (select count(*) from world_map_nodes),
    'world_contract_count', (select count(*) from world_contracts),
    'world_work_order_count', (select count(*) from world_work_orders),
    'world_faction_standing_count', (select count(*) from world_faction_standings),
    'latest_event_ids', coalesce((select jsonb_agg(event_id order by created_at desc, event_id desc) from (select event_id, created_at from world_events order by created_at desc, event_id desc limit 6) recent_events), '[]'::jsonb),
    'latest_work_order_ids', coalesce((select jsonb_agg(work_order_id order by created_at desc, work_order_id desc) from (select work_order_id, created_at from world_work_orders order by created_at desc, work_order_id desc limit 6) recent_work_orders), '[]'::jsonb)
  ) into read_model;

  if read_model->>'read_model_version' <> 'trillionnium_normalized_world_home_read_model_v1' then
    raise exception 'normalized world home read model version mismatch: %', read_model;
  end if;
  if (read_model->>'world_event_count')::bigint < 1 then
    raise exception 'normalized world home read model missing world events: %', read_model;
  end if;
  if (read_model->>'world_map_node_count')::bigint < 1 then
    raise exception 'normalized world home read model missing map nodes: %', read_model;
  end if;
  if jsonb_array_length(read_model->'latest_event_ids') < 1 then
    raise exception 'normalized world home read model missing latest events: %', read_model;
  end if;

  select jsonb_build_object(
    'read_model_version', 'trillionnium_normalized_client_feed_read_model_v1',
    'source_tables', jsonb_build_array('world_events', 'world_contracts', 'world_purchases', 'world_work_orders', 'world_work_deliveries', 'world_work_acceptances', 'world_work_rejections', 'world_work_reopens', 'world_work_cancellations', 'world_economy_events'),
    'world_event_count', (select count(*) from world_events),
    'world_contract_count', (select count(*) from world_contracts),
    'world_purchase_count', (select count(*) from world_purchases),
    'world_work_order_count', (select count(*) from world_work_orders),
    'world_work_delivery_count', (select count(*) from world_work_deliveries),
    'world_work_acceptance_count', (select count(*) from world_work_acceptances),
    'world_work_rejection_count', (select count(*) from world_work_rejections),
    'world_work_reopen_count', (select count(*) from world_work_reopens),
    'world_work_cancellation_count', (select count(*) from world_work_cancellations),
    'world_economy_event_count', (select count(*) from world_economy_events),
    'feed_item_count', (
      select count(*)
      from (
        select event_id as item_id from world_events
        union all select contract_id from world_contracts
        union all select purchase_id from world_purchases
        union all select work_order_id from world_work_orders
        union all select delivery_id from world_work_deliveries
        union all select acceptance_id from world_work_acceptances
        union all select rejection_id from world_work_rejections
        union all select reopen_id from world_work_reopens
        union all select cancellation_id from world_work_cancellations
        union all select economy_event_id from world_economy_events
      ) feed_items
    ),
    'latest_feed_items', coalesce((
      select jsonb_agg(jsonb_build_object('kind', feed_kind, 'id', item_id) order by created_at desc, item_id desc)
      from (
        select feed_kind, item_id, created_at
        from (
          select 'event' as feed_kind, event_id as item_id, created_at from world_events
          union all select 'contract', contract_id, created_at from world_contracts
          union all select 'purchase', purchase_id, created_at from world_purchases
          union all select 'work_order', work_order_id, created_at from world_work_orders
          union all select 'work_delivery', delivery_id, created_at from world_work_deliveries
          union all select 'work_acceptance', acceptance_id, created_at from world_work_acceptances
          union all select 'work_rejection', rejection_id, created_at from world_work_rejections
          union all select 'work_reopen', reopen_id, created_at from world_work_reopens
          union all select 'work_cancellation', cancellation_id, created_at from world_work_cancellations
          union all select 'economy_event', economy_event_id, created_at from world_economy_events
        ) raw_feed_items
        order by created_at desc, item_id desc
        limit 12
      ) latest_feed_items
    ), '[]'::jsonb)
  ) into read_model;

  if read_model->>'read_model_version' <> 'trillionnium_normalized_client_feed_read_model_v1' then
    raise exception 'normalized client feed read model version mismatch: %', read_model;
  end if;
  if (read_model->>'feed_item_count')::bigint < 1 then
    raise exception 'normalized client feed read model missing feed items: %', read_model;
  end if;
  if jsonb_array_length(read_model->'latest_feed_items') < 1 then
    raise exception 'normalized client feed read model missing latest feed items: %', read_model;
  end if;
end
\$\$;
" >/dev/null

echo "==> validating normalized repository read-switch snapshot in $TMP_DB"
run_tmp_sql "
do \$\$
declare
  latest_state jsonb;
begin
  select state into latest_state
  from league_state_snapshots
  where snapshot_kind = 'consumer_entry_json_v1'
  order by created_at desc
  limit 1;

  if latest_state is null then
    raise exception 'normalized repository read-switch snapshot missing';
  end if;
  if not (latest_state ? 'players_by_matrix_user') then
    raise exception 'normalized repository read-switch snapshot missing players_by_matrix_user';
  end if;
  if not (latest_state ? 'world_map_nodes') then
    raise exception 'normalized repository read-switch snapshot missing world_map_nodes';
  end if;
  if not (latest_state ? 'world_work_orders') then
    raise exception 'normalized repository read-switch snapshot missing world_work_orders';
  end if;
  if not (latest_state ? 'world_economy_events') then
    raise exception 'normalized repository read-switch snapshot missing world_economy_events';
  end if;
end
\$\$;
" >/dev/null

run_tmp_sql "
select jsonb_build_object(
  'ok', true,
  'database', current_database(),
  'normalized_world_row_count_parity', true,
  'normalized_world_home_read_model_checked', true,
  'normalized_client_feed_read_model_checked', true,
  'repository_read_switch_snapshot_checked', true,
  'latest_snapshot_hash', (select state_hash from league_state_snapshots where snapshot_kind = 'consumer_entry_json_v1' order by created_at desc limit 1),
  'snapshot_rows', (select count(*) from league_state_snapshots),
  'repository_audit_rows', (select count(*) from league_state_repository_snapshots),
  'repository_write_set_audit_rows', (select count(*) from league_state_repository_write_set_audits),
  'world_map_nodes', (select count(*) from world_map_nodes),
  'world_work_acceptances', (select count(*) from world_work_acceptances),
  'world_work_cancellations', (select count(*) from world_work_cancellations)
)::text as snapshot_db_check;
"
