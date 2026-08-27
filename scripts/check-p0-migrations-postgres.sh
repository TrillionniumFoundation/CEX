#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
: "${DATABASE_URL:?DATABASE_URL is required}"

python3 "$root/scripts/check-p0-migrations.py"

database_name=$(psql "$DATABASE_URL" -X -A -t -v ON_ERROR_STOP=1 \
  -c "select current_database()")
if [[ ! "$database_name" =~ (test|ci|scratch|tmp) ]] \
  && [[ "${CEX_ALLOW_MIGRATION_TEST_ON_ANY_DATABASE:-0}" != "1" ]]; then
  echo "ERROR: refusing migration test against database '$database_name'" >&2
  echo "Use a disposable database whose name contains test/ci/scratch/tmp." >&2
  exit 2
fi

while IFS= read -r migration; do
  echo "applying $(basename "$migration")"
  psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 -f "$migration" >/dev/null
done < <(find "$root/migrations" -maxdepth 1 -type f -name '[0-9][0-9][0-9][0-9]_*.sql' | sort)

psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 <<'SQL'
begin;

insert into public.organizations (org_id, name)
values ('10000000-0000-4000-8000-000000000001', 'P0 migration test org');

insert into public.organizations (org_id, name)
values ('10000000-0000-4000-8000-000000000002', 'P0 mismatch test org');

insert into public.accounts (
    account_id, org_id, account_type, currency_unit, balance, reserved
) values (
    '20000000-0000-4000-8000-000000000001',
    '10000000-0000-4000-8000-000000000001',
    'test',
    'credit',
    10.250000,
    1.500000
);

do $test$
declare
    account_balance_minor bigint;
    account_reserved_minor bigint;
begin
    select balance_minor, reserved_minor
      into account_balance_minor, account_reserved_minor
      from public.accounts
     where account_id = '20000000-0000-4000-8000-000000000001';

    if account_balance_minor <> 10250000 or account_reserved_minor <> 1500000 then
        raise exception 'money v2 account synchronization mismatch';
    end if;

    update public.accounts
       set balance_minor = 11000000,
           reserved_minor = 2000000
     where account_id = '20000000-0000-4000-8000-000000000001';

    if not exists (
        select 1
          from public.accounts
         where account_id = '20000000-0000-4000-8000-000000000001'
           and balance = 11.000000
           and reserved = 2.000000
    ) then
        raise exception 'money v2 minor-to-numeric synchronization failed';
    end if;
end
$test$;

insert into public.ledger_entries (
    entry_id, account_id, direction, amount, reason, idempotency_key
) values (
    '30000000-0000-4000-8000-000000000001',
    '20000000-0000-4000-8000-000000000001',
    'debit',
    1.250000,
    'p0-migration-test',
    'p0-migration-test-ledger-entry'
);

do $test$
begin
    if not exists (
        select 1
          from public.ledger_entries
         where entry_id = '30000000-0000-4000-8000-000000000001'
           and amount_minor = 1250000
    ) then
        raise exception 'money v2 ledger-entry synchronization failed';
    end if;
end
$test$;

insert into public.cex_saga_commands_v1 (
    command_id, workflow_kind, workflow_id, operation_key,
    command_kind, execution_mode, status, max_attempts, payload
) values
(
    '40000000-0000-4000-8000-000000000001',
    'invocation',
    '50000000-0000-4000-8000-000000000001',
    'p0-test:shadow',
    'execution_create',
    'shadow',
    'pending',
    3,
    '{"shadow_only":true}'::jsonb
),
(
    '40000000-0000-4000-8000-000000000002',
    'invocation',
    '50000000-0000-4000-8000-000000000002',
    'p0-test:active',
    'execution_create',
    'active',
    'pending',
    3,
    '{"shadow_only":false}'::jsonb
);

do $test$
declare
    claimed_count bigint;
    claimed_mode text;
begin
    select count(*), min(execution_mode)
      into claimed_count, claimed_mode
      from public.cex_claim_saga_commands_v1(
          'p0-test-worker',
          array['execution_create'],
          10,
          30
      );

    if claimed_count <> 1 or claimed_mode <> 'active' then
        raise exception 'saga claim selected shadow commands or wrong count';
    end if;
    if not exists (
        select 1
          from public.cex_saga_commands_v1
         where operation_key = 'p0-test:shadow'
           and status = 'pending'
    ) then
        raise exception 'shadow saga command was mutated by active claim';
    end if;
end
$test$;

do $test$
declare
    first_result jsonb;
    replay_result jsonb;
    second_result jsonb;
    first_hash text;
    mutation_rejected boolean := false;
begin
    first_result := public.cex_append_audit_event_v2(
        '60000000-0000-4000-8000-000000000001',
        '70000000-0000-4000-8000-000000000001',
        '10000000-0000-4000-8000-000000000001',
        'gateway-service',
        'workload-token-v1',
        'gateway-service',
        'p0-test-actor',
        'p0.test.first',
        'cex.audit.event.v2',
        clock_timestamp(),
        '{"value":1}'::jsonb
    );

    replay_result := public.cex_append_audit_event_v2(
        '60000000-0000-4000-8000-000000000001',
        '70000000-0000-4000-8000-000000000001',
        '10000000-0000-4000-8000-000000000001',
        'gateway-service',
        'workload-token-v1',
        'gateway-service',
        'p0-test-actor',
        'p0.test.first',
        'cex.audit.event.v2',
        (first_result #>> '{record,occurred_at}')::timestamptz,
        '{"value":1}'::jsonb
    );

    if first_result ->> 'replayed' <> 'false'
       or replay_result ->> 'replayed' <> 'true'
       or first_result #>> '{record,event_hash}'
          <> replay_result #>> '{record,event_hash}' then
        raise exception 'audit v2 exact replay contract failed';
    end if;

    begin
        perform public.cex_append_audit_event_v2(
            '60000000-0000-4000-8000-000000000001',
            '70000000-0000-4000-8000-000000000001',
            '10000000-0000-4000-8000-000000000001',
            'gateway-service',
            'workload-token-v1',
            'gateway-service',
            'p0-test-actor',
            'p0.test.first',
            'cex.audit.event.v2',
            (first_result #>> '{record,occurred_at}')::timestamptz,
            '{"value":2}'::jsonb
        );
        raise exception 'audit v2 id collision was not rejected';
    exception
        when unique_violation then null;
    end;

    second_result := public.cex_append_audit_event_v2(
        '60000000-0000-4000-8000-000000000002',
        '70000000-0000-4000-8000-000000000001',
        '10000000-0000-4000-8000-000000000001',
        'execution-service',
        'workload-token-v1',
        'execution-service',
        'p0-test-worker',
        'p0.test.second',
        'cex.audit.event.v2',
        clock_timestamp(),
        '{"value":2}'::jsonb
    );

    first_hash := first_result #>> '{record,event_hash}';
    if second_result #>> '{record,tenant_sequence}' <> '2'
       or second_result #>> '{record,previous_event_hash}' <> first_hash then
        raise exception 'audit v2 tenant sequence/hash chain failed';
    end if;

    begin
        update public.cex_audit_events_v2
           set payload = '{"tampered":true}'::jsonb
         where event_id = '60000000-0000-4000-8000-000000000001';
    exception
        when others then mutation_rejected := true;
    end;
    if not mutation_rejected then
        raise exception 'audit v2 append-only trigger failed';
    end if;
end
$test$;

do $test$
declare
    event_id_value uuid := '80000000-0000-4000-8000-000000000001';
    trace_id_value uuid := '81000000-0000-4000-8000-000000000001';
    envelope_value jsonb;
    first_row public.cex_audit_outbox_v1%rowtype;
    replay_row public.cex_audit_outbox_v1%rowtype;
    claimed_row public.cex_audit_outbox_v1%rowtype;
    failed_row public.cex_audit_outbox_v1%rowtype;
    delivered_row public.cex_audit_outbox_v1%rowtype;
    append_result jsonb;
    receipt_value jsonb;
    collision_rejected boolean := false;
begin
    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', trace_id_value,
        'org_id', '10000000-0000-4000-8000-000000000001'::uuid,
        'actor_type', 'execution-service',
        'actor_id', 'p0-dispatcher-test',
        'event_type', 'p0.dispatcher.delivery',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', clock_timestamp(),
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'execution-service',
            'value', 1
        )
    );

    first_row := public.cex_enqueue_audit_outbox_v1(
        'execution-service',
        event_id_value,
        trace_id_value,
        '10000000-0000-4000-8000-000000000001',
        envelope_value,
        3
    );
    replay_row := public.cex_enqueue_audit_outbox_v1(
        'execution-service',
        event_id_value,
        trace_id_value,
        '10000000-0000-4000-8000-000000000001',
        envelope_value,
        3
    );

    if first_row.outbox_id <> replay_row.outbox_id then
        raise exception 'audit outbox exact enqueue replay changed row identity';
    end if;

    begin
        perform public.cex_enqueue_audit_outbox_v1(
            'execution-service',
            event_id_value,
            trace_id_value,
            '10000000-0000-4000-8000-000000000001',
            jsonb_set(envelope_value, '{payload,value}', '2'::jsonb),
            3
        );
        raise exception 'audit outbox immutable collision was not rejected';
    exception
        when unique_violation then collision_rejected := true;
    end;
    if not collision_rejected then
        raise exception 'audit outbox collision probe did not execute';
    end if;

    select *
      into claimed_row
      from public.cex_claim_audit_outbox_v1('p0-audit-worker', 1, 30);
    if claimed_row.outbox_id <> first_row.outbox_id then
        raise exception 'audit outbox claim selected unexpected row';
    end if;

    failed_row := public.cex_fail_audit_outbox_delivery_v1(
        first_row.outbox_id,
        'p0-audit-worker',
        true,
        'temporary_unavailable',
        'retry test',
        503,
        1
    );
    if failed_row.status <> 'retry_wait'
       or failed_row.claimed_by is not null
       or failed_row.attempt_count <> 1 then
        raise exception 'audit outbox retry transition failed';
    end if;

    update public.cex_audit_outbox_v1
       set available_at = now() - interval '1 second'
     where outbox_id = first_row.outbox_id;

    select *
      into claimed_row
      from public.cex_claim_audit_outbox_v1('p0-audit-worker', 1, 30);
    if claimed_row.attempt_count <> 2 then
        raise exception 'audit outbox re-claim did not increment attempt count';
    end if;

    append_result := public.cex_append_audit_event_v2(
        event_id_value,
        trace_id_value,
        '10000000-0000-4000-8000-000000000001',
        'audit-outbox-dispatcher',
        'workload-token-v1',
        envelope_value ->> 'actor_type',
        envelope_value ->> 'actor_id',
        envelope_value ->> 'event_type',
        envelope_value ->> 'schema_version',
        (envelope_value ->> 'occurred_at')::timestamptz,
        envelope_value -> 'payload'
    );
    receipt_value := jsonb_build_object(
        'replayed', append_result -> 'replayed',
        'record', append_result -> 'record'
    );

    delivered_row := public.cex_mark_audit_outbox_delivered_v1(
        first_row.outbox_id,
        'p0-audit-worker',
        event_id_value,
        append_result #>> '{record,event_hash}',
        (append_result #>> '{record,tenant_sequence}')::bigint,
        receipt_value,
        201
    );
    if delivered_row.status <> 'delivered'
       or delivered_row.delivery_receipt is null
       or delivered_row.delivered_event_hash is null then
        raise exception 'audit outbox verified ACK failed';
    end if;

    delivered_row := public.cex_mark_audit_outbox_delivered_v1(
        first_row.outbox_id,
        'p0-audit-worker',
        event_id_value,
        append_result #>> '{record,event_hash}',
        (append_result #>> '{record,tenant_sequence}')::bigint,
        receipt_value,
        201
    );
    if delivered_row.status <> 'delivered' then
        raise exception 'audit outbox exact ACK replay failed';
    end if;
end
$test$;

do $test$
declare
    event_id_value uuid := '82000000-0000-4000-8000-000000000001';
    trace_id_value uuid := '83000000-0000-4000-8000-000000000001';
    envelope_value jsonb;
    outbox_row public.cex_audit_outbox_v1%rowtype;
    wrong_worker_rejected boolean := false;
begin
    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', trace_id_value,
        'org_id', '10000000-0000-4000-8000-000000000001'::uuid,
        'actor_type', 'identity-service',
        'actor_id', 'p0-dead-letter-test',
        'event_type', 'p0.dispatcher.dead_letter',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', clock_timestamp(),
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'identity-service'
        )
    );

    outbox_row := public.cex_enqueue_audit_outbox_v1(
        'identity-service',
        event_id_value,
        trace_id_value,
        '10000000-0000-4000-8000-000000000001',
        envelope_value,
        2
    );

    perform public.cex_claim_audit_outbox_v1('p0-dead-letter-worker', 1, 30);

    begin
        perform public.cex_fail_audit_outbox_delivery_v1(
            outbox_row.outbox_id,
            'wrong-worker',
            false,
            'permanent_contract_error',
            'wrong worker probe',
            400,
            null
        );
    exception
        when others then wrong_worker_rejected := true;
    end;
    if not wrong_worker_rejected then
        raise exception 'audit outbox wrong-worker transition was accepted';
    end if;

    outbox_row := public.cex_fail_audit_outbox_delivery_v1(
        outbox_row.outbox_id,
        'p0-dead-letter-worker',
        false,
        'permanent_contract_error',
        'dead-letter probe',
        400,
        null
    );
    if outbox_row.status <> 'dead_letter'
       or outbox_row.dead_lettered_at is null then
        raise exception 'audit outbox dead-letter transition failed';
    end if;
end
$test$;

do $test$
declare
    event_id_value uuid := '84000000-0000-4000-8000-000000000001';
    trace_id_value uuid := '85000000-0000-4000-8000-000000000001';
    outbox_row public.cex_audit_outbox_v1%rowtype;
    expired_lease_rejected boolean := false;
begin
    outbox_row := public.cex_enqueue_audit_outbox_v1(
        'execution-service',
        event_id_value,
        trace_id_value,
        '10000000-0000-4000-8000-000000000001',
        jsonb_build_object(
            'event_id', event_id_value,
            'trace_id', trace_id_value,
            'org_id', '10000000-0000-4000-8000-000000000001'::uuid,
            'actor_type', 'execution-service',
            'actor_id', 'p0-expired-lease-test',
            'event_type', 'p0.dispatcher.expired_lease',
            'schema_version', 'cex.audit.event.v2',
            'occurred_at', clock_timestamp(),
            'payload', jsonb_build_object(
                '_cex_audit_source_service', 'execution-service'
            )
        ),
        2
    );
    perform public.cex_claim_audit_outbox_v1('p0-expired-worker', 1, 30);
    update public.cex_audit_outbox_v1
       set lease_expires_at = now() - interval '1 second'
     where outbox_id = outbox_row.outbox_id;

    begin
        perform public.cex_fail_audit_outbox_delivery_v1(
            outbox_row.outbox_id,
            'p0-expired-worker',
            true,
            'expired_lease',
            'expired lease probe',
            503,
            1
        );
    exception
        when others then expired_lease_rejected := true;
    end;
    if not expired_lease_rejected then
        raise exception 'audit outbox expired lease transition was accepted';
    end if;
end
$test$;

insert into public.invocations (
    invocation_id,
    org_id,
    status,
    request_payload,
    trace_id,
    created_at,
    updated_at
) values (
    '90000000-0000-4000-8000-000000000001',
    '10000000-0000-4000-8000-000000000001',
    'Created',
    '{"prompt":"p0 execution audit test"}'::jsonb,
    '91000000-0000-4000-8000-000000000001',
    clock_timestamp(),
    clock_timestamp()
);

insert into public.executions (
    execution_id,
    invocation_id,
    status,
    trace_id,
    org_id
) values (
    '92000000-0000-4000-8000-000000000001',
    '90000000-0000-4000-8000-000000000001',
    'Queued',
    '91000000-0000-4000-8000-000000000001',
    '10000000-0000-4000-8000-000000000001'
);

do $test$
declare
    before_count bigint;
    after_count bigint;
    org_mismatch_rejected boolean := false;
begin
    if not exists (
        select 1
          from public.executions
         where execution_id = '92000000-0000-4000-8000-000000000001'
           and audit_revision = 1
    ) then
        raise exception 'execution audit revision insert failed';
    end if;
    if not exists (
        select 1
          from public.cex_audit_outbox_v1
         where source_service = 'execution-service'
           and envelope #>> '{payload,execution_id}'
               = '92000000-0000-4000-8000-000000000001'
           and envelope ->> 'event_type' = 'execution.persisted.created'
    ) then
        raise exception 'execution insert did not enqueue an audit intent';
    end if;

    select count(*)
      into before_count
      from public.cex_audit_outbox_v1
     where source_service = 'execution-service'
       and envelope #>> '{payload,execution_id}'
           = '92000000-0000-4000-8000-000000000001';

    update public.executions
       set updated_at = clock_timestamp()
     where execution_id = '92000000-0000-4000-8000-000000000001';

    select count(*)
      into after_count
      from public.cex_audit_outbox_v1
     where source_service = 'execution-service'
       and envelope #>> '{payload,execution_id}'
           = '92000000-0000-4000-8000-000000000001';

    if before_count <> after_count then
        raise exception 'execution timestamp-only update emitted an audit intent';
    end if;

    update public.executions
       set status = 'Dispatching',
           worker_id = 'p0-execution-worker',
           lease_expires_at = now() + interval '30 seconds',
           updated_at = clock_timestamp()
     where execution_id = '92000000-0000-4000-8000-000000000001';

    if not exists (
        select 1
          from public.executions
         where execution_id = '92000000-0000-4000-8000-000000000001'
           and audit_revision = 2
    ) then
        raise exception 'execution meaningful update did not increment audit revision';
    end if;
    if not exists (
        select 1
          from public.cex_audit_outbox_v1
         where source_service = 'execution-service'
           and envelope #>> '{payload,execution_id}'
               = '92000000-0000-4000-8000-000000000001'
           and envelope ->> 'event_type' = 'execution.persisted.status_changed'
    ) then
        raise exception 'execution status update did not enqueue audit intent';
    end if;

    begin
        insert into public.executions (
            execution_id,
            invocation_id,
            status,
            trace_id,
            org_id
        ) values (
            '92000000-0000-4000-8000-000000000003',
            '90000000-0000-4000-8000-000000000001',
            'Queued',
            '91000000-0000-4000-8000-000000000001',
            '10000000-0000-4000-8000-000000000002'
        );
    exception
        when others then org_mismatch_rejected := true;
    end;
    if not org_mismatch_rejected then
        raise exception 'execution org mismatch was accepted';
    end if;
end
$test$;

do $test$
begin
    begin
        insert into public.executions (
            execution_id,
            invocation_id,
            status,
            trace_id,
            org_id
        ) values (
            '92000000-0000-4000-8000-000000000002',
            '90000000-0000-4000-8000-000000000001',
            'Queued',
            '91000000-0000-4000-8000-000000000001',
            '10000000-0000-4000-8000-000000000001'
        );
        raise exception 'forced execution rollback';
    exception
        when others then null;
    end;

    if exists (
        select 1
          from public.executions
         where execution_id = '92000000-0000-4000-8000-000000000002'
    ) or exists (
        select 1
          from public.cex_audit_outbox_v1
         where envelope #>> '{payload,execution_id}'
               = '92000000-0000-4000-8000-000000000002'
    ) then
        raise exception 'execution rollback left source row or audit intent';
    end if;
end
$test$;

insert into public.api_keys (
    api_key_id,
    org_id,
    key_hash,
    key_prefix,
    label,
    status
) values (
    'a0000000-0000-4000-8000-000000000001',
    '10000000-0000-4000-8000-000000000001',
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    'cex_p0_test',
    'P0 key',
    'active'
);

do $test$
declare
    before_count bigint;
    after_count bigint;
begin
    if not exists (
        select 1
          from public.api_keys
         where api_key_id = 'a0000000-0000-4000-8000-000000000001'
           and audit_revision = 1
    ) then
        raise exception 'identity API-key insert audit revision failed';
    end if;
    if not exists (
        select 1
          from public.cex_audit_outbox_v1
         where source_service = 'identity-service'
           and envelope #>> '{payload,api_key_id}'
               = 'a0000000-0000-4000-8000-000000000001'
           and envelope ->> 'event_type' = 'identity.api_key.persisted.issued'
           and not ((envelope -> 'payload') ? 'key_hash')
    ) then
        raise exception 'identity API-key issue intent missing or leaks key_hash';
    end if;

    select count(*)
      into before_count
      from public.cex_audit_outbox_v1
     where source_service = 'identity-service'
       and envelope #>> '{payload,api_key_id}'
           = 'a0000000-0000-4000-8000-000000000001';

    -- last_used_at-only updates are high-volume auth telemetry, not security mutations.
    update public.api_keys
       set last_used_at = clock_timestamp()
     where api_key_id = 'a0000000-0000-4000-8000-000000000001';

    select count(*)
      into after_count
      from public.cex_audit_outbox_v1
     where source_service = 'identity-service'
       and envelope #>> '{payload,api_key_id}'
           = 'a0000000-0000-4000-8000-000000000001';

    if before_count <> after_count then
        raise exception 'last_used_at-only update emitted an audit intent';
    end if;
    if not exists (
        select 1
          from public.api_keys
         where api_key_id = 'a0000000-0000-4000-8000-000000000001'
           and audit_revision = 1
    ) then
        raise exception 'last_used_at-only update changed audit revision';
    end if;

    perform set_config('cex.audit.actor_id', 'p0-identity-admin', true);
    perform set_config('cex.audit.actor_label', 'P0 Identity Admin', true);

    update public.api_keys
       set status = 'revoked',
           revoked_at = clock_timestamp(),
           revoked_reason = 'p0-test'
     where api_key_id = 'a0000000-0000-4000-8000-000000000001';

    if not exists (
        select 1
          from public.api_keys
         where api_key_id = 'a0000000-0000-4000-8000-000000000001'
           and audit_revision = 2
    ) then
        raise exception 'identity revoke did not increment audit revision';
    end if;
    if not exists (
        select 1
          from public.cex_audit_outbox_v1
         where source_service = 'identity-service'
           and envelope #>> '{payload,api_key_id}'
               = 'a0000000-0000-4000-8000-000000000001'
           and envelope ->> 'event_type' = 'identity.api_key.persisted.revoked'
           and envelope ->> 'actor_id' = 'p0-identity-admin'
    ) then
        raise exception 'identity revoke audit intent missing actor context';
    end if;
end
$test$;

do $test$
begin
    begin
        insert into public.api_keys (
            api_key_id,
            org_id,
            key_hash,
            key_prefix,
            status
        ) values (
            'a0000000-0000-4000-8000-000000000002',
            '10000000-0000-4000-8000-000000000001',
            'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
            'cex_p0_rollback',
            'active'
        );
        raise exception 'forced identity rollback';
    exception
        when others then null;
    end;

    if exists (
        select 1
          from public.api_keys
         where api_key_id = 'a0000000-0000-4000-8000-000000000002'
    ) or exists (
        select 1
          from public.cex_audit_outbox_v1
         where envelope #>> '{payload,api_key_id}'
               = 'a0000000-0000-4000-8000-000000000002'
    ) then
        raise exception 'identity rollback left source row or audit intent';
    end if;
end
$test$;

do $test$
begin
    if not exists (
        select 1
          from public.cex_audit_outbox_delivery_summary_v1
    ) then
        raise exception 'audit outbox delivery summary view is empty';
    end if;
end
$test$;

rollback;
SQL

echo "P0 PostgreSQL migration gate passed"
