#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env
cex_require_cmd python3 >/dev/null

SUMMARY_DIR="$PROJECT_ROOT/run/multi-node-latency"
mkdir -p "$SUMMARY_DIR"
CHECKED_AT="$(date +%s)"
EVIDENCE_PATH="${TRILLIONNIUM_MULTI_NODE_LATENCY_EVIDENCE_PATH:-$SUMMARY_DIR/latest.json}"
MIN_NODE_COUNT="${TRILLIONNIUM_MULTI_NODE_LATENCY_MIN_NODE_COUNT:-2}"
MIN_SAMPLES_PER_ENDPOINT="${TRILLIONNIUM_MULTI_NODE_LATENCY_MIN_SAMPLES_PER_ENDPOINT:-100}"
TARGET_HEALTH_P95_SECONDS="${TRILLIONNIUM_MULTI_NODE_LATENCY_TARGET_HEALTH_P95_SECONDS:-1.0}"
TARGET_HEALTH_P99_SECONDS="${TRILLIONNIUM_MULTI_NODE_LATENCY_TARGET_HEALTH_P99_SECONDS:-2.0}"
TARGET_METRICS_P95_SECONDS="${TRILLIONNIUM_MULTI_NODE_LATENCY_TARGET_METRICS_P95_SECONDS:-1.0}"
TARGET_METRICS_P99_SECONDS="${TRILLIONNIUM_MULTI_NODE_LATENCY_TARGET_METRICS_P99_SECONDS:-2.0}"
TARGET_ERROR_RATE="${TRILLIONNIUM_MULTI_NODE_LATENCY_TARGET_ERROR_RATE:-0.001}"
MIN_LIVE_WINDOW_MINUTES="${TRILLIONNIUM_MULTI_NODE_LATENCY_MIN_LIVE_WINDOW_MINUTES:-30}"
MIN_LIVE_REQUESTS="${TRILLIONNIUM_MULTI_NODE_LATENCY_MIN_LIVE_REQUESTS:-1000}"

python3 - \
  "$PROJECT_ROOT" \
  "$SUMMARY_DIR" \
  "$CHECKED_AT" \
  "$EVIDENCE_PATH" \
  "$MIN_NODE_COUNT" \
  "$MIN_SAMPLES_PER_ENDPOINT" \
  "$TARGET_HEALTH_P95_SECONDS" \
  "$TARGET_HEALTH_P99_SECONDS" \
  "$TARGET_METRICS_P95_SECONDS" \
  "$TARGET_METRICS_P99_SECONDS" \
  "$TARGET_ERROR_RATE" \
  "$MIN_LIVE_WINDOW_MINUTES" \
  "$MIN_LIVE_REQUESTS" <<'PY'
import ipaddress
import json
import sys
from pathlib import Path
from urllib.parse import urlparse

project_root = Path(sys.argv[1])
summary_dir = Path(sys.argv[2])
checked_at = int(sys.argv[3])
evidence_path = Path(sys.argv[4])
min_node_count = int(sys.argv[5])
min_samples_per_endpoint = int(sys.argv[6])
targets = {
    "/health": {"p95_seconds": float(sys.argv[7]), "p99_seconds": float(sys.argv[8])},
    "/metrics": {"p95_seconds": float(sys.argv[9]), "p99_seconds": float(sys.argv[10])},
}
target_error_rate = float(sys.argv[11])
min_live_window_minutes = float(sys.argv[12])
min_live_requests = int(sys.argv[13])

contract_version = "trillionnium_multi_node_latency_evidence_gate_v1"
expected_input_contract = "trillionnium_multi_node_latency_evidence_v1"
summary_path = summary_dir / f"multi-node-latency-summary-{checked_at}.json"
required_endpoints = ["/health", "/metrics"]

if not evidence_path.exists():
    summary = {
        "contract_version": contract_version,
        "ok": False,
        "status": "blocked_missing_multi_node_or_live_traffic_latency_evidence",
        "checked_at_epoch": checked_at,
        "evidence_path": str(evidence_path),
        "required_input_contract_version": expected_input_contract,
        "reason": "Set TRILLIONNIUM_MULTI_NODE_LATENCY_EVIDENCE_PATH to a real multi-node or live-traffic latency evidence JSON file. Local single-node soak/browser evidence is intentionally insufficient for technical 9.9+.",
        "summary": str(summary_path),
    }
    summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    raise SystemExit(2)

try:
    evidence = json.loads(evidence_path.read_text())
except Exception as error:
    summary = {
        "contract_version": contract_version,
        "ok": False,
        "status": "blocked_invalid_json",
        "checked_at_epoch": checked_at,
        "evidence_path": str(evidence_path),
        "error": str(error),
        "summary": str(summary_path),
    }
    summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    raise SystemExit(1)

errors = []
if evidence.get("contract_version") != expected_input_contract:
    errors.append(f"contract_version must be {expected_input_contract}")
if evidence.get("template") is True or evidence.get("fabricated") is True:
    errors.append("template/fabricated evidence is not accepted for multi-node/live-traffic latency proof")
if evidence.get("localhost_only") is True:
    errors.append("localhost_only=true is not accepted; use at least two real nodes or real live traffic")

attestation = evidence.get("operator_attestation") or {}
for key in [
    "real_multi_node_or_live_traffic_environment",
    "production_like_config",
    "no_localhost_only_measurement",
    "raw_logs_retained",
    "no_secrets_or_personal_data_in_evidence",
]:
    if attestation.get(key) is not True:
        errors.append(f"operator_attestation.{key} must be true")

mode = str(evidence.get("evidence_type") or evidence.get("mode") or "").strip()
allowed_modes = {"multi_node", "live_traffic", "multi_node_live_traffic"}
if mode not in allowed_modes:
    errors.append(f"evidence_type/mode must be one of {sorted(allowed_modes)}")


def host_is_localhost(host):
    if not host:
        return False
    host = host.strip("[]").lower()
    if host in {"localhost", "localhost.localdomain"}:
        return True
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False


def metric_number(metrics, name, path, strict=True):
    value = metrics.get(name)
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return float(value)
    if strict:
        errors.append(f"{path}.{name} must be numeric")
    return None


def endpoint_check(scope, endpoint, metrics, strict=True):
    path = f"{scope}.endpoints[{endpoint}]"
    if not isinstance(metrics, dict):
        if strict:
            errors.append(f"{path} must be an object")
        metrics = {}
    samples = metric_number(metrics, "samples", path, strict=strict)
    p95 = metric_number(metrics, "p95_seconds", path, strict=strict)
    p99 = metric_number(metrics, "p99_seconds", path, strict=strict)
    error_rate = metric_number(metrics, "error_rate", path, strict=strict)
    passed = (
        samples is not None
        and samples >= min_samples_per_endpoint
        and p95 is not None
        and p95 <= targets[endpoint]["p95_seconds"]
        and p99 is not None
        and p99 <= targets[endpoint]["p99_seconds"]
        and error_rate is not None
        and error_rate <= target_error_rate
    )
    return {
        "scope": scope,
        "endpoint": endpoint,
        "passed": passed,
        "detail": {
            "samples": samples,
            "min_samples": min_samples_per_endpoint,
            "p95_seconds": p95,
            "target_p95_seconds": targets[endpoint]["p95_seconds"],
            "p99_seconds": p99,
            "target_p99_seconds": targets[endpoint]["p99_seconds"],
            "error_rate": error_rate,
            "target_error_rate": target_error_rate,
        },
    }

raw_nodes = evidence.get("nodes") or []
if not isinstance(raw_nodes, list):
    errors.append("nodes must be a list")
    raw_nodes = []

node_checks = []
node_ids = set()
for index, node in enumerate(raw_nodes, start=1):
    if not isinstance(node, dict):
        errors.append(f"nodes[{index}] must be an object")
        continue
    node_id = str(node.get("node_id") or "").strip()
    scope = f"node[{node_id or index}]"
    if not node_id:
        errors.append(f"nodes[{index}].node_id is required")
    if node_id in node_ids:
        errors.append(f"duplicate node_id {node_id}")
    node_ids.add(node_id)
    if node.get("localhost_only") is True:
        errors.append(f"{scope}.localhost_only must not be true")
    base_url = str(node.get("base_url_redacted") or node.get("base_url") or "").strip()
    if base_url:
        host = urlparse(base_url).hostname or base_url.split('/')[0]
        if host_is_localhost(host):
            errors.append(f"{scope}.base_url must not be localhost/loopback")
    endpoints = node.get("endpoints") or {}
    checks = [endpoint_check(scope, endpoint, endpoints.get(endpoint) or {}) for endpoint in required_endpoints]
    node_checks.append({
        "node_id": node_id,
        "role": node.get("role"),
        "site_or_region": node.get("site_or_region") or node.get("region"),
        "runtime_profile": node.get("runtime_profile"),
        "endpoint_checks": checks,
        "passed": all(check["passed"] for check in checks),
    })

traffic_window = evidence.get("traffic_window") or {}
if not isinstance(traffic_window, dict):
    errors.append("traffic_window must be an object")
    traffic_window = {}
duration_minutes = traffic_window.get("duration_minutes")
live_request_count = traffic_window.get("live_request_count") if "live_request_count" in traffic_window else traffic_window.get("request_count")
real_user_sessions = traffic_window.get("real_user_sessions")
traffic_source = str(traffic_window.get("traffic_source") or traffic_window.get("source") or "").strip()

multi_node_required = mode in {"multi_node", "multi_node_live_traffic"}
live_traffic_required = mode in {"live_traffic", "multi_node_live_traffic"}

aggregate = evidence.get("aggregate") or evidence.get("aggregate_endpoints") or {}
aggregate_checks = [
    endpoint_check("aggregate", endpoint, (aggregate.get(endpoint) or {}), strict=live_traffic_required)
    for endpoint in required_endpoints
]

multi_node_check = {
    "check_id": "multi_node_endpoint_latency_green",
    "passed": len(raw_nodes) >= min_node_count and bool(node_checks) and all(check["passed"] for check in node_checks),
    "detail": {"node_count": len(raw_nodes), "min_node_count": min_node_count, "node_checks": node_checks},
}

live_traffic_detail = {
    "duration_minutes": duration_minutes,
    "min_duration_minutes": min_live_window_minutes,
    "live_request_count": live_request_count,
    "min_live_requests": min_live_requests,
    "real_user_sessions": real_user_sessions,
    "traffic_source": traffic_source,
    "aggregate_checks": aggregate_checks,
}
live_traffic_check = {
    "check_id": "live_traffic_latency_green",
    "passed": (
        isinstance(duration_minutes, (int, float))
        and float(duration_minutes) >= min_live_window_minutes
        and isinstance(live_request_count, (int, float))
        and int(live_request_count) >= min_live_requests
        and traffic_source not in {"", "localhost_probe", "single_node_localhost"}
        and all(check["passed"] for check in aggregate_checks)
    ),
    "detail": live_traffic_detail,
}

if multi_node_required and not multi_node_check["passed"]:
    errors.append("multi-node mode requires at least two real nodes with green /health and /metrics latency/error metrics")
if live_traffic_required and not live_traffic_check["passed"]:
    errors.append("live-traffic mode requires a real traffic window with green aggregate /health and /metrics latency/error metrics")
if not multi_node_required and not live_traffic_required:
    errors.append("at least one of multi-node or live-traffic proof is required")

ok = not errors
summary = {
    "contract_version": contract_version,
    "ok": ok,
    "status": "green" if ok else "needs_multi_node_or_live_traffic_latency_evidence",
    "checked_at_epoch": checked_at,
    "evidence_path": str(evidence_path),
    "input_contract_version": evidence.get("contract_version"),
    "evidence_type": mode,
    "operator_attestation": attestation,
    "target_thresholds": {
        "min_node_count": min_node_count,
        "min_samples_per_endpoint": min_samples_per_endpoint,
        "endpoint_targets": targets,
        "target_error_rate": target_error_rate,
        "min_live_window_minutes": min_live_window_minutes,
        "min_live_requests": min_live_requests,
    },
    "proof_checks": [multi_node_check, live_traffic_check],
    "node_checks": node_checks,
    "aggregate_checks": aggregate_checks,
    "traffic_window": live_traffic_detail,
    "errors": errors,
    "summary": str(summary_path),
}
summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps(summary, ensure_ascii=False, indent=2))
if not ok:
    raise SystemExit(1)
PY
