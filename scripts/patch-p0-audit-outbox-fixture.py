#!/usr/bin/env python3
"""Keep the P0 Audit outbox lifecycle probe isolated from earlier exact intents."""

from pathlib import Path

path = Path(__file__).resolve().parents[1] / "scripts/check-p0-migrations-postgres.sh"
text = path.read_text(encoding="utf-8")
old = """    if not collision_rejected then
        raise exception 'audit outbox collision probe did not execute';
    end if;

    select *
      into claimed_row
      from public.cex_claim_audit_outbox_v1('p0-audit-worker', 1, 30);
"""
new = """    if not collision_rejected then
        raise exception 'audit outbox collision probe did not execute';
    end if;

    -- Exact account opening and Ledger effects above legitimately enqueue
    -- earlier Audit intents. Delay every non-target row so this probe tests
    -- the requested outbox row instead of relying on an empty global queue.
    update public.cex_audit_outbox_v1
       set available_at = now() + interval '1 hour'
     where outbox_id <> first_row.outbox_id
       and status in ('pending', 'retry_wait');

    select *
      into claimed_row
      from public.cex_claim_audit_outbox_v1('p0-audit-worker', 1, 30);
"""
if text.count(old) != 1:
    raise SystemExit(f"expected one Audit outbox claim probe, found {text.count(old)}")
path.write_text(text.replace(old, new), encoding="utf-8")
print("P0 Audit outbox fixture isolated from earlier exact intents")
