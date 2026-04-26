-- Trillionnium League SQL-ready state snapshot bridge.
-- This table is the low-risk cutover bridge from the consumer-entry JSON store
-- to SQL persistence: the app can keep serving the current read model while
-- writing full JSONB snapshots that can be replayed, diffed, or migrated into
-- the normalized 0010 league tables.

create table if not exists league_state_snapshots (
    snapshot_id uuid primary key default gen_random_uuid(),
    snapshot_kind text not null default 'consumer_entry_json_v1',
    state_hash text not null,
    state jsonb not null,
    created_at timestamptz not null default now()
);

create index if not exists idx_league_state_snapshots_created_at
    on league_state_snapshots(created_at desc);

create unique index if not exists idx_league_state_snapshots_state_hash
    on league_state_snapshots(state_hash);
