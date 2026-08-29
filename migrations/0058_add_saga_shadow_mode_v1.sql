begin;

alter table public.cex_saga_commands_v1
    add column if not exists execution_mode text not null default 'active';

do $$
begin
    if not exists (
        select 1
          from pg_constraint
         where conrelid = 'public.cex_saga_commands_v1'::regclass
           and conname = 'cex_saga_commands_execution_mode_v1'
    ) then
        alter table public.cex_saga_commands_v1
            add constraint cex_saga_commands_execution_mode_v1
            check (execution_mode in ('shadow', 'active'));
    end if;
end
$$;

-- Replace the claim function so shadow commands can never trigger a side effect.
create or replace function public.cex_claim_saga_commands_v1(
    p_worker_id text,
    p_command_kinds text[],
    p_limit integer default 10,
    p_lease_seconds integer default 300
)
returns setof public.cex_saga_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if p_worker_id is null or btrim(p_worker_id) = '' or length(p_worker_id) > 128 then
        raise exception 'worker_id must contain 1..128 characters';
    end if;
    if p_command_kinds is null or cardinality(p_command_kinds) = 0 then
        raise exception 'at least one command kind is required';
    end if;
    if p_limit < 1 or p_limit > 100 then
        raise exception 'claim limit must be between 1 and 100';
    end if;
    if p_lease_seconds < 1 or p_lease_seconds > 3600 then
        raise exception 'lease seconds must be between 1 and 3600';
    end if;

    return query
    with candidates as (
        select command_id
          from public.cex_saga_commands_v1
         where execution_mode = 'active'
           and command_kind = any(p_command_kinds)
           and attempt_count < max_attempts
           and available_at <= now()
           and (
               status in ('pending', 'retry_wait')
               or (status = 'claimed' and lease_expires_at <= now())
           )
         order by available_at, created_at, command_id
         for update skip locked
         limit p_limit
    )
    update public.cex_saga_commands_v1 command
       set status = 'claimed',
           attempt_count = command.attempt_count + 1,
           claimed_by = p_worker_id,
           lease_expires_at = now() + make_interval(secs => p_lease_seconds),
           updated_at = now()
      from candidates
     where command.command_id = candidates.command_id
    returning command.*;
end
$$;

drop index if exists public.idx_cex_saga_commands_claim_v1;
create index idx_cex_saga_commands_claim_v1
    on public.cex_saga_commands_v1 (
        execution_mode, command_kind, status, available_at, created_at
    )
    where status in ('pending', 'claimed', 'retry_wait');

-- PostgreSQL permits CREATE OR REPLACE VIEW to append columns, but not to
-- insert them before existing columns. Preserve the 0057 public column order
-- and append execution_mode so both fresh and 0057->0058 upgrades are valid.
create or replace view public.cex_saga_queue_summary_v1 as
select
    command_kind,
    status,
    count(*)::bigint as command_count,
    min(available_at) as oldest_available_at,
    min(lease_expires_at) filter (where status = 'claimed') as oldest_lease_expiry,
    count(*) filter (where attempt_count >= max_attempts)::bigint as retry_budget_exhausted,
    execution_mode
from public.cex_saga_commands_v1
group by command_kind, status, execution_mode;

commit;