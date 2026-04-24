#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DIR="$ROOT_DIR/run/matrix-bot-entry"
LOG_DIR="$ROOT_DIR/logs/matrix-bot-entry"
ENV_FILE="$ROOT_DIR/.env"

mkdir -p "$RUN_DIR" "$LOG_DIR"

if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  set -a
  while IFS='' read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    if [[ -z "$line" || "$line" == \#* ]]; then
      continue
    fi
    export "$line"
  done < "$ENV_FILE"
  set +a
fi

: "${CONSUMER_ENTRY_BIND_ADDR:=127.0.0.1:8090}"
: "${MATRIX_ENTRY_ADAPTER_BIND_ADDR:=127.0.0.1:8091}"
: "${MATRIX_BOT_RELAY_BIND_ADDR:=127.0.0.1:8092}"

: "${CONSUMER_ENTRY_BASE_URL:=http://127.0.0.1:8090}"
: "${MATRIX_ADAPTER_BASE_URL:=http://127.0.0.1:8091}"
: "${MATRIX_BOT_RELAY_BASE_URL:=http://127.0.0.1:8092}"

: "${MATRIX_POLL_HOMESERVER:=http://127.0.0.1:8008}"
: "${MATRIX_POLL_INTERVAL_MS:=3000}"
: "${MATRIX_POLL_MAX_RECENT_EVENT_IDS:=1000}"
: "${MATRIX_SYNC_STATE_FILE:=/tmp/matrix-bot-poller.state}"
: "${MATRIX_BOT_USER_ID:=@cex-bot:local.dev}"
: "${MATRIX_SYNC_FILTER:=}"
: "${MATRIX_RELAY_QUEUE_ENABLED:=true}"
: "${MATRIX_RELAY_QUEUE_PATH:=/tmp/matrix-bot-relay-queue.json}"
: "${MATRIX_HOMESERVER_BASE_URL:=http://127.0.0.1:8008}"
: "${MATRIX_RELAY_QUEUE_POLL_INTERVAL_MS:=4000}"
: "${MATRIX_RELAY_QUEUE_MAX_SIZE:=2000}"
: "${MATRIX_RELAY_SEND_MAX_ATTEMPTS:=3}"
: "${MATRIX_RELAY_SEND_INITIAL_DELAY_MS:=200}"
: "${MATRIX_RELAY_SEND_MAX_DELAY_MS:=2500}"
: "${MATRIX_RELAY_MAX_RECENT_EVENT_IDS:=1000}"

PACKAGE_LIST=(
  "consumer-entry-api"
  "matrix-entry-adapter"
  "matrix-bot-relay"
)

is_running() {
  local pkg="$1"
  local pid_file="$RUN_DIR/${pkg}.pid"

  if [[ ! -f "$pid_file" ]]; then
    return 1
  fi

  local pid
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  if [[ -z "$pid" ]]; then
    return 1
  fi

  if kill -0 "$pid" >/dev/null 2>&1; then
    return 0
  fi

  return 1
}

cleanup_pid_file() {
  local pkg="$1"
  rm -f "$RUN_DIR/${pkg}.pid" "$RUN_DIR/${pkg}.json"
}

start_service() {
  local pkg="$1"

  if is_running "$pkg"; then
    local pid
    pid="$(cat "$RUN_DIR/${pkg}.pid")"
    echo "[skip] ${pkg} already running (pid=${pid})"
    return
  fi

  cleanup_pid_file "$pkg"

  local log_file="$LOG_DIR/${pkg}.log"
  local pid_file="$RUN_DIR/${pkg}.pid"
  local pid=""

  case "$pkg" in
    consumer-entry-api)
      (
        cd "$ROOT_DIR"
        CEX_RUNTIME_PROFILE="${CEX_RUNTIME_PROFILE:-}" \
        CONSUMER_ENTRY_RUNTIME_PROFILE="${CONSUMER_ENTRY_RUNTIME_PROFILE:-}" \
        CONSUMER_ENTRY_BIND_ADDR="$CONSUMER_ENTRY_BIND_ADDR" \
        MATRIX_ENTRY_BASE_URL="$CONSUMER_ENTRY_BASE_URL" \
        CONSUMER_ENTRY_INGRESS_TOKEN="${CONSUMER_ENTRY_INGRESS_TOKEN:-}" \
        CONSUMER_ENTRY_REQUIRE_SESSION_AUTH="${CONSUMER_ENTRY_REQUIRE_SESSION_AUTH:-}" \
        CONSUMER_ENTRY_SESSION_AUTH_SECRET="${CONSUMER_ENTRY_SESSION_AUTH_SECRET:-}" \
        CONSUMER_ENTRY_SESSION_AUTH_ISSUER_SECRETS_JSON="${CONSUMER_ENTRY_SESSION_AUTH_ISSUER_SECRETS_JSON:-}" \
        CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON="${CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON:-}" \
        CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH="${CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH:-}" \
        CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH="${CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH:-}" \
        CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION="${CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION:-}" \
        CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS="${CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS:-}" \
        CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE="${CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE:-}" \
        CONSUMER_ENTRY_SESSION_AUTH_MAX_CLOCK_SKEW_SECS="${CONSUMER_ENTRY_SESSION_AUTH_MAX_CLOCK_SKEW_SECS:-}" \
        CONSUMER_ENTRY_SESSION_AUTH_MAX_TTL_SECS="${CONSUMER_ENTRY_SESSION_AUTH_MAX_TTL_SECS:-}" \
        CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH="${CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH:-}" \
        CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH="${CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH:-}" \
        CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH="${CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH:-}" \
        CONSUMER_ENTRY_IDENTITY_BINDING_APPROVED_REVISIONS_PATH="${CONSUMER_ENTRY_IDENTITY_BINDING_APPROVED_REVISIONS_PATH:-}" \
        CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_REVISION="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_REVISION:-}" \
        CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REJECT_SAME_REVISION="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REJECT_SAME_REVISION:-}" \
        CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOW_LEGACY_FORMAT="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOW_LEGACY_FORMAT:-}" \
        CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_APPROVED_REVISION="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_APPROVED_REVISION:-}" \
        CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOW_ROLLBACK="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOW_ROLLBACK:-}" \
        CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR:-}" \
        CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ACTOR_HEADER="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ACTOR_HEADER:-}" \
        CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS="${CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS:-}" \
        CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING="${CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING:-}" \
        CONSUMER_ENTRY_MAX_TEXT_CHARS="${CONSUMER_ENTRY_MAX_TEXT_CHARS:-}" \
        CONSUMER_ENTRY_RATE_LIMIT_USER_MAX_REQUESTS="${CONSUMER_ENTRY_RATE_LIMIT_USER_MAX_REQUESTS:-}" \
        CONSUMER_ENTRY_RATE_LIMIT_ROOM_MAX_REQUESTS="${CONSUMER_ENTRY_RATE_LIMIT_ROOM_MAX_REQUESTS:-}" \
        CONSUMER_ENTRY_RATE_LIMIT_SESSION_MAX_REQUESTS="${CONSUMER_ENTRY_RATE_LIMIT_SESSION_MAX_REQUESTS:-}" \
        CONSUMER_ENTRY_RATE_LIMIT_ORG_MAX_REQUESTS="${CONSUMER_ENTRY_RATE_LIMIT_ORG_MAX_REQUESTS:-}" \
        CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH="${CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH:-}" \
        CONSUMER_ENTRY_REPLAY_WINDOW_SECS="${CONSUMER_ENTRY_REPLAY_WINDOW_SECS:-}" \
        CONSUMER_ENTRY_REPLAY_CACHE_SIZE="${CONSUMER_ENTRY_REPLAY_CACHE_SIZE:-}" \
        CONSUMER_ENTRY_REPLAY_STORE_PATH="${CONSUMER_ENTRY_REPLAY_STORE_PATH:-}" \
        CONSUMER_ENTRY_MATRIX_EVENT_WINDOW_SECS="${CONSUMER_ENTRY_MATRIX_EVENT_WINDOW_SECS:-}" \
        CONSUMER_ENTRY_MATRIX_EVENT_CACHE_SIZE="${CONSUMER_ENTRY_MATRIX_EVENT_CACHE_SIZE:-}" \
        CONSUMER_ENTRY_MATRIX_EVENT_STORE_PATH="${CONSUMER_ENTRY_MATRIX_EVENT_STORE_PATH:-}" \
        nohup cargo run -p "$pkg" >>"$log_file" 2>&1 &
        echo $! > "$pid_file"
      )
      ;;
    matrix-entry-adapter)
      (
        cd "$ROOT_DIR"
        CEX_RUNTIME_PROFILE="${CEX_RUNTIME_PROFILE:-}" \
        MATRIX_ENTRY_RUNTIME_PROFILE="${MATRIX_ENTRY_RUNTIME_PROFILE:-}" \
        MATRIX_ENTRY_ADAPTER_BIND_ADDR="$MATRIX_ENTRY_ADAPTER_BIND_ADDR" \
        CONSUMER_ENTRY_BASE_URL="$CONSUMER_ENTRY_BASE_URL" \
        CONSUMER_ENTRY_API_KEY="${CONSUMER_ENTRY_API_KEY:-}" \
        CONSUMER_ENTRY_INGRESS_TOKEN="${CONSUMER_ENTRY_INGRESS_TOKEN:-}" \
        CONSUMER_ENTRY_SESSION_AUTH_SECRET="${CONSUMER_ENTRY_SESSION_AUTH_SECRET:-}" \
        MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET="${MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET:-}" \
        MATRIX_ENTRY_CONSUMER_SESSION_AUTH_KEY_ID="${MATRIX_ENTRY_CONSUMER_SESSION_AUTH_KEY_ID:-}" \
        MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_PATH="${MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_PATH:-}" \
        MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH="${MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH:-}" \
        MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION="${MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION:-}" \
        MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER="${MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER:-}" \
        MATRIX_ENTRY_CONSUMER_SESSION_AUTH_AUDIENCE="${MATRIX_ENTRY_CONSUMER_SESSION_AUTH_AUDIENCE:-}" \
        MATRIX_ENTRY_CONSUMER_SESSION_AUTH_TTL_SECS="${MATRIX_ENTRY_CONSUMER_SESSION_AUTH_TTL_SECS:-}" \
        MATRIX_ENTRY_INGRESS_TOKEN="${MATRIX_ENTRY_INGRESS_TOKEN:-}" \
        MATRIX_ENTRY_MAX_TEXT_CHARS="${MATRIX_ENTRY_MAX_TEXT_CHARS:-}" \
        MATRIX_ENTRY_RECENT_EVENT_STORE_PATH="${MATRIX_ENTRY_RECENT_EVENT_STORE_PATH:-}" \
        MATRIX_BOT_USER_ID="$MATRIX_BOT_USER_ID" \
        nohup cargo run -p "$pkg" >>"$log_file" 2>&1 &
        echo $! > "$pid_file"
      )
      ;;
    matrix-bot-relay)
      (
        cd "$ROOT_DIR"
        MATRIX_BOT_RELAY_BIND_ADDR="$MATRIX_BOT_RELAY_BIND_ADDR" \
        MATRIX_ADAPTER_BASE_URL="$MATRIX_ADAPTER_BASE_URL" \
        MATRIX_ENTRY_INGRESS_TOKEN="${MATRIX_ENTRY_INGRESS_TOKEN:-}" \
        MATRIX_HOMESERVER_BASE_URL="$MATRIX_HOMESERVER_BASE_URL" \
        MATRIX_BOT_USER_ID="$MATRIX_BOT_USER_ID" \
        MATRIX_RELAY_QUEUE_ENABLED="$MATRIX_RELAY_QUEUE_ENABLED" \
        MATRIX_RELAY_QUEUE_PATH="$MATRIX_RELAY_QUEUE_PATH" \
        MATRIX_RELAY_QUEUE_POLL_INTERVAL_MS="$MATRIX_RELAY_QUEUE_POLL_INTERVAL_MS" \
        MATRIX_RELAY_QUEUE_MAX_SIZE="$MATRIX_RELAY_QUEUE_MAX_SIZE" \
        MATRIX_RELAY_SEND_MAX_ATTEMPTS="$MATRIX_RELAY_SEND_MAX_ATTEMPTS" \
        MATRIX_RELAY_SEND_INITIAL_DELAY_MS="$MATRIX_RELAY_SEND_INITIAL_DELAY_MS" \
        MATRIX_RELAY_SEND_MAX_DELAY_MS="$MATRIX_RELAY_SEND_MAX_DELAY_MS" \
        MATRIX_RELAY_MAX_RECENT_EVENT_IDS="$MATRIX_RELAY_MAX_RECENT_EVENT_IDS" \
        MATRIX_ACCESS_TOKEN="${MATRIX_ACCESS_TOKEN:-}" \
        nohup cargo run -p "$pkg" >>"$log_file" 2>&1 &
        echo $! > "$pid_file"
      )
      ;;
    *)
      echo "[error] unknown package: ${pkg}" >&2
      return 1
      ;;
  esac

  for _ in $(seq 1 50); do
    pid="$(cat "$pid_file" 2>/dev/null || true)"
    if [[ -n "$pid" ]]; then
      break
    fi
    sleep 0.1
  done

  if [[ -z "$pid" || "$pid" == "0" ]]; then
    echo "[error] failed to start ${pkg}" >&2
    return 1
  fi

  cat > "$RUN_DIR/${pkg}.json" <<JSON
{
  "package": "${pkg}",
  "pid": ${pid},
  "started_at": "$(date -Iseconds)",
  "log": "${log_file}"
}
JSON

  echo "[start] ${pkg} -> pid=${pid}, log=${log_file}"
}

wait_for_http() {
  local label="$1"
  local url="$2"
  local retry

  for retry in $(seq 1 30); do
    if curl -fsS "$url/health" >/dev/null; then
      echo "[ok] ${label}"
      return 0
    fi
    sleep 1
  done

  echo "[warn] ${label} health timeout: ${url}/health"
  return 1
}

run_smoke() {
  local event_id="event-$(date +%s)-${RANDOM}"
  local sample_event
  sample_event=$(cat <<JSON
{
  "event_id": "${event_id}",
  "event_type": "m.room.message",
  "room_id": "!room-local:local.dev",
  "sender": "@alice:local.dev",
  "text": "测试矩阵任务链路"
}
JSON
)

  echo "\n[smoke] consumer-entry-api health"
  curl -fsS "http://$CONSUMER_ENTRY_BIND_ADDR/health" | cat

  echo "\n[smoke] matrix-entry-adapter health"
  curl -fsS "http://$MATRIX_ENTRY_ADAPTER_BIND_ADDR/health" | cat

  echo "\n[smoke] matrix-bot-relay health"
  curl -fsS "http://$MATRIX_BOT_RELAY_BIND_ADDR/health" | cat

  echo "\n[smoke] end-to-end payload to relay"
  curl -sS -X POST "http://$MATRIX_BOT_RELAY_BIND_ADDR/v1/inbound/matrix-event" \
    -H 'content-type: application/json' \
    -d "$sample_event" | cat
  echo
}

start_poller() {
  if [[ -z "${MATRIX_ACCESS_TOKEN:-}" ]]; then
    echo "[skip] matrix-bot-poller requires MATRIX_ACCESS_TOKEN; not started"
    return 0
  fi

  local poller_log="$LOG_DIR/matrix-bot-poller.log"
  local poller_pid_file="$RUN_DIR/matrix-bot-poller.pid"
  local poller_pid=""

  (
    cd "$ROOT_DIR"
    MATRIX_ACCESS_TOKEN="$MATRIX_ACCESS_TOKEN" \
    MATRIX_POLL_HOMESERVER="$MATRIX_POLL_HOMESERVER" \
    MATRIX_BOT_RELAY_BASE_URL="$MATRIX_BOT_RELAY_BASE_URL" \
    MATRIX_BOT_USER_ID="$MATRIX_BOT_USER_ID" \
    MATRIX_POLL_INTERVAL_MS="$MATRIX_POLL_INTERVAL_MS" \
    MATRIX_SYNC_FILTER="$MATRIX_SYNC_FILTER" \
    MATRIX_POLL_MAX_RECENT_EVENT_IDS="$MATRIX_POLL_MAX_RECENT_EVENT_IDS" \
    MATRIX_SYNC_STATE_FILE="$MATRIX_SYNC_STATE_FILE" \
    nohup cargo run -p matrix-bot-poller >>"$poller_log" 2>&1 &
    echo $! > "$poller_pid_file"
  )

  for _ in $(seq 1 50); do
    poller_pid="$(cat "$poller_pid_file" 2>/dev/null || true)"
    if [[ -n "$poller_pid" ]]; then
      break
    fi
    sleep 0.1
  done

  if [[ -z "$poller_pid" || "$poller_pid" == "0" ]]; then
    echo "[error] failed to start matrix-bot-poller" >&2
    return 1
  fi

  cat > "$RUN_DIR/matrix-bot-poller.json" <<JSON
{
  "package": "matrix-bot-poller",
  "pid": ${poller_pid},
  "started_at": "$(date -Iseconds)",
  "log": "$poller_log"
}
JSON

  echo "[start] matrix-bot-poller -> pid=${poller_pid}, log=$poller_log"
}

main() {
  local do_smoke="${1:-}"

  for pkg in "${PACKAGE_LIST[@]}"; do
    start_service "$pkg"
  done

  echo "\n[wait] waiting services to boot"
  wait_for_http "consumer-entry-api" "http://$CONSUMER_ENTRY_BIND_ADDR"
  wait_for_http "matrix-entry-adapter" "http://$MATRIX_ENTRY_ADAPTER_BIND_ADDR"
  wait_for_http "matrix-bot-relay" "http://$MATRIX_BOT_RELAY_BIND_ADDR"

  start_poller

  if [[ "$do_smoke" == "smoke" ]]; then
    run_smoke
  fi

  echo "\nAll matrix entry services started."
  echo "logs: $LOG_DIR"
  echo "pids: $RUN_DIR"
}

main "${1:-}"