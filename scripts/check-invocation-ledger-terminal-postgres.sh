#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
: "${DATABASE_URL:?DATABASE_URL is required}"

python3 "$root/scripts/check-p0-migrations.py"
python3 "$root/scripts/check-invocation-ledger-contract-static.py"

psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 <<'SQL'
begin;

insert into public.organizations (org_id, name)
values ('e0000000-0000-4000-8000-000000000001', 'Invocation Ledger terminal test org');

select public.cex_open_account_v2(
    'e1000000-0000-4000-8000-000000000001'::uuid,
    'e0000000-0000-4000-8000-000000000001'::uuid,
    'e1100000-0000-4000-8000-000000000001'::uuid,
    'test',
    'credit',
    6::smallint,
    100000000::bigint,
    'org:e0000000:opening',
    'terminal-exclusivity-account-opening',
    'p0-terminal-exclusivity-test'
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
    'e2000000-0000-4000-8000-000000000001',
    'e0000000-0000-4000-8000-000000000001',
    'Created',
    '{"purpose":"terminal-consumed"}'::jsonb,
    'e3000000-0000-4000-8000-000000000001',
    clock_timestamp(),
    clock_timestamp()
),
(
    'e2000000-0000-4000-8000-000000000002',
    'e0000000-0000-4000-8000-000000000001',
    'Created',
    '{"purpose":"terminal-refunded"}'::jsonb,
    'e3000000-0000-4000-8000-000000000002',
    clock_timestamp(),
    clock_timestamp()
);

select public.cex_register_invocation_ledger_contract_v1(
    'e2000000-0000-4000-8000-000000000001'::uuid,
    'e1000000-0000-4000-8000-000000000001'::uuid,
    'e0000000-0000-4000-8000-000000000001'::uuid,
    'e3000000-0000-4000-8000-000000000001'::uuid,
    'credit',
    6::smallint,
    10000000::bigint,
    'gateway-service',
    'terminal-test-gateway'
);

select public.cex_register_invocation_ledger_contract_v1(
    'e2000000-0000-4000-8000-000000000002'::uuid,
    'e1000000-0000-4000-8000-000000000001'::uuid,
    'e0000000-0000-4000-8000-000000000001'::uuid,
    'e3000000-0000-4000-8000-000000000002'::uuid,
    'credit',
    6::smallint,
    5000000::bigint,
    'gateway-service',
    'terminal-test-gateway'
);

do $test$
declare
    request jsonb;
begin
    request := public.cex_invocation_ledger_effect_request_v1(
        'e2000000-0000-4000-8000-000000000001'::uuid,
        'reserve'
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
        'terminal-test-execution',
        'explicit'
    );

    request := public.cex_invocation_ledger_effect_request_v1(
        'e2000000-0000-4000-8000-000000000001'::uuid,
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
        'terminal-test-execution',
        'explicit'
    );
end
$test$;

do $test$
declare
    refund_after_consume_rejected boolean := false;
    before_entries bigint;
    after_entries bigint;
    before_balance bigint;
    before_reserved bigint;
    after_balance bigint;
    after_reserved bigint;
    contract_status text;
begin
    select count(*)::bigint
      into before_entries
      from public.ledger_entries
     where reference_id = 'e2000000-0000-4000-8000-000000000001';
    select balance_minor, reserved_minor
      into before_balance, before_reserved
      from public.accounts
     where account_id = 'e1000000-0000-4000-8000-000000000001';

    begin
        perform public.cex_invocation_ledger_effect_request_v1(
            'e2000000-0000-4000-8000-000000000001'::uuid,
            'refund'
        );
    exception
        when others then
            refund_after_consume_rejected := true;
    end;

    if not refund_after_consume_rejected then
        raise exception 'refund-after-consume terminal transition was not rejected';
    end if;

    select count(*)::bigint
      into after_entries
      from public.ledger_entries
     where reference_id = 'e2000000-0000-4000-8000-000000000001';
    select balance_minor, reserved_minor
      into after_balance, after_reserved
      from public.accounts
     where account_id = 'e1000000-0000-4000-8000-000000000001';
    select status
      into contract_status
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = 'e2000000-0000-4000-8000-000000000001';

    if before_entries <> 2
       or after_entries <> before_entries
       or after_balance <> before_balance
       or after_reserved <> before_reserved
       or contract_status <> 'consumed' then
        raise exception 'refund-after-consume rejection changed durable state';
    end if;
end
$test$;

do $test$
declare
    request jsonb;
begin
    request := public.cex_invocation_ledger_effect_request_v1(
        'e2000000-0000-4000-8000-000000000002'::uuid,
        'reserve'
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
        'terminal-test-execution',
        'explicit'
    );

    request := public.cex_invocation_ledger_effect_request_v1(
        'e2000000-0000-4000-8000-000000000002'::uuid,
        'refund'
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
        'terminal-test-execution',
        'explicit'
    );
end
$test$;

do $test$
declare
    consume_after_refund_rejected boolean := false;
    before_entries bigint;
    after_entries bigint;
    before_balance bigint;
    before_reserved bigint;
    after_balance bigint;
    after_reserved bigint;
    contract_status text;
begin
    select count(*)::bigint
      into before_entries
      from public.ledger_entries
     where reference_id = 'e2000000-0000-4000-8000-000000000002';
    select balance_minor, reserved_minor
      into before_balance, before_reserved
      from public.accounts
     where account_id = 'e1000000-0000-4000-8000-000000000001';

    begin
        perform public.cex_invocation_ledger_effect_request_v1(
            'e2000000-0000-4000-8000-000000000002'::uuid,
            'consume'
        );
    exception
        when others then
            consume_after_refund_rejected := true;
    end;

    if not consume_after_refund_rejected then
        raise exception 'consume-after-refund terminal transition was not rejected';
    end if;

    select count(*)::bigint
      into after_entries
      from public.ledger_entries
     where reference_id = 'e2000000-0000-4000-8000-000000000002';
    select balance_minor, reserved_minor
      into after_balance, after_reserved
      from public.accounts
     where account_id = 'e1000000-0000-4000-8000-000000000001';
    select status
      into contract_status
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = 'e2000000-0000-4000-8000-000000000002';

    if before_entries <> 2
       or after_entries <> before_entries
       or after_balance <> before_balance
       or after_reserved <> before_reserved
       or contract_status <> 'refunded' then
        raise exception 'consume-after-refund rejection changed durable state';
    end if;
end
$test$;

do $test$
declare
    consumed_count bigint;
    refunded_count bigint;
    missing_evidence bigint;
    account_balance bigint;
    account_reserved bigint;
begin
    select consumed_contracts, refunded_contracts, missing_effect_evidence
      into consumed_count, refunded_count, missing_evidence
      from public.cex_invocation_ledger_contract_status_v1;
    select balance_minor, reserved_minor
      into account_balance, account_reserved
      from public.accounts
     where account_id = 'e1000000-0000-4000-8000-000000000001';

    if consumed_count <> 1
       or refunded_count <> 1
       or missing_evidence <> 0
       or account_balance <> 90000000
       or account_reserved <> 0 then
        raise exception 'terminal settlement evidence is inconsistent';
    end if;
end
$test$;

rollback;
SQL

echo "P0 Invocation Ledger terminal exclusivity gate passed"