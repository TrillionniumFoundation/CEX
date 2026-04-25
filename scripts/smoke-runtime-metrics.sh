#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

GATEWAY_BASE_URL="${GATEWAY_BASE_URL:-http://127.0.0.1:8080}"
EXECUTION_BASE_URL="${EXECUTION_BASE_URL:-http://127.0.0.1:7003}"

cex_require_cmd curl grep

check_metric() {
  local service="$1"
  local url="$2"
  local pattern="$3"
  local body
  body="$(curl -fsS "$url")"
  if ! grep -Eq "$pattern" <<<"$body"; then
    echo "FAIL $service metrics missing pattern: $pattern" >&2
    return 1
  fi
  echo "OK $service metrics $url"
}

check_metric execution "$EXECUTION_BASE_URL/metrics" '^cex_execution_runtime_up 1$'
check_metric execution "$EXECUTION_BASE_URL/metrics" '^cex_execution_operator_signal_active\{name="provider_dead_letters"\} [01]$'
check_metric gateway "$GATEWAY_BASE_URL/metrics" '^cex_gateway_runtime_counter_total\{name="invocation_create_requests"\} [0-9]+$'
check_metric gateway "$GATEWAY_BASE_URL/metrics" '^cex_gateway_operator_signal_active\{name="invocation_create_upstream_failures"\} [01]$'
