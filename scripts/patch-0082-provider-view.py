#!/usr/bin/env python3
from pathlib import Path

path = Path(__file__).resolve().parents[1] / "migrations/0082_close_provider_unknown_outcome_reconciliation.sql"
text = path.read_text(encoding="utf-8")
old = """    count(*) filter (
        where command.status in ('reconcile_required','dead_letter')
          and evidence.evidence_id is null
    )::bigint as missing_reconciliation_evidence_count,
    count(*) filter (
        where command.status in ('reconcile_required','dead_letter')
          and command.acknowledged_at is null
    )::bigint as unacknowledged_count,
    min(command.available_at) filter (where command.status in ('pending','retry_wait')) as oldest_available_at,
    min(command.lease_expires_at) filter (where command.status='claimed') as oldest_lease_expiry
"""
new = """    count(*) filter (
        where command.status in ('reconcile_required','dead_letter')
          and command.acknowledged_at is null
    )::bigint as unacknowledged_count,
    min(command.available_at) filter (where command.status in ('pending','retry_wait')) as oldest_available_at,
    min(command.lease_expires_at) filter (where command.status='claimed') as oldest_lease_expiry,
    count(*) filter (
        where command.status in ('reconcile_required','dead_letter')
          and evidence.evidence_id is null
    )::bigint as missing_reconciliation_evidence_count
"""
if text.count(old) != 1:
    raise SystemExit(f"expected one provider status view block, found {text.count(old)}")
path.write_text(text.replace(old, new), encoding="utf-8")
print("0082 provider status view made upgrade-safe")
