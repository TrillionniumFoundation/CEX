alter table executions
    add column if not exists worker_id text,
    add column if not exists lease_expires_at timestamptz;

create index if not exists idx_executions_worker_lease
    on executions (status, lease_expires_at, created_at)
    where status in ('Queued', 'Dispatching');
