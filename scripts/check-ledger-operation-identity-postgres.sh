#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
: "${DATABASE_URL:?DATABASE_URL is required}"

python3 "$root/scripts/check-p0-migrations.py"

psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 <<'SQL'
begin;

insert into public.organizations (org_id, name)
values ('c0000000-0000-4000-8000-000000000001', 'Ledger operation identity org');

insert into public.accounts (
    account_id,
    org_id,
    account_type,
    currency_unit,
    balance,
    reserved,
    status
) values (
    'c1000000-0000-4000-8000-000000000001',
    'c0000000-0000-4000-8000-000000000001',
    'test',
    'credit',
    100.000000,
    0.000000,
    'active'
);

do $test$
declare
    first_result jsonb;
    replay_result jsonb;
    second_scope_result jsonb;
    collision_rejected boolean := false;
    balance_after bigint;
    reserved_after bigint;
    entry_count bigint;
    audit_intent_count bigint;
begin
    first_result := public.cex_apply_ledger_effect_v1(
        'c1000000-0000-4000-8000-000000000001',
        'c2000000-0000-4000-8000-000000000001',
        'c3000000-0000-4000-8000-000000000001',
        'reserve',
        10000000,
        6,
        'invocation',
        'c4000000-0000-4000-8000-000000000001',
        'org:c0000000:reserve',
        'same-key',
        'ledger-service',
        'p0-ledger-operator',
        'explicit'
    );

    if first_result ->> 'replayed' <> 'false'
       or first_result #>> '{effect,operation_id}'
          <> 'c3000000-0000-4000-8000-000000000001'
       or first_result #>> '{effect,amount_minor}' <> '10000000' then
        raise exception 'first exact ledger effect failed: %', first_result;
    end if;

    replay_result := public.cex_apply_ledger_effect_v1(
        'c1000000-0000-4000-8000-000000000001',
        'c2000000-0000-4000-8000-000000000001',
        'c3000000-0000-4000-8000-000000000001',
        'reserve',
        10000000,
        6,
        'invocation',
        'c4000000-0000-4000-8000-000000000001',
        'org:c0000000:reserve',
        'same-key',
        'ledger-service',
        'p0-ledger-operator',
        'explicit'
    );

    if replay_result ->> 'replayed' <> 'true'
       or replay_result #>> '{effect,entry_id}'
          <> first_result #>> '{effect,entry_id}'
       or replay_result #>> '{effect,request_fingerprint}'
          <> first_result #>> '{effect,request_fingerprint}' then
        raise exception 'exact replay did not return original effect: %', replay_result;
    end if;

    select balance_minor, reserved_minor
      into balance_after, reserved_after
      from public.accounts
     where account_id = 'c1000000-0000-4000-8000-000000000001';

    if balance_after <> 100000000 or reserved_after <> 10000000 then
        raise exception 'exact replay double-applied account projection';
    end if;

    begin
        perform public.cex_apply_ledger_effect_v1(
            'c1000000-0000-4000-8000-000000000001',
            'c2000000-0000-4000-8000-000000000001',
            'c3000000-0000-4000-8000-000000000099',
            'reserve',
            11000000,
            6,
            'invocation',
            'c4000000-0000-4000-8000-000000000001',
            'org:c0000000:reserve',
            'same-key',
            'ledger-service',
            'p0-ledger-operator',
            'explicit'
        );
    exception
        when unique_violation then collision_rejected := true;
    end;

    if not collision_rejected then
        raise exception 'scoped idempotency collision was not rejected';
    end if;

    second_scope_result := public.cex_apply_ledger_effect_v1(
        'c1000000-0000-4000-8000-000000000001',
        'c2000000-0000-4000-8000-000000000002',
        'c3000000-0000-4000-8000-000000000002',
        'grant',
        5000000,
        6,
        null,
        null,
        'org:c0000000:grant',
        'same-key',
        'ledger-service',
        'p0-ledger-operator',
        'explicit'
    );

    if second_scope_result ->> 'replayed' <> 'false' then
        raise exception 'same key in a different scope was rejected';
    end if;

    select count(*)::bigint
      into entry_count
      from public.ledger_entries
     where account_id = 'c1000000-0000-4000-8000-000000000001'
       and provenance_mode = 'explicit';

    if entry_count <> 2 then
        raise exception 'explicit ledger effect count mismatch: %', entry_count;
    end if;

    select count(*)::bigint
      into audit_intent_count
      from public.cex_audit_outbox_v1
     where source_service = 'ledger-service'
       and envelope ->> 'event_type' = 'ledger.effect.persisted'
       and envelope #>> '{payload,account_id}'
           = 'c1000000-0000-4000-8000-000000000001';

    if audit_intent_count <> 2 then
        raise exception 'ledger effect Audit intent count mismatch: %', audit_intent_count;
    end if;
end
$test$;

-- Existing v1 writers remain operational during expand/cutover, but are explicitly labelled.
insert into public.ledger_entries (
    entry_id,
    account_id,
    direction,
    amount,
    reason,
    idempotency_key
) values (
    'c5000000-0000-4000-8000-000000000001',
    'c1000000-0000-4000-8000-000000000001',
    'credit',
    1.000000,
    'grant',
    'compatibility-key'
);

do $test$
declare
    compatibility_row public.ledger_entries%rowtype;
    mutation_rejected boolean := false;
    delete_rejected boolean := false;
    status_row record;
begin
    select *
      into compatibility_row
      from public.ledger_entries
     where entry_id = 'c5000000-0000-4000-8000-000000000001';

    if compatibility_row.provenance_mode <> 'operation_scoped_compatibility'
       or compatibility_row.trace_id is null
       or compatibility_row.operation_id is null
       or compatibility_row.request_fingerprint !~ '^sha256:[0-9a-f]{64}$' then
        raise exception 'compatibility writer did not receive explicit provenance labels';
    end if;

    begin
        update public.ledger_entries
           set reason = 'tampered'
         where entry_id = compatibility_row.entry_id;
    exception
        when others then mutation_rejected := true;
    end;
    if not mutation_rejected then
        raise exception 'ledger entry update was not rejected';
    end if;

    begin
        delete from public.ledger_entries
         where entry_id = compatibility_row.entry_id;
    exception
        when others then delete_rejected := true;
    end;
    if not delete_rejected then
        raise exception 'ledger entry delete was not rejected';
    end if;

    select * into status_row
      from public.cex_ledger_operation_identity_status_v1;

    if status_row.missing_provenance_entries <> 0
       or status_row.explicit_entries < 2
       or status_row.compatibility_entries < 1
       or status_row.distinct_operation_ids <> status_row.total_entries then
        raise exception 'ledger operation identity status is not internally consistent';
    end if;
end
$test$;

rollback;
SQL

echo "P0 Ledger operation identity gate passed"
