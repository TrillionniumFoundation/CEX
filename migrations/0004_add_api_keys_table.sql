-- 0004_add_api_keys_table.sql
-- Minimal persisted API key provenance for identity-service.

create table if not exists api_keys (
    api_key_id uuid primary key default gen_random_uuid(),
    org_id uuid not null references organizations(org_id),
    user_id uuid references users(user_id),
    key_hash text not null unique,
    key_prefix text not null,
    label text,
    status text not null default 'active',
    expires_at timestamptz,
    last_used_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz not null default now()
);

create index if not exists idx_api_keys_org_id on api_keys(org_id);
create index if not exists idx_api_keys_status on api_keys(status);
create index if not exists idx_api_keys_user_id on api_keys(user_id);
