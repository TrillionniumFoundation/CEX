#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

ADMIN_TOKEN="${LEDGER_ADMIN_TOKEN:-${IDENTITY_ADMIN_TOKEN:?ledger admin token required}}"
PRIMARY_URL="${LEDGER_BASE_URL:-http://127.0.0.1:7002}"
SECONDARY_URL="http://127.0.0.1:7012"
RUN_ID="dr-$(date +%s)-${RANDOM}"
RESTORE_DB="cex_trnm_restore_${RANDOM}_$$"
WORK_DIR="$(mktemp -d /tmp/cex-trnm-dr.XXXXXX)"
SECONDARY_PID=""
CURRENT_PHASE="bootstrap"
cleanup() {
  [[ -z "$SECONDARY_PID" ]] || kill "$SECONDARY_PID" >/dev/null 2>&1 || true
  cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" dropdb -U "$CEX_POSTGRES_USER" --if-exists "$RESTORE_DB" >/dev/null 2>&1 || true
  rm -rf "$WORK_DIR"
}
trap 'status=$?; if [[ $status -ne 0 ]]; then echo "TRNM economy DR gate failed in phase $CURRENT_PHASE" >&2; fi; cleanup; exit $status' EXIT

CURRENT_PHASE="logical-backup"
cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" \
  pg_dump -U "$CEX_POSTGRES_USER" -d "$CEX_POSTGRES_DB" --format=custom \
    --table=organizations --table=accounts --table=ledger_entries \
    --table='trnm_*' >"$WORK_DIR/cex.dump"
test -s "$WORK_DIR/cex.dump"
cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" createdb -U "$CEX_POSTGRES_USER" "$RESTORE_DB"
cex_docker exec -i "$CEX_POSTGRES_CONTAINER_NAME" \
  pg_restore -U "$CEX_POSTGRES_USER" -d "$RESTORE_DB" --no-owner --no-privileges <"$WORK_DIR/cex.dump"

CURRENT_PHASE="restore-parity"
primary_counts="$(cex_psql_stdin -Atc "select json_build_object(
  'intents',(select count(*) from trnm_economic_intents),
  'receipts',(select count(*) from trnm_economic_receipts),
  'escrows',(select count(*) from trnm_escrow_trades),
  'identities',(select count(*) from trnm_player_identities));")"
restored_counts="$(cex_docker exec "$CEX_POSTGRES_CONTAINER_NAME" psql -U "$CEX_POSTGRES_USER" -d "$RESTORE_DB" -Atc "select json_build_object(
  'intents',(select count(*) from trnm_economic_intents),
  'receipts',(select count(*) from trnm_economic_receipts),
  'escrows',(select count(*) from trnm_escrow_trades),
  'identities',(select count(*) from trnm_player_identities));")"
jq -e --argjson restored "$restored_counts" '. == $restored' <<<"$primary_counts" >/dev/null

ledger_binary="$CEX_PROJECT_ROOT/target/release/ledger-service"
[[ -x "$ledger_binary" ]] || cargo build --release -p ledger-service
CURRENT_PHASE="secondary-ledger-start"
DATABASE_URL="$(cex_effective_database_url)" LEDGER_FAIL_FAST=true \
  LEDGER_BIND_ADDR=127.0.0.1:7012 LEDGER_ADMIN_TOKEN="$ADMIN_TOKEN" \
  TRNM_VALUE_ENTITLEMENT_SIGNING_SECRET="trnm-entitlement-signing-v1:$IDENTITY_ADMIN_TOKEN" \
  TRNM_PLAYER_SESSION_SIGNING_SECRET="trnm-player-session-signing-v1:$IDENTITY_ADMIN_TOKEN" \
  TRNM_REQUIRE_PLAYER_SESSION=true TRNM_ALLOW_SYSTEM_ECONOMY_OPERATIONS=true \
  "$ledger_binary" >"$WORK_DIR/secondary-ledger.log" 2>&1 &
SECONDARY_PID=$!
for _ in $(seq 1 60); do
  curl -fsS "$SECONDARY_URL/v1/trnm/economy/readiness" >/dev/null 2>&1 && break
  sleep 1
done
curl -fsS "$SECONDARY_URL/v1/trnm/economy/readiness" | jq -e '.status == "ok"' >/dev/null

account_id="$(curl -fsS "$PRIMARY_URL/v1/accounts" -H "x-admin-token: $ADMIN_TOKEN" \
  -H 'content-type: application/json' --data-binary "$(jq -cn \
    '{org_id:"00000000-0000-0000-0000-00000000ce01",account_type:"trnm-dr",currency_unit:"credit",initial_balance:0}')" | jq -er '.account_id')"
entitlement="$(curl -fsS "$PRIMARY_URL/v1/trnm/economy/entitlements" \
  -H "x-admin-token: $ADMIN_TOKEN" -H 'content-type: application/json' \
  --data-binary "$(jq -cn --arg run "$RUN_ID" --arg account "$account_id" \
  '{actor_id:$run,account_id:$account,source:"battle",source_id:($run+":battle"),intent_id:($run+":reward"),amount_credits:25}')")"
intent="$(jq -cn --arg run "$RUN_ID" --arg account "$account_id" --argjson entitlement "$entitlement" '{intent:{
  protocol_version:"term_exchange_protocol_v2",intent_id:($run+":reward"),
  term_id:"trnm-dr",term_version:"2.3.0",domain:"trnm_game",kind:"release_reward",
  idempotency_key:{scope:"trnm-dr",key:($run+":reward")},
  actors:[{actor_id:$run,actor_kind:"player",account_id:$account}],assets:[],
  amount_credits:25,currency:"credit",metadata:{cross_instance:true,server_signed_value_entitlement:$entitlement},created_at_epoch:(now|floor)}}')"

CURRENT_PHASE="cross-instance-race"
curl -fsS "$PRIMARY_URL/v1/trnm/economy/intents" -H "x-admin-token: $ADMIN_TOKEN" \
  -H 'x-trnm-system-operation: true' \
  -H 'content-type: application/json' --data-binary "$intent" >"$WORK_DIR/primary.json" &
p1=$!
curl -fsS "$SECONDARY_URL/v1/trnm/economy/intents" -H "x-admin-token: $ADMIN_TOKEN" \
  -H 'x-trnm-system-operation: true' \
  -H 'content-type: application/json' --data-binary "$intent" >"$WORK_DIR/secondary.json" &
p2=$!
wait "$p1" "$p2"
cmp "$WORK_DIR/primary.json" "$WORK_DIR/secondary.json"

db_exactly_once="$(cex_psql_stdin -Atc "select json_build_object(
  'intent_count',(select count(*) from trnm_economic_intents where intent_id = '$RUN_ID:reward'),
  'receipt_count',(select count(*) from trnm_economic_receipts where intent_id = '$RUN_ID:reward'),
  'entry_count',(select count(*) from ledger_entries where idempotency_key = '$RUN_ID:reward'),
  'balance',(select balance from accounts where account_id = '$account_id')); ")"
jq -e '.intent_count == 1 and .receipt_count == 1 and .entry_count == 1 and (.balance|tonumber) == 25' \
  <<<"$db_exactly_once" >/dev/null

CURRENT_PHASE="report"
jq -n --arg run_id "$RUN_ID" --argjson backup "$primary_counts" \
  --argjson restored "$restored_counts" --argjson exactly_once "$db_exactly_once" \
  '{status:"passed",run_id:$run_id,backup_restore:{primary:$backup,restored:$restored,logical_row_count_parity:true},
    cross_instance_exactly_once:$exactly_once,postgres_dump_nonempty:true,
    pitr_boundary:"logical restore and cross-instance race proven here; physical WAL/PITR is proven by check-trnm-postgres-pitr-failover.sh; multi-host HA remains external"}'
