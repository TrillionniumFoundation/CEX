-- Durable TRNM native-economy command, receipt, cursor, and escrow state.
-- The ledger transaction is the source of truth; legacy League/World receipt
-- tables remain read-compatible projections only.

do $$
begin
    if not exists (
        select 1 from pg_constraint
        where conname = 'accounts_non_negative_balances'
    ) then
        alter table accounts
            add constraint accounts_non_negative_balances
            check (balance >= 0 and reserved >= 0 and reserved <= balance) not valid;
    end if;
end $$;

alter table accounts validate constraint accounts_non_negative_balances;

create table if not exists trnm_economic_intents (
    intent_id text primary key,
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    payload_hash text not null,
    intent_json jsonb not null,
    status text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (idempotency_scope, idempotency_key)
);

create table if not exists trnm_economic_receipts (
    receipt_id text primary key,
    intent_id text not null unique references trnm_economic_intents(intent_id),
    protocol_version text not null,
    idempotency_scope text not null,
    idempotency_key text not null,
    progression_class text not null,
    status text not null,
    receipt_json jsonb not null,
    finalized_at timestamptz not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (idempotency_scope, idempotency_key)
);

create index if not exists idx_trnm_economic_receipts_finalized
    on trnm_economic_receipts(finalized_at desc);

create table if not exists trnm_economy_reconciliation_cursors (
    actor_id text not null,
    account_id uuid not null references accounts(account_id),
    cursor bigint not null check (cursor >= 0),
    updated_at timestamptz not null default now(),
    primary key (actor_id, account_id),
    unique (account_id)
);

create table if not exists trnm_escrow_trades (
    purchase_id text primary key,
    buyer_account_id uuid not null references accounts(account_id),
    seller_account_id uuid not null references accounts(account_id),
    asset_id text not null,
    quantity bigint not null check (quantity > 0),
    amount numeric(20, 6) not null check (amount > 0),
    status text not null check (status in ('held', 'committed', 'refunded', 'reversed')),
    reserve_intent_id text not null unique,
    settle_intent_id text unique,
    consume_intent_id text unique,
    reversal_intent_id text unique,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (buyer_account_id <> seller_account_id)
);

create index if not exists idx_trnm_escrow_trades_accounts_status
    on trnm_escrow_trades(buyer_account_id, seller_account_id, status);

