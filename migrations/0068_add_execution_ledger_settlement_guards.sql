begin;

-- P0-N5 settlement command guards, append-only evidence and triggers.

create or replace function public.cex_validate_execution_ledger_settlement_insert_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    execution_invocation_id uuid;
    execution_org_id uuid;
    execution_trace_id uuid;
    contract_row public.cex_invocation_ledger_contracts_v1%rowtype;
    expected_request jsonb;
    expected_operation_id uuid;
    expected_command_id uuid;
    expected_fingerprint text;
begin
    select invocation_id, org_id, trace_id
      into execution_invocation_id, execution_org_id, execution_trace_id
      from public.executions
     where execution_id = new.execution_id;
    if not found then
        raise exception 'settlement command execution binding does not exist';
    end if;
    if execution_invocation_id is distinct from new.invocation_id
       or execution_org_id is distinct from new.org_id then
        raise exception 'settlement command execution/Invocation/tenant binding mismatch';
    end if;

    select * into contract_row
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = new.invocation_id;
    if not found then
        raise exception 'settlement command durable Invocation contract does not exist';
    end if;
    if contract_row.org_id is distinct from new.org_id
       or contract_row.trace_id is distinct from execution_trace_id
       or contract_row.contract_hash is distinct from new.contract_hash then
        raise exception 'settlement command differs from durable Invocation contract binding';
    end if;

    expected_request := public.cex_invocation_ledger_effect_request_v1(new.invocation_id, new.action);
    expected_operation_id := (expected_request ->> 'operation_id')::uuid;
    expected_command_id := public.cex_deterministic_uuid_v1(
        'cex:execution-ledger-settlement:' || new.invocation_id::text
    );
    expected_fingerprint :=
        public.cex_execution_ledger_settlement_request_fingerprint_v1(expected_request);

    if new.command_id is distinct from expected_command_id
       or new.operation_id is distinct from expected_operation_id
       or new.request_payload is distinct from expected_request
       or new.request_fingerprint is distinct from expected_fingerprint then
        raise exception 'settlement command immutable request projection is invalid';
    end if;
    return new;
end
$$;

create or replace function public.cex_guard_execution_ledger_settlement_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if new.command_id is distinct from old.command_id
       or new.invocation_id is distinct from old.invocation_id
       or new.execution_id is distinct from old.execution_id
       or new.org_id is distinct from old.org_id
       or new.action is distinct from old.action
       or new.operation_id is distinct from old.operation_id
       or new.contract_hash is distinct from old.contract_hash
       or new.request_payload is distinct from old.request_payload
       or new.request_fingerprint is distinct from old.request_fingerprint
       or new.source_service is distinct from old.source_service
       or new.source_principal is distinct from old.source_principal
       or new.schema_version is distinct from old.schema_version
       or new.created_at is distinct from old.created_at then
        raise exception 'Execution Ledger settlement immutable fields cannot be changed';
    end if;
    if new.execution_mode is distinct from old.execution_mode
       and not (
           old.execution_mode = 'shadow'
           and new.execution_mode = 'active'
           and old.status = 'pending'
           and old.attempt_count = 0
       ) then
        raise exception 'settlement mode can only promote untouched shadow commands';
    end if;
    return new;
end
$$;

create or replace function public.cex_validate_execution_ledger_settlement_transition_v1()
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
    elsif old.status = 'retry_wait' and new.status in ('claimed', 'dead_letter', 'cancelled') then
        return new;
    elsif old.status = 'claimed' and new.status in (
        'succeeded', 'retry_wait', 'reconcile_required', 'dead_letter', 'cancelled'
    ) then
        return new;
    elsif old.status in ('reconcile_required', 'dead_letter')
       and new.status in ('pending', 'cancelled') then
        return new;
    end if;
    raise exception 'invalid settlement transition % -> % for command %',
        old.status, new.status, old.command_id;
end
$$;

create or replace function public.cex_record_execution_ledger_settlement_transition_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if tg_op = 'INSERT' then
        insert into public.cex_execution_ledger_settlement_transitions_v1 (
            command_id, from_status, to_status, attempt_count, worker_id, error_code, receipt_hash
        ) values (
            new.command_id, null, new.status, new.attempt_count,
            new.claimed_by, new.last_error_code, new.ledger_receipt_hash
        );
    elsif new.status is distinct from old.status then
        insert into public.cex_execution_ledger_settlement_transitions_v1 (
            command_id, from_status, to_status, attempt_count, worker_id, error_code, receipt_hash
        ) values (
            new.command_id, old.status, new.status, new.attempt_count,
            coalesce(old.claimed_by, new.claimed_by), new.last_error_code, new.ledger_receipt_hash
        );
    end if;
    return new;
end
$$;

create or replace function public.cex_reject_execution_ledger_settlement_delete_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'Execution Ledger settlement commands cannot be deleted';
end
$$;

create or replace function public.cex_reject_execution_ledger_settlement_transition_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'Execution Ledger settlement transition evidence is append-only';
end
$$;

drop trigger if exists trg_cex_validate_execution_ledger_settlement_insert_v1
    on public.cex_execution_ledger_settlement_commands_v1;

create trigger trg_cex_validate_execution_ledger_settlement_insert_v1
before insert on public.cex_execution_ledger_settlement_commands_v1
for each row execute function public.cex_validate_execution_ledger_settlement_insert_v1();

drop trigger if exists trg_cex_guard_execution_ledger_settlement_mutation_v1
    on public.cex_execution_ledger_settlement_commands_v1;

create trigger trg_cex_guard_execution_ledger_settlement_mutation_v1
before update on public.cex_execution_ledger_settlement_commands_v1
for each row execute function public.cex_guard_execution_ledger_settlement_mutation_v1();

drop trigger if exists trg_cex_validate_execution_ledger_settlement_transition_v1
    on public.cex_execution_ledger_settlement_commands_v1;

create trigger trg_cex_validate_execution_ledger_settlement_transition_v1
before update of status on public.cex_execution_ledger_settlement_commands_v1
for each row execute function public.cex_validate_execution_ledger_settlement_transition_v1();

drop trigger if exists trg_cex_record_execution_ledger_settlement_transition_v1
    on public.cex_execution_ledger_settlement_commands_v1;

create trigger trg_cex_record_execution_ledger_settlement_transition_v1
after insert or update of status on public.cex_execution_ledger_settlement_commands_v1
for each row execute function public.cex_record_execution_ledger_settlement_transition_v1();

drop trigger if exists trg_cex_reject_execution_ledger_settlement_delete_v1
    on public.cex_execution_ledger_settlement_commands_v1;

create trigger trg_cex_reject_execution_ledger_settlement_delete_v1
before delete on public.cex_execution_ledger_settlement_commands_v1
for each row execute function public.cex_reject_execution_ledger_settlement_delete_v1();

drop trigger if exists trg_cex_reject_execution_ledger_settlement_transition_mutation_v1
    on public.cex_execution_ledger_settlement_transitions_v1;

create trigger trg_cex_reject_execution_ledger_settlement_transition_mutation_v1
before update or delete on public.cex_execution_ledger_settlement_transitions_v1
for each row execute function public.cex_reject_execution_ledger_settlement_transition_mutation_v1();

commit;
