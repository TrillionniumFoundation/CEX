begin;

-- P0-N5 exact enqueue, shadow promotion and status audit.

create or replace function public.cex_enqueue_execution_ledger_settlement_v1(
    p_execution_id uuid,
    p_action text,
    p_execution_mode text default 'shadow',
    p_max_attempts integer default 5,
    p_source_principal text default 'execution-settlement-coordinator'
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    execution_invocation_id uuid;
    execution_org_id uuid;
    execution_trace_id uuid;
    contract_row public.cex_invocation_ledger_contracts_v1%rowtype;
    request_value jsonb;
    operation_id_value uuid;
    request_fingerprint_value text;
    command_id_value uuid;
    existing_command public.cex_execution_ledger_settlement_commands_v1%rowtype;
    inserted_command public.cex_execution_ledger_settlement_commands_v1%rowtype;
    audit_event_id uuid;
    audit_envelope jsonb;
begin
    if p_execution_id is null
       or p_execution_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'settlement execution_id must be non-nil';
    end if;
    if p_action not in ('consume', 'refund') then
        raise exception 'settlement action must be consume or refund';
    end if;
    if p_execution_mode not in ('shadow', 'active') then
        raise exception 'settlement mode must be shadow or active';
    end if;
    if p_max_attempts not between 1 and 100 then
        raise exception 'settlement max_attempts must be between 1 and 100';
    end if;
    if p_source_principal is null
       or length(btrim(p_source_principal)) not between 1 and 256 then
        raise exception 'settlement source principal must contain 1..256 characters';
    end if;

    select invocation_id, org_id, trace_id
      into execution_invocation_id, execution_org_id, execution_trace_id
      from public.executions
     where execution_id = p_execution_id
     for share;
    if not found then
        raise exception using errcode = 'P0002', message = 'execution not found';
    end if;
    if execution_org_id is null or execution_trace_id is null then
        raise exception 'settlement requires durable org and trace binding';
    end if;

    perform pg_advisory_xact_lock(
        hashtextextended('cex:execution-ledger-settlement:' || execution_invocation_id::text, 0)
    );

    select * into contract_row
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = execution_invocation_id
     for share;
    if not found then
        raise exception using errcode = 'P0002', message = 'Invocation Ledger contract not found';
    end if;
    if contract_row.org_id is distinct from execution_org_id
       or contract_row.trace_id is distinct from execution_trace_id then
        raise exception 'Execution and Invocation Ledger contract binding mismatch';
    end if;

    request_value := public.cex_invocation_ledger_effect_request_v1(
        execution_invocation_id, p_action
    );
    operation_id_value := (request_value ->> 'operation_id')::uuid;
    request_fingerprint_value :=
        public.cex_execution_ledger_settlement_request_fingerprint_v1(request_value);
    command_id_value := public.cex_deterministic_uuid_v1(
        'cex:execution-ledger-settlement:' || execution_invocation_id::text
    );

    select * into existing_command
      from public.cex_execution_ledger_settlement_commands_v1
     where invocation_id = execution_invocation_id
     for update;
    if found then
        if existing_command.command_id is distinct from command_id_value
           or existing_command.execution_id is distinct from p_execution_id
           or existing_command.org_id is distinct from execution_org_id
           or existing_command.action is distinct from p_action
           or existing_command.operation_id is distinct from operation_id_value
           or existing_command.contract_hash is distinct from contract_row.contract_hash
           or existing_command.request_payload is distinct from request_value
           or existing_command.request_fingerprint is distinct from request_fingerprint_value
           or existing_command.source_principal is distinct from btrim(p_source_principal) then
            raise exception using
                errcode = '23505',
                message = 'settlement command collision with different immutable content';
        end if;
        return jsonb_build_object('replayed', true, 'command', to_jsonb(existing_command));
    end if;

    insert into public.cex_execution_ledger_settlement_commands_v1 (
        command_id, invocation_id, execution_id, org_id, action, operation_id,
        contract_hash, request_payload, request_fingerprint, execution_mode,
        max_attempts, source_principal
    ) values (
        command_id_value, execution_invocation_id, p_execution_id, execution_org_id,
        p_action, operation_id_value, contract_row.contract_hash, request_value,
        request_fingerprint_value, p_execution_mode, p_max_attempts,
        btrim(p_source_principal)
    ) returning * into inserted_command;

    audit_event_id := public.cex_deterministic_uuid_v1(
        'execution-ledger-settlement-commanded:' || command_id_value::text
    );
    audit_envelope := jsonb_build_object(
        'event_id', audit_event_id,
        'trace_id', execution_trace_id,
        'org_id', execution_org_id,
        'actor_type', 'execution-service',
        'actor_id', btrim(p_source_principal),
        'event_type', 'execution.ledger_settlement.commanded',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', inserted_command.created_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'execution-service',
            'command_id', command_id_value,
            'invocation_id', execution_invocation_id,
            'execution_id', p_execution_id,
            'operation_id', operation_id_value,
            'action', p_action,
            'execution_mode', p_execution_mode,
            'request_fingerprint', request_fingerprint_value,
            'contract_hash', contract_row.contract_hash
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'execution-service', audit_event_id, execution_trace_id,
        execution_org_id, audit_envelope, 10
    );

    return jsonb_build_object('replayed', false, 'command', to_jsonb(inserted_command));
end
$$;

create or replace function public.cex_promote_execution_ledger_settlement_v1(
    p_command_id uuid,
    p_actor text
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
        raise exception 'settlement promotion actor must contain 1..256 characters';
    end if;
    select * into current_command
      from public.cex_execution_ledger_settlement_commands_v1
     where command_id = p_command_id
     for update;
    if not found then
        raise exception using errcode = 'P0002', message = 'settlement command not found';
    end if;
    if current_command.execution_mode = 'active' then
        return current_command;
    end if;
    if current_command.status <> 'pending' or current_command.attempt_count <> 0 then
        raise exception 'only untouched pending shadow commands can be promoted';
    end if;

    update public.cex_execution_ledger_settlement_commands_v1
       set execution_mode = 'active', updated_at = now()
     where command_id = p_command_id
    returning * into current_command;

    audit_event_id := public.cex_deterministic_uuid_v1(
        'execution-ledger-settlement-promoted:' || current_command.command_id::text
    );
    audit_envelope := jsonb_build_object(
        'event_id', audit_event_id,
        'trace_id', current_command.request_payload ->> 'trace_id',
        'org_id', current_command.org_id,
        'actor_type', 'operator',
        'actor_id', btrim(p_actor),
        'event_type', 'execution.ledger_settlement.promoted',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', current_command.updated_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'execution-service',
            'command_id', current_command.command_id,
            'execution_id', current_command.execution_id,
            'operation_id', current_command.operation_id,
            'execution_mode', current_command.execution_mode
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

create or replace function public.cex_enqueue_execution_ledger_settlement_status_audit_v1(
    p_command_id uuid,
    p_actor_id text,
    p_event_suffix text
)
returns void
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    command_row public.cex_execution_ledger_settlement_commands_v1%rowtype;
    audit_event_id uuid;
    audit_envelope jsonb;
begin
    if p_actor_id is null or length(btrim(p_actor_id)) not between 1 and 256 then
        raise exception 'settlement status Audit actor must contain 1..256 characters';
    end if;
    if p_event_suffix is null
       or p_event_suffix !~ '^[a-z][a-z0-9_]{0,63}$' then
        raise exception 'settlement status Audit suffix is invalid';
    end if;
    select * into command_row
      from public.cex_execution_ledger_settlement_commands_v1
     where command_id = p_command_id;
    if not found then
        raise exception using errcode = 'P0002', message = 'settlement command not found';
    end if;

    audit_event_id := public.cex_deterministic_uuid_v1(
        'execution-ledger-settlement-status:'
        || command_row.command_id::text
        || ':' || command_row.attempt_count::text
        || ':' || p_event_suffix
    );
    audit_envelope := jsonb_build_object(
        'event_id', audit_event_id,
        'trace_id', command_row.request_payload ->> 'trace_id',
        'org_id', command_row.org_id,
        'actor_type', 'execution-settlement-worker',
        'actor_id', btrim(p_actor_id),
        'event_type', 'execution.ledger_settlement.' || p_event_suffix,
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', command_row.updated_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'execution-service',
            'command_id', command_row.command_id,
            'invocation_id', command_row.invocation_id,
            'execution_id', command_row.execution_id,
            'operation_id', command_row.operation_id,
            'action', command_row.action,
            'attempt_count', command_row.attempt_count,
            'status', command_row.status,
            'http_status', command_row.last_http_status,
            'error_code', command_row.last_error_code,
            'receipt_hash', command_row.ledger_receipt_hash,
            'ledger_replayed', command_row.ledger_replayed
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'execution-service', audit_event_id,
        (command_row.request_payload ->> 'trace_id')::uuid,
        command_row.org_id, audit_envelope, 10
    );
end
$$;

commit;
