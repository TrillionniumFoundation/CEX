#!/usr/bin/env bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL is required}"

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
evidence_dir="${CEX_P0_EVIDENCE_DIR:-$root/run/p0-release-evidence}"
work_dir=$(mktemp -d /tmp/cex-trnm-receipt-lookup.XXXXXX)
ledger_binary="${CEX_LEDGER_BINARY:-$root/target/debug/ledger-service}"
ledger_addr="127.0.0.1:17002"
unavailable_addr="127.0.0.1:17003"
base_url="http://$ledger_addr"
unavailable_url="http://$unavailable_addr"
game_authority_token="p0-receipt-lookup-game-authority-0123456789"
ledger_pid=""
started_at_epoch=$(date +%s)

org_id="92000000-0000-4000-8000-000000000001"
account_id="92000000-0000-4000-8000-000000000101"
trace_id="92000000-0000-4000-8000-000000000201"
response_loss_intent="p0-receipt-lookup-response-loss-v1"
concurrent_intent="p0-receipt-lookup-concurrent-v1"
pending_intent="p0-receipt-lookup-pending-v1"

cleanup() {
  if [[ -n "$ledger_pid" ]] && kill -0 "$ledger_pid" 2>/dev/null; then
    kill "$ledger_pid" 2>/dev/null || true
    wait "$ledger_pid" 2>/dev/null || true
  fi
  rm -rf "$work_dir"
}
trap cleanup EXIT

for command in psql curl python3; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "$command is required" >&2
    exit 69
  }
done
if [[ ! -x "$ledger_binary" ]]; then
  echo "ledger binary is missing or not executable: $ledger_binary" >&2
  exit 66
fi
mkdir -p "$evidence_dir"

start_ledger() {
  local bind_addr="$1"
  local database_url="$2"
  local log_file="$3"
  env \
    CEX_RUNTIME_PROFILE=dev \
    APP_ENV=dev \
    LEDGER_FAIL_FAST=false \
    LEDGER_BIND_ADDR="$bind_addr" \
    DATABASE_URL="$database_url" \
    TRNM_GAME_AUTHORITY_TOKEN="$game_authority_token" \
    "$ledger_binary" >>"$log_file" 2>&1 &
  ledger_pid=$!
}

stop_ledger() {
  if [[ -n "$ledger_pid" ]] && kill -0 "$ledger_pid" 2>/dev/null; then
    kill "$ledger_pid"
    wait "$ledger_pid" 2>/dev/null || true
  fi
  ledger_pid=""
}

wait_health() {
  local url="$1"
  local log_file="$2"
  local attempt
  for attempt in $(seq 1 120); do
    if curl -fsS "$url/health" >/dev/null 2>&1; then
      return 0
    fi
    if [[ -n "$ledger_pid" ]] && ! kill -0 "$ledger_pid" 2>/dev/null; then
      echo "ledger-service exited before health became ready" >&2
      cat "$log_file" >&2 || true
      return 1
    fi
    sleep 0.25
  done
  echo "ledger-service did not become healthy: $url" >&2
  cat "$log_file" >&2 || true
  return 1
}

http_call() {
  local expected_status="$1"
  local output="$2"
  shift 2
  local actual_status
  actual_status=$(curl --silent --show-error --output "$output" --write-out '%{http_code}' "$@")
  if [[ "$actual_status" != "$expected_status" ]]; then
    echo "HTTP assertion failed: expected=$expected_status actual=$actual_status" >&2
    cat "$output" >&2 || true
    return 1
  fi
}

lookup() {
  local url="$1"
  local intent_id="$2"
  local intent_hash="$3"
  local expected_status="$4"
  local output="$5"
  local authority="${6:-$game_authority_token}"
  http_call "$expected_status" "$output" \
    --get "$url/v1/trnm/economy/receipts/by-intent" \
    --data-urlencode "intent_id=$intent_id" \
    -H "x-trnm-game-authority: $authority" \
    -H "x-trnm-intent-sha256: $intent_hash"
}

assert_error_code() {
  local path="$1"
  local expected="$2"
  python3 - "$path" "$expected" <<'PY'
import json
from pathlib import Path
import sys

payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert payload["contract_version"] == "trnm_cex_settlement_receipt_lookup_v1", payload
assert payload["error"]["code"] == sys.argv[2], payload
PY
}

psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 \
  -v org_id="$org_id" \
  -v account_id="$account_id" \
  -v trace_id="$trace_id" <<'SQL'
insert into public.organizations (org_id, name)
values (:'org_id'::uuid, 'P0 TRNM receipt lookup qualification')
on conflict (org_id) do nothing;

select public.cex_open_account_v2(
    :'account_id'::uuid,
    :'org_id'::uuid,
    :'trace_id'::uuid,
    'trnm-receipt-lookup',
    'credit',
    6::smallint,
    100000000::bigint,
    'p0-receipt-lookup:account',
    'opening-v1',
    'p0-release-candidate'
);
SQL

python3 - "$work_dir" "$account_id" "$response_loss_intent" "$concurrent_intent" "$pending_intent" <<'PY'
import hashlib
import json
from pathlib import Path
import sys

work = Path(sys.argv[1])
account_id = sys.argv[2]

def write_case(name: str, intent_id: str, marker: str, amount: int) -> None:
    intent = {
        "protocol_version": "term_exchange_protocol_v2",
        "intent_id": intent_id,
        "term_id": f"p0-receipt-lookup:{intent_id}",
        "term_version": "1.0.0",
        "domain": "trnm_game",
        "kind": "reserve",
        "idempotency_key": {"scope": "p0-receipt-lookup", "key": intent_id},
        "actors": [{
            "actor_id": "p0-world-settlement",
            "actor_kind": "world-service",
            "account_id": account_id,
        }],
        "assets": [],
        "amount_credits": amount,
        "currency": "credit",
        "metadata": {"qualification_case": marker},
        "created_at_epoch": 1_700_000_000,
    }
    canonical = json.dumps(intent, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
    digest = hashlib.sha256(canonical).hexdigest()
    (work / f"{name}.request.json").write_text(
        json.dumps({"intent": intent}, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    (work / f"{name}.intent.json").write_text(
        json.dumps(intent, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    (work / f"{name}.hash").write_text(digest + "\n", encoding="utf-8")

write_case("response-loss", sys.argv[3], "response-loss", 25)
write_case("concurrent", sys.argv[4], "concurrent", 25)
write_case("pending", sys.argv[5], "pending", 1)
PY

response_loss_hash=$(<"$work_dir/response-loss.hash")
concurrent_hash=$(<"$work_dir/concurrent.hash")
pending_hash=$(<"$work_dir/pending.hash")

start_ledger "$ledger_addr" "$DATABASE_URL" "$work_dir/ledger.log"
wait_health "$base_url" "$work_dir/ledger.log"

# Send a complete request, wait until the server has produced its response (which
# occurs only after the durable transaction commits), then discard the response.
# This models an application that loses the response after the remote side effect,
# rather than cancelling the request before the server can commit it.
python3 - "$ledger_addr" "$game_authority_token" "$work_dir/response-loss.request.json" <<'PY'
from pathlib import Path
import socket
import sys

host, port = sys.argv[1].rsplit(":", 1)
token = sys.argv[2]
body = Path(sys.argv[3]).read_bytes()
request = (
    f"POST /v1/trnm/economy/intents HTTP/1.1\r\n"
    f"Host: {host}:{port}\r\n"
    f"Content-Type: application/json\r\n"
    f"x-trnm-game-authority: {token}\r\n"
    f"Content-Length: {len(body)}\r\n"
    f"Connection: close\r\n\r\n"
).encode() + body
with socket.create_connection((host, int(port)), timeout=5) as connection:
    connection.settimeout(15)
    connection.sendall(request)
    if not connection.recv(1):
        raise SystemExit("ledger-service closed before producing a response")
    # Deliberately discard the status, headers and body. The caller receives no
    # usable business response and must recover through receipt lookup.
PY

committed=0
for _ in $(seq 1 120); do
  committed=$(psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 -At \
    -v intent_id="$response_loss_intent" <<'SQL'
select count(*) from public.trnm_economic_receipts where intent_id = :'intent_id';
SQL
)
  if [[ "$committed" == "1" ]]; then
    break
  fi
  sleep 0.25
done
if [[ "$committed" != "1" ]]; then
  echo "response-loss request did not commit a durable receipt" >&2
  cat "$work_dir/ledger.log" >&2 || true
  exit 1
fi

lookup "$base_url" "$response_loss_intent" "$response_loss_hash" 200 \
  "$work_dir/response-loss.lookup.json"

# Two first submissions race on the same immutable identity.
for lane in a b; do
  (
    http_call 200 "$work_dir/concurrent-$lane.json" \
      "$base_url/v1/trnm/economy/intents" \
      -H "x-trnm-game-authority: $game_authority_token" \
      -H 'content-type: application/json' \
      --data-binary "@$work_dir/concurrent.request.json"
  ) >"$work_dir/concurrent-$lane.status" 2>&1 &
  eval "lane_${lane}_pid=$!"
done
wait "$lane_a_pid"
wait "$lane_b_pid"
cmp "$work_dir/concurrent-a.json" "$work_dir/concurrent-b.json"
lookup "$base_url" "$concurrent_intent" "$concurrent_hash" 200 \
  "$work_dir/concurrent.lookup.json"
python3 - "$work_dir/concurrent-a.json" "$work_dir/concurrent.lookup.json" <<'PY'
import json
from pathlib import Path
import sys
submitted = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
looked_up = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))["receipt"]
assert submitted == looked_up, (submitted, looked_up)
PY

wrong_hash="${response_loss_hash%?}0"
if [[ "$wrong_hash" == "$response_loss_hash" ]]; then
  wrong_hash="${response_loss_hash%?}1"
fi
lookup "$base_url" "$response_loss_intent" "$wrong_hash" 409 "$work_dir/hash-conflict.json"
assert_error_code "$work_dir/hash-conflict.json" immutable_intent_hash_conflict
lookup "$base_url" "$response_loss_intent" ABC 400 "$work_dir/malformed-hash.json"
assert_error_code "$work_dir/malformed-hash.json" invalid_intent_hash
lookup "$base_url" "$response_loss_intent" "$response_loss_hash" 401 \
  "$work_dir/wrong-authority.json" wrong-audience-token
assert_error_code "$work_dir/wrong-authority.json" unauthorized
http_call 401 "$work_dir/missing-authority.json" \
  --get "$base_url/v1/trnm/economy/receipts/by-intent" \
  --data-urlencode "intent_id=$response_loss_intent" \
  -H "x-trnm-intent-sha256: $response_loss_hash"
assert_error_code "$work_dir/missing-authority.json" unauthorized
lookup "$base_url" p0-receipt-lookup-never-created "$response_loss_hash" 404 \
  "$work_dir/not-found.json"
assert_error_code "$work_dir/not-found.json" intent_receipt_not_found

psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 \
  -v intent_id="$pending_intent" \
  -v payload_hash="$pending_hash" \
  -v intent_json="$(<"$work_dir/pending.intent.json")" <<'SQL'
insert into public.trnm_economic_intents (
    intent_id, protocol_version, idempotency_scope, idempotency_key,
    payload_hash, intent_json, status
) values (
    :'intent_id', 'term_exchange_protocol_v2', 'p0-receipt-lookup', :'intent_id',
    :'payload_hash', :'intent_json'::jsonb, 'processing'
);
SQL
lookup "$base_url" "$pending_intent" "$pending_hash" 503 "$work_dir/pending.json"
assert_error_code "$work_dir/pending.json" receipt_not_finalized

python3 - "$work_dir/response-loss.lookup.json" "$response_loss_intent" "$response_loss_hash" <<'PY'
import json
from pathlib import Path
import sys
payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert payload["contract_version"] == "trnm_cex_settlement_receipt_lookup_v1", payload
assert payload["intent_id"] == sys.argv[2], payload
assert payload["intent_hash"] == sys.argv[3], payload
assert payload["receipt"]["intent_id"] == sys.argv[2], payload
assert payload["receipt"]["evidence"]["payload_hash"] == sys.argv[3], payload
PY

# A process restart must not change lookup identity or bytes.
stop_ledger
start_ledger "$ledger_addr" "$DATABASE_URL" "$work_dir/ledger-restart.log"
wait_health "$base_url" "$work_dir/ledger-restart.log"
lookup "$base_url" "$response_loss_intent" "$response_loss_hash" 200 \
  "$work_dir/response-loss.lookup-after-restart.json"
cmp "$work_dir/response-loss.lookup.json" "$work_dir/response-loss.lookup-after-restart.json"
stop_ledger

# A database outage/placeholder repository is a 503, never a false 404.
start_ledger "$unavailable_addr" \
  'postgres://cex:cex_ci_password@127.0.0.1:1/cex_ci?connect_timeout=1' \
  "$work_dir/ledger-unavailable.log"
wait_health "$unavailable_url" "$work_dir/ledger-unavailable.log"
lookup "$unavailable_url" "$response_loss_intent" "$response_loss_hash" 503 \
  "$work_dir/database-unavailable.json"
assert_error_code "$work_dir/database-unavailable.json" receipt_lookup_unavailable
stop_ledger

raw_database=$(psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 -At \
  -v account_id="$account_id" \
  -v response_loss_intent="$response_loss_intent" \
  -v concurrent_intent="$concurrent_intent" <<'SQL'
select json_build_object(
  'intent_count', (
    select count(*) from public.trnm_economic_intents
     where intent_id in (:'response_loss_intent', :'concurrent_intent')
  ),
  'receipt_count', (
    select count(*) from public.trnm_economic_receipts
     where intent_id in (:'response_loss_intent', :'concurrent_intent')
  ),
  'ledger_entry_count', (
    select count(*) from public.ledger_entries where account_id = :'account_id'::uuid
  ),
  'distinct_operation_count', (
    select count(distinct operation_id) from public.ledger_entries
     where account_id = :'account_id'::uuid
  ),
  'balance', (select balance from public.accounts where account_id = :'account_id'::uuid),
  'reserved', (select reserved from public.accounts where account_id = :'account_id'::uuid),
  'response_loss_receipt_id', (
    select receipt_id from public.trnm_economic_receipts where intent_id = :'response_loss_intent'
  ),
  'concurrent_receipt_id', (
    select receipt_id from public.trnm_economic_receipts where intent_id = :'concurrent_intent'
  )
);
SQL
)

ended_at_epoch=$(date +%s)
python3 - \
  "$evidence_dir/trnm-receipt-lookup.json" \
  "$raw_database" \
  "$work_dir/response-loss.lookup.json" \
  "$work_dir/concurrent.lookup.json" \
  "$response_loss_hash" \
  "$concurrent_hash" \
  "$started_at_epoch" \
  "$ended_at_epoch" \
  "${GITHUB_SHA:-unknown}" <<'PY'
import json
from pathlib import Path
import sys

output = Path(sys.argv[1])
database = json.loads(sys.argv[2])
response_loss = json.loads(Path(sys.argv[3]).read_text(encoding="utf-8"))
concurrent = json.loads(Path(sys.argv[4]).read_text(encoding="utf-8"))
assert database["intent_count"] == 2, database
assert database["receipt_count"] == 2, database
assert database["ledger_entry_count"] == 3, database
assert database["distinct_operation_count"] == 3, database
assert float(database["balance"]) == 100.0, database
assert float(database["reserved"]) == 50.0, database
assert database["response_loss_receipt_id"] == response_loss["receipt"]["receipt_id"], database
assert database["concurrent_receipt_id"] == concurrent["receipt"]["receipt_id"], database
payload = {
    "schema": "cex.trnm-settlement-receipt-lookup-qualification.v1",
    "status": "passed",
    "ok": True,
    "commit_sha": sys.argv[9],
    "started_at_epoch": int(sys.argv[7]),
    "ended_at_epoch": int(sys.argv[8]),
    "duration_seconds": int(sys.argv[8]) - int(sys.argv[7]),
    "contract_version": "trnm_cex_settlement_receipt_lookup_v1",
    "response_loss_after_commit": True,
    "concurrent_duplicate_same_receipt": True,
    "restart_lookup_identical": True,
    "database_outage_is_503_not_404": True,
    "retention_mode": "indefinite-no-pruning-v1",
    "response_loss_intent_hash": sys.argv[5],
    "concurrent_intent_hash": sys.argv[6],
    "database": database,
}
output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "TRNM settlement receipt lookup qualification passed: $evidence_dir/trnm-receipt-lookup.json"
