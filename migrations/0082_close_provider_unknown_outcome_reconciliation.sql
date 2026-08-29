begin;

-- A provider claim may expire after the remote side effect succeeded. Lease expiry
-- is therefore always an unknown outcome, never an automatic retry signal.

create table if not exists public.cex_provider_reconciliation_evidence_v1 (
    evidence_id uuid primary key,
    command_id uuid not null references public.cex_provider_dispatch_commands_v1(command_id),
    incident_attempt_count integer not null,
    disposition text not null,
    artifact_uri text not null,
    artifact_sha256 text not null,
    evidence jsonb not null,
    result_payload jsonb,
    recorded_by text not null,
    recorded_at timestamptz not null default now(),
    constraint cex_provider_reconciliation_attempt_v1 check (incident_attempt_count between 1 and 100),
    constraint cex_provider_reconciliation_disposition_v1 check (
        disposition in ('confirmed_not_executed','confirmed_executed','indeterminate')
    ),
    constraint cex_provider_reconciliation_artifact_v1 check (
        artifact_uri ~ '^(https://|gh://|oci://|s3://|gs://|file://)[^[:space:]]+$'
        and artifact_sha256 ~ '^sha256:[0-9a-f]{64}$'
    ),
    constraint cex_provider_reconciliation_evidence_v1 check (
        jsonb_typeof(evidence)='object'
        and (disposition <> 'confirmed_executed'
             or (result_payload is not null and jsonb_typeof(result_payload)='object'))
        and length(btrim(recorded_by)) between 1 and 256
    ),
    unique (command_id, incident_attempt_count)
);

create or replace function public.cex_reject_provider_reconciliation_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'provider reconciliation evidence is append-only';
end
$$;

drop trigger if exists trg_cex_reject_provider_reconciliation_mutation_v1
    on public.cex_provider_reconciliation_evidence_v1;
create trigger trg_cex_reject_provider_reconciliation_mutation_v1
before update or delete on public.cex_provider_reconciliation_evidence_v1
for each row execute function public.cex_reject_provider_reconciliation_mutation_v1();

create or replace function public.cex_reconcile_expired_provider_claims_v1()
returns bigint
language plpgsql
set search_path = pg_catalog, public
as $$
declare affected bigint;
begin
    update public.cex_provider_dispatch_commands_v1
       set status='reconcile_required',
           claimed_by=null,
           lease_expires_at=null,
           last_error_code='provider_lease_expired_unknown_outcome',
           last_error_message='provider claim lease expired after dispatch may have started; external reconciliation is required',
           updated_at=now()
     where status='claimed' and lease_expires_at <= now();
    get diagnostics affected = row_count;
    return affected;
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

    perform public.cex_reconcile_expired_provider_claims_v1();

    return query
    with candidates as (
        select command_id
          from public.cex_provider_dispatch_commands_v1
         where status in ('pending','retry_wait')
           and available_at <= now()
           and attempt_count < max_attempts
         order by available_at, created_at, command_id
         for update skip locked
         limit p_limit
    )
    update public.cex_provider_dispatch_commands_v1 command
       set status='claimed',
           attempt_count=command.attempt_count+1,
           claimed_by=btrim(p_worker_id),
           lease_expires_at=now()+make_interval(secs=>p_lease_seconds),
           updated_at=now()
      from candidates
     where command.command_id=candidates.command_id
    returning command.*;
end
$$;

create or replace function public.cex_guard_provider_retry_classification_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if new.status='retry_wait' and new.status is distinct from old.status then
        if old.status <> 'claimed'
           or new.last_error_code not in (
               'provider_rate_limited',
               'provider_definitive_not_executed'
           ) then
            raise exception 'provider retry_wait requires definitive not-executed evidence';
        end if;
    end if;
    return new;
end
$$;

drop trigger if exists trg_cex_guard_provider_retry_classification_v1
    on public.cex_provider_dispatch_commands_v1;
create trigger trg_cex_guard_provider_retry_classification_v1
before update of status on public.cex_provider_dispatch_commands_v1
for each row execute function public.cex_guard_provider_retry_classification_v1();

create or replace function public.cex_record_provider_reconciliation_v1(
    p_command_id uuid,
    p_actor text,
    p_disposition text,
    p_artifact_uri text,
    p_artifact_sha256 text,
    p_evidence jsonb,
    p_result_payload jsonb default null
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    command_row public.cex_provider_dispatch_commands_v1%rowtype;
    existing_row public.cex_provider_reconciliation_evidence_v1%rowtype;
    evidence_row public.cex_provider_reconciliation_evidence_v1%rowtype;
    evidence_id_value uuid;
    result_hash_value text;
    event_id_value uuid;
    envelope_value jsonb;
begin
    if p_command_id is null or p_command_id='00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'provider reconciliation command_id must be non-nil';
    end if;
    if p_actor is null or length(btrim(p_actor)) not between 1 and 256 then
        raise exception 'provider reconciliation actor is invalid';
    end if;
    if p_disposition not in ('confirmed_not_executed','confirmed_executed','indeterminate') then
        raise exception 'provider reconciliation disposition is invalid';
    end if;
    if p_artifact_uri is null
       or p_artifact_uri !~ '^(https://|gh://|oci://|s3://|gs://|file://)[^[:space:]]+$'
       or p_artifact_sha256 is null
       or p_artifact_sha256 !~ '^sha256:[0-9a-f]{64}$' then
        raise exception 'provider reconciliation requires an immutable artifact URI and SHA-256';
    end if;
    if p_evidence is null or jsonb_typeof(p_evidence) <> 'object' then
        raise exception 'provider reconciliation evidence must be an object';
    end if;
    if p_disposition='confirmed_executed'
       and (p_result_payload is null or jsonb_typeof(p_result_payload) <> 'object') then
        raise exception 'confirmed provider execution requires an object result payload';
    end if;

    select * into command_row
      from public.cex_provider_dispatch_commands_v1
     where command_id=p_command_id
     for update;
    if not found then
        raise exception using errcode='P0002', message='provider command not found';
    end if;
    if command_row.status not in ('reconcile_required','dead_letter') then
        raise exception 'provider command is not awaiting reconciliation evidence';
    end if;

    select * into existing_row
      from public.cex_provider_reconciliation_evidence_v1
     where command_id=p_command_id
       and incident_attempt_count=command_row.attempt_count;
    if found then
        if existing_row.disposition is distinct from p_disposition
           or existing_row.artifact_uri is distinct from p_artifact_uri
           or existing_row.artifact_sha256 is distinct from p_artifact_sha256
           or existing_row.evidence is distinct from p_evidence
           or existing_row.result_payload is distinct from p_result_payload
           or existing_row.recorded_by is distinct from btrim(p_actor) then
            raise exception using errcode='23505', message='provider reconciliation evidence collision';
        end if;
        return jsonb_build_object('replayed',true,'evidence',to_jsonb(existing_row),'command',to_jsonb(command_row));
    end if;

    evidence_id_value := public.cex_deterministic_uuid_v1(
        'provider-reconciliation:' || p_command_id::text || ':' || command_row.attempt_count::text
    );
    insert into public.cex_provider_reconciliation_evidence_v1 (
        evidence_id,command_id,incident_attempt_count,disposition,
        artifact_uri,artifact_sha256,evidence,result_payload,recorded_by
    ) values (
        evidence_id_value,p_command_id,command_row.attempt_count,p_disposition,
        p_artifact_uri,p_artifact_sha256,p_evidence,p_result_payload,btrim(p_actor)
    ) returning * into evidence_row;

    update public.cex_provider_dispatch_commands_v1
       set acknowledged_by=null,
           acknowledged_reason=null,
           acknowledged_at=null,
           updated_at=now()
     where command_id=p_command_id
    returning * into command_row;

    if p_disposition='confirmed_executed' then
        result_hash_value := 'sha256:' || encode(digest(p_result_payload::text,'sha256'),'hex');
        update public.cex_provider_dispatch_commands_v1
           set status='succeeded',
               result_payload=p_result_payload,
               result_sha256=result_hash_value,
               last_error_code=null,
               last_error_message=null,
               completed_at=now(),
               updated_at=now()
         where command_id=p_command_id
        returning * into command_row;

        perform set_config('cex.provider_dispatch_v1','enabled',true);
        update public.executions
           set status='Succeeded',
               started_at=coalesce(started_at,now()),
               ended_at=now(),
               result_payload=p_result_payload,
               worker_id=null,
               lease_expires_at=null,
               updated_at=now()
         where execution_id=command_row.execution_id;
        update public.invocations
           set status='Succeeded',
               execution_id=command_row.execution_id,
               failure_reason=null,
               updated_at=now()
         where invocation_id=command_row.invocation_id;
    end if;

    event_id_value := public.cex_deterministic_uuid_v1(
        'provider-reconciliation-recorded:' || evidence_id_value::text
    );
    envelope_value := jsonb_build_object(
        'event_id',event_id_value,
        'trace_id',command_row.trace_id,
        'org_id',command_row.org_id,
        'actor_type','provider-reconciliation-operator',
        'actor_id',btrim(p_actor),
        'event_type','execution.provider_dispatch.reconciled',
        'schema_version','cex.audit.event.v2',
        'occurred_at',evidence_row.recorded_at,
        'payload',jsonb_build_object(
            '_cex_audit_source_service','execution-service',
            'command_id',p_command_id,
            'execution_id',command_row.execution_id,
            'attempt_count',command_row.attempt_count,
            'disposition',p_disposition,
            'artifact_uri',p_artifact_uri,
            'artifact_sha256',p_artifact_sha256,
            'result_sha256',command_row.result_sha256
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'execution-service',event_id_value,command_row.trace_id,
        command_row.org_id,envelope_value,10
    );

    return jsonb_build_object('replayed',false,'evidence',to_jsonb(evidence_row),'command',to_jsonb(command_row));
end
$$;

create or replace function public.cex_acknowledge_provider_dispatch_v1(
    p_command_id uuid,
    p_actor text,
    p_reason text
)
returns public.cex_provider_dispatch_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    row_value public.cex_provider_dispatch_commands_v1%rowtype;
    evidence_row public.cex_provider_reconciliation_evidence_v1%rowtype;
begin
    if p_actor is null or length(btrim(p_actor)) not between 1 and 256
       or p_reason is null or length(btrim(p_reason)) not between 1 and 1000 then
        raise exception 'provider acknowledgement actor/reason is invalid';
    end if;
    select * into row_value
      from public.cex_provider_dispatch_commands_v1
     where command_id=p_command_id
     for update;
    if not found then
        raise exception using errcode='P0002', message='provider command not found';
    end if;
    if row_value.status not in ('reconcile_required','dead_letter') then
        raise exception 'provider command is not awaiting operator acknowledgement';
    end if;
    select * into evidence_row
      from public.cex_provider_reconciliation_evidence_v1
     where command_id=p_command_id
       and incident_attempt_count=row_value.attempt_count;
    if not found then
        raise exception 'provider acknowledgement requires fresh reconciliation evidence for this attempt';
    end if;
    if row_value.acknowledged_at is not null then
        if row_value.acknowledged_at < evidence_row.recorded_at
           or row_value.acknowledged_by is distinct from btrim(p_actor)
           or row_value.acknowledged_reason is distinct from btrim(p_reason) then
            raise exception using errcode='23505', message='provider acknowledgement collision';
        end if;
        return row_value;
    end if;
    update public.cex_provider_dispatch_commands_v1
       set acknowledged_by=btrim(p_actor),
           acknowledged_reason=btrim(p_reason),
           acknowledged_at=now(),
           updated_at=now()
     where command_id=p_command_id
    returning * into row_value;
    return row_value;
end
$$;

create or replace function public.cex_requeue_provider_dispatch_v1(
    p_command_id uuid,
    p_actor text,
    p_reason text,
    p_additional_attempts integer default 1
)
returns public.cex_provider_dispatch_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    row_value public.cex_provider_dispatch_commands_v1%rowtype;
    evidence_row public.cex_provider_reconciliation_evidence_v1%rowtype;
begin
    if p_actor is null or length(btrim(p_actor)) not between 1 and 256
       or p_reason is null or length(btrim(p_reason)) not between 1 and 1000 then
        raise exception 'provider requeue actor/reason is invalid';
    end if;
    if p_additional_attempts not between 1 and 20 then
        raise exception 'provider additional attempts invalid';
    end if;
    select * into row_value
      from public.cex_provider_dispatch_commands_v1
     where command_id=p_command_id
     for update;
    if not found then
        raise exception using errcode='P0002', message='provider command not found';
    end if;
    if row_value.status not in ('reconcile_required','dead_letter')
       or row_value.acknowledged_at is null then
        raise exception 'provider requeue requires a fresh acknowledgement';
    end if;
    select * into evidence_row
      from public.cex_provider_reconciliation_evidence_v1
     where command_id=p_command_id
       and incident_attempt_count=row_value.attempt_count;
    if not found
       or evidence_row.disposition <> 'confirmed_not_executed'
       or row_value.acknowledged_at < evidence_row.recorded_at then
        raise exception 'provider requeue requires confirmed-not-executed evidence for this attempt';
    end if;
    if row_value.max_attempts+p_additional_attempts > 100 then
        raise exception 'provider attempt budget overflow';
    end if;

    update public.cex_provider_dispatch_commands_v1
       set status='pending',
           max_attempts=max_attempts+p_additional_attempts,
           available_at=now(),
           acknowledged_by=null,
           acknowledged_reason=null,
           acknowledged_at=null,
           last_requeued_by=btrim(p_actor),
           last_requeue_reason=btrim(p_reason),
           last_requeue_additional_attempts=p_additional_attempts,
           last_requeued_at=now(),
           requeue_count=requeue_count+1,
           completed_at=null,
           updated_at=now()
     where command_id=p_command_id
    returning * into row_value;

    update public.executions
       set status='Queued',worker_id=null,lease_expires_at=null,updated_at=now()
     where execution_id=row_value.execution_id;
    update public.invocations
       set status='Queued',failure_reason=null,updated_at=now()
     where invocation_id=row_value.invocation_id;
    return row_value;
end
$$;

do $$
begin
    if not exists (select 1 from pg_roles where rolname='cex_provider_reconciliation_operator') then
        create role cex_provider_reconciliation_operator nologin;
    end if;
end
$$;

revoke execute on function public.cex_acknowledge_provider_dispatch_v1(uuid,text,text)
    from public,cex_projection_operator;
revoke execute on function public.cex_requeue_provider_dispatch_v1(uuid,text,text,integer)
    from public,cex_projection_operator;
revoke execute on function public.cex_record_provider_reconciliation_v1(
    uuid,text,text,text,text,jsonb,jsonb
) from public;

grant execute on function public.cex_reconcile_expired_provider_claims_v1(),
    public.cex_claim_provider_dispatch_v1(text,integer,integer),
    public.cex_finish_provider_dispatch_v1(uuid,text,text,jsonb,integer,text,text,integer)
    to cex_provider_dispatch_worker;
grant execute on function public.cex_record_provider_reconciliation_v1(
    uuid,text,text,text,text,jsonb,jsonb
),
    public.cex_acknowledge_provider_dispatch_v1(uuid,text,text),
    public.cex_requeue_provider_dispatch_v1(uuid,text,text,integer)
    to cex_provider_reconciliation_operator;

create or replace view public.cex_provider_dispatch_status_v1 as
select
    command.status,
    count(*)::bigint as command_count,
    count(*) filter (where command.attempt_count>=command.max_attempts)::bigint as exhausted_count,
    count(*) filter (
        where command.status in ('reconcile_required','dead_letter')
          and evidence.evidence_id is null
    )::bigint as missing_reconciliation_evidence_count,
    count(*) filter (
        where command.status in ('reconcile_required','dead_letter')
          and command.acknowledged_at is null
    )::bigint as unacknowledged_count,
    min(command.available_at) filter (where command.status in ('pending','retry_wait')) as oldest_available_at,
    min(command.lease_expires_at) filter (where command.status='claimed') as oldest_lease_expiry
from public.cex_provider_dispatch_commands_v1 command
left join public.cex_provider_reconciliation_evidence_v1 evidence
  on evidence.command_id=command.command_id
 and evidence.incident_attempt_count=command.attempt_count
group by command.status;

create or replace view public.cex_provider_reconciliation_backlog_v1 as
select
    command.command_id,
    command.execution_id,
    command.invocation_id,
    command.org_id,
    command.status,
    command.attempt_count,
    command.max_attempts,
    command.last_error_code,
    command.updated_at as incident_at,
    evidence.disposition,
    evidence.artifact_uri,
    evidence.artifact_sha256,
    evidence.recorded_at,
    command.acknowledged_by,
    command.acknowledged_at
from public.cex_provider_dispatch_commands_v1 command
left join public.cex_provider_reconciliation_evidence_v1 evidence
  on evidence.command_id=command.command_id
 and evidence.incident_attempt_count=command.attempt_count
where command.status in ('reconcile_required','dead_letter');

commit;
