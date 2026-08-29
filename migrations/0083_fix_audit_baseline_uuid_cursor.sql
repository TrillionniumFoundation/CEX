begin;

-- PostgreSQL does not define min/max aggregates for uuid. Replace the
-- 0064 backfill function so both fresh installs and 0082 -> 0083 upgrades use
-- a bounded, deterministic UUID cursor without casting away the source type.
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
        select
            count(*)::bigint,
            (array_agg(execution_id order by execution_id desc))[1]
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
        select
            count(*)::bigint,
            (array_agg(api_key_id order by api_key_id desc))[1]
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

commit;
