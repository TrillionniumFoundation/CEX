#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
HISTORY_FILE="run/session-auth-runtime-history/history.jsonl"
OUTPUT_MODE="text"
SUMMARY_VIEW=0
LIMIT=""
LATEST=0
ACTION_FILTER=""
STATUS_FILTER=""
FIELD=""
FAIL_ON_LATEST_STATUS=""
REQUIRE_LATEST_CONVERGED=0
REQUIRE_LATEST_SUCCESSFUL=0

usage() {
  cat <<'EOF'
Usage: ./scripts/read-session-auth-runtime-history.sh [options]

Read append-only activation/rollback history for session-auth runtime authority changes.

Options:
  --history-file <path>         Read an explicit JSONL history file
  --json                        Emit a JSON array
  --jsonl                       Emit one JSON object per line
  --compact                     Emit compact text output
  --field <name>                Emit a single field from one matched event (or summary when --summary is set)
  --summary                     Emit aggregate summary instead of individual events
  --limit <n>                   Keep the first n matched events after filtering (newest first)
  --latest                      Shortcut for --limit 1
  --action <name>               Filter by action (activation|rollback)
  --status <name>               Filter by overall.status
  --fail-on-latest-status <s>   Exit non-zero when the latest matched event has this status
  --require-latest-converged    Exit non-zero unless the latest matched event converged=true
  --require-latest-successful   Exit non-zero unless the latest matched event successful=true
  -h, --help                    Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --history-file)
      [[ $# -ge 2 ]] || { echo "Error: --history-file requires a path" >&2; exit 2; }
      HISTORY_FILE="$2"
      shift 2
      ;;
    --json)
      [[ "$OUTPUT_MODE" == "text" ]] || { echo "Error: only one of --json, --jsonl, --compact, or --field may be used" >&2; exit 2; }
      OUTPUT_MODE="json"
      shift
      ;;
    --jsonl)
      [[ "$OUTPUT_MODE" == "text" ]] || { echo "Error: only one of --json, --jsonl, --compact, or --field may be used" >&2; exit 2; }
      OUTPUT_MODE="jsonl"
      shift
      ;;
    --compact)
      [[ "$OUTPUT_MODE" == "text" ]] || { echo "Error: only one of --json, --jsonl, --compact, or --field may be used" >&2; exit 2; }
      OUTPUT_MODE="compact"
      shift
      ;;
    --field)
      [[ "$OUTPUT_MODE" == "text" ]] || { echo "Error: only one of --json, --jsonl, --compact, or --field may be used" >&2; exit 2; }
      [[ $# -ge 2 ]] || { echo "Error: --field requires a path" >&2; exit 2; }
      OUTPUT_MODE="field"
      FIELD="$2"
      shift 2
      ;;
    --summary)
      SUMMARY_VIEW=1
      shift
      ;;
    --limit)
      [[ $# -ge 2 ]] || { echo "Error: --limit requires a number" >&2; exit 2; }
      LIMIT="$2"
      shift 2
      ;;
    --latest)
      LATEST=1
      shift
      ;;
    --action)
      [[ $# -ge 2 ]] || { echo "Error: --action requires a value" >&2; exit 2; }
      ACTION_FILTER="$2"
      shift 2
      ;;
    --status)
      [[ $# -ge 2 ]] || { echo "Error: --status requires a value" >&2; exit 2; }
      STATUS_FILTER="$2"
      shift 2
      ;;
    --fail-on-latest-status)
      [[ $# -ge 2 ]] || { echo "Error: --fail-on-latest-status requires a value" >&2; exit 2; }
      FAIL_ON_LATEST_STATUS="$2"
      shift 2
      ;;
    --require-latest-converged)
      REQUIRE_LATEST_CONVERGED=1
      shift
      ;;
    --require-latest-successful)
      REQUIRE_LATEST_SUCCESSFUL=1
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

python3 - "$REPO_ROOT" "$HISTORY_FILE" "$OUTPUT_MODE" "$SUMMARY_VIEW" "$LIMIT" "$LATEST" "$ACTION_FILTER" "$STATUS_FILTER" "$FIELD" "$FAIL_ON_LATEST_STATUS" "$REQUIRE_LATEST_CONVERGED" "$REQUIRE_LATEST_SUCCESSFUL" <<'PY'
from __future__ import annotations
import json, shlex, sys
from collections import Counter
from pathlib import Path

repo_root = Path(sys.argv[1])
history_path = Path(sys.argv[2])
if not history_path.is_absolute():
    history_path = repo_root / history_path
output_mode = sys.argv[3]
summary_view = sys.argv[4] == '1'
limit = int(sys.argv[5]) if sys.argv[5] else None
latest = sys.argv[6] == '1'
action_filter = sys.argv[7]
status_filter = sys.argv[8]
field = sys.argv[9]
fail_on_latest_status = sys.argv[10]
require_latest_converged = sys.argv[11] == '1'
require_latest_successful = sys.argv[12] == '1'

if latest:
    limit = 1

if not history_path.exists():
    raise SystemExit(f"Error: history file not found: {history_path}")

raw_entries = []
for line in history_path.read_text(encoding='utf-8').splitlines():
    line = line.strip()
    if not line:
        continue
    raw_entries.append(json.loads(line))
raw_entries.reverse()

for entry in raw_entries:
    entry['status'] = (entry.get('overall') or {}).get('status')
    entry['successful'] = (entry.get('overall') or {}).get('successful')
    entry['converged'] = (entry.get('overall') or {}).get('converged')
    entry['summaryDisplay'] = (entry.get('overall') or {}).get('summaryDisplay')
    entry['revision'] = entry.get('candidateRevision') if entry.get('action') == 'activation' else entry.get('restoredRevision')

entries = list(raw_entries)
if action_filter:
    entries = [e for e in entries if (e.get('action') or '') == action_filter]
if status_filter:
    entries = [e for e in entries if (e.get('status') or '') == status_filter]
if limit is not None:
    entries = entries[:limit]

latest_entry = entries[0] if entries else None
latest_successful_entry = next((e for e in entries if e.get('successful') is True), None)
latest_failed_entry = next((e for e in entries if e.get('successful') is False), None)
summary = {
    'historyFile': str(history_path),
    'kind': 'session-auth-runtime-history-summary',
    'schemaVersion': 1,
    'filters': {
        'action': action_filter or None,
        'status': status_filter or None,
        'limit': limit,
        'latest': latest,
    },
    'counts': {
        'totalEntries': len(raw_entries),
        'matchedEntries': len(entries),
        'actionCounts': dict(Counter((e.get('action') or 'unknown') for e in entries)),
        'statusCounts': dict(Counter((e.get('status') or 'unknown') for e in entries)),
    },
    'latestEvent': latest_entry,
    'latestSuccessfulEvent': latest_successful_entry,
    'latestFailedEvent': latest_failed_entry,
}
summary['overall'] = {
    'status': (latest_entry or {}).get('status'),
    'successful': (latest_entry or {}).get('successful'),
    'converged': (latest_entry or {}).get('converged'),
    'requiresAttention': False if latest_entry is None else (((latest_entry or {}).get('successful') is not True) or ((latest_entry or {}).get('converged') is False)),
    'summaryDisplay': 'empty' if latest_entry is None else f"latest:{latest_entry.get('action')}:{latest_entry.get('status')}|revision:{latest_entry.get('revision') or '-'}|converged:{latest_entry.get('converged')}",
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


def print_event_lines(target_entries):
    if not target_entries:
        print(f"historyFile: {history_path}")
        print("count: 0")
        return
    print(f"historyFile: {history_path}")
    print(f"count: {len(target_entries)}")
    for idx, entry in enumerate(target_entries, 1):
        print(f"[{idx}] {entry.get('loggedAt')} {entry.get('action')} {entry.get('status')} revision={entry.get('revision')} converged={entry.get('converged')}")

if summary_view:
    if output_mode == 'jsonl':
        raise SystemExit('Error: --summary cannot be combined with --jsonl')
    if output_mode == 'json':
        print(json.dumps(summary, ensure_ascii=False, indent=2))
    elif output_mode == 'compact':
        print(' '.join([
            f"status={format_scalar(summary['overall']['status'])}",
            f"successful={format_scalar(summary['overall']['successful'])}",
            f"converged={format_scalar(summary['overall']['converged'])}",
            f"matched={format_scalar(summary['counts']['matchedEntries'])}",
            f"total={format_scalar(summary['counts']['totalEntries'])}",
            f"summary={shlex.quote(format_scalar(summary['overall']['summaryDisplay']))}",
        ]))
    elif output_mode == 'field':
        try:
            value = lookup(summary, field)
        except KeyError:
            raise SystemExit(f"Error: unsupported field path: {field}")
        if isinstance(value, (dict, list)):
            print(json.dumps(value, ensure_ascii=False))
        elif isinstance(value, bool):
            print(str(value).lower())
        elif value is None:
            print('null')
        else:
            print(value)
    else:
        print(f"historyFile: {summary['historyFile']}")
        print(f"totalEntries: {summary['counts']['totalEntries']}")
        print(f"matchedEntries: {summary['counts']['matchedEntries']}")
        print(f"latestStatus: {summary['overall']['status']}")
        print(f"latestSuccessful: {summary['overall']['successful']}")
        print(f"latestConverged: {summary['overall']['converged']}")
        print(f"requiresAttention: {summary['overall']['requiresAttention']}")
        print(f"summary: {summary['overall']['summaryDisplay']}")
else:
    if output_mode == 'json':
        print(json.dumps(entries, ensure_ascii=False, indent=2))
    elif output_mode == 'jsonl':
        for entry in entries:
            print(json.dumps(entry, ensure_ascii=False))
    elif output_mode == 'compact':
        for entry in entries:
            print(' '.join([
                f"loggedAt={format_scalar(entry.get('loggedAt'))}",
                f"action={format_scalar(entry.get('action'))}",
                f"status={format_scalar(entry.get('status'))}",
                f"successful={format_scalar(entry.get('successful'))}",
                f"converged={format_scalar(entry.get('converged'))}",
                f"revision={format_scalar(entry.get('revision'))}",
                f"summary={shlex.quote(format_scalar(entry.get('summaryDisplay')))}",
            ]))
    elif output_mode == 'field':
        if len(entries) != 1:
            raise SystemExit(f"Error: --field requires exactly one matched event, got {len(entries)}")
        try:
            value = lookup(entries[0], field)
        except KeyError:
            raise SystemExit(f"Error: unsupported field path: {field}")
        if isinstance(value, (dict, list)):
            print(json.dumps(value, ensure_ascii=False))
        elif isinstance(value, bool):
            print(str(value).lower())
        elif value is None:
            print('null')
        else:
            print(value)
    else:
        print_event_lines(entries)

if fail_on_latest_status and (latest_entry or {}).get('status') == fail_on_latest_status:
    raise SystemExit(1)
if require_latest_converged and (latest_entry or {}).get('converged') is not True:
    raise SystemExit(1)
if require_latest_successful and (latest_entry or {}).get('successful') is not True:
    raise SystemExit(1)
PY
