-- Trillionnium Term Exchange receipt normalized tables.
-- Stores compact typed EconomicReceipt projections from LeagueState and WorldState
-- so progression gates can shadow/cut over from legacy string ledger statuses to
-- ReceiptProgressionClass while preserving JSON snapshot rollback compatibility.

create table if not exists league_term_exchange_receipts (
    receipt_id text primary key,
    protocol_version text not null,
    intent_id text not null,
    term_id text not null,
    backend_id text not null,
    backend_kind text not null,
    status text not null,
    progression_class text not null,
    settlement_reference text,
    ledger_entry_id text,
    reason text,
    finalized_at timestamptz not null default now(),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists idx_league_term_exchange_receipts_intent
    on league_term_exchange_receipts(intent_id);
create index if not exists idx_league_term_exchange_receipts_term_status
    on league_term_exchange_receipts(term_id, status, finalized_at desc);
create index if not exists idx_league_term_exchange_receipts_progression
    on league_term_exchange_receipts(progression_class, finalized_at desc);
create index if not exists idx_league_term_exchange_receipts_backend
    on league_term_exchange_receipts(backend_id, backend_kind, finalized_at desc);

create table if not exists world_term_exchange_receipts (
    receipt_id text primary key,
    protocol_version text not null,
    intent_id text not null,
    term_id text not null,
    backend_id text not null,
    backend_kind text not null,
    status text not null,
    progression_class text not null,
    settlement_reference text,
    ledger_entry_id text,
    reason text,
    finalized_at timestamptz not null default now(),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists idx_world_term_exchange_receipts_intent
    on world_term_exchange_receipts(intent_id);
create index if not exists idx_world_term_exchange_receipts_term_status
    on world_term_exchange_receipts(term_id, status, finalized_at desc);
create index if not exists idx_world_term_exchange_receipts_progression
    on world_term_exchange_receipts(progression_class, finalized_at desc);
create index if not exists idx_world_term_exchange_receipts_backend
    on world_term_exchange_receipts(backend_id, backend_kind, finalized_at desc);
