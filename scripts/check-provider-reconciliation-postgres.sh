#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
source "$root/scripts/_dev-helpers.sh"
: "${DATABASE_URL:?DATABASE_URL is required}"
cex_load_env
cex_sync_postgres_env_from_database_url "$DATABASE_URL"

python3 "$root/scripts/check-p0-migrations.py"

cex_psql_stdin -X <<'SQL'
begin;

insert into public.organizations (org_id, name)
values (
    '84000000-0000-4000-8000-000000000001',
    'Provider reconciliation lifecycle org'
);

-- Origin-mode authority probe: value-bearing provider dispatch cannot exist
-- without verified active exact reserve evidence.
insert into public.invocations (
    invocation_id, org_id, status, request_payload, trace_id, created_at, updated_at
) values (
    '84000000-0000-4000-8000-000000000801',
    '84000000-0000-4000-8000-000000000001',
    'Created',
    '{"prompt":"authority probe","amount":1}'::jsonb,
    '84000000-0000-4000-8000-000000000701',
    clock_timestamp(), clock_timestamp()
);
insert into public.executions (
    execution_id, invocation_id, status, provider_target, trace_id, org_id,
    created_at, updated_at
) values (
    '84000000-0000-4000-8000-000000000901',
    '84000000-0000-4000-8000-000000000801',
    'Approved',
    'ollama://authority-probe',
    '84000000-0000-4000-8000-000000000701',
    '84000000-0000-4000-8000-000000000001',
    clock_timestamp(), clock_timestamp()
);

do $test$
declare
    authority_rejected boolean := false;
begin
    begin
        insert into public.cex_provider_dispatch_commands_v1 (
            command_id, execution_id, invocation_id, org_id, trace_id,
            provider_target, prompt_sha256, request_fingerprint, max_attempts
        ) values (
            '84000000-0000-4000-8000-000000000999',
            '84000000-0000-4000-8000-000000000901',
            '84000000-0000-4000-8000-000000000801',
            '84000000-0000-4000-8000-000000000001',
            '84000000-0000-4000-8000-000000000701',
            'ollama://authority-probe',
            'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
            'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
            2
        );
    exception
        when others then
            if position(
                'value-bearing provider dispatch requires an exact Invocation Ledger contract'
                in sqlerrm
            ) = 0 then
                raise;
            end if;
            authority_rejected := true;
    end;

    if not authority_rejected then
        raise exception 'value-bearing provider dispatch bypassed exact authority';
    end if;
    if exists (
        select 1 from public.cex_provider_dispatch_commands_v1
         where command_id='84000000-0000-4000-8000-000000000999'
    ) then
        raise exception 'rejected provider authority probe left a command row';
    end if;
end
$test$;

-- Durable lifecycle fixtures use real Invocation/Execution rows. Command INSERT
-- validation is bypassed only to isolate claim, lease, immutable evidence,
-- exact replay and operator-recovery behavior already protected by 0081.
insert into public.invocations (
    invocation_id, org_id, status, request_payload, trace_id, created_at, updated_at
) values
(
    '84000000-0000-4000-8000-000000000810',
    '84000000-0000-4000-8000-000000000001',
    'Dispatching',
    '{"prompt":"indeterminate lifecycle"}'::jsonb,
    '84000000-0000-4000-8000-000000000710',
    clock_timestamp(), clock_timestamp()
),
(
    '84000000-0000-4000-8000-000000000811',
    '84000000-0000-4000-8000-000000000001',
    'Dispatching',
    '{"prompt":"confirmed execution lifecycle"}'::jsonb,
    '84000000-0000-4000-8000-000000000711',
    clock_timestamp(), clock_timestamp()
),
(
    '84000000-0000-4000-8000-000000000812',
    '84000000-0000-4000-8000-000000000001',
    'Dispatching',
    '{"prompt":"confirmed non-execution lifecycle"}'::jsonb,
    '84000000-0000-4000-8000-000000000712',
    clock_timestamp(), clock_timestamp()
);

insert into public.executions (
    execution_id, invocation_id, status, provider_target, trace_id, org_id,
    created_at, updated_at
) values
(
    '84000000-0000-4000-8000-000000000910',
    '84000000-0000-4000-8000-000000000810',
    'Dispatching',
    'ollama://indeterminate',
    '84000000-0000-4000-8000-000000000710',
    '84000000-0000-4000-8000-000000000001',
    clock_timestamp(), clock_timestamp()
),
(
    '84000000-0000-4000-8000-000000000911',
    '84000000-0000-4000-8000-000000000811',
    'Dispatching',
    'ollama://confirmed-executed',
    '84000000-0000-4000-8000-000000000711',
    '84000000-0000-4000-8000-000000000001',
    clock_timestamp(), clock_timestamp()
),
(
    '84000000-0000-4000-8000-000000000912',
    '84000000-0000-4000-8000-000000000812',
    'Dispatching',
    'ollama://confirmed-not-executed',
    '84000000-0000-4000-8000-000000000712',
    '84000000-0000-4000-8000-000000000001',
    clock_timestamp(), clock_timestamp()
);

set local session_replication_role = replica;
insert into public.cex_provider_dispatch_commands_v1 (
    command_id, execution_id, invocation_id, org_id, trace_id,
    provider_target, prompt_sha256, request_fingerprint,
    status, attempt_count, max_attempts, available_at
) values
(
    '84000000-0000-4000-8000-000000000010',
    '84000000-0000-4000-8000-000000000910',
    '84000000-0000-4000-8000-000000000810',
    '84000000-0000-4000-8000-000000000001',
    '84000000-0000-4000-8000-000000000710',
    'ollama://indeterminate',
    'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    'sha256:baaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    'pending', 0, 2, now()
),
(
    '84000000-0000-4000-8000-000000000011',
    '84000000-0000-4000-8000-000000000911',
    '84000000-0000-4000-8000-000000000811',
    '84000000-0000-4000-8000-000000000001',
    '84000000-0000-4000-8000-000000000711',
    'ollama://confirmed-executed',
    'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
    'sha256:cbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
    'pending', 0, 2, now()
),
(
    '84000000-0000-4000-8000-000000000012',
    '84000000-0000-4000-8000-000000000912',
    '84000000-0000-4000-8000-000000000812',
    '84000000-0000-4000-8000-000000000001',
    '84000000-0000-4000-8000-000000000712',
    'ollama://confirmed-not-executed',
    'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
    'sha256:dccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
    'pending', 0, 2, now()
);
set local session_replication_role = origin;

-- Indeterminate evidence never authorizes a retry, even after acknowledgement.
do $test$
declare
    command_row public.cex_provider_dispatch_commands_v1%rowtype;
    first_result jsonb;
    replay_result jsonb;
    collision_rejected boolean := false;
    requeue_rejected boolean := false;
    update_rejected boolean := false;
    delete_rejected boolean := false;
    evidence_id_value uuid;
begin
    select * into command_row
      from public.cex_claim_provider_dispatch_v1('provider-worker-a',1,10);
    if command_row.command_id is distinct from '84000000-0000-4000-8000-000000000010'
       or command_row.status <> 'claimed'
       or command_row.attempt_count <> 1 then
        raise exception 'indeterminate provider command claim failed';
    end if;

    update public.cex_provider_dispatch_commands_v1
       set lease_expires_at=now()-interval '1 second'
     where command_id=command_row.command_id;
    if public.cex_reconcile_expired_provider_claims_v1() <> 1 then
        raise exception 'expired provider claim was not reconciled exactly once';
    end if;

    select * into command_row
      from public.cex_provider_dispatch_commands_v1
     where command_id=command_row.command_id;
    if command_row.status <> 'reconcile_required'
       or command_row.last_error_code <> 'provider_lease_expired_unknown_outcome' then
        raise exception 'expired provider claim did not become an unknown outcome';
    end if;

    first_result := public.cex_record_provider_reconciliation_v1(
        command_row.command_id,
        'provider-operator-a',
        'indeterminate',
        'gh://provider-reconciliation/indeterminate/attempt-1',
        'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
        '{"probe":"indeterminate"}'::jsonb,
        null::jsonb
    );
    replay_result := public.cex_record_provider_reconciliation_v1(
        command_row.command_id,
        'provider-operator-a',
        'indeterminate',
        'gh://provider-reconciliation/indeterminate/attempt-1',
        'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
        '{"probe":"indeterminate"}'::jsonb,
        null::jsonb
    );
    if first_result ->> 'replayed' <> 'false'
       or replay_result ->> 'replayed' <> 'true'
       or first_result #>> '{evidence,evidence_id}'
          is distinct from replay_result #>> '{evidence,evidence_id}' then
        raise exception 'indeterminate reconciliation exact replay failed';
    end if;

    begin
        perform public.cex_record_provider_reconciliation_v1(
            command_row.command_id,
            'provider-operator-a',
            'indeterminate',
            'gh://provider-reconciliation/indeterminate/attempt-1',
            'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
            '{"probe":"different"}'::jsonb,
            null::jsonb
        );
    exception when unique_violation then
        collision_rejected := true;
    end;
    if not collision_rejected then
        raise exception 'provider reconciliation evidence collision was not rejected';
    end if;

    perform public.cex_acknowledge_provider_dispatch_v1(
        command_row.command_id,
        'provider-operator-a',
        'indeterminate evidence reviewed'
    );
    begin
        perform public.cex_requeue_provider_dispatch_v1(
            command_row.command_id,
            'provider-operator-a',
            'indeterminate evidence must not authorize replay',
            1
        );
    exception when others then
        if position('requires confirmed-not-executed evidence' in sqlerrm)=0 then
            raise;
        end if;
        requeue_rejected := true;
    end;
    if not requeue_rejected then
        raise exception 'indeterminate evidence authorized a provider retry';
    end if;

    evidence_id_value := (first_result #>> '{evidence,evidence_id}')::uuid;
    begin
        update public.cex_provider_reconciliation_evidence_v1
           set evidence='{"tampered":true}'::jsonb
         where evidence_id=evidence_id_value;
    exception when others then update_rejected := true;
    end;
    begin
        delete from public.cex_provider_reconciliation_evidence_v1
         where evidence_id=evidence_id_value;
    exception when others then delete_rejected := true;
    end;
    if not update_rejected or not delete_rejected then
        raise exception 'provider reconciliation evidence is not append-only';
    end if;
end
$test$;

-- Confirmed execution closes the command once and remains exactly replayable
-- after the command has become terminal.
do $test$
declare
    command_row public.cex_provider_dispatch_commands_v1%rowtype;
    first_result jsonb;
    replay_result jsonb;
    reconciliation_audit_count bigint;
begin
    select * into command_row
      from public.cex_claim_provider_dispatch_v1('provider-worker-b',1,10);
    if command_row.command_id is distinct from '84000000-0000-4000-8000-000000000011' then
        raise exception 'confirmed-executed provider command claim failed';
    end if;
    update public.cex_provider_dispatch_commands_v1
       set lease_expires_at=now()-interval '1 second'
     where command_id=command_row.command_id;
    perform public.cex_reconcile_expired_provider_claims_v1();

    first_result := public.cex_record_provider_reconciliation_v1(
        command_row.command_id,
        'provider-operator-b',
        'confirmed_executed',
        'gh://provider-reconciliation/confirmed-executed/attempt-1',
        'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
        '{"provider_receipt":"receipt-1"}'::jsonb,
        '{"answer":"confirmed"}'::jsonb
    );
    replay_result := public.cex_record_provider_reconciliation_v1(
        command_row.command_id,
        'provider-operator-b',
        'confirmed_executed',
        'gh://provider-reconciliation/confirmed-executed/attempt-1',
        'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
        '{"provider_receipt":"receipt-1"}'::jsonb,
        '{"answer":"confirmed"}'::jsonb
    );

    if first_result ->> 'replayed' <> 'false'
       or replay_result ->> 'replayed' <> 'true'
       or first_result #>> '{command,status}' <> 'succeeded'
       or replay_result #>> '{command,status}' <> 'succeeded'
       or first_result #>> '{command,result_sha256}' is null then
        raise exception 'terminal confirmed-executed reconciliation replay failed';
    end if;
    if not exists (
        select 1 from public.executions
         where execution_id='84000000-0000-4000-8000-000000000911'
           and status='Succeeded'
           and result_payload='{"answer":"confirmed"}'::jsonb
    ) or not exists (
        select 1 from public.invocations
         where invocation_id='84000000-0000-4000-8000-000000000811'
           and status='Succeeded'
           and execution_id='84000000-0000-4000-8000-000000000911'
    ) then
        raise exception 'confirmed provider execution did not close Execution/Invocation';
    end if;

    select count(*)::bigint into reconciliation_audit_count
      from public.cex_audit_outbox_v1
     where source_service='execution-service'
       and envelope ->> 'event_type'='execution.provider_dispatch.reconciled'
       and envelope #>> '{payload,command_id}'=command_row.command_id::text;
    if reconciliation_audit_count <> 1 then
        raise exception 'confirmed execution reconciliation Audit intent count mismatch';
    end if;
end
$test$;

-- Only fresh confirmed-not-executed evidence plus acknowledgement may requeue.
do $test$
declare
    command_row public.cex_provider_dispatch_commands_v1%rowtype;
    first_result jsonb;
    replay_result jsonb;
    missing_ack_rejected boolean := false;
begin
    select * into command_row
      from public.cex_claim_provider_dispatch_v1('provider-worker-c',1,10);
    if command_row.command_id is distinct from '84000000-0000-4000-8000-000000000012' then
        raise exception 'confirmed-not-executed provider command claim failed';
    end if;
    update public.cex_provider_dispatch_commands_v1
       set lease_expires_at=now()-interval '1 second'
     where command_id=command_row.command_id;
    perform public.cex_reconcile_expired_provider_claims_v1();

    first_result := public.cex_record_provider_reconciliation_v1(
        command_row.command_id,
        'provider-operator-c',
        'confirmed_not_executed',
        'gh://provider-reconciliation/confirmed-not-executed/attempt-1',
        'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
        '{"provider_receipt":"not-executed-1"}'::jsonb,
        null::jsonb
    );
    replay_result := public.cex_record_provider_reconciliation_v1(
        command_row.command_id,
        'provider-operator-c',
        'confirmed_not_executed',
        'gh://provider-reconciliation/confirmed-not-executed/attempt-1',
        'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
        '{"provider_receipt":"not-executed-1"}'::jsonb,
        null::jsonb
    );
    if first_result ->> 'replayed' <> 'false'
       or replay_result ->> 'replayed' <> 'true' then
        raise exception 'confirmed-not-executed exact replay failed';
    end if;

    begin
        perform public.cex_requeue_provider_dispatch_v1(
            command_row.command_id,
            'provider-operator-c',
            'acknowledgement intentionally missing',
            1
        );
    exception when others then
        if position('requires a fresh acknowledgement' in sqlerrm)=0 then
            raise;
        end if;
        missing_ack_rejected := true;
    end;
    if not missing_ack_rejected then
        raise exception 'provider retry did not require acknowledgement';
    end if;

    perform public.cex_acknowledge_provider_dispatch_v1(
        command_row.command_id,
        'provider-operator-c',
        'confirmed non-execution reviewed'
    );
    perform public.cex_requeue_provider_dispatch_v1(
        command_row.command_id,
        'provider-operator-c',
        'one bounded provider replay approved',
        1
    );

    select * into command_row
      from public.cex_provider_dispatch_commands_v1
     where command_id=command_row.command_id;
    if command_row.status <> 'pending'
       or command_row.max_attempts <> 3
       or command_row.requeue_count <> 1
       or command_row.acknowledged_at is not null then
        raise exception 'confirmed-not-executed provider requeue evidence is invalid';
    end if;
    if not exists (
        select 1 from public.executions
         where execution_id='84000000-0000-4000-8000-000000000912'
           and status='Queued'
    ) or not exists (
        select 1 from public.invocations
         where invocation_id='84000000-0000-4000-8000-000000000812'
           and status='Queued'
    ) then
        raise exception 'provider requeue did not restore Execution/Invocation queue state';
    end if;

    replay_result := public.cex_record_provider_reconciliation_v1(
        command_row.command_id,
        'provider-operator-c',
        'confirmed_not_executed',
        'gh://provider-reconciliation/confirmed-not-executed/attempt-1',
        'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
        '{"provider_receipt":"not-executed-1"}'::jsonb,
        null::jsonb
    );
    if replay_result ->> 'replayed' <> 'true'
       or replay_result #>> '{command,status}' <> 'pending' then
        raise exception 'post-requeue reconciliation replay changed command state';
    end if;
end
$test$;

do $test$
declare
    reconcile_row record;
    transition_count bigint;
begin
    select * into reconcile_row
      from public.cex_provider_dispatch_status_v1
     where status='reconcile_required';
    if reconcile_row.command_count <> 1
       or reconcile_row.missing_reconciliation_evidence_count <> 0
       or reconcile_row.unacknowledged_count <> 0 then
        raise exception 'provider reconciliation status view is inconsistent';
    end if;

    if not exists (
        select 1 from public.cex_provider_reconciliation_backlog_v1
         where command_id='84000000-0000-4000-8000-000000000010'
           and disposition='indeterminate'
           and acknowledged_by='provider-operator-a'
    ) then
        raise exception 'provider reconciliation backlog omits indeterminate incident';
    end if;

    select count(*)::bigint into transition_count
      from public.cex_provider_dispatch_transitions_v1
     where command_id in (
         '84000000-0000-4000-8000-000000000010',
         '84000000-0000-4000-8000-000000000011',
         '84000000-0000-4000-8000-000000000012'
     )
       and to_status in ('claimed','reconcile_required','succeeded','pending');
    if transition_count < 8 then
        raise exception 'provider dispatch transition evidence is incomplete: %', transition_count;
    end if;
end
$test$;

rollback;
SQL

echo "P0 provider unknown-outcome reconciliation gate passed"
