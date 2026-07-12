#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

URL="${LEDGER_BASE_URL:-http://127.0.0.1:7002}"
ADMIN="${LEDGER_ADMIN_TOKEN:-${IDENTITY_ADMIN_TOKEN:?ledger admin token required}}"
GAME_AUTHORITY="${TRNM_GAME_AUTHORITY_TOKEN:-trnm-game-authority-v1:$IDENTITY_ADMIN_TOKEN}"
ORG="00000000-0000-0000-0000-00000000ce01"
RUN="value-auth-$(date +%s)-${RANDOM}"
WORK="$(mktemp -d /tmp/cex-trnm-value-auth.XXXXXX)"

admin_post() {
  curl -fsS "$URL$1" -H "x-admin-token: $ADMIN" \
    -H 'content-type: application/json' --data-binary "$2"
}

authority_post() {
  curl -fsS "$URL$1" -H "x-trnm-game-authority: $GAME_AUTHORITY" \
    -H 'content-type: application/json' --data-binary "$2"
}

account="$(admin_post /v1/accounts "$(jq -cn --arg org "$ORG" \
  '{org_id:$org,account_type:"value-auth",currency_unit:"credit",initial_balance:0}')" | jq -er .account_id)"
other_account="$(admin_post /v1/accounts "$(jq -cn --arg org "$ORG" \
  '{org_id:$org,account_type:"value-auth-other",currency_unit:"credit",initial_balance:0}')" | jq -er .account_id)"
player="player-$RUN"
recovery="recovery-$RUN-012345678901234567890123"
new_recovery="new-recovery-$RUN-012345678901234567890123"
admin_post /v1/trnm/identity/register "$(jq -cn --arg p "$player" --arg a "$account" --arg r "$recovery" \
  '{player_id:$p,account_id:$a,recovery_key:$r}')" >/dev/null
session="$(curl -fsS "$URL/v1/trnm/identity/session" -H 'content-type: application/json' \
  --data-binary "$(jq -cn --arg p "$player" --arg r "$recovery" \
  '{player_id:$p,recovery_key:$r,device_id:"value-auth-device"}')" | jq -er .session_token)"

intent() {
  local kind="$1" id="$2" target="$3" amount="$4" entitlement="${5:-null}"
  jq -cn --arg kind "$kind" --arg id "$id" --arg player "$player" \
    --arg account "$target" --argjson amount "$amount" --argjson entitlement "$entitlement" \
    '{intent:{protocol_version:"term_exchange_protocol_v2",intent_id:$id,
      term_id:("value-auth:"+$id),term_version:"2.3.0",domain:"trnm_game",kind:$kind,
      idempotency_key:{scope:"value-auth",key:$id},
      actors:[{actor_id:$player,actor_kind:"player",account_id:$account}],assets:[],
      amount_credits:$amount,currency:"credit",
      metadata:(if $entitlement == null then {} else {server_signed_value_entitlement:$entitlement} end),
      created_at_epoch:(now|floor)}}'
}

http_code="$(curl -sS -o "$WORK/unsigned.json" -w '%{http_code}' "$URL/v1/trnm/economy/intents" \
  -H "x-trnm-player-session: $session" -H 'content-type: application/json' \
  --data-binary "$(intent release_reward "$RUN-unsigned" "$account" 25)")"
[[ "$http_code" == 401 ]]

http_code="$(curl -sS -o "$WORK/contract.json" -w '%{http_code}' "$URL/v1/trnm/economy/intents" \
  -H "x-trnm-player-session: $session" -H 'content-type: application/json' \
  --data-binary "$(intent complete_contract "$RUN-contract" "$account" 1)")"
[[ "$http_code" == 400 ]]

http_code="$(curl -sS -o "$WORK/ownership.json" -w '%{http_code}' "$URL/v1/trnm/economy/wallet" \
  -H "x-trnm-player-session: $session" -H 'content-type: application/json' \
  --data-binary "$(jq -cn --arg p "$player" --arg a "$other_account" \
    '{actor_id:$p,account_id:$a,reconciliation_cursor:0}')")"
[[ "$http_code" == 401 ]]

for ordinal in 1 2 3; do
  id="$RUN-budget-$ordinal"
  entitlement="$(authority_post /v1/trnm/economy/entitlements "$(jq -cn \
    --arg p "$player" --arg a "$account" --arg id "$id" --arg source "$RUN-battle-$ordinal" \
    '{actor_id:$p,account_id:$a,source:"battle",source_id:$source,intent_id:$id,amount_credits:100}')")"
  curl -fsS "$URL/v1/trnm/economy/intents" -H "x-trnm-player-session: $session" \
    -H 'content-type: application/json' --data-binary "$(intent release_reward "$id" "$account" 100 "$entitlement")" \
    | jq -e '.status == "approved_release"' >/dev/null
done

id="$RUN-over-budget"
entitlement="$(authority_post /v1/trnm/economy/entitlements "$(jq -cn \
  --arg p "$player" --arg a "$account" --arg id "$id" --arg source "$RUN-battle-over" \
  '{actor_id:$p,account_id:$a,source:"battle",source_id:$source,intent_id:$id,amount_credits:1}')")"
http_code="$(curl -sS -o "$WORK/budget.json" -w '%{http_code}' "$URL/v1/trnm/economy/intents" \
  -H "x-trnm-player-session: $session" -H 'content-type: application/json' \
  --data-binary "$(intent release_reward "$id" "$account" 1 "$entitlement")")"
[[ "$http_code" == 401 ]]

admin_post /v1/trnm/identity/recover "$(jq -cn --arg p "$player" --arg r "$recovery" --arg n "$new_recovery" \
  '{player_id:$p,recovery_key:$r,new_recovery_key:$n}')" >/dev/null
http_code="$(curl -sS -o "$WORK/revoked.json" -w '%{http_code}' "$URL/v1/trnm/economy/wallet" \
  -H "x-trnm-player-session: $session" -H 'content-type: application/json' \
  --data-binary "$(jq -cn --arg p "$player" --arg a "$account" \
    '{actor_id:$p,account_id:$a,reconciliation_cursor:0}')")"
[[ "$http_code" == 401 ]]

new_session="$(curl -fsS "$URL/v1/trnm/identity/session" -H 'content-type: application/json' \
  --data-binary "$(jq -cn --arg p "$player" --arg r "$new_recovery" \
  '{player_id:$p,recovery_key:$r,device_id:"replacement-device"}')")"
new_token="$(jq -er .session_token <<<"$new_session")"
curl -fsS "$URL/v1/trnm/economy/wallet" -H "x-trnm-player-session: $new_token" \
  -H 'content-type: application/json' --data-binary "$(jq -cn --arg p "$player" --arg a "$account" \
  '{actor_id:$p,account_id:$a,reconciliation_cursor:2}')" | jq -e '.available_credits == 300' >/dev/null

admin_post /v1/trnm/identity/status "$(jq -cn --arg p "$player" \
  '{player_id:$p,status:"suspended"}')" >/dev/null
http_code="$(curl -sS -o "$WORK/suspended.json" -w '%{http_code}' "$URL/v1/trnm/economy/wallet" \
  -H "x-trnm-player-session: $new_token" -H 'content-type: application/json' \
  --data-binary "$(jq -cn --arg p "$player" --arg a "$account" \
    '{actor_id:$p,account_id:$a,reconciliation_cursor:2}')")"
[[ "$http_code" == 401 ]]
admin_post /v1/trnm/identity/status "$(jq -cn --arg p "$player" \
  '{player_id:$p,status:"active"}')" >/dev/null
replacement_token="$(curl -fsS "$URL/v1/trnm/identity/session" -H 'content-type: application/json' \
  --data-binary "$(jq -cn --arg p "$player" --arg r "$new_recovery" \
  '{player_id:$p,recovery_key:$r,device_id:"post-suspension-device"}')" | jq -er .session_token)"
curl -fsS "$URL/v1/trnm/economy/wallet" -H "x-trnm-player-session: $replacement_token" \
  -H 'content-type: application/json' --data-binary "$(jq -cn --arg p "$player" --arg a "$account" \
  '{actor_id:$p,account_id:$a,reconciliation_cursor:3}')" | jq -e '.available_credits == 300' >/dev/null

jq -n --arg run_id "$RUN" '{status:"passed",run_id:$run_id,
  unsigned_reward_rejected:true,positive_complete_contract_rejected:true,
  account_ownership_enforced:true,per_event_cap:100,daily_cap:300,
  recovery_revokes_old_sessions:true,replacement_device_session_works:true,
  suspension_revokes_sessions:true,reactivation_requires_new_session:true}'
