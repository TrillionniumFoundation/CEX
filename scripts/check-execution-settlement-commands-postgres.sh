#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
: "${DATABASE_URL:?DATABASE_URL is required}"

python3 "$root/scripts/check-execution-settlement-commands.py"

psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 <<'SQL'
begin;

insert into public.organizations (org_id, name)
values ('f0000000-0000-4000-8000-000000000001', 'P0-N5 settlement test org');

insert into public.accounts (
    account_id, org_id, account_type, currency_unit, balance, reserved, status
) values (
    'f1000000-0000-4000-8000-000000000001',
    'f0000000-0000-4000-8000-000000000001',
    'settlement-test',
    'credit',
    100.000000,
    0.000000,
    'active'
);

insert into public.invocations (
    invocation_id, org_id, status, request_payload, trace_id, created_at, updated_at
) values
(
    'f2000000-0000-4000-8000-000000000001',
    'f0000000-0000-4000-8000-000000000001',
    'Running',
    '{"fixture":"consume-success"}'::jsonb,
    'f3000000-0000-4000-8000-000000000001',
    clock_timestamp(), clock_timestamp()
),
(
    'f2000000-0000-4000-8000-000000000002',
    'f0000000-0000-4000-8000-000000000001',
    'Failed',
    '{"fixture":"reconcile-refund"}'::jsonb,
    'f3000000-0000-4000-8000-000000000002',
    clock_timestamp(), clock_timestamp()
),
(
    'f2000000-0000-4000-8000-000000000003',
    'f0000000-0000-4000-8000-000000000001',
    'Running',
    '{"fixture":"final-lease-unknown"}'::jsonb,
    'f3000000-0000-4000-8000-000000000003',
    clock_timestamp(), clock_timestamp()
),
(
    'f2000000-0000-4000-8000-000000000004',
    'f0000000-0000-4000-8000-000000000001',
    'Failed',
    '{"fixture":"lease-exact-replay"}'::jsonb,
    'f3000000-0000-4000-8000-000000000004',
    clock_timestamp(), clock_timestamp()
);

insert into public.executions (
    execution_id, invocation_id, status, trace_id, org_id, created_at, updated_at
) values
(
    'f4000000-0000-4000-8000-000000000001',
    'f2000000-0000-4000-8000-000000000001',
    'Running',
    'f3000000-0000-4000-8000-000000000001',
    'f0000000-0000-4000-8000-000000000001',
    clock_timestamp(), clock_timestamp()
),
(
    'f4000000-0000-4000-8000-000000000002',
    'f2000000-0000-4000-8000-000000000002',
    'Failed',
    'f3000000-0000-4000-8000-000000000002',
    'f0000000-0000-4000-8000-000000000001',
    clock_timestamp(), clock_timestamp()
),
(
    'f4000000-0000-4000-8000-000000000003',
    'f2000000-0000-4000-8000-000000000003',
    'Running',
    'f3000000-0000-4000-8000-000000000003',
    'f0000000-0000-4000-8000-000000000001',
    clock_timestamp(), clock_timestamp()
),
(
    'f4000000-0000-4000-8000-000000000004',
    'f2000000-0000-4000-8000-000000000004',
    'Failed',
    'f3000000-0000-4000-8000-000000000004',
    'f0000000-0000-4000-8000-000000000001',
    clock_timestamp(), clock_timestamp()
);

do $fixture$
declare
    invocation_ids uuid[] := array[
        'f2000000-0000-4000-8000-000000000001'::uuid,
        'f2000000-0000-4000-8000-000000000002'::uuid,
        'f2000000-0000-4000-8000-000000000003'::uuid,
        'f2000000-0000-4000-8000-000000000004'::uuid
    ];
    trace_ids uuid[] := array[
        'f3000000-0000-4000-8000-000000000001'::uuid,
        'f3000000-0000-4000-8000-000000000002'::uuid,
        'f3000000-0000-4000-8000-000000000003'::uuid,
        'f3000000-0000-4000-8000-000000000004'::uuid
    ];
    request jsonb;
    index_value integer;
begin
    for index_value in 1..4 loop
        perform public.cex_register_invocation_ledger_contract_v1(
            invocation_ids[index_value],
            'f1000000-0000-4000-8000-000000000001',
            'f0000000-0000-4000-8000-000000000001',
            trace_ids[index_value],
            'credit',
            6,
            index_value::bigint * 1000000,
            'gateway-service',
            'p0-n5-fixture'
        );
        request := public.cex_invocation_ledger_effect_request_v1(
            invocation_ids[index_value], 'reserve'
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
            'p0-n5-fixture',
            'explicit'
        );
    end loop;
end
$fixture$;

-- Direct table writes cannot bypass Execution/Invocation/tenant/0066 binding validation.
do $test$
declare
    request jsonb;
    contract_hash_value text;
    direct_insert_rejected boolean := false;
begin
    request := public.cex_invocation_ledger_effect_request_v1(
        'f2000000-0000-4000-8000-000000000004', 'refund'
    );
    select contract_hash into contract_hash_value
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = 'f2000000-0000-4000-8000-000000000004';
    begin
        insert into public.cex_execution_ledger_settlement_commands_v1 (
            command_id, invocation_id, execution_id, org_id, action, operation_id,
            contract_hash, request_payload, request_fingerprint, execution_mode,
            max_attempts, source_principal
        ) values (
            public.cex_deterministic_uuid_v1(
                'cex:execution-ledger-settlement:f2000000-0000-4000-8000-000000000004'
            ),
            'f2000000-0000-4000-8000-000000000004',
            'f4000000-0000-4000-8000-000000000003',
            'f0000000-0000-4000-8000-000000000001',
            'refund',
            (request ->> 'operation_id')::uuid,
            contract_hash_value,
            request,
            public.cex_execution_ledger_settlement_request_fingerprint_v1(request),
            'active', 2, 'direct-insert-probe'
        );
    exception when others then
        direct_insert_rejected := true;
    end;
    if not direct_insert_rejected then
        raise exception 'direct settlement table insert bypassed cross-table validation';
    end if;
end
$test$;

-- Shadow command exact replay, promotion, claim ownership and verified consume.
do $test$
declare
    first_result jsonb;
    replay_result jsonb;
    command_id_value uuid;
    claimed record;
    request jsonb;
    receipt jsonb;
    wrong_worker_rejected boolean := false;
    opposite_action_rejected boolean := false;
    claimed_count bigint;
begin
    first_result := public.cex_enqueue_execution_ledger_settlement_v1(
        'f4000000-0000-4000-8000-000000000001',
        'consume', 'shadow', 3, 'p0-n5-test'
    );
    replay_result := public.cex_enqueue_execution_ledger_settlement_v1(
        'f4000000-0000-4000-8000-000000000001',
        'consume', 'shadow', 3, 'p0-n5-test'
    );
    if first_result ->> 'replayed' <> 'false'
       or replay_result ->> 'replayed' <> 'true'
       or first_result #>> '{command,request_fingerprint}'
          is distinct from replay_result #>> '{command,request_fingerprint}' then
        raise exception 'settlement command exact replay failed';
    end if;
    command_id_value := (first_result #>> '{command,command_id}')::uuid;

    begin
        perform public.cex_enqueue_execution_ledger_settlement_v1(
            'f4000000-0000-4000-8000-000000000001',
            'refund', 'shadow', 3, 'p0-n5-test'
        );
    exception when unique_violation then
        opposite_action_rejected := true;
    end;
    if not opposite_action_rejected then
        raise exception 'consume/refund command collision was not rejected';
    end if;

    select count(*)::bigint into claimed_count
      from public.cex_claim_execution_ledger_settlements_v1(
          'p0-n5-worker-a', 10, 60
      );
    if claimed_count <> 0 then
        raise exception 'shadow settlement command was claimable';
    end if;

    perform public.cex_promote_execution_ledger_settlement_v1(
        command_id_value, 'p0-n5-operator'
    );
    select * into claimed
      from public.cex_claim_execution_ledger_settlements_v1(
          'p0-n5-worker-a', 1, 60
      );
    if claimed.command_id is distinct from command_id_value
       or claimed.attempt_count <> 1
       or claimed.status <> 'claimed' then
        raise exception 'active settlement command claim failed';
    end if;

    -- wrong worker must never complete another worker's lease.
    begin
        perform public.cex_finish_execution_ledger_settlement_v1(
            command_id_value, 'p0-n5-wrong-worker', 'reconcile_required',
            'wrong_worker', 'wrong worker probe', null, null, null, null
        );
    exception when others then
        wrong_worker_rejected := true;
    end;
    if not wrong_worker_rejected then
        raise exception 'wrong worker completion was accepted';
    end if;

    request := public.cex_invocation_ledger_effect_request_v1(
        'f2000000-0000-4000-8000-000000000001', 'consume'
    );
    receipt := public.cex_apply_ledger_effect_v1(
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
        'ledger-service', 'p0-n5-worker-a', 'explicit'
    );
    perform public.cex_finish_execution_ledger_settlement_v1(
        command_id_value, 'p0-n5-worker-a', 'succeeded',
        null, null, 200, receipt, (receipt ->> 'replayed')::boolean, null
    );
    -- Exact finish replay returns the original verified evidence.
    perform public.cex_finish_execution_ledger_settlement_v1(
        command_id_value, 'p0-n5-worker-a', 'succeeded',
        null, null, 200, receipt, (receipt ->> 'replayed')::boolean, null
    );
end
$test$;

-- Unknown outcome acknowledgement, bounded requeue, fresh acknowledgement and refund success.
do $test$
declare
    command_result jsonb;
    command_id_value uuid;
    claimed record;
    after_requeue record;
    request jsonb;
    receipt jsonb;
begin
    command_result := public.cex_enqueue_execution_ledger_settlement_v1(
        'f4000000-0000-4000-8000-000000000002',
        'refund', 'active', 2, 'p0-n5-test'
    );
    command_id_value := (command_result #>> '{command,command_id}')::uuid;
    select * into claimed
      from public.cex_claim_execution_ledger_settlements_v1(
          'p0-n5-worker-b', 1, 60
      );
    if claimed.command_id is distinct from command_id_value then
        raise exception 'refund reconciliation command was not claimed';
    end if;

    perform public.cex_finish_execution_ledger_settlement_v1(
        command_id_value, 'p0-n5-worker-b', 'reconcile_required',
        'timeout_unknown', 'simulated unknown remote outcome', null, null, null, null
    );
    perform public.cex_acknowledge_execution_ledger_settlement_v1(
        command_id_value, 'p0-n5-operator', 'contract remains reserved; exact refund replay approved'
    );
    select * into after_requeue
      from public.cex_requeue_execution_ledger_settlement_v1(
          command_id_value, 'p0-n5-operator', 'retry exact refund after review', 1
      );
    if after_requeue.status <> 'pending'
       or after_requeue.requeue_count <> 1
       or after_requeue.max_attempts <> 3
       or after_requeue.operator_acknowledged_at is not null
       or after_requeue.last_requeued_by <> 'p0-n5-operator'
       or after_requeue.last_requeue_additional_attempts <> 1 then
        raise exception 'requeue did not require and consume a fresh acknowledgement';
    end if;
    -- Lost-response replay of the same operator command must not add attempts twice.
    select * into after_requeue
      from public.cex_requeue_execution_ledger_settlement_v1(
          command_id_value, 'p0-n5-operator', 'retry exact refund after review', 1
      );
    if after_requeue.requeue_count <> 1 or after_requeue.max_attempts <> 3 then
        raise exception 'settlement requeue exact replay changed the command twice';
    end if;

    select * into claimed
      from public.cex_claim_execution_ledger_settlements_v1(
          'p0-n5-worker-b', 1, 60
      );
    request := public.cex_invocation_ledger_effect_request_v1(
        'f2000000-0000-4000-8000-000000000002', 'refund'
    );
    receipt := public.cex_apply_ledger_effect_v1(
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
        'ledger-service', 'p0-n5-worker-b', 'explicit'
    );
    perform public.cex_finish_execution_ledger_settlement_v1(
        command_id_value, 'p0-n5-worker-b', 'succeeded',
        null, null, 200, receipt, (receipt ->> 'replayed')::boolean, null
    );
end
$test$;

-- A final expired claim is an unknown outcome, never an assumed dead letter.
do $test$
declare
    command_result jsonb;
    command_id_value uuid;
    claimed record;
    recovered_count bigint;
    final_row record;
begin
    command_result := public.cex_enqueue_execution_ledger_settlement_v1(
        'f4000000-0000-4000-8000-000000000003',
        'consume', 'active', 1, 'p0-n5-test'
    );
    command_id_value := (command_result #>> '{command,command_id}')::uuid;
    select * into claimed
      from public.cex_claim_execution_ledger_settlements_v1(
          'p0-n5-worker-c', 1, 60
      );
    update public.cex_execution_ledger_settlement_commands_v1
       set lease_expires_at = now() - interval '1 second'
     where command_id = command_id_value;

    select count(*)::bigint into recovered_count
      from public.cex_claim_execution_ledger_settlements_v1(
          'p0-n5-worker-d', 1, 60
      );
    if recovered_count <> 0 then
        raise exception 'final expired claim was automatically replayed';
    end if;
    select * into final_row
      from public.cex_execution_ledger_settlement_commands_v1
     where command_id = command_id_value;
    if final_row.status <> 'reconcile_required'
       or final_row.last_error_code
          <> 'claim_lease_expired_after_final_attempt_unknown_outcome'
       or final_row.dead_lettered_at is not null then
        raise exception 'final lease expiry did not preserve unknown-outcome semantics';
    end if;
end
$test$;

-- A non-final expired lease is immediately reclaimable as the same exact operation.
do $test$
declare
    command_result jsonb;
    command_id_value uuid;
    first_claim record;
    replay_claim record;
    retry_transition_count bigint;
begin
    command_result := public.cex_enqueue_execution_ledger_settlement_v1(
        'f4000000-0000-4000-8000-000000000004',
        'refund', 'active', 2, 'p0-n5-test'
    );
    command_id_value := (command_result #>> '{command,command_id}')::uuid;
    select * into first_claim
      from public.cex_claim_execution_ledger_settlements_v1(
          'p0-n5-worker-e', 1, 60
      );
    update public.cex_execution_ledger_settlement_commands_v1
       set lease_expires_at = now() - interval '1 second'
     where command_id = command_id_value;

    select * into replay_claim
      from public.cex_claim_execution_ledger_settlements_v1(
          'p0-n5-worker-f', 1, 60
      );
    if replay_claim.command_id is distinct from command_id_value
       or replay_claim.attempt_count <> 2
       or replay_claim.operation_id is distinct from first_claim.operation_id then
        raise exception 'expired claim exact replay identity failed';
    end if;
    select count(*)::bigint into retry_transition_count
      from public.cex_execution_ledger_settlement_transitions_v1
     where command_id = command_id_value
       and from_status = 'claimed'
       and to_status = 'retry_wait'
       and error_code = 'claim_lease_expired_exact_replay';
    if retry_transition_count <> 1 then
        raise exception 'lease recovery transition evidence is missing';
    end if;
    perform public.cex_finish_execution_ledger_settlement_v1(
        command_id_value, 'p0-n5-worker-f', 'retry_wait',
        'temporary_upstream', 'final retry remains an unknown outcome', 503, null, null, 1
    );
    if not exists (
        select 1
          from public.cex_execution_ledger_settlement_commands_v1
         where command_id = command_id_value
           and status = 'reconcile_required'
           and last_error_code = 'retry_budget_exhausted_unknown_outcome'
    ) then
        raise exception 'database authority dead-lettered a final retryable outcome';
    end if;
end
$test$;

-- Status projection, append-only evidence and transactional Audit intent cardinality.
do $test$
declare
    status_total bigint;
    unacknowledged_total bigint;
    transition_mutation_rejected boolean := false;
    command_audits bigint;
begin
    select coalesce(sum(command_count), 0),
           coalesce(sum(unacknowledged_operator_items), 0)
      into status_total, unacknowledged_total
      from public.cex_execution_ledger_settlement_status_v1;
    if status_total <> 4 or unacknowledged_total <> 2 then
        raise exception 'settlement status projection mismatch: total %, operator %',
            status_total, unacknowledged_total;
    end if;

    begin
        update public.cex_execution_ledger_settlement_transitions_v1
           set error_code = 'mutated'
         where transition_id = (
             select min(transition_id)
               from public.cex_execution_ledger_settlement_transitions_v1
         );
    exception when others then
        transition_mutation_rejected := true;
    end;
    if not transition_mutation_rejected then
        raise exception 'append-only transition mutation was accepted';
    end if;

    select count(*)::bigint into command_audits
      from public.cex_audit_outbox_v1
     where source_service = 'execution-service'
       and envelope ->> 'event_type' = 'execution.ledger_settlement.commanded';
    if command_audits <> 4 then
        raise exception 'settlement command Audit intent count mismatch: %', command_audits;
    end if;
end
$test$;

rollback;
SQL

echo "P0-N5 Execution settlement command gate passed"
