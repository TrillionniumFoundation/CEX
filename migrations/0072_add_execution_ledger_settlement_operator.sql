begin;

-- P0-N5 operator acknowledgement/requeue, indexes and status projection.

create or replace function public.cex_acknowledge_execution_ledger_settlement_v1(
    p_command_id uuid,
    p_actor text,
    p_reason text
)
returns public.cex_execution_ledger_settlement_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    current_command public.cex_execution_ledger_settlement_commands_v1%rowtype;
    audit_event_id uuid;
    audit_envelope jsonb;
begin
    if p_actor is null or length(btrim(p_actor)) not between 1 and 256 then
        raise exception 'settlement acknowledgement actor must contain 1..256 characters';
    end if;
    if p_reason is null or length(btrim(p_reason)) not between 1 and 1000 then
        raise exception 'settlement acknowledgement reason must contain 1..1000 characters';
    end if;
    select * into current_command
      from public.cex_execution_ledger_settlement_commands_v1
     where command_id = p_command_id
     for update;
    if not found then
        raise exception using errcode = 'P0002', message = 'settlement command not found';
    end if;
    if current_command.status not in ('reconcile_required', 'dead_letter') then
        raise exception 'only reconciliation/dead-letter commands can be acknowledged';
    end if;
    if current_command.operator_acknowledged_at is not null then
        if current_command.operator_acknowledged_by is distinct from btrim(p_actor)
           or current_command.operator_acknowledged_reason is distinct from btrim(p_reason) then
            raise exception using
                errcode = '23505',
                message = 'settlement acknowledgement collision';
        end if;
        return current_command;
    end if;

    update public.cex_execution_ledger_settlement_commands_v1
       set operator_acknowledged_by = btrim(p_actor),
           operator_acknowledged_reason = btrim(p_reason),
           operator_acknowledged_at = now(),
           updated_at = now()
     where command_id = p_command_id
    returning * into current_command;

    audit_event_id := public.cex_deterministic_uuid_v1(
        'execution-ledger-settlement-acknowledged:' || current_command.command_id::text
        || ':' || current_command.requeue_count::text
    );
    audit_envelope := jsonb_build_object(
        'event_id', audit_event_id,
        'trace_id', current_command.request_payload ->> 'trace_id',
        'org_id', current_command.org_id,
        'actor_type', 'operator',
        'actor_id', btrim(p_actor),
        'event_type', 'execution.ledger_settlement.acknowledged',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', current_command.operator_acknowledged_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'execution-service',
            'command_id', current_command.command_id,
            'execution_id', current_command.execution_id,
            'operation_id', current_command.operation_id,
            'status', current_command.status,
            'reason', current_command.operator_acknowledged_reason
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'execution-service', audit_event_id,
        (current_command.request_payload ->> 'trace_id')::uuid,
        current_command.org_id, audit_envelope, 10
    );
    return current_command;
end
$$;

create or replace function public.cex_requeue_execution_ledger_settlement_v1(
    p_command_id uuid,
    p_actor text,
    p_reason text,
    p_additional_attempts integer default 1
)
returns public.cex_execution_ledger_settlement_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    current_command public.cex_execution_ledger_settlement_commands_v1%rowtype;
    acknowledged_by_value text;
    acknowledged_reason_value text;
    acknowledged_at_value timestamptz;
    audit_event_id uuid;
    audit_envelope jsonb;
begin
    if p_actor is null or length(btrim(p_actor)) not between 1 and 256 then
        raise exception 'settlement requeue actor must contain 1..256 characters';
    end if;
    if p_reason is null or length(btrim(p_reason)) not between 1 and 1000 then
        raise exception 'settlement requeue reason must contain 1..1000 characters';
    end if;
    if p_additional_attempts not between 1 and 20 then
        raise exception 'settlement additional attempts must be between 1 and 20';
    end if;

    select * into current_command
      from public.cex_execution_ledger_settlement_commands_v1
     where command_id = p_command_id
     for update;
    if not found then
        raise exception using errcode = 'P0002', message = 'settlement command not found';
    end if;
    if current_command.status not in ('reconcile_required', 'dead_letter') then
        if current_command.status = 'pending'
           and current_command.last_requeued_by is not distinct from btrim(p_actor)
           and current_command.last_requeue_reason is not distinct from btrim(p_reason)
           and current_command.last_requeue_additional_attempts
               is not distinct from p_additional_attempts then
            return current_command;
        end if;
        raise exception using
            errcode = '23505',
            message = 'settlement requeue collision or command is not recoverable';
    end if;
    if current_command.operator_acknowledged_at is null then
        raise exception 'operator acknowledgement is required before requeue';
    end if;
    if current_command.max_attempts + p_additional_attempts > 100 then
        raise exception 'requeue would exceed maximum attempt budget';
    end if;

    acknowledged_by_value := current_command.operator_acknowledged_by;
    acknowledged_reason_value := current_command.operator_acknowledged_reason;
    acknowledged_at_value := current_command.operator_acknowledged_at;

    update public.cex_execution_ledger_settlement_commands_v1
       set status = 'pending',
           max_attempts = max_attempts + p_additional_attempts,
           available_at = now(),
           claimed_by = null,
           lease_expires_at = null,
           dead_lettered_at = null,
           requeue_count = requeue_count + 1,
           last_requeued_by = btrim(p_actor),
           last_requeue_reason = btrim(p_reason),
           last_requeue_additional_attempts = p_additional_attempts,
           last_requeued_at = now(),
           operator_acknowledged_by = null,
           operator_acknowledged_reason = null,
           operator_acknowledged_at = null,
           updated_at = now()
     where command_id = p_command_id
    returning * into current_command;

    audit_event_id := public.cex_deterministic_uuid_v1(
        'execution-ledger-settlement-requeued:' || current_command.command_id::text
        || ':' || current_command.requeue_count::text
    );
    audit_envelope := jsonb_build_object(
        'event_id', audit_event_id,
        'trace_id', current_command.request_payload ->> 'trace_id',
        'org_id', current_command.org_id,
        'actor_type', 'operator',
        'actor_id', btrim(p_actor),
        'event_type', 'execution.ledger_settlement.requeued',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', current_command.updated_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'execution-service',
            'command_id', current_command.command_id,
            'execution_id', current_command.execution_id,
            'operation_id', current_command.operation_id,
            'status', current_command.status,
            'requeue_count', current_command.requeue_count,
            'max_attempts', current_command.max_attempts,
            'acknowledged_by', acknowledged_by_value,
            'acknowledged_reason', acknowledged_reason_value,
            'acknowledged_at', acknowledged_at_value,
            'requeued_by', current_command.last_requeued_by,
            'requeue_reason', current_command.last_requeue_reason,
            'additional_attempts', current_command.last_requeue_additional_attempts
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'execution-service', audit_event_id,
        (current_command.request_payload ->> 'trace_id')::uuid,
        current_command.org_id, audit_envelope, 10
    );
    return current_command;
end
$$;

create index if not exists idx_cex_execution_ledger_settlement_claim_v1
    on public.cex_execution_ledger_settlement_commands_v1 (
        execution_mode, status, available_at, created_at, command_id
    ) where status in ('pending', 'claimed', 'retry_wait');

create index if not exists idx_cex_execution_ledger_settlement_execution_v1
    on public.cex_execution_ledger_settlement_commands_v1 (
        execution_id, created_at, command_id
    );

create index if not exists idx_cex_execution_ledger_settlement_org_v1
    on public.cex_execution_ledger_settlement_commands_v1 (
        org_id, status, updated_at, command_id
    );

create index if not exists idx_cex_execution_ledger_settlement_operator_v1
    on public.cex_execution_ledger_settlement_commands_v1 (
        status, operator_acknowledged_at, updated_at, command_id
    ) where status in ('reconcile_required', 'dead_letter');

create index if not exists idx_cex_execution_ledger_settlement_transitions_command_v1
    on public.cex_execution_ledger_settlement_transitions_v1 (
        command_id, transition_id
    );

create or replace view public.cex_execution_ledger_settlement_status_v1 as
select
    execution_mode,
    action,
    status,
    count(*)::bigint as command_count,
    count(*) filter (where attempt_count >= max_attempts)::bigint as retry_budget_exhausted,
    count(*) filter (
        where status in ('reconcile_required', 'dead_letter')
          and operator_acknowledged_at is null
    )::bigint as unacknowledged_operator_items,
    min(available_at) filter (where status in ('pending', 'retry_wait')) as oldest_available_at,
    min(lease_expires_at) filter (where status = 'claimed') as oldest_lease_expiry,
    min(updated_at) filter (
        where status in ('reconcile_required', 'dead_letter')
          and operator_acknowledged_at is null
    ) as oldest_unacknowledged_at
from public.cex_execution_ledger_settlement_commands_v1
group by execution_mode, action, status;

commit;
