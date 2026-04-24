#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
RELOAD_HELPER="$SCRIPT_DIR/reload-session-auth-runtime.sh"

layout_root="${SESSION_AUTH_RUNTIME_LAYOUT_ROOT:-$SCRIPT_DIR/../run/local-runtime}"
default_candidate_dir="${SESSION_AUTH_RUNTIME_CANDIDATE_DIR:-$layout_root/session-auth-candidates}"
default_live_file="${SESSION_AUTH_RUNTIME_LIVE_FILE:-$layout_root/session-auth-issuer-registry.json}"
default_approved_revisions_file="${SESSION_AUTH_RUNTIME_APPROVED_REVISIONS_FILE:-$layout_root/session-auth-issuer-registry-approved-revisions.json}"
candidate_file=""
live_file=""
approved_revisions_file=""
backup_dir="${SESSION_AUTH_RUNTIME_BACKUP_DIR:-$SCRIPT_DIR/../run/session-auth-runtime-backups}"
consumer_base_url="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
matrix_base_url="${MATRIX_ENTRY_BASE_URL:-http://127.0.0.1:8091}"
consumer_token="${CONSUMER_ENTRY_INGRESS_TOKEN:-}"
matrix_token="${MATRIX_ENTRY_INGRESS_TOKEN:-}"
dry_run=0
allow_unapproved=0
compact=0
print_defaults=0
summary_file="${SESSION_AUTH_RUNTIME_SUMMARY_FILE:-$SCRIPT_DIR/../run/session-auth-runtime-activation/last.json}"
history_file="${SESSION_AUTH_RUNTIME_HISTORY_FILE:-$SCRIPT_DIR/../run/session-auth-runtime-history/history.jsonl}"

usage() {
    cat <<'EOF'
Usage: ./scripts/activate-session-auth-runtime.sh [options]

Promote a candidate repo-local session-auth issuer registry file into the live path,
run coordinated consumer+matrix runtime reload, and roll back the live file if convergence fails.

Options:
  --candidate-file <path>             Candidate issuer-registry JSON file (required; basename resolves under default candidate dir)
  --live-file <path>                  Live issuer-registry JSON file to overwrite (default: run/local-runtime/session-auth-issuer-registry.json)
  --approved-revisions-file <path>    Optional approved-revisions JSON source (default: run/local-runtime/session-auth-issuer-registry-approved-revisions.json when present)
  --backup-dir <path>                 Backup directory for pre-activation live file snapshot
  --consumer-base-url <url>           Consumer base URL
  --matrix-base-url <url>             Matrix base URL
  --consumer-token <token>            x-entry-token for consumer admin endpoints
  --matrix-token <token>              x-entry-token for matrix admin endpoints
  --summary-file <path>               Write machine-readable summary JSON to this file
  --history-file <path>               Append a machine-readable history event JSONL to this file
  --allow-unapproved                  Skip approved-revision gating in file precheck
  --dry-run                           Do not write the live file or call reload
  --compact                           Emit single-line JSON
  --print-defaults                    Print the resolved layout/default paths as JSON and exit
  --help                              Show this help

Notes:
  - Activation writes the candidate file into the live path before coordinated reload.
  - If coordinated reload fails, the helper restores the pre-activation live file and runs coordinated reload again.
  - Success requires both services to converge on the same live revision.
  - By default the latest summary is written to ./run/session-auth-runtime-activation/last.json.
  - Default layout root is ./run/local-runtime, with candidate dir ./run/local-runtime/session-auth-candidates.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --candidate-file)
            candidate_file="${2:-}"
            shift 2
            ;;
        --live-file)
            live_file="${2:-}"
            shift 2
            ;;
        --approved-revisions-file)
            approved_revisions_file="${2:-}"
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
        --allow-unapproved)
            allow_unapproved=1
            shift
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        --compact)
            compact=1
            shift
            ;;
        --print-defaults)
            print_defaults=1
            shift
            ;;
        --help|-h)
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
if [[ -z "$approved_revisions_file" && -f "$default_approved_revisions_file" ]]; then
    approved_revisions_file="$default_approved_revisions_file"
fi
if [[ -n "$candidate_file" && "$candidate_file" != /* && ! -f "$candidate_file" && -f "$default_candidate_dir/$candidate_file" ]]; then
    candidate_file="$default_candidate_dir/$candidate_file"
fi

if [[ "$print_defaults" -eq 1 ]]; then
    python3 - <<PY
import json
print(json.dumps({
    "kind": "session-auth-runtime-activation-defaults",
    "schemaVersion": 1,
    "layoutRoot": ${layout_root@Q},
    "candidateDir": ${default_candidate_dir@Q},
    "liveFile": ${live_file@Q},
    "approvedRevisionsFile": ${approved_revisions_file@Q},
    "backupDir": ${backup_dir@Q},
    "summaryFile": ${summary_file@Q},
    "historyFile": ${history_file@Q},
}, ensure_ascii=False, indent=None if ${compact} else 2))
PY
    exit 0
fi

if [[ -z "$candidate_file" ]]; then
    echo "Error: --candidate-file is required" >&2
    exit 1
fi
if [[ ! -x "$RELOAD_HELPER" ]]; then
    echo "Error: reload helper is not executable: $RELOAD_HELPER" >&2
    exit 1
fi
if [[ ! -f "$candidate_file" ]]; then
    echo "Error: candidate file not found: $candidate_file" >&2
    exit 1
fi

json_get() {
    local path="$1"
    local expr="$2"
    python3 - "$path" "$expr" <<'PY'
import json, pathlib, sys
path, expr = sys.argv[1:3]
text = pathlib.Path(path).read_text(encoding='utf-8')
obj = json.loads(text)
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

candidate_revision=$(json_get "$candidate_file" "revision")
if [[ -z "$candidate_revision" ]]; then
    echo "Error: candidate file does not contain a revision" >&2
    exit 1
fi

live_revision=""
if [[ -f "$live_file" ]]; then
    live_revision=$(json_get "$live_file" "revision")
fi

approval_check_status="not_configured"
approval_check_ok=1
if [[ -n "$approved_revisions_file" ]]; then
    if [[ ! -f "$approved_revisions_file" ]]; then
        echo "Error: approved revisions file not found: $approved_revisions_file" >&2
        exit 1
    fi
    if python3 - "$approved_revisions_file" "$candidate_revision" <<'PY'
import json, pathlib, sys
path, revision = sys.argv[1:3]
obj = json.loads(pathlib.Path(path).read_text(encoding='utf-8'))
approved = [str(x).strip() for x in obj.get('approved_revisions', []) if str(x).strip()]
sys.exit(0 if revision in approved else 1)
PY
    then
        approval_check_status="approved"
    else
        if [[ "$allow_unapproved" -eq 1 ]]; then
            approval_check_status="unapproved_but_allowed"
        else
            approval_check_status="unapproved"
            approval_check_ok=0
        fi
    fi
fi

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

build_summary() {
    local activation_result_json="$1"
    local rollback_result_json="$2"
    python3 - "$layout_root" "$default_candidate_dir" "$candidate_file" "$live_file" "$candidate_revision" "$live_revision" "$approved_revisions_file" "$approval_check_status" "$dry_run" "$activation_result_json" "$rollback_result_json" "$pre_status" <<'PY'
import json, pathlib, sys
(
    layout_root,
    candidate_dir,
    candidate_file,
    live_file,
    candidate_revision,
    live_revision,
    approved_revisions_file,
    approval_check_status,
    dry_run,
    activation_result_json,
    rollback_result_json,
    pre_status_json,
) = sys.argv[1:13]
activation_result = json.loads(activation_result_json) if activation_result_json else None
rollback_result = json.loads(rollback_result_json) if rollback_result_json else None
pre_status = json.loads(pre_status_json) if pre_status_json else None
activation_body = (activation_result or {}).get('body') or {}
rollback_body = (rollback_result or {}).get('body') or {}
activation_ok = (activation_result or {}).get('exitCode') == 0 and activation_body.get('ok') is True
rollback_attempted = rollback_result is not None
rollback_ok = (rollback_result or {}).get('exitCode') == 0 and rollback_body.get('ok') is True
converged = activation_body.get('postStatus', {}).get('liveRevisionMatch')
if converged is None and rollback_body:
    converged = rollback_body.get('postStatus', {}).get('liveRevisionMatch')
status = 'activated' if activation_ok else 'activation_failed'
if dry_run == '1':
    status = 'dry_run'
elif approval_check_status == 'unapproved':
    status = 'approval_precheck_failed'
elif not activation_ok and rollback_attempted and rollback_ok:
    status = 'activation_failed_rolled_back'
summary = {
    "kind": "session-auth-runtime-activation-summary",
    "schemaVersion": 1,
    "layout": {
        "layoutRoot": layout_root,
        "candidateDir": candidate_dir,
    },
    "candidateFile": candidate_file,
    "liveFile": live_file,
    "candidateRevision": candidate_revision,
    "previousLiveRevision": live_revision or None,
    "approvedRevisionsFile": approved_revisions_file or None,
    "approvalCheckStatus": approval_check_status,
    "dryRun": dry_run == '1',
    "preStatus": pre_status,
    "activation": activation_result,
    "rollback": rollback_result,
    "overall": {
        "status": status,
        "successful": activation_ok,
        "rollbackAttempted": rollback_attempted,
        "rollbackSuccessful": rollback_ok,
        "converged": converged,
        "requiresAttention": (not activation_ok) or (converged is False),
        "summaryDisplay": f"{status}|candidate:{candidate_revision}|previous:{live_revision or '-'}|converged:{converged if converged is not None else 'unknown'}|rollback:{'ok' if rollback_ok else ('attempted' if rollback_attempted else 'none')}",
    },
}
print(json.dumps(summary, ensure_ascii=False))
PY
}

emit_summary() {
    local summary_json="$1"
    if [[ -n "$summary_file" ]]; then
        mkdir -p "$(dirname "$summary_file")"
        printf '%s\n' "$summary_json" > "$summary_file"
    fi
    if [[ "$compact" -eq 1 ]]; then
        printf '%s\n' "$summary_json"
    else
        python3 -c 'import json,sys; print(json.dumps(json.loads(sys.stdin.read()), ensure_ascii=False, indent=2))' <<<"$summary_json"
    fi
}

append_history() {
    local summary_json="$1"
    [[ -n "$history_file" ]] || return 0
    mkdir -p "$(dirname "$history_file")"
    python3 - "$history_file" "$summary_file" "$summary_json" <<'PY'
import json, sys, datetime
history_file, summary_file, summary_json = sys.argv[1:4]
summary = json.loads(summary_json)
event = {
    "kind": "session-auth-runtime-history-event",
    "schemaVersion": 1,
    "loggedAt": datetime.datetime.now(datetime.timezone.utc).isoformat().replace('+00:00', 'Z'),
    "action": "activation",
    "summaryKind": summary.get("kind"),
    "summaryFile": summary_file or None,
    "overall": summary.get("overall") or {},
    "candidateRevision": summary.get("candidateRevision"),
    "previousLiveRevision": summary.get("previousLiveRevision"),
    "approvalCheckStatus": summary.get("approvalCheckStatus"),
    "dryRun": summary.get("dryRun"),
}
with open(history_file, 'a', encoding='utf-8') as fh:
    fh.write(json.dumps(event, ensure_ascii=False) + "\n")
PY
}

if [[ "$approval_check_ok" -ne 1 ]]; then
    summary_json=$(build_summary "$(python3 -c 'import json; print(json.dumps({"status":"approval_precheck_failed","ok":False}))')" "")
    emit_summary "$summary_json"
    exit 1
fi

if [[ "$dry_run" -eq 1 ]]; then
    summary_json=$(build_summary "$(python3 -c 'import json,sys; print(json.dumps({"status":"dry_run","ok":True,"message":"candidate precheck passed; no file copy or reload executed"}))')" "")
    emit_summary "$summary_json"
    exit 0
fi

mkdir -p "$backup_dir"
mkdir -p "$(dirname "$live_file")"
backup_path=""
backup_mode="absent"
if [[ -f "$live_file" ]]; then
    backup_mode="file"
    backup_path="$backup_dir/$(basename "$live_file").pre-activate.$(date +%s).bak.json"
    cp "$live_file" "$backup_path"
fi

cp "$candidate_file" "$live_file"
activation_result=$(run_reload_helper reload both)
activation_ok=$(python3 -c 'import json,sys; data=json.load(sys.stdin); body=data.get("body") or {}; sys.exit(0 if data.get("exitCode")==0 and body.get("ok") is True else 1)' <<<"$activation_result" && echo 1 || echo 0)

rollback_result=""
if [[ "$activation_ok" -ne 1 ]]; then
    if [[ "$backup_mode" == "file" ]]; then
        cp "$backup_path" "$live_file"
    else
        rm -f "$live_file"
    fi
    rollback_result=$(run_reload_helper reload both)
fi

summary_json=$(build_summary "$activation_result" "$rollback_result")
emit_summary "$summary_json"
append_history "$summary_json"

if [[ "$activation_ok" -eq 1 ]]; then
    exit 0
fi
exit 1
