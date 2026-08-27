begin;

alter table public.executions
    add column if not exists audit_revision bigint not null default 0;

do $migration$
begin
    if not exists (
        select 1
          from pg_constraint
         where conname = 'executions_audit_revision_nonnegative_v1'
           and conrelid = 'public.executions'::regclass
    ) then
        alter table public.executions
            add constraint executions_audit_revision_nonnegative_v1
            check (audit_revision >= 0) not valid;
    end if;
end
$migration$;

create or replace function public.cex_prepare_execution_audit_revision_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if tg_op = 'INSERT' then
        new.audit_revision := 1;
        return new;
    end if;

    if old.invocation_id is distinct from new.invocation_id
       or old.status is distinct from new.status
       or old.provider_target is distinct from new.provider_target
       or old.attempt_count is distinct from new.attempt_count
       or old.max_attempts is distinct from new.max_attempts
       or old.worker_id is distinct from new.worker_id
       or old.lease_expires_at is distinct from new.lease_expires_at
       or old.started_at is distinct from new.started_at
       or old.ended_at is distinct from new.ended_at
       or old.result_payload is distinct from new.result_payload
       or old.trace_id is distinct from new.trace_id
       or old.org_id is distinct from new.org_id
       or old.approval_required is distinct from new.approval_required
       or old.policy_reason is distinct from new.policy_reason
       or old.approved_by is distinct from new.approved_by then
        new.audit_revision := old.audit_revision + 1;
    else
        new.audit_revision := old.audit_revision;
    end if;

    return new;
end
$$;

create or replace function public.cex_enqueue_execution_audit_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    invocation_org_id uuid;
    event_org_id uuid;
    event_type_value text;
    event_id_value uuid;
    actor_id_value text;
    result_payload_hash text;
    envelope_value jsonb;
begin
    if tg_op = 'UPDATE' and new.audit_revision = old.audit_revision then
        return new;
    end if;

    select org_id
      into invocation_org_id
      from public.invocations
     where invocation_id = new.invocation_id;

    if not found then
        raise exception 'execution audit enqueue cannot resolve invocation tenancy';
    end if;

    if new.org_id is not null and new.org_id is distinct from invocation_org_id then
        raise exception 'execution org_id differs from authoritative invocation org_id';
    end if;
    event_org_id := coalesce(new.org_id, invocation_org_id);

    event_type_value := case
        when tg_op = 'INSERT' then 'execution.persisted.created'
        when old.status is distinct from new.status then 'execution.persisted.status_changed'
        when old.attempt_count is distinct from new.attempt_count
          or old.max_attempts is distinct from new.max_attempts
            then 'execution.persisted.attempt_changed'
        when old.worker_id is distinct from new.worker_id
          or old.lease_expires_at is distinct from new.lease_expires_at
            then 'execution.persisted.lease_changed'
        when old.result_payload is distinct from new.result_payload
          or old.ended_at is distinct from new.ended_at
            then 'execution.persisted.result_changed'
        when old.approval_required is distinct from new.approval_required
          or old.policy_reason is distinct from new.policy_reason
          or old.approved_by is distinct from new.approved_by
            then 'execution.persisted.approval_changed'
        else 'execution.persisted.changed'
    end;

    event_id_value := public.cex_deterministic_uuid_v1(
        'execution-service:executions:' ||
        new.execution_id::text || ':' ||
        new.audit_revision::text || ':' ||
        event_type_value
    );

    actor_id_value := coalesce(
        nullif(btrim(new.worker_id), ''),
        nullif(btrim(new.approved_by), '')
    );

    result_payload_hash := case
        when new.result_payload is null then null
        else 'sha256:' || encode(digest(new.result_payload::text, 'sha256'), 'hex')
    end;

    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', new.trace_id,
        'org_id', event_org_id,
        'actor_type', 'execution-service',
        'actor_id', actor_id_value,
        'event_type', event_type_value,
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', coalesce(new.updated_at, new.created_at, clock_timestamp()),
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'execution-service',
            'execution_id', new.execution_id,
            'invocation_id', new.invocation_id,
            'audit_revision', new.audit_revision,
            'previous_status', case when tg_op = 'INSERT' then null else old.status end,
            'status', new.status,
            'provider_target', new.provider_target,
            'attempt_count', new.attempt_count,
            'max_attempts', new.max_attempts,
            'worker_id', new.worker_id,
            'lease_expires_at', new.lease_expires_at,
            'started_at', new.started_at,
            'ended_at', new.ended_at,
            'result_payload_sha256', result_payload_hash,
            'approval_required', new.approval_required,
            'policy_reason', new.policy_reason,
            'approved_by', new.approved_by
        )
    );

    perform public.cex_enqueue_audit_outbox_v1(
        'execution-service',
        event_id_value,
        new.trace_id,
        event_org_id,
        envelope_value,
        10
    );

    return new;
end
$$;

drop trigger if exists trg_cex_prepare_execution_audit_revision_v1
    on public.executions;
create trigger trg_cex_prepare_execution_audit_revision_v1
before insert or update on public.executions
for each row execute function public.cex_prepare_execution_audit_revision_v1();

drop trigger if exists trg_cex_enqueue_execution_audit_v1
    on public.executions;
create trigger trg_cex_enqueue_execution_audit_v1
after insert or update on public.executions
for each row execute function public.cex_enqueue_execution_audit_v1();

alter table public.executions
    validate constraint executions_audit_revision_nonnegative_v1;

create index if not exists idx_executions_audit_revision_v1
    on public.executions (execution_id, audit_revision);

commit;
