-- 0002_add_account_summary_columns.sql
-- Add account summary columns needed for transaction-backed reserve/consume/refund.

alter table accounts
    add column if not exists balance numeric(20, 6) not null default 0,
    add column if not exists reserved numeric(20, 6) not null default 0;
