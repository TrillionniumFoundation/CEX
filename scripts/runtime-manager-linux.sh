#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

PROJECT_ROOT="$CEX_PROJECT_ROOT"
OPENCLAW_SCOPE_ROOT_DEFAULT="$PROJECT_ROOT/run/openclaw-cex"
OPENCLAW_SCOPE_CONFIG_DEFAULT="$OPENCLAW_SCOPE_ROOT_DEFAULT/openclaw.json"
OPENCLAW_SCOPE_AGENT_DIR_DEFAULT="$OPENCLAW_SCOPE_ROOT_DEFAULT/agents/cex/agent"

if [[ -z "${OPENCLAW_STATE_DIR:-}" && -f "$OPENCLAW_SCOPE_CONFIG_DEFAULT" ]]; then
  export OPENCLAW_STATE_DIR="$OPENCLAW_SCOPE_ROOT_DEFAULT"
fi
if [[ -z "${OPENCLAW_CONFIG_PATH:-}" && -f "$OPENCLAW_SCOPE_CONFIG_DEFAULT" ]]; then
  export OPENCLAW_CONFIG_PATH="$OPENCLAW_SCOPE_CONFIG_DEFAULT"
fi
if [[ -z "${OPENCLAW_AGENT_DIR:-}" && -d "$OPENCLAW_SCOPE_AGENT_DIR_DEFAULT" ]]; then
  export OPENCLAW_AGENT_DIR="$OPENCLAW_SCOPE_AGENT_DIR_DEFAULT"
fi
if [[ -z "${CAPABILITY_OPENCLAW_MODELS_JSON_PATH:-}" && -n "${OPENCLAW_AGENT_DIR:-}" && -f "$OPENCLAW_AGENT_DIR/models.json" ]]; then
  export CAPABILITY_OPENCLAW_MODELS_JSON_PATH="$OPENCLAW_AGENT_DIR/models.json"
fi

export RUST_LOG="${RUST_LOG:-info}"
export APP_ENV="${APP_ENV:-dev}"
export GATEWAY_HOST="${GATEWAY_HOST:-127.0.0.1}"
export GATEWAY_PORT="${GATEWAY_PORT:-8080}"
export LEDGER_BASE_URL="${LEDGER_BASE_URL:-http://127.0.0.1:7002}"
export EXECUTION_BASE_URL="${EXECUTION_BASE_URL:-http://127.0.0.1:7003}"
export IDENTITY_BASE_URL="${IDENTITY_BASE_URL:-http://127.0.0.1:7001}"
export AUDIT_BASE_URL="${AUDIT_BASE_URL:-http://127.0.0.1:7004}"
export CAPABILITY_BASE_URL="${CAPABILITY_BASE_URL:-http://127.0.0.1:7005}"
export OLLAMA_BASE_URL="${OLLAMA_BASE_URL:-http://127.0.0.1:11434}"
export EXECUTION_CLAIM_LEASE_SECONDS="${EXECUTION_CLAIM_LEASE_SECONDS:-300}"
export EXECUTION_DEFAULT_MAX_ATTEMPTS="${EXECUTION_DEFAULT_MAX_ATTEMPTS:-1}"
export EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS="${EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS:-3}"
export IDENTITY_ADMIN_TOKEN="${IDENTITY_ADMIN_TOKEN:-local-dev-admin-token}"
export LEDGER_FAIL_FAST="${LEDGER_FAIL_FAST:-false}"
export DATABASE_URL="$(cex_effective_database_url)"
export REDIS_URL="${REDIS_URL:-redis://127.0.0.1:6379}"
export NATS_URL="${NATS_URL:-nats://127.0.0.1:4222}"

RUNTIME_DIR="${CEX_LINUX_RUNTIME_DIR:-$PROJECT_ROOT/run/linux-runtime}"
LOG_DIR="$RUNTIME_DIR/logs"
PID_DIR="$RUNTIME_DIR/pids"
mkdir -p "$LOG_DIR" "$PID_DIR"

SERVICES=(ledger-service execution-service identity-service audit-service capability-service gateway-service)
HEALTH_URLS=(
  "http://127.0.0.1:7002/health"
  "http://127.0.0.1:7003/health"
  "http://127.0.0.1:7001/health"
  "http://127.0.0.1:7004/health"
  "http://127.0.0.1:7005/health"
  "http://127.0.0.1:8080/health"
)

usage() {
  cat <<'EOF'
Usage: scripts/runtime-manager-linux.sh <start|stop|restart|status|logs>
EOF
}

ensure_binary() {
  local svc="$1"
  local bin="$PROJECT_ROOT/target/debug/$svc"
  if [[ -x "$bin" ]]; then
    return 0
  fi
  echo "==> building $svc"
  (cd "$PROJECT_ROOT" && cargo build -p "$svc" --bin "$svc")
}

stop_runtime() {
  local svc pidfile pid
  for svc in "${SERVICES[@]}"; do
    pidfile="$PID_DIR/$svc.pid"
    if [[ -f "$pidfile" ]]; then
      pid="$(cat "$pidfile" 2>/dev/null || true)"
      if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
      fi
      rm -f "$pidfile"
    fi
  done
  sleep 1
  for svc in "${SERVICES[@]}"; do
    pkill -f "$PROJECT_ROOT/target/debug/$svc" 2>/dev/null || true
  done
}

start_runtime() {
  local svc bin
  cex_require_cmd cargo curl
  for svc in "${SERVICES[@]}"; do
    ensure_binary "$svc"
  done

  for svc in "${SERVICES[@]}"; do
    bin="$PROJECT_ROOT/target/debug/$svc"
    echo "==> starting $svc"
    nohup "$bin" > "$LOG_DIR/$svc.log" 2>&1 &
    echo $! > "$PID_DIR/$svc.pid"
    sleep 0.3
  done
}

health_check() {
  local attempt url ok
  for attempt in $(seq 1 40); do
    ok=1
    for url in "${HEALTH_URLS[@]}"; do
      if ! curl -fsS "$url" >/dev/null 2>&1; then
        ok=0
        break
      fi
    done
    if [[ "$ok" -eq 1 ]]; then
      return 0
    fi
    sleep 1
  done
  echo "runtime health check failed" >&2
  return 1
}

status_runtime() {
  local url
  for url in "${HEALTH_URLS[@]}"; do
    if curl -fsS "$url" >/dev/null 2>&1; then
      echo "OK $url"
    else
      echo "FAIL $url"
    fi
  done
}

logs_runtime() {
  local svc
  for svc in "${SERVICES[@]}"; do
    echo "=== $svc ==="
    tail -n 40 "$LOG_DIR/$svc.log" 2>/dev/null || true
  done
}

cmd="${1:-}"
case "$cmd" in
  start)
    start_runtime
    health_check
    ;;
  stop)
    stop_runtime
    ;;
  restart)
    stop_runtime
    start_runtime
    health_check
    ;;
  status)
    status_runtime
    ;;
  logs)
    logs_runtime
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage >&2
    exit 64
    ;;
esac
