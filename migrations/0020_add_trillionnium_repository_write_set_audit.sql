-- Trillionnium repository write-set audit bridge.
-- Materializes the dual-write command write-set contract per snapshot so the
-- normalized repository cutover can prove every command boundary has an
-- explicit table/idempotency/validation seam before direct SQL writes replace
-- snapshot-backed dual-write.

create table if not exists league_state_repository_write_set_audits (
    repository_write_set_audit_id uuid primary key default gen_random_uuid(),
    state_hash text not null,
    cutover_phase text not null default 'shadow_snapshot',
    command text not null,
    boundary text not null,
    tables text[] not null default '{}',
    idempotency_key text not null,
    validation jsonb not null default '[]'::jsonb,
    write_set jsonb not null,
    migration_floor text not null,
    created_at timestamptz not null default now(),
    unique(state_hash, cutover_phase, command)
);

create index if not exists idx_league_state_repository_write_set_audits_command
    on league_state_repository_write_set_audits(command, created_at desc);

create index if not exists idx_league_state_repository_write_set_audits_tables
    on league_state_repository_write_set_audits using gin (tables);

create index if not exists idx_league_state_repository_write_set_audits_validation
    on league_state_repository_write_set_audits using gin (validation jsonb_path_ops);
