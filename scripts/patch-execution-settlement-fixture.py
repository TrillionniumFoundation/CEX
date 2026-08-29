#!/usr/bin/env python3
"""Move P0-N5 fixtures and governance onto exact authority and plan v12."""

from pathlib import Path

root = Path(__file__).resolve().parents[1]

fixture_path = root / "scripts/check-execution-settlement-commands-postgres.sh"
text = fixture_path.read_text(encoding="utf-8")
old_account = """insert into public.accounts (
    account_id, org_id, account_type, currency_unit, balance, reserved, status
) values (
    'f1000000-0000-4000-8000-000000000001',
    'f0000000-0000-4000-8000-000000000001',
    'settlement-test',
    'credit',
    100.000000,
    0.000000,
    'active'
);
"""
new_account = """select public.cex_open_account_v2(
    'f1000000-0000-4000-8000-000000000001'::uuid,
    'f0000000-0000-4000-8000-000000000001'::uuid,
    'f1100000-0000-4000-8000-000000000001'::uuid,
    'settlement-test',
    'credit',
    6::smallint,
    100000000::bigint,
    'org:f0000000:opening',
    'execution-settlement-account-opening',
    'p0-n5-fixture'
);
"""
if text.count(old_account) == 1:
    text = text.replace(old_account, new_account)
elif text.count(old_account) != 0 or text.count(new_account) != 1:
    raise SystemExit("P0-N5 account fixture is neither legacy nor the reviewed exact form")

old_scale = """            'credit',
            6,
            index_value::bigint * 1000000,
"""
new_scale = """            'credit',
            6::smallint,
            index_value::bigint * 1000000::bigint,
"""
if text.count(old_scale) == 1:
    text = text.replace(old_scale, new_scale)
elif text.count(old_scale) != 0 or text.count(new_scale) != 1:
    raise SystemExit("P0-N5 contract money types are neither legacy nor the reviewed exact form")
fixture_path.write_text(text, encoding="utf-8")

static_path = root / "scripts/check-execution-settlement-commands.py"
static_text = static_path.read_text(encoding="utf-8")
old_plan = '''require(
    "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v11.md",
    "P0-N5 delivered by this candidate",
    "Gateway exact registration and reserve",
    "not production-ready",
)
'''
new_plan = '''require(
    "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md",
    "P0-N5 durable Execution settlement",
    "P0-N6 delivered by this candidate",
    "not production-ready",
)
'''
if static_text.count(old_plan) == 1:
    static_text = static_text.replace(old_plan, new_plan)
elif static_text.count(old_plan) != 0 or static_text.count(new_plan) != 1:
    raise SystemExit("Execution static gate plan contract is neither v11 nor reviewed v12")
static_path.write_text(static_text, encoding="utf-8")

workflow_path = root / ".github/workflows/p0-execution-settlement-gate.yml"
workflow_text = workflow_path.read_text(encoding="utf-8")
old_ref = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v11.md"
new_ref = "docs/CEX-DEVELOPMENT-PLAN-2026-08-28-v12.md"
if workflow_text.count(old_ref) == 2:
    workflow_text = workflow_text.replace(old_ref, new_ref)
elif workflow_text.count(old_ref) != 0 or workflow_text.count(new_ref) != 2:
    raise SystemExit("Execution workflow plan paths are neither v11 nor reviewed v12")
workflow_path.write_text(workflow_text, encoding="utf-8")

print("P0-N5 fixture and governance now use exact authority and plan v12")
