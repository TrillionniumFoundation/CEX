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

Validates the Trillionnium game-account register/login/profile/password-change client contract.

Default mode is non-mutating and checks:
  - /account renders the account client contract and embedded readiness JSON,
  - register/login/profile/password-change/session-refresh/session-revoke/session/logout endpoints are advertised,
  - browser credential storage stays disabled,
  - public_launch_credit remains false,
  - /account return_to stays allowlisted and /app plus /world expose Account CTAs,
  - /world exposes the game-account player identity binding contract,
  - /account/session without a cookie reports active=false,
  - password auth posture is coherent when enabled.

Mutating mode is opt-in through --mutating or CEX_GAME_ACCOUNT_AUTH_MUTATING_SMOKE=1.
It requires password auth to be enabled, registers one unique test account, verifies
session/profile/password-change/session-refresh/session-revoke/logout, checks the profile binds into the /world first-human surface, checks the registry stores Argon2id rather than plaintext, and proves
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
account_world_html="$tmpdir/account-world-return.html"
account_unsafe_html="$tmpdir/account-unsafe-return.html"
app_html="$tmpdir/app.html"
world_html="$tmpdir/world.html"
client_http="$(curl -sS -D "$client_headers" -o "$client_html" -w '%{http_code}' "$BASE_URL/account" || printf '000')"
account_world_http="$(curl -sS -o "$account_world_html" -w '%{http_code}' "$BASE_URL/account?return_to=/world" || printf '000')"
account_unsafe_http="$(curl -sS -o "$account_unsafe_html" -w '%{http_code}' "$BASE_URL/account?return_to=https://example.test" || printf '000')"
app_http="$(curl -sS -o "$app_html" -w '%{http_code}' "$BASE_URL/app" || printf '000')"
world_http="$(curl -sS -o "$world_html" -w '%{http_code}' "$BASE_URL/world" || printf '000')"
session_http="$(curl -sS -o "$session_json" -w '%{http_code}' "$BASE_URL/account/session" || printf '000')"
metrics_http="$(curl -sS -o "$metrics_txt" -w '%{http_code}' "$BASE_URL/metrics" || printf '000')"

python3 - "$client_headers" "$client_html" "$session_json" "$metrics_txt" "$account_world_html" "$account_unsafe_html" "$app_html" "$world_html" "$summary_tmp" "$BASE_URL" "$CHECKED_AT" "$CONTRACT_VERSION" "$client_http" "$account_world_http" "$account_unsafe_http" "$app_http" "$world_http" "$session_http" "$metrics_http" "${CEX_READINESS_MODE:-}" <<'PY'
import html
import json
import re
import sys
from pathlib import Path

headers_path = Path(sys.argv[1])
html_path = Path(sys.argv[2])
session_path = Path(sys.argv[3])
metrics_path = Path(sys.argv[4])
account_world_path = Path(sys.argv[5])
account_unsafe_path = Path(sys.argv[6])
app_path = Path(sys.argv[7])
world_path = Path(sys.argv[8])
summary_path = Path(sys.argv[9])
base_url = sys.argv[10]
checked_at = int(sys.argv[11])
contract_version = sys.argv[12]
client_http = sys.argv[13]
account_world_http = sys.argv[14]
account_unsafe_http = sys.argv[15]
app_http = sys.argv[16]
world_http = sys.argv[17]
session_http = sys.argv[18]
metrics_http = sys.argv[19]
readiness_mode = sys.argv[20] or "unknown"

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
account_world_body = read_text(account_world_path)
account_unsafe_body = read_text(account_unsafe_path)
app_body = read_text(app_path)
world_body = read_text(world_path)
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
require("account_return_to_world_http_200", account_world_http == "200", {"http_status": account_world_http})
require("account_return_to_unsafe_http_200", account_unsafe_http == "200", {"http_status": account_unsafe_http})
require("app_http_200", app_http == "200", {"http_status": app_http})
require("world_http_200", world_http == "200", {"http_status": world_http})
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
require("account_default_return_to_app", readiness.get("return_to") == "/app", readiness.get("return_to"))
require("account_default_open_game_app", 'href="/app">Open Game' in client_body, None)
require("account_world_return_to_whitelisted", 'href="/world">Open Game' in account_world_body and '"return_to": "/world"' in account_world_body, None)
require("account_unsafe_return_to_app", 'href="/app">Open Game' in account_unsafe_body and '"return_to": "/app"' in account_unsafe_body and "https://example.test" not in account_unsafe_body, None)
require("app_account_session_bridge", all(token in app_body for token in ["app-account-session-card", "trillionnium_game_account_surface_session_v1", 'data-session-active="false"', 'data-auth-state="signed_session_required"', "/account?return_to=/app"]), None)
require("world_account_session_bridge", all(token in world_body for token in ["world-account-session-card", "trillionnium_game_account_surface_session_v1", 'data-session-active="false"', "/account?return_to=/world"]), None)
require("world_account_identity_binding_contract", all(token in world_body for token in ["trillionnium_game_account_player_identity_binding_v1", 'data-account-profile-bound="false"']), None)

expected_paths = {
    "client_surface": "/account",
    "alternate_surface": "/game/account",
    "session_status_endpoint": "/account/session",
    "profile_endpoint": "/account/profile",
    "session_refresh_endpoint": "/account/session/refresh",
    "session_revoke_endpoint": "/account/session/revoke",
    "register_endpoint": "/account/register",
    "login_endpoint": "/account/login",
    "password_change_endpoint": "/account/password/change",
    "logout_endpoint": "/account/logout",
}
for key, value in expected_paths.items():
    require(f"readiness_{key}", readiness.get(key) == value, readiness.get(key))

flows = readiness.get("flows") if isinstance(readiness.get("flows"), dict) else {}
register = flows.get("register") if isinstance(flows.get("register"), dict) else {}
login = flows.get("login") if isinstance(flows.get("login"), dict) else {}
profile = flows.get("profile") if isinstance(flows.get("profile"), dict) else {}
password_change = flows.get("password_change") if isinstance(flows.get("password_change"), dict) else {}
session_refresh = flows.get("session_refresh") if isinstance(flows.get("session_refresh"), dict) else {}
session_revoke = flows.get("session_revoke") if isinstance(flows.get("session_revoke"), dict) else {}
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
require("profile_endpoint_visible", profile.get("endpoint") == "/account/profile", profile.get("endpoint"))
require("profile_form_id", profile.get("form_id") == "account-profile-form", profile.get("form_id"))
require("profile_requires_active_session", profile.get("requires_active_session") is True, profile.get("requires_active_session"))
require("profile_csrf_required", profile.get("csrf_required") is True, profile.get("csrf_required"))
require("profile_client_storage", profile.get("client_storage") == "localStorage:trillionnium.account.profile.v1", profile.get("client_storage"))
require("profile_credential_storage_disabled", profile.get("credential_storage_in_browser") is False, profile.get("credential_storage_in_browser"))
require("profile_no_secret_logging", profile.get("passwords_tokens_or_cookie_values_logged") is False, profile.get("passwords_tokens_or_cookie_values_logged"))
require("password_change_endpoint_visible", password_change.get("endpoint") == "/account/password/change", password_change.get("endpoint"))
require("password_change_form_id", password_change.get("form_id") == "account-password-change-form", password_change.get("form_id"))
require("password_change_requires_active_session", password_change.get("requires_active_session") is True, password_change.get("requires_active_session"))
require("password_change_csrf_required", password_change.get("csrf_required") is True, password_change.get("csrf_required"))
require("password_change_password_hash_argon2id", password_change.get("password_hash") == "argon2id", password_change.get("password_hash"))
require("password_change_credential_storage_disabled", password_change.get("credential_storage_in_browser") is False, password_change.get("credential_storage_in_browser"))
require("session_refresh_endpoint_visible", session_refresh.get("endpoint") == "/account/session/refresh", session_refresh.get("endpoint"))
require("session_refresh_button_id", session_refresh.get("button_id") == "account-session-refresh-button", session_refresh.get("button_id"))
require("session_refresh_requires_active_session", session_refresh.get("requires_active_session") is True, session_refresh.get("requires_active_session"))
require("session_refresh_csrf_required", session_refresh.get("csrf_required") is True, session_refresh.get("csrf_required"))
require("session_refresh_rotates_session_cookie", session_refresh.get("rotates_session_cookie") is True, session_refresh.get("rotates_session_cookie"))
require("session_refresh_rotates_csrf", session_refresh.get("rotates_csrf") is True, session_refresh.get("rotates_csrf"))
require("session_revoke_endpoint_visible", session_revoke.get("endpoint") == "/account/session/revoke", session_revoke.get("endpoint"))
require("session_revoke_button_id", session_revoke.get("button_id") == "account-session-revoke-button", session_revoke.get("button_id"))
require("session_revoke_requires_active_session", session_revoke.get("requires_active_session") is True, session_revoke.get("requires_active_session"))
require("session_revoke_csrf_required", session_revoke.get("csrf_required") is True, session_revoke.get("csrf_required"))
require("session_revoke_all_sessions", session_revoke.get("revokes_all_game_account_sessions") is True, session_revoke.get("revokes_all_game_account_sessions"))
require("session_revoke_clears_cookie", session_revoke.get("clears_session_cookie") is True, session_revoke.get("clears_session_cookie"))

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
    "cex_consumer_entry_game_account_profile_updates_total",
    "cex_consumer_entry_game_account_password_change_successes_total",
    "cex_consumer_entry_game_account_password_change_failures_total",
    "cex_consumer_entry_game_account_session_refresh_successes_total",
    "cex_consumer_entry_game_account_session_revoke_successes_total",
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
require("session_profile_endpoint_status_visible", session_password_auth.get("profile_endpoint") == "/account/profile", session_password_auth.get("profile_endpoint"))
require("session_refresh_endpoint_status_visible", session_password_auth.get("session_refresh_endpoint") == "/account/session/refresh", session_password_auth.get("session_refresh_endpoint"))
require("session_revoke_endpoint_status_visible", session_password_auth.get("session_revoke_endpoint") == "/account/session/revoke", session_password_auth.get("session_revoke_endpoint"))
require("session_password_change_endpoint_visible", session_password_auth.get("password_change_endpoint") == "/account/password/change", session_password_auth.get("password_change_endpoint"))
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
    "account_return_to_world_http_status": account_world_http,
    "account_return_to_unsafe_http_status": account_unsafe_http,
    "app_http_status": app_http,
    "world_http_status": world_http,
    "session_http_status": session_http,
    "metrics_http_status": metrics_http,
    "client_header_contract": client_header_contract,
    "client_contract": readiness.get("contract_version"),
    "default_return_to": readiness.get("return_to"),
    "app_account_session_bridge": all(token in app_body for token in ["app-account-session-card", "trillionnium_game_account_surface_session_v1", 'data-session-active="false"', "/account?return_to=/app"]),
    "world_account_session_bridge": all(token in world_body for token in ["world-account-session-card", "trillionnium_game_account_surface_session_v1", 'data-session-active="false"', "/account?return_to=/world"]),
    "world_account_identity_binding_contract": all(token in world_body for token in ["trillionnium_game_account_player_identity_binding_v1", 'data-account-profile-bound="false"']),
    "session_contract": session.get("contract_version"),
    "password_auth_enabled": password_auth_enabled,
    "password_auth_implemented": register.get("password_auth_implemented") is True and login.get("password_auth_implemented") is True,
    "profile_endpoint": profile.get("endpoint"),
    "password_change_endpoint": password_change.get("endpoint"),
    "session_refresh_endpoint": session_refresh.get("endpoint"),
    "session_revoke_endpoint": session_revoke.get("endpoint"),
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
    profile_json="$tmpdir/profile.json"
    world_after_profile_html="$tmpdir/world-after-profile.html"
    password_change_json="$tmpdir/password-change.json"
    session_refresh_json="$tmpdir/session-refresh.json"
    old_login_json="$tmpdir/old-login.json"
    new_login_json="$tmpdir/new-login.json"
    session_revoke_json="$tmpdir/session-revoke.json"
    logout_json="$tmpdir/logout.json"
    bad_login_json="$tmpdir/bad-login.json"
    handle="gate-$CHECKED_AT-$$-$RANDOM"
    display_name="Gate $CHECKED_AT"
    updated_display_name="Gate Updated $CHECKED_AT"
    password="Trillionnium-$CHECKED_AT-$$-$RANDOM-pass9"
    new_password="Trillionnium-$CHECKED_AT-$$-$RANDOM-pass10"
    room_id="!gate-$CHECKED_AT:trillionnium.local"
    updated_room_id="!gate-updated-$CHECKED_AT:trillionnium.local"
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
    csrf="$(jq -r '.session.csrf // .csrf // ""' "$session_after_register_json" 2>/dev/null || printf '')"
    profile_payload="$(jq -n --arg display_name "$updated_display_name" --arg room_id "$updated_room_id" --arg csrf "$csrf" '{display_name: $display_name, room_id: $room_id, csrf: $csrf}')"
    profile_http="$(curl -sS -o "$profile_json" -w '%{http_code}' -b "$cookie_jar" -H 'content-type: application/json' --data "$profile_payload" "$BASE_URL/account/profile" || printf '000')"
    world_after_profile_http="$(curl -sS -o "$world_after_profile_html" -w '%{http_code}' -b "$cookie_jar" "$BASE_URL/world?first_human_session=1" || printf '000')"
    password_change_payload="$(jq -n --arg old_password "$password" --arg new_password "$new_password" --arg csrf "$csrf" '{old_password: $old_password, new_password: $new_password, csrf: $csrf}')"
    password_change_http="$(curl -sS -o "$password_change_json" -w '%{http_code}' -b "$cookie_jar" -c "$cookie_jar" -H 'content-type: application/json' --data "$password_change_payload" "$BASE_URL/account/password/change" || printf '000')"
    password_change_csrf="$(jq -r '.csrf // ""' "$password_change_json" 2>/dev/null || printf '')"
    session_refresh_payload="$(jq -n --arg csrf "$password_change_csrf" --arg session_id "refresh-$session_id" '{csrf: $csrf, session_id: $session_id}')"
    session_refresh_http="$(curl -sS -o "$session_refresh_json" -w '%{http_code}' -b "$cookie_jar" -c "$cookie_jar" -H 'content-type: application/json' --data "$session_refresh_payload" "$BASE_URL/account/session/refresh" || printf '000')"
    old_login_payload="$(jq -n --arg handle "$handle" --arg password "$password" --arg session_id "old-$session_id" '{handle: $handle, password: $password, session_id: $session_id}')"
    old_login_http="$(curl -sS -o "$old_login_json" -w '%{http_code}' -H 'content-type: application/json' --data "$old_login_payload" "$BASE_URL/account/login" || printf '000')"
    new_login_payload="$(jq -n --arg handle "$handle" --arg password "$new_password" --arg room_id "$room_id" --arg session_id "changed-$session_id" '{handle: $handle, password: $password, room_id: $room_id, session_id: $session_id}')"
    new_login_http="$(curl -sS -o "$new_login_json" -w '%{http_code}' -c "$cookie_jar" -H 'content-type: application/json' --data "$new_login_payload" "$BASE_URL/account/login" || printf '000')"
    new_login_csrf="$(jq -r '.csrf // ""' "$new_login_json" 2>/dev/null || printf '')"
    session_revoke_payload="$(jq -n --arg csrf "$new_login_csrf" '{csrf: $csrf}')"
    session_revoke_http="$(curl -sS -o "$session_revoke_json" -w '%{http_code}' -b "$cookie_jar" -c "$cookie_jar" -H 'content-type: application/json' --data "$session_revoke_payload" "$BASE_URL/account/session/revoke" || printf '000')"
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
      if ! grep -Fq "$password" "$registry_persistence" && ! grep -Fq "$new_password" "$registry_persistence"; then
        registry_plaintext_absent=true
      fi
    fi

    python3 - "$summary_tmp" "$mutation_tmp" "$register_json" "$session_after_register_json" "$profile_json" "$world_after_profile_html" "$password_change_json" "$session_refresh_json" "$old_login_json" "$new_login_json" "$session_revoke_json" "$logout_json" "$bad_login_json" "$register_http" "$session_after_register_http" "$profile_http" "$world_after_profile_http" "$password_change_http" "$session_refresh_http" "$old_login_http" "$new_login_http" "$session_revoke_http" "$logout_http" "$rate_limit_http" "$registry_checked" "$registry_argon2id" "$registry_plaintext_absent" "$rate_attempts" "$updated_display_name" "$updated_room_id" <<'PY'
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
out_path = Path(sys.argv[2])
register_path = Path(sys.argv[3])
session_path = Path(sys.argv[4])
profile_path = Path(sys.argv[5])
world_after_profile_path = Path(sys.argv[6])
password_change_path = Path(sys.argv[7])
session_refresh_path = Path(sys.argv[8])
old_login_path = Path(sys.argv[9])
new_login_path = Path(sys.argv[10])
session_revoke_path = Path(sys.argv[11])
logout_path = Path(sys.argv[12])
bad_login_path = Path(sys.argv[13])
register_http = sys.argv[14]
session_http = sys.argv[15]
profile_http = sys.argv[16]
world_after_profile_http = sys.argv[17]
password_change_http = sys.argv[18]
session_refresh_http = sys.argv[19]
old_login_http = sys.argv[20]
new_login_http = sys.argv[21]
session_revoke_http = sys.argv[22]
logout_http = sys.argv[23]
rate_limit_http = sys.argv[24]
registry_checked = sys.argv[25] == "true"
registry_argon2id = sys.argv[26] == "true"
registry_plaintext_absent = sys.argv[27] == "true"
rate_attempts = int(sys.argv[28])
updated_display_name = sys.argv[29]
updated_room_id = sys.argv[30]

def read_json(path):
    try:
        text = path.read_text()
        return json.loads(text) if text.strip() else {}
    except Exception:
        return {}

summary = json.loads(summary_path.read_text())
register = read_json(register_path)
session = read_json(session_path)
profile = read_json(profile_path)
world_after_profile_body = world_after_profile_path.read_text(errors="replace") if world_after_profile_path.exists() else ""
password_change = read_json(password_change_path)
session_refresh = read_json(session_refresh_path)
old_login = read_json(old_login_path)
new_login = read_json(new_login_path)
session_revoke = read_json(session_revoke_path)
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
require("mutating_profile_http_200", profile_http == "200", {"http_status": profile_http, "body": profile})
require("mutating_profile_kind", profile.get("kind") == "game_account_profile", profile.get("kind"))
require("mutating_profile_status", profile.get("status") == "profile_updated", profile.get("status"))
require("mutating_profile_display_name", profile.get("display_name") == updated_display_name, profile.get("display_name"))
require("mutating_profile_room_id", profile.get("room_id") == updated_room_id, profile.get("room_id"))
require("mutating_profile_no_secret_logging", profile.get("passwords_tokens_or_cookie_values_logged") is False, profile.get("passwords_tokens_or_cookie_values_logged"))
require("mutating_world_after_profile_http_200", world_after_profile_http == "200", {"http_status": world_after_profile_http})
require(
    "mutating_world_profile_identity_binding",
    all(token in world_after_profile_body for token in [
        "trillionnium_game_account_player_identity_binding_v1",
        'data-account-profile-bound="true"',
        'data-account-identity-source="game_account_profile"',
        'data-profile-bound="true"',
        updated_display_name,
        updated_room_id,
    ]),
    None,
)
require("mutating_password_change_http_200", password_change_http == "200", {"http_status": password_change_http, "body": password_change})
require("mutating_password_change_kind", password_change.get("kind") == "game_account_password_change", password_change.get("kind"))
require("mutating_password_change_status", password_change.get("status") == "password_changed", password_change.get("status"))
require("mutating_password_change_session_preserved", password_change.get("active_session_preserved") is True, password_change.get("active_session_preserved"))
require("mutating_session_refresh_http_200", session_refresh_http == "200", {"http_status": session_refresh_http, "body": session_refresh})
require("mutating_session_refresh_status", session_refresh.get("status") == "session_refreshed", session_refresh.get("status"))
require("mutating_session_refresh_csrf_rotated", bool(session_refresh.get("csrf")) and session_refresh.get("csrf") != password_change.get("csrf"), {"new": session_refresh.get("csrf"), "old": password_change.get("csrf")})
require("mutating_old_password_rejected", old_login_http == "401", {"http_status": old_login_http, "body": old_login})
require("mutating_new_password_login_http_200", new_login_http == "200", {"http_status": new_login_http, "body": new_login})
require("mutating_session_revoke_http_200", session_revoke_http == "200", {"http_status": session_revoke_http, "body": session_revoke})
require("mutating_session_revoke_status", session_revoke.get("status") == "sessions_revoked", session_revoke.get("status"))
require("mutating_session_revoke_all_sessions", session_revoke.get("revoked_game_account_sessions") is True, session_revoke.get("revoked_game_account_sessions"))
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
    "profile_http_status": profile_http,
    "world_after_profile_http_status": world_after_profile_http,
    "world_profile_identity_binding": all(token in world_after_profile_body for token in ["trillionnium_game_account_player_identity_binding_v1", 'data-account-profile-bound="true"', 'data-account-identity-source="game_account_profile"', updated_display_name, updated_room_id]),
    "password_change_http_status": password_change_http,
    "session_refresh_http_status": session_refresh_http,
    "old_password_login_http_status": old_login_http,
    "new_password_login_http_status": new_login_http,
    "session_revoke_http_status": session_revoke_http,
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
