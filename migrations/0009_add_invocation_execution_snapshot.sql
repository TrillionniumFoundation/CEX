alter table if exists invocations
    add column if not exists execution_snapshot jsonb;
