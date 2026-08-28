-- 0010_trnm_economy_settlement_v1.sql
-- Durable, byte-exact, intent-hash-bound settlement receipts for Trillionnium World.

create table if not exists public.trnm_economy_settlement_receipts_v1 (
    intent_id text primary key
        check (btrim(intent_id) <> '' and length(intent_id) <= 256),
    intent_hash text not null
        check (intent_hash ~ '^[0-9a-f]{64}$'),
    intent_bytes bytea not null
        check (octet_length(intent_bytes) between 2 and 1048576),
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
        pg_catalog.encode(public.digest(intent_bytes, 'sha256'), 'hex') = intent_hash
        and pg_catalog.convert_from(intent_bytes, 'UTF8')::jsonb = intent_json
        and coalesce(intent_json ->> 'intent_id', '') = intent_id
        and coalesce(receipt_json ->> 'intent_id', '') = intent_id
        and coalesce(receipt_json ->> 'receipt_id', '') = receipt_id
        and coalesce(receipt_json #>> '{evidence,intent_hash}', '') = intent_hash
    )
);

create unique index if not exists idx_trnm_economy_settlement_intent_hash_v1
    on public.trnm_economy_settlement_receipts_v1(intent_hash);

create index if not exists idx_trnm_economy_settlement_account_v1
    on public.trnm_economy_settlement_receipts_v1(account_id, created_at)
    where account_id is not null;

create or replace function public.trnm_economy_reject_settlement_receipt_mutation_v1()
returns trigger
language plpgsql
security invoker
set search_path = pg_catalog, public
as $function$
begin
    raise exception using
        errcode = '55000',
        message = 'trnm economy settlement receipts are append-only';
end;
$function$;

drop trigger if exists trnm_economy_settlement_receipts_no_update_delete_v1
    on public.trnm_economy_settlement_receipts_v1;
create trigger trnm_economy_settlement_receipts_no_update_delete_v1
before update or delete on public.trnm_economy_settlement_receipts_v1
for each statement
execute function public.trnm_economy_reject_settlement_receipt_mutation_v1();

drop trigger if exists trnm_economy_settlement_receipts_no_truncate_v1
    on public.trnm_economy_settlement_receipts_v1;
create trigger trnm_economy_settlement_receipts_no_truncate_v1
before truncate on public.trnm_economy_settlement_receipts_v1
for each statement
execute function public.trnm_economy_reject_settlement_receipt_mutation_v1();

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
    'Append-only exact intent bytes/hash/receipt authority for trnm_cex_settlement_receipt_lookup_v1';
comment on column public.trnm_economy_settlement_receipts_v1.intent_bytes is
    'Exact serde EconomicIntent JSON bytes whose database-recomputed SHA-256 equals intent_hash';
comment on table public.trnm_economy_reward_budget_v1 is
    'Per-account UTC battle reward budget; public player market remains disabled';
