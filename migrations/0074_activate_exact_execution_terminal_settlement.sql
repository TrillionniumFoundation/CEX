begin;

-- P0-N6 caller activation boundary for exact-value Invocations.
-- Once a 0073 reserve command has verified success, an Execution terminal status
-- and the corresponding 0067 consume/refund command commit in the same transaction.

create or replace function public.cex_exact_execution_terminal_action_v1(
    p_status text
)
returns text
language sql
immutable
strict
set search_path = pg_catalog, public
as $$
    select case lower(btrim(p_status))
        when 'succeeded' then 'consume'
        when 'failed' then 'refund'
        when 'cancelled' then 'refund'
        when 'canceled' then 'refund'
        when 'timed_out' then 'refund'
        when 'timeout' then 'refund'
        when 'refunded' then 'refund'
        else null
    end
$$;

create or replace function public.cex_enqueue_exact_execution_terminal_settlement_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    terminal_action text;
    contract_row public.cex_invocation_ledger_contracts_v1%rowtype;
    reserve_row public.cex_gateway_ledger_reserve_commands_v1%rowtype;
    settlement_result jsonb;
begin
    if new.status is not distinct from old.status then
        return new;
    end if;

    terminal_action := public.cex_exact_execution_terminal_action_v1(new.status::text);
    if terminal_action is null then
        return new;
    end if;

    select * into contract_row
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = new.invocation_id
     for update;
    if not found then
        -- Legacy-compatible Invocation: no exact contract, no exact settlement trigger.
        return new;
    end if;

    select * into reserve_row
      from public.cex_gateway_ledger_reserve_commands_v1
     where invocation_id = new.invocation_id
     for update;
    if not found then
        raise exception 'exact Execution terminal transition requires a durable Gateway reserve command';
    end if;
    if reserve_row.execution_mode <> 'active'
       or reserve_row.status <> 'succeeded'
       or reserve_row.org_id is distinct from new.org_id
       or reserve_row.contract_hash is distinct from contract_row.contract_hash then
        raise exception 'exact Execution terminal transition requires verified active reserve evidence';
    end if;
    if contract_row.status <> 'reserved'
       or contract_row.last_operation_id is distinct from reserve_row.operation_id
       or contract_row.last_entry_id is null then
        raise exception 'exact Execution terminal transition requires the 0066 contract to be reserved';
    end if;

    settlement_result := public.cex_enqueue_execution_ledger_settlement_v1(
        new.execution_id,
        terminal_action,
        'active',
        5,
        'execution-terminal-trigger'
    );
    if settlement_result -> 'command' ->> 'invocation_id' is distinct from new.invocation_id::text
       or settlement_result -> 'command' ->> 'execution_id' is distinct from new.execution_id::text
       or settlement_result -> 'command' ->> 'action' is distinct from terminal_action then
        raise exception 'exact Execution terminal settlement projection mismatch';
    end if;

    return new;
end
$$;

create or replace function public.cex_guard_exact_execution_reactivation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    old_action text;
    new_action text;
begin
    if new.status is not distinct from old.status then
        return new;
    end if;

    old_action := public.cex_exact_execution_terminal_action_v1(old.status::text);
    new_action := public.cex_exact_execution_terminal_action_v1(new.status::text);
    if old_action is not null
       and new_action is null
       and exists (
           select 1
             from public.cex_execution_ledger_settlement_commands_v1 command
            where command.execution_id = old.execution_id
       ) then
        raise exception 'Execution with durable terminal settlement evidence cannot be reactivated';
    end if;
    return new;
end
$$;

drop trigger if exists trg_cex_guard_exact_execution_reactivation_v1
    on public.executions;
create trigger trg_cex_guard_exact_execution_reactivation_v1
before update of status on public.executions
for each row execute function public.cex_guard_exact_execution_reactivation_v1();

drop trigger if exists trg_cex_enqueue_exact_execution_terminal_settlement_v1
    on public.executions;
create trigger trg_cex_enqueue_exact_execution_terminal_settlement_v1
before update of status on public.executions
for each row execute function public.cex_enqueue_exact_execution_terminal_settlement_v1();

create or replace view public.cex_exact_execution_terminal_settlement_status_v1 as
select
    execution.execution_id,
    execution.invocation_id,
    execution.org_id,
    execution.status as execution_status,
    public.cex_exact_execution_terminal_action_v1(execution.status::text) as expected_action,
    contract.status as contract_status,
    reserve.command_id as reserve_command_id,
    reserve.status as reserve_status,
    reserve.execution_mode as reserve_execution_mode,
    settlement.command_id as settlement_command_id,
    settlement.action as settlement_action,
    settlement.status as settlement_status,
    case
        when public.cex_exact_execution_terminal_action_v1(execution.status::text) is null
            then 'not_terminal'
        when contract.invocation_id is null
            then 'legacy_or_unregistered'
        when reserve.command_id is null
            then 'missing_reserve_command'
        when reserve.status <> 'succeeded' or reserve.execution_mode <> 'active'
            then 'reserve_not_verified_active'
        when contract.status = 'reserved' and settlement.command_id is null
            then 'missing_terminal_settlement_command'
        when settlement.action is distinct from
             public.cex_exact_execution_terminal_action_v1(execution.status::text)
            then 'terminal_action_mismatch'
        when settlement.invocation_id is distinct from execution.invocation_id
             or settlement.execution_id is distinct from execution.execution_id
            then 'settlement_binding_mismatch'
        else 'consistent'
    end as consistency_status
from public.executions execution
left join public.cex_invocation_ledger_contracts_v1 contract
       on contract.invocation_id = execution.invocation_id
left join public.cex_gateway_ledger_reserve_commands_v1 reserve
       on reserve.invocation_id = execution.invocation_id
left join public.cex_execution_ledger_settlement_commands_v1 settlement
       on settlement.execution_id = execution.execution_id;

comment on function public.cex_enqueue_exact_execution_terminal_settlement_v1() is
    'Atomically binds exact Execution terminal status to one active consume/refund settlement command.';
comment on view public.cex_exact_execution_terminal_settlement_status_v1 is
    'Detects terminal exact Executions missing reserve or consume/refund evidence.';

commit;
