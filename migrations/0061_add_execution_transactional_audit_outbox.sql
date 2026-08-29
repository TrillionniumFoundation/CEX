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

create or replace function public.cex_enqueue_execution_audit_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    invocation_org_id uuid;
    org_id_value uuid;
    event_type_value text;
    event_id_value uuid;
    actor_id_value text;
    previous_status_value text;
    payload_value jsonb;
    envelope_value jsonb;
begin
    if tg_op <> 'INSERT' then
        if new.audit_revision = old.audit_revision then
            return new;
        end if;
        previous_status_value := old.status;

        if new.status is distinct from old.status then
            event_type_value := 'execution.persisted.status_changed';
        elsif new.attempt_count is distinct from old.attempt_count
              or new.max_attempts is distinct from old.max_attempts then
            event_type_value := 'execution.persisted.attempt_changed';
        elsif new.worker_id is distinct from old.worker_id
              or new.lease_expires_at is distinct from old.lease_expires_at then
            event_type_value := 'execution.persisted.lease_changed';
        elsif new.result_payload is distinct from old.result_payload then
            event_type_value := 'execution.persisted.result_changed';
        elsif new.approval_required is distinct from old.approval_required
              or new.approved_by is distinct from old.approved_by
              or new.policy_reason is distinct from old.policy_reason then
            event_type_value := 'execution.persisted.approval_changed';
        else
            event_type_value := 'execution.persisted.changed';
        end if;
    else
        previous_status_value := null;
        event_type_value := 'execution.persisted.created';
    end if;

    select org_id
      into invocation_org_id
      from public.invocations
     where invocation_id = new.invocation_id;

    if invocation_org_id is null then
        raise exception 'execution audit source invocation is missing';
    end if;

    org_id_value := coalesce(new.org_id, invocation_org_id);
    if org_id_value is distinct from invocation_org_id then
        raise exception 'execution audit org binding differs from invocation authority';
    end if;

    actor_id_value := coalesce(
        nullif(current_setting('cex.audit.actor_id', true), ''),
        new.worker_id,
        new.approved_by
    );

    event_id_value := public.cex_deterministic_uuid_v1(
        'execution-service:' || new.execution_id::text || ':' ||
        new.audit_revision::text || ':' || event_type_value
    );

    payload_value := jsonb_build_object(
        '_cex_audit_source_service', 'execution-service',
        'execution_id', new.execution_id,
        'invocation_id', new.invocation_id,
        'audit_revision', new.audit_revision,
        'previous_status', previous_status_value,
        'status', new.status,
        'provider_target', new.provider_target,
        'attempt_count', new.attempt_count,
        'max_attempts', new.max_attempts,
        'worker_id', new.worker_id,
        'lease_expires_at', new.lease_expires_at,
        'started_at', new.started_at,
        'ended_at', new.ended_at,
        'approval_required', new.approval_required,
        'policy_reason', new.policy_reason,
        'approved_by', new.approved_by,
        'result_payload_sha256', case
            when new.result_payload is null then null
            else 'sha256:' || encode(digest(new.result_payload::text, 'sha256'), 'hex')
        end
    );

    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', new.trace_id,
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
        new.trace_id,
        org_id_value,
        envelope_value,
        greatest(3, new.max_attempts)
    );

    return new;
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
for each row execute function public.cex_enqueue_execution_audit_v1();

create index if not exists idx_executions_audit_revision_v1
    on public.executions (execution_id, audit_revision);

commit;
