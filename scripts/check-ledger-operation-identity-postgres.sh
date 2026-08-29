#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
: "${DATABASE_URL:?DATABASE_URL is required}"

python3 "$root/scripts/check-p0-migrations.py"

psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 <<'SQL'
begin;

insert into public.organizations (org_id, name)
values ('c0000000-0000-4000-8000-000000000001', 'Ledger operation identity org');

select public.cex_open_account_v2(
    'c1000000-0000-4000-8000-000000000001'::uuid,
    'c0000000-0000-4000-8000-000000000001'::uuid,
    'c1100000-0000-4000-8000-000000000001'::uuid,
    'test',
    'credit',
    6::smallint,
    100000000::bigint,
    'org:c0000000:opening',
    'account-opening',
    'p0-ledger-operator'
);

do $test$
declare
    first_result jsonb;
    replay_result jsonb;
    second_scope_result jsonb;
    collision_rejected boolean := false;
    balance_after bigint;
    reserved_after bigint;
    effect_entry_count bigint;
    audit_intent_count bigint;
begin
    first_result := public.cex_apply_ledger_effect_v1(
        'c1000000-0000-4000-8000-000000000001'::uuid,
        'c2000000-0000-4000-8000-000000000001'::uuid,
        'c3000000-0000-4000-8000-000000000001'::uuid,
        'reserve',
        10000000::bigint,
        6::smallint,
        'invocation',
        'c4000000-0000-4000-8000-000000000001'::uuid,
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
        'c1000000-0000-4000-8000-000000000001'::uuid,
        'c2000000-0000-4000-8000-000000000001'::uuid,
        'c3000000-0000-4000-8000-000000000001'::uuid,
        'reserve',
        10000000::bigint,
        6::smallint,
        'invocation',
        'c4000000-0000-4000-8000-000000000001'::uuid,
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
            'c1000000-0000-4000-8000-000000000001'::uuid,
            'c2000000-0000-4000-8000-000000000001'::uuid,
            'c3000000-0000-4000-8000-000000000099'::uuid,
            'reserve',
            11000000::bigint,
            6::smallint,
            'invocation',
            'c4000000-0000-4000-8000-000000000001'::uuid,
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
        'c1000000-0000-4000-8000-000000000001'::uuid,
        'c2000000-0000-4000-8000-000000000002'::uuid,
        'c3000000-0000-4000-8000-000000000002'::uuid,
        'grant',
        5000000::bigint,
        6::smallint,
        null::text,
        null::uuid,
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
      into effect_entry_count
      from public.ledger_entries
     where account_id = 'c1000000-0000-4000-8000-000000000001'
       and provenance_mode = 'explicit'
       and operation_kind in ('reserve', 'grant');

    if effect_entry_count <> 2 then
        raise exception 'explicit business effect count mismatch: %', effect_entry_count;
    end if;

    select count(*)::bigint
      into audit_intent_count
      from public.cex_audit_outbox_v1
     where source_service = 'ledger-service'
       and envelope ->> 'event_type' = 'ledger.effect.persisted'
       and envelope #>> '{payload,account_id}'
           = 'c1000000-0000-4000-8000-000000000001'
       and envelope #>> '{payload,operation_kind}' in ('reserve', 'grant');

    if audit_intent_count <> 2 then
        raise exception 'ledger business-effect Audit intent count mismatch: %', audit_intent_count;
    end if;
end
$test$;

-- Exact-only cutover: direct compatibility value writes must fail closed.
do $test$
declare
    compatibility_rejected boolean := false;
    mutation_rejected boolean := false;
    delete_rejected boolean := false;
    status_row record;
    reserve_entry_id uuid;
begin
    begin
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
    exception
        when others then compatibility_rejected := true;
    end;

    if not compatibility_rejected then
        raise exception 'direct compatibility Ledger value write was not rejected';
    end if;
    if exists (
        select 1
          from public.ledger_entries
         where entry_id = 'c5000000-0000-4000-8000-000000000001'
    ) then
        raise exception 'rejected compatibility write left a Ledger row';
    end if;

    select entry_id
      into reserve_entry_id
      from public.ledger_entries
     where operation_id = 'c3000000-0000-4000-8000-000000000001';

    begin
        update public.ledger_entries
           set reason = 'tampered'
         where entry_id = reserve_entry_id;
    exception
        when others then mutation_rejected := true;
    end;
    if not mutation_rejected then
        raise exception 'ledger entry update was not rejected';
    end if;

    begin
        delete from public.ledger_entries
         where entry_id = reserve_entry_id;
    exception
        when others then delete_rejected := true;
    end;
    if not delete_rejected then
        raise exception 'ledger entry delete was not rejected';
    end if;

    select * into status_row
      from public.cex_ledger_operation_identity_status_v1;

    if status_row.missing_provenance_entries <> 0
       or status_row.explicit_entries < 3
       or status_row.compatibility_entries <> 0
       or status_row.legacy_entry_scoped_entries <> 0
       or status_row.distinct_operation_ids <> status_row.total_entries then
        raise exception 'ledger operation identity status is not internally consistent';
    end if;
end
$test$;

rollback;
SQL

echo "P0 Ledger operation identity gate passed"