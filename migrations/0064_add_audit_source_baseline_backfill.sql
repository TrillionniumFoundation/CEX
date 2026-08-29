begin;

create table if not exists public.cex_audit_source_baseline_progress_v1 (
    source_service text primary key,
    status text not null default 'pending',
    worker_id text,
    last_source_id uuid,
    processed_count bigint not null default 0,
    last_batch_count integer not null default 0,
    remaining_count bigint not null default 0,
    last_outbox_backlog bigint not null default 0,
    max_outbox_backlog bigint not null default 5000,
    started_at timestamptz,
    last_batch_at timestamptz,
    completed_at timestamptz,
    blocked_at timestamptz,
    last_error_code text,
    last_error_message text,
    updated_at timestamptz not null default now(),
    constraint cex_audit_source_baseline_source_v1
        check (source_service in ('execution-service', 'identity-service')),
    constraint cex_audit_source_baseline_status_v1
        check (status in ('pending', 'running', 'blocked', 'complete')),
    constraint cex_audit_source_baseline_worker_v1
        check (worker_id is null or length(btrim(worker_id)) between 1 and 128),
    constraint cex_audit_source_baseline_counts_v1
        check (
            processed_count >= 0
            and last_batch_count >= 0
            and remaining_count >= 0
            and last_outbox_backlog >= 0
            and max_outbox_backlog between 1 and 10000000
        ),
    constraint cex_audit_source_baseline_completion_v1
        check (
            (status = 'complete' and remaining_count = 0 and completed_at is not null)
            or
            (status <> 'complete' and completed_at is null)
        ),
    constraint cex_audit_source_baseline_blocked_v1
        check (
            (status = 'blocked' and blocked_at is not null)
            or
            (status <> 'blocked' and blocked_at is null)
        )
);

insert into public.cex_audit_source_baseline_progress_v1 (
    source_service,
    remaining_count
) values
(
    'execution-service',
    (select count(*)::bigint from public.executions where audit_revision = 0)
),
(
    'identity-service',
    (select count(*)::bigint from public.api_keys where audit_revision = 0)
)
on conflict (source_service) do nothing;

create or replace function public.cex_prepare_execution_audit_revision_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    baseline_mode boolean :=
        current_setting('cex.audit.baseline_backfill', true) = 'on';
begin
    if tg_op = 'INSERT' then
        new.audit_revision := 1;
        return new;
    end if;

    if baseline_mode
       and old.audit_revision = 0
       and new.audit_revision = 1 then
        return new;
    end if;

    if old.invocation_id is distinct from new.invocation_id
       or old.status is distinct from new.status
       or old.provider_target is distinct from new.provider_target
       or old.attempt_count is distinct from new.attempt_count
       or old.max_attempts is distinct from new.max_attempts
       or old.worker_id is distinct from new.worker_id
       or old.lease_expires_at is distinct from new.lease_expires_at
       or old.started_at is distinct from new.started_at
       or old.ended_at is distinct from new.ended_at
       or old.result_payload is distinct from new.result_payload
       or old.trace_id is distinct from new.trace_id
       or old.org_id is distinct from new.org_id
       or old.approval_required is distinct from new.approval_required
       or old.policy_reason is distinct from new.policy_reason
       or old.approved_by is distinct from new.approved_by then
        new.audit_revision := old.audit_revision + 1;
    else
        new.audit_revision := old.audit_revision;
    end if;

    return new;
end
$$;

create or replace function public.cex_enqueue_execution_audit_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    baseline_mode boolean :=
        current_setting('cex.audit.baseline_backfill', true) = 'on';
    invocation_org_id uuid;
    event_org_id uuid;
    event_type_value text;
    event_id_value uuid;
    actor_id_value text;
    result_payload_hash text;
    occurred_at_value timestamptz;
    envelope_value jsonb;
begin
    if tg_op = 'UPDATE' and new.audit_revision = old.audit_revision then
        return new;
    end if;

    if baseline_mode
       and not (
           tg_op = 'UPDATE'
           and old.audit_revision = 0
           and new.audit_revision = 1
       ) then
        raise exception 'execution baseline mode may only advance revision 0 to 1';
    end if;

    select org_id
      into invocation_org_id
      from public.invocations
     where invocation_id = new.invocation_id;

    if not found then
        raise exception 'execution audit enqueue cannot resolve invocation tenancy';
    end if;

    if new.org_id is not null and new.org_id is distinct from invocation_org_id then
        raise exception 'execution org_id differs from authoritative invocation org_id';
    end if;
    event_org_id := coalesce(new.org_id, invocation_org_id);

    event_type_value := case
        when baseline_mode then 'execution.persisted.baseline'
        when tg_op = 'INSERT' then 'execution.persisted.created'
        when old.status is distinct from new.status then 'execution.persisted.status_changed'
        when old.attempt_count is distinct from new.attempt_count
          or old.max_attempts is distinct from new.max_attempts
            then 'execution.persisted.attempt_changed'
        when old.worker_id is distinct from new.worker_id
          or old.lease_expires_at is distinct from new.lease_expires_at
            then 'execution.persisted.lease_changed'
        when old.result_payload is distinct from new.result_payload
          or old.ended_at is distinct from new.ended_at
            then 'execution.persisted.result_changed'
        when old.approval_required is distinct from new.approval_required
          or old.policy_reason is distinct from new.policy_reason
          or old.approved_by is distinct from new.approved_by
            then 'execution.persisted.approval_changed'
        else 'execution.persisted.changed'
    end;

    event_id_value := public.cex_deterministic_uuid_v1(
        'execution-service:executions:' ||
        new.execution_id::text || ':' ||
        new.audit_revision::text || ':' ||
        event_type_value
    );

    actor_id_value := coalesce(
        nullif(current_setting('cex.audit.actor_id', true), ''),
        nullif(btrim(new.worker_id), ''),
        nullif(btrim(new.approved_by), '')
    );

    result_payload_hash := case
        when new.result_payload is null then null
        else 'sha256:' || encode(digest(new.result_payload::text, 'sha256'), 'hex')
    end;

    occurred_at_value := case
        when baseline_mode then clock_timestamp()
        else coalesce(new.updated_at, new.created_at, clock_timestamp())
    end;

    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', new.trace_id,
        'org_id', event_org_id,
        'actor_type', 'execution-service',
        'actor_id', actor_id_value,
        'event_type', event_type_value,
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', occurred_at_value,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'execution-service',
            'execution_id', new.execution_id,
            'invocation_id', new.invocation_id,
            'audit_revision', new.audit_revision,
            'baseline', baseline_mode,
            'source_created_at', new.created_at,
            'previous_status', case when tg_op = 'INSERT' then null else old.status end,
            'status', new.status,
            'provider_target', new.provider_target,
            'attempt_count', new.attempt_count,
            'max_attempts', new.max_attempts,
            'worker_id', new.worker_id,
            'lease_expires_at', new.lease_expires_at,
            'started_at', new.started_at,
            'ended_at', new.ended_at,
            'result_payload_sha256', result_payload_hash,
            'approval_required', new.approval_required,
            'policy_reason', new.policy_reason,
            'approved_by', new.approved_by
        )
    );

    perform public.cex_enqueue_audit_outbox_v1(
        'execution-service',
        event_id_value,
        new.trace_id,
        event_org_id,
        envelope_value,
        10
    );

    return new;
end
$$;

create or replace function public.cex_prepare_api_key_audit_revision_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    baseline_mode boolean :=
        current_setting('cex.audit.baseline_backfill', true) = 'on';
begin
    if tg_op = 'INSERT' then
        new.audit_revision := 1;
        return new;
    end if;

    if baseline_mode
       and old.audit_revision = 0
       and new.audit_revision = 1 then
        return new;
    end if;

    if old.org_id is distinct from new.org_id
       or old.user_id is distinct from new.user_id
       or old.key_hash is distinct from new.key_hash
       or old.key_prefix is distinct from new.key_prefix
       or old.label is distinct from new.label
       or old.status is distinct from new.status
       or old.expires_at is distinct from new.expires_at
       or old.revoked_at is distinct from new.revoked_at
       or old.revoked_reason is distinct from new.revoked_reason then
        new.audit_revision := old.audit_revision + 1;
    else
        new.audit_revision := old.audit_revision;
    end if;

    return new;
end
$$;

create or replace function public.cex_enqueue_api_key_audit_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    baseline_mode boolean :=
        current_setting('cex.audit.baseline_backfill', true) = 'on';
    event_type_value text;
    event_id_value uuid;
    actor_id_value text;
    actor_label_value text;
    occurred_at_value timestamptz;
    envelope_value jsonb;
begin
    if tg_op = 'UPDATE' and new.audit_revision = old.audit_revision then
        return new;
    end if;

    if baseline_mode
       and not (
           tg_op = 'UPDATE'
           and old.audit_revision = 0
           and new.audit_revision = 1
       ) then
        raise exception 'identity baseline mode may only advance revision 0 to 1';
    end if;

    event_type_value := case
        when baseline_mode then 'identity.api_key.persisted.baseline'
        when tg_op = 'INSERT' then 'identity.api_key.persisted.issued'
        when new.status = 'revoked'
          and (old.status is distinct from new.status or old.revoked_at is distinct from new.revoked_at)
            then 'identity.api_key.persisted.revoked'
        when old.expires_at is distinct from new.expires_at
            then 'identity.api_key.persisted.expiry_changed'
        when old.key_hash is distinct from new.key_hash
          or old.key_prefix is distinct from new.key_prefix
            then 'identity.api_key.persisted.material_changed'
        else 'identity.api_key.persisted.changed'
    end;

    event_id_value := public.cex_deterministic_uuid_v1(
        'identity-service:api_keys:' ||
        new.api_key_id::text || ':' ||
        new.audit_revision::text || ':' ||
        event_type_value
    );

    actor_id_value := coalesce(
        nullif(current_setting('cex.audit.actor_id', true), ''),
        new.user_id::text
    );
    actor_label_value := nullif(
        current_setting('cex.audit.actor_label', true),
        ''
    );
    occurred_at_value := clock_timestamp();

    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', new.api_key_id,
        'org_id', new.org_id,
        'actor_type', 'identity-service',
        'actor_id', actor_id_value,
        'event_type', event_type_value,
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', occurred_at_value,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'identity-service',
            'api_key_id', new.api_key_id,
            'org_id', new.org_id,
            'user_id', new.user_id,
            'key_prefix', new.key_prefix,
            'label', new.label,
            'status', new.status,
            'expires_at', new.expires_at,
            'last_used_at', new.last_used_at,
            'revoked_at', new.revoked_at,
            'revoked_reason', new.revoked_reason,
            'audit_revision', new.audit_revision,
            'baseline', baseline_mode,
            'source_created_at', new.created_at,
            'previous_status', case when tg_op = 'INSERT' then null else old.status end,
            'key_material_changed', case
                when baseline_mode or tg_op = 'INSERT' then false
                else old.key_hash is distinct from new.key_hash
                  or old.key_prefix is distinct from new.key_prefix
            end,
            'admin_actor_label', actor_label_value
        )
    );

    perform public.cex_enqueue_audit_outbox_v1(
        'identity-service',
        event_id_value,
        new.api_key_id,
        new.org_id,
        envelope_value,
        10
    );

    return new;
end
$$;

create or replace function public.cex_backfill_audit_source_baseline_v1(
    p_source_service text,
    p_worker_id text,
    p_batch_size integer default 100,
    p_max_outbox_backlog bigint default 5000
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    acquired boolean;
    backlog_before bigint;
    backlog_after bigint;
    processed bigint := 0;
    remaining bigint := 0;
    last_id uuid;
    next_status text;
    previous_baseline_mode text := current_setting('cex.audit.baseline_backfill', true);
    previous_actor_id text := current_setting('cex.audit.actor_id', true);
    previous_actor_label text := current_setting('cex.audit.actor_label', true);
begin
    if p_source_service not in ('execution-service', 'identity-service') then
        raise exception 'unsupported Audit baseline source_service';
    end if;
    if p_worker_id is null or length(btrim(p_worker_id)) not between 1 and 128 then
        raise exception 'Audit baseline worker_id must contain 1..128 characters';
    end if;
    if p_batch_size not between 1 and 1000 then
        raise exception 'Audit baseline batch_size must be between 1 and 1000';
    end if;
    if p_max_outbox_backlog not between 1 and 10000000 then
        raise exception 'Audit baseline max_outbox_backlog must be between 1 and 10000000';
    end if;

    acquired := pg_try_advisory_xact_lock(
        hashtextextended('cex:audit-source-baseline:' || p_source_service, 0)
    );
    if not acquired then
        return jsonb_build_object(
            'source_service', p_source_service,
            'status', 'busy',
            'processed', 0
        );
    end if;

    insert into public.cex_audit_source_baseline_progress_v1 (
        source_service,
        max_outbox_backlog
    ) values (
        p_source_service,
        p_max_outbox_backlog
    )
    on conflict (source_service) do nothing;

    select count(*)::bigint
      into backlog_before
      from public.cex_audit_outbox_v1
     where status in ('pending', 'claimed', 'retry_wait');

    if p_source_service = 'execution-service' then
        select count(*)::bigint
          into remaining
          from public.executions
         where audit_revision = 0;
    else
        select count(*)::bigint
          into remaining
          from public.api_keys
         where audit_revision = 0;
    end if;

    if backlog_before >= p_max_outbox_backlog and remaining > 0 then
        update public.cex_audit_source_baseline_progress_v1
           set status = 'blocked',
               worker_id = btrim(p_worker_id),
               last_batch_count = 0,
               remaining_count = remaining,
               last_outbox_backlog = backlog_before,
               max_outbox_backlog = p_max_outbox_backlog,
               blocked_at = now(),
               completed_at = null,
               last_error_code = 'outbox_backlog_limit',
               last_error_message = format(
                   'nonterminal Audit outbox backlog %s reached limit %s',
                   backlog_before,
                   p_max_outbox_backlog
               ),
               updated_at = now()
         where source_service = p_source_service;

        return jsonb_build_object(
            'source_service', p_source_service,
            'status', 'blocked',
            'processed', 0,
            'remaining', remaining,
            'outbox_backlog', backlog_before,
            'max_outbox_backlog', p_max_outbox_backlog
        );
    end if;

    update public.cex_audit_source_baseline_progress_v1
       set status = 'running',
           worker_id = btrim(p_worker_id),
           started_at = coalesce(started_at, now()),
           blocked_at = null,
           completed_at = null,
           last_error_code = null,
           last_error_message = null,
           max_outbox_backlog = p_max_outbox_backlog,
           last_outbox_backlog = backlog_before,
           updated_at = now()
     where source_service = p_source_service;

    perform set_config('cex.audit.baseline_backfill', 'on', true);
    perform set_config(
        'cex.audit.actor_id',
        left('audit-baseline-backfill:' || btrim(p_worker_id), 256),
        true
    );
    perform set_config(
        'cex.audit.actor_label',
        'CEX Audit source baseline backfill',
        true
    );

    if p_source_service = 'execution-service' then
        with candidates as (
            select execution_id
              from public.executions
             where audit_revision = 0
             order by execution_id
             for update skip locked
             limit p_batch_size
        ),
        updated as (
            update public.executions execution
               set audit_revision = 1
              from candidates
             where execution.execution_id = candidates.execution_id
            returning execution.execution_id
        )
        select count(*)::bigint, max(execution_id)
          into processed, last_id
          from updated;

        select count(*)::bigint
          into remaining
          from public.executions
         where audit_revision = 0;
    else
        with candidates as (
            select api_key_id
              from public.api_keys
             where audit_revision = 0
             order by api_key_id
             for update skip locked
             limit p_batch_size
        ),
        updated as (
            update public.api_keys api_key
               set audit_revision = 1
              from candidates
             where api_key.api_key_id = candidates.api_key_id
            returning api_key.api_key_id
        )
        select count(*)::bigint, max(api_key_id)
          into processed, last_id
          from updated;

        select count(*)::bigint
          into remaining
          from public.api_keys
         where audit_revision = 0;
    end if;

    select count(*)::bigint
      into backlog_after
      from public.cex_audit_outbox_v1
     where status in ('pending', 'claimed', 'retry_wait');

    next_status := case when remaining = 0 then 'complete' else 'pending' end;

    perform set_config(
        'cex.audit.baseline_backfill',
        coalesce(previous_baseline_mode, ''),
        true
    );
    perform set_config(
        'cex.audit.actor_id',
        coalesce(previous_actor_id, ''),
        true
    );
    perform set_config(
        'cex.audit.actor_label',
        coalesce(previous_actor_label, ''),
        true
    );

    update public.cex_audit_source_baseline_progress_v1
       set status = next_status,
           worker_id = btrim(p_worker_id),
           last_source_id = coalesce(last_id, last_source_id),
           processed_count = processed_count + processed,
           last_batch_count = processed::integer,
           remaining_count = remaining,
           last_outbox_backlog = backlog_after,
           max_outbox_backlog = p_max_outbox_backlog,
           last_batch_at = now(),
           completed_at = case when remaining = 0 then now() else null end,
           blocked_at = null,
           last_error_code = null,
           last_error_message = null,
           updated_at = now()
     where source_service = p_source_service;

    return jsonb_build_object(
        'source_service', p_source_service,
        'status', next_status,
        'processed', processed,
        'remaining', remaining,
        'last_source_id', last_id,
        'outbox_backlog_before', backlog_before,
        'outbox_backlog_after', backlog_after,
        'max_outbox_backlog', p_max_outbox_backlog
    );
end
$$;

create or replace view public.cex_audit_source_baseline_status_v1 as
select
    progress.source_service,
    progress.status,
    progress.worker_id,
    progress.last_source_id,
    progress.processed_count,
    progress.last_batch_count,
    progress.last_outbox_backlog,
    progress.max_outbox_backlog,
    progress.started_at,
    progress.last_batch_at,
    progress.completed_at,
    progress.blocked_at,
    progress.last_error_code,
    progress.last_error_message,
    case
        when progress.source_service = 'execution-service'
            then (select count(*)::bigint from public.executions where audit_revision = 0)
        else (select count(*)::bigint from public.api_keys where audit_revision = 0)
    end as actual_remaining_count,
    (
        select count(*)::bigint
          from public.cex_audit_outbox_v1 outbox
         where outbox.source_service = progress.source_service
           and outbox.envelope ->> 'event_type' in (
               'execution.persisted.baseline',
               'identity.api_key.persisted.baseline'
           )
    ) as baseline_intent_count,
    progress.updated_at
from public.cex_audit_source_baseline_progress_v1 progress;

create index if not exists idx_cex_audit_source_baseline_status_v1
    on public.cex_audit_source_baseline_progress_v1 (status, updated_at);

commit;
