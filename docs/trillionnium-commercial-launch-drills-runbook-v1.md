# Trillionnium Commercial Launch Drills Runbook v1

Purpose: keep commercial-release scoring honest. Existing product, browser, production-readiness, and signoff gates prove the local stack is strong; this drill gate proves launch operations are ready across payment/refund support, legal/privacy, operator response, and traffic/error-budget policy.

## When to run

Run this before claiming commercial release playability `8+`, and again before any public paid launch.

```bash
CEX_ENV_FILE=run/local-production/.env \
TRILLIONNIUM_COMMERCIAL_LAUNCH_DRILL_EVIDENCE_PATH=run/commercial-launch-drills/drill-YYYYMMDD.json \
scripts/check-trillionnium-commercial-launch-drills.sh
```

The script writes:

```text
run/commercial-launch-drills/commercial-launch-drills-summary-<epoch>.json
```

## Required drills

Each drill must have `passed=true`, an owner, an evidence reference, and a rollback/escalation/follow-up policy.

1. `payment_reserve_consume_reconciliation`
   - Exercise payment reserve/consume or provider authorization/capture path.
   - Reconcile it with the Ledger settlement record.
2. `refund_chargeback_recovery`
   - Exercise refund or chargeback recovery.
   - Show who owns support follow-up and how user balance/order state is corrected.
3. `customer_support_escalation`
   - Exercise support intake, triage, escalation, and close-the-loop messaging.
4. `legal_privacy_osm_review`
   - Review privacy/data inventory plus OSM attribution/ODbL obligations.
   - Confirm no public launch violates tile-server policy or location privacy constraints.
5. `operator_incident_runbook`
   - Rehearse degraded runtime/provider path, alert intake, restart/escalation, and recovery evidence.
6. `live_traffic_error_budget`
   - Review traffic ramp, error budget, rollback criteria, and launch stop/go thresholds.

## Green thresholds

- refund recovery drill: `<=60` minutes
- support first response drill: `<=60` minutes
- incident ack drill: `<=15` minutes
- error-budget policy defined
- privacy data inventory reviewed
- payment/refund reconciliation done
- no unresolved `launch_blocker=true` risks unless explicitly `accepted_by_operator`

## Evidence schema

Do not include secrets, API keys, card numbers, contact info, or personal data. Use internal ticket IDs or sanitized file references.

```json
{
  "contract_version": "trillionnium_commercial_launch_drills_evidence_v1",
  "synthetic": false,
  "template": false,
  "operator_attestation": {
    "payment_or_sandbox_provider_checked": true,
    "refund_support_owner_assigned": true,
    "legal_privacy_review_completed": true,
    "operator_runbook_reviewed": true,
    "traffic_error_budget_reviewed": true,
    "no_secrets_or_personal_data_in_evidence": true,
    "notes": "Use sanitized references only."
  },
  "drills": [
    {
      "drill_id": "payment_reserve_consume_reconciliation",
      "passed": true,
      "owner": "ops-payment-owner",
      "evidence_ref": "ticket-or-sanitized-log-ref",
      "rollback_or_escalation": "Stop paid launch and disable paid task entry if reconciliation fails."
    },
    {
      "drill_id": "refund_chargeback_recovery",
      "passed": true,
      "owner": "support-refund-owner",
      "evidence_ref": "ticket-or-sanitized-log-ref",
      "rollback_or_escalation": "Escalate to finance/support owner; freeze affected order until ledger correction is verified."
    },
    {
      "drill_id": "customer_support_escalation",
      "passed": true,
      "owner": "support-lead",
      "evidence_ref": "ticket-or-sanitized-log-ref",
      "rollback_or_escalation": "Escalate unresolved paid-user issue to launch commander."
    },
    {
      "drill_id": "legal_privacy_osm_review",
      "passed": true,
      "owner": "legal-privacy-owner",
      "evidence_ref": "review-or-checklist-ref",
      "rollback_or_escalation": "Block public traffic if attribution/privacy/tile-server obligations are not met."
    },
    {
      "drill_id": "operator_incident_runbook",
      "passed": true,
      "owner": "oncall-owner",
      "evidence_ref": "incident-drill-ref",
      "rollback_or_escalation": "Follow operator runbook; page product/infra owner on unresolved critical alerts."
    },
    {
      "drill_id": "live_traffic_error_budget",
      "passed": true,
      "owner": "launch-commander",
      "evidence_ref": "traffic-plan-ref",
      "rollback_or_escalation": "Stop ramp or roll back if error-budget threshold is breached."
    }
  ],
  "metrics": {
    "refund_recovery_minutes": 45,
    "support_first_response_minutes": 30,
    "incident_ack_minutes": 10,
    "error_budget_policy_defined": true,
    "privacy_data_inventory_reviewed": true,
    "payment_refund_reconciliation_done": true
  },
  "risk_register": [
    {
      "risk_id": "risk-example",
      "launch_blocker": false,
      "status": "resolved",
      "owner": "owner-id",
      "note": "Sanitized note only."
    }
  ]
}
```

## How to use failures

- Payment/refund failure -> fix settlement/reconciliation or support ownership before paid launch.
- Privacy/legal failure -> block public traffic until review is complete.
- Runbook failure -> update operator docs and rehearse again.
- Traffic budget failure -> define ramp/rollback thresholds before public exposure.

Do not raise commercial release playability to `8+` until this gate is green with real drill evidence.
