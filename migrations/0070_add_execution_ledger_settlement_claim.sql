begin;

-- P0-N5 claim leases and verified Ledger receipt validation.

create or replace function public.cex_claim_execution_ledger_settlements_v1(
    p_worker_id text,
    p_limit integer default 10,
    p_lease_seconds integer default 60
)
returns setof public.cex_execution_ledger_settlement_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    expired_command public.cex_execution_ledger_settlement_commands_v1%rowtype;
begin
    if p_worker_id is null or length(btrim(p_worker_id)) not between 1 and 128 then
        raise exception 'settlement worker_id must contain 1..128 characters';
    end if;
    if p_limit not between 1 and 100 then
        raise exception 'settlement claim limit must be between 1 and 100';
    end if;
    if p_lease_seconds not between 5 and 3600 then
        raise exception 'settlement lease must be between 5 and 3600 seconds';
    end if;

    for expired_command in
        with expired as (
            select command_id
              from public.cex_execution_ledger_settlement_commands_v1
             where execution_mode = 'active'
               and status = 'claimed'
               and lease_expires_at <= now()
               and attempt_count < max_attempts
             order by lease_expires_at, created_at, command_id
             for update skip locked
             limit p_limit
        )
        update public.cex_execution_ledger_settlement_commands_v1 command
           set status = 'retry_wait',
               claimed_by = null,
               lease_expires_at = null,
               available_at = now(),
               last_error_code = 'claim_lease_expired_exact_replay',
               last_error_message = 'claim lease expired; exact replay remains safe',
               dead_lettered_at = null,
               updated_at = now()
          from expired
         where command.command_id = expired.command_id
        returning command.*
    loop
        perform public.cex_enqueue_execution_ledger_settlement_status_audit_v1(
            expired_command.command_id, 'lease-recovery', 'retry_wait'
        );
    end loop;

    for expired_command in
        with expired as (
            select command_id
              from public.cex_execution_ledger_settlement_commands_v1
             where execution_mode = 'active'
               and status = 'claimed'
               and lease_expires_at <= now()
               and attempt_count >= max_attempts
             order by lease_expires_at, created_at, command_id
             for update skip locked
             limit p_limit
        )
        update public.cex_execution_ledger_settlement_commands_v1 command
           set status = 'reconcile_required',
               claimed_by = null,
               lease_expires_at = null,
               last_error_code = 'claim_lease_expired_after_final_attempt_unknown_outcome',
               last_error_message = 'final lease expired; inspect contract before exact replay',
               dead_lettered_at = null,
               updated_at = now()
          from expired
         where command.command_id = expired.command_id
        returning command.*
    loop
        perform public.cex_enqueue_execution_ledger_settlement_status_audit_v1(
            expired_command.command_id, 'lease-recovery', 'reconcile_required'
        );
    end loop;

    return query
    with candidates as (
        select command_id
          from public.cex_execution_ledger_settlement_commands_v1
         where execution_mode = 'active'
           and status in ('pending', 'retry_wait')
           and attempt_count < max_attempts
           and available_at <= now()
         order by available_at, created_at, command_id
         for update skip locked
         limit p_limit
    )
    update public.cex_execution_ledger_settlement_commands_v1 command
       set status = 'claimed',
           attempt_count = command.attempt_count + 1,
           claimed_by = btrim(p_worker_id),
           lease_expires_at = now() + make_interval(secs => p_lease_seconds),
           updated_at = now()
      from candidates
     where command.command_id = candidates.command_id
    returning command.*;
end
$$;

create or replace function public.cex_validate_execution_ledger_settlement_receipt_v1(
    p_command_id uuid,
    p_receipt jsonb
)
returns void
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    command_row public.cex_execution_ledger_settlement_commands_v1%rowtype;
    contract_row public.cex_invocation_ledger_contracts_v1%rowtype;
    receipt_entry_id uuid;
    expected_contract_status text;
begin
    if p_receipt is null or jsonb_typeof(p_receipt) <> 'object' then
        raise exception 'settlement receipt must be a JSON object';
    end if;
    select * into command_row
      from public.cex_execution_ledger_settlement_commands_v1
     where command_id = p_command_id;
    if not found then
        raise exception using errcode = 'P0002', message = 'settlement command not found';
    end if;
    if public.cex_execution_ledger_settlement_request_fingerprint_v1(
           command_row.request_payload
       ) is distinct from command_row.request_fingerprint then
        raise exception 'settlement request fingerprint mismatch';
    end if;

    if p_receipt #>> '{effect,account_id}'
           is distinct from command_row.request_payload ->> 'account_id'
       or p_receipt #>> '{effect,trace_id}'
           is distinct from command_row.request_payload ->> 'trace_id'
       or p_receipt #>> '{effect,operation_id}' is distinct from command_row.operation_id::text
       or p_receipt #>> '{effect,operation_kind}' is distinct from command_row.action
       or p_receipt #>> '{effect,idempotency_scope}'
           is distinct from command_row.request_payload ->> 'idempotency_scope'
       or p_receipt #>> '{effect,idempotency_key}'
           is distinct from command_row.request_payload ->> 'idempotency_key'
       or p_receipt #>> '{effect,amount_minor}'
           is distinct from command_row.request_payload ->> 'amount_minor'
       or p_receipt #>> '{effect,currency_scale}'
           is distinct from command_row.request_payload ->> 'currency_scale'
       or p_receipt #>> '{account,currency_unit}'
           is distinct from command_row.request_payload ->> 'currency_unit'
       or p_receipt #>> '{account,currency_scale}'
           is distinct from command_row.request_payload ->> 'currency_scale' then
        raise exception 'Ledger receipt differs from immutable settlement request';
    end if;

    begin
        receipt_entry_id := (p_receipt #>> '{effect,entry_id}')::uuid;
    exception when others then
        raise exception 'Ledger receipt entry_id is missing or invalid';
    end;
    if receipt_entry_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'Ledger receipt entry_id must be non-nil';
    end if;

    select * into contract_row
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = command_row.invocation_id
     for share;
    if not found then
        raise exception 'durable Invocation Ledger contract disappeared';
    end if;
    expected_contract_status := case command_row.action
        when 'consume' then 'consumed' else 'refunded' end;
    if contract_row.contract_hash is distinct from command_row.contract_hash
       or contract_row.status is distinct from expected_contract_status
       or contract_row.last_operation_id is distinct from command_row.operation_id
       or contract_row.last_entry_id is distinct from receipt_entry_id then
        raise exception 'Ledger receipt lacks expected durable contract evidence';
    end if;
end
$$;

commit;
