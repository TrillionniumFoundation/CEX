-- Server-authorized value issuance and player-scoped economic sessions.

create table if not exists trnm_value_entitlements (
    entitlement_id text primary key,
    intent_id text not null unique,
    issuer text not null,
    key_id text not null,
    actor_id text not null,
    account_id uuid not null references accounts(account_id),
    source_kind text not null check (source_kind in ('battle', 'contract')),
    source_id text not null,
    amount_credits bigint not null check (amount_credits >= 0 and amount_credits <= 100),
    currency text not null check (currency = 'wallet_credits'),
    budget_day integer not null check (budget_day between 20000101 and 29991231),
    issued_at timestamptz not null,
    expires_at timestamptz not null,
    entitlement_json jsonb not null,
    consumed_at timestamptz not null default now(),
    constraint trnm_entitlement_time_order check (expires_at > issued_at)
);

create unique index if not exists idx_trnm_entitlement_source_once
    on trnm_value_entitlements(account_id, source_kind, source_id);

create table if not exists trnm_wallet_reward_daily_budget (
    account_id uuid not null references accounts(account_id),
    budget_day integer not null check (budget_day between 20000101 and 29991231),
    issued_credits bigint not null default 0 check (issued_credits between 0 and 300),
    updated_at timestamptz not null default now(),
    primary key (account_id, budget_day)
);

create table if not exists trnm_player_sessions (
    session_id uuid primary key,
    player_id text not null references trnm_player_identities(player_id),
    account_id uuid not null references accounts(account_id),
    device_id text not null,
    recovery_generation bigint not null check (recovery_generation > 0),
    token_hash text not null unique,
    issued_at timestamptz not null,
    expires_at timestamptz not null,
    revoked_at timestamptz,
    revoke_reason text,
    last_used_at timestamptz,
    created_at timestamptz not null default now(),
    constraint trnm_player_session_time_order check (expires_at > issued_at),
    constraint trnm_player_session_revocation_pair check (
        (revoked_at is null and revoke_reason is null)
        or (revoked_at is not null and revoke_reason is not null)
    )
);

create index if not exists idx_trnm_player_sessions_active
    on trnm_player_sessions(player_id, account_id, expires_at)
    where revoked_at is null;

create table if not exists trnm_dr_markers (
    marker_id uuid primary key,
    marker_name text not null unique,
    created_at timestamptz not null default now()
);

alter table trnm_identity_recovery_audit
    drop constraint if exists trnm_identity_recovery_audit_event_kind_check;
alter table trnm_identity_recovery_audit
    add constraint trnm_identity_recovery_audit_event_kind_check
    check (event_kind in ('registered', 'recovered', 'reactivated', 'suspended', 'closed'));
