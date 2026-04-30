#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env
cex_require_cmd curl jq python3 >/dev/null

CONSUMER_ENTRY_BASE_URL="${CONSUMER_ENTRY_BASE_URL:-http://127.0.0.1:8090}"
MATRIX_ENTRY_BASE_URL="${MATRIX_ENTRY_BASE_URL:-http://127.0.0.1:8091}"
SUMMARY_DIR="$PROJECT_ROOT/run/public-commercial"
mkdir -p "$SUMMARY_DIR"
CHECKED_AT="$(date +%s)"
PRODUCTION_READINESS_LOG="$SUMMARY_DIR/production-readiness-${CHECKED_AT}.log"
HEALTH_JSON="$SUMMARY_DIR/consumer-health-${CHECKED_AT}.json"
MATRIX_HEALTH_JSON="$SUMMARY_DIR/matrix-health-${CHECKED_AT}.json"
METRICS_TXT="$SUMMARY_DIR/consumer-metrics-${CHECKED_AT}.txt"

curl -fsS "$CONSUMER_ENTRY_BASE_URL/health" > "$HEALTH_JSON"
curl -fsS "$MATRIX_ENTRY_BASE_URL/health" > "$MATRIX_HEALTH_JSON"
curl -fsS "$CONSUMER_ENTRY_BASE_URL/metrics" > "$METRICS_TXT"

PRODUCTION_READINESS_OK=false
RUN_PRODUCTION_READINESS="${CEX_PUBLIC_COMMERCIAL_RUN_PRODUCTION_READINESS:-1}"
PROVIDER_PROBE_REQUIRED="${CEX_PUBLIC_COMMERCIAL_PROVIDER_PROBE_REQUIRED:-1}"
PROVIDER_PROBE_MODEL="${CEX_PUBLIC_COMMERCIAL_PROVIDER_PROBE_MODEL:-openai-codex/gpt-5.5}"
PROVIDER_PROBE_TRANSPORT="${CEX_PUBLIC_COMMERCIAL_PROVIDER_PROBE_TRANSPORT:-gateway}"
if [[ "$RUN_PRODUCTION_READINESS" == "1" ]]; then
  tmp_env="$(mktemp "${TMPDIR:-/tmp}/cex-public-commercial-env.XXXXXX")"
  cleanup_tmp_env() { rm -f "$tmp_env"; }
  trap cleanup_tmp_env EXIT
  if [[ -n "${CEX_ENV_FILE:-}" && -f "$CEX_ENV_FILE" ]]; then
    cat "$CEX_ENV_FILE" > "$tmp_env"
  fi
  {
    printf '\n# public commercial product gate overrides\n'
    printf 'CEX_PROVIDER_PROBE_REQUIRED=%s\n' "$PROVIDER_PROBE_REQUIRED"
    printf 'CEX_PROVIDER_PROBE_MODEL=%s\n' "$PROVIDER_PROBE_MODEL"
    printf 'CEX_PROVIDER_PROBE_TRANSPORT=%s\n' "$PROVIDER_PROBE_TRANSPORT"
  } >> "$tmp_env"
  if CEX_ENV_FILE="$tmp_env" "$SCRIPT_DIR/check-production-readiness.sh" > "$PRODUCTION_READINESS_LOG" 2>&1; then
    PRODUCTION_READINESS_OK=true
  else
    cat "$PRODUCTION_READINESS_LOG" >&2 || true
  fi
else
  printf 'production readiness smoke skipped by CEX_PUBLIC_COMMERCIAL_RUN_PRODUCTION_READINESS=%s\n' "$RUN_PRODUCTION_READINESS" > "$PRODUCTION_READINESS_LOG"
fi

python3 - "$HEALTH_JSON" "$MATRIX_HEALTH_JSON" "$METRICS_TXT" "$SUMMARY_DIR" "$CHECKED_AT" "$CONSUMER_ENTRY_BASE_URL" "$MATRIX_ENTRY_BASE_URL" "$PRODUCTION_READINESS_OK" "$PRODUCTION_READINESS_LOG" <<'PY'
import json
import sys
from pathlib import Path

health_path = Path(sys.argv[1])
matrix_path = Path(sys.argv[2])
metrics_path = Path(sys.argv[3])
summary_dir = Path(sys.argv[4])
checked_at = int(sys.argv[5])
consumer_base_url = sys.argv[6]
matrix_base_url = sys.argv[7]
production_readiness_ok = sys.argv[8] == "true"
production_readiness_log = sys.argv[9]
consumer = json.loads(health_path.read_text())
matrix = json.loads(matrix_path.read_text())
metrics = metrics_path.read_text()
failures = []

def require(check_id, passed, detail=None):
    if not passed:
        failures.append({"check_id": check_id, "detail": detail})

public_product = consumer.get("trillionnium_world_public_commercial_product") or {}
public_axes = public_product.get("axes") or {}
real_user_beta = consumer.get("trillionnium_world_real_user_beta") or {}
closed_beta = consumer.get("trillionnium_world_closed_beta_prototype") or {}
maturity = consumer.get("trillionnium_world_maturity") or {}
repo = consumer.get("league_repository_runtime") or {}

require("public_commercial_contract_version", public_product.get("contract_version") == "trillionnium_world_public_commercial_product_v1", public_product.get("contract_version"))
require("public_commercial_target", public_product.get("target") == "public_commercial_product_100_percent", public_product.get("target"))
require("public_commercial_overall_100", public_product.get("overall_percent") == 100, public_product.get("overall_percent"))
require("public_commercial_converged", public_product.get("overall_status") == "converged", public_product.get("overall_status"))
for axis_id in ["public_launch_surface", "commercial_engine", "trust_safety", "durable_scale_ops", "growth_network", "public_world_depth"]:
    axis = public_axes.get(axis_id) or {}
    require(f"public_commercial_axis_{axis_id}_100", axis.get("percent") == 100, axis)
    require(f"public_commercial_axis_{axis_id}_converged", axis.get("status") == "converged", axis)
    require(f"public_commercial_axis_{axis_id}_no_remaining", not axis.get("remaining_checks"), axis.get("remaining_checks"))

require("real_user_beta_overall_100", real_user_beta.get("overall_percent") == 100, real_user_beta.get("overall_percent"))
require("closed_beta_overall_100", closed_beta.get("overall_percent") == 100, closed_beta.get("overall_percent"))
require("world_maturity_overall_100", maturity.get("overall_percent") == 100, maturity.get("overall_percent"))
require("repository_effective_normalized", repo.get("effective_repository") == "normalized_sql_direct_write_final", repo)
require("repository_cutover_read_switch_active", repo.get("repository_cutover_status") == "normalized_sql_direct_write_final_cutover_active", repo)
require("consumer_ingress_protected", consumer.get("ingress_protected") is True, consumer.get("ingress_protected"))
require("consumer_session_auth_required", consumer.get("require_session_auth") is True, consumer.get("require_session_auth"))
require("consumer_identity_binding_required", consumer.get("require_identity_binding") is True, consumer.get("require_identity_binding"))
require("consumer_replay_store_enabled", consumer.get("replay_store_enabled") is True, consumer.get("replay_store_enabled"))
require("consumer_rate_limit_store_enabled", consumer.get("rate_limit_store_enabled") is True, consumer.get("rate_limit_store_enabled"))
require("identity_governance_valid", ((consumer.get("identity_governance_overview") or {}).get("valid") is True), consumer.get("identity_governance_overview"))
require("matrix_ingress_protected", matrix.get("ingress_protected") is True, matrix.get("ingress_protected"))
require("matrix_consumer_entry_protected", matrix.get("consumer_entry_protected") is True, matrix.get("consumer_entry_protected"))
require("matrix_recent_event_store_enabled", matrix.get("recent_event_store_enabled") is True, matrix.get("recent_event_store_enabled"))
require("matrix_rate_limit_store_enabled", matrix.get("rate_limit_store_enabled") is True, matrix.get("rate_limit_store_enabled"))
require("production_readiness_ok", production_readiness_ok, production_readiness_log)
for metric in [
    "cex_consumer_entry_trillionnium_world_public_commercial_product_overall_percent",
    "cex_consumer_entry_trillionnium_world_public_commercial_product_public_launch_surface_percent",
    "cex_consumer_entry_trillionnium_world_public_commercial_product_commercial_engine_percent",
    "cex_consumer_entry_trillionnium_world_public_commercial_product_trust_safety_percent",
    "cex_consumer_entry_trillionnium_world_public_commercial_product_durable_scale_ops_percent",
    "cex_consumer_entry_trillionnium_world_public_commercial_product_growth_network_percent",
    "cex_consumer_entry_trillionnium_world_public_commercial_product_public_world_depth_percent",
]:
    require(f"metric_{metric}", metric in metrics, metric)

summary = {
    "ok": not failures,
    "summary": None,
    "checked_at_epoch": checked_at,
    "consumer_base_url": consumer_base_url,
    "matrix_base_url": matrix_base_url,
    "public_commercial_overall_percent": public_product.get("overall_percent"),
    "public_commercial_axes": {axis_id: (public_axes.get(axis_id) or {}).get("percent") for axis_id in ["public_launch_surface", "commercial_engine", "trust_safety", "durable_scale_ops", "growth_network", "public_world_depth"]},
    "real_user_beta_overall_percent": real_user_beta.get("overall_percent"),
    "closed_beta_overall_percent": closed_beta.get("overall_percent"),
    "world_maturity_overall_percent": maturity.get("overall_percent"),
    "repository_effective": repo.get("effective_repository"),
    "repository_cutover_status": repo.get("repository_cutover_status"),
    "matrix_ingress_protected": matrix.get("ingress_protected"),
    "matrix_consumer_entry_protected": matrix.get("consumer_entry_protected"),
    "matrix_recent_event_store_enabled": matrix.get("recent_event_store_enabled"),
    "matrix_rate_limit_store_enabled": matrix.get("rate_limit_store_enabled"),
    "production_readiness_ok": production_readiness_ok,
    "production_readiness_log": production_readiness_log,
    "failures": failures,
}
path = summary_dir / f"public-commercial-summary-{checked_at}.json"
summary["summary"] = str(path)
path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps(summary, ensure_ascii=False, indent=2))
if failures:
    raise SystemExit(1)
PY
