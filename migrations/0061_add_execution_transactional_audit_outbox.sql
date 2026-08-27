begin;

alter table public.executions
    add column if not exists audit_revision bigint not null default 0;

alter table public.executions
    drop constraint if exists executions_audit_revision_v1,
    add constraint executions_audit_revision_v1 check (audit_revision >= 0);

create or replace function public.cex_execution_prepare_audit_revision_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if tg_op = 'INSERT' then
        new.audit_revision := 1;
        return new;
    end if;

    if new.invocation_id is distinct from old.invocation_id
       or new.status is distinct from old.status
       or new.provider_target is distinct from old.provider_target
       or new.started_at is distinct from old.started_at
       or new.ended_at is distinct from old.ended_at
       or new.result_payload is distinct from old.result_payload
       or new.trace_id is distinct from old.trace_id
       or new.org_id is distinct from old.org_id
       or new.approval_required is distinct from old.approval_required
       or new.policy_reason is distinct from old.policy_reason
       or new.approved_by is distinct from old.approved_by
       or new.worker_id is distinct from old.worker_id
       or new.lease_expires_at is distinct from old.lease_expires_at
       or new.attempt_count is distinct from old.attempt_count
       or new.max_attempts is distinct from old.max_attempts then
        new.audit_revision := old.audit_revision + 1;
    else
        new.audit_revision := old.audit_revision;
    end if;

    return new;
end
$$;

create or replace function public.cex_enqueue_execution_audit_v1(
    old_record public.executions,
    new_record public.executions
)
returns public.executions
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    invocation_org_id uuid;
    org_id_value uuid;
    event_type_value text;
    event_id_value uuid;
    actor_id_value text;
    payload_value jsonb;
    envelope_value jsonb;
begin
    if old_record is not null
       and new_record.audit_revision = old_record.audit_revision then
        return new_record;
    end if;

    select org_id
      into invocation_org_id
      from public.invocations
     where invocation_id = new_record.invocation_id;

    if invocation_org_id is null then
        raise exception 'execution audit source invocation is missing';
    end if;

    org_id_value := coalesce(new_record.org_id, invocation_org_id);
    if org_id_value is distinct from invocation_org_id then
        raise exception 'execution audit org binding differs from invocation authority';
    end if;

    event_type_value := case
        when old_record is null then 'execution.persisted.created'
        when new_record.status is distinct from old_record.status
            then 'execution.persisted.status_changed'
        when new_record.attempt_count is distinct from old_record.attempt_count
             or new_record.max_attempts is distinct from old_record.max_attempts
            then 'execution.persisted.attempt_changed'
        when new_record.worker_id is distinct from old_record.worker_id
             or new_record.lease_expires_at is distinct from old_record.lease_expires_at
            then 'execution.persisted.lease_changed'
        when new_record.result_payload is distinct from old_record.result_payload
            then 'execution.persisted.result_changed'
        when new_record.approval_required is distinct from old_record.approval_required
             or new_record.approved_by is distinct from old_record.approved_by
             or new_record.policy_reason is distinct from old_record.policy_reason
            then 'execution.persisted.approval_changed'
        else 'execution.persisted.changed'
    end;

    actor_id_value := coalesce(
        nullif(current_setting('cex.audit.actor_id', true), ''),
        new_record.worker_id,
        new_record.approved_by
    );

    event_id_value := public.cex_deterministic_uuid_v1(
        'execution-service:' || new_record.execution_id::text || ':' ||
        new_record.audit_revision::text || ':' || event_type_value
    );

    payload_value := jsonb_build_object(
        '_cex_audit_source_service', 'execution-service',
        'execution_id', new_record.execution_id,
        'invocation_id', new_record.invocation_id,
        'audit_revision', new_record.audit_revision,
        'previous_status', case when old_record is null then null else old_record.status end,
        'status', new_record.status,
        'provider_target', new_record.provider_target,
        'attempt_count', new_record.attempt_count,
        'max_attempts', new_record.max_attempts,
        'worker_id', new_record.worker_id,
        'lease_expires_at', new_record.lease_expires_at,
        'started_at', new_record.started_at,
        'ended_at', new_record.ended_at,
        'approval_required', new_record.approval_required,
        'policy_reason', new_record.policy_reason,
        'approved_by', new_record.approved_by,
        'result_payload_sha256', case
            when new_record.result_payload is null then null
            else 'sha256:' || encode(digest(new_record.result_payload::text, 'sha256'), 'hex')
        end
    );

    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', new_record.trace_id,
        'org_id', org_id_value,
        'actor_type', 'execution-service',
        'actor_id', actor_id_value,
        'event_type', event_type_value,
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', clock_timestamp(),
        'payload', payload_value
    );

    perform public.cex_enqueue_audit_outbox_v1(
        'execution-service',
        event_id_value,
        new_record.trace_id,
        org_id_value,
        envelope_value,
        greatest(3, new_record.max_attempts)
    );

    return new_record;
end
$$;

drop trigger if exists trg_cex_execution_prepare_audit_revision_v1
    on public.executions;
create trigger trg_cex_execution_prepare_audit_revision_v1
before insert or update on public.executions
for each row execute function public.cex_execution_prepare_audit_revision_v1();

drop trigger if exists trg_cex_execution_enqueue_audit_v1
    on public.executions;
create trigger trg_cex_execution_enqueue_audit_v1
after insert or update on public.executions
for each row execute function public.cex_enqueue_execution_audit_v1(
    case when tg_op = 'INSERT' then null else old end,
    new
);

create index if not exists idx_executions_audit_revision_v1
    on public.executions (execution_id, audit_revision);

commit;
