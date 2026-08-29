begin;

-- P0-N5 worker outcome persistence and unknown-result handling.

create or replace function public.cex_finish_execution_ledger_settlement_v1(
    p_command_id uuid,
    p_worker_id text,
    p_outcome text,
    p_error_code text default null,
    p_error_message text default null,
    p_http_status integer default null,
    p_receipt jsonb default null,
    p_replayed boolean default null,
    p_retry_after_seconds integer default null
)
returns public.cex_execution_ledger_settlement_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    current_command public.cex_execution_ledger_settlement_commands_v1%rowtype;
    receipt_hash_value text;
    retry_delay_seconds integer;
    next_status text;
begin
    if p_worker_id is null or length(btrim(p_worker_id)) not between 1 and 128 then
        raise exception 'settlement worker_id must contain 1..128 characters';
    end if;
    if p_outcome not in ('succeeded', 'retry_wait', 'reconcile_required', 'dead_letter') then
        raise exception 'unsupported settlement outcome';
    end if;
    if p_http_status is not null and p_http_status not between 100 and 599 then
        raise exception 'settlement HTTP status is invalid';
    end if;
    if p_retry_after_seconds is not null
       and p_retry_after_seconds not between 1 and 3600 then
        raise exception 'settlement retry delay must be between 1 and 3600 seconds';
    end if;

    select * into current_command
      from public.cex_execution_ledger_settlement_commands_v1
     where command_id = p_command_id
     for update;
    if not found then
        raise exception using errcode = 'P0002', message = 'settlement command not found';
    end if;

    if current_command.status = 'succeeded' and p_outcome = 'succeeded' then
        receipt_hash_value := public.cex_execution_ledger_settlement_receipt_hash_v1(p_receipt);
        if current_command.ledger_receipt_hash is distinct from receipt_hash_value
           or current_command.ledger_receipt is distinct from p_receipt
           or current_command.ledger_replayed is distinct from p_replayed then
            raise exception using
                errcode = '23505',
                message = 'settlement completion collision with different receipt';
        end if;
        return current_command;
    end if;

    if current_command.status <> 'claimed'
       or current_command.claimed_by is distinct from btrim(p_worker_id) then
        raise exception 'settlement completion requires ownership of an active claim';
    end if;
    if current_command.lease_expires_at is null
       or current_command.lease_expires_at <= now() then
        raise exception 'settlement completion claim lease has expired';
    end if;

    if p_outcome = 'succeeded' then
        if p_replayed is null then
            raise exception 'verified settlement success must include replayed flag';
        end if;
        perform public.cex_validate_execution_ledger_settlement_receipt_v1(
            p_command_id, p_receipt
        );
        receipt_hash_value := public.cex_execution_ledger_settlement_receipt_hash_v1(p_receipt);
        next_status := 'succeeded';
        update public.cex_execution_ledger_settlement_commands_v1
           set status = next_status,
               claimed_by = null,
               lease_expires_at = null,
               last_http_status = p_http_status,
               last_error_code = null,
               last_error_message = null,
               ledger_receipt = p_receipt,
               ledger_receipt_hash = receipt_hash_value,
               ledger_replayed = p_replayed,
               dead_lettered_at = null,
               completed_at = now(),
               updated_at = now()
         where command_id = p_command_id
        returning * into current_command;
    elsif p_outcome = 'retry_wait' and current_command.attempt_count < current_command.max_attempts then
        retry_delay_seconds := greatest(
            coalesce(p_retry_after_seconds, 0),
            least(3600, power(2, least(greatest(current_command.attempt_count - 1, 0), 10))::integer)
        );
        next_status := 'retry_wait';
        update public.cex_execution_ledger_settlement_commands_v1
           set status = next_status,
               claimed_by = null,
               lease_expires_at = null,
               available_at = now() + make_interval(secs => retry_delay_seconds),
               last_http_status = p_http_status,
               last_error_code = left(coalesce(nullif(btrim(p_error_code), ''), 'retryable_failure'), 128),
               last_error_message = left(coalesce(p_error_message, ''), 2000),
               dead_lettered_at = null,
               updated_at = now()
         where command_id = p_command_id
        returning * into current_command;
    elsif p_outcome = 'reconcile_required' then
        next_status := 'reconcile_required';
        update public.cex_execution_ledger_settlement_commands_v1
           set status = next_status,
               claimed_by = null,
               lease_expires_at = null,
               last_http_status = p_http_status,
               last_error_code = left(coalesce(nullif(btrim(p_error_code), ''), 'unknown_remote_outcome'), 128),
               last_error_message = left(coalesce(p_error_message, ''), 2000),
               updated_at = now()
         where command_id = p_command_id
        returning * into current_command;
    elsif p_outcome = 'retry_wait' then
        next_status := 'reconcile_required';
        update public.cex_execution_ledger_settlement_commands_v1
           set status = next_status,
               claimed_by = null,
               lease_expires_at = null,
               last_http_status = p_http_status,
               last_error_code = 'retry_budget_exhausted_unknown_outcome',
               last_error_message = left(coalesce(
                   p_error_message,
                   'automatic attempts exhausted; inspect durable contract before replay'
               ), 2000),
               updated_at = now()
         where command_id = p_command_id
        returning * into current_command;
    else
        next_status := 'dead_letter';
        update public.cex_execution_ledger_settlement_commands_v1
           set status = next_status,
               claimed_by = null,
               lease_expires_at = null,
               last_http_status = p_http_status,
               last_error_code = left(coalesce(
                   nullif(btrim(p_error_code), ''),
                   'permanent_rejection'
               ), 128),
               last_error_message = left(coalesce(p_error_message, ''), 2000),
               dead_lettered_at = now(),
               updated_at = now()
         where command_id = p_command_id
        returning * into current_command;
    end if;

    perform public.cex_enqueue_execution_ledger_settlement_status_audit_v1(
        current_command.command_id, btrim(p_worker_id), next_status
    );
    return current_command;
end
$$;

commit;
