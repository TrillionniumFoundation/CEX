#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/_dev-helpers.sh
source "$SCRIPT_DIR/_dev-helpers.sh"
cex_load_env
cex_require_cmd python3 >/dev/null

SUMMARY_DIR="$PROJECT_ROOT/run/commercial-launch-drills"
mkdir -p "$SUMMARY_DIR"
CHECKED_AT="$(date +%s)"
EVIDENCE_PATH="${TRILLIONNIUM_COMMERCIAL_LAUNCH_DRILL_EVIDENCE_PATH:-$SUMMARY_DIR/latest.json}"
TARGET_REFUND_RECOVERY_MINUTES="${TRILLIONNIUM_COMMERCIAL_LAUNCH_TARGET_REFUND_RECOVERY_MINUTES:-60}"
TARGET_SUPPORT_FIRST_RESPONSE_MINUTES="${TRILLIONNIUM_COMMERCIAL_LAUNCH_TARGET_SUPPORT_FIRST_RESPONSE_MINUTES:-60}"
TARGET_INCIDENT_ACK_MINUTES="${TRILLIONNIUM_COMMERCIAL_LAUNCH_TARGET_INCIDENT_ACK_MINUTES:-15}"

python3 - \
  "$PROJECT_ROOT" \
  "$SUMMARY_DIR" \
  "$CHECKED_AT" \
  "$EVIDENCE_PATH" \
  "$TARGET_REFUND_RECOVERY_MINUTES" \
  "$TARGET_SUPPORT_FIRST_RESPONSE_MINUTES" \
  "$TARGET_INCIDENT_ACK_MINUTES" <<'PY'
import json
import sys
from pathlib import Path

project_root = Path(sys.argv[1])
summary_dir = Path(sys.argv[2])
checked_at = int(sys.argv[3])
evidence_path = Path(sys.argv[4])
target_refund_recovery_minutes = float(sys.argv[5])
target_support_first_response_minutes = float(sys.argv[6])
target_incident_ack_minutes = float(sys.argv[7])

contract_version = "trillionnium_commercial_launch_drills_gate_v1"
expected_input_contract = "trillionnium_commercial_launch_drills_evidence_v1"
summary_path = summary_dir / f"commercial-launch-drills-summary-{checked_at}.json"

required_drills = {
    "payment_reserve_consume_reconciliation": "Payment reserve/consume or provider authorization/capture reconciles with ledger settlement.",
    "refund_chargeback_recovery": "Refund/chargeback recovery is exercised and auditable.",
    "customer_support_escalation": "Support intake/escalation/owner handoff is exercised.",
    "legal_privacy_osm_review": "Legal/privacy and OSM attribution/ODbL obligations are reviewed.",
    "operator_incident_runbook": "Operator incident runbook is rehearsed for degraded runtime/provider paths.",
    "live_traffic_error_budget": "Launch traffic/error-budget policy and stop/go criteria are reviewed.",
}

if not evidence_path.exists():
    summary = {
        "contract_version": contract_version,
        "ok": False,
        "status": "blocked_missing_commercial_launch_drill_evidence",
        "checked_at_epoch": checked_at,
        "evidence_path": str(evidence_path),
        "required_input_contract_version": expected_input_contract,
        "required_drills": required_drills,
        "reason": "Set TRILLIONNIUM_COMMERCIAL_LAUNCH_DRILL_EVIDENCE_PATH to a completed commercial launch drill evidence JSON file. Browser/test green is not enough for commercial 8+.",
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
if evidence.get("synthetic") is True or evidence.get("template") is True:
    errors.append("synthetic/template evidence is not accepted for commercial launch readiness")

attestation = evidence.get("operator_attestation") or {}
for key in [
    "payment_or_sandbox_provider_checked",
    "refund_support_owner_assigned",
    "legal_privacy_review_completed",
    "operator_runbook_reviewed",
    "traffic_error_budget_reviewed",
    "no_secrets_or_personal_data_in_evidence",
]:
    if attestation.get(key) is not True:
        errors.append(f"operator_attestation.{key} must be true")

# Accept either a list of drill objects or a map keyed by drill_id.
raw_drills = evidence.get("drills") or []
if isinstance(raw_drills, dict):
    drills = []
    for drill_id, payload in raw_drills.items():
        item = dict(payload or {})
        item.setdefault("drill_id", drill_id)
        drills.append(item)
else:
    drills = list(raw_drills) if isinstance(raw_drills, list) else []

by_id = {str(drill.get("drill_id") or ""): drill for drill in drills if isinstance(drill, dict)}
drill_checks = []
for drill_id, description in required_drills.items():
    drill = by_id.get(drill_id) or {}
    passed = drill.get("passed") is True
    has_owner = bool(str(drill.get("owner") or "").strip())
    has_evidence_ref = bool(str(drill.get("evidence_ref") or "").strip())
    has_followup_policy = bool(str(drill.get("rollback_or_escalation") or drill.get("followup") or "").strip())
    check_passed = passed and has_owner and has_evidence_ref and has_followup_policy
    if not check_passed:
        errors.append(f"drill {drill_id} must have passed=true, owner, evidence_ref, and rollback_or_escalation/followup")
    drill_checks.append({
        "drill_id": drill_id,
        "description": description,
        "passed": check_passed,
        "detail": {
            "passed": passed,
            "has_owner": has_owner,
            "has_evidence_ref": has_evidence_ref,
            "has_followup_policy": has_followup_policy,
        },
    })

metrics = evidence.get("metrics") or {}
def numeric_metric(name):
    value = metrics.get(name)
    if isinstance(value, (int, float)):
        return float(value)
    errors.append(f"metrics.{name} must be numeric")
    return None

refund_recovery_minutes = numeric_metric("refund_recovery_minutes")
support_first_response_minutes = numeric_metric("support_first_response_minutes")
incident_ack_minutes = numeric_metric("incident_ack_minutes")

metric_checks = [
    {
        "check_id": "refund_recovery_time_budget",
        "passed": refund_recovery_minutes is not None and refund_recovery_minutes <= target_refund_recovery_minutes,
        "detail": {"refund_recovery_minutes": refund_recovery_minutes, "target": target_refund_recovery_minutes},
    },
    {
        "check_id": "support_first_response_budget",
        "passed": support_first_response_minutes is not None and support_first_response_minutes <= target_support_first_response_minutes,
        "detail": {"support_first_response_minutes": support_first_response_minutes, "target": target_support_first_response_minutes},
    },
    {
        "check_id": "incident_ack_budget",
        "passed": incident_ack_minutes is not None and incident_ack_minutes <= target_incident_ack_minutes,
        "detail": {"incident_ack_minutes": incident_ack_minutes, "target": target_incident_ack_minutes},
    },
    {
        "check_id": "error_budget_policy_defined",
        "passed": metrics.get("error_budget_policy_defined") is True,
        "detail": {"error_budget_policy_defined": metrics.get("error_budget_policy_defined")},
    },
    {
        "check_id": "privacy_data_inventory_reviewed",
        "passed": metrics.get("privacy_data_inventory_reviewed") is True,
        "detail": {"privacy_data_inventory_reviewed": metrics.get("privacy_data_inventory_reviewed")},
    },
    {
        "check_id": "payment_refund_reconciliation_done",
        "passed": metrics.get("payment_refund_reconciliation_done") is True,
        "detail": {"payment_refund_reconciliation_done": metrics.get("payment_refund_reconciliation_done")},
    },
]

if not all(check["passed"] for check in metric_checks):
    errors.append("one or more commercial launch drill metric checks failed")

risk_register = evidence.get("risk_register") or []
if not isinstance(risk_register, list):
    errors.append("risk_register must be a list")
    risk_register = []
open_blockers = [risk for risk in risk_register if isinstance(risk, dict) and risk.get("launch_blocker") is True and risk.get("status") not in {"resolved", "accepted_by_operator"}]
if open_blockers:
    errors.append("risk_register contains unresolved launch blockers")

ok = not errors
summary = {
    "contract_version": contract_version,
    "ok": ok,
    "status": "green" if ok else "needs_launch_drill_evidence_or_fixes",
    "checked_at_epoch": checked_at,
    "evidence_path": str(evidence_path),
    "input_contract_version": evidence.get("contract_version"),
    "operator_attestation": attestation,
    "drill_checks": drill_checks,
    "metric_checks": metric_checks,
    "metrics": {
        "refund_recovery_minutes": refund_recovery_minutes,
        "support_first_response_minutes": support_first_response_minutes,
        "incident_ack_minutes": incident_ack_minutes,
        "error_budget_policy_defined": metrics.get("error_budget_policy_defined"),
        "privacy_data_inventory_reviewed": metrics.get("privacy_data_inventory_reviewed"),
        "payment_refund_reconciliation_done": metrics.get("payment_refund_reconciliation_done"),
    },
    "risk_register_count": len(risk_register),
    "open_launch_blockers": open_blockers,
    "errors": errors,
    "summary": str(summary_path),
}
summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps(summary, ensure_ascii=False, indent=2))
if not ok:
    raise SystemExit(1)
PY
