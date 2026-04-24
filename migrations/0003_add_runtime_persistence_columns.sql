-- 0003_add_runtime_persistence_columns.sql
-- Persist invocation / execution / approval runtime state in Postgres

alter table if exists invocations
    add column if not exists updated_at timestamptz not null default now(),
    add column if not exists execution_id uuid,
    add column if not exists ledger_reserved boolean not null default false,
    add column if not exists ledger_refunded boolean not null default false,
    add column if not exists approval_required boolean not null default false,
    add column if not exists policy_reason text,
    add column if not exists failure_reason text;

alter table if exists executions
    add column if not exists approval_required boolean not null default false,
    add column if not exists policy_reason text,
    add column if not exists approved_by text,
    add column if not exists updated_at timestamptz not null default now();

alter table if exists approvals
    add column if not exists resolver_id text,
    add column if not exists resolution_payload jsonb,
    add column if not exists updated_at timestamptz not null default now();

create index if not exists idx_invocations_execution_id on invocations(execution_id);
create unique index if not exists idx_approvals_execution_id on approvals(execution_id);
