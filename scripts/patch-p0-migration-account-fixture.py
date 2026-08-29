#!/usr/bin/env python3
from pathlib import Path

path = Path(__file__).resolve().parents[1] / "scripts/check-p0-migrations-postgres.sh"
text = path.read_text(encoding="utf-8")
old = """insert into public.ledger_entries (
    entry_id, account_id, direction, amount, reason, idempotency_key
) values (
    '30000000-0000-4000-8000-000000000001',
    '20000000-0000-4000-8000-000000000001',
    'debit',
    1.250000,
    'p0-migration-test',
    'p0-migration-test-ledger-entry'
);

do $test$
begin
    if not exists (
        select 1
          from public.ledger_entries
         where entry_id = '30000000-0000-4000-8000-000000000001'
           and amount_minor = 1250000
    ) then
        raise exception 'money v2 ledger-entry synchronization failed';
    end if;
end
$test$;
"""
new = """select public.cex_apply_ledger_effect_v1(
    '20000000-0000-4000-8000-000000000001',
    '31000000-0000-4000-8000-000000000001',
    '30000000-0000-4000-8000-000000000001',
    'grant',
    1250000,
    6::smallint,
    'p0-migration-test',
    '32000000-0000-4000-8000-000000000001',
    'p0-migration-test',
    'p0-migration-test-ledger-entry',
    'ledger-service',
    'p0-migration-test',
    'explicit'
);

do $test$
begin
    if not exists (
        select 1
          from public.ledger_entries
         where operation_id = '30000000-0000-4000-8000-000000000001'
           and operation_kind = 'grant'
           and amount_minor = 1250000
           and currency_scale = 6
           and provenance_mode = 'explicit'
    ) then
        raise exception 'exact Ledger v2 effect persistence failed';
    end if;
end
$test$;
"""
if text.count(old) != 1:
    raise SystemExit(f"expected one legacy ledger fixture, found {text.count(old)}")
path.write_text(text.replace(old, new), encoding="utf-8")
print("P0 migration fixture now uses cex_apply_ledger_effect_v1")
