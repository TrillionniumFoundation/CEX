#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

EXECUTION_BASE_URL="${EXECUTION_BASE_URL:-http://127.0.0.1:7003}"
WORKER_ID="${EXECUTION_WORKER_ID:-cex-linux-worker}"
ADMIN_TOKEN="${EXECUTION_WORKER_ADMIN_TOKEN:-${EXECUTION_ADMIN_TOKEN:-${IDENTITY_ADMIN_TOKEN:-}}}"
POLL_IDLE_SECS="${EXECUTION_WORKER_IDLE_SECS:-2}"
LEASE_SECONDS="${EXECUTION_CLAIM_LEASE_SECONDS:-300}"
LEASE_RENEW_INTERVAL="${EXECUTION_WORKER_RENEW_INTERVAL_SECS:-$(( LEASE_SECONDS > 30 ? LEASE_SECONDS / 3 : 10 ))}"
CURL_TIMEOUT_SECS="${EXECUTION_WORKER_CURL_TIMEOUT_SECS:-600}"

usage() {
  cat <<'EOF'
Usage: scripts/execution-queued-worker.sh [run|once|status]

Commands:
  run     Loop forever: claim queued-worker executions and process them
  once    Attempt one claim/process cycle, then exit
  status  Print /v1/executions/worker-queue/summary
EOF
}

require_worker_config() {
  cex_require_cmd curl python3
  if [[ -z "$ADMIN_TOKEN" ]]; then
    echo "missing execution worker admin token; set EXECUTION_WORKER_ADMIN_TOKEN, EXECUTION_ADMIN_TOKEN, or IDENTITY_ADMIN_TOKEN" >&2
    return 64
  fi
}

json_field() {
  local body="$1"
  local expr="$2"
  python3 - "$body" "$expr" <<'PY'
import json, sys
body = sys.argv[1]
expr = sys.argv[2]
obj = json.loads(body)
value = obj
for part in expr.split('.'):
    if not part:
        continue
    if isinstance(value, dict):
        value = value.get(part)
    else:
        value = None
        break
if value is None:
    sys.exit(1)
if isinstance(value, (dict, list)):
    print(json.dumps(value, ensure_ascii=False))
else:
    print(value)
PY
}

api_request() {
  local method="$1"
  local path="$2"
  local payload="${3:-}"
  local body_file curl_status code
  body_file="$(mktemp)"
  curl_status=0
  if [[ -n "$payload" ]]; then
    code="$(curl -sS -o "$body_file" -w '%{http_code}' \
      --max-time "$CURL_TIMEOUT_SECS" \
      -X "$method" \
      -H 'content-type: application/json' \
      -H "x-admin-token: $ADMIN_TOKEN" \
      --data "$payload" \
      "$EXECUTION_BASE_URL$path")" || curl_status=$?
  else
    code="$(curl -sS -o "$body_file" -w '%{http_code}' \
      --max-time "$CURL_TIMEOUT_SECS" \
      -X "$method" \
      -H "x-admin-token: $ADMIN_TOKEN" \
      "$EXECUTION_BASE_URL$path")" || curl_status=$?
  fi
  if [[ "$curl_status" -ne 0 ]]; then
    printf '000\n'
    printf '{"error":"curl_failed","curl_status":%s,"path":%s}\n' "$curl_status" "$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$path")"
  else
    printf '%s\n' "$code"
    cat "$body_file"
  fi
  rm -f "$body_file"
}

print_status() {
  mapfile -t resp < <(api_request GET "/v1/executions/worker-queue/summary")
  local code="${resp[0]:-000}"
  local body="$(printf '%s\n' "${resp[@]:1}")"
  if [[ "$code" != "200" ]]; then
    echo "worker summary request failed: status=$code body=$body" >&2
    return 1
  fi
  printf '%s\n' "$body"
}

start_lease_renewer() {
  local execution_id="$1"
  local renew_note="${2:-auto-renew}"
  (
    while true; do
      sleep "$LEASE_RENEW_INTERVAL"
      mapfile -t renew_resp < <(api_request POST "/v1/executions/$execution_id/renew-lease" "{\"worker_id\":\"$WORKER_ID\",\"note\":\"$renew_note\"}")
      local renew_code="${renew_resp[0]:-000}"
      local renew_body="$(printf '%s\n' "${renew_resp[@]:1}")"
      case "$renew_code" in
        200)
          ;;
        404|409)
          echo "[queued-worker] stop renewing execution=$execution_id status=$renew_code body=$renew_body" >&2
          break
          ;;
        *)
          echo "[queued-worker] renew lease unexpected status execution=$execution_id status=$renew_code body=$renew_body" >&2
          ;;
      esac
    done
  ) >/dev/null &
  echo $!
}

claim_once() {
  mapfile -t claim_resp < <(api_request POST "/v1/executions/claim-next" "{\"claimed_by\":\"$WORKER_ID\",\"note\":\"auto-claim\"}")
  local claim_code="${claim_resp[0]:-000}"
  local claim_body="$(printf '%s\n' "${claim_resp[@]:1}")"

  case "$claim_code" in
    200)
      local execution_id
      execution_id="$(json_field "$claim_body" "execution_id")"
      echo "[queued-worker] claimed execution=$execution_id"

      local renew_pid
      renew_pid="$(start_lease_renewer "$execution_id")"
      mapfile -t process_resp < <(api_request POST "/v1/executions/$execution_id/process" "{\"processed_by\":\"$WORKER_ID\",\"note\":\"auto-process\"}")
      local process_code="${process_resp[0]:-000}"
      local process_body="$(printf '%s\n' "${process_resp[@]:1}")"
      kill "$renew_pid" 2>/dev/null || true
      wait "$renew_pid" 2>/dev/null || true

      if [[ "$process_code" == "200" ]]; then
        local final_status provider_target
        final_status="$(json_field "$process_body" "status" || true)"
        provider_target="$(json_field "$process_body" "provider_target" || true)"
        echo "[queued-worker] processed execution=$execution_id status=${final_status:-unknown} provider_target=${provider_target:-unknown}"
        return 0
      fi

      echo "[queued-worker] process failed execution=$execution_id status=$process_code body=$process_body" >&2
      return 1
      ;;
    404)
      echo "[queued-worker] idle"
      return 10
      ;;
    *)
      echo "[queued-worker] claim failed status=$claim_code body=$claim_body" >&2
      return 1
      ;;
  esac
}

run_loop() {
  local rc
  while true; do
    if claim_once; then
      continue
    else
      rc=$?
    fi
    if [[ "$rc" -eq 10 ]]; then
      sleep "$POLL_IDLE_SECS"
      continue
    fi
    sleep "$POLL_IDLE_SECS"
  done
}

require_worker_config
cmd="${1:-run}"
case "$cmd" in
  run)
    run_loop
    ;;
  once)
    if claim_once; then
      exit 0
    fi
    rc=$?
    if [[ "$rc" -eq 10 ]]; then
      exit 0
    fi
    exit "$rc"
    ;;
  status)
    print_status
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    echo "unknown command: $cmd" >&2
    usage >&2
    exit 64
    ;;
esac
