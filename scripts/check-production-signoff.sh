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
print(age)
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

head_commit="$(git -C "$CEX_PROJECT_ROOT" rev-parse --short HEAD)"
ended_at_epoch="$(date +%s)"

python3 - "$SUMMARY_PATH" "$RUN_ID" "$CEX_SIGNOFF_SCOPE" "$head_commit" "$failures" "$readiness_status" "$READINESS_LOG" "$soak_summary_path" "$soak_ok" "$soak_age" "$CEX_PROVIDER_PROBE_MODEL" "$ended_at_epoch" <<'PY'
from pathlib import Path
import json, sys
summary_path = Path(sys.argv[1])
failures = int(sys.argv[5])
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
