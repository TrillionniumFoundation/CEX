#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
: "${DATABASE_URL:?DATABASE_URL is required}"

psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f - <<'SQL'
begin;

insert into public.organizations (org_id, name, status, plan)
values (
    '73000000-0000-4000-8000-000000000001',
    'P0-N6 exact reserve fixture',
    'active',
    'test'
)
on conflict (org_id) do update
set name = excluded.name,
    status = excluded.status,
    plan = excluded.plan,
    updated_at = now();

-- Keep the synthetic command attached to a real tenant-bound Invocation. The
-- command insert itself still bypasses the exact-contract INSERT trigger so the
-- remainder of this probe isolates claim ownership, lease expiry, retry budget
-- and operator recovery rather than duplicating the dedicated 0066 lifecycle.
insert into public.invocations (
    invocation_id,
    org_id,
    status,
    request_payload,
    trace_id,
    created_at,
    updated_at
) values (
    '73000000-0000-4000-8000-000000000020',
    '73000000-0000-4000-8000-000000000001',
    'Created',
    '{"purpose":"gateway-exact-reserve-lifecycle"}'::jsonb,
    '73000000-0000-4000-8000-000000000050',
    clock_timestamp(),
    clock_timestamp()
);

-- The lifecycle probe intentionally inserts a synthetic command while all
-- INSERT triggers are disabled. Registration/binding correctness is covered by
-- the fresh migration chain and the dedicated 0066 contract gate.
set local session_replication_role = replica;
insert into public.cex_gateway_ledger_reserve_commands_v1 (
    command_id,
    invocation_id,
    org_id,
    operation_id,
    contract_hash,
    request_payload,
    request_fingerprint,
    execution_mode,
    status,
    max_attempts,
    source_principal
) values (
    '73000000-0000-4000-8000-000000000010',
    '73000000-0000-4000-8000-000000000020',
    '73000000-0000-4000-8000-000000000001',
    '73000000-0000-4000-8000-000000000030',
    'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    jsonb_build_object(
        'account_id', '73000000-0000-4000-8000-000000000040',
        'trace_id', '73000000-0000-4000-8000-000000000050',
        'operation_id', '73000000-0000-4000-8000-000000000030',
        'operation_kind', 'reserve',
        'currency_unit', 'credit',
        'currency_scale', 6,
        'amount_minor', '1000000',
        'reference_type', 'invocation',
        'reference_id', '73000000-0000-4000-8000-000000000020',
        'idempotency_scope', 'org:73000000-0000-4000-8000-000000000001:invocation:73000000-0000-4000-8000-000000000020',
        'idempotency_key', 'reserve'
    ),
    'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
    'shadow',
    'pending',
    2,
    'p0-n6-postgres-gate'
);
set local session_replication_role = origin;

do $test$
declare
    command_row public.cex_gateway_ledger_reserve_commands_v1%rowtype;
    claimed_count integer;
begin
    select count(*) into claimed_count
      from public.cex_claim_gateway_exact_reserves_v1('reserve-worker-a', 1, 10);
    if claimed_count <> 0 then
        raise exception 'shadow command was claimed';
    end if;

    perform public.cex_promote_gateway_exact_reserve_v1(
        '73000000-0000-4000-8000-000000000010',
        'p0-n6-operator'
    );

    select * into command_row
      from public.cex_claim_gateway_exact_reserves_v1('reserve-worker-a', 1, 10);
    if command_row.command_id is null
       or command_row.status <> 'claimed'
       or command_row.claimed_by <> 'reserve-worker-a'
       or command_row.attempt_count <> 1 then
        raise exception 'active Gateway reserve command was not claimed exactly once';
    end if;

    begin
        perform public.cex_finish_gateway_exact_reserve_v1(
            command_row.command_id,
            'wrong worker',
            'retry_wait',
            'temporary',
            'wrong owner must fail',
            503,
            null,
            null,
            1
        );
        raise exception 'wrong worker was accepted';
    exception when others then
        if sqlerrm = 'wrong worker was accepted' then
            raise;
        end if;
    end;

    perform public.cex_finish_gateway_exact_reserve_v1(
        command_row.command_id,
        'reserve-worker-a',
        'retry_wait',
        'ledger_503',
        'exact replay is safe',
        503,
        null,
        null,
        1
    );

    update public.cex_gateway_ledger_reserve_commands_v1
       set available_at = now()
     where command_id = command_row.command_id;

    select * into command_row
      from public.cex_claim_gateway_exact_reserves_v1('reserve-worker-b', 1, 10);
    if command_row.attempt_count <> 2 or command_row.claimed_by <> 'reserve-worker-b' then
        raise exception 'Gateway reserve retry attempt was not claimed by the second worker';
    end if;

    update public.cex_gateway_ledger_reserve_commands_v1
       set lease_expires_at = now() - interval '1 second'
     where command_id = command_row.command_id;

    perform public.cex_claim_gateway_exact_reserves_v1('lease-recovery', 1, 10);

    select * into command_row
      from public.cex_gateway_ledger_reserve_commands_v1
     where command_id = command_row.command_id;
    if command_row.status <> 'reconcile_required'
       or command_row.last_error_code <>
          'claim_lease_expired_after_final_attempt_unknown_outcome' then
        raise exception 'final expired claim did not become reconcile_required';
    end if;

    perform public.cex_acknowledge_gateway_exact_reserve_v1(
        command_row.command_id,
        'operator-a',
        'durable contract inspected after final lease expiry'
    );
    perform public.cex_requeue_gateway_exact_reserve_v1(
        command_row.command_id,
        'operator-a',
        'approved one exact replay after reconciliation',
        1
    );

    select * into command_row
      from public.cex_gateway_ledger_reserve_commands_v1
     where command_id = command_row.command_id;
    if command_row.status <> 'pending'
       or command_row.max_attempts <> 3
       or command_row.requeue_count <> 1
       or command_row.operator_acknowledged_at is not null then
        raise exception 'Gateway reserve acknowledgement/requeue evidence is invalid';
    end if;

    select * into command_row
      from public.cex_claim_gateway_exact_reserves_v1('reserve-worker-c', 1, 10);
    perform public.cex_finish_gateway_exact_reserve_v1(
        command_row.command_id,
        'reserve-worker-c',
        'reconcile_required',
        'response_decode_unknown',
        'second incident requires fresh acknowledgement',
        200,
        null,
        null,
        null
    );

    begin
        perform public.cex_requeue_gateway_exact_reserve_v1(
            command_row.command_id,
            'operator-a',
            'fresh acknowledgement was intentionally omitted',
            1
        );
        raise exception 'fresh acknowledgement was not required';
    exception when others then
        if sqlerrm = 'fresh acknowledgement was not required' then
            raise;
        end if;
    end;

    perform public.cex_acknowledge_gateway_exact_reserve_v1(
        command_row.command_id,
        'operator-b',
        'second durable contract inspection completed'
    );
    perform public.cex_requeue_gateway_exact_reserve_v1(
        command_row.command_id,
        'operator-b',
        'second exact replay explicitly approved',
        1
    );

    select * into command_row
      from public.cex_gateway_ledger_reserve_commands_v1
     where command_id = command_row.command_id;
    if command_row.requeue_count <> 2
       or command_row.max_attempts <> 4
       or command_row.operator_acknowledged_at is not null then
        raise exception 'fresh acknowledgement was not consumed by the second requeue';
    end if;

    if not exists (
        select 1
          from public.cex_gateway_ledger_reserve_transitions_v1
         where command_id = command_row.command_id
           and to_status = 'reconcile_required'
    ) then
        raise exception 'Gateway reserve append-only transition evidence is missing';
    end if;
end
$test$;

rollback;
SQL

echo "P0-N6 Gateway exact reserve PostgreSQL gate passed"