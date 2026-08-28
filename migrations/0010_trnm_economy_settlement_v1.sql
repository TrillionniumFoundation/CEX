-- 0010_trnm_economy_settlement_v1.sql
-- Durable, intent-hash-bound settlement receipts for Trillionnium World.

create table if not exists public.trnm_economy_settlement_receipts_v1 (
    intent_id text primary key
        check (btrim(intent_id) <> '' and length(intent_id) <= 256),
    intent_hash text not null
        check (intent_hash ~ '^[0-9a-f]{64}$'),
    intent_json jsonb not null
        check (jsonb_typeof(intent_json) = 'object'),
    receipt_id text not null unique
        check (btrim(receipt_id) <> '' and length(receipt_id) <= 256),
    receipt_json jsonb not null
        check (jsonb_typeof(receipt_json) = 'object'),
    authority_id text not null
        check (btrim(authority_id) <> '' and length(authority_id) <= 256),
    account_id uuid references public.accounts(account_id) on delete restrict,
    ledger_entry_id uuid references public.ledger_entries(entry_id) on delete restrict,
    created_at timestamptz not null default pg_catalog.clock_timestamp(),
    check (
        coalesce(intent_json ->> 'intent_id', '') = intent_id
        and coalesce(receipt_json ->> 'intent_id', '') = intent_id
        and coalesce(receipt_json ->> 'receipt_id', '') = receipt_id
    )
);

create unique index if not exists idx_trnm_economy_settlement_intent_hash_v1
    on public.trnm_economy_settlement_receipts_v1(intent_hash);

create index if not exists idx_trnm_economy_settlement_account_v1
    on public.trnm_economy_settlement_receipts_v1(account_id, created_at)
    where account_id is not null;

create table if not exists public.trnm_economy_reward_budget_v1 (
    account_id uuid not null
        references public.accounts(account_id) on delete restrict,
    budget_day integer not null
        check (budget_day between 19700101 and 99991231),
    amount_credits bigint not null default 0
        check (amount_credits between 0 and 300),
    created_at timestamptz not null default pg_catalog.clock_timestamp(),
    updated_at timestamptz not null default pg_catalog.clock_timestamp(),
    primary key (account_id, budget_day)
);

comment on table public.trnm_economy_settlement_receipts_v1 is
    'Immutable intent/hash/receipt authority for trnm_cex_settlement_receipt_lookup_v1';
comment on table public.trnm_economy_reward_budget_v1 is
    'Per-account UTC battle reward budget; public player market remains disabled';
