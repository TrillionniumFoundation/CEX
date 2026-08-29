#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

IDENTITY_BASE_URL="${IDENTITY_BASE_URL:-http://127.0.0.1:7001}"
LEDGER_BASE_URL="${LEDGER_BASE_URL:-http://127.0.0.1:7002}"
EXECUTION_BASE_URL="${EXECUTION_BASE_URL:-http://127.0.0.1:7003}"
AUDIT_BASE_URL="${AUDIT_BASE_URL:-http://127.0.0.1:7004}"
CAPABILITY_BASE_URL="${CAPABILITY_BASE_URL:-http://127.0.0.1:7005}"
GATEWAY_BASE_URL="${GATEWAY_BASE_URL:-http://127.0.0.1:8080}"
CONSUMER_ENTRY_BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
MATRIX_ENTRY_BASE_URL="${MATRIX_ENTRY_BASE_URL:-http://127.0.0.1:8091}"
CEX_ENABLE_ENTRY_SERVICES="${CEX_ENABLE_ENTRY_SERVICES:-1}"

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

check_metric identity "$IDENTITY_BASE_URL/metrics" '^cex_identity_service_up 1$'
check_metric ledger "$LEDGER_BASE_URL/metrics" '^cex_ledger_service_up 1$'
check_metric execution "$EXECUTION_BASE_URL/metrics" '^cex_execution_runtime_up 1$'
check_metric execution "$EXECUTION_BASE_URL/metrics" '^cex_execution_operator_signal_active\{name="provider_dead_letters"\} [01]$'
check_metric audit "$AUDIT_BASE_URL/metrics" '^cex_audit_service_up 1$'
check_metric capability "$CAPABILITY_BASE_URL/metrics" '^cex_capability_service_up 1$'
check_metric gateway "$GATEWAY_BASE_URL/metrics" '^cex_gateway_runtime_counter_total\{name="invocation_create_requests"\} [0-9]+$'
check_metric gateway "$GATEWAY_BASE_URL/metrics" '^cex_gateway_operator_signal_active\{name="invocation_create_upstream_failures"\} [01]$'

if [[ "$CEX_ENABLE_ENTRY_SERVICES" == "1" ]]; then
  check_metric consumer-entry "$CONSUMER_ENTRY_BASE_URL/metrics" '^cex_consumer_entry_task_create_requests_total [0-9]+$'
  check_metric consumer-entry "$CONSUMER_ENTRY_BASE_URL/metrics" '^cex_consumer_entry_trillionnium_world_runtime_adapter_green 1$'
  check_metric matrix-entry "$MATRIX_ENTRY_BASE_URL/metrics" '^cex_matrix_entry_event_requests_total [0-9]+$'
fi
