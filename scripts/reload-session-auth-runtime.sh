#!/usr/bin/env bash
set -euo pipefail

consumer_base_url="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
matrix_base_url="${MATRIX_ENTRY_BASE_URL:-http://127.0.0.1:8091}"
consumer_token="${CONSUMER_ENTRY_INGRESS_TOKEN:-}"
matrix_token="${MATRIX_ENTRY_INGRESS_TOKEN:-}"
action="validate"
service="both"
compact=0

usage() {
    cat <<'EOF'
Usage: ./scripts/reload-session-auth-runtime.sh [options]

Coordinate consumer-entry verifier and matrix-entry signer runtime session-auth registry actions.

Options:
  --action <status|validate|reload>   Action to run (default: validate)
  --service <consumer|matrix|both>    Target service set (default: both)
  --consumer-base-url <url>           Consumer base URL (default: $CONSUMER_ENTRY_BASE_URL or http://127.0.0.1:8090)
  --matrix-base-url <url>             Matrix base URL (default: $MATRIX_ENTRY_BASE_URL or http://127.0.0.1:8091)
  --consumer-token <token>            x-entry-token for consumer admin endpoints
  --matrix-token <token>              x-entry-token for matrix admin endpoints
  --compact                           Emit single-line JSON
  --help                              Show this help

Notes:
  - For coordinated reloads, consumer runs first and matrix runs second.
  - After a coordinated reload, the helper fetches both live status surfaces and reports whether revisions match.
  - Tokens default from CONSUMER_ENTRY_INGRESS_TOKEN / MATRIX_ENTRY_INGRESS_TOKEN.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --action)
            action="${2:-}"
            shift 2
            ;;
        --service)
            service="${2:-}"
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
        --compact)
            compact=1
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

case "$action" in
    status|validate|reload) ;;
    *)
        echo "Error: --action must be one of: status, validate, reload" >&2
        exit 1
        ;;
esac

case "$service" in
    consumer|matrix|both) ;;
    *)
        echo "Error: --service must be one of: consumer, matrix, both" >&2
        exit 1
        ;;
esac

consumer_action_endpoint() {
    case "$action" in
        status) echo "$consumer_base_url/v1/admin/session-auth/issuer-registry/status" ;;
        validate) echo "$consumer_base_url/v1/admin/session-auth/issuer-registry/validate" ;;
        reload) echo "$consumer_base_url/v1/admin/session-auth/issuer-registry/reload" ;;
    esac
}

matrix_action_endpoint() {
    case "$action" in
        status) echo "$matrix_base_url/v1/admin/consumer-entry-session-auth/status" ;;
        validate) echo "$matrix_base_url/v1/admin/consumer-entry-session-auth/validate" ;;
        reload) echo "$matrix_base_url/v1/admin/consumer-entry-session-auth/reload" ;;
    esac
}

consumer_status_endpoint() {
    echo "$consumer_base_url/v1/admin/session-auth/issuer-registry/status"
}

matrix_status_endpoint() {
    echo "$matrix_base_url/v1/admin/consumer-entry-session-auth/status"
}

http_method() {
    case "$action" in
        status) echo "GET" ;;
        validate|reload) echo "POST" ;;
    esac
}

call_endpoint() {
    local name="$1"
    local method="$2"
    local url="$3"
    local token="$4"

    if [[ -z "$token" ]]; then
        python3 - "$name" "$url" <<'PY'
import json, sys
name, url = sys.argv[1:3]
print(json.dumps({
    "service": name,
    "url": url,
    "ok": False,
    "statusCode": None,
    "error": "missing_token"
}))
PY
        return 0
    fi

    local body_file
    body_file=$(mktemp)
    local status_code
    status_code=$(curl -sS -o "$body_file" -w '%{http_code}' -X "$method" -H "x-entry-token: $token" "$url" || true)

    python3 - "$name" "$url" "$status_code" "$body_file" <<'PY'
import json, pathlib, sys
name, url, status_code_raw, body_path = sys.argv[1:5]
text = pathlib.Path(body_path).read_text(encoding='utf-8')
try:
    body = json.loads(text) if text.strip() else None
except Exception:
    body = {"raw": text}
try:
    status_code = int(status_code_raw)
except Exception:
    status_code = None
ok = status_code is not None and 200 <= status_code < 300
print(json.dumps({
    "service": name,
    "url": url,
    "ok": ok,
    "statusCode": status_code,
    "body": body,
}))
PY
    rm -f "$body_file"
}

build_skipped_result() {
    local name="$1"
    local url="$2"
    local reason="$3"
    python3 - "$name" "$url" "$reason" <<'PY'
import json, sys
name, url, reason = sys.argv[1:4]
print(json.dumps({
    "service": name,
    "url": url,
    "ok": False,
    "statusCode": None,
    "error": reason,
    "skipped": True,
}))
PY
}

json_ok() {
    python3 -c 'import json,sys; sys.exit(0 if json.load(sys.stdin).get("ok") else 1)'
}

append_result() {
    local file="$1"
    local json="$2"
    printf '%s\n' "$json" >> "$file"
}

run_generic_action() {
    local method="$1"
    local results_file="$2"

    case "$service" in
        consumer)
            append_result "$results_file" "$(call_endpoint consumer "$method" "$(consumer_action_endpoint)" "$consumer_token")"
            ;;
        matrix)
            append_result "$results_file" "$(call_endpoint matrix "$method" "$(matrix_action_endpoint)" "$matrix_token")"
            ;;
        both)
            append_result "$results_file" "$(call_endpoint consumer "$method" "$(consumer_action_endpoint)" "$consumer_token")"
            append_result "$results_file" "$(call_endpoint matrix "$method" "$(matrix_action_endpoint)" "$matrix_token")"
            ;;
    esac
}

run_coordinated_reload() {
    local results_file="$1"
    local post_status_file="$2"

    local consumer_result
    consumer_result=$(call_endpoint consumer POST "$(consumer_action_endpoint)" "$consumer_token")
    append_result "$results_file" "$consumer_result"

    local matrix_result
    if json_ok <<<"$consumer_result"; then
        matrix_result=$(call_endpoint matrix POST "$(matrix_action_endpoint)" "$matrix_token")
    else
        matrix_result=$(build_skipped_result matrix "$(matrix_action_endpoint)" "blocked_by_consumer_reload_failure")
    fi
    append_result "$results_file" "$matrix_result"

    append_result "$post_status_file" "$(call_endpoint consumer GET "$(consumer_status_endpoint)" "$consumer_token")"
    append_result "$post_status_file" "$(call_endpoint matrix GET "$(matrix_status_endpoint)" "$matrix_token")"
}

summarize_results() {
    local results_file="$1"
    local post_status_file="$2"
    python3 - "$action" "$service" "$results_file" "$post_status_file" <<'PY'
import json, pathlib, sys

action, service, results_path, post_status_path = sys.argv[1:5]

def load_lines(path):
    if not path or not pathlib.Path(path).exists():
        return []
    items = []
    for line in pathlib.Path(path).read_text(encoding='utf-8').splitlines():
        line = line.strip()
        if not line:
            continue
        items.append(json.loads(line))
    return items

def deep_get(obj, path):
    cur = obj
    for key in path:
        if not isinstance(cur, dict):
            return None
        cur = cur.get(key)
    return cur

results = load_lines(results_path)
post_status = load_lines(post_status_path)
primary_ok = all(item.get('ok') for item in results)
post_status_ok = all(item.get('ok') for item in post_status) if post_status else None

consumer_live_revision = None
matrix_live_revision = None
for item in post_status:
    body = item.get('body')
    if item.get('service') == 'consumer':
        consumer_live_revision = deep_get(body, ['session_auth_issuer_registry', 'metadata', 'revision'])
    elif item.get('service') == 'matrix':
        matrix_live_revision = deep_get(body, ['consumer_entry_session_auth', 'issuer_registry_metadata', 'revision'])

revision_match = None
if consumer_live_revision is not None and matrix_live_revision is not None:
    revision_match = consumer_live_revision == matrix_live_revision

overall_ok = primary_ok
if post_status_ok is False:
    overall_ok = False
if revision_match is False:
    overall_ok = False

summary = {
    'action': action,
    'service': service,
    'coordinated': action == 'reload' and service == 'both',
    'ok': overall_ok,
    'serviceCount': len(results),
    'failedServices': [item.get('service') for item in results if not item.get('ok')],
    'executionOrder': [item.get('service') for item in results],
    'results': results,
}

if action == 'reload' and service == 'both':
    summary['coordination'] = {
        'mode': 'consumer_then_matrix',
        'matrixBlockedByConsumerFailure': any(
            item.get('service') == 'matrix' and item.get('error') == 'blocked_by_consumer_reload_failure'
            for item in results
        ),
        'postStatusCollected': bool(post_status),
    }

if post_status:
    summary['postStatus'] = {
        'ok': post_status_ok,
        'failedServices': [item.get('service') for item in post_status if not item.get('ok')],
        'consumerLiveRevision': consumer_live_revision,
        'matrixLiveRevision': matrix_live_revision,
        'liveRevisionMatch': revision_match,
        'results': post_status,
    }

print(json.dumps(summary, ensure_ascii=False))
PY
}

results_file=$(mktemp)
post_status_file=$(mktemp)
trap 'rm -f "$results_file" "$post_status_file"' EXIT

if [[ "$action" == "reload" && "$service" == "both" ]]; then
    run_coordinated_reload "$results_file" "$post_status_file"
else
    run_generic_action "$(http_method)" "$results_file"
fi

result_json=$(summarize_results "$results_file" "$post_status_file")

if [[ "$compact" -eq 1 ]]; then
    printf '%s\n' "$result_json"
else
    python3 -c 'import json,sys; print(json.dumps(json.loads(sys.stdin.read()), ensure_ascii=False, indent=2))' <<<"$result_json"
fi

python3 -c 'import json,sys; sys.exit(0 if json.load(sys.stdin).get("ok") else 1)' <<<"$result_json"
