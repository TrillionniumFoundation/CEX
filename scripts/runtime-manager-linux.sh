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

cex_first_token_from_json_env() {
  local env_name="$1"
  python3 - "$env_name" <<'PY'
import json
import os
import sys

raw = os.environ.get(sys.argv[1], "").strip()
if not raw:
    raise SystemExit(0)
try:
    parsed = json.loads(raw)
except Exception:
    raise SystemExit(0)
items = parsed if isinstance(parsed, list) else [parsed]
for item in items:
    if isinstance(item, dict) and str(item.get("token") or "").strip():
        print(str(item["token"]).strip())
        break
PY
}

if [[ -z "${IDENTITY_ADMIN_TOKEN:-}" ]]; then
  CEX_FIRST_IDENTITY_ADMIN_TOKEN="$(cex_first_token_from_json_env IDENTITY_ADMIN_TOKENS_JSON || true)"
  if [[ -n "$CEX_FIRST_IDENTITY_ADMIN_TOKEN" ]]; then
    export IDENTITY_ADMIN_TOKEN="$CEX_FIRST_IDENTITY_ADMIN_TOKEN"
  fi
fi
if [[ -z "${LEDGER_ADMIN_TOKEN:-}" ]]; then
  CEX_FIRST_LEDGER_ADMIN_TOKEN="$(cex_first_token_from_json_env LEDGER_ADMIN_TOKENS_JSON || true)"
  if [[ -n "$CEX_FIRST_LEDGER_ADMIN_TOKEN" ]]; then
    export LEDGER_ADMIN_TOKEN="$CEX_FIRST_LEDGER_ADMIN_TOKEN"
  fi
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
export CONSUMER_ENTRY_BIND_ADDR="${CONSUMER_ENTRY_BIND_ADDR:-127.0.0.1:8090}"
export MATRIX_ENTRY_ADAPTER_BIND_ADDR="${MATRIX_ENTRY_ADAPTER_BIND_ADDR:-127.0.0.1:8091}"
export CONSUMER_ENTRY_BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
export CEX_GATEWAY_BASE_URL="${CEX_GATEWAY_BASE_URL:-http://127.0.0.1:8080}"
export CEX_GATEWAY_API_KEY="${CEX_GATEWAY_API_KEY:-local-dev-key}"
export CONSUMER_ENTRY_API_KEY="${CONSUMER_ENTRY_API_KEY:-local-dev-key}"
export CONSUMER_ENTRY_LEAGUE_WEB_SESSION_REQUIRED="${CONSUMER_ENTRY_LEAGUE_WEB_SESSION_REQUIRED:-true}"
export CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET="${CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET:-local-dev-league-web-session-secret}"
export OLLAMA_BASE_URL="${OLLAMA_BASE_URL:-http://127.0.0.1:11434}"
export EXECUTION_CLAIM_LEASE_SECONDS="${EXECUTION_CLAIM_LEASE_SECONDS:-300}"
export EXECUTION_DEFAULT_MAX_ATTEMPTS="${EXECUTION_DEFAULT_MAX_ATTEMPTS:-1}"
export EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS="${EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS:-3}"
export EXECUTION_PROVIDER_DISPATCH_TIMEOUT_SECONDS="${EXECUTION_PROVIDER_DISPATCH_TIMEOUT_SECONDS:-60}"
export CEX_ENABLE_QUEUED_WORKER="${CEX_ENABLE_QUEUED_WORKER:-1}"
export CEX_ENABLE_ENTRY_SERVICES="${CEX_ENABLE_ENTRY_SERVICES:-1}"
export CEX_RUNTIME_SKIP_BUILD="${CEX_RUNTIME_SKIP_BUILD:-0}"
export EXECUTION_WORKER_ID="${EXECUTION_WORKER_ID:-cex-linux-worker}"
export EXECUTION_WORKER_IDLE_SECS="${EXECUTION_WORKER_IDLE_SECS:-2}"
export IDENTITY_ADMIN_TOKEN="${IDENTITY_ADMIN_TOKEN:-local-dev-admin-token}"
export LEDGER_ADMIN_TOKEN="${LEDGER_ADMIN_TOKEN:-local-dev-admin-token}"
export LEDGER_FAIL_FAST="${LEDGER_FAIL_FAST:-false}"
export DATABASE_URL="$(cex_effective_database_url)"
export REDIS_URL="${REDIS_URL:-redis://127.0.0.1:6379}"
export NATS_URL="${NATS_URL:-nats://127.0.0.1:4222}"

RUNTIME_DIR="${CEX_LINUX_RUNTIME_DIR:-$PROJECT_ROOT/run/linux-runtime}"
LOG_DIR="$RUNTIME_DIR/logs"
PID_DIR="$RUNTIME_DIR/pids"
ENTRY_CONFIG_DIR="$RUNTIME_DIR/entry-config"
mkdir -p "$LOG_DIR" "$PID_DIR"

SERVICES=(ledger-service execution-service identity-service audit-service capability-service gateway-service)
if [[ "$CEX_ENABLE_ENTRY_SERVICES" == "1" ]]; then
  SERVICES+=(consumer-entry-api matrix-entry-adapter)
fi
WORKER_NAME="execution-queued-worker"
WORKER_SCRIPT="$SCRIPT_DIR/execution-queued-worker.sh"
HEALTH_URLS=(
  "http://127.0.0.1:7002/health"
  "http://127.0.0.1:7003/health"
  "http://127.0.0.1:7001/health"
  "http://127.0.0.1:7004/health"
  "http://127.0.0.1:7005/health"
  "http://127.0.0.1:8080/health"
)
if [[ "$CEX_ENABLE_ENTRY_SERVICES" == "1" ]]; then
  HEALTH_URLS+=(
    "http://127.0.0.1:8090/health"
    "http://127.0.0.1:8091/health"
  )
fi

usage() {
  cat <<'EOF'
Usage: scripts/runtime-manager-linux.sh <start|stop|restart|status|logs>
EOF
}

ensure_entry_runtime_config() {
  if [[ "$CEX_ENABLE_ENTRY_SERVICES" != "1" ]]; then
    return 0
  fi

  mkdir -p "$ENTRY_CONFIG_DIR"
  local bindings_path registry_path approvals_path audit_path league_state_path league_sql_snapshot_path matrix_recent_event_store_path matrix_rate_limit_store_path
  bindings_path="${CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH:-$ENTRY_CONFIG_DIR/identity-bindings.json}"
  registry_path="${CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH:-$ENTRY_CONFIG_DIR/identity-registry.json}"
  approvals_path="${CONSUMER_ENTRY_IDENTITY_BINDING_APPROVED_REVISIONS_PATH:-$ENTRY_CONFIG_DIR/identity-approved-revisions.json}"
  audit_path="${CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH:-$ENTRY_CONFIG_DIR/identity-binding-audit.jsonl}"
  league_state_path="${CONSUMER_ENTRY_LEAGUE_STATE_PATH:-$ENTRY_CONFIG_DIR/league-state.json}"
  league_sql_snapshot_path="${CONSUMER_ENTRY_LEAGUE_SQL_SNAPSHOT_PATH:-$ENTRY_CONFIG_DIR/league-state-snapshot.sql}"
  matrix_recent_event_store_path="${MATRIX_ENTRY_RECENT_EVENT_STORE_PATH:-$ENTRY_CONFIG_DIR/matrix-entry-recent-events.json}"
  matrix_rate_limit_store_path="${MATRIX_ENTRY_RATE_LIMIT_STORE_PATH:-$ENTRY_CONFIG_DIR/matrix-entry-rate-limit.json}"

  if [[ ! -f "$bindings_path" ]]; then
    cat > "$bindings_path" <<'JSON'
{
  "version": 1,
  "revision": "local-dev-entry-bindings-v1",
  "chat_users": {
    "local-dev-chat-user": {
      "product_user_id": "pu-local-dev"
    }
  },
  "matrix_users": {
    "@alice:local.dev": {
      "product_user_id": "pu-local-dev"
    },
    "@cex-bot:local.dev": {
      "product_user_id": "pu-local-bot"
    }
  }
}
JSON
  fi

  if [[ ! -f "$registry_path" ]]; then
    cat > "$registry_path" <<'JSON'
{
  "version": 1,
  "revision": "local-dev-entry-registry-v1",
  "product_users": {
    "pu-local-dev": {
      "org_id": "00000000-0000-0000-0000-00000000ce01",
      "account_id": "00000000-0000-0000-0000-00000000ce31",
      "status": "active"
    },
    "pu-local-bot": {
      "org_id": "00000000-0000-0000-0000-00000000ce01",
      "account_id": "00000000-0000-0000-0000-00000000ce31",
      "status": "active"
    }
  }
}
JSON
  fi

  if [[ ! -f "$approvals_path" ]]; then
    cat > "$approvals_path" <<'JSON'
{
  "version": 1,
  "revision": "local-dev-entry-approval-v1",
  "approved_revisions": [
    "binding:local-dev-entry-bindings-v1|registry:local-dev-entry-registry-v1"
  ]
}
JSON
  fi
  touch "$audit_path"

  export CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH="$bindings_path"
  export CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH="$registry_path"
  export CONSUMER_ENTRY_IDENTITY_BINDING_APPROVED_REVISIONS_PATH="$approvals_path"
  export CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH="$audit_path"
  export CONSUMER_ENTRY_LEAGUE_STATE_PATH="$league_state_path"
  export CONSUMER_ENTRY_LEAGUE_SQL_SNAPSHOT_PATH="$league_sql_snapshot_path"
  export MATRIX_ENTRY_RECENT_EVENT_STORE_PATH="$matrix_recent_event_store_path"
  export MATRIX_ENTRY_RATE_LIMIT_STORE_PATH="$matrix_rate_limit_store_path"
  export CONSUMER_ENTRY_INGRESS_TOKEN="${CONSUMER_ENTRY_INGRESS_TOKEN:-local-dev-entry-ingress-token}"
  export MATRIX_ENTRY_INGRESS_TOKEN="${MATRIX_ENTRY_INGRESS_TOKEN:-local-dev-matrix-entry-ingress-token}"
  export CONSUMER_ENTRY_REQUIRE_SESSION_AUTH="${CONSUMER_ENTRY_REQUIRE_SESSION_AUTH:-true}"
  export CONSUMER_ENTRY_SESSION_AUTH_SECRET="${CONSUMER_ENTRY_SESSION_AUTH_SECRET:-local-dev-consumer-session-auth-secret}"
  export MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET="${MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET:-$CONSUMER_ENTRY_SESSION_AUTH_SECRET}"
  export CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS="${CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS:-matrix-entry-adapter}"
  export CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE="${CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE:-consumer-entry-api}"
  export CONSUMER_ENTRY_REPLAY_STORE_PATH="${CONSUMER_ENTRY_REPLAY_STORE_PATH:-$ENTRY_CONFIG_DIR/consumer-entry-replay.json}"
  export CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH="${CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH:-$ENTRY_CONFIG_DIR/consumer-entry-rate-limit.json}"
  export CONSUMER_ENTRY_RATE_LIMIT_USER_MAX_REQUESTS="${CONSUMER_ENTRY_RATE_LIMIT_USER_MAX_REQUESTS:-20}"
  export CONSUMER_ENTRY_RATE_LIMIT_ROOM_MAX_REQUESTS="${CONSUMER_ENTRY_RATE_LIMIT_ROOM_MAX_REQUESTS:-80}"
  export CONSUMER_ENTRY_RATE_LIMIT_SESSION_MAX_REQUESTS="${CONSUMER_ENTRY_RATE_LIMIT_SESSION_MAX_REQUESTS:-30}"
  export CONSUMER_ENTRY_RATE_LIMIT_ORG_MAX_REQUESTS="${CONSUMER_ENTRY_RATE_LIMIT_ORG_MAX_REQUESTS:-200}"
  export CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL="${CONSUMER_ENTRY_LEAGUE_NORMALIZED_DATABASE_URL:-$DATABASE_URL}"
  export CONSUMER_ENTRY_LEAGUE_NORMALIZED_DUAL_WRITE_ENABLED="${CONSUMER_ENTRY_LEAGUE_NORMALIZED_DUAL_WRITE_ENABLED:-true}"
  export CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED="${CONSUMER_ENTRY_LEAGUE_NORMALIZED_READ_SWITCH_ENABLED:-true}"
  export CONSUMER_ENTRY_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED="${CONSUMER_ENTRY_LEAGUE_NORMALIZED_FINAL_CUTOVER_ENABLED:-true}"
  export CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING="${CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING:-true}"
  export CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_REVISION="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_REVISION:-true}"
  export CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_APPROVED_REVISION="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_APPROVED_REVISION:-true}"
  export CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR:-true}"
  export CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS:-cex-runtime-manager}"
}

stop_worker() {
  local pidfile pid
  pidfile="$PID_DIR/$WORKER_NAME.pid"
  if [[ -f "$pidfile" ]]; then
    pid="$(cat "$pidfile" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
    fi
    rm -f "$pidfile"
  fi
  pkill -f "$WORKER_SCRIPT run" 2>/dev/null || true
}

start_worker() {
  if [[ "$CEX_ENABLE_QUEUED_WORKER" != "1" ]]; then
    echo "==> queued worker disabled (CEX_ENABLE_QUEUED_WORKER=$CEX_ENABLE_QUEUED_WORKER)"
    return 0
  fi
  cex_require_cmd python3
  echo "==> starting $WORKER_NAME"
  nohup bash "$WORKER_SCRIPT" run > "$LOG_DIR/$WORKER_NAME.log" 2>&1 &
  echo $! > "$PID_DIR/$WORKER_NAME.pid"
}

ensure_binary() {
  local svc="$1"
  local bin="$PROJECT_ROOT/target/debug/$svc"
  if [[ "$CEX_RUNTIME_SKIP_BUILD" == "1" && -x "$bin" ]]; then
    return 0
  fi
  echo "==> building $svc"
  (cd "$PROJECT_ROOT" && cargo build -p "$svc" --bin "$svc")
}

stop_runtime() {
  local svc pidfile pid
  stop_worker
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
  ensure_entry_runtime_config
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

  if [[ "$CEX_ENABLE_QUEUED_WORKER" == "1" ]]; then
    local pidfile pid
    pidfile="$PID_DIR/$WORKER_NAME.pid"
    pid="$(cat "$pidfile" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      echo "OK worker $WORKER_NAME pid=$pid id=$EXECUTION_WORKER_ID"
    else
      echo "FAIL worker $WORKER_NAME"
    fi
  else
    echo "DISABLED worker $WORKER_NAME"
  fi
}

logs_runtime() {
  local svc
  for svc in "${SERVICES[@]}"; do
    echo "=== $svc ==="
    tail -n 40 "$LOG_DIR/$svc.log" 2>/dev/null || true
  done
  echo "=== $WORKER_NAME ==="
  tail -n 40 "$LOG_DIR/$WORKER_NAME.log" 2>/dev/null || true
}

cmd="${1:-}"
case "$cmd" in
  start)
    start_runtime
    health_check
    start_worker
    ;;
  stop)
    stop_runtime
    ;;
  restart)
    stop_runtime
    start_runtime
    health_check
    start_worker
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
