#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

CEX_SIGNOFF_SCOPE="${CEX_SIGNOFF_SCOPE:-linux-self-hosted-local-production}"
CEX_SIGNOFF_SOAK_SUMMARY_PATH="${CEX_SIGNOFF_SOAK_SUMMARY_PATH:-}"
CEX_SIGNOFF_MIN_SOAK_SECONDS="${CEX_SIGNOFF_MIN_SOAK_SECONDS:-7200}"
CEX_SIGNOFF_MAX_EVIDENCE_AGE_SECONDS="${CEX_SIGNOFF_MAX_EVIDENCE_AGE_SECONDS:-86400}"
CEX_SIGNOFF_REQUIRE_GIT_CLEAN="${CEX_SIGNOFF_REQUIRE_GIT_CLEAN:-1}"
CEX_SIGNOFF_OUT_DIR="${CEX_SIGNOFF_OUT_DIR:-$CEX_PROJECT_ROOT/run/signoff}"
CEX_PROVIDER_PROBE_MODEL="${CEX_PROVIDER_PROBE_MODEL:-google/gemini-2.5-flash}"
CONSUMER_ENTRY_BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
CEX_MONITORING_DEPLOY_METADATA_PATH="${CEX_MONITORING_DEPLOY_METADATA_PATH:-$CEX_PROJECT_ROOT/run/monitoring-live-target/metadata/monitoring-deploy-metadata.yml}"

usage() {
  cat <<'EOF'
Usage: scripts/check-production-signoff.sh

Runs the final scoped production signoff gate for the Linux self-hosted CEX
launch profile. This is intentionally stricter than a single readiness smoke:
  - repository clean check (default on),
  - production readiness smoke with required live provider probe,
  - fresh 2h+ soak evidence,
  - fresh DB restore drill and monitoring deploy evidence via readiness.

The signoff scope is explicit: linux-self-hosted-local-production. Non-launch
providers must be blocked by policy, and provider probe must target the intended
launch provider.

Env:
  CEX_ENV_FILE                              production-posture env file
  CEX_PROVIDER_PROBE_MODEL                 launch provider model
  CEX_SIGNOFF_SOAK_SUMMARY_PATH            optional explicit soak summary
  CEX_SIGNOFF_MIN_SOAK_SECONDS             default 7200
  CEX_SIGNOFF_MAX_EVIDENCE_AGE_SECONDS     default 86400
  CEX_SIGNOFF_REQUIRE_GIT_CLEAN=0           skip git clean requirement
  CEX_SIGNOFF_OUT_DIR                      default run/signoff
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

cex_require_cmd bash jq git python3
mkdir -p "$CEX_SIGNOFF_OUT_DIR"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
SUMMARY_PATH="$CEX_SIGNOFF_OUT_DIR/production-signoff-$RUN_ID.summary.json"
READINESS_LOG="$CEX_SIGNOFF_OUT_DIR/production-signoff-$RUN_ID.readiness.log"
ROUTE_RUNNER_HANDOFF_EVIDENCE_PATH="$CEX_SIGNOFF_OUT_DIR/production-signoff-$RUN_ID.route-runner-handoff.json"
ROUTE_RUNNER_HANDOFF_MONITORING_EVIDENCE_PATH="$CEX_SIGNOFF_OUT_DIR/production-signoff-$RUN_ID.route-runner-handoff-monitoring.json"

failures=0
failure_messages=()

record_failure() {
  failures=$((failures + 1))
  failure_messages+=("$*")
  printf 'FAIL %s\n' "$*" >&2
}

pass() {
  printf 'OK %s\n' "$*"
}

repo_status=""
if [[ "$CEX_SIGNOFF_REQUIRE_GIT_CLEAN" == "1" ]]; then
  repo_status="$(git -C "$CEX_PROJECT_ROOT" status --short)"
  if [[ -n "$repo_status" ]]; then
    record_failure 'repository is not clean'
  else
    pass 'repository is clean'
  fi
else
  pass 'repository clean check skipped'
fi

readiness_status=0
if CEX_PROVIDER_PROBE_MODEL="$CEX_PROVIDER_PROBE_MODEL" bash "$SCRIPT_DIR/check-production-readiness.sh" >"$READINESS_LOG" 2>&1; then
  pass 'production readiness passed'
else
  readiness_status=$?
  record_failure "production readiness failed (see $READINESS_LOG)"
fi

soak_summary_path="$CEX_SIGNOFF_SOAK_SUMMARY_PATH"
if [[ -z "$soak_summary_path" ]]; then
  soak_summary_path="$(find "$CEX_PROJECT_ROOT/run/soak" -maxdepth 1 -type f -name 'soak-*.summary.json' -printf '%T@ %p\n' 2>/dev/null | sort -nr | awk 'NR==1 { $1=""; sub(/^ /, ""); print }')"
fi

soak_ok=false
soak_age=-1
if [[ -z "$soak_summary_path" || ! -f "$soak_summary_path" ]]; then
  record_failure '2h soak summary is missing'
else
  if python3 - "$soak_summary_path" "$CEX_SIGNOFF_MIN_SOAK_SECONDS" "$CEX_SIGNOFF_MAX_EVIDENCE_AGE_SECONDS" <<'PY'
from pathlib import Path
import json, sys, time
path = Path(sys.argv[1])
min_seconds = int(sys.argv[2])
max_age = int(sys.argv[3])
data = json.loads(path.read_text())
if data.get('ok') is not True:
    raise SystemExit('soak summary ok=false')
if int(data.get('requested_duration_seconds') or 0) < min_seconds:
    raise SystemExit('soak requested duration below minimum')
if int(data.get('duration_seconds') or 0) < min_seconds:
    raise SystemExit('soak actual duration below minimum')
if int(data.get('failures') or 0) != 0:
    raise SystemExit('soak failures nonzero')
if int(data.get('provider_probe_failures') or 0) != 0:
    raise SystemExit('soak provider probe failures nonzero')
age = int(time.time()) - int(data.get('ended_at_epoch') or 0)
if age < 0 or age > max_age:
    raise SystemExit(f'soak summary stale age={age}s max={max_age}s')
PY
  then
    soak_ok=true
    soak_age="$(python3 - "$soak_summary_path" <<'PY'
from pathlib import Path
import json, sys, time
data = json.loads(Path(sys.argv[1]).read_text())
print(int(time.time()) - int(data.get('ended_at_epoch') or 0))
PY
)"
    pass "2h soak evidence fresh (${soak_age}s old, $soak_summary_path)"
  else
    record_failure "2h soak summary is not successful/fresh ($soak_summary_path)"
  fi
fi

route_runner_handoff_evidence_ok=false
route_runner_handoff_status=0
route_runner_handoff_health_file="$(mktemp)"
if curl -fsS "$CONSUMER_ENTRY_BASE_URL/health" >"$route_runner_handoff_health_file" && \
  python3 - "$route_runner_handoff_health_file" "$ROUTE_RUNNER_HANDOFF_EVIDENCE_PATH" <<'PY'
from pathlib import Path
import json
import sys

health_path = Path(sys.argv[1])
out_path = Path(sys.argv[2])
health = json.loads(health_path.read_text())

def gate_ok(gate):
    if not isinstance(gate, dict):
        return False
    return (
        gate.get('contract_version') == 'trillionnium_playability_route_runner_handoff_gate_v1'
        and gate.get('feed_contract_visible') is True
        and gate.get('map_hub_contract_visible') is True
        and int(gate.get('source_count') or 0) >= 7
        and gate.get('sources_include_route_runner_handoff') is True
        and gate.get('feed_handoff_contract_version') == 'trillionnium_route_runner_handoff_v1'
        and gate.get('map_hub_handoff_contract_version') == 'trillionnium_route_runner_handoff_v1'
        and int(gate.get('runner_count') or 0) >= 1
        and int(gate.get('reward_claim_action_count') or 0) >= 1
        and int(gate.get('next_route_action_count') or 0) >= 1
        and bool(gate.get('first_next_route_status'))
        and bool(gate.get('first_next_route_sequence_summary'))
        and bool(gate.get('handoff_prompt'))
    )

gate_sources = {
    'playability': (health.get('trillionnium_world_playability_scorecard') or {}).get('route_runner_handoff_gate') or {},
    'closed_beta': (health.get('trillionnium_world_closed_beta_prototype') or {}).get('route_runner_handoff_gate') or {},
    'real_user_beta': (health.get('trillionnium_world_real_user_beta') or {}).get('route_runner_handoff_gate') or {},
    'public_commercial': (health.get('trillionnium_world_public_commercial_product') or {}).get('route_runner_handoff_gate') or {},
}
gate_results = {name: gate_ok(gate) for name, gate in gate_sources.items()}
primary_gate = gate_sources['playability']
evidence = {
    'ok': all(gate_results.values()),
    'contract_version': 'trillionnium_signoff_route_runner_handoff_evidence_v1',
    'source': f"{health.get('service') or 'consumer-entry-api'}/health",
    'health_status': health.get('status'),
    'gate_results': gate_results,
    'gate_names': list(gate_sources.keys()),
    'source_count': primary_gate.get('source_count'),
    'runner_count': primary_gate.get('runner_count'),
    'reward_claim_action_count': primary_gate.get('reward_claim_action_count'),
    'next_route_action_count': primary_gate.get('next_route_action_count'),
    'first_next_route_status': primary_gate.get('first_next_route_status'),
    'first_next_route_sequence_summary': primary_gate.get('first_next_route_sequence_summary'),
    'handoff_prompt': primary_gate.get('handoff_prompt'),
    'gates': gate_sources,
}
out_path.write_text(json.dumps(evidence, indent=2, sort_keys=True) + '\n')
if not evidence['ok']:
    missing = [name for name, ok in gate_results.items() if not ok]
    raise SystemExit(f'route-runner handoff signoff evidence not green: {missing}')
PY
then
  route_runner_handoff_evidence_ok=true
  pass "route-runner handoff signoff evidence green ($ROUTE_RUNNER_HANDOFF_EVIDENCE_PATH)"
else
  route_runner_handoff_status=$?
  record_failure "route-runner handoff signoff evidence failed (status=$route_runner_handoff_status path=$ROUTE_RUNNER_HANDOFF_EVIDENCE_PATH)"
fi
rm -f "$route_runner_handoff_health_file"

route_runner_handoff_monitoring_evidence_ok=false
route_runner_handoff_monitoring_status=0
if bash "$SCRIPT_DIR/check-trillionnium-route-runner-handoff-monitoring.sh" \
  --metadata "$CEX_MONITORING_DEPLOY_METADATA_PATH" \
  --max-age-seconds "$CEX_SIGNOFF_MAX_EVIDENCE_AGE_SECONDS" \
  --summary-file "$ROUTE_RUNNER_HANDOFF_MONITORING_EVIDENCE_PATH" \
  --contract-version "trillionnium_signoff_route_runner_handoff_monitoring_evidence_v1" \
  --quiet; then
  route_runner_handoff_monitoring_evidence_ok=true
  pass "route-runner handoff monitoring signoff evidence green ($ROUTE_RUNNER_HANDOFF_MONITORING_EVIDENCE_PATH)"
else
  route_runner_handoff_monitoring_status=$?
  record_failure "route-runner handoff monitoring signoff evidence failed (status=$route_runner_handoff_monitoring_status path=$ROUTE_RUNNER_HANDOFF_MONITORING_EVIDENCE_PATH)"
fi

head_commit="$(git -C "$CEX_PROJECT_ROOT" rev-parse --short HEAD)"
ended_at_epoch="$(date +%s)"

python3 - "$SUMMARY_PATH" "$RUN_ID" "$CEX_SIGNOFF_SCOPE" "$head_commit" "$failures" "$readiness_status" "$READINESS_LOG" "$soak_summary_path" "$soak_ok" "$soak_age" "$CEX_PROVIDER_PROBE_MODEL" "$ended_at_epoch" "$ROUTE_RUNNER_HANDOFF_EVIDENCE_PATH" "$route_runner_handoff_evidence_ok" "$route_runner_handoff_status" "$ROUTE_RUNNER_HANDOFF_MONITORING_EVIDENCE_PATH" "$route_runner_handoff_monitoring_evidence_ok" "$route_runner_handoff_monitoring_status" <<'PY'
from pathlib import Path
import json, sys
summary_path = Path(sys.argv[1])
failures = int(sys.argv[5])
route_runner_handoff_evidence_path = Path(sys.argv[13])
route_runner_handoff_evidence = None
if route_runner_handoff_evidence_path.exists():
    route_runner_handoff_evidence = json.loads(route_runner_handoff_evidence_path.read_text())
route_runner_handoff_monitoring_evidence_path = Path(sys.argv[16])
route_runner_handoff_monitoring_evidence = None
if route_runner_handoff_monitoring_evidence_path.exists():
    route_runner_handoff_monitoring_evidence = json.loads(route_runner_handoff_monitoring_evidence_path.read_text())
summary = {
    'ok': failures == 0,
    'kind': 'production_signoff',
    'run_id': sys.argv[2],
    'scope': sys.argv[3],
    'head_commit': sys.argv[4],
    'failures': failures,
    'readiness': {
        'exit_code': int(sys.argv[6]),
        'log_path': sys.argv[7],
        'ok': int(sys.argv[6]) == 0,
    },
    'soak': {
        'summary_path': sys.argv[8],
        'ok': sys.argv[9] == 'true',
        'age_seconds': None if sys.argv[10] == '-1' else int(sys.argv[10]),
    },
    'provider_probe_model': sys.argv[11],
    'ended_at_epoch': int(sys.argv[12]),
    'route_runner_handoff_evidence': {
        'summary_path': sys.argv[13],
        'ok': sys.argv[14] == 'true',
        'exit_code': int(sys.argv[15]),
        'evidence': route_runner_handoff_evidence,
    },
    'route_runner_handoff_monitoring_evidence': {
        'summary_path': sys.argv[16],
        'ok': sys.argv[17] == 'true',
        'exit_code': int(sys.argv[18]),
        'evidence': route_runner_handoff_monitoring_evidence,
    },
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + '\n')
PY

if [[ "$failures" -eq 0 ]]; then
  echo "SIGNOFF_READY scope=$CEX_SIGNOFF_SCOPE summary=$SUMMARY_PATH"
  exit 0
fi

printf 'SIGNOFF_NOT_READY failures=%s summary=%s\n' "$failures" "$SUMMARY_PATH" >&2
for msg in "${failure_messages[@]}"; do
  printf '  - %s\n' "$msg" >&2
done
exit 2
