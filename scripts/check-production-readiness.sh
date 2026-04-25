#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

EXECUTION_BASE_URL="${EXECUTION_BASE_URL:-http://127.0.0.1:7003}"
EXECUTION_ADMIN_TOKEN="${EXECUTION_ADMIN_TOKEN:-${CEX_EXECUTION_ADMIN_TOKEN:-${LOCAL_DEV_ADMIN_TOKEN:-local-dev-admin-token}}}"
CEX_PROVIDER_PROBE_REQUIRED="${CEX_PROVIDER_PROBE_REQUIRED:-1}"
CEX_PROVIDER_PROBE_MODEL="${CEX_PROVIDER_PROBE_MODEL:-}"
CEX_READINESS_MODE="${CEX_READINESS_MODE:-production}"

case "$CEX_READINESS_MODE" in
  local|production) ;;
  *)
    echo "invalid CEX_READINESS_MODE: $CEX_READINESS_MODE (expected local|production)" >&2
    exit 64
    ;;
esac

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

secret_is_default_or_empty() {
  local value="$1"
  [[ -z "$value" || "$value" == "local-dev-key" || "$value" == "local-dev-admin-token" ]]
}

required_bool_true() {
  local value="$1"
  [[ "${value,,}" == "true" || "$value" == "1" ]]
}

section 'deployment posture'
printf 'readiness mode=%s\n' "$CEX_READINESS_MODE"
if [[ "$CEX_READINESS_MODE" == "local" ]]; then
  pass 'local readiness posture selected (production secret/profile checks skipped)'
else
  posture_failures=0
  if secret_is_default_or_empty "${CEX_GATEWAY_API_KEY:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires non-default CEX_GATEWAY_API_KEY'
  fi
  if secret_is_default_or_empty "${EXECUTION_ADMIN_TOKEN:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires non-default execution admin token'
  fi
  if [[ -z "${CONSUMER_ENTRY_INGRESS_TOKEN:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires CONSUMER_ENTRY_INGRESS_TOKEN'
  fi
  if [[ -z "${MATRIX_ENTRY_INGRESS_TOKEN:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires MATRIX_ENTRY_INGRESS_TOKEN'
  fi
  if ! required_bool_true "${CONSUMER_ENTRY_REQUIRE_SESSION_AUTH:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires CONSUMER_ENTRY_REQUIRE_SESSION_AUTH=true'
  fi
  if ! required_bool_true "${CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING:-}"; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING=true'
  fi
  if [[ -z "${CONSUMER_ENTRY_REPLAY_STORE_PATH:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires durable CONSUMER_ENTRY_REPLAY_STORE_PATH'
  fi
  if [[ -z "${CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires durable CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH'
  fi
  if [[ -z "${MATRIX_ENTRY_RECENT_EVENT_STORE_PATH:-}" ]]; then
    posture_failures=$((posture_failures + 1))
    fail 'production posture requires durable MATRIX_ENTRY_RECENT_EVENT_STORE_PATH'
  fi

  if [[ "$posture_failures" -eq 0 ]]; then
    pass 'production posture checks clear'
  fi
fi

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

section 'live provider probe'
if [[ "$CEX_PROVIDER_PROBE_REQUIRED" == "0" ]]; then
  pass 'live provider probe not required by environment'
elif [[ -z "$CEX_PROVIDER_PROBE_MODEL" ]]; then
  fail 'live provider probe model is not configured (set CEX_PROVIDER_PROBE_MODEL)'
else
  provider_probe_json_file="$(mktemp)"
  provider_probe_status=0
  bash "$SCRIPT_DIR/probe-openclaw-provider.sh" --model "$CEX_PROVIDER_PROBE_MODEL" --compact \
    >"$provider_probe_json_file" || provider_probe_status=$?
  provider_probe_ok="$(jq -r '.ok // false' "$provider_probe_json_file")"
  provider_probe_status_text="$(jq -r '.status // "unknown"' "$provider_probe_json_file")"
  if [[ "$provider_probe_status" -eq 0 && "$provider_probe_ok" == "true" ]]; then
    pass "live provider probe succeeded ($CEX_PROVIDER_PROBE_MODEL)"
  else
    provider_probe_error="$(jq -r '.error // "unknown provider probe error"' "$provider_probe_json_file")"
    fail "live provider probe failed ($CEX_PROVIDER_PROBE_MODEL status=$provider_probe_status_text): $provider_probe_error"
  fi
  rm -f "$provider_probe_json_file"
fi

section 'production readiness verdict'
if [[ "$failures" -eq 0 ]]; then
  echo "READY $CEX_READINESS_MODE readiness smoke passed"
  exit 0
fi

echo "NOT_READY $CEX_READINESS_MODE readiness smoke found $failures blocker(s)" >&2
exit 2
