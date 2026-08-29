#!/usr/bin/env python3
"""Move the P0-N5 database fixture onto exact account-opening authority."""

from pathlib import Path

path = Path(__file__).resolve().parents[1] / "scripts/check-execution-settlement-commands-postgres.sh"
text = path.read_text(encoding="utf-8")

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
if text.count(old_account) != 1:
    raise SystemExit(f"expected one legacy P0-N5 account fixture, found {text.count(old_account)}")
text = text.replace(old_account, new_account)

old_scale = """            'credit',
            6,
            index_value::bigint * 1000000,
"""
new_scale = """            'credit',
            6::smallint,
            index_value::bigint * 1000000::bigint,
"""
if text.count(old_scale) != 1:
    raise SystemExit(f"expected one untyped P0-N5 contract scale, found {text.count(old_scale)}")
text = text.replace(old_scale, new_scale)

path.write_text(text, encoding="utf-8")
print("P0-N5 settlement fixture now uses exact account opening and typed money")
