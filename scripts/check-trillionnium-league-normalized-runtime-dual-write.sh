#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"

cex_load_env
cex_require_cmd cargo curl node >/dev/null

TMP_DB="${CEX_NORMALIZED_RUNTIME_TMP_DB:-cex_normalized_runtime_$(date +%s)_$$}"
if [[ ! "$TMP_DB" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
  echo "unsafe temp database name: $TMP_DB" >&2
  exit 1
fi

BASE_URL="$(cex_effective_database_url)"
if [[ "$BASE_URL" != *"127.0.0.1"* && "$BASE_URL" != *"localhost"* && "${CEX_ALLOW_NONLOCAL_NORMALIZED_RUNTIME_CHECK:-0}" != "1" ]]; then
  if ! cex_can_use_docker_postgres; then
    echo "refusing non-local DATABASE_URL for normalized runtime check: $BASE_URL" >&2
    echo "set CEX_ALLOW_NONLOCAL_NORMALIZED_RUNTIME_CHECK=1 only for an isolated disposable database" >&2
    exit 1
  fi
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/cex-normalized-runtime.XXXXXX")"
LOG_DIR="$TMP_DIR/logs"
mkdir -p "$LOG_DIR"
APP_PID=""
SECOND_APP_PID=""

port_open() {
  local port="$1"
  (exec 3<>"/dev/tcp/127.0.0.1/${port}") >/dev/null 2>&1
}

choose_port() {
  local start="$1"
  local port="$start"
  for _ in $(seq 1 80); do
    if ! port_open "$port"; then
      printf '%s\n' "$port"
      return 0
    fi
    port=$((port + 1))
  done
  echo "no free localhost port near $start" >&2
  return 1
}

APP_PORT="${CEX_NORMALIZED_RUNTIME_PORT:-$(choose_port 18940)}"
READ_SWITCH_PORT="${CEX_NORMALIZED_READ_SWITCH_PORT:-$(choose_port $((APP_PORT + 40)))}"

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

drop_tmp_db() {
  run_admin_sql "drop database if exists \"$TMP_DB\" with (force);" >/dev/null 2>&1 || \
    run_admin_sql "drop database if exists \"$TMP_DB\";" >/dev/null 2>&1 || true
}

cleanup() {
  if [[ -n "${APP_PID:-}" ]] && kill -0 "$APP_PID" >/dev/null 2>&1; then
    kill "$APP_PID" >/dev/null 2>&1 || true
    wait "$APP_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "${SECOND_APP_PID:-}" ]] && kill -0 "$SECOND_APP_PID" >/dev/null 2>&1; then
    kill "$SECOND_APP_PID" >/dev/null 2>&1 || true
    wait "$SECOND_APP_PID" >/dev/null 2>&1 || true
  fi
  drop_tmp_db
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

wait_for_health() {
  local base_url="$1"
  local log_file="$2"
  local label="$3"
  for _ in $(seq 1 90); do
    if curl -fsS "$base_url/health" >/dev/null 2>&1; then
      return 0
    fi
    if [[ -s "$log_file" ]] && grep -q "invalid consumer-entry-api runtime profile\|failed to connect normalized repository database\|normalized repository read switch" "$log_file"; then
      echo "${label} failed during startup" >&2
      tail -80 "$log_file" >&2
      return 1
    fi
    sleep 1
  done
  echo "${label} health timeout: $base_url/health" >&2
  tail -120 "$log_file" >&2 || true
  return 1
}

cex_wait_postgres 60 1 >/dev/null
drop_tmp_db
run_admin_sql "create database \"$TMP_DB\";" >/dev/null

for migration in "$PROJECT_ROOT"/migrations/*.sql; do
  [[ -f "$migration" ]] || continue
  echo "==> applying $(basename "$migration") to $TMP_DB"
  run_tmp_file "$migration" >/dev/null
done

APP_DB_URL="${BASE_URL%/*}/$TMP_DB"
DUAL_WRITE_LOG="$LOG_DIR/consumer-entry-api-dual-write.log"
DUAL_WRITE_STATE="$TMP_DIR/dual-write-state.json"
DUAL_WRITE_SQL_SNAPSHOT="$TMP_DIR/dual-write-snapshot.sql"
SMOKE_BODY="Runtime dual-write smoke: record normalized repository evidence, risk gate, acceptance standard, and next step."

(
  cd "$PROJECT_ROOT"
  CONSUMER_ENTRY_BIND_ADDR="127.0.0.1:$APP_PORT" \
  CEX_RUNTIME_PROFILE="local_dev" \
  CONSUMER_ENTRY_RUNTIME_PROFILE="local_dev" \
  CONSUMER_ENTRY_INGRESS_TOKEN="" \
  CONSUMER_ENTRY_REQUIRE_SESSION_AUTH="false" \
  CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING="false" \
  CONSUMER_ENTRY_REPLAY_STORE_PATH="" \
  CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH="" \
  CONSUMER_ENTRY_LEAGUE_STATE_PATH="$DUAL_WRITE_STATE" \
  CONSUMER_ENTRY_LEAGUE_SQL_SNAPSHOT_PATH="$DUAL_WRITE_SQL_SNAPSHOT" \
  CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL="$APP_DB_URL" \
  CONSUMER_ENTRY_LEAGUE_NORMALIZED_DUAL_WRITE_ENABLED="true" \
  CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED="false" \
  CONSUMER_ENTRY_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED="true" \
  cargo run -p consumer-entry-api >"$DUAL_WRITE_LOG" 2>&1
) &
APP_PID=$!

BASE_APP_URL="http://127.0.0.1:$APP_PORT"
wait_for_health "$BASE_APP_URL" "$DUAL_WRITE_LOG" "consumer-entry-api dual-write"

WORLD_ACTION_STATUS="$TMP_DIR/world-action-status.txt"
if ! curl -sS -o "$TMP_DIR/world-action-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/action" \
  -H 'content-type: application/json' \
  -d "{\"matrix_user_id\":\"@runtime-dual:local.dev\",\"room_id\":\"!runtime-dual:local.dev\",\"location_id\":\"zbj-market-gate\",\"body\":\"$SMOKE_BODY\"}" \
  > "$WORLD_ACTION_STATUS"; then
  echo "runtime dual-write world_action curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-action-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$WORLD_ACTION_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_action returned HTTP $(cat "$WORLD_ACTION_STATUS")" >&2
  cat "$TMP_DIR/world-action-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

CONTRACT_ACTION_STATUS="$TMP_DIR/world-contract-action-status.txt"
if ! curl -sS -o "$TMP_DIR/world-contract-action-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/action" \
  -H 'content-type: application/json' \
  -d '{"matrix_user_id":"@runtime-dual:local.dev","room_id":"!runtime-dual:local.dev","location_id":"zbj-market-gate","cex_task_id":"runtime-contract-task-direct-write","cex_status":"completed","body":"Register direct-write contract task with evidence source, risk gate, acceptance standard, completion proof, and next action."}' \
  > "$CONTRACT_ACTION_STATUS"; then
  echo "runtime dual-write contract seed world_action curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-contract-action-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$CONTRACT_ACTION_STATUS")" != "200" ]]; then
  echo "runtime dual-write contract seed world_action returned HTTP $(cat "$CONTRACT_ACTION_STATUS")" >&2
  cat "$TMP_DIR/world-contract-action-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi
CONTRACT_ID="$(node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); process.stdout.write(j?.contract?.contract_id || "");' "$TMP_DIR/world-contract-action-response.json")"
if [[ -z "$CONTRACT_ID" ]]; then
  echo "runtime dual-write contract seed response did not expose contract.contract_id" >&2
  cat "$TMP_DIR/world-contract-action-response.json" >&2 || true
  exit 1
fi

CONTRACT_COMPLETE_STATUS="$TMP_DIR/world-contract-complete-status.txt"
if ! curl -sS -o "$TMP_DIR/world-contract-complete-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/contracts/$CONTRACT_ID/complete" \
  -H 'content-type: application/json' \
  -d '{"matrix_user_id":"@runtime-dual:local.dev","room_id":"!runtime-dual:local.dev","body":"Complete direct-write contract with delivery evidence, acceptance proof, risk control, source data, self-review, and normalized repository parity notes."}' \
  > "$CONTRACT_COMPLETE_STATUS"; then
  echo "runtime dual-write world_contract_completion curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-contract-complete-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$CONTRACT_COMPLETE_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_contract_completion returned HTTP $(cat "$CONTRACT_COMPLETE_STATUS")" >&2
  cat "$TMP_DIR/world-contract-complete-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

WORLD_MOVE_STATUS="$TMP_DIR/world-map-move-status.txt"
if ! curl -sS -o "$TMP_DIR/world-map-move-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/map/move" \
  -H 'content-type: application/json' \
  -d '{"matrix_user_id":"@runtime-dual:local.dev","room_id":"!runtime-dual:local.dev","target":"south"}' \
  > "$WORLD_MOVE_STATUS"; then
  echo "runtime dual-write world_map_move curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-map-move-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$WORLD_MOVE_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_map_move returned HTTP $(cat "$WORLD_MOVE_STATUS")" >&2
  cat "$TMP_DIR/world-map-move-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

ASSET_SEED_STATUS="$TMP_DIR/world-asset-seed-status.txt"
if ! curl -sS -o "$TMP_DIR/world-asset-seed-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/action" \
  -H 'content-type: application/json' \
  -d '{"matrix_user_id":"@runtime-dual:local.dev","room_id":"!runtime-dual:local.dev","location_id":"starter-studio","body":"Build direct-write asset seed for normalized repository helper validation."}' \
  > "$ASSET_SEED_STATUS"; then
  echo "runtime dual-write asset seed world_action curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-asset-seed-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$ASSET_SEED_STATUS")" != "200" ]]; then
  echo "runtime dual-write asset seed world_action returned HTTP $(cat "$ASSET_SEED_STATUS")" >&2
  cat "$TMP_DIR/world-asset-seed-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

ASSET_UPGRADE_STATUS="$TMP_DIR/world-asset-upgrade-status.txt"
if ! curl -sS -o "$TMP_DIR/world-asset-upgrade-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/assets/latest/upgrade" \
  -H 'content-type: application/json' \
  -d '{"matrix_user_id":"@runtime-dual:local.dev","room_id":"!runtime-dual:local.dev","body":"Direct-write asset upgrade package: evidence, risk control, acceptance standard, and next normalized write helper."}' \
  > "$ASSET_UPGRADE_STATUS"; then
  echo "runtime dual-write world_asset_upgrade curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-asset-upgrade-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$ASSET_UPGRADE_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_asset_upgrade returned HTTP $(cat "$ASSET_UPGRADE_STATUS")" >&2
  cat "$TMP_DIR/world-asset-upgrade-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

COMPANY_STATUS="$TMP_DIR/world-company-status.txt"
if ! curl -sS -o "$TMP_DIR/world-company-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/companies" \
  -H 'content-type: application/json' \
  -d '{"matrix_user_id":"@runtime-dual:local.dev","asset_id":"latest","body":"Launch direct-write company storefront with operating loop, customer promise, evidence package, and revenue model."}' \
  > "$COMPANY_STATUS"; then
  echo "runtime dual-write world_company curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-company-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$COMPANY_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_company returned HTTP $(cat "$COMPANY_STATUS")" >&2
  cat "$TMP_DIR/world-company-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

# Listing ids are timestamp-derived at one-second granularity. Keep the direct
# world_listing smoke from colliding with the auto-listing created by
# world_company when the script runs very quickly on local CI.
sleep 1

LISTING_STATUS="$TMP_DIR/world-listing-status.txt"
if ! curl -sS -o "$TMP_DIR/world-listing-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/listings" \
  -H 'content-type: application/json' \
  -d '{"matrix_user_id":"@runtime-dual:local.dev","company_id":"latest","body":"Publish direct-write listing with customer deliverable, pricing logic, evidence data source, risk control, acceptance proof, self-review, and next action plan."}' \
  > "$LISTING_STATUS"; then
  echo "runtime dual-write world_listing curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-listing-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$LISTING_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_listing returned HTTP $(cat "$LISTING_STATUS")" >&2
  cat "$TMP_DIR/world-listing-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi
LISTING_ID="$(node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); process.stdout.write(j?.listing?.listing_id || "");' "$TMP_DIR/world-listing-response.json")"
if [[ -z "$LISTING_ID" ]]; then
  echo "runtime dual-write world_listing response did not expose listing.listing_id" >&2
  cat "$TMP_DIR/world-listing-response.json" >&2 || true
  exit 1
fi

BUY_STATUS="$TMP_DIR/world-buy-status.txt"
if ! curl -sS -o "$TMP_DIR/world-buy-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/listings/$LISTING_ID/buy" \
  -H 'content-type: application/json' \
  -d '{"matrix_user_id":"@runtime-buyer:local.dev","room_id":"!runtime-dual:local.dev","body":"Buy the direct-write listing and open normalized repository work order evidence for parity validation."}' \
  > "$BUY_STATUS"; then
  echo "runtime dual-write world_buy curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-buy-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$BUY_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_buy returned HTTP $(cat "$BUY_STATUS")" >&2
  cat "$TMP_DIR/world-buy-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi
BUY_WORK_ORDER_ID="$(node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); process.stdout.write(j?.work_order?.work_order_id || "");' "$TMP_DIR/world-buy-response.json")"
BUY_SELLER_ID="$(node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); process.stdout.write(j?.work_order?.seller_matrix_user_id || "");' "$TMP_DIR/world-buy-response.json")"
BUY_BUYER_ID="$(node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); process.stdout.write(j?.work_order?.buyer_matrix_user_id || "");' "$TMP_DIR/world-buy-response.json")"
if [[ -z "$BUY_WORK_ORDER_ID" || -z "$BUY_SELLER_ID" || -z "$BUY_BUYER_ID" ]]; then
  echo "runtime dual-write world_buy response did not expose work_order ids" >&2
  cat "$TMP_DIR/world-buy-response.json" >&2 || true
  exit 1
fi

DELIVER_STATUS="$TMP_DIR/world-work-deliver-status.txt"
if ! curl -sS -o "$TMP_DIR/world-work-deliver-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/work-orders/$BUY_WORK_ORDER_ID/deliver" \
  -H 'content-type: application/json' \
  -d "{\"matrix_user_id\":\"$BUY_SELLER_ID\",\"room_id\":\"!runtime-dual:local.dev\",\"body\":\"Deliver direct-write work package with customer evidence, source data, acceptance checklist, risk controls, self-review, and next action plan for normalized repository validation.\"}" \
  > "$DELIVER_STATUS"; then
  echo "runtime dual-write world_work_deliver curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-work-deliver-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$DELIVER_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_work_deliver returned HTTP $(cat "$DELIVER_STATUS")" >&2
  cat "$TMP_DIR/world-work-deliver-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

ACCEPT_STATUS="$TMP_DIR/world-work-accept-status.txt"
if ! curl -sS -o "$TMP_DIR/world-work-accept-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/work-orders/$BUY_WORK_ORDER_ID/accept" \
  -H 'content-type: application/json' \
  -d "{\"matrix_user_id\":\"$BUY_BUYER_ID\",\"room_id\":\"!runtime-dual:local.dev\",\"body\":\"Accept direct-write delivery after evidence review, quality proof, risk closeout, payment consume check, and next collaboration note.\"}" \
  > "$ACCEPT_STATUS"; then
  echo "runtime dual-write world_work_accept curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-work-accept-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$ACCEPT_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_work_accept returned HTTP $(cat "$ACCEPT_STATUS")" >&2
  cat "$TMP_DIR/world-work-accept-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

sleep 1

REVIEW_LISTING_STATUS="$TMP_DIR/world-review-listing-status.txt"
if ! curl -sS -o "$TMP_DIR/world-review-listing-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/listings" \
  -H 'content-type: application/json' \
  -d '{"matrix_user_id":"@runtime-dual:local.dev","company_id":"latest","body":"Publish second direct-write listing for rejection and reopen validation with customer evidence, data source, risk control, acceptance standard, and next action."}' \
  > "$REVIEW_LISTING_STATUS"; then
  echo "runtime dual-write second world_listing curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-review-listing-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$REVIEW_LISTING_STATUS")" != "200" ]]; then
  echo "runtime dual-write second world_listing returned HTTP $(cat "$REVIEW_LISTING_STATUS")" >&2
  cat "$TMP_DIR/world-review-listing-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi
REVIEW_LISTING_ID="$(node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); process.stdout.write(j?.listing?.listing_id || "");' "$TMP_DIR/world-review-listing-response.json")"
if [[ -z "$REVIEW_LISTING_ID" ]]; then
  echo "runtime dual-write second world_listing response did not expose listing.listing_id" >&2
  cat "$TMP_DIR/world-review-listing-response.json" >&2 || true
  exit 1
fi

REVIEW_BUY_STATUS="$TMP_DIR/world-review-buy-status.txt"
if ! curl -sS -o "$TMP_DIR/world-review-buy-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/listings/$REVIEW_LISTING_ID/buy" \
  -H 'content-type: application/json' \
  -d '{"matrix_user_id":"@runtime-buyer-review:local.dev","room_id":"!runtime-dual:local.dev","body":"Buy the second direct-write listing to validate rejection reopen cancel normalized repository helpers."}' \
  > "$REVIEW_BUY_STATUS"; then
  echo "runtime dual-write second world_buy curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-review-buy-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$REVIEW_BUY_STATUS")" != "200" ]]; then
  echo "runtime dual-write second world_buy returned HTTP $(cat "$REVIEW_BUY_STATUS")" >&2
  cat "$TMP_DIR/world-review-buy-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi
REVIEW_WORK_ORDER_ID="$(node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); process.stdout.write(j?.work_order?.work_order_id || "");' "$TMP_DIR/world-review-buy-response.json")"
REVIEW_SELLER_ID="$(node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); process.stdout.write(j?.work_order?.seller_matrix_user_id || "");' "$TMP_DIR/world-review-buy-response.json")"
REVIEW_BUYER_ID="$(node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); process.stdout.write(j?.work_order?.buyer_matrix_user_id || "");' "$TMP_DIR/world-review-buy-response.json")"
if [[ -z "$REVIEW_WORK_ORDER_ID" || -z "$REVIEW_SELLER_ID" || -z "$REVIEW_BUYER_ID" ]]; then
  echo "runtime dual-write second world_buy response did not expose work_order ids" >&2
  cat "$TMP_DIR/world-review-buy-response.json" >&2 || true
  exit 1
fi

REVIEW_DELIVER_STATUS="$TMP_DIR/world-review-deliver-status.txt"
if ! curl -sS -o "$TMP_DIR/world-review-deliver-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/work-orders/$REVIEW_WORK_ORDER_ID/deliver" \
  -H 'content-type: application/json' \
  -d "{\"matrix_user_id\":\"$REVIEW_SELLER_ID\",\"room_id\":\"!runtime-dual:local.dev\",\"body\":\"Deliver review-lane package with customer evidence, source data, known risk, acceptance checklist, self-review, and next revision action.\"}" \
  > "$REVIEW_DELIVER_STATUS"; then
  echo "runtime dual-write second world_work_deliver curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-review-deliver-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$REVIEW_DELIVER_STATUS")" != "200" ]]; then
  echo "runtime dual-write second world_work_deliver returned HTTP $(cat "$REVIEW_DELIVER_STATUS")" >&2
  cat "$TMP_DIR/world-review-deliver-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

REJECT_STATUS="$TMP_DIR/world-work-reject-status.txt"
if ! curl -sS -o "$TMP_DIR/world-work-reject-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/work-orders/$REVIEW_WORK_ORDER_ID/reject" \
  -H 'content-type: application/json' \
  -d "{\"matrix_user_id\":\"$REVIEW_BUYER_ID\",\"room_id\":\"!runtime-dual:local.dev\",\"body\":\"Reject direct-write delivery because evidence gaps remain, risk is not closed, acceptance checklist failed, and next revision is required.\"}" \
  > "$REJECT_STATUS"; then
  echo "runtime dual-write world_work_reject curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-work-reject-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$REJECT_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_work_reject returned HTTP $(cat "$REJECT_STATUS")" >&2
  cat "$TMP_DIR/world-work-reject-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

REOPEN_STATUS="$TMP_DIR/world-work-reopen-status.txt"
if ! curl -sS -o "$TMP_DIR/world-work-reopen-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/work-orders/$REVIEW_WORK_ORDER_ID/reopen" \
  -H 'content-type: application/json' \
  -d "{\"matrix_user_id\":\"$REVIEW_BUYER_ID\",\"room_id\":\"!runtime-dual:local.dev\",\"body\":\"Reopen direct-write work with renewed reserve, clear evidence requirements, risk controls, acceptance standard, and next redelivery plan.\"}" \
  > "$REOPEN_STATUS"; then
  echo "runtime dual-write world_work_reopen curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-work-reopen-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$REOPEN_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_work_reopen returned HTTP $(cat "$REOPEN_STATUS")" >&2
  cat "$TMP_DIR/world-work-reopen-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

CANCEL_STATUS="$TMP_DIR/world-work-cancel-status.txt"
if ! curl -sS -o "$TMP_DIR/world-work-cancel-response.json" -w '%{http_code}' -X POST "$BASE_APP_URL/v1/world/work-orders/$REVIEW_WORK_ORDER_ID/cancel" \
  -H 'content-type: application/json' \
  -d "{\"matrix_user_id\":\"$REVIEW_BUYER_ID\",\"room_id\":\"!runtime-dual:local.dev\",\"body\":\"Cancel reopened direct-write work before delivery, refund buyer reserve, record risk closeout, evidence decision, and next marketplace action.\"}" \
  > "$CANCEL_STATUS"; then
  echo "runtime dual-write world_work_cancel curl failed" >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  cat "$TMP_DIR/world-work-cancel-response.json" >&2 || true
  exit 1
fi
if [[ "$(cat "$CANCEL_STATUS")" != "200" ]]; then
  echo "runtime dual-write world_work_cancel returned HTTP $(cat "$CANCEL_STATUS")" >&2
  cat "$TMP_DIR/world-work-cancel-response.json" >&2 || true
  echo >&2
  tail -120 "$DUAL_WRITE_LOG" >&2 || true
  exit 1
fi

if [[ ! -s "$DUAL_WRITE_SQL_SNAPSHOT" ]]; then
  echo "dual-write SQL snapshot was not written: $DUAL_WRITE_SQL_SNAPSHOT" >&2
  exit 1
fi

run_tmp_sql "
do \$\$
declare
  read_model jsonb;
begin
  if (select count(*) from league_state_snapshots where snapshot_kind = 'consumer_entry_json_v1') < 1 then
    raise exception 'runtime dual-write did not write league_state_snapshots';
  end if;
  if (select count(*) from league_state_repository_snapshots where cutover_phase = 'final_cutover') < 1 then
    raise exception 'runtime dual-write did not write repository audit snapshots';
  end if;
  if (select count(*) from league_state_repository_write_set_audits where cutover_phase = 'final_cutover') < 12 then
    raise exception 'runtime dual-write did not write repository write-set audits';
  end if;
  if (select count(*) from league_state_repository_write_set_audits where command = 'world_action' and 'world_events' = any(tables)) < 1 then
    raise exception 'runtime dual-write write-set audit missing world_action world_events seam';
  end if;
  if (select count(*) from league_state_repository_write_set_audits where command = 'world_contract_completion' and 'world_contract_completions' = any(tables) and 'world_contracts' = any(tables)) < 1 then
    raise exception 'runtime dual-write write-set audit missing world_contract_completion contract/completion tables';
  end if;
  if (select count(*) from league_state_repository_write_set_audits where command = 'world_map_move' and 'world_player_positions' = any(tables) and 'world_economy_events' = any(tables)) < 1 then
    raise exception 'runtime dual-write write-set audit missing world_map_move movement/economy tables';
  end if;
  if (select count(*) from league_state_repository_write_set_audits where command = 'world_work_deliver' and 'world_work_deliveries' = any(tables) and 'world_economy_events' = any(tables) and 'world_faction_standings' = any(tables)) < 1 then
    raise exception 'runtime dual-write write-set audit missing world_work_deliver delivery/economy/faction tables';
  end if;
  if (select count(*) from world_events where actor_matrix_user_id = '@runtime-dual:local.dev' and body = '$SMOKE_BODY') < 1 then
    raise exception 'runtime dual-write world event missing from normalized world_events';
  end if;
  if (select count(*) from world_contracts where actor_matrix_user_id = '@runtime-dual:local.dev' and task_id = 'runtime-contract-task-direct-write') < 1 then
    raise exception 'runtime direct world_contract_completion helper did not preserve world_contracts dependency';
  end if;
  if (select count(*) from world_contract_completions where matrix_user_id = '@runtime-dual:local.dev') < 1 then
    raise exception 'runtime direct world_contract_completion helper did not write world_contract_completions';
  end if;
  if (select count(*) from world_relationships where from_id = '@runtime-dual:local.dev') < 1 then
    raise exception 'runtime dual-write relationship missing from normalized world_relationships';
  end if;
  if (select count(*) from world_map_nodes where node_id = 'zbj-market-gate') < 1 then
    raise exception 'runtime direct world_map_move helper did not write dependency world_map_nodes';
  end if;
  if (select count(*) from world_player_positions where matrix_user_id = '@runtime-dual:local.dev' and node_id = 'zbj-market-gate') < 1 then
    raise exception 'runtime direct world_map_move helper did not write world_player_positions';
  end if;
  if (select count(*) from world_economy_events where matrix_user_id = '@runtime-dual:local.dev' and event_kind = 'map_move' and subject_id = 'zbj-market-gate') < 1 then
    raise exception 'runtime direct world_map_move helper did not write world_economy_events';
  end if;
  if (select count(*) from league_players where matrix_user_id = '@runtime-dual:local.dev') < 1 then
    raise exception 'runtime direct world_asset_upgrade helper did not write league_players';
  end if;
  if (select count(*) from world_assets where owner_matrix_user_id = '@runtime-dual:local.dev' and location_id = 'starter-studio') < 1 then
    raise exception 'runtime direct world_asset_upgrade helper did not write world_assets';
  end if;
  if (select count(*) from world_asset_upgrades where matrix_user_id = '@runtime-dual:local.dev') < 1 then
    raise exception 'runtime direct world_asset_upgrade helper did not write world_asset_upgrades';
  end if;
  if (select count(*) from world_companies where owner_matrix_user_id = '@runtime-dual:local.dev') < 1 then
    raise exception 'runtime direct world_company helper did not write world_companies';
  end if;
  if (select count(*) from world_shops where owner_matrix_user_id = '@runtime-dual:local.dev') < 1 then
    raise exception 'runtime direct world_company helper did not write world_shops';
  end if;
  if (select count(*) from world_listings where owner_matrix_user_id = '@runtime-dual:local.dev') < 2 then
    raise exception 'runtime direct world_listing helper did not write world_listings';
  end if;
  if (select count(*) from world_economy_events where matrix_user_id = '@runtime-dual:local.dev' and event_kind = 'company_launch') < 1 then
    raise exception 'runtime direct world_company helper did not write company_launch economy event';
  end if;
  if (select count(*) from world_economy_events where matrix_user_id = '@runtime-dual:local.dev' and event_kind = 'listing_published') < 1 then
    raise exception 'runtime direct world_listing helper did not write listing_published economy event';
  end if;
  if (select count(*) from league_players where matrix_user_id = '@runtime-buyer:local.dev') < 1 then
    raise exception 'runtime direct world_buy helper did not write buyer league_players';
  end if;
  if (select count(*) from world_purchases where buyer_matrix_user_id = '@runtime-buyer:local.dev' and seller_matrix_user_id = '@runtime-dual:local.dev') < 1 then
    raise exception 'runtime direct world_buy helper did not write world_purchases';
  end if;
  if (select count(*) from world_work_orders where buyer_matrix_user_id = '@runtime-buyer:local.dev' and seller_matrix_user_id = '@runtime-dual:local.dev') < 1 then
    raise exception 'runtime direct world_buy helper did not write world_work_orders';
  end if;
  if (select count(*) from world_faction_standings where matrix_user_id in ('@runtime-buyer:local.dev', '@runtime-dual:local.dev')) < 2 then
    raise exception 'runtime direct world_buy helper did not write world_faction_standings';
  end if;
  if (select count(*) from world_economy_events where matrix_user_id = '@runtime-dual:local.dev' and event_kind = 'listing_purchase') < 1 then
    raise exception 'runtime direct world_buy helper did not write listing_purchase economy event';
  end if;
  if (select count(*) from world_work_deliveries where matrix_user_id = '@runtime-dual:local.dev') < 2 then
    raise exception 'runtime direct world_work_deliver helper did not write world_work_deliveries';
  end if;
  if (select count(*) from world_work_acceptances where matrix_user_id = '@runtime-buyer:local.dev') < 1 then
    raise exception 'runtime direct world_work_accept helper did not write world_work_acceptances';
  end if;
  if (select count(*) from world_work_rejections where matrix_user_id = '@runtime-buyer-review:local.dev') < 1 then
    raise exception 'runtime direct world_work_reject helper did not write world_work_rejections';
  end if;
  if (select count(*) from world_work_reopens where matrix_user_id = '@runtime-buyer-review:local.dev') < 1 then
    raise exception 'runtime direct world_work_reopen helper did not write world_work_reopens';
  end if;
  if (select count(*) from world_work_cancellations where matrix_user_id = '@runtime-buyer-review:local.dev') < 1 then
    raise exception 'runtime direct world_work_cancel helper did not write world_work_cancellations';
  end if;
  if (select count(*) from world_purchases where buyer_matrix_user_id = '@runtime-buyer-review:local.dev') < 1 then
    raise exception 'runtime direct review world_buy helper did not write review world_purchases';
  end if;
  if (select count(*) from league_players where matrix_user_id = '@runtime-buyer-review:local.dev') < 1 then
    raise exception 'runtime direct review world_buy helper did not write review buyer league_players';
  end if;
  if (select count(*) from world_economy_events where matrix_user_id = '@runtime-dual:local.dev' and event_kind = 'work_delivered') < 1 then
    raise exception 'runtime direct world_work_deliver helper did not write work_delivered economy event';
  end if;
  if (select count(*) from world_economy_events where matrix_user_id = '@runtime-dual:local.dev' and event_kind = 'work_accepted') < 1 then
    raise exception 'runtime direct world_work_accept helper did not write work_accepted economy event';
  end if;
  if (select count(*) from world_economy_events where matrix_user_id = '@runtime-buyer-review:local.dev' and event_kind = 'work_rejected') < 1 then
    raise exception 'runtime direct world_work_reject helper did not write work_rejected economy event';
  end if;
  if (select count(*) from world_economy_events where matrix_user_id = '@runtime-buyer-review:local.dev' and event_kind = 'work_reopened') < 1 then
    raise exception 'runtime direct world_work_reopen helper did not write work_reopened economy event';
  end if;
  if (select count(*) from world_economy_events where matrix_user_id = '@runtime-buyer-review:local.dev' and event_kind = 'work_cancelled') < 1 then
    raise exception 'runtime direct world_work_cancel helper did not write work_cancelled economy event';
  end if;
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
    raise exception 'runtime normalized world home read model version mismatch: %', read_model;
  end if;
  if (read_model->>'world_event_count')::bigint < 1 then
    raise exception 'runtime normalized world home read model missing world events: %', read_model;
  end if;
  if (read_model->>'world_relationship_count')::bigint < 1 then
    raise exception 'runtime normalized world home read model missing relationships: %', read_model;
  end if;
  if jsonb_array_length(read_model->'latest_event_ids') < 1 then
    raise exception 'runtime normalized world home read model missing latest events: %', read_model;
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
    raise exception 'runtime normalized client feed read model version mismatch: %', read_model;
  end if;
  if (read_model->>'feed_item_count')::bigint < 1 then
    raise exception 'runtime normalized client feed read model missing feed items: %', read_model;
  end if;
  if jsonb_array_length(read_model->'latest_feed_items') < 1 then
    raise exception 'runtime normalized client feed read model missing latest feed items: %', read_model;
  end if;
end
\$\$;
" >/dev/null

curl -fsS "$BASE_APP_URL/health" > "$TMP_DIR/dual-write-health.json"
if ! grep -q '"normalized_dual_write_active":true' "$TMP_DIR/dual-write-health.json"; then
  echo "dual-write health did not expose normalized_dual_write_active=true" >&2
  cat "$TMP_DIR/dual-write-health.json" >&2
  exit 1
fi
if ! grep -q '"normalized_final_cutover_active":true' "$TMP_DIR/dual-write-health.json"; then
  echo "dual-write health did not expose normalized_final_cutover_active=true" >&2
  cat "$TMP_DIR/dual-write-health.json" >&2
  exit 1
fi
if ! grep -q '"normalized_runtime_write_mode":"normalized_sql_primary_world_command_writes_with_snapshot_export_rollback"' "$TMP_DIR/dual-write-health.json"; then
  echo "dual-write health did not expose final-cutover normalized runtime write mode" >&2
  cat "$TMP_DIR/dual-write-health.json" >&2
  exit 1
fi
if ! grep -q '"normalized_direct_write_mode":"typed_sqlx_command_helpers_primary_for_supported_world_commands"' "$TMP_DIR/dual-write-health.json"; then
  echo "dual-write health did not expose normalized final direct-write runtime mode" >&2
  cat "$TMP_DIR/dual-write-health.json" >&2
  exit 1
fi
if ! grep -q '"normalized_direct_write_helper":"execute_normalized_repository_direct_command_write"' "$TMP_DIR/dual-write-health.json"; then
  echo "dual-write health did not expose normalized direct-write helper" >&2
  cat "$TMP_DIR/dual-write-health.json" >&2
  exit 1
fi
if ! grep -q 'trillionnium_normalized_repository_direct_write_v1' "$TMP_DIR/dual-write-health.json"; then
  echo "dual-write health did not expose normalized direct-write contract" >&2
  cat "$TMP_DIR/dual-write-health.json" >&2
  exit 1
fi
if ! grep -q '"normalized_direct_write_transaction_mode":"single_pg_transaction_direct_sql_primary_plus_snapshot_export"' "$TMP_DIR/dual-write-health.json"; then
  echo "dual-write health did not expose normalized direct-write transaction mode" >&2
  cat "$TMP_DIR/dual-write-health.json" >&2
  exit 1
fi
if ! grep -q 'single_pg_transaction_direct_sql_primary_plus_snapshot_export' "$TMP_DIR/dual-write-health.json"; then
  echo "dual-write health direct-write contract did not expose transaction boundary" >&2
  cat "$TMP_DIR/dual-write-health.json" >&2
  exit 1
fi
if ! grep -q '"normalized_unknown_command_mode":"unsupported_world_commands_rejected_no_generated_sql_fallback"' "$TMP_DIR/dual-write-health.json"; then
  echo "dual-write health did not expose final-cutover unsupported command mode" >&2
  cat "$TMP_DIR/dual-write-health.json" >&2
  exit 1
fi
if ! grep -q 'trillionnium_normalized_repository_read_model_v1' "$TMP_DIR/dual-write-health.json"; then
  echo "dual-write health did not expose normalized read-model contract" >&2
  cat "$TMP_DIR/dual-write-health.json" >&2
  exit 1
fi
if ! grep -q 'trillionnium_normalized_client_feed_read_model_v1' "$TMP_DIR/dual-write-health.json"; then
  echo "dual-write health did not expose normalized client-feed read-model contract" >&2
  cat "$TMP_DIR/dual-write-health.json" >&2
  exit 1
fi

kill "$APP_PID" >/dev/null 2>&1 || true
wait "$APP_PID" >/dev/null 2>&1 || true
APP_PID=""

READ_SWITCH_LOG="$LOG_DIR/consumer-entry-api-read-switch.log"
READ_SWITCH_STATE="$TMP_DIR/read-switch-state.json"
(
  cd "$PROJECT_ROOT"
  CONSUMER_ENTRY_BIND_ADDR="127.0.0.1:$READ_SWITCH_PORT" \
  CEX_RUNTIME_PROFILE="local_dev" \
  CONSUMER_ENTRY_RUNTIME_PROFILE="local_dev" \
  CONSUMER_ENTRY_INGRESS_TOKEN="" \
  CONSUMER_ENTRY_REQUIRE_SESSION_AUTH="false" \
  CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING="false" \
  CONSUMER_ENTRY_REPLAY_STORE_PATH="" \
  CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH="" \
  CONSUMER_ENTRY_LEAGUE_STATE_PATH="$READ_SWITCH_STATE" \
  CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL="$APP_DB_URL" \
  CONSUMER_ENTRY_LEAGUE_NORMALIZED_DUAL_WRITE_ENABLED="true" \
  CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED="true" \
  CONSUMER_ENTRY_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED="true" \
  cargo run -p consumer-entry-api >"$READ_SWITCH_LOG" 2>&1
) &
SECOND_APP_PID=$!

READ_SWITCH_URL="http://127.0.0.1:$READ_SWITCH_PORT"
wait_for_health "$READ_SWITCH_URL" "$READ_SWITCH_LOG" "consumer-entry-api read-switch"

curl -fsS "$READ_SWITCH_URL/health" > "$TMP_DIR/read-switch-health.json"
if ! grep -q '"normalized_read_switch_active":true' "$TMP_DIR/read-switch-health.json"; then
  echo "read-switch health did not expose normalized_read_switch_active=true" >&2
  cat "$TMP_DIR/read-switch-health.json" >&2
  exit 1
fi
if ! grep -q '"normalized_read_switch_gate":"latest_snapshot_requires_repository_audit_and_write_set_audit"' "$TMP_DIR/read-switch-health.json"; then
  echo "read-switch health did not expose repository audit/write-set audit gate" >&2
  cat "$TMP_DIR/read-switch-health.json" >&2
  exit 1
fi
if ! grep -q '"normalized_read_switch_source_of_truth_gate":"latest_snapshot_requires_repository_audit_write_set_audit_and_normalized_world_home_and_client_feed_read_models"' "$TMP_DIR/read-switch-health.json"; then
  echo "read-switch health did not expose normalized read-model source-of-truth gate" >&2
  cat "$TMP_DIR/read-switch-health.json" >&2
  exit 1
fi

curl -fsS "$READ_SWITCH_URL/v1/world/home" > "$TMP_DIR/read-switch-world-home.json"
if ! grep -q "$SMOKE_BODY" "$TMP_DIR/read-switch-world-home.json"; then
  echo "read-switch app did not hydrate the dual-written world event" >&2
  cat "$TMP_DIR/read-switch-world-home.json" >&2
  exit 1
fi

run_tmp_sql "
select jsonb_build_object(
  'ok', true,
  'database', current_database(),
  'runtime_dual_write_checked', true,
  'runtime_read_switch_checked', true,
  'repository_audit_rows', (select count(*) from league_state_repository_snapshots),
  'repository_write_set_audit_rows', (select count(*) from league_state_repository_write_set_audits),
  'runtime_command_scoped_write_checked', true,
  'runtime_direct_write_checked', true,
  'runtime_world_home_read_model_checked', true,
  'runtime_client_feed_read_model_checked', true,
  'snapshot_rows', (select count(*) from league_state_snapshots),
  'world_map_node_rows_after_direct_write', (select count(*) from world_map_nodes),
  'world_player_position_rows_after_direct_write', (select count(*) from world_player_positions where matrix_user_id = '@runtime-dual:local.dev'),
  'world_economy_event_rows_after_direct_write', (select count(*) from world_economy_events where matrix_user_id = '@runtime-dual:local.dev' and event_kind = 'map_move'),
  'world_asset_rows_after_direct_write', (select count(*) from world_assets where owner_matrix_user_id = '@runtime-dual:local.dev'),
  'world_asset_upgrade_rows_after_direct_write', (select count(*) from world_asset_upgrades where matrix_user_id = '@runtime-dual:local.dev'),
  'world_company_rows_after_direct_write', (select count(*) from world_companies where owner_matrix_user_id = '@runtime-dual:local.dev'),
  'world_shop_rows_after_direct_write', (select count(*) from world_shops where owner_matrix_user_id = '@runtime-dual:local.dev'),
  'world_listing_rows_after_direct_write', (select count(*) from world_listings where owner_matrix_user_id = '@runtime-dual:local.dev'),
  'league_player_rows_after_direct_write', (select count(*) from league_players where matrix_user_id = '@runtime-dual:local.dev'),
  'buyer_league_player_rows_after_direct_write', (select count(*) from league_players where matrix_user_id = '@runtime-buyer:local.dev'),
  'world_purchase_rows_after_direct_write', (select count(*) from world_purchases where buyer_matrix_user_id = '@runtime-buyer:local.dev'),
  'world_work_order_rows_after_direct_write', (select count(*) from world_work_orders where buyer_matrix_user_id = '@runtime-buyer:local.dev'),
  'world_faction_standing_rows_after_direct_write', (select count(*) from world_faction_standings where matrix_user_id in ('@runtime-buyer:local.dev', '@runtime-dual:local.dev')),
  'review_buyer_league_player_rows_after_direct_write', (select count(*) from league_players where matrix_user_id = '@runtime-buyer-review:local.dev'),
  'review_world_purchase_rows_after_direct_write', (select count(*) from world_purchases where buyer_matrix_user_id = '@runtime-buyer-review:local.dev'),
  'world_work_delivery_rows_after_direct_write', (select count(*) from world_work_deliveries where matrix_user_id = '@runtime-dual:local.dev'),
  'world_work_acceptance_rows_after_direct_write', (select count(*) from world_work_acceptances where matrix_user_id = '@runtime-buyer:local.dev'),
  'world_work_rejection_rows_after_direct_write', (select count(*) from world_work_rejections where matrix_user_id = '@runtime-buyer-review:local.dev'),
  'world_work_reopen_rows_after_direct_write', (select count(*) from world_work_reopens where matrix_user_id = '@runtime-buyer-review:local.dev'),
  'world_work_cancellation_rows_after_direct_write', (select count(*) from world_work_cancellations where matrix_user_id = '@runtime-buyer-review:local.dev'),
  'world_event_rows', (select count(*) from world_events where actor_matrix_user_id = '@runtime-dual:local.dev'),
  'world_relationship_rows', (select count(*) from world_relationships where from_id = '@runtime-dual:local.dev'),
  'dual_write_health_active', true,
  'read_switch_health_active', true
)::text as normalized_runtime_check;
"
