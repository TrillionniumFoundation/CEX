#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
BASE_URL="${BASE_URL%/}"
SUMMARY_DIR="$CEX_PROJECT_ROOT/run/game-account-auth"
SUMMARY_FILE=""
QUIET="false"
MUTATING_SMOKE="${CEX_GAME_ACCOUNT_AUTH_MUTATING_SMOKE:-0}"
CONTRACT_VERSION="trillionnium_game_account_auth_readiness_gate_v1"

usage() {
  cat <<'EOF'
Usage: scripts/check-trillionnium-game-account-auth.sh [--base-url <url>] [--summary-file <path>] [--mutating] [--quiet]

Validates the Trillionnium game-account register/login client contract.

Default mode is non-mutating and checks:
  - /account renders the account client contract and embedded readiness JSON,
  - register/login/session/logout endpoints are advertised,
  - browser credential storage stays disabled,
  - public_launch_credit remains false,
  - /account/session without a cookie reports active=false,
  - password auth posture is coherent when enabled.

Mutating mode is opt-in through --mutating or CEX_GAME_ACCOUNT_AUTH_MUTATING_SMOKE=1.
It requires password auth to be enabled, registers one unique test account, verifies
session/logout, checks the registry stores Argon2id rather than plaintext, and proves
repeated bad login attempts eventually hit the auth rate limit.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-url)
      [[ $# -ge 2 ]] || { echo "Error: --base-url requires a value" >&2; exit 2; }
      BASE_URL="${2%/}"
      shift 2
      ;;
    --summary-file)
      [[ $# -ge 2 ]] || { echo "Error: --summary-file requires a path" >&2; exit 2; }
      SUMMARY_FILE="$2"
      shift 2
      ;;
    --contract-version)
      [[ $# -ge 2 ]] || { echo "Error: --contract-version requires a value" >&2; exit 2; }
      CONTRACT_VERSION="$2"
      shift 2
      ;;
    --mutating)
      MUTATING_SMOKE="1"
      shift
      ;;
    --quiet)
      QUIET="true"
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

cex_require_cmd curl jq python3
mkdir -p "$SUMMARY_DIR"
CHECKED_AT="$(date +%s)"

if [[ -z "$SUMMARY_FILE" ]]; then
  SUMMARY_FILE="$SUMMARY_DIR/game-account-auth-summary-$CHECKED_AT.json"
fi
summary_tmp="$(mktemp)"
tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
  rm -f "$summary_tmp"
}
trap cleanup EXIT

client_headers="$tmpdir/account.headers"
client_html="$tmpdir/account.html"
session_json="$tmpdir/account-session.json"
metrics_txt="$tmpdir/metrics.txt"
client_http="$(curl -sS -D "$client_headers" -o "$client_html" -w '%{http_code}' "$BASE_URL/account" || printf '000')"
session_http="$(curl -sS -o "$session_json" -w '%{http_code}' "$BASE_URL/account/session" || printf '000')"
metrics_http="$(curl -sS -o "$metrics_txt" -w '%{http_code}' "$BASE_URL/metrics" || printf '000')"

python3 - "$client_headers" "$client_html" "$session_json" "$metrics_txt" "$summary_tmp" "$BASE_URL" "$CHECKED_AT" "$CONTRACT_VERSION" "$client_http" "$session_http" "$metrics_http" "${CEX_READINESS_MODE:-}" <<'PY'
import html
import json
import re
import sys
from pathlib import Path

headers_path = Path(sys.argv[1])
html_path = Path(sys.argv[2])
session_path = Path(sys.argv[3])
metrics_path = Path(sys.argv[4])
summary_path = Path(sys.argv[5])
base_url = sys.argv[6]
checked_at = int(sys.argv[7])
contract_version = sys.argv[8]
client_http = sys.argv[9]
session_http = sys.argv[10]
metrics_http = sys.argv[11]
readiness_mode = sys.argv[12] or "unknown"

failures = []

def require(check_id, passed, detail=None):
    if not passed:
        failures.append({"check_id": check_id, "detail": detail})

def read_text(path):
    try:
        return path.read_text()
    except FileNotFoundError:
        return ""

headers_text = read_text(headers_path)
client_body = read_text(html_path)
session_text = read_text(session_path)
metrics_text = read_text(metrics_path)
client_header_contract = ""
for line in headers_text.splitlines():
    if ":" not in line:
        continue
    name, value = line.split(":", 1)
    if name.strip().lower() == "x-trillionnium-resource-contract":
        client_header_contract = value.strip()
        break

readiness = {}
match = re.search(
    r'<script id="account-client-readiness" type="application/json">(.*?)</script>',
    client_body,
    re.S,
)
if match:
    try:
        readiness = json.loads(html.unescape(match.group(1)))
    except json.JSONDecodeError as exc:
        require("readiness_json_parse", False, str(exc))
else:
    require("readiness_json_present", False, "missing #account-client-readiness script")

try:
    session = json.loads(session_text) if session_text.strip() else {}
except json.JSONDecodeError as exc:
    session = {}
    require("session_json_parse", False, str(exc))

require("account_http_200", client_http == "200", {"http_status": client_http})
require("session_http_200", session_http == "200", {"http_status": session_http})
require("metrics_http_200", metrics_http == "200", {"http_status": metrics_http})
require(
    "account_resource_contract_header",
    client_header_contract == "trillionnium_game_account_client_v1",
    client_header_contract,
)
require(
    "account_body_contract",
    'data-contract="trillionnium_game_account_client_v1"' in client_body,
    None,
)
require(
    "account_readiness_contract",
    readiness.get("contract_version") == "trillionnium_game_account_client_v1",
    readiness.get("contract_version"),
)

expected_paths = {
    "client_surface": "/account",
    "alternate_surface": "/game/account",
    "session_status_endpoint": "/account/session",
    "register_endpoint": "/account/register",
    "login_endpoint": "/account/login",
    "logout_endpoint": "/account/logout",
}
for key, value in expected_paths.items():
    require(f"readiness_{key}", readiness.get(key) == value, readiness.get(key))

flows = readiness.get("flows") if isinstance(readiness.get("flows"), dict) else {}
register = flows.get("register") if isinstance(flows.get("register"), dict) else {}
login = flows.get("login") if isinstance(flows.get("login"), dict) else {}
logout = flows.get("logout") if isinstance(flows.get("logout"), dict) else {}
for flow_name, flow in (("register", register), ("login", login)):
    require(
        f"{flow_name}_password_auth_implemented",
        flow.get("password_auth_implemented") is True,
        flow.get("password_auth_implemented"),
    )
    require(
        f"{flow_name}_credential_storage_disabled",
        flow.get("credential_storage_in_browser") is False,
        flow.get("credential_storage_in_browser"),
    )
    require(f"{flow_name}_password_hash_argon2id", flow.get("password_hash") == "argon2id", flow.get("password_hash"))
require("logout_endpoint_visible", logout.get("endpoint") == "/account/logout", logout.get("endpoint"))
require("logout_client_clears_profile", logout.get("client_clears_local_profile") is True, logout.get("client_clears_local_profile"))

production_boundary = readiness.get("production_boundary") if isinstance(readiness.get("production_boundary"), dict) else {}
require(
    "public_launch_credit_false",
    production_boundary.get("public_launch_credit") is False,
    production_boundary.get("public_launch_credit"),
)
require(
    "client_submits_intent_only",
    production_boundary.get("client_submits_intent_only") is True,
    production_boundary.get("client_submits_intent_only"),
)

password_auth = readiness.get("password_auth") if isinstance(readiness.get("password_auth"), dict) else {}
observability = readiness.get("observability") if isinstance(readiness.get("observability"), dict) else {}
password_auth_enabled = password_auth.get("enabled") is True
auth_limit = password_auth.get("auth_rate_limit_max_requests")
auth_window = password_auth.get("auth_rate_limit_window_secs")
min_chars = password_auth.get("minimum_password_chars")
registry_persistence = password_auth.get("registry_persistence")
require("password_min_chars_at_least_8", isinstance(min_chars, int) and min_chars >= 8, min_chars)
require("auth_rate_limit_positive", isinstance(auth_limit, int) and auth_limit > 0, auth_limit)
require("auth_rate_limit_window_positive", isinstance(auth_window, int) and auth_window > 0, auth_window)
require(
    "observability_contract",
    observability.get("contract_version") == "trillionnium_game_account_auth_observability_v1",
    observability.get("contract_version"),
)
require("observability_no_secret_logging", observability.get("passwords_tokens_or_cookie_values_logged") is False, observability.get("passwords_tokens_or_cookie_values_logged"))
for metric_name in (
    "cex_consumer_entry_game_account_register_successes_total",
    "cex_consumer_entry_game_account_login_successes_total",
    "cex_consumer_entry_game_account_login_failures_total",
    "cex_consumer_entry_game_account_logout_successes_total",
    "cex_consumer_entry_game_account_auth_rate_limited_total",
):
    require(f"metric_visible_{metric_name}", metric_name in metrics_text, None)
if password_auth_enabled:
    require(
        "password_auth_registry_persistent_when_enabled",
        isinstance(registry_persistence, str) and registry_persistence not in ("", "memory_only"),
        registry_persistence,
    )
else:
    require(
        "signed_upstream_required_when_password_auth_disabled",
        production_boundary.get("requires_signed_upstream_user_session_when_password_auth_disabled") is True
        or readiness_mode == "local",
        production_boundary.get("requires_signed_upstream_user_session_when_password_auth_disabled"),
    )

require(
    "session_status_contract",
    session.get("contract_version") == "trillionnium_game_account_session_status_v1",
    session.get("contract_version"),
)
require("session_no_cookie_inactive", session.get("active") is False, session.get("active"))
session_password_auth = session.get("password_auth") if isinstance(session.get("password_auth"), dict) else {}
require("session_password_auth_implemented", session_password_auth.get("implemented") is True, session_password_auth.get("implemented"))
require("session_public_launch_credit_false", session.get("public_launch_credit") is False, session.get("public_launch_credit"))

summary = {
    "kind": "trillionnium_game_account_auth_readiness",
    "contract_version": contract_version,
    "checked_at_epoch": checked_at,
    "base_url": base_url,
    "readiness_mode": readiness_mode,
    "ok": len(failures) == 0,
    "failures": failures,
    "client_http_status": client_http,
    "session_http_status": session_http,
    "metrics_http_status": metrics_http,
    "client_header_contract": client_header_contract,
    "client_contract": readiness.get("contract_version"),
    "session_contract": session.get("contract_version"),
    "password_auth_enabled": password_auth_enabled,
    "password_auth_implemented": register.get("password_auth_implemented") is True and login.get("password_auth_implemented") is True,
    "registry_persistence": registry_persistence,
    "minimum_password_chars": min_chars,
    "auth_rate_limit_max_requests": auth_limit,
    "auth_rate_limit_window_secs": auth_window,
    "public_launch_credit": production_boundary.get("public_launch_credit"),
    "client_submits_intent_only": production_boundary.get("client_submits_intent_only"),
    "observability_contract": observability.get("contract_version"),
    "observability_no_secret_logging": observability.get("passwords_tokens_or_cookie_values_logged"),
    "session_without_cookie_active": session.get("active"),
    "mutating_smoke": {"requested": False, "ok": None, "skipped": True},
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
PY

if [[ "$MUTATING_SMOKE" == "1" ]]; then
  password_auth_enabled="$(jq -r '.password_auth_enabled // false' "$summary_tmp")"
  mutation_tmp="$(mktemp)"
  trap 'rm -rf "$tmpdir"; rm -f "$summary_tmp" "$mutation_tmp"' EXIT

  if [[ "$password_auth_enabled" != "true" ]]; then
    jq '.mutating_smoke = {
          "requested": true,
          "ok": false,
          "skipped": false,
          "failures": [{"check_id": "mutating_password_auth_enabled", "detail": "password auth is disabled"}]
        }
        | .failures += .mutating_smoke.failures
        | .ok = ((.failures | length) == 0)' "$summary_tmp" >"$mutation_tmp"
    mv "$mutation_tmp" "$summary_tmp"
  else
    cookie_jar="$tmpdir/account.cookies"
    register_json="$tmpdir/register.json"
    session_after_register_json="$tmpdir/session-after-register.json"
    logout_json="$tmpdir/logout.json"
    bad_login_json="$tmpdir/bad-login.json"
    handle="gate-$CHECKED_AT-$$-$RANDOM"
    display_name="Gate $CHECKED_AT"
    password="Trillionnium-$CHECKED_AT-$$-$RANDOM-pass9"
    room_id="!gate-$CHECKED_AT:trillionnium.local"
    session_id="gate-smoke-$CHECKED_AT"
    auth_limit="$(jq -r '.auth_rate_limit_max_requests // 5' "$summary_tmp")"
    if ! [[ "$auth_limit" =~ ^[0-9]+$ ]] || [[ "$auth_limit" -lt 1 ]]; then
      auth_limit=5
    fi
    rate_attempts=$((auth_limit + 1))
    if [[ "$rate_attempts" -gt 20 ]]; then
      rate_attempts=20
    fi
    source_octet=$((RANDOM % 200 + 1))
    source_ip="203.0.113.$source_octet"

    register_payload="$(jq -n --arg handle "$handle" --arg display_name "$display_name" --arg password "$password" --arg room_id "$room_id" --arg session_id "$session_id" '{handle: $handle, display_name: $display_name, password: $password, room_id: $room_id, session_id: $session_id}')"
    register_http="$(curl -sS -o "$register_json" -w '%{http_code}' -c "$cookie_jar" -H 'content-type: application/json' --data "$register_payload" "$BASE_URL/account/register" || printf '000')"
    session_after_register_http="$(curl -sS -o "$session_after_register_json" -w '%{http_code}' -b "$cookie_jar" "$BASE_URL/account/session" || printf '000')"
    logout_http="$(curl -sS -o "$logout_json" -w '%{http_code}' -b "$cookie_jar" -c "$cookie_jar" -X POST "$BASE_URL/account/logout" || printf '000')"

    rate_limit_http=""
    bad_payload="$(jq -n --arg handle "$handle" --arg password "wrong-$password" --arg room_id "$room_id" --arg session_id "bad-$session_id" '{handle: $handle, password: $password, room_id: $room_id, session_id: $session_id}')"
    for _attempt in $(seq 1 "$rate_attempts"); do
      rate_limit_http="$(curl -sS -o "$bad_login_json" -w '%{http_code}' -H 'content-type: application/json' -H "x-forwarded-for: $source_ip" --data "$bad_payload" "$BASE_URL/account/login" || printf '000')"
    done

    registry_persistence="$(jq -r '.registry_persistence // ""' "$summary_tmp")"
    registry_argon2id=false
    registry_plaintext_absent=false
    registry_checked=false
    if [[ -n "$registry_persistence" && "$registry_persistence" != "memory_only" && -f "$registry_persistence" ]]; then
      registry_checked=true
      if grep -Fq '$argon2id$' "$registry_persistence"; then
        registry_argon2id=true
      fi
      if ! grep -Fq "$password" "$registry_persistence"; then
        registry_plaintext_absent=true
      fi
    fi

    python3 - "$summary_tmp" "$mutation_tmp" "$register_json" "$session_after_register_json" "$logout_json" "$bad_login_json" "$register_http" "$session_after_register_http" "$logout_http" "$rate_limit_http" "$registry_checked" "$registry_argon2id" "$registry_plaintext_absent" "$rate_attempts" <<'PY'
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
out_path = Path(sys.argv[2])
register_path = Path(sys.argv[3])
session_path = Path(sys.argv[4])
logout_path = Path(sys.argv[5])
bad_login_path = Path(sys.argv[6])
register_http = sys.argv[7]
session_http = sys.argv[8]
logout_http = sys.argv[9]
rate_limit_http = sys.argv[10]
registry_checked = sys.argv[11] == "true"
registry_argon2id = sys.argv[12] == "true"
registry_plaintext_absent = sys.argv[13] == "true"
rate_attempts = int(sys.argv[14])

def read_json(path):
    try:
        text = path.read_text()
        return json.loads(text) if text.strip() else {}
    except Exception:
        return {}

summary = json.loads(summary_path.read_text())
register = read_json(register_path)
session = read_json(session_path)
logout = read_json(logout_path)
bad_login = read_json(bad_login_path)
failures = []

def require(check_id, passed, detail=None):
    if not passed:
        failures.append({"check_id": check_id, "detail": detail})

require("mutating_register_http_200", register_http == "200", {"http_status": register_http, "body": register})
require(
    "mutating_register_contract",
    register.get("contract_version") == "trillionnium_game_account_password_auth_v1",
    register.get("contract_version"),
)
require("mutating_session_http_200", session_http == "200", {"http_status": session_http, "body": session})
require("mutating_session_active", session.get("active") is True, session.get("active"))
require("mutating_logout_http_200", logout_http == "200", {"http_status": logout_http, "body": logout})
require("mutating_logout_kind", logout.get("kind") == "game_account_logout", logout.get("kind"))
require("mutating_rate_limit_429", rate_limit_http == "429", {"http_status": rate_limit_http, "body": bad_login, "attempts": rate_attempts})
require("mutating_registry_checked", registry_checked, None)
require("mutating_registry_argon2id", registry_argon2id, None)
require("mutating_registry_plaintext_absent", registry_plaintext_absent, None)

mutation = {
    "requested": True,
    "skipped": False,
    "ok": len(failures) == 0,
    "failures": failures,
    "register_http_status": register_http,
    "session_http_status": session_http,
    "logout_http_status": logout_http,
    "rate_limit_http_status": rate_limit_http,
    "rate_limit_attempts": rate_attempts,
    "registry_checked": registry_checked,
    "registry_argon2id": registry_argon2id,
    "registry_plaintext_absent": registry_plaintext_absent,
}
summary["mutating_smoke"] = mutation
summary["failures"].extend(failures)
summary["ok"] = len(summary["failures"]) == 0
out_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
PY
    mv "$mutation_tmp" "$summary_tmp"
  fi
fi

cp "$summary_tmp" "$SUMMARY_FILE"

if [[ "$QUIET" != "true" ]]; then
  if jq -e '.ok == true' "$SUMMARY_FILE" >/dev/null; then
    printf 'OK game account auth readiness gate passed (%s)\n' "$SUMMARY_FILE"
  else
    printf 'FAIL game account auth readiness gate failed (%s)\n' "$SUMMARY_FILE" >&2
    jq -r '.failures[]? | "  - " + (.check_id // "unknown") + (if .detail == null then "" else " " + (.detail | tostring) end)' "$SUMMARY_FILE" >&2
  fi
fi

if jq -e '.ok == true' "$SUMMARY_FILE" >/dev/null; then
  exit 0
fi
exit 2
