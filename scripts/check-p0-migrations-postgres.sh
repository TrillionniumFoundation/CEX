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

do $$
declare
    account_balance_minor bigint;
    account_reserved_minor bigint;
begin
    select balance_minor, reserved_minor
      into account_balance_minor, account_reserved_minor
      from public.accounts
     where account_id = '20000000-0000-4000-8000-000000000001';
    if account_balance_minor <> 10250000 or account_reserved_minor <> 1500000 then
        raise exception 'money v2 account backfill/trigger mismatch';
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
$$;

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

do $$
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
$$;

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

do $$
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
        select 1 from public.cex_saga_commands_v1
         where operation_key = 'p0-test:shadow' and status = 'pending'
    ) then
        raise exception 'shadow saga command was mutated by active claim';
    end if;
end
$$;

do $$
declare
    first_result jsonb;
    replay_result jsonb;
    second_result jsonb;
    first_hash text;
    second_previous_hash text;
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
    second_previous_hash := second_result #>> '{record,previous_event_hash}';
    if second_result #>> '{record,tenant_sequence}' <> '2'
       or second_previous_hash <> first_hash then
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
$$;

insert into public.cex_audit_outbox_v1 (
    outbox_id, event_id, source_service, org_id, trace_id, envelope
) values (
    '80000000-0000-4000-8000-000000000001',
    '90000000-0000-4000-8000-000000000001',
    'gateway-service',
    '10000000-0000-4000-8000-000000000001',
    '70000000-0000-4000-8000-000000000001',
    '{"schema":"cex.audit.outbox.v1"}'::jsonb
);

do $$
declare
    claimed_count bigint;
begin
    select count(*) into claimed_count
      from public.cex_claim_audit_outbox_v1('p0-audit-worker', 10, 30);
    if claimed_count <> 1 then
        raise exception 'audit outbox claim failed';
    end if;
    if not exists (
        select 1 from public.cex_audit_outbox_v1
         where outbox_id = '80000000-0000-4000-8000-000000000001'
           and status = 'claimed'
           and attempt_count = 1
           and claimed_by = 'p0-audit-worker'
    ) then
        raise exception 'audit outbox lease state mismatch';
    end if;
end
$$;

rollback;
SQL

echo "P0 PostgreSQL migration gate passed"
