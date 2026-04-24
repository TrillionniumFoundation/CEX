#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
RELOAD_HELPER="$SCRIPT_DIR/reload-session-auth-runtime.sh"

layout_root="${SESSION_AUTH_RUNTIME_LAYOUT_ROOT:-$SCRIPT_DIR/../run/local-runtime}"
default_live_file="${SESSION_AUTH_RUNTIME_LIVE_FILE:-$layout_root/session-auth-issuer-registry.json}"
backup_dir="${SESSION_AUTH_RUNTIME_BACKUP_DIR:-$SCRIPT_DIR/../run/session-auth-runtime-backups}"
summary_file="${SESSION_AUTH_RUNTIME_ROLLBACK_SUMMARY_FILE:-$SCRIPT_DIR/../run/session-auth-runtime-rollback/last.json}"
history_file="${SESSION_AUTH_RUNTIME_HISTORY_FILE:-$SCRIPT_DIR/../run/session-auth-runtime-history/history.jsonl}"
live_file=""
backup_file=""
consumer_base_url="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
matrix_base_url="${MATRIX_ENTRY_BASE_URL:-http://127.0.0.1:8091}"
consumer_token="${CONSUMER_ENTRY_INGRESS_TOKEN:-}"
matrix_token="${MATRIX_ENTRY_INGRESS_TOKEN:-}"
compact=0
print_defaults=0

usage() {
  cat <<'EOF'
Usage: ./scripts/rollback-session-auth-runtime.sh [options]

Restore a repo-local session-auth issuer registry backup into the live path and run coordinated consumer+matrix reload.

Options:
  --backup-file <path>      Backup JSON file to restore (default: latest matching pre-activate backup)
  --live-file <path>        Live issuer-registry JSON file to overwrite (default: run/local-runtime/session-auth-issuer-registry.json)
  --backup-dir <path>       Directory to search for backups
  --consumer-base-url <u>   Consumer base URL
  --matrix-base-url <u>     Matrix base URL
  --consumer-token <token>  x-entry-token for consumer admin endpoints
  --matrix-token <token>    x-entry-token for matrix admin endpoints
  --summary-file <path>     Write machine-readable summary JSON to this file
  --history-file <path>     Append a machine-readable history event JSONL to this file
  --compact                 Emit single-line JSON
  --print-defaults          Print resolved default paths as JSON and exit
  -h, --help                Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --backup-file)
      backup_file="${2:-}"
      shift 2
      ;;
    --live-file)
      live_file="${2:-}"
      shift 2
      ;;
    --backup-dir)
      backup_dir="${2:-}"
      shift 2
      ;;
    --consumer-base-url)
      consumer_base_url="${2:-}"
      shift 2
      ;;
    --matrix-base-url)
      matrix_base_url="${2:-}"
      shift 2
      ;;
    --consumer-token)
      consumer_token="${2:-}"
      shift 2
      ;;
    --matrix-token)
      matrix_token="${2:-}"
      shift 2
      ;;
    --summary-file)
      summary_file="${2:-}"
      shift 2
      ;;
    --history-file)
      history_file="${2:-}"
      shift 2
      ;;
    --compact)
      compact=1
      shift
      ;;
    --print-defaults)
      print_defaults=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Error: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$live_file" ]]; then
  live_file="$default_live_file"
fi

resolve_latest_backup() {
  local target_name
  target_name="$(basename "$live_file")"
  python3 - "$backup_dir" "$target_name" <<'PY'
from pathlib import Path
import sys
backup_dir = Path(sys.argv[1])
target_name = sys.argv[2]
if not backup_dir.exists():
    print("")
    raise SystemExit(0)
pattern = f"{target_name}.pre-activate.*.bak.json"
matches = sorted(backup_dir.glob(pattern))
print(matches[-1] if matches else "")
PY
}

if [[ -z "$backup_file" ]]; then
  backup_file="$(resolve_latest_backup)"
fi

if [[ "$print_defaults" -eq 1 ]]; then
  python3 - <<PY
import json
print(json.dumps({
  "kind": "session-auth-runtime-rollback-defaults",
  "schemaVersion": 1,
  "layoutRoot": ${layout_root@Q},
  "liveFile": ${live_file@Q},
  "backupDir": ${backup_dir@Q},
  "backupFile": ${backup_file@Q},
  "summaryFile": ${summary_file@Q},
  "historyFile": ${history_file@Q},
}, ensure_ascii=False, indent=None if ${compact} else 2))
PY
  exit 0
fi

if [[ ! -x "$RELOAD_HELPER" ]]; then
  echo "Error: reload helper is not executable: $RELOAD_HELPER" >&2
  exit 1
fi
if [[ -z "$backup_file" ]]; then
  echo "Error: no matching backup file found" >&2
  exit 1
fi
if [[ ! -f "$backup_file" ]]; then
  echo "Error: backup file not found: $backup_file" >&2
  exit 1
fi

json_get() {
  local path="$1"
  local expr="$2"
  python3 - "$path" "$expr" <<'PY'
import json, pathlib, sys
path, expr = sys.argv[1:3]
obj = json.loads(pathlib.Path(path).read_text(encoding='utf-8'))
cur = obj
for part in expr.split('.'):
    if not part:
        continue
    if isinstance(cur, dict):
        cur = cur.get(part)
    else:
        cur = None
        break
if cur is None:
    print("")
elif isinstance(cur, (dict, list)):
    print(json.dumps(cur, ensure_ascii=False))
else:
    print(cur)
PY
}

run_reload_helper() {
  local helper_action="$1"
  local helper_service="$2"
  local tmp_out
  tmp_out=$(mktemp)
  local exit_code=0
  set +e
  "$RELOAD_HELPER" \
    --action "$helper_action" \
    --service "$helper_service" \
    --consumer-base-url "$consumer_base_url" \
    --matrix-base-url "$matrix_base_url" \
    --consumer-token "$consumer_token" \
    --matrix-token "$matrix_token" \
    --compact >"$tmp_out"
  exit_code=$?
  set -e
  python3 - "$tmp_out" "$exit_code" <<'PY'
import json, pathlib, sys
path, exit_code = sys.argv[1:3]
text = pathlib.Path(path).read_text(encoding='utf-8').strip()
body = json.loads(text) if text else None
print(json.dumps({"exitCode": int(exit_code), "body": body}, ensure_ascii=False))
PY
  rm -f "$tmp_out"
}

pre_status=$(run_reload_helper status both)
backup_revision=$(json_get "$backup_file" revision)
live_revision=""
if [[ -f "$live_file" ]]; then
  live_revision=$(json_get "$live_file" revision)
fi

mkdir -p "$(dirname "$live_file")"
cp "$backup_file" "$live_file"
rollback_result=$(run_reload_helper reload both)
rollback_ok=$(python3 -c 'import json,sys; data=json.load(sys.stdin); body=data.get("body") or {}; sys.exit(0 if data.get("exitCode")==0 and body.get("ok") is True else 1)' <<<"$rollback_result" && echo 1 || echo 0)

summary_json=$(python3 - "$layout_root" "$live_file" "$backup_dir" "$backup_file" "$backup_revision" "$live_revision" "$pre_status" "$rollback_result" <<'PY'
import json, sys
(
    layout_root,
    live_file,
    backup_dir,
    backup_file,
    backup_revision,
    previous_live_revision,
    pre_status_json,
    rollback_result_json,
) = sys.argv[1:9]
pre_status = json.loads(pre_status_json) if pre_status_json else None
rollback_result = json.loads(rollback_result_json) if rollback_result_json else None
rollback_body = (rollback_result or {}).get('body') or {}
rollback_ok = (rollback_result or {}).get('exitCode') == 0 and rollback_body.get('ok') is True
converged = rollback_body.get('postStatus', {}).get('liveRevisionMatch')
status = 'rolled_back' if rollback_ok else 'rollback_failed'
summary = {
    'kind': 'session-auth-runtime-rollback-summary',
    'schemaVersion': 1,
    'layout': {
        'layoutRoot': layout_root,
        'backupDir': backup_dir,
    },
    'liveFile': live_file,
    'backupFile': backup_file,
    'restoredRevision': backup_revision or None,
    'previousLiveRevision': previous_live_revision or None,
    'preStatus': pre_status,
    'rollback': rollback_result,
    'overall': {
        'status': status,
        'successful': rollback_ok,
        'converged': converged,
        'requiresAttention': (not rollback_ok) or (converged is False),
        'summaryDisplay': f"{status}|restored:{backup_revision or '-'}|previous:{previous_live_revision or '-'}|converged:{converged if converged is not None else 'unknown'}",
    },
}
print(json.dumps(summary, ensure_ascii=False))
PY
)

mkdir -p "$(dirname "$summary_file")"
printf '%s\n' "$summary_json" > "$summary_file"
if [[ -n "$history_file" ]]; then
  mkdir -p "$(dirname "$history_file")"
  python3 - "$history_file" "$summary_file" "$summary_json" <<'PY'
import json, sys, datetime
history_file, summary_file, summary_json = sys.argv[1:4]
summary = json.loads(summary_json)
event = {
    "kind": "session-auth-runtime-history-event",
    "schemaVersion": 1,
    "loggedAt": datetime.datetime.now(datetime.timezone.utc).isoformat().replace('+00:00', 'Z'),
    "action": "rollback",
    "summaryKind": summary.get("kind"),
    "summaryFile": summary_file or None,
    "overall": summary.get("overall") or {},
    "restoredRevision": summary.get("restoredRevision"),
    "previousLiveRevision": summary.get("previousLiveRevision"),
}
with open(history_file, 'a', encoding='utf-8') as fh:
    fh.write(json.dumps(event, ensure_ascii=False) + "\n")
PY
fi
if [[ "$compact" -eq 1 ]]; then
  printf '%s\n' "$summary_json"
else
  python3 -c 'import json,sys; print(json.dumps(json.loads(sys.stdin.read()), ensure_ascii=False, indent=2))' <<<"$summary_json"
fi

if [[ "$rollback_ok" -eq 1 ]]; then
  exit 0
fi
exit 1
