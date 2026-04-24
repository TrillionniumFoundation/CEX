#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

DEPLOY_ROOT="run/monitoring-live-target"
METADATA_FILE=""
OUTPUT_MODE="text"
FIELD=""
FAIL_ON_SEVERITY=""

usage() {
  cat <<'EOF'
Usage: scripts/read-monitoring-deploy-status.sh [--deploy-root <dir>] [--metadata-file <path>] [--json|--compact|--field <name>] [--fail-on-severity <warn|error>]

Reads monitoring deploy metadata and prints the current post-deploy verdict in a small operator-friendly shape.

Default behavior:
  - reads <deploy-root>/metadata/monitoring-deploy-metadata.yml
  - prints a short multi-line text summary

Options:
  --deploy-root <dir>      Override deploy root (default: run/monitoring-live-target)
  --metadata-file <path>   Read an explicit metadata file instead of deploy-root/metadata/monitoring-deploy-metadata.yml
  --json                   Print a compact structured JSON object
  --compact                Print a single-line key=value summary
  --field <name>           Print a single field from the derived summary (supports dotted paths like overall.status or raw.postDeployActions.overall.status)
  --fail-on-severity <s>   Exit non-zero when overall.severity is at or above the given threshold (warn or error)
  -h, --help               Show this help

Examples:
  ./scripts/read-monitoring-deploy-status.sh
  ./scripts/read-monitoring-deploy-status.sh --deploy-root /tmp/cex-monitoring-live-target
  ./scripts/read-monitoring-deploy-status.sh --compact
  ./scripts/read-monitoring-deploy-status.sh --json
  ./scripts/read-monitoring-deploy-status.sh --field overall.operatorDisplay
  ./scripts/read-monitoring-deploy-status.sh --compact --fail-on-severity warn
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --deploy-root)
      [[ $# -ge 2 ]] || { echo "Error: --deploy-root requires a directory" >&2; exit 2; }
      DEPLOY_ROOT="$2"
      shift 2
      ;;
    --metadata-file)
      [[ $# -ge 2 ]] || { echo "Error: --metadata-file requires a path" >&2; exit 2; }
      METADATA_FILE="$2"
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
    --fail-on-severity)
      [[ $# -ge 2 ]] || { echo "Error: --fail-on-severity requires warn or error" >&2; exit 2; }
      FAIL_ON_SEVERITY="$2"
      shift 2
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

if [[ -z "$METADATA_FILE" ]]; then
  METADATA_FILE="$DEPLOY_ROOT/metadata/monitoring-deploy-metadata.yml"
fi

case "$FAIL_ON_SEVERITY" in
  ""|warn|error) ;;
  *)
    echo "Error: --fail-on-severity must be warn or error" >&2
    exit 2
    ;;
esac

python3 - "$REPO_ROOT" "$METADATA_FILE" "$OUTPUT_MODE" "$FIELD" "$FAIL_ON_SEVERITY" <<'PY'
from __future__ import annotations

import json
import shlex
import sys
from pathlib import Path

import yaml

repo_root = Path(sys.argv[1])
metadata_path = Path(sys.argv[2])
if not metadata_path.is_absolute():
    metadata_path = repo_root / metadata_path
output_mode = sys.argv[3]
field = sys.argv[4]
fail_on_severity = sys.argv[5]

if not metadata_path.exists():
    raise SystemExit(f"Error: metadata file not found: {metadata_path}")

raw = yaml.safe_load(metadata_path.read_text(encoding='utf-8')) or {}
post = raw.get('postDeployActions') or {}
overall = post.get('overall') or {}
reload_state = post.get('reload') or {}
verify_state = post.get('verify') or {}

summary = {
    'metadataFile': str(metadata_path),
    'deployedAt': raw.get('deployedAt'),
    'postActionsUpdatedAt': post.get('updatedAt'),
    'overall': {
        'status': overall.get('status'),
        'severity': overall.get('severity'),
        'requiresAttention': overall.get('requiresAttention'),
        'successful': overall.get('successful'),
        'summaryDisplay': overall.get('summaryDisplay'),
        'operatorDisplay': overall.get('operatorDisplay'),
        'nextActionHint': overall.get('nextActionHint'),
        'requestedActions': overall.get('requestedActions') or [],
        'completedActions': overall.get('completedActions') or [],
        'failedActions': overall.get('failedActions') or [],
        'pendingActions': overall.get('pendingActions') or [],
    },
    'reload': {
        'requested': reload_state.get('requested'),
        'completed': reload_state.get('completed'),
        'successful': reload_state.get('successful'),
    },
    'verify': {
        'requested': verify_state.get('requested'),
        'completed': verify_state.get('completed'),
        'successful': verify_state.get('successful'),
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
    overall_block = summary['overall']
    compact_parts = [
        f"status={format_scalar(overall_block['status'])}",
        f"severity={format_scalar(overall_block['severity'])}",
        f"attention={format_scalar(overall_block['requiresAttention'])}",
        f"successful={format_scalar(overall_block['successful'])}",
        f"reload={format_scalar(reload_state.get('successful'))}",
        f"verify={format_scalar(verify_state.get('successful'))}",
        f"operator={shlex.quote(format_scalar(overall_block['operatorDisplay']))}",
        f"next={shlex.quote(format_scalar(overall_block['nextActionHint']))}",
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
    overall_block = summary['overall']
    print(f"metadata: {summary['metadataFile']}")
    print(f"deployedAt: {summary['deployedAt']}")
    print(f"postActionsUpdatedAt: {summary['postActionsUpdatedAt']}")
    print(f"status: {overall_block['status']}")
    print(f"severity: {overall_block['severity']}")
    print(f"requiresAttention: {overall_block['requiresAttention']}")
    print(f"successful: {overall_block['successful']}")
    print(f"summary: {overall_block['summaryDisplay']}")
    print(f"operator: {overall_block['operatorDisplay']}")
    print(f"nextAction: {overall_block['nextActionHint']}")
    print(f"requestedActions: {','.join(overall_block['requestedActions'])}")
    print(f"failedActions: {','.join(overall_block['failedActions'])}")
    print(f"pendingActions: {','.join(overall_block['pendingActions'])}")

severity_rank = {'ok': 0, 'warn': 1, 'error': 2}
if fail_on_severity:
    current = summary['overall'].get('severity')
    if current not in severity_rank:
        raise SystemExit(f"Error: unsupported current severity: {current}")
    if severity_rank[current] >= severity_rank[fail_on_severity]:
        raise SystemExit(1)
PY
