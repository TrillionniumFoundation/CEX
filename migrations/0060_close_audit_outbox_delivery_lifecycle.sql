begin;

alter table public.cex_audit_outbox_v1
    add column if not exists last_http_status integer,
    add column if not exists last_attempt_at timestamptz,
    add column if not exists delivered_event_hash text,
    add column if not exists delivered_tenant_sequence bigint,
    add column if not exists delivery_receipt jsonb,
    add column if not exists dead_lettered_at timestamptz;

alter table public.cex_audit_outbox_v1
    drop constraint if exists cex_audit_outbox_http_status_v1,
    add constraint cex_audit_outbox_http_status_v1
        check (last_http_status is null or last_http_status between 100 and 599),
    drop constraint if exists cex_audit_outbox_delivered_hash_v1,
    add constraint cex_audit_outbox_delivered_hash_v1
        check (
            delivered_event_hash is null
            or delivered_event_hash ~ '^sha256:[0-9a-f]{64}$'
        ),
    drop constraint if exists cex_audit_outbox_delivered_sequence_v1,
    add constraint cex_audit_outbox_delivered_sequence_v1
        check (
            delivered_tenant_sequence is null
            or delivered_tenant_sequence > 0
        ),
    drop constraint if exists cex_audit_outbox_receipt_v1,
    add constraint cex_audit_outbox_receipt_v1
        check (
            delivery_receipt is null
            or jsonb_typeof(delivery_receipt) = 'object'
        ),
    drop constraint if exists cex_audit_outbox_dead_letter_time_v1,
    add constraint cex_audit_outbox_dead_letter_time_v1
        check (
            (status = 'dead_letter' and dead_lettered_at is not null)
            or
            (status <> 'dead_letter' and dead_lettered_at is null)
        ) not valid,
    drop constraint if exists cex_audit_outbox_verified_delivery_v1,
    add constraint cex_audit_outbox_verified_delivery_v1
        check (
            (
                status = 'delivered'
                and delivered_at is not null
                and delivered_event_hash is not null
                and delivered_tenant_sequence is not null
                and delivery_receipt is not null
            )
            or
            (
                status <> 'delivered'
                and delivered_at is null
                and delivered_event_hash is null
                and delivered_tenant_sequence is null
                and delivery_receipt is null
            )
        ) not valid;

update public.cex_audit_outbox_v1
   set dead_lettered_at = coalesce(dead_lettered_at, updated_at, now())
 where status = 'dead_letter'
   and dead_lettered_at is null;

alter table public.cex_audit_outbox_v1
    validate constraint cex_audit_outbox_dead_letter_time_v1;

create or replace function public.cex_deterministic_uuid_v1(p_material text)
returns uuid
language sql
immutable
strict
set search_path = pg_catalog, public
as $$
    with raw as (
        select encode(digest(p_material, 'sha256'), 'hex') as hex
    ), shaped as (
        select overlay(overlay(hex placing '4' from 13 for 1) placing '8' from 17 for 1) as hex
          from raw
    )
    select (
        substr(hex, 1, 8) || '-' ||
        substr(hex, 9, 4) || '-' ||
        substr(hex, 13, 4) || '-' ||
        substr(hex, 17, 4) || '-' ||
        substr(hex, 21, 12)
    )::uuid
      from shaped
$$;

create or replace function public.cex_enqueue_audit_outbox_v1(
    p_source_service text,
    p_event_id uuid,
    p_trace_id uuid,
    p_org_id uuid,
    p_envelope jsonb,
    p_max_attempts integer default 10
)
returns public.cex_audit_outbox_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    existing_row public.cex_audit_outbox_v1%rowtype;
    inserted_row public.cex_audit_outbox_v1%rowtype;
    occurred_at_value timestamptz;
begin
    if p_source_service is null
       or p_source_service !~ '^[a-z0-9][a-z0-9._-]{0,127}$' then
        raise exception 'invalid audit outbox source_service';
    end if;
    if p_event_id is null or p_event_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'audit outbox event_id must be non-nil';
    end if;
    if p_trace_id is null or p_trace_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'audit outbox trace_id must be non-nil';
    end if;
    if p_max_attempts not between 1 and 100 then
        raise exception 'audit outbox max_attempts must be between 1 and 100';
    end if;
    if p_envelope is null or jsonb_typeof(p_envelope) is distinct from 'object' then
        raise exception 'audit outbox envelope must be a JSON object';
    end if;
    if (p_envelope ->> 'event_id')::uuid is distinct from p_event_id then
        raise exception 'audit outbox envelope event_id mismatch';
    end if;
    if (p_envelope ->> 'trace_id')::uuid is distinct from p_trace_id then
        raise exception 'audit outbox envelope trace_id mismatch';
    end if;
    if p_envelope ? 'org_id'
       and jsonb_typeof(p_envelope -> 'org_id') not in ('string', 'null') then
        raise exception 'audit outbox envelope org_id must be a UUID string or null';
    end if;
    if (p_envelope ->> 'org_id')::uuid is distinct from p_org_id then
        raise exception 'audit outbox envelope org_id mismatch';
    end if;
    if coalesce(length(btrim(p_envelope ->> 'actor_type')), 0) not between 1 and 128 then
        raise exception 'audit outbox envelope actor_type must contain 1..128 characters';
    end if;
    if p_envelope ? 'actor_id'
       and jsonb_typeof(p_envelope -> 'actor_id') not in ('string', 'null') then
        raise exception 'audit outbox envelope actor_id must be a string or null';
    end if;
    if coalesce(length(btrim(p_envelope ->> 'event_type')), 0) not between 1 and 128 then
        raise exception 'audit outbox envelope event_type must contain 1..128 characters';
    end if;
    if p_envelope ->> 'schema_version' is distinct from 'cex.audit.event.v2' then
        raise exception 'audit outbox envelope schema_version mismatch';
    end if;
    begin
        occurred_at_value := (p_envelope ->> 'occurred_at')::timestamptz;
    exception
        when others then
            raise exception 'audit outbox envelope occurred_at is invalid';
    end;
    if occurred_at_value > clock_timestamp() + interval '5 minutes' then
        raise exception 'audit outbox envelope occurred_at exceeds future-time allowance';
    end if;
    if jsonb_typeof(p_envelope -> 'payload') is distinct from 'object' then
        raise exception 'audit outbox envelope payload must be a JSON object';
    end if;
    if (p_envelope -> 'payload') ? '_cex_audit_writer' then
        raise exception 'audit outbox payload contains reserved writer metadata';
    end if;
    if p_envelope #>> '{payload,_cex_audit_source_service}' is distinct from p_source_service then
        raise exception 'audit outbox payload source-service marker mismatch';
    end if;

    perform pg_advisory_xact_lock(hashtextextended(p_event_id::text, 0));

    select *
      into existing_row
      from public.cex_audit_outbox_v1
     where event_id = p_event_id;

    if found then
        if existing_row.source_service is distinct from p_source_service
           or existing_row.org_id is distinct from p_org_id
           or existing_row.trace_id is distinct from p_trace_id
           or existing_row.envelope is distinct from p_envelope
           or existing_row.max_attempts is distinct from p_max_attempts then
            raise exception using
                errcode = '23505',
                message = 'audit outbox event id collision with different immutable content';
        end if;
        return existing_row;
    end if;

    insert into public.cex_audit_outbox_v1 (
        event_id,
        source_service,
        org_id,
        trace_id,
        status,
        attempt_count,
        max_attempts,
        available_at,
        envelope
    ) values (
        p_event_id,
        p_source_service,
        p_org_id,
        p_trace_id,
        'pending',
        0,
        p_max_attempts,
        now(),
        p_envelope
    ) returning * into inserted_row;

    return inserted_row;
end
$$;

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
    p_delivery_receipt jsonb,
    p_http_status integer
)
returns public.cex_audit_outbox_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    current_row public.cex_audit_outbox_v1%rowtype;
    updated_row public.cex_audit_outbox_v1%rowtype;
begin
    if p_worker_id is null or length(btrim(p_worker_id)) not between 1 and 128 then
        raise exception 'audit outbox ACK worker_id must contain 1..128 characters';
    end if;
    if p_event_hash !~ '^sha256:[0-9a-f]{64}$' then
        raise exception 'audit outbox ACK event hash is invalid';
    end if;
    if p_tenant_sequence <= 0 then
        raise exception 'audit outbox ACK tenant sequence must be positive';
    end if;
    if p_http_status not between 200 and 299 then
        raise exception 'audit outbox ACK HTTP status must be 2xx';
    end if;
    if p_delivery_receipt is null or jsonb_typeof(p_delivery_receipt) is distinct from 'object' then
        raise exception 'audit outbox ACK receipt must be a JSON object';
    end if;
    if (p_delivery_receipt #>> '{record,event_id}')::uuid is distinct from p_event_id
       or p_delivery_receipt #>> '{record,event_hash}' is distinct from p_event_hash
       or (p_delivery_receipt #>> '{record,tenant_sequence}')::bigint
          is distinct from p_tenant_sequence then
        raise exception 'audit outbox ACK receipt does not match verified event evidence';
    end if;

    select *
      into current_row
      from public.cex_audit_outbox_v1
     where outbox_id = p_outbox_id
     for update;

    if not found then
        raise exception 'audit outbox ACK target not found';
    end if;
    if current_row.event_id is distinct from p_event_id then
        raise exception 'audit outbox ACK event_id mismatch';
    end if;

    if current_row.status = 'delivered' then
        if current_row.delivered_event_hash is distinct from p_event_hash
           or current_row.delivered_tenant_sequence is distinct from p_tenant_sequence
           or current_row.delivery_receipt is distinct from p_delivery_receipt
           or current_row.last_http_status is distinct from p_http_status then
            raise exception using
                errcode = '23505',
                message = 'audit outbox ACK collision with different receipt';
        end if;
        return current_row;
    end if;

    if current_row.status <> 'claimed'
       or current_row.claimed_by is distinct from p_worker_id
       or current_row.lease_expires_at is null
       or current_row.lease_expires_at <= now() then
        raise exception 'audit outbox ACK requires an active claim owned by the worker';
    end if;

    update public.cex_audit_outbox_v1
       set status = 'delivered',
           claimed_by = null,
           lease_expires_at = null,
           delivered_at = now(),
           delivered_event_hash = p_event_hash,
           delivered_tenant_sequence = p_tenant_sequence,
           delivery_receipt = p_delivery_receipt,
           dead_lettered_at = null,
           last_http_status = p_http_status,
           last_error_code = null,
           last_error_message = null,
           updated_at = now()
     where outbox_id = p_outbox_id
    returning * into updated_row;

    return updated_row;
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
    current_row public.cex_audit_outbox_v1%rowtype;
    updated_row public.cex_audit_outbox_v1%rowtype;
    retry_delay_seconds integer;
    next_status text;
begin
    if p_worker_id is null or length(btrim(p_worker_id)) not between 1 and 128 then
        raise exception 'audit outbox failure worker_id must contain 1..128 characters';
    end if;
    if p_error_code is null or length(btrim(p_error_code)) not between 1 and 128 then
        raise exception 'audit outbox failure error_code must contain 1..128 characters';
    end if;
    if p_error_message is null or length(btrim(p_error_message)) not between 1 and 4000 then
        raise exception 'audit outbox failure message must contain 1..4000 characters';
    end if;
    if p_http_status is not null and p_http_status not between 100 and 599 then
        raise exception 'audit outbox failure HTTP status is invalid';
    end if;
    if p_retry_after_seconds is not null and p_retry_after_seconds not between 1 and 3600 then
        raise exception 'audit outbox retry delay must be between 1 and 3600 seconds';
    end if;

    select *
      into current_row
      from public.cex_audit_outbox_v1
     where outbox_id = p_outbox_id
     for update;

    if not found then
        raise exception 'audit outbox failure target not found';
    end if;
    if current_row.status <> 'claimed'
       or current_row.claimed_by is distinct from p_worker_id
       or current_row.lease_expires_at is null
       or current_row.lease_expires_at <= now() then
        raise exception 'audit outbox failure transition requires an active claim owned by the worker';
    end if;

    next_status := case
        when p_retryable and current_row.attempt_count < current_row.max_attempts
            then 'retry_wait'
        else 'dead_letter'
    end;

    retry_delay_seconds := case
        when next_status = 'retry_wait'
            then coalesce(
                p_retry_after_seconds,
                least(3600, greatest(1, (2 ^ least(current_row.attempt_count - 1, 10))::integer))
            )
        else 0
    end;

    update public.cex_audit_outbox_v1
       set status = next_status,
           available_at = case
               when next_status = 'retry_wait'
                   then now() + make_interval(secs => retry_delay_seconds)
               else available_at
           end,
           claimed_by = null,
           lease_expires_at = null,
           last_http_status = p_http_status,
           last_error_code = left(btrim(p_error_code), 128),
           last_error_message = left(btrim(p_error_message), 4000),
           dead_lettered_at = case
               when next_status = 'dead_letter' then now()
               else null
           end,
           updated_at = now()
     where outbox_id = p_outbox_id
    returning * into updated_row;

    return updated_row;
end
$$;

create index if not exists idx_cex_audit_outbox_dead_letter_v1
    on public.cex_audit_outbox_v1 (dead_lettered_at, source_service)
    where status = 'dead_letter';
create index if not exists idx_cex_audit_outbox_delivery_v1
    on public.cex_audit_outbox_v1 (delivered_at, source_service)
    where status = 'delivered';

create or replace view public.cex_audit_outbox_delivery_summary_v1 as
select
    source_service,
    status,
    count(*)::bigint as event_count,
    min(available_at) as oldest_available_at,
    min(last_attempt_at) as oldest_last_attempt_at,
    max(attempt_count)::integer as max_attempt_count,
    count(*) filter (where attempt_count >= max_attempts)::bigint as retry_budget_exhausted,
    count(*) filter (
        where status = 'delivered'
          and (
              delivered_event_hash is null
              or delivered_tenant_sequence is null
              or delivery_receipt is null
          )
    )::bigint as unverified_delivered
from public.cex_audit_outbox_v1
group by source_service, status;

commit;
