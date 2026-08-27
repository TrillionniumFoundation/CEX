#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
: "${DATABASE_URL:?DATABASE_URL is required}"

python3 "$root/scripts/check-p0-migrations.py"

psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 <<'SQL'
begin;

insert into public.organizations (org_id, name)
values
    ('b0000000-0000-4000-8000-000000000001', 'Audit baseline org'),
    ('b0000000-0000-4000-8000-000000000002', 'Audit baseline wrong org');

insert into public.users (user_id, org_id, email)
values (
    'b1000000-0000-4000-8000-000000000001',
    'b0000000-0000-4000-8000-000000000001',
    'audit-baseline@example.invalid'
);

insert into public.invocations (
    invocation_id,
    org_id,
    status,
    request_payload,
    trace_id,
    created_at,
    updated_at
) values (
    'b2000000-0000-4000-8000-000000000001',
    'b0000000-0000-4000-8000-000000000001',
    'Created',
    '{"purpose":"audit-baseline-test"}'::jsonb,
    'b3000000-0000-4000-8000-000000000001',
    clock_timestamp(),
    clock_timestamp()
);

-- Simulate rows that existed before the source-audit triggers were deployed.
alter table public.executions
    disable trigger trg_cex_prepare_execution_audit_revision_v1;
alter table public.executions
    disable trigger trg_cex_enqueue_execution_audit_v1;

insert into public.executions (
    execution_id,
    invocation_id,
    status,
    provider_target,
    trace_id,
    org_id,
    created_at,
    updated_at,
    audit_revision
) values
(
    'b4000000-0000-4000-8000-000000000001',
    'b2000000-0000-4000-8000-000000000001',
    'Queued',
    'baseline:test',
    'b3000000-0000-4000-8000-000000000001',
    'b0000000-0000-4000-8000-000000000001',
    clock_timestamp(),
    clock_timestamp(),
    0
),
(
    'b4000000-0000-4000-8000-000000000002',
    'b2000000-0000-4000-8000-000000000001',
    'Running',
    'baseline:test',
    'b3000000-0000-4000-8000-000000000002',
    'b0000000-0000-4000-8000-000000000001',
    clock_timestamp(),
    clock_timestamp(),
    0
);

alter table public.executions
    enable trigger trg_cex_prepare_execution_audit_revision_v1;
alter table public.executions
    enable trigger trg_cex_enqueue_execution_audit_v1;

alter table public.api_keys
    disable trigger trg_cex_prepare_api_key_audit_revision_v1;
alter table public.api_keys
    disable trigger trg_cex_enqueue_api_key_audit_v1;

insert into public.api_keys (
    api_key_id,
    org_id,
    user_id,
    key_hash,
    key_prefix,
    label,
    status,
    audit_revision
) values
(
    'b5000000-0000-4000-8000-000000000001',
    'b0000000-0000-4000-8000-000000000001',
    'b1000000-0000-4000-8000-000000000001',
    'sha256:audit-baseline-key-1',
    'cex_baseline_1',
    'Baseline key 1',
    'active',
    0
),
(
    'b5000000-0000-4000-8000-000000000002',
    'b0000000-0000-4000-8000-000000000001',
    'b1000000-0000-4000-8000-000000000001',
    'sha256:audit-baseline-key-2',
    'cex_baseline_2',
    'Baseline key 2',
    'revoked',
    0
);

alter table public.api_keys
    enable trigger trg_cex_prepare_api_key_audit_revision_v1;
alter table public.api_keys
    enable trigger trg_cex_enqueue_api_key_audit_v1;

do $test$
declare
    result jsonb;
    intent_count bigint;
begin
    result := public.cex_backfill_audit_source_baseline_v1(
        'execution-service',
        'p0-baseline-worker',
        1,
        10000
    );

    if result ->> 'status' <> 'pending'
       or (result ->> 'processed')::bigint <> 1
       or (result ->> 'remaining')::bigint <> 1 then
        raise exception 'execution baseline first bounded batch failed: %', result;
    end if;

    select count(*)::bigint
      into intent_count
      from public.cex_audit_outbox_v1
     where source_service = 'execution-service'
       and envelope ->> 'event_type' = 'execution.persisted.baseline';

    if intent_count <> 1 then
        raise exception 'execution baseline first batch emitted % intents', intent_count;
    end if;

    result := public.cex_backfill_audit_source_baseline_v1(
        'execution-service',
        'p0-baseline-worker',
        1,
        10000
    );

    if result ->> 'status' <> 'complete'
       or (result ->> 'processed')::bigint <> 1
       or (result ->> 'remaining')::bigint <> 0 then
        raise exception 'execution baseline completion batch failed: %', result;
    end if;

    result := public.cex_backfill_audit_source_baseline_v1(
        'execution-service',
        'p0-baseline-worker',
        1,
        10000
    );

    if result ->> 'status' <> 'complete'
       or (result ->> 'processed')::bigint <> 0 then
        raise exception 'execution baseline exact restart was not a no-op: %', result;
    end if;

    select count(*)::bigint
      into intent_count
      from public.cex_audit_outbox_v1
     where source_service = 'execution-service'
       and envelope ->> 'event_type' = 'execution.persisted.baseline';

    if intent_count <> 2 then
        raise exception 'execution baseline restart duplicated/lost intents: %', intent_count;
    end if;

    if exists (
        select 1
          from public.executions
         where execution_id in (
             'b4000000-0000-4000-8000-000000000001',
             'b4000000-0000-4000-8000-000000000002'
         )
           and audit_revision <> 1
    ) then
        raise exception 'execution baseline did not atomically advance revisions';
    end if;
end
$test$;

do $test$
declare
    result jsonb;
begin
    -- The two pending execution baseline intents exceed a deliberately tiny limit.
    result := public.cex_backfill_audit_source_baseline_v1(
        'identity-service',
        'p0-baseline-worker',
        1,
        1
    );

    if result ->> 'status' <> 'blocked'
       or (result ->> 'processed')::bigint <> 0 then
        raise exception 'identity baseline backlog guard failed: %', result;
    end if;

    if exists (
        select 1
          from public.api_keys
         where api_key_id in (
             'b5000000-0000-4000-8000-000000000001',
             'b5000000-0000-4000-8000-000000000002'
         )
           and audit_revision <> 0
    ) then
        raise exception 'blocked identity baseline mutated source rows';
    end if;

    result := public.cex_backfill_audit_source_baseline_v1(
        'identity-service',
        'p0-baseline-worker',
        1,
        10000
    );
    if result ->> 'status' <> 'pending'
       or (result ->> 'processed')::bigint <> 1 then
        raise exception 'identity baseline first batch failed: %', result;
    end if;

    result := public.cex_backfill_audit_source_baseline_v1(
        'identity-service',
        'p0-baseline-worker',
        1,
        10000
    );
    if result ->> 'status' <> 'complete'
       or (result ->> 'processed')::bigint <> 1
       or (result ->> 'remaining')::bigint <> 0 then
        raise exception 'identity baseline completion failed: %', result;
    end if;
end
$test$;

do $test$
declare
    identity_intents bigint;
begin
    select count(*)::bigint
      into identity_intents
      from public.cex_audit_outbox_v1
     where source_service = 'identity-service'
       and envelope ->> 'event_type' = 'identity.api_key.persisted.baseline';

    if identity_intents <> 2 then
        raise exception 'identity baseline intent count mismatch: %', identity_intents;
    end if;

    if exists (
        select 1
          from public.cex_audit_outbox_v1
         where source_service = 'identity-service'
           and envelope ->> 'event_type' = 'identity.api_key.persisted.baseline'
           and (
               (envelope -> 'payload') ? 'key_hash'
               or (envelope -> 'payload') ? 'raw_key'
           )
    ) then
        raise exception 'identity baseline leaked key material';
    end if;

    if exists (
        select 1
          from public.cex_audit_source_baseline_status_v1
         where source_service in ('execution-service', 'identity-service')
           and (
               status <> 'complete'
               or actual_remaining_count <> 0
               or baseline_intent_count <> 2
           )
    ) then
        raise exception 'durable baseline status view does not prove completion';
    end if;
end
$test$;

-- A failing source trigger must roll back revision, intent, and progress changes.
alter table public.executions
    disable trigger trg_cex_prepare_execution_audit_revision_v1;
alter table public.executions
    disable trigger trg_cex_enqueue_execution_audit_v1;

insert into public.executions (
    execution_id,
    invocation_id,
    status,
    trace_id,
    org_id,
    audit_revision
) values (
    'b4000000-0000-4000-8000-000000000003',
    'b2000000-0000-4000-8000-000000000001',
    'Queued',
    'b3000000-0000-4000-8000-000000000003',
    'b0000000-0000-4000-8000-000000000002',
    0
);

alter table public.executions
    enable trigger trg_cex_prepare_execution_audit_revision_v1;
alter table public.executions
    enable trigger trg_cex_enqueue_execution_audit_v1;

do $test$
declare
    before_processed bigint;
    after_processed bigint;
    failed_closed boolean := false;
begin
    select processed_count
      into before_processed
      from public.cex_audit_source_baseline_progress_v1
     where source_service = 'execution-service';

    begin
        perform public.cex_backfill_audit_source_baseline_v1(
            'execution-service',
            'p0-baseline-worker',
            10,
            10000
        );
    exception
        when others then failed_closed := true;
    end;

    if not failed_closed then
        raise exception 'org-mismatched execution baseline did not fail closed';
    end if;

    select processed_count
      into after_processed
      from public.cex_audit_source_baseline_progress_v1
     where source_service = 'execution-service';

    if after_processed <> before_processed then
        raise exception 'failed baseline batch advanced durable progress';
    end if;

    if not exists (
        select 1
          from public.executions
         where execution_id = 'b4000000-0000-4000-8000-000000000003'
           and audit_revision = 0
    ) then
        raise exception 'failed baseline batch advanced source revision';
    end if;

    if exists (
        select 1
          from public.cex_audit_outbox_v1
         where envelope #>> '{payload,execution_id}'
               = 'b4000000-0000-4000-8000-000000000003'
    ) then
        raise exception 'failed baseline batch left an outbox intent';
    end if;
end
$test$;

rollback;
SQL

echo "P0 Audit source baseline backfill gate passed"
