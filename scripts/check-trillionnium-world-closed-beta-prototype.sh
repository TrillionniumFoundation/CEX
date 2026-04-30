#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env

CONSUMER_ENTRY_BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
MATRIX_ENTRY_BASE_URL="${MATRIX_ENTRY_BASE_URL:-http://127.0.0.1:8091}"
SUMMARY_DIR="${CEX_CLOSED_BETA_SUMMARY_DIR:-$CEX_PROJECT_ROOT/run/closed-beta}"
mkdir -p "$SUMMARY_DIR"

consumer_health_file="$(mktemp)"
matrix_health_file="$(mktemp)"
trap 'rm -f "$consumer_health_file" "$matrix_health_file"' EXIT

curl -fsS "$CONSUMER_ENTRY_BASE_URL/health" >"$consumer_health_file"
curl -fsS "$MATRIX_ENTRY_BASE_URL/health" >"$matrix_health_file"

python3 - "$consumer_health_file" "$matrix_health_file" "$SUMMARY_DIR" <<'PY'
import json
import pathlib
import sys
import time

consumer_path = pathlib.Path(sys.argv[1])
matrix_path = pathlib.Path(sys.argv[2])
summary_dir = pathlib.Path(sys.argv[3])
consumer = json.loads(consumer_path.read_text())
matrix = json.loads(matrix_path.read_text())

failures = []

def require(name, condition, detail=None):
    if not condition:
        failures.append({"check": name, "detail": detail})

closed_beta = consumer.get("trillionnium_world_closed_beta_prototype") or {}
closed_beta_axes = closed_beta.get("axes") or {}
maturity = consumer.get("trillionnium_world_maturity") or {}
repo = consumer.get("league_repository_runtime") or {}
profile_checks = (consumer.get("profile_validation") or {}).get("checks") or {}

require("consumer_health_ok", consumer.get("status") == "ok", consumer.get("status"))
require(
    "closed_beta_contract_version",
    closed_beta.get("contract_version") == "trillionnium_world_closed_beta_prototype_v1",
    closed_beta.get("contract_version"),
)
require("closed_beta_target", closed_beta.get("target") == "closed_beta_prototype_100_percent", closed_beta.get("target"))
require("closed_beta_overall_100", closed_beta.get("overall_percent") == 100, closed_beta.get("overall_percent"))
require("closed_beta_converged", closed_beta.get("overall_status") == "converged", closed_beta.get("overall_status"))
for axis_id in ["product_loop", "access_governance", "persistence_runtime", "world_depth", "commerce_recovery"]:
    axis = closed_beta_axes.get(axis_id) or {}
    require(f"closed_beta_axis_{axis_id}_100", axis.get("percent") == 100, axis)
    require(f"closed_beta_axis_{axis_id}_converged", axis.get("status") == "converged", axis)
    require(f"closed_beta_axis_{axis_id}_no_remaining", not axis.get("remaining_checks"), axis.get("remaining_checks"))

require("world_maturity_overall_100", maturity.get("overall_percent") == 100, maturity.get("overall_percent"))
require("world_maturity_converged", maturity.get("overall_status") == "converged", maturity.get("overall_status"))
require("consumer_ingress_protected", consumer.get("ingress_protected") is True, consumer.get("ingress_protected"))
require("consumer_session_auth_required", consumer.get("require_session_auth") is True, consumer.get("require_session_auth"))
require("consumer_identity_binding_required", consumer.get("require_identity_binding") is True, consumer.get("require_identity_binding"))
require("consumer_identity_governance_valid", (consumer.get("identity_governance_overview") or {}).get("valid") is True, consumer.get("identity_governance_overview"))
require("consumer_replay_store_enabled", consumer.get("replay_store_enabled") is True, consumer.get("replay_store_enabled"))
require("consumer_rate_limit_store_enabled", consumer.get("rate_limit_store_enabled") is True, consumer.get("rate_limit_store_enabled"))
require("consumer_session_auth_secret_or_registry", profile_checks.get("session_auth_secret_present") is True or profile_checks.get("session_auth_issuer_registry_loaded") is True, profile_checks)
require("consumer_session_auth_allowed_issuer", int(profile_checks.get("session_auth_allowed_issuer_count") or 0) >= 1, profile_checks)
require("consumer_session_auth_expected_audience", profile_checks.get("session_auth_expected_audience_configured") is True, profile_checks)
require("repository_normalized_sql_direct_write_final", repo.get("effective_repository") == "normalized_sql_direct_write_final", repo.get("effective_repository"))
require("repository_read_switch_active", repo.get("repository_cutover_status") == "normalized_sql_direct_write_final_cutover_active", repo.get("repository_cutover_status"))

matrix_governance = matrix.get("consumer_entry_session_auth_governance_overview") or {}
selection = (matrix.get("consumer_entry_session_auth") or {}).get("selection") or {}
require("matrix_health_ok", matrix.get("status") == "ok", matrix.get("status"))
require("matrix_ingress_protected", matrix.get("ingress_protected") is True, matrix.get("ingress_protected"))
require("matrix_consumer_entry_protected", matrix.get("consumer_entry_protected") is True, matrix.get("consumer_entry_protected"))
require("matrix_recent_event_store_enabled", matrix.get("recent_event_store_enabled") is True, matrix.get("recent_event_store_enabled"))
require("matrix_rate_limit_store_enabled", matrix.get("rate_limit_store_enabled") is True, matrix.get("rate_limit_store_enabled"))
require("matrix_session_auth_selection_ok", selection.get("status") == "ok", selection)
require("matrix_session_auth_governance_valid", matrix_governance.get("valid") is True, matrix_governance)

summary = {
    "ok": not failures,
    "checked_at_epoch": int(time.time()),
    "consumer_base_url": consumer.get("cex_gateway_base_url"),
    "consumer_entry_base_url": consumer.get("service"),
    "closed_beta_overall_percent": closed_beta.get("overall_percent"),
    "closed_beta_axes": {axis_id: (closed_beta_axes.get(axis_id) or {}).get("percent") for axis_id in ["product_loop", "access_governance", "persistence_runtime", "world_depth", "commerce_recovery"]},
    "world_maturity_overall_percent": maturity.get("overall_percent"),
    "repository_effective": repo.get("effective_repository"),
    "repository_cutover_status": repo.get("repository_cutover_status"),
    "matrix_ingress_protected": matrix.get("ingress_protected"),
    "matrix_consumer_entry_protected": matrix.get("consumer_entry_protected"),
    "matrix_recent_event_store_enabled": matrix.get("recent_event_store_enabled"),
    "matrix_rate_limit_store_enabled": matrix.get("rate_limit_store_enabled"),
    "failures": failures,
}
path = summary_dir / f"closed-beta-prototype-summary-{summary['checked_at_epoch']}.json"
path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps({"ok": not failures, "summary": str(path), **summary}, ensure_ascii=False, indent=2))
if failures:
    raise SystemExit(1)
PY
