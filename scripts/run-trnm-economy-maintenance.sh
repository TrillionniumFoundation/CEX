#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

LEDGER_URL="${LEDGER_BASE_URL:-http://127.0.0.1:7002}"
CONSUMER_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
ADMIN_TOKEN="${LEDGER_ADMIN_TOKEN:-${IDENTITY_ADMIN_TOKEN:?ledger admin token required}}"
ENTRY_TOKEN="${CONSUMER_ENTRY_INGRESS_TOKEN:-$ADMIN_TOKEN}"
REPORT_DIR="$CEX_PROJECT_ROOT/run/trnm-economy"
mkdir -p "$REPORT_DIR"

for _ in $(seq 1 60); do
  if curl -fsS "$LEDGER_URL/v1/trnm/economy/readiness" >/dev/null 2>&1 \
    && curl -fsS "$CONSUMER_URL/health" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

maintenance="$(curl -fsS "$LEDGER_URL/v1/trnm/economy/maintenance" \
  -H "x-admin-token: $ADMIN_TOKEN" -H 'content-type: application/json' --data-binary '{}')"
projection="$(curl -fsS "$CONSUMER_URL/v1/trillionnium/economy/projection/rebuild" \
  -H "x-entry-token: $ENTRY_TOKEN" -H 'content-type: application/json' --data-binary '{}')"

jq -e '.overdue_seller_holds == 0 and .alert == false' <<<"$maintenance" >/dev/null
jq -e '.rebuilt == true and .authoritative_receipts == .projected_receipts' \
  <<<"$projection" >/dev/null
jq -n --argjson maintenance "$maintenance" --argjson projection "$projection" \
  '{status:"ok",maintenance:$maintenance,projection:$projection}' \
  >"$REPORT_DIR/maintenance-latest.json"
