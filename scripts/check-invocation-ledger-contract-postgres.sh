#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
: "${DATABASE_URL:?DATABASE_URL is required}"

python3 "$root/scripts/check-p0-migrations.py"
python3 "$root/scripts/check-invocation-ledger-contract-static.py"

psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 <<'SQL'
begin;

insert into public.organizations (org_id, name)
values ('d0000000-0000-4000-8000-000000000001', 'Invocation Ledger contract org');

insert into public.accounts (
    account_id,
    org_id,
    account_type,
    currency_unit,
    balance,
    reserved,
    status
) values (
    'd1000000-0000-4000-8000-000000000001',
    'd0000000-0000-4000-8000-000000000001',
    'test',
    'credit',
    100.000000,
    0.000000,
    'active'
);

insert into public.invocations (
    invocation_id,
    org_id,
    status,
    request_payload,
    trace_id,
    created_at,
    updated_at
) values
(
    'd2000000-0000-4000-8000-000000000001',
    'd0000000-0000-4000-8000-000000000001',
    'Created',
    '{"purpose":"contract-consume"}'::jsonb,
    'd3000000-0000-4000-8000-000000000001',
    clock_timestamp(),
    clock_timestamp()
),
(
    'd2000000-0000-4000-8000-000000000002',
    'd0000000-0000-4000-8000-000000000001',
    'Created',
    '{"purpose":"contract-refund"}'::jsonb,
    'd3000000-0000-4000-8000-000000000002',
    clock_timestamp(),
    clock_timestamp()
),
(
    'd2000000-0000-4000-8000-000000000003',
    'd0000000-0000-4000-8000-000000000001',
    'Created',
    '{"purpose":"contract-wrong-operation"}'::jsonb,
    'd3000000-0000-4000-8000-000000000003',
    clock_timestamp(),
    clock_timestamp()
);

do $test$
declare
    first_result jsonb;
    replay_result jsonb;
    collision_rejected boolean := false;
    registration_intents bigint;
begin
    first_result := public.cex_register_invocation_ledger_contract_v1(
        'd2000000-0000-4000-8000-000000000001',
        'd1000000-0000-4000-8000-000000000001',
        'd0000000-0000-4000-8000-000000000001',
        'd3000000-0000-4000-8000-000000000001',
        'credit',
        6,
        10000000,
        'gateway-service',
        'p0-gateway-principal'
    );
    replay_result := public.cex_register_invocation_ledger_contract_v1(
        'd2000000-0000-4000-8000-000000000001',
        'd1000000-0000-4000-8000-000000000001',
        'd0000000-0000-4000-8000-000000000001',
        'd3000000-0000-4000-8000-000000000001',
        'credit',
        6,
        10000000,
        'gateway-service',
        'p0-gateway-principal'
    );

    if first_result ->> 'replayed' <> 'false'
       or replay_result ->> 'replayed' <> 'true'
       or first_result #>> '{contract,contract_hash}'
          <> replay_result #>> '{contract,contract_hash}' then
        raise exception 'exact registration replay failed';
    end if;

    begin
        perform public.cex_register_invocation_ledger_contract_v1(
            'd2000000-0000-4000-8000-000000000001',
            'd1000000-0000-4000-8000-000000000001',
            'd0000000-0000-4000-8000-000000000001',
            'd3000000-0000-4000-8000-000000000001',
            'credit',
            6,
            11000000,
            'gateway-service',
            'p0-gateway-principal'
        );
    exception
        when unique_violation then collision_rejected := true;
    end;
    if not collision_rejected then
        raise exception 'contract collision was not rejected';
    end if;

    select count(*)::bigint
      into registration_intents
      from public.cex_audit_outbox_v1
     where source_service = 'gateway-service'
       and envelope ->> 'event_type' = 'invocation.ledger_contract.registered'
       and envelope #>> '{payload,invocation_id}'
           = 'd2000000-0000-4000-8000-000000000001';
    if registration_intents <> 1 then
        raise exception 'registration Audit intent count mismatch: %', registration_intents;
    end if;
end
$test$;

do $test$
declare
    request jsonb;
    result jsonb;
    contract_status text;
    reserved_minor_value bigint;
begin
    request := public.cex_invocation_ledger_effect_request_v1(
        'd2000000-0000-4000-8000-000000000001',
        'reserve'
    );
    if jsonb_typeof(request -> 'amount_minor') <> 'string'
       or request ->> 'amount_minor' <> '10000000' then
        raise exception 'contract request does not use exact string minor units';
    end if;

    result := public.cex_apply_ledger_effect_v1(
        (request ->> 'account_id')::uuid,
        (request ->> 'trace_id')::uuid,
        (request ->> 'operation_id')::uuid,
        request ->> 'operation_kind',
        (request ->> 'amount_minor')::bigint,
        (request ->> 'currency_scale')::smallint,
        request ->> 'reference_type',
        (request ->> 'reference_id')::uuid,
        request ->> 'idempotency_scope',
        request ->> 'idempotency_key',
        'ledger-service',
        'execution-settlement-test',
        'explicit'
    );
    if result ->> 'replayed' <> 'false' then
        raise exception 'contract reserve unexpectedly replayed';
    end if;

    select status into contract_status
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = 'd2000000-0000-4000-8000-000000000001';
    select reserved_minor into reserved_minor_value
      from public.accounts
     where account_id = 'd1000000-0000-4000-8000-000000000001';
    if contract_status <> 'reserved' or reserved_minor_value <> 10000000 then
        raise exception 'reserve did not advance contract state';
    end if;

    request := public.cex_invocation_ledger_effect_request_v1(
        'd2000000-0000-4000-8000-000000000001',
        'consume'
    );
    perform public.cex_apply_ledger_effect_v1(
        (request ->> 'account_id')::uuid,
        (request ->> 'trace_id')::uuid,
        (request ->> 'operation_id')::uuid,
        request ->> 'operation_kind',
        (request ->> 'amount_minor')::bigint,
        (request ->> 'currency_scale')::smallint,
        request ->> 'reference_type',
        (request ->> 'reference_id')::uuid,
        request ->> 'idempotency_scope',
        request ->> 'idempotency_key',
        'ledger-service',
        'execution-settlement-test',
        'explicit'
    );

    select status into contract_status
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = 'd2000000-0000-4000-8000-000000000001';
    if contract_status <> 'consumed' then
        raise exception 'consume did not terminally settle contract';
    end if;

    begin
        perform public.cex_invocation_ledger_effect_request_v1(
            'd2000000-0000-4000-8000-000000000001',
            'refund'
        );
        raise exception 'refund request was allowed after consume';
    exception
        when raise_exception then null;
    end;
end
$test$;

do $test$
declare
    request jsonb;
    contract_status text;
    reserved_minor_value bigint;
begin
    perform public.cex_register_invocation_ledger_contract_v1(
        'd2000000-0000-4000-8000-000000000002',
        'd1000000-0000-4000-8000-000000000001',
        'd0000000-0000-4000-8000-000000000001',
        'd3000000-0000-4000-8000-000000000002',
        'credit',
        6,
        5000000,
        'gateway-service',
        'p0-gateway-principal'
    );

    request := public.cex_invocation_ledger_effect_request_v1(
        'd2000000-0000-4000-8000-000000000002',
        'reserve'
    );
    perform public.cex_apply_ledger_effect_v1(
        (request ->> 'account_id')::uuid,
        (request ->> 'trace_id')::uuid,
        (request ->> 'operation_id')::uuid,
        'reserve',
        (request ->> 'amount_minor')::bigint,
        6,
        'invocation',
        'd2000000-0000-4000-8000-000000000002',
        request ->> 'idempotency_scope',
        'reserve',
        'ledger-service',
        'execution-settlement-test',
        'explicit'
    );

    request := public.cex_invocation_ledger_effect_request_v1(
        'd2000000-0000-4000-8000-000000000002',
        'refund'
    );
    perform public.cex_apply_ledger_effect_v1(
        (request ->> 'account_id')::uuid,
        (request ->> 'trace_id')::uuid,
        (request ->> 'operation_id')::uuid,
        'refund',
        (request ->> 'amount_minor')::bigint,
        6,
        'invocation',
        'd2000000-0000-4000-8000-000000000002',
        request ->> 'idempotency_scope',
        'refund',
        'ledger-service',
        'execution-settlement-test',
        'explicit'
    );

    select status into contract_status
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = 'd2000000-0000-4000-8000-000000000002';
    select reserved_minor into reserved_minor_value
      from public.accounts
     where account_id = 'd1000000-0000-4000-8000-000000000001';
    if contract_status <> 'refunded' or reserved_minor_value <> 0 then
        raise exception 'refund did not terminally settle contract';
    end if;
end
$test$;

do $test$
declare
    request jsonb;
    wrong_operation_id uuid := 'd4000000-0000-4000-8000-000000000099';
    failed_closed boolean := false;
    entry_count bigint;
begin
    perform public.cex_register_invocation_ledger_contract_v1(
        'd2000000-0000-4000-8000-000000000003',
        'd1000000-0000-4000-8000-000000000001',
        'd0000000-0000-4000-8000-000000000001',
        'd3000000-0000-4000-8000-000000000003',
        'credit',
        6,
        1000000,
        'gateway-service',
        'p0-gateway-principal'
    );
    request := public.cex_invocation_ledger_effect_request_v1(
        'd2000000-0000-4000-8000-000000000003',
        'reserve'
    );

    begin
        perform public.cex_apply_ledger_effect_v1(
            (request ->> 'account_id')::uuid,
            (request ->> 'trace_id')::uuid,
            wrong_operation_id,
            'reserve',
            1000000,
            6,
            'invocation',
            'd2000000-0000-4000-8000-000000000003',
            request ->> 'idempotency_scope',
            'reserve',
            'ledger-service',
            'execution-settlement-test',
            'explicit'
        );
    exception
        when others then failed_closed := true;
    end;
    if not failed_closed then
        raise exception 'wrong operation identity did not fail closed';
    end if;

    select count(*)::bigint into entry_count
      from public.ledger_entries
     where reference_id = 'd2000000-0000-4000-8000-000000000003';
    if entry_count <> 0 then
        raise exception 'wrong operation identity left a ledger entry';
    end if;

    if exists (
        select 1
          from public.cex_invocation_ledger_contracts_v1
         where invocation_id = 'd2000000-0000-4000-8000-000000000003'
           and status <> 'registered'
    ) then
        raise exception 'wrong operation identity advanced contract state';
    end if;
end
$test$;

do $test$
declare
    status_row record;
begin
    select * into status_row
      from public.cex_invocation_ledger_contract_status_v1;
    if status_row.total_contracts <> 3
       or status_row.consumed_contracts <> 1
       or status_row.refunded_contracts <> 1
       or status_row.registered_contracts <> 1
       or status_row.missing_effect_evidence <> 0 then
        raise exception 'Invocation Ledger contract status view is inconsistent';
    end if;
end
$test$;

rollback;
SQL

echo "P0 Invocation Ledger contract gate passed"
