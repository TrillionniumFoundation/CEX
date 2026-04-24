#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
SUMMARY_FILE="run/session-auth-runtime-activation/last.json"
OUTPUT_MODE="text"
FIELD=""
FAIL_ON_STATUS=""
REQUIRE_CONVERGED=0

usage() {
  cat <<'EOF'
Usage: ./scripts/read-session-auth-runtime-activation-status.sh [--summary-file <path>] [--json|--compact|--field <name>] [--fail-on-status <status>] [--require-converged]

Reads the latest session-auth runtime activation summary and prints a small operator-friendly shape.

Default behavior:
  - reads run/session-auth-runtime-activation/last.json
  - prints a short multi-line text summary

Options:
  --summary-file <path>   Read an explicit summary JSON file
  --json                  Print structured JSON summary
  --compact               Print a single-line key=value summary
  --field <name>          Print a single field from the derived summary (supports dotted paths like overall.status or raw.activation.body.ok)
  --fail-on-status <s>    Exit non-zero when overall.status equals the given value
  --require-converged     Exit non-zero unless overall.converged is true
  -h, --help              Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --summary-file)
      [[ $# -ge 2 ]] || { echo "Error: --summary-file requires a path" >&2; exit 2; }
      SUMMARY_FILE="$2"
      shift 2
      ;;
    --json)
      [[ "$OUTPUT_MODE" == "text" ]] || { echo "Error: only one of --json, --compact, or --field may be used" >&2; exit 2; }
      OUTPUT_MODE="json"
      shift
      ;;
    --compact)
      [[ "$OUTPUT_MODE" == "text" ]] || { echo "Error: only one of --json, --compact, or --field may be used" >&2; exit 2; }
      OUTPUT_MODE="compact"
      shift
      ;;
    --field)
      [[ "$OUTPUT_MODE" == "text" ]] || { echo "Error: only one of --json, --compact, or --field may be used" >&2; exit 2; }
      [[ $# -ge 2 ]] || { echo "Error: --field requires a name" >&2; exit 2; }
      OUTPUT_MODE="field"
      FIELD="$2"
      shift 2
      ;;
    --fail-on-status)
      [[ $# -ge 2 ]] || { echo "Error: --fail-on-status requires a status" >&2; exit 2; }
      FAIL_ON_STATUS="$2"
      shift 2
      ;;
    --require-converged)
      REQUIRE_CONVERGED=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

python3 - "$REPO_ROOT" "$SUMMARY_FILE" "$OUTPUT_MODE" "$FIELD" "$FAIL_ON_STATUS" "$REQUIRE_CONVERGED" <<'PY'
from __future__ import annotations

import json
import shlex
import sys
from pathlib import Path

repo_root = Path(sys.argv[1])
summary_path = Path(sys.argv[2])
if not summary_path.is_absolute():
    summary_path = repo_root / summary_path
output_mode = sys.argv[3]
field = sys.argv[4]
fail_on_status = sys.argv[5]
require_converged = sys.argv[6] == '1'

if not summary_path.exists():
    raise SystemExit(f"Error: summary file not found: {summary_path}")

raw = json.loads(summary_path.read_text(encoding='utf-8'))
overall = raw.get('overall') or {}
activation = raw.get('activation') or {}
activation_body = activation.get('body') or {}
rollback = raw.get('rollback') or {}
rollback_body = rollback.get('body') or {}
pre_status = raw.get('preStatus') or {}
pre_status_body = pre_status.get('body') or {}

summary = {
    'summaryFile': str(summary_path),
    'kind': raw.get('kind'),
    'schemaVersion': raw.get('schemaVersion'),
    'candidateRevision': raw.get('candidateRevision'),
    'previousLiveRevision': raw.get('previousLiveRevision'),
    'approvalCheckStatus': raw.get('approvalCheckStatus'),
    'overall': {
        'status': overall.get('status'),
        'successful': overall.get('successful'),
        'rollbackAttempted': overall.get('rollbackAttempted'),
        'rollbackSuccessful': overall.get('rollbackSuccessful'),
        'converged': overall.get('converged'),
        'requiresAttention': overall.get('requiresAttention'),
        'summaryDisplay': overall.get('summaryDisplay'),
    },
    'activation': {
        'exitCode': activation.get('exitCode'),
        'ok': activation_body.get('ok'),
        'consumerLiveRevision': ((activation_body.get('postStatus') or {}).get('consumerLiveRevision')),
        'matrixLiveRevision': ((activation_body.get('postStatus') or {}).get('matrixLiveRevision')),
        'liveRevisionMatch': ((activation_body.get('postStatus') or {}).get('liveRevisionMatch')),
    },
    'rollback': {
        'exitCode': rollback.get('exitCode'),
        'ok': rollback_body.get('ok'),
        'consumerLiveRevision': ((rollback_body.get('postStatus') or {}).get('consumerLiveRevision')),
        'matrixLiveRevision': ((rollback_body.get('postStatus') or {}).get('matrixLiveRevision')),
        'liveRevisionMatch': ((rollback_body.get('postStatus') or {}).get('liveRevisionMatch')),
    },
    'preStatus': {
        'exitCode': pre_status.get('exitCode'),
        'ok': pre_status_body.get('ok'),
    },
    'raw': raw,
}


def lookup(value, dotted: str):
    current = value
    for part in dotted.split('.'):
        if isinstance(current, dict) and part in current:
            current = current[part]
        else:
            raise KeyError(dotted)
    return current


def format_scalar(value):
    if value is None:
        return 'null'
    if isinstance(value, bool):
        return str(value).lower()
    return str(value)

if output_mode == 'json':
    payload = dict(summary)
    payload.pop('raw', None)
    print(json.dumps(payload, ensure_ascii=False, indent=2))
elif output_mode == 'compact':
    compact_parts = [
        f"status={format_scalar(summary['overall']['status'])}",
        f"successful={format_scalar(summary['overall']['successful'])}",
        f"converged={format_scalar(summary['overall']['converged'])}",
        f"rollback={format_scalar(summary['overall']['rollbackSuccessful'])}",
        f"candidate={format_scalar(summary['candidateRevision'])}",
        f"previous={format_scalar(summary['previousLiveRevision'])}",
        f"summary={shlex.quote(format_scalar(summary['overall']['summaryDisplay']))}",
    ]
    print(' '.join(compact_parts))
elif output_mode == 'field':
    try:
        value = lookup(summary, field)
    except KeyError:
        raise SystemExit(f"Error: unsupported field path: {field}")
    if isinstance(value, (dict, list)):
        print(json.dumps(value, ensure_ascii=False))
    elif value is None:
        print('null')
    elif isinstance(value, bool):
        print(str(value).lower())
    else:
        print(value)
else:
    print(f"summaryFile: {summary['summaryFile']}")
    print(f"kind: {summary['kind']}")
    print(f"schemaVersion: {summary['schemaVersion']}")
    print(f"candidateRevision: {summary['candidateRevision']}")
    print(f"previousLiveRevision: {summary['previousLiveRevision']}")
    print(f"approvalCheckStatus: {summary['approvalCheckStatus']}")
    print(f"status: {summary['overall']['status']}")
    print(f"successful: {summary['overall']['successful']}")
    print(f"converged: {summary['overall']['converged']}")
    print(f"rollbackAttempted: {summary['overall']['rollbackAttempted']}")
    print(f"rollbackSuccessful: {summary['overall']['rollbackSuccessful']}")
    print(f"requiresAttention: {summary['overall']['requiresAttention']}")
    print(f"summary: {summary['overall']['summaryDisplay']}")

if fail_on_status and summary['overall']['status'] == fail_on_status:
    raise SystemExit(1)
if require_converged and summary['overall']['converged'] is not True:
    raise SystemExit(1)
PY
