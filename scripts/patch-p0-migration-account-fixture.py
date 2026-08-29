#!/usr/bin/env python3
from pathlib import Path

path = Path(__file__).resolve().parents[1] / "scripts/check-p0-migrations-postgres.sh"
text = path.read_text(encoding="utf-8")
old = """insert into public.accounts (
    account_id, org_id, account_type, currency_unit, balance, reserved
) values (
    '20000000-0000-4000-8000-000000000001',
    '10000000-0000-4000-8000-000000000001',
    'test',
    'credit',
    10.250000,
    1.500000
);
"""
new = """select public.cex_open_account_v2(
    '20000000-0000-4000-8000-000000000001',
    '10000000-0000-4000-8000-000000000001',
    '12000000-0000-4000-8000-000000000001',
    'test',
    'credit',
    6::smallint,
    10250000,
    'p0-migration-test',
    'account-opening',
    'p0-migration-test'
);

update public.accounts
   set reserved_minor = 1500000
 where account_id = '20000000-0000-4000-8000-000000000001';
"""
if text.count(old) != 1:
    raise SystemExit(f"expected one legacy non-zero account fixture, found {text.count(old)}")
path.write_text(text.replace(old, new), encoding="utf-8")
print("P0 migration fixture now uses cex_open_account_v2")
