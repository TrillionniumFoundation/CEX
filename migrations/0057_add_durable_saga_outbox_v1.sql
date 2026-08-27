begin;

create table if not exists public.cex_saga_commands_v1 (
    command_id uuid primary key default gen_random_uuid(),
    workflow_kind text not null,
    workflow_id uuid not null,
    org_id uuid,
    operation_key text not null unique,
    command_kind text not null,
    status text not null default 'pending',
    attempt_count integer not null default 0,
    max_attempts integer not null default 3,
    available_at timestamptz not null default now(),
    claimed_by text,
    lease_expires_at timestamptz,
    payload jsonb not null default '{}'::jsonb,
    last_error_code text,
    last_error_message text,
    completed_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint cex_saga_commands_workflow_kind_v1
        check (workflow_kind ~ '^[a-z0-9][a-z0-9._-]{0,127}$'),
    constraint cex_saga_commands_operation_key_v1
        check (length(operation_key) between 1 and 256),
    constraint cex_saga_commands_kind_v1
        check (command_kind in (
            'ledger_reserve',
            'execution_create',
            'provider_dispatch',
            'ledger_consume',
            'ledger_refund',
            'audit_deliver',
            'reconcile'
        )),
    constraint cex_saga_commands_status_v1
        check (status in (
            'pending',
            'claimed',
            'retry_wait',
            'succeeded',
            'failed',
            'dead_letter',
            'cancelled'
        )),
    constraint cex_saga_commands_attempt_budget_v1
        check (
            max_attempts between 1 and 100
            and attempt_count between 0 and max_attempts
        ),
    constraint cex_saga_commands_payload_object_v1
        check (jsonb_typeof(payload) = 'object'),
    constraint cex_saga_commands_claim_shape_v1
        check (
            (status = 'claimed' and claimed_by is not null and lease_expires_at is not null)
            or
            (status <> 'claimed' and claimed_by is null and lease_expires_at is null)
        ),
    constraint cex_saga_commands_completion_shape_v1
        check (
            (status = 'succeeded' and completed_at is not null)
            or
            (status <> 'succeeded' and completed_at is null)
        )
);

create table if not exists public.cex_saga_receipts_v1 (
    receipt_id uuid primary key default gen_random_uuid(),
    command_id uuid not null references public.cex_saga_commands_v1(command_id),
    source_system text not null,
    receipt_key text not null,
    outcome text not null,
    payload jsonb not null default '{}'::jsonb,
    observed_at timestamptz not null default now(),
    created_at timestamptz not null default now(),
    constraint cex_saga_receipts_source_v1
        check (source_system ~ '^[a-z0-9][a-z0-9._-]{0,127}$'),
    constraint cex_saga_receipts_key_v1
        check (length(receipt_key) between 1 and 256),
    constraint cex_saga_receipts_outcome_v1
        check (outcome ~ '^[a-z0-9][a-z0-9._-]{0,127}$'),
    constraint cex_saga_receipts_payload_object_v1
        check (jsonb_typeof(payload) = 'object'),
    constraint cex_saga_receipts_dedupe_v1
        unique (source_system, receipt_key)
);

create table if not exists public.cex_saga_transitions_v1 (
    transition_id bigserial primary key,
    command_id uuid not null references public.cex_saga_commands_v1(command_id),
    from_status text,
    to_status text not null,
    attempt_count integer not null,
    worker_id text,
    error_code text,
    occurred_at timestamptz not null default now()
);

create or replace function public.cex_validate_saga_transition_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if new.status is not distinct from old.status then
        return new;
    end if;

    if old.status = 'pending' and new.status in ('claimed', 'cancelled') then
        return new;
    elsif old.status = 'claimed' and new.status in (
        'succeeded', 'retry_wait', 'failed', 'dead_letter', 'cancelled'
    ) then
        return new;
    elsif old.status = 'retry_wait' and new.status in ('claimed', 'dead_letter', 'cancelled') then
        return new;
    elsif old.status = 'failed' and new.status in ('retry_wait', 'dead_letter', 'cancelled') then
        return new;
    end if;

    raise exception 'invalid saga command transition % -> % for command %',
        old.status, new.status, old.command_id;
end
$$;

create or replace function public.cex_record_saga_transition_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if tg_op = 'INSERT' then
        insert into public.cex_saga_transitions_v1 (
            command_id, from_status, to_status, attempt_count, worker_id, error_code
        ) values (
            new.command_id, null, new.status, new.attempt_count, new.claimed_by, new.last_error_code
        );
    elsif new.status is distinct from old.status then
        insert into public.cex_saga_transitions_v1 (
            command_id, from_status, to_status, attempt_count, worker_id, error_code
        ) values (
            new.command_id, old.status, new.status, new.attempt_count,
            coalesce(new.claimed_by, old.claimed_by), new.last_error_code
        );
    end if;
    return new;
end
$$;

drop trigger if exists trg_cex_validate_saga_transition_v1
    on public.cex_saga_commands_v1;
create trigger trg_cex_validate_saga_transition_v1
before update of status on public.cex_saga_commands_v1
for each row execute function public.cex_validate_saga_transition_v1();

drop trigger if exists trg_cex_record_saga_transition_v1
    on public.cex_saga_commands_v1;
create trigger trg_cex_record_saga_transition_v1
after insert or update of status on public.cex_saga_commands_v1
for each row execute function public.cex_record_saga_transition_v1();

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
         where command_kind = any(p_command_kinds)
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

create index if not exists idx_cex_saga_commands_claim_v1
    on public.cex_saga_commands_v1 (
        command_kind, status, available_at, created_at
    )
    where status in ('pending', 'claimed', 'retry_wait');

create index if not exists idx_cex_saga_commands_workflow_v1
    on public.cex_saga_commands_v1 (workflow_kind, workflow_id, created_at);

create index if not exists idx_cex_saga_commands_org_v1
    on public.cex_saga_commands_v1 (org_id, created_at)
    where org_id is not null;

create index if not exists idx_cex_saga_receipts_command_v1
    on public.cex_saga_receipts_v1 (command_id, observed_at);

create index if not exists idx_cex_saga_transitions_command_v1
    on public.cex_saga_transitions_v1 (command_id, transition_id);

create or replace view public.cex_saga_queue_summary_v1 as
select
    command_kind,
    status,
    count(*)::bigint as command_count,
    min(available_at) as oldest_available_at,
    min(lease_expires_at) filter (where status = 'claimed') as oldest_lease_expiry,
    count(*) filter (where attempt_count >= max_attempts)::bigint as retry_budget_exhausted
from public.cex_saga_commands_v1
group by command_kind, status;

commit;
