alter table executions
    add column if not exists attempt_count integer not null default 0,
    add column if not exists max_attempts integer not null default 3;

create index if not exists idx_executions_retry_budget
    on executions (status, attempt_count, max_attempts, created_at)
    where status in ('Queued', 'Dispatching');
