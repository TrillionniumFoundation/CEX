#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

LEDGER_URL="${LEDGER_BASE_URL:-http://127.0.0.1:7002}"
CONSUMER_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
ADMIN_TOKEN="${LEDGER_ADMIN_TOKEN:-${IDENTITY_ADMIN_TOKEN:?IDENTITY_ADMIN_TOKEN is required}}"
ENTRY_TOKEN="${CONSUMER_ENTRY_INGRESS_TOKEN:-$ADMIN_TOKEN}"
ORG_ID="00000000-0000-0000-0000-00000000ce01"
RUN_ID="cross-process-$(date +%s)-${RANDOM}"
WORK_DIR="$(mktemp -d /tmp/cex-trnm-cross-process.XXXXXX)"
CURRENT_PHASE="bootstrap"
trap 'echo "cross-process E2E failed in phase ${CURRENT_PHASE} at line ${LINENO}" >&2' ERR

post_ledger() {
  local path="$1"
  local payload="$2"
  curl -fsS "$LEDGER_URL$path" \
    -H "x-admin-token: $ADMIN_TOKEN" \
    -H 'content-type: application/json' \
    --data-binary "$payload"
}

post_consumer() {
  local path="$1"
  local payload="$2"
  curl -fsS "$CONSUMER_URL$path" \
    -H "x-entry-token: $ENTRY_TOKEN" \
    -H 'content-type: application/json' \
    --data-binary "$payload"
}

create_account() {
  local role="$1"
  local balance="$2"
  post_ledger /v1/accounts "$(jq -cn \
    --arg org "$ORG_ID" --arg role "$role" --argjson balance "$balance" \
    '{org_id:$org,account_type:$role,currency_unit:"credit",initial_balance:$balance}')" \
    | jq -er '.account_id'
}

intent_json() {
  local kind="$1"
  local intent_id="$2"
  local actor_id="$3"
  local account_id="$4"
  local amount="$5"
  local purchase_id="${6:-}"
  local buyer_id="${7:-}"
  local seller_id="${8:-}"
  local reserve_intent_id="${9:-}"
  jq -cn \
    --arg kind "$kind" \
    --arg intent "$intent_id" \
    --arg actor "$actor_id" \
    --arg account "$account_id" \
    --argjson amount "$amount" \
    --arg purchase "$purchase_id" \
    --arg buyer "$buyer_id" \
    --arg seller "$seller_id" \
    --arg reserve "$reserve_intent_id" \
    '{intent:{
      protocol_version:"term_exchange_protocol_v2",
      intent_id:$intent,
      term_id:("trnm-cross-process:"+$intent),
      term_version:"2.2.0",
      domain:"trnm_game",
      kind:$kind,
      idempotency_key:{scope:"trnm-cross-process",key:$intent},
      actors:[{actor_id:$actor,actor_kind:"player",account_id:$account}],
      assets:(if $purchase == "" then [] else [{asset_id:"tradeable-iron",asset_kind:"tradeable_item",quantity:2,unit:"item"}] end),
      amount_credits:$amount,
      currency:"credit",
      metadata:({test_run:true} +
        (if $purchase == "" then {} else {
          purchase_id:$purchase,
          buyer_account_id:$buyer,
          seller_account_id:$seller
        } end) +
        (if $reserve == "" then {} else {reserve_intent_id:$reserve} end)),
      created_at_epoch:(now|floor)
    }}'
}

assert_status() {
  local expected="$1"
  local file="$2"
  jq -e --arg expected "$expected" '.status == $expected and .progression_class == "progression_allowed"' "$file" >/dev/null
}

wait_ready() {
  local attempt
  for attempt in $(seq 1 60); do
    if curl -fsS "$LEDGER_URL/v1/trnm/economy/readiness" >/dev/null 2>&1 \
      && curl -fsS "$CONSUMER_URL/v1/trillionnium/economy/adapters/readiness" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "TRNM economy services did not become ready" >&2
  return 1
}

wait_ready
CURRENT_PHASE="create-accounts"
buyer_id="$(create_account trnm-e2e-buyer 200)"
seller_id="$(create_account trnm-e2e-seller 0)"
actor_id="trnm-e2e-actor-$RUN_ID"

CURRENT_PHASE="reward-idempotency-before-restart"
reward_id="$RUN_ID-reward"
reward_payload="$(intent_json release_reward "$reward_id" "$actor_id" "$buyer_id" 25)"
post_consumer /v1/trillionnium/economy/intents "$reward_payload" >"$WORK_DIR/reward.json"
post_consumer /v1/trillionnium/economy/intents "$reward_payload" >"$WORK_DIR/reward-replay-before.json"
assert_status approved_release "$WORK_DIR/reward.json"
cmp "$WORK_DIR/reward.json" "$WORK_DIR/reward-replay-before.json"

held_purchase="$RUN_ID-held"
CURRENT_PHASE="held-escrow-refund"
held_reserve="$held_purchase-reserve"
post_consumer /v1/trillionnium/economy/intents \
  "$(intent_json reserve "$held_reserve" "$actor_id" "$buyer_id" 20 "$held_purchase" "$buyer_id" "$seller_id")" \
  >"$WORK_DIR/held-reserve.json"
assert_status reserved "$WORK_DIR/held-reserve.json"
post_consumer /v1/trillionnium/economy/intents \
  "$(intent_json settle "$held_purchase-settle" "$actor_id" "$buyer_id" 20 "$held_purchase" "$buyer_id" "$seller_id" "$held_reserve")" \
  >"$WORK_DIR/held-settle.json"
assert_status settled "$WORK_DIR/held-settle.json"
post_consumer /v1/trillionnium/economy/intents \
  "$(intent_json refund "$held_purchase-refund" "$actor_id" "$buyer_id" 20 "$held_purchase" "$buyer_id" "$seller_id")" \
  >"$WORK_DIR/held-refund.json"
assert_status refunded "$WORK_DIR/held-refund.json"

committed_purchase="$RUN_ID-committed"
CURRENT_PHASE="committed-escrow"
committed_reserve="$committed_purchase-reserve"
post_consumer /v1/trillionnium/economy/intents \
  "$(intent_json reserve "$committed_reserve" "$actor_id" "$buyer_id" 30 "$committed_purchase" "$buyer_id" "$seller_id")" \
  >"$WORK_DIR/committed-reserve.json"
assert_status reserved "$WORK_DIR/committed-reserve.json"
post_consumer /v1/trillionnium/economy/intents \
  "$(intent_json settle "$committed_purchase-settle" "$actor_id" "$buyer_id" 30 "$committed_purchase" "$buyer_id" "$seller_id" "$committed_reserve")" \
  >"$WORK_DIR/committed-settle.json"
assert_status settled "$WORK_DIR/committed-settle.json"
post_consumer /v1/trillionnium/economy/intents \
  "$(intent_json consume "$committed_purchase-consume" "$actor_id" "$buyer_id" 30 "$committed_purchase" "$buyer_id" "$seller_id")" \
  >"$WORK_DIR/committed-consume.json"
assert_status consumed "$WORK_DIR/committed-consume.json"

post_consumer /v1/trillionnium/economy/wallet "$(jq -cn \
  --arg actor "$actor_id" --arg account "$buyer_id" \
  '{actor_id:$actor,account_id:$account,reconciliation_cursor:17}')" \
  >"$WORK_DIR/wallet-before-restart.json"

CURRENT_PHASE="service-restart"
systemctl --user restart cex-trnm-ledger.service cex-trnm-consumer.service
wait_ready

CURRENT_PHASE="restart-replay"
post_consumer /v1/trillionnium/economy/intents "$reward_payload" >"$WORK_DIR/reward-replay-after.json"
cmp "$WORK_DIR/reward.json" "$WORK_DIR/reward-replay-after.json"
post_consumer /v1/trillionnium/economy/wallet "$(jq -cn \
  --arg actor "$actor_id" --arg account "$buyer_id" \
  '{actor_id:$actor,account_id:$account,reconciliation_cursor:17}')" \
  >"$WORK_DIR/wallet-after-restart.json"
cmp "$WORK_DIR/wallet-before-restart.json" "$WORK_DIR/wallet-after-restart.json"

post_consumer /v1/trillionnium/economy/intents \
  "$(intent_json chargeback "$committed_purchase-chargeback" "$actor_id" "$buyer_id" 30 "$committed_purchase" "$buyer_id" "$seller_id")" \
  >"$WORK_DIR/committed-chargeback.json"
assert_status seller_chargeback_consumed "$WORK_DIR/committed-chargeback.json"

CURRENT_PHASE="final-wallet"
post_consumer /v1/trillionnium/economy/wallet "$(jq -cn \
  --arg actor "$actor_id" --arg account "$buyer_id" \
  '{actor_id:$actor,account_id:$account,reconciliation_cursor:18}')" \
  >"$WORK_DIR/wallet-final.json"
jq -e '.available_credits == 225 and .reserved_credits == 0 and .observed_at_cursor == 18' \
  "$WORK_DIR/wallet-final.json" >/dev/null

CURRENT_PHASE="database-evidence"
db_evidence="$(cex_psql_stdin -Atc "
  select json_build_object(
    'intents', count(distinct i.intent_id),
    'receipts', count(distinct r.intent_id),
    'ledger_entries', count(distinct l.entry_id),
    'held_refund_status', max(e.status) filter (where e.purchase_id = '$held_purchase'),
    'committed_reversal_status', max(e.status) filter (where e.purchase_id = '$committed_purchase'),
    'cursor', max(c.cursor)
  )
  from trnm_economic_intents i
  join trnm_economic_receipts r on r.intent_id = i.intent_id
  left join ledger_entries l on l.reference_type = 'trnm_native_economy'
    and (l.idempotency_key = i.idempotency_key
      or l.idempotency_key like i.idempotency_key || ':%')
  left join trnm_escrow_trades e on e.purchase_id in ('$held_purchase', '$committed_purchase')
  left join trnm_economy_reconciliation_cursors c on c.actor_id = '$actor_id'
  where i.idempotency_key like '$RUN_ID%';
")"

jq -e '.intents == 8 and .receipts == 8 and .ledger_entries == 9 and
  .held_refund_status == "refunded" and .committed_reversal_status == "reversed" and .cursor == 18' \
  <<<"$db_evidence" >/dev/null

CURRENT_PHASE="report"
jq -n \
  --arg run_id "$RUN_ID" \
  --arg buyer_account_id "$buyer_id" \
  --arg seller_account_id "$seller_id" \
  --argjson wallet "$(<"$WORK_DIR/wallet-final.json")" \
  --argjson database "$db_evidence" \
  '{status:"passed",run_id:$run_id,buyer_account_id:$buyer_account_id,
    seller_account_id:$seller_account_id,wallet:$wallet,database:$database,
    restart_replay_identical:true,inventory_authority:"verified_by_trnm_campaign_e2e"}'
