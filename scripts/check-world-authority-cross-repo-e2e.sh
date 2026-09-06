#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORLD_ROOT="${1:-$ROOT/.world-source}"
WORLD_SHA="${WORLD_SOURCE_SHA:-554761417edbb37a2f20deed23917a9b05abdfe2}"
WORLD_MANIFEST="$WORLD_ROOT/trillionnium/crates/world-authority/Cargo.toml"

[[ -f "$WORLD_MANIFEST" ]] || {
  echo "missing exact-SHA World source checkout at $WORLD_ROOT" >&2
  exit 1
}
if [[ -d "$WORLD_ROOT/.git" ]]; then
  actual_sha="$(git -C "$WORLD_ROOT" rev-parse HEAD)"
  [[ "$actual_sha" == "$WORLD_SHA" ]] || {
    echo "World source SHA mismatch: expected=$WORLD_SHA actual=$actual_sha" >&2
    exit 1
  }
fi

RUN="$(mktemp -d)"
WORLD_PID=""
ADAPTER_PID=""
cleanup() {
  for pid in "$ADAPTER_PID" "$WORLD_PID"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done
  rm -rf "$RUN"
}
trap cleanup EXIT

wait_http() {
  local url="$1"
  local attempts="${2:-90}"
  for ((i=1; i<=attempts; i++)); do
    if curl --fail --silent --show-error --max-time 2 "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "endpoint did not become ready: $url" >&2
  return 1
}

stop_world() {
  if [[ -n "$WORLD_PID" ]] && kill -0 "$WORLD_PID" 2>/dev/null; then
    kill "$WORLD_PID"
    wait "$WORLD_PID" 2>/dev/null || true
  fi
  WORLD_PID=""
}

start_world() {
  local reset="${1:-false}"
  local args=(serve --bind 127.0.0.1:18787 --actor-id local-player --state-file "$RUN/world-state.json")
  if [[ "$reset" == "true" ]]; then
    args+=(--reset-state)
  fi
  "$RUN/world-target/debug/trnm-world-server" "${args[@]}" \
    >"$RUN/world.log" 2>&1 &
  WORLD_PID=$!
  wait_http http://127.0.0.1:18787/health
}

cd "$ROOT"
CARGO_TARGET_DIR="$RUN/cex-target" \
  cargo build --quiet --locked -p consumer-entry-api --bin world-authority-adapter
CARGO_TARGET_DIR="$RUN/world-target" \
  cargo build --quiet --locked --manifest-path "$WORLD_MANIFEST" -p trnm-world-server

start_world true

WORLD_AUTHORITY_ADAPTER_RUNTIME_PROFILE=local_dev \
WORLD_AUTHORITY_ADAPTER_BIND_ADDR=127.0.0.1:18096 \
WORLD_AUTHORITY_ADAPTER_TIMEOUT_MS=2000 \
TRILLIONNIUM_WORLD_BASE_URL=http://127.0.0.1:18787 \
TRILLIONNIUM_WORLD_API_CONTRACT=trillionnium_world_api_v1 \
"$RUN/cex-target/debug/world-authority-adapter" \
  >"$RUN/adapter.log" 2>&1 &
ADAPTER_PID=$!
wait_http http://127.0.0.1:18096/health
wait_http http://127.0.0.1:18096/v1/world/full-split

curl --fail --silent --show-error \
  http://127.0.0.1:18096/v1/world/full-split \
  >"$RUN/full-split.json"
curl --fail --silent --show-error \
  http://127.0.0.1:18096/v1/world/state \
  >"$RUN/state-before.json"
cp "$RUN/world-state.json" "$RUN/world-state-rollback.json"

curl --fail --silent --show-error \
  -X POST \
  -H 'content-type: application/json' \
  -H 'idempotency-key: world-cutover-smoke-1' \
  --data '{"api_contract":"trillionnium_world_api_v1","command":{"kind":"move","actor_id":"local-player","direction":"east"}}' \
  http://127.0.0.1:18096/v1/world/command \
  >"$RUN/command.json"
curl --fail --silent --show-error \
  http://127.0.0.1:18096/v1/world/state \
  >"$RUN/state-after.json"

python3 - "$RUN" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
full_split = json.loads((root / "full-split.json").read_text())
command = json.loads((root / "command.json").read_text())
before = json.loads((root / "state-before.json").read_text())
after = json.loads((root / "state-after.json").read_text())

def player_node(state):
    for position in state.get("positions", []):
        if position.get("actor_id") == "local-player":
            return position.get("node_id")
    raise SystemExit("local-player position missing")

if full_split.get("api_contract") != "trillionnium_world_api_v1":
    raise SystemExit("full-split contract mismatch through CEX adapter")
if full_split.get("response_contract") != "trillionnium_world_full_split_response_v1":
    raise SystemExit("full-split response contract mismatch through CEX adapter")
if command.get("api_contract") != "trillionnium_world_api_v1":
    raise SystemExit("command response contract mismatch")
if command.get("decision", {}).get("accepted") is not True:
    raise SystemExit("World command was not accepted")
if player_node(before) == player_node(after):
    raise SystemExit("World command did not mutate the World-owned repository")
(root / "nodes.json").write_text(json.dumps({
    "before": player_node(before),
    "after": player_node(after),
}, indent=2) + "\n")
PY

stop_world
outage_code="$(curl --silent --show-error --max-time 8 \
  -o "$RUN/outage.json" -w '%{http_code}' \
  http://127.0.0.1:18096/v1/world/full-split || true)"
[[ "$outage_code" == "503" ]] || {
  cat "$RUN/outage.json" >&2 || true
  echo "adapter outage response was $outage_code, expected 503" >&2
  exit 1
}
python3 - "$RUN/outage.json" <<'PY'
import json
import pathlib
import sys

value = json.loads(pathlib.Path(sys.argv[1]).read_text())
if value.get("error") != "world_authority_unavailable":
    raise SystemExit("adapter outage error contract mismatch")
if value.get("local_fallback_used") is not False:
    raise SystemExit("adapter used or claimed a local World fallback")
PY

start_world false
curl --fail --silent --show-error \
  http://127.0.0.1:18096/v1/world/state \
  >"$RUN/state-restarted.json"
python3 - "$RUN" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
nodes = json.loads((root / "nodes.json").read_text())
restarted = json.loads((root / "state-restarted.json").read_text())
node = next(
    position.get("node_id")
    for position in restarted.get("positions", [])
    if position.get("actor_id") == "local-player"
)
if node != nodes["after"]:
    raise SystemExit(f"World repository restart drift: expected={nodes['after']} actual={node}")
PY

curl --fail --silent --show-error http://127.0.0.1:18096/v1/world/state >"$RUN/replay-a.json"
curl --fail --silent --show-error http://127.0.0.1:18096/v1/world/state >"$RUN/replay-b.json"
python3 - "$RUN/replay-a.json" "$RUN/replay-b.json" <<'PY'
import json
import pathlib
import sys

a = json.loads(pathlib.Path(sys.argv[1]).read_text())
b = json.loads(pathlib.Path(sys.argv[2]).read_text())
if a != b:
    raise SystemExit("repeated read projection is unstable")
PY

stop_world
cp "$RUN/world-state-rollback.json" "$RUN/world-state.json"
start_world false
curl --fail --silent --show-error \
  http://127.0.0.1:18096/v1/world/state \
  >"$RUN/state-rolled-back.json"
python3 - "$RUN" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
nodes = json.loads((root / "nodes.json").read_text())
rolled_back = json.loads((root / "state-rolled-back.json").read_text())
node = next(
    position.get("node_id")
    for position in rolled_back.get("positions", [])
    if position.get("actor_id") == "local-player"
)
if node != nodes["before"]:
    raise SystemExit(f"World rollback drift: expected={nodes['before']} actual={node}")
summary = {
    "schema": "cex.world.authority.cross-repo-smoke.v1",
    "world_source_sha": "554761417edbb37a2f20deed23917a9b05abdfe2",
    "world_api_contract": "trillionnium_world_api_v1",
    "adapter_contract": "cex_trillionnium_world_authority_adapter_v1",
    "success_forwarding": True,
    "world_owned_mutation": True,
    "restart_persistence": True,
    "outage_fail_closed_503": True,
    "local_fallback_used": False,
    "repeated_read_stable": True,
    "development_snapshot_rollback": True,
    "mutation_idempotency": "not_claimed_pending_durable_world_adapter",
    "production_authorization": "not_granted",
}
(root / "cross-repo-smoke-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))
PY
