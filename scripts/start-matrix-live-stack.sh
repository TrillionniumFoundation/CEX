#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
# shellcheck source=scripts/_dev-helpers.sh
source "$ROOT_DIR/scripts/_dev-helpers.sh"

CEX_ENV_FILE="${CEX_ENV_FILE:-$ROOT_DIR/run/local-production/.env}"
if [[ -f "$CEX_ENV_FILE" ]]; then
  cex_load_env "$CEX_ENV_FILE"
else
  cex_load_env
fi

RUN_DIR="$ROOT_DIR/run/matrix-live"
SYNAPSE_DATA_DIR="$RUN_DIR/synapse"
LOG_DIR="$ROOT_DIR/logs/matrix-bot-entry"
PID_DIR="$ROOT_DIR/run/matrix-bot-entry"
mkdir -p "$RUN_DIR" "$SYNAPSE_DATA_DIR" "$LOG_DIR" "$PID_DIR"

SYNAPSE_SERVER_NAME="${SYNAPSE_SERVER_NAME:-local.dev}"
SYNAPSE_CONTAINER_NAME="${SYNAPSE_CONTAINER_NAME:-cex-synapse}"
ELEMENT_CONTAINER_NAME="${ELEMENT_CONTAINER_NAME:-cex-element-web}"
ELEMENT_PORT="${ELEMENT_PORT:-8081}"
MATRIX_HOMESERVER_BASE_URL="${MATRIX_HOMESERVER_BASE_URL:-http://127.0.0.1:8008}"
MATRIX_BOT_RELAY_BASE_URL="${MATRIX_BOT_RELAY_BASE_URL:-http://127.0.0.1:8092}"
MATRIX_SYNC_STATE_FILE="${MATRIX_SYNC_STATE_FILE:-$RUN_DIR/poller.state}"
MATRIX_RELAY_QUEUE_PATH="${MATRIX_RELAY_QUEUE_PATH:-$RUN_DIR/relay-queue.json}"
MATRIX_BOT_USER_LOCALPART="${MATRIX_BOT_USER_LOCALPART:-cex-bot}"
MATRIX_ALICE_LOCALPART="${MATRIX_ALICE_LOCALPART:-alice}"
MATRIX_BOT_USER_ID="@${MATRIX_BOT_USER_LOCALPART}:${SYNAPSE_SERVER_NAME}"
MATRIX_ALICE_USER_ID="@${MATRIX_ALICE_LOCALPART}:${SYNAPSE_SERVER_NAME}"
export RUN_DIR MATRIX_HOMESERVER_BASE_URL MATRIX_ALICE_LOCALPART MATRIX_BOT_USER_LOCALPART

wait_http() {
  local url="$1"
  local label="$2"
  local tries="${3:-60}"
  for _ in $(seq 1 "$tries"); do
    if curl -fsS --max-time 3 "$url" >/dev/null 2>&1; then
      echo "[ok] $label"
      return 0
    fi
    sleep 2
  done
  echo "[error] timeout waiting for $label: $url" >&2
  return 1
}

stop_pid_file() {
  local file="$1"
  if [[ -f "$file" ]]; then
    local pid
    pid="$(cat "$file" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
    fi
    rm -f "$file"
  fi
}

if [[ ! -f "$SYNAPSE_DATA_DIR/homeserver.yaml" ]]; then
  echo "[matrix] generating Synapse config for $SYNAPSE_SERVER_NAME"
  cex_docker run --rm \
    -v "$SYNAPSE_DATA_DIR:/data" \
    -e SYNAPSE_SERVER_NAME="$SYNAPSE_SERVER_NAME" \
    -e SYNAPSE_REPORT_STATS=no \
    matrixdotorg/synapse:latest generate
fi

if ! cex_docker ps --format '{{.Names}}' | grep -qx "$SYNAPSE_CONTAINER_NAME"; then
  cex_docker rm -f "$SYNAPSE_CONTAINER_NAME" >/dev/null 2>&1 || true
  echo "[matrix] starting Synapse container $SYNAPSE_CONTAINER_NAME"
  cex_docker run -d --name "$SYNAPSE_CONTAINER_NAME" \
    -v "$SYNAPSE_DATA_DIR:/data" \
    -p 8008:8008 \
    matrixdotorg/synapse:latest >/dev/null
else
  echo "[skip] Synapse already running: $SYNAPSE_CONTAINER_NAME"
fi
wait_http "$MATRIX_HOMESERVER_BASE_URL/_matrix/client/versions" "Synapse /versions" 90

CREDENTIALS_FILE="$RUN_DIR/credentials.env"
if [[ ! -f "$CREDENTIALS_FILE" ]]; then
  python3 - <<'PY' > "$CREDENTIALS_FILE"
import secrets
print('ALICE_PASS='+secrets.token_urlsafe(24))
print('BOT_PASS='+secrets.token_urlsafe(24))
PY
  chmod 600 "$CREDENTIALS_FILE"
fi
# shellcheck disable=SC1090
set -a; source "$CREDENTIALS_FILE"; set +a

cex_docker exec "$SYNAPSE_CONTAINER_NAME" register_new_matrix_user \
  -u "$MATRIX_ALICE_LOCALPART" -p "$ALICE_PASS" --no-admin --exists-ok \
  -c /data/homeserver.yaml http://localhost:8008 >/dev/null
cex_docker exec "$SYNAPSE_CONTAINER_NAME" register_new_matrix_user \
  -u "$MATRIX_BOT_USER_LOCALPART" -p "$BOT_PASS" --admin --exists-ok \
  -c /data/homeserver.yaml http://localhost:8008 >/dev/null

echo "[matrix] users ready: $MATRIX_ALICE_USER_ID / $MATRIX_BOT_USER_ID"

python3 - <<'PY'
import json, os, pathlib, urllib.parse, urllib.request
base = os.environ['MATRIX_HOMESERVER_BASE_URL']
run_dir = pathlib.Path(os.environ['RUN_DIR'])
tokens_path = run_dir / 'tokens.json'

def login(user, password):
    body = json.dumps({
        'type': 'm.login.password',
        'identifier': {'type': 'm.id.user', 'user': user},
        'password': password,
    }).encode()
    req = urllib.request.Request(base + '/_matrix/client/v3/login', data=body, headers={'content-type': 'application/json'})
    with urllib.request.urlopen(req, timeout=15) as resp:
        return json.load(resp)

def req(method, path, token, body=None):
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(base + path, data=data, method=method, headers={'Authorization': 'Bearer ' + token, 'content-type': 'application/json'})
    with urllib.request.urlopen(request, timeout=15) as resp:
        return json.load(resp)

tokens = {}
if tokens_path.exists():
    try:
        tokens = json.load(open(tokens_path))
    except Exception:
        tokens = {}

def valid_session(record, expected_user_id):
    if not isinstance(record, dict) or not record.get('access_token'):
        return False
    try:
        whoami = req('GET', '/_matrix/client/v3/account/whoami', record['access_token'])
    except Exception:
        return False
    return whoami.get('user_id') == expected_user_id

server_name = os.environ.get('SYNAPSE_SERVER_NAME', 'local.dev')
alice_expected = '@' + os.environ['MATRIX_ALICE_LOCALPART'] + ':' + server_name
bot_expected = '@' + os.environ['MATRIX_BOT_USER_LOCALPART'] + ':' + server_name
alice = tokens.get('alice') if valid_session(tokens.get('alice'), alice_expected) else login(os.environ['MATRIX_ALICE_LOCALPART'], os.environ['ALICE_PASS'])
bot = tokens.get('cex_bot') if valid_session(tokens.get('cex_bot'), bot_expected) else login(os.environ['MATRIX_BOT_USER_LOCALPART'], os.environ['BOT_PASS'])
tokens.update({'homeserver': base, 'alice': alice, 'cex_bot': bot})
room_id = tokens.get('room_id')
if not room_id:
    room = req('POST', '/_matrix/client/v3/createRoom', alice['access_token'], {
        'preset': 'private_chat',
        'name': 'CEX Frontend E2E',
        'topic': 'CEX Matrix/Element frontend end-to-end room',
        'invite': [bot['user_id']],
    })
    room_id = room['room_id']
    join_path = '/_matrix/client/v3/join/' + urllib.parse.quote(room_id, safe='')
    try:
        req('POST', join_path, bot['access_token'], {})
    except Exception:
        pass
    tokens['room_id'] = room_id

tokens_path.write_text(json.dumps(tokens, indent=2))
tokens_path.chmod(0o600)
print(json.dumps({'room_id': room_id, 'alice_user_id': alice['user_id'], 'bot_user_id': bot['user_id']}))
PY

cat > "$RUN_DIR/element-config.json" <<JSON
{
  "default_server_config": {
    "m.homeserver": {
      "base_url": "$MATRIX_HOMESERVER_BASE_URL",
      "server_name": "$SYNAPSE_SERVER_NAME"
    }
  },
  "brand": "CEX Element",
  "disable_custom_urls": false,
  "disable_guests": true,
  "default_theme": "light",
  "room_directory": { "servers": ["$SYNAPSE_SERVER_NAME"] }
}
JSON

if ! cex_docker ps --format '{{.Names}}' | grep -qx "$ELEMENT_CONTAINER_NAME"; then
  cex_docker rm -f "$ELEMENT_CONTAINER_NAME" >/dev/null 2>&1 || true
  echo "[matrix] starting Element Web on http://127.0.0.1:$ELEMENT_PORT"
  cex_docker run -d --name "$ELEMENT_CONTAINER_NAME" \
    -p "$ELEMENT_PORT:80" \
    -v "$RUN_DIR/element-config.json:/app/config.json:ro" \
    vectorim/element-web:latest >/dev/null
else
  echo "[skip] Element Web already running: $ELEMENT_CONTAINER_NAME"
fi
wait_http "http://127.0.0.1:$ELEMENT_PORT/config.json" "Element Web config" 60

if [[ "${CEX_MATRIX_START_CEX_RUNTIME:-1}" == "1" ]]; then
  echo "[cex] ensuring local-production runtime is up"
  CEX_ENV_FILE="$CEX_ENV_FILE" "$ROOT_DIR/scripts/runtime-manager-linux.sh" restart >/tmp/cex-matrix-live-runtime-restart.log
  CEX_ENV_FILE="$CEX_ENV_FILE" "$ROOT_DIR/scripts/runtime-manager-linux.sh" status
fi

python3 - <<'PY' > "$RUN_DIR/live-env.sh"
import json, pathlib, shlex
j=json.load(open('run/matrix-live/tokens.json'))
print('export MATRIX_HOMESERVER_BASE_URL='+shlex.quote(j['homeserver']))
print('export MATRIX_POLL_HOMESERVER='+shlex.quote(j['homeserver']))
print('export MATRIX_ACCESS_TOKEN='+shlex.quote(j['cex_bot']['access_token']))
print('export MATRIX_BOT_USER_ID='+shlex.quote(j['cex_bot']['user_id']))
print('export MATRIX_LIVE_ROOM_ID='+shlex.quote(j['room_id']))
PY
chmod 600 "$RUN_DIR/live-env.sh"
# shellcheck disable=SC1090
source "$RUN_DIR/live-env.sh"

stop_pid_file "$PID_DIR/matrix-bot-relay.pid"
stop_pid_file "$PID_DIR/matrix-bot-poller.pid"
rm -f "$MATRIX_SYNC_STATE_FILE"

MATRIX_BOT_RELAY_BIND_ADDR="${MATRIX_BOT_RELAY_BIND_ADDR:-127.0.0.1:8092}" \
MATRIX_ADAPTER_BASE_URL="${MATRIX_ADAPTER_BASE_URL:-http://127.0.0.1:8091}" \
MATRIX_ENTRY_INGRESS_TOKEN="${MATRIX_ENTRY_INGRESS_TOKEN:-}" \
MATRIX_HOMESERVER_BASE_URL="$MATRIX_HOMESERVER_BASE_URL" \
MATRIX_BOT_USER_ID="$MATRIX_BOT_USER_ID" \
MATRIX_ACCESS_TOKEN="$MATRIX_ACCESS_TOKEN" \
MATRIX_RELAY_QUEUE_PATH="$MATRIX_RELAY_QUEUE_PATH" \
nohup cargo run -p matrix-bot-relay >> "$LOG_DIR/matrix-bot-relay.log" 2>&1 &
echo $! > "$PID_DIR/matrix-bot-relay.pid"
wait_http "${MATRIX_BOT_RELAY_BASE_URL}/health" "matrix-bot-relay" 60

MATRIX_ACCESS_TOKEN="$MATRIX_ACCESS_TOKEN" \
MATRIX_POLL_HOMESERVER="$MATRIX_POLL_HOMESERVER" \
MATRIX_BOT_RELAY_BASE_URL="$MATRIX_BOT_RELAY_BASE_URL" \
MATRIX_BOT_USER_ID="$MATRIX_BOT_USER_ID" \
MATRIX_SYNC_STATE_FILE="$MATRIX_SYNC_STATE_FILE" \
MATRIX_SYNC_FILTER="${MATRIX_SYNC_FILTER:-}" \
nohup cargo run -p matrix-bot-poller >> "$LOG_DIR/matrix-bot-poller.log" 2>&1 &
echo $! > "$PID_DIR/matrix-bot-poller.pid"

echo "[ok] Matrix live stack ready"
echo "Element Web: http://127.0.0.1:$ELEMENT_PORT"
echo "Homeserver: $MATRIX_HOMESERVER_BASE_URL ($SYNAPSE_SERVER_NAME)"
echo "Room ID: $MATRIX_LIVE_ROOM_ID"
echo "Run: ./scripts/check-matrix-live-room-e2e.sh"
