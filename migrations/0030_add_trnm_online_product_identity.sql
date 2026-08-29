-- Closed-alpha self-service identity lifecycle and suspension appeals.

create table if not exists trnm_product_registration_invites (
    invite_id uuid primary key,
    invite_code_hash text not null unique check (length(invite_code_hash) = 64),
    max_uses integer not null default 1 check (max_uses between 1 and 100),
    used_count integer not null default 0 check (used_count between 0 and max_uses),
    expires_at timestamptz not null,
    revoked_at timestamptz,
    created_at timestamptz not null default now()
);

create index if not exists idx_trnm_product_registration_invites_active
    on trnm_product_registration_invites(expires_at)
    where revoked_at is null;

create table if not exists trnm_product_login_attempts (
    player_id text primary key,
    attempt_count integer not null default 0 check (attempt_count >= 0),
    window_started_at timestamptz not null default now(),
    locked_until timestamptz,
    updated_at timestamptz not null default now()
);

create table if not exists trnm_identity_appeals (
    appeal_id uuid primary key,
    player_id text not null references trnm_player_identities(player_id),
    status text not null default 'pending' check (status in ('pending', 'approved', 'rejected')),
    message text not null check (length(message) between 10 and 2000),
    resolution text,
    created_at timestamptz not null default now(),
    resolved_at timestamptz,
    constraint trnm_identity_appeal_resolution_pair check (
        (status = 'pending' and resolution is null and resolved_at is null)
        or (status in ('approved', 'rejected') and resolution is not null and resolved_at is not null)
    )
);

create unique index if not exists idx_trnm_identity_one_pending_appeal
    on trnm_identity_appeals(player_id) where status = 'pending';

create index if not exists idx_trnm_identity_appeals_status_created
    on trnm_identity_appeals(status, created_at);
