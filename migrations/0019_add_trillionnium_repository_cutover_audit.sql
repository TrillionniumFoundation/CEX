-- Trillionnium repository cutover audit bridge.
-- This table materializes the JSON->normalized SQL cutover contract next to
-- each SQL-ready state snapshot so shadow parity, dual-write rollout, and
-- read-switch gates can be audited without re-parsing application logs.

create table if not exists league_state_repository_snapshots (
    repository_snapshot_id uuid primary key default gen_random_uuid(),
    state_hash text not null,
    source_snapshot_kind text not null default 'consumer_entry_json_v1',
    cutover_phase text not null default 'shadow_snapshot',
    current_repository text not null,
    next_repository text not null,
    migration_floor text not null,
    cutover_plan jsonb not null,
    shadow_validation jsonb not null,
    created_at timestamptz not null default now(),
    unique(state_hash, cutover_phase)
);

create index if not exists idx_league_state_repository_snapshots_created
    on league_state_repository_snapshots(created_at desc);

create index if not exists idx_league_state_repository_snapshots_phase
    on league_state_repository_snapshots(cutover_phase, next_repository, created_at desc);

create index if not exists idx_league_state_repository_snapshots_validation
    on league_state_repository_snapshots using gin (shadow_validation jsonb_path_ops);
