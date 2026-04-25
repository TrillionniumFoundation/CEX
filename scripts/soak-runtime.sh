#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

EXECUTION_BASE_URL="${EXECUTION_BASE_URL:-http://127.0.0.1:7003}"
EXECUTION_ADMIN_TOKEN="${EXECUTION_ADMIN_TOKEN:-${CEX_EXECUTION_ADMIN_TOKEN:-${LOCAL_DEV_ADMIN_TOKEN:-local-dev-admin-token}}}"
DURATION_SECONDS="${CEX_SOAK_DURATION_SECONDS:-300}"
INTERVAL_SECONDS="${CEX_SOAK_INTERVAL_SECONDS:-30}"
OUT_DIR="${CEX_SOAK_OUT_DIR:-$CEX_PROJECT_ROOT/run/soak}"
PROVIDER_MODEL="${CEX_PROVIDER_PROBE_MODEL:-}"
SKIP_PROVIDER_PROBE="${CEX_SOAK_SKIP_PROVIDER_PROBE:-0}"

usage() {
  cat <<'EOF'
Usage: scripts/soak-runtime.sh

Runs a bounded local runtime soak: runtime status, native metrics smoke, operator
signals, and worker queue summary every interval. If CEX_PROVIDER_PROBE_MODEL is
set, it runs the live provider probe once at start and once at end rather than on
every loop tick.

Env:
  CEX_SOAK_DURATION_SECONDS       default 300
  CEX_SOAK_INTERVAL_SECONDS       default 30
  CEX_SOAK_OUT_DIR                default run/soak
  CEX_PROVIDER_PROBE_MODEL        optional live provider model for start/end probe
  CEX_SOAK_SKIP_PROVIDER_PROBE=1  disable provider probes even if model is set
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

for pair in "CEX_SOAK_DURATION_SECONDS:$DURATION_SECONDS" "CEX_SOAK_INTERVAL_SECONDS:$INTERVAL_SECONDS"; do
  name="${pair%%:*}"
  value="${pair#*:}"
  if ! [[ "$value" =~ ^[0-9]+$ ]]; then
    echo "invalid $name: $value" >&2
    exit 64
  fi
done
if [[ "$INTERVAL_SECONDS" -eq 0 ]]; then
  echo "invalid CEX_SOAK_INTERVAL_SECONDS: must be > 0" >&2
  exit 64
fi
if [[ "$SKIP_PROVIDER_PROBE" != "0" && "$SKIP_PROVIDER_PROBE" != "1" ]]; then
  echo "invalid CEX_SOAK_SKIP_PROVIDER_PROBE: $SKIP_PROVIDER_PROBE" >&2
  exit 64
fi

cex_require_cmd bash curl jq date
mkdir -p "$OUT_DIR"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
JSONL_PATH="$OUT_DIR/soak-$RUN_ID.jsonl"
SUMMARY_PATH="$OUT_DIR/soak-$RUN_ID.summary.json"

failures=0
iterations=0
provider_probe_failures=0

run_provider_probe() {
  local phase="$1"
  if [[ "$SKIP_PROVIDER_PROBE" == "1" || -z "$PROVIDER_MODEL" ]]; then
    jq -cn --arg phase "$phase" '{phase:$phase,skipped:true,reason:"provider probe not configured"}'
    return 0
  fi

  local probe_file status
  probe_file="$(mktemp)"
  status=0
  bash "$SCRIPT_DIR/probe-openclaw-provider.sh" --model "$PROVIDER_MODEL" --compact >"$probe_file" || status=$?
  jq -cn \
    --arg phase "$phase" \
    --argjson status "$status" \
    --slurpfile probe "$probe_file" \
    '{phase:$phase,skipped:false,exit_code:$status,probe:($probe[0] // {ok:false,status:"missing_probe_output"})}'
  rm -f "$probe_file"
  if [[ "$status" -ne 0 ]]; then
    return 1
  fi
}

append_iteration() {
  local iteration="$1"
  local ts runtime_status_file metrics_file operator_file worker_file
  local runtime_status=0 metrics_status=0 operator_status=0 worker_status=0
  ts="$(date -Iseconds)"
  runtime_status_file="$(mktemp)"
  metrics_file="$(mktemp)"
  operator_file="$(mktemp)"
  worker_file="$(mktemp)"

  bash "$SCRIPT_DIR/runtime-manager-linux.sh" status >"$runtime_status_file" 2>&1 || runtime_status=$?
  bash "$SCRIPT_DIR/smoke-runtime-metrics.sh" >"$metrics_file" 2>&1 || metrics_status=$?
  bash "$SCRIPT_DIR/check-operator-signals.sh" --compact >"$operator_file" 2>&1 || operator_status=$?
  curl -fsS -H "x-admin-token: $EXECUTION_ADMIN_TOKEN" \
    "$EXECUTION_BASE_URL/v1/executions/worker-queue/summary" >"$worker_file" 2>&1 || worker_status=$?

  if [[ "$runtime_status" -ne 0 || "$metrics_status" -ne 0 || "$operator_status" -ne 0 || "$worker_status" -ne 0 ]]; then
    failures=$((failures + 1))
  fi

  jq -cn \
    --arg ts "$ts" \
    --argjson iteration "$iteration" \
    --argjson runtime_status "$runtime_status" \
    --arg runtime_output "$(cat "$runtime_status_file")" \
    --argjson metrics_status "$metrics_status" \
    --arg metrics_output "$(cat "$metrics_file")" \
    --argjson operator_status "$operator_status" \
    --slurpfile operator "$operator_file" \
    --argjson worker_status "$worker_status" \
    --slurpfile worker "$worker_file" \
    '{ts:$ts,iteration:$iteration,
      runtime:{exit_code:$runtime_status,ok:($runtime_status==0),output:$runtime_output},
      metrics:{exit_code:$metrics_status,ok:($metrics_status==0),output:$metrics_output},
      operator:{exit_code:$operator_status,ok:($operator_status==0),snapshot:($operator[0] // null)},
      worker_queue:{exit_code:$worker_status,ok:($worker_status==0),summary:($worker[0] // null)}}' \
    >> "$JSONL_PATH"

  rm -f "$runtime_status_file" "$metrics_file" "$operator_file" "$worker_file"
}

start_epoch="$(date +%s)"
provider_start_file="$(mktemp)"
if ! run_provider_probe start >"$provider_start_file"; then
  provider_probe_failures=$((provider_probe_failures + 1))
fi
cat "$provider_start_file" >> "$JSONL_PATH"
rm -f "$provider_start_file"

end_at=$((SECONDS + DURATION_SECONDS))
while :; do
  iterations=$((iterations + 1))
  append_iteration "$iterations"
  if [[ "$SECONDS" -ge "$end_at" ]]; then
    break
  fi
  sleep "$INTERVAL_SECONDS"
done

provider_end_file="$(mktemp)"
if ! run_provider_probe end >"$provider_end_file"; then
  provider_probe_failures=$((provider_probe_failures + 1))
fi
cat "$provider_end_file" >> "$JSONL_PATH"
rm -f "$provider_end_file"

end_epoch="$(date +%s)"
if [[ "$provider_probe_failures" -gt 0 ]]; then
  failures=$((failures + provider_probe_failures))
fi

jq -cn \
  --arg run_id "$RUN_ID" \
  --arg jsonl "$JSONL_PATH" \
  --argjson started_at_epoch "$start_epoch" \
  --argjson ended_at_epoch "$end_epoch" \
  --argjson duration_seconds "$((end_epoch - start_epoch))" \
  --argjson requested_duration_seconds "$DURATION_SECONDS" \
  --argjson interval_seconds "$INTERVAL_SECONDS" \
  --argjson iterations "$iterations" \
  --argjson failures "$failures" \
  --argjson provider_probe_failures "$provider_probe_failures" \
  '{run_id:$run_id,jsonl_path:$jsonl,started_at_epoch:$started_at_epoch,ended_at_epoch:$ended_at_epoch,duration_seconds:$duration_seconds,requested_duration_seconds:$requested_duration_seconds,interval_seconds:$interval_seconds,iterations:$iterations,failures:$failures,provider_probe_failures:$provider_probe_failures,ok:($failures==0)}' \
  | tee "$SUMMARY_PATH"

if [[ "$failures" -eq 0 ]]; then
  echo "SOAK_OK summary=$SUMMARY_PATH jsonl=$JSONL_PATH"
  exit 0
fi

echo "SOAK_FAILED failures=$failures summary=$SUMMARY_PATH jsonl=$JSONL_PATH" >&2
exit 2
