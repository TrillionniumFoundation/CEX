begin;

create or replace function public.cex_claim_audit_outbox_v1(
    p_worker_id text,
    p_limit integer default 50,
    p_lease_seconds integer default 60
)
returns setof public.cex_audit_outbox_v1
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if p_worker_id is null or length(btrim(p_worker_id)) not between 1 and 128 then
        raise exception 'audit outbox worker_id must contain 1..128 characters';
    end if;
    if p_limit not between 1 and 200 then
        raise exception 'audit outbox claim limit must be between 1 and 200';
    end if;
    if p_lease_seconds not between 1 and 600 then
        raise exception 'audit outbox lease must be between 1 and 600 seconds';
    end if;

    update public.cex_audit_outbox_v1
       set status = 'dead_letter',
           claimed_by = null,
           lease_expires_at = null,
           last_error_code = 'lease_expired_after_final_attempt',
           last_error_message = 'claim lease expired after the final automatic attempt',
           dead_lettered_at = now(),
           updated_at = now()
     where status = 'claimed'
       and lease_expires_at <= now()
       and attempt_count >= max_attempts;

    return query
    with candidates as (
        select outbox_id
          from public.cex_audit_outbox_v1
         where attempt_count < max_attempts
           and available_at <= now()
           and (
               status in ('pending', 'retry_wait')
               or (status = 'claimed' and lease_expires_at <= now())
           )
         order by available_at, created_at, outbox_id
         for update skip locked
         limit p_limit
    )
    update public.cex_audit_outbox_v1 outbox
       set status = 'claimed',
           attempt_count = outbox.attempt_count + 1,
           claimed_by = p_worker_id,
           lease_expires_at = now() + make_interval(secs => p_lease_seconds),
           last_attempt_at = now(),
           updated_at = now()
      from candidates
     where outbox.outbox_id = candidates.outbox_id
    returning outbox.*;
end
$$;

create or replace function public.cex_mark_audit_outbox_delivered_v1(
    p_outbox_id uuid,
    p_worker_id text,
    p_event_id uuid,
    p_event_hash text,
    p_tenant_sequence bigint,
    p_receipt jsonb,
    p_http_status integer default 200
)
returns public.cex_audit_outbox_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    current_outbox public.cex_audit_outbox_v1%rowtype;
begin
    if p_worker_id is null or length(btrim(p_worker_id)) not between 1 and 128 then
        raise exception 'audit outbox worker_id must contain 1..128 characters';
    end if;
    if p_event_hash is null or p_event_hash !~ '^sha256:[0-9a-f]{64}$' then
        raise exception 'audit outbox ACK event_hash is invalid';
    end if;
    if p_tenant_sequence is null or p_tenant_sequence <= 0 then
        raise exception 'audit outbox ACK tenant_sequence must be positive';
    end if;
    if p_receipt is null or jsonb_typeof(p_receipt) <> 'object' then
        raise exception 'audit outbox ACK receipt must be a JSON object';
    end if;
    if p_http_status is null or p_http_status not between 200 and 299 then
        raise exception 'audit outbox ACK HTTP status must be 2xx';
    end if;

    select *
      into current_outbox
      from public.cex_audit_outbox_v1
     where outbox_id = p_outbox_id
     for update;

    if not found then
        raise exception 'audit outbox row not found';
    end if;
    if current_outbox.event_id is distinct from p_event_id then
        raise exception 'audit outbox ACK event_id mismatch';
    end if;

    if current_outbox.status = 'delivered' then
        if current_outbox.delivered_event_hash is distinct from p_event_hash
           or current_outbox.delivered_tenant_sequence is distinct from p_tenant_sequence
           or current_outbox.delivery_receipt -> 'record' is distinct from p_receipt -> 'record' then
            raise exception using
                errcode = '23505',
                message = 'audit outbox ACK collision with different delivery receipt';
        end if;
        return current_outbox;
    end if;

    if current_outbox.status <> 'claimed'
       or current_outbox.claimed_by is distinct from p_worker_id then
        raise exception 'audit outbox ACK requires ownership of an active claim';
    end if;
    if current_outbox.lease_expires_at is null
       or current_outbox.lease_expires_at <= now() then
        raise exception 'audit outbox ACK claim lease has expired';
    end if;

    update public.cex_audit_outbox_v1
       set status = 'delivered',
           claimed_by = null,
           lease_expires_at = null,
           last_http_status = p_http_status,
           last_error_code = null,
           last_error_message = null,
           delivered_at = now(),
           delivered_event_hash = p_event_hash,
           delivered_tenant_sequence = p_tenant_sequence,
           delivery_receipt = p_receipt,
           dead_lettered_at = null,
           updated_at = now()
     where outbox_id = p_outbox_id
    returning * into current_outbox;

    return current_outbox;
end
$$;

create or replace function public.cex_fail_audit_outbox_delivery_v1(
    p_outbox_id uuid,
    p_worker_id text,
    p_retryable boolean,
    p_error_code text,
    p_error_message text,
    p_http_status integer default null,
    p_retry_after_seconds integer default null
)
returns public.cex_audit_outbox_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    current_outbox public.cex_audit_outbox_v1%rowtype;
    retry_delay_seconds integer;
begin
    if p_worker_id is null or length(btrim(p_worker_id)) not between 1 and 128 then
        raise exception 'audit outbox worker_id must contain 1..128 characters';
    end if;
    if p_retryable is null then
        raise exception 'audit outbox retryable flag must be non-null';
    end if;
    if p_error_code is null or length(btrim(p_error_code)) not between 1 and 128 then
        raise exception 'audit outbox error_code must contain 1..128 characters';
    end if;
    if p_http_status is not null and p_http_status not between 100 and 599 then
        raise exception 'audit outbox failure HTTP status is invalid';
    end if;
    if p_retry_after_seconds is not null
       and p_retry_after_seconds not between 1 and 3600 then
        raise exception 'audit outbox retry delay must be between 1 and 3600 seconds';
    end if;

    select *
      into current_outbox
      from public.cex_audit_outbox_v1
     where outbox_id = p_outbox_id
     for update;

    if not found then
        raise exception 'audit outbox row not found';
    end if;
    if current_outbox.status <> 'claimed'
       or current_outbox.claimed_by is distinct from p_worker_id then
        raise exception 'audit outbox failure transition requires ownership of an active claim';
    end if;
    if current_outbox.lease_expires_at is null
       or current_outbox.lease_expires_at <= now() then
        raise exception 'audit outbox failure transition claim lease has expired';
    end if;

    if p_retryable and current_outbox.attempt_count < current_outbox.max_attempts then
        retry_delay_seconds := greatest(
            coalesce(p_retry_after_seconds, 0),
            least(
                3600,
                power(2, least(greatest(current_outbox.attempt_count - 1, 0), 10))::integer
            )
        );

        update public.cex_audit_outbox_v1
           set status = 'retry_wait',
               claimed_by = null,
               lease_expires_at = null,
               available_at = now() + make_interval(secs => retry_delay_seconds),
               last_http_status = p_http_status,
               last_error_code = btrim(p_error_code),
               last_error_message = left(coalesce(p_error_message, ''), 2000),
               delivered_at = null,
               delivered_event_hash = null,
               delivered_tenant_sequence = null,
               delivery_receipt = null,
               dead_lettered_at = null,
               updated_at = now()
         where outbox_id = p_outbox_id
        returning * into current_outbox;
    else
        update public.cex_audit_outbox_v1
           set status = 'dead_letter',
               claimed_by = null,
               lease_expires_at = null,
               last_http_status = p_http_status,
               last_error_code = btrim(p_error_code),
               last_error_message = left(coalesce(p_error_message, ''), 2000),
               delivered_at = null,
               delivered_event_hash = null,
               delivered_tenant_sequence = null,
               delivery_receipt = null,
               dead_lettered_at = now(),
               updated_at = now()
         where outbox_id = p_outbox_id
        returning * into current_outbox;
    end if;

    return current_outbox;
end
$$;

create index if not exists idx_cex_audit_outbox_dead_letter_v1
    on public.cex_audit_outbox_v1 (dead_lettered_at, source_service, outbox_id)
    where status = 'dead_letter';

create index if not exists idx_cex_audit_outbox_delivered_v1
    on public.cex_audit_outbox_v1 (delivered_at, source_service, outbox_id)
    where status = 'delivered';

create or replace view public.cex_audit_outbox_delivery_summary_v1 as
select
    source_service,
    status,
    count(*)::bigint as event_count,
    min(available_at) as oldest_available_at,
    min(last_attempt_at) as oldest_attempt_at,
    min(lease_expires_at) filter (where status = 'claimed') as oldest_lease_expiry,
    count(*) filter (where attempt_count >= max_attempts)::bigint as retry_budget_exhausted,
    count(*) filter (
        where status = 'delivered'
          and (
              delivery_receipt is null
              or delivered_event_hash is null
              or delivered_tenant_sequence is null
          )
    )::bigint as unverified_delivery_count
from public.cex_audit_outbox_v1
group by source_service, status;

update public.cex_audit_outbox_v1
   set status = 'dead_letter',
       claimed_by = null,
       lease_expires_at = null,
       last_error_code = 'legacy_envelope_unverified',
       last_error_message = 'pre-0060 pending envelope lacks a verified source-service marker',
       dead_lettered_at = now(),
       updated_at = now()
 where status in ('pending', 'retry_wait')
   and envelope #>> '{payload,_cex_audit_source_service}'
       is distinct from source_service;

update public.cex_audit_outbox_v1
   set dead_lettered_at = coalesce(dead_lettered_at, updated_at, now())
 where status = 'dead_letter'
   and dead_lettered_at is null;

alter table public.cex_audit_outbox_v1
    validate constraint cex_audit_outbox_dead_letter_time_v1;

-- Existing delivered rows predate verified receipts. The NOT VALID constraint
-- still protects all new/updated rows; migration evidence must explicitly
-- reconcile legacy delivered rows before a later migration validates it.

commit;
