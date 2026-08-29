begin;

-- P0-N6: provider I/O is represented by a durable command. Network calls happen after claim commit.

create table if not exists public.cex_provider_dispatch_commands_v1 (
    command_id uuid primary key,
    execution_id uuid not null unique references public.executions(execution_id),
    invocation_id uuid not null references public.invocations(invocation_id),
    org_id uuid not null references public.organizations(org_id),
    trace_id uuid not null,
    provider_target text not null,
    prompt_sha256 text not null,
    request_fingerprint text not null,
    status text not null default 'pending',
    attempt_count integer not null default 0,
    max_attempts integer not null default 3,
    available_at timestamptz not null default now(),
    claimed_by text,
    lease_expires_at timestamptz,
    last_http_status integer,
    last_error_code text,
    last_error_message text,
    result_payload jsonb,
    result_sha256 text,
    acknowledged_by text,
    acknowledged_reason text,
    acknowledged_at timestamptz,
    last_requeued_by text,
    last_requeue_reason text,
    last_requeue_additional_attempts integer,
    last_requeued_at timestamptz,
    requeue_count integer not null default 0,
    completed_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint cex_provider_dispatch_non_nil_v1 check (
        command_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and execution_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and invocation_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and org_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and trace_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ),
    constraint cex_provider_dispatch_target_v1 check (
        length(provider_target) between 4 and 512 and provider_target like '%://%'
    ),
    constraint cex_provider_dispatch_hash_v1 check (
        prompt_sha256 ~ '^sha256:[0-9a-f]{64}$'
        and request_fingerprint ~ '^sha256:[0-9a-f]{64}$'
        and (result_sha256 is null or result_sha256 ~ '^sha256:[0-9a-f]{64}$')
    ),
    constraint cex_provider_dispatch_status_v1 check (status in (
        'pending','claimed','retry_wait','succeeded','reconcile_required','dead_letter','cancelled'
    )),
    constraint cex_provider_dispatch_attempts_v1 check (
        max_attempts between 1 and 100 and attempt_count between 0 and max_attempts
        and requeue_count between 0 and 100
    ),
    constraint cex_provider_dispatch_claim_v1 check (
        (status = 'claimed' and claimed_by is not null and lease_expires_at is not null)
        or (status <> 'claimed' and claimed_by is null and lease_expires_at is null)
    ),
    constraint cex_provider_dispatch_result_v1 check (
        (status = 'succeeded' and result_payload is not null and result_sha256 is not null and completed_at is not null)
        or (status <> 'succeeded' and completed_at is null)
    ),
    constraint cex_provider_dispatch_text_v1 check (
        (claimed_by is null or length(btrim(claimed_by)) between 1 and 128)
        and (last_error_code is null or length(last_error_code) between 1 and 128)
        and (last_error_message is null or length(last_error_message) <= 2000)
        and (acknowledged_by is null or length(btrim(acknowledged_by)) between 1 and 256)
        and (acknowledged_reason is null or length(btrim(acknowledged_reason)) between 1 and 1000)
        and (last_requeued_by is null or length(btrim(last_requeued_by)) between 1 and 256)
        and (last_requeue_reason is null or length(btrim(last_requeue_reason)) between 1 and 1000)
    )
);

create table if not exists public.cex_provider_dispatch_transitions_v1 (
    transition_id bigserial primary key,
    command_id uuid not null references public.cex_provider_dispatch_commands_v1(command_id),
    from_status text,
    to_status text not null,
    attempt_count integer not null,
    worker_id text,
    error_code text,
    result_sha256 text,
    occurred_at timestamptz not null default now()
);

create or replace function public.cex_provider_dispatch_fingerprint_v1(
    p_execution_id uuid,
    p_invocation_id uuid,
    p_org_id uuid,
    p_trace_id uuid,
    p_provider_target text,
    p_prompt_sha256 text
)
returns text
language sql
immutable
set search_path = pg_catalog, public
as $$
    select 'sha256:' || encode(digest(jsonb_build_object(
        'execution_id', p_execution_id,
        'invocation_id', p_invocation_id,
        'org_id', p_org_id,
        'trace_id', p_trace_id,
        'provider_target', p_provider_target,
        'prompt_sha256', p_prompt_sha256,
        'schema_version', 'cex.provider.dispatch.v1'
    )::text, 'sha256'), 'hex')
$$;

create or replace function public.cex_guard_provider_backed_execution_transition_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if old.provider_target is not null
       and new.status is distinct from old.status
       and new.status in ('Running','Succeeded','Failed','Cancelled','TimedOut','Refunded')
       and coalesce(current_setting('cex.provider_dispatch_v1', true), '') <> 'enabled' then
        raise exception 'provider-backed execution transitions require durable provider dispatch authority';
    end if;
    return new;
end
$$;

drop trigger if exists trg_cex_guard_provider_backed_execution_transition_v1 on public.executions;
create trigger trg_cex_guard_provider_backed_execution_transition_v1
before update of status on public.executions
for each row execute function public.cex_guard_provider_backed_execution_transition_v1();

create or replace function public.cex_record_provider_dispatch_transition_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if tg_op = 'INSERT' then
        insert into public.cex_provider_dispatch_transitions_v1 (
            command_id, from_status, to_status, attempt_count, worker_id, error_code, result_sha256
        ) values (new.command_id, null, new.status, new.attempt_count, new.claimed_by, new.last_error_code, new.result_sha256);
    elsif new.status is distinct from old.status then
        insert into public.cex_provider_dispatch_transitions_v1 (
            command_id, from_status, to_status, attempt_count, worker_id, error_code, result_sha256
        ) values (new.command_id, old.status, new.status, new.attempt_count,
                  coalesce(old.claimed_by,new.claimed_by), new.last_error_code, new.result_sha256);
    end if;
    return new;
end
$$;

drop trigger if exists trg_cex_record_provider_dispatch_transition_v1 on public.cex_provider_dispatch_commands_v1;
create trigger trg_cex_record_provider_dispatch_transition_v1
after insert or update of status on public.cex_provider_dispatch_commands_v1
for each row execute function public.cex_record_provider_dispatch_transition_v1();

create or replace function public.cex_reject_provider_dispatch_evidence_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'provider dispatch transition evidence is append-only';
end
$$;

drop trigger if exists trg_cex_reject_provider_dispatch_transition_mutation_v1
    on public.cex_provider_dispatch_transitions_v1;
create trigger trg_cex_reject_provider_dispatch_transition_mutation_v1
before update or delete on public.cex_provider_dispatch_transitions_v1
for each row execute function public.cex_reject_provider_dispatch_evidence_mutation_v1();

create or replace function public.cex_enqueue_provider_dispatch_v1(
    p_execution_id uuid,
    p_requested_by text,
    p_expected_worker_id text default null,
    p_max_attempts integer default 3
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    execution_row public.executions%rowtype;
    invocation_row public.invocations%rowtype;
    prompt_value text;
    prompt_sha_value text;
    command_id_value uuid;
    fingerprint_value text;
    existing_row public.cex_provider_dispatch_commands_v1%rowtype;
    command_row public.cex_provider_dispatch_commands_v1%rowtype;
    event_id_value uuid;
    envelope_value jsonb;
begin
    if p_execution_id is null or p_execution_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'provider execution_id must be non-nil';
    end if;
    if p_requested_by is null or length(btrim(p_requested_by)) not between 1 and 256 then
        raise exception 'provider dispatch requester is invalid';
    end if;
    if p_max_attempts not between 1 and 100 then
        raise exception 'provider max_attempts must be between 1 and 100';
    end if;

    select * into execution_row from public.executions where execution_id=p_execution_id for update;
    if not found then raise exception using errcode='P0002', message='execution not found'; end if;
    if execution_row.provider_target is null then
        raise exception 'execution has no provider target';
    end if;
    if execution_row.status not in ('Approved','Queued','Dispatching') then
        raise exception 'execution cannot enqueue provider dispatch from status %', execution_row.status;
    end if;
    if p_expected_worker_id is not null and (
        execution_row.worker_id is distinct from btrim(p_expected_worker_id)
        or execution_row.lease_expires_at is null
        or execution_row.lease_expires_at <= now()
    ) then
        raise exception 'provider dispatch requires the active Execution worker lease';
    end if;

    select * into invocation_row from public.invocations
     where invocation_id=execution_row.invocation_id for share;
    if not found then raise exception 'provider dispatch Invocation not found'; end if;
    prompt_value := invocation_row.request_payload ->> 'prompt';
    if prompt_value is null or length(prompt_value) not between 1 and 1048576 then
        raise exception 'provider dispatch prompt is missing or too large';
    end if;
    prompt_sha_value := 'sha256:' || encode(digest(prompt_value, 'sha256'), 'hex');
    command_id_value := public.cex_deterministic_uuid_v1(
        'provider-dispatch:' || execution_row.execution_id::text
    );
    fingerprint_value := public.cex_provider_dispatch_fingerprint_v1(
        execution_row.execution_id, execution_row.invocation_id, execution_row.org_id,
        execution_row.trace_id, execution_row.provider_target, prompt_sha_value
    );

    select * into existing_row from public.cex_provider_dispatch_commands_v1
     where execution_id=execution_row.execution_id for update;
    if found then
        if existing_row.command_id is distinct from command_id_value
           or existing_row.invocation_id is distinct from execution_row.invocation_id
           or existing_row.org_id is distinct from execution_row.org_id
           or existing_row.trace_id is distinct from execution_row.trace_id
           or existing_row.provider_target is distinct from execution_row.provider_target
           or existing_row.prompt_sha256 is distinct from prompt_sha_value
           or existing_row.request_fingerprint is distinct from fingerprint_value then
            raise exception using errcode='23505', message='provider dispatch immutable collision';
        end if;
        return jsonb_build_object('replayed', true, 'command', to_jsonb(existing_row));
    end if;

    insert into public.cex_provider_dispatch_commands_v1 (
        command_id, execution_id, invocation_id, org_id, trace_id, provider_target,
        prompt_sha256, request_fingerprint, max_attempts
    ) values (
        command_id_value, execution_row.execution_id, execution_row.invocation_id,
        execution_row.org_id, execution_row.trace_id, execution_row.provider_target,
        prompt_sha_value, fingerprint_value, p_max_attempts
    ) returning * into command_row;

    perform set_config('cex.provider_dispatch_v1','enabled',true);
    update public.executions set status='Dispatching', updated_at=now()
     where execution_id=execution_row.execution_id;
    update public.invocations set status='Dispatching', execution_id=execution_row.execution_id, updated_at=now()
     where invocation_id=execution_row.invocation_id;

    event_id_value := public.cex_deterministic_uuid_v1('provider-dispatch-commanded:' || command_id_value::text);
    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', execution_row.trace_id,
        'org_id', execution_row.org_id,
        'actor_type', 'execution-service',
        'actor_id', btrim(p_requested_by),
        'event_type', 'execution.provider_dispatch.commanded',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', command_row.created_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service','execution-service',
            'command_id',command_id_value,
            'execution_id',execution_row.execution_id,
            'invocation_id',execution_row.invocation_id,
            'provider_target',execution_row.provider_target,
            'prompt_sha256',prompt_sha_value,
            'request_fingerprint',fingerprint_value
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'execution-service', event_id_value, execution_row.trace_id,
        execution_row.org_id, envelope_value, 10
    );
    return jsonb_build_object('replayed', false, 'command', to_jsonb(command_row));
end
$$;

create or replace function public.cex_claim_provider_dispatch_v1(
    p_worker_id text,
    p_limit integer default 2,
    p_lease_seconds integer default 90
)
returns setof public.cex_provider_dispatch_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if p_worker_id is null or length(btrim(p_worker_id)) not between 1 and 128 then
        raise exception 'provider worker ID is invalid';
    end if;
    if p_limit not between 1 and 25 or p_lease_seconds not between 5 and 3600 then
        raise exception 'provider claim bounds are invalid';
    end if;

    update public.cex_provider_dispatch_commands_v1
       set status = case when attempt_count >= max_attempts then 'reconcile_required' else 'retry_wait' end,
           claimed_by=null, lease_expires_at=null, available_at=now(),
           last_error_code=case when attempt_count >= max_attempts
             then 'provider_final_lease_expired_unknown_outcome'
             else 'provider_lease_expired_exact_replay' end,
           last_error_message='provider claim lease expired; remote outcome was not assumed',
           updated_at=now()
     where status='claimed' and lease_expires_at <= now();

    return query
    with candidates as (
        select command_id from public.cex_provider_dispatch_commands_v1
        where status in ('pending','retry_wait') and available_at <= now()
          and attempt_count < max_attempts
        order by available_at, created_at, command_id
        for update skip locked limit p_limit
    )
    update public.cex_provider_dispatch_commands_v1 command
       set status='claimed', attempt_count=command.attempt_count+1,
           claimed_by=btrim(p_worker_id),
           lease_expires_at=now()+make_interval(secs=>p_lease_seconds),
           updated_at=now()
      from candidates where command.command_id=candidates.command_id
    returning command.*;
end
$$;

create or replace function public.cex_finish_provider_dispatch_v1(
    p_command_id uuid,
    p_worker_id text,
    p_outcome text,
    p_result_payload jsonb default null,
    p_http_status integer default null,
    p_error_code text default null,
    p_error_message text default null,
    p_retry_after_seconds integer default null
)
returns public.cex_provider_dispatch_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    command_row public.cex_provider_dispatch_commands_v1%rowtype;
    result_hash_value text;
    retry_delay integer;
begin
    if p_outcome not in ('succeeded','retry_wait','reconcile_required','dead_letter') then
        raise exception 'unsupported provider outcome';
    end if;
    select * into command_row from public.cex_provider_dispatch_commands_v1
     where command_id=p_command_id for update;
    if not found then raise exception using errcode='P0002', message='provider command not found'; end if;
    if command_row.status <> 'claimed' or command_row.claimed_by is distinct from btrim(p_worker_id)
       or command_row.lease_expires_at is null or command_row.lease_expires_at <= now() then
        raise exception 'provider outcome requires ownership of a live claim';
    end if;

    if p_outcome='succeeded' then
        if p_result_payload is null or jsonb_typeof(p_result_payload) <> 'object' then
            raise exception 'provider success requires an object result';
        end if;
        result_hash_value := 'sha256:' || encode(digest(p_result_payload::text,'sha256'),'hex');
        update public.cex_provider_dispatch_commands_v1 set
            status='succeeded', claimed_by=null, lease_expires_at=null,
            result_payload=p_result_payload, result_sha256=result_hash_value,
            last_http_status=p_http_status, last_error_code=null, last_error_message=null,
            completed_at=now(), updated_at=now()
        where command_id=p_command_id returning * into command_row;
        perform set_config('cex.provider_dispatch_v1','enabled',true);
        update public.executions set
            status='Succeeded', started_at=coalesce(started_at,now()), ended_at=now(),
            result_payload=p_result_payload, worker_id=null, lease_expires_at=null, updated_at=now()
        where execution_id=command_row.execution_id;
        update public.invocations set
            status='Succeeded', execution_id=command_row.execution_id, failure_reason=null, updated_at=now()
        where invocation_id=command_row.invocation_id;
    elsif p_outcome='retry_wait' and command_row.attempt_count < command_row.max_attempts then
        retry_delay := greatest(coalesce(p_retry_after_seconds,0),
            least(3600,power(2,least(greatest(command_row.attempt_count-1,0),10))::integer));
        update public.cex_provider_dispatch_commands_v1 set
            status='retry_wait', claimed_by=null, lease_expires_at=null,
            available_at=now()+make_interval(secs=>retry_delay),
            last_http_status=p_http_status,
            last_error_code=left(coalesce(nullif(btrim(p_error_code),''),'provider_retryable'),128),
            last_error_message=left(coalesce(p_error_message,''),2000), updated_at=now()
        where command_id=p_command_id returning * into command_row;
        update public.executions set status='Queued', worker_id=null, lease_expires_at=null, updated_at=now()
         where execution_id=command_row.execution_id;
        update public.invocations set status='Queued', failure_reason=null, updated_at=now()
         where invocation_id=command_row.invocation_id;
    elsif p_outcome='reconcile_required' or p_outcome='retry_wait' then
        update public.cex_provider_dispatch_commands_v1 set
            status='reconcile_required', claimed_by=null, lease_expires_at=null,
            last_http_status=p_http_status,
            last_error_code=left(coalesce(nullif(btrim(p_error_code),''),'provider_unknown_remote_outcome'),128),
            last_error_message=left(coalesce(p_error_message,''),2000), updated_at=now()
        where command_id=p_command_id returning * into command_row;
    else
        update public.cex_provider_dispatch_commands_v1 set
            status='dead_letter', claimed_by=null, lease_expires_at=null,
            last_http_status=p_http_status,
            last_error_code=left(coalesce(nullif(btrim(p_error_code),''),'provider_permanent_rejection'),128),
            last_error_message=left(coalesce(p_error_message,''),2000), updated_at=now()
        where command_id=p_command_id returning * into command_row;
        perform set_config('cex.provider_dispatch_v1','enabled',true);
        update public.executions set status='Failed', ended_at=now(), worker_id=null,
            lease_expires_at=null, result_payload=jsonb_build_object(
                'provider_target',command_row.provider_target,
                'error_code',command_row.last_error_code,
                'error',command_row.last_error_message
            ), updated_at=now()
         where execution_id=command_row.execution_id;
        update public.invocations set status='Failed', execution_id=command_row.execution_id,
            failure_reason=command_row.last_error_message, updated_at=now()
         where invocation_id=command_row.invocation_id;
    end if;
    return command_row;
end
$$;

create or replace function public.cex_transition_provider_execution_terminal_v1(
    p_execution_id uuid,
    p_status text,
    p_actor text,
    p_reason text default null
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare execution_row public.executions%rowtype;
begin
    if p_status not in ('Succeeded','Failed','Cancelled','TimedOut') then
        raise exception 'unsupported terminal execution status';
    end if;
    if p_actor is null or length(btrim(p_actor)) not between 1 and 256 then
        raise exception 'terminal actor is invalid';
    end if;
    select * into execution_row from public.executions where execution_id=p_execution_id for update;
    if not found then raise exception using errcode='P0002', message='execution not found'; end if;
    if execution_row.status = p_status then return to_jsonb(execution_row); end if;
    if execution_row.provider_target is not null and p_status='Succeeded' then
        raise exception 'provider-backed success must be recorded by the provider worker';
    end if;
    perform set_config('cex.provider_dispatch_v1','enabled',true);
    update public.executions set status=p_status, ended_at=case when p_status in ('Succeeded','Failed','Cancelled','TimedOut') then now() else ended_at end,
        worker_id=null, lease_expires_at=null,
        result_payload=coalesce(result_payload,'{}'::jsonb) || jsonb_build_object(
            'terminal_actor',btrim(p_actor),'terminal_reason',p_reason
        ), updated_at=now()
    where execution_id=p_execution_id returning * into execution_row;
    update public.invocations set status=p_status, execution_id=p_execution_id,
        failure_reason=case when p_status='Succeeded' then null else p_reason end, updated_at=now()
    where invocation_id=execution_row.invocation_id;
    return to_jsonb(execution_row);
end
$$;

create or replace function public.cex_acknowledge_provider_dispatch_v1(
    p_command_id uuid, p_actor text, p_reason text
)
returns public.cex_provider_dispatch_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare row_value public.cex_provider_dispatch_commands_v1%rowtype;
begin
    if length(btrim(p_actor)) not between 1 and 256 or length(btrim(p_reason)) not between 1 and 1000 then
        raise exception 'provider acknowledgement actor/reason is invalid';
    end if;
    select * into row_value from public.cex_provider_dispatch_commands_v1 where command_id=p_command_id for update;
    if row_value.status not in ('reconcile_required','dead_letter') then
        raise exception 'provider command is not awaiting operator acknowledgement';
    end if;
    if row_value.acknowledged_at is not null then
        if row_value.acknowledged_by is distinct from btrim(p_actor)
           or row_value.acknowledged_reason is distinct from btrim(p_reason) then
            raise exception using errcode='23505', message='provider acknowledgement collision';
        end if;
        return row_value;
    end if;
    update public.cex_provider_dispatch_commands_v1 set
        acknowledged_by=btrim(p_actor), acknowledged_reason=btrim(p_reason), acknowledged_at=now(), updated_at=now()
    where command_id=p_command_id returning * into row_value;
    return row_value;
end
$$;

create or replace function public.cex_requeue_provider_dispatch_v1(
    p_command_id uuid, p_actor text, p_reason text, p_additional_attempts integer default 1
)
returns public.cex_provider_dispatch_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare row_value public.cex_provider_dispatch_commands_v1%rowtype;
begin
    if p_additional_attempts not between 1 and 20 then raise exception 'provider additional attempts invalid'; end if;
    select * into row_value from public.cex_provider_dispatch_commands_v1 where command_id=p_command_id for update;
    if row_value.status not in ('reconcile_required','dead_letter') or row_value.acknowledged_at is null then
        raise exception 'provider requeue requires a fresh acknowledgement';
    end if;
    if row_value.max_attempts+p_additional_attempts > 100 then raise exception 'provider attempt budget overflow'; end if;
    update public.cex_provider_dispatch_commands_v1 set
        status='pending', max_attempts=max_attempts+p_additional_attempts,
        available_at=now(), acknowledged_by=null, acknowledged_reason=null, acknowledged_at=null,
        last_requeued_by=btrim(p_actor), last_requeue_reason=btrim(p_reason),
        last_requeue_additional_attempts=p_additional_attempts, last_requeued_at=now(),
        requeue_count=requeue_count+1, completed_at=null, updated_at=now()
    where command_id=p_command_id returning * into row_value;
    update public.executions set status='Queued', worker_id=null, lease_expires_at=null, updated_at=now()
     where execution_id=row_value.execution_id;
    update public.invocations set status='Queued', failure_reason=null, updated_at=now()
     where invocation_id=row_value.invocation_id;
    return row_value;
end
$$;

create or replace view public.cex_provider_dispatch_status_v1 as
select status, count(*)::bigint as command_count,
       count(*) filter (where attempt_count>=max_attempts)::bigint as exhausted_count,
       count(*) filter (where status in ('reconcile_required','dead_letter') and acknowledged_at is null)::bigint
           as unacknowledged_count,
       min(available_at) filter (where status in ('pending','retry_wait')) as oldest_available_at,
       min(lease_expires_at) filter (where status='claimed') as oldest_lease_expiry
from public.cex_provider_dispatch_commands_v1 group by status;

create index if not exists idx_cex_provider_dispatch_claim_v1
    on public.cex_provider_dispatch_commands_v1(status, available_at, created_at, command_id)
    where status in ('pending','claimed','retry_wait');

commit;
