#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

EXECUTION_BASE_URL="${EXECUTION_BASE_URL:-http://127.0.0.1:7003}"
EXECUTION_ADMIN_TOKEN="${EXECUTION_ADMIN_TOKEN:-${CEX_EXECUTION_ADMIN_TOKEN:-${LOCAL_DEV_ADMIN_TOKEN:-local-dev-admin-token}}}"

cex_require_cmd bash curl jq

failures=0

section() {
  printf '\n==> %s\n' "$*"
}

fail() {
  failures=$((failures + 1))
  printf 'FAIL %s\n' "$*" >&2
}

pass() {
  printf 'OK %s\n' "$*"
}

section 'runtime health/status'
if bash "$SCRIPT_DIR/runtime-manager-linux.sh" status; then
  pass 'runtime status'
else
  fail 'runtime status'
fi

section 'native metrics smoke'
if bash "$SCRIPT_DIR/smoke-runtime-metrics.sh"; then
  pass 'native metrics smoke'
else
  fail 'native metrics smoke'
fi

section 'operator signals'
operator_json_file="$(mktemp)"
operator_status=0
bash "$SCRIPT_DIR/check-operator-signals.sh" --compact >"$operator_json_file" || operator_status=$?
operator_overall="$(jq -r '.overall // "unknown"' "$operator_json_file")"
operator_warns="$(jq -r '.summary.warn_count // 0' "$operator_json_file")"
operator_criticals="$(jq -r '.summary.critical_count // 0' "$operator_json_file")"
printf 'operator overall=%s warn=%s critical=%s exit=%s\n' \
  "$operator_overall" "$operator_warns" "$operator_criticals" "$operator_status"
if [[ "$operator_status" -eq 0 && "$operator_overall" == "ok" ]]; then
  pass 'operator signals clear'
else
  fail "operator signals not clear (overall=$operator_overall, exit=$operator_status)"
  jq -r '.alerts[]? | "  - \(.severity // "unknown") \(.service):\(.name) value=\(.value // "n/a") threshold=\(.threshold // "n/a")"' "$operator_json_file" >&2
fi
rm -f "$operator_json_file"

section 'provider dead-letter readiness'
dead_letters_json_file="$(mktemp)"
if curl -fsS -H "x-admin-token: $EXECUTION_ADMIN_TOKEN" \
  "$EXECUTION_BASE_URL/v1/executions/provider-dead-letters?limit=200" >"$dead_letters_json_file"; then
  dead_letter_count="$(jq 'length' "$dead_letters_json_file")"
  billing_count="$(jq '[.[] | select(.provider_failure_kind == "billing")] | length' "$dead_letters_json_file")"
  retry_exhausted_count="$(jq '[.[] | select(.retry_budget_exhausted == true)] | length' "$dead_letters_json_file")"
  printf 'provider dead_letters=%s billing=%s retry_budget_exhausted=%s\n' \
    "$dead_letter_count" "$billing_count" "$retry_exhausted_count"
  if [[ "$dead_letter_count" -eq 0 ]]; then
    pass 'provider dead-letter queue clear'
  else
    fail 'provider dead-letter queue is not clear'
    jq -r '.[:10][] | "  - \(.provider_failure_kind) \(.dead_letter_reason) \(.execution_id) \(.provider_target // "unknown") :: \(.error // "")"' "$dead_letters_json_file" >&2
  fi
else
  fail 'provider dead-letter endpoint unreachable'
fi
rm -f "$dead_letters_json_file"

section 'production readiness verdict'
if [[ "$failures" -eq 0 ]]; then
  echo 'READY production readiness smoke passed'
  exit 0
fi

echo "NOT_READY production readiness smoke found $failures blocker(s)" >&2
exit 2
