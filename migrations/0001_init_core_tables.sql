-- 0001_init_core_tables.sql
-- Initial core tables for Rust AI-native platform MVP

create extension if not exists "pgcrypto";

create table if not exists organizations (
    org_id uuid primary key default gen_random_uuid(),
    name text not null,
    status text not null default 'active',
    plan text not null default 'free',
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists users (
    user_id uuid primary key default gen_random_uuid(),
    org_id uuid not null references organizations(org_id),
    email text,
    role text not null default 'member',
    status text not null default 'active',
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists accounts (
    account_id uuid primary key default gen_random_uuid(),
    org_id uuid not null references organizations(org_id),
    account_type text not null,
    currency_unit text not null default 'credit',
    status text not null default 'active',
    created_at timestamptz not null default now()
);

create table if not exists ledger_entries (
    entry_id uuid primary key default gen_random_uuid(),
    account_id uuid not null references accounts(account_id),
    direction text not null check (direction in ('debit', 'credit')),
    amount numeric(20, 6) not null,
    reason text not null,
    reference_type text,
    reference_id uuid,
    idempotency_key text,
    created_at timestamptz not null default now()
);

create unique index if not exists idx_ledger_entries_idempotency_key
    on ledger_entries(idempotency_key)
    where idempotency_key is not null;

create table if not exists capabilities (
    capability_id uuid primary key default gen_random_uuid(),
    org_id uuid references organizations(org_id),
    capability_type text not null,
    provider text not null,
    name text not null,
    version text not null,
    pricing_model jsonb,
    policy_tags jsonb,
    status text not null default 'active',
    created_at timestamptz not null default now()
);

create table if not exists invocations (
    invocation_id uuid primary key default gen_random_uuid(),
    org_id uuid not null references organizations(org_id),
    actor_id uuid references users(user_id),
    capability_id uuid references capabilities(capability_id),
    status text not null default 'created',
    request_payload jsonb not null,
    route_hint jsonb,
    trace_id uuid not null,
    created_at timestamptz not null default now()
);

create table if not exists executions (
    execution_id uuid primary key default gen_random_uuid(),
    invocation_id uuid not null references invocations(invocation_id),
    status text not null,
    provider_target text,
    started_at timestamptz,
    ended_at timestamptz,
    result_payload jsonb,
    trace_id uuid not null,
    created_at timestamptz not null default now()
);

create table if not exists approvals (
    approval_id uuid primary key default gen_random_uuid(),
    execution_id uuid not null references executions(execution_id),
    status text not null default 'pending',
    requested_to text,
    requested_at timestamptz not null default now(),
    resolved_at timestamptz,
    resolution text
);

create table if not exists audit_events (
    event_id uuid primary key default gen_random_uuid(),
    trace_id uuid not null,
    actor_type text not null,
    actor_id text,
    event_type text not null,
    payload jsonb,
    created_at timestamptz not null default now()
);

create index if not exists idx_invocations_org_id on invocations(org_id);
create index if not exists idx_executions_invocation_id on executions(invocation_id);
create index if not exists idx_audit_events_trace_id on audit_events(trace_id);
