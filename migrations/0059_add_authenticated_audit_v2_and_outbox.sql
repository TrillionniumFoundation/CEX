begin;

create table if not exists public.cex_audit_chain_heads_v2 (
    chain_key text primary key,
    org_id uuid,
    last_sequence bigint not null default 0,
    last_event_hash text,
    updated_at timestamptz not null default now(),
    constraint cex_audit_chain_heads_key_v2
        check (
            (org_id is null and chain_key = 'global')
            or
            (org_id is not null and chain_key = 'org:' || org_id::text)
        ),
    constraint cex_audit_chain_heads_sequence_v2
        check (last_sequence >= 0),
    constraint cex_audit_chain_heads_hash_v2
        check (
            last_event_hash is null
            or last_event_hash ~ '^sha256:[0-9a-f]{64}$'
        )
);

create table if not exists public.cex_audit_events_v2 (
    event_id uuid primary key,
    trace_id uuid not null,
    org_id uuid,
    chain_key text not null,
    tenant_sequence bigint not null,
    previous_event_hash text,
    event_hash text not null,
    writer_service_id text not null,
    writer_auth_scheme text not null,
    actor_type text not null,
    actor_id text,
    event_type text not null,
    schema_version text not null,
    occurred_at timestamptz not null,
    received_at timestamptz not null default now(),
    payload jsonb not null,
    constraint cex_audit_events_chain_key_v2
        check (
            (org_id is null and chain_key = 'global')
            or
            (org_id is not null and chain_key = 'org:' || org_id::text)
        ),
    constraint cex_audit_events_sequence_v2 check (tenant_sequence > 0),
    constraint cex_audit_events_previous_hash_v2
        check (
            previous_event_hash is null
            or previous_event_hash ~ '^sha256:[0-9a-f]{64}$'
        ),
    constraint cex_audit_events_hash_v2
        check (event_hash ~ '^sha256:[0-9a-f]{64}$'),
    constraint cex_audit_events_writer_v2
        check (writer_service_id ~ '^[a-z0-9][a-z0-9._-]{0,127}$'),
    constraint cex_audit_events_writer_auth_v2
        check (writer_auth_scheme = 'workload-token-v1'),
    constraint cex_audit_events_actor_type_v2
        check (actor_type ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'),
    constraint cex_audit_events_actor_id_v2
        check (actor_id is null or length(btrim(actor_id)) between 1 and 256),
    constraint cex_audit_events_event_type_v2
        check (event_type ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'),
    constraint cex_audit_events_schema_v2
        check (schema_version ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'),
    constraint cex_audit_events_payload_v2
        check (jsonb_typeof(payload) = 'object'),
    constraint cex_audit_events_future_time_v2
        check (occurred_at <= received_at + interval '5 minutes'),
    constraint cex_audit_events_chain_sequence_unique_v2
        unique (chain_key, tenant_sequence)
);

create or replace function public.cex_audit_event_hash_v2(
    p_previous_event_hash text,
    p_chain_key text,
    p_tenant_sequence bigint,
    p_event_id uuid,
    p_trace_id uuid,
    p_writer_service_id text,
    p_writer_auth_scheme text,
    p_actor_type text,
    p_actor_id text,
    p_event_type text,
    p_schema_version text,
    p_occurred_at timestamptz,
    p_payload jsonb
)
returns text
language sql
immutable
strict
set search_path = pg_catalog, public
as $$
    select 'sha256:' || encode(
        digest(
            jsonb_build_object(
                'previous_event_hash', p_previous_event_hash,
                'chain_key', p_chain_key,
                'tenant_sequence', p_tenant_sequence,
                'event_id', p_event_id,
                'trace_id', p_trace_id,
                'writer_service_id', p_writer_service_id,
                'writer_auth_scheme', p_writer_auth_scheme,
                'actor_type', p_actor_type,
                'actor_id', p_actor_id,
                'event_type', p_event_type,
                'schema_version', p_schema_version,
                'occurred_at', p_occurred_at,
                'payload', p_payload
            )::text,
            'sha256'
        ),
        'hex'
    )
$$;

create or replace function public.cex_append_audit_event_v2(
    p_event_id uuid,
    p_trace_id uuid,
    p_org_id uuid,
    p_writer_service_id text,
    p_writer_auth_scheme text,
    p_actor_type text,
    p_actor_id text,
    p_event_type text,
    p_schema_version text,
    p_occurred_at timestamptz,
    p_payload jsonb
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    existing_event public.cex_audit_events_v2%rowtype;
    inserted_event public.cex_audit_events_v2%rowtype;
    chain_head public.cex_audit_chain_heads_v2%rowtype;
    calculated_chain_key text;
    next_sequence bigint;
    previous_hash text;
    calculated_hash text;
    received_time timestamptz := clock_timestamp();
begin
    perform pg_advisory_xact_lock(hashtextextended(p_event_id::text, 0));

    select *
      into existing_event
      from public.cex_audit_events_v2
     where event_id = p_event_id;

    if found then
        if existing_event.trace_id is distinct from p_trace_id
           or existing_event.org_id is distinct from p_org_id
           or existing_event.writer_service_id is distinct from p_writer_service_id
           or existing_event.writer_auth_scheme is distinct from p_writer_auth_scheme
           or existing_event.actor_type is distinct from p_actor_type
           or existing_event.actor_id is distinct from p_actor_id
           or existing_event.event_type is distinct from p_event_type
           or existing_event.schema_version is distinct from p_schema_version
           or existing_event.occurred_at is distinct from p_occurred_at
           or existing_event.payload is distinct from p_payload then
            raise exception using
                errcode = '23505',
                message = 'audit event id collision with different immutable content';
        end if;
        return jsonb_build_object(
            'replayed', true,
            'record', to_jsonb(existing_event)
        );
    end if;

    if p_payload is null or jsonb_typeof(p_payload) is distinct from 'object' then
        raise exception 'audit v2 payload must be a JSON object';
    end if;
    if p_occurred_at > received_time + interval '5 minutes' then
        raise exception 'audit v2 occurred_at exceeds future-time allowance';
    end if;

    calculated_chain_key := case
        when p_org_id is null then 'global'
        else 'org:' || p_org_id::text
    end;

    insert into public.cex_audit_chain_heads_v2 (
        chain_key, org_id, last_sequence, last_event_hash, updated_at
    ) values (
        calculated_chain_key, p_org_id, 0, null, received_time
    ) on conflict (chain_key) do nothing;

    select *
      into chain_head
      from public.cex_audit_chain_heads_v2
     where chain_key = calculated_chain_key
     for update;

    if chain_head.org_id is distinct from p_org_id then
        raise exception 'audit chain head org binding mismatch';
    end if;

    next_sequence := chain_head.last_sequence + 1;
    previous_hash := chain_head.last_event_hash;
    calculated_hash := public.cex_audit_event_hash_v2(
        coalesce(previous_hash, 'GENESIS'),
        calculated_chain_key,
        next_sequence,
        p_event_id,
        p_trace_id,
        p_writer_service_id,
        p_writer_auth_scheme,
        p_actor_type,
        coalesce(p_actor_id, ''),
        p_event_type,
        p_schema_version,
        p_occurred_at,
        p_payload
    );

    insert into public.cex_audit_events_v2 (
        event_id,
        trace_id,
        org_id,
        chain_key,
        tenant_sequence,
        previous_event_hash,
        event_hash,
        writer_service_id,
        writer_auth_scheme,
        actor_type,
        actor_id,
        event_type,
        schema_version,
        occurred_at,
        received_at,
        payload
    ) values (
        p_event_id,
        p_trace_id,
        p_org_id,
        calculated_chain_key,
        next_sequence,
        previous_hash,
        calculated_hash,
        p_writer_service_id,
        p_writer_auth_scheme,
        p_actor_type,
        p_actor_id,
        p_event_type,
        p_schema_version,
        p_occurred_at,
        received_time,
        p_payload
    ) returning * into inserted_event;

    update public.cex_audit_chain_heads_v2
       set last_sequence = next_sequence,
           last_event_hash = calculated_hash,
           updated_at = received_time
     where chain_key = calculated_chain_key;

    return jsonb_build_object(
        'replayed', false,
        'record', to_jsonb(inserted_event)
    );
end
$$;

create or replace function public.cex_reject_audit_event_mutation_v2()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'cex_audit_events_v2 is append-only';
end
$$;

drop trigger if exists trg_cex_audit_events_append_only_v2
    on public.cex_audit_events_v2;
create trigger trg_cex_audit_events_append_only_v2
before update or delete on public.cex_audit_events_v2
for each row execute function public.cex_reject_audit_event_mutation_v2();

create table if not exists public.cex_audit_outbox_v1 (
    outbox_id uuid primary key default gen_random_uuid(),
    event_id uuid not null unique,
    source_service text not null,
    org_id uuid,
    trace_id uuid not null,
    status text not null default 'pending',
    attempt_count integer not null default 0,
    max_attempts integer not null default 10,
    available_at timestamptz not null default now(),
    claimed_by text,
    lease_expires_at timestamptz,
    envelope jsonb not null,
    last_error_code text,
    last_error_message text,
    delivered_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint cex_audit_outbox_source_v1
        check (source_service ~ '^[a-z0-9][a-z0-9._-]{0,127}$'),
    constraint cex_audit_outbox_status_v1
        check (status in ('pending', 'claimed', 'retry_wait', 'delivered', 'dead_letter')),
    constraint cex_audit_outbox_attempts_v1
        check (
            max_attempts between 1 and 100
            and attempt_count between 0 and max_attempts
        ),
    constraint cex_audit_outbox_claim_v1
        check (
            (status = 'claimed' and claimed_by is not null and lease_expires_at is not null)
            or
            (status <> 'claimed' and claimed_by is null and lease_expires_at is null)
        ),
    constraint cex_audit_outbox_delivery_v1
        check (
            (status = 'delivered' and delivered_at is not null)
            or
            (status <> 'delivered' and delivered_at is null)
        ),
    constraint cex_audit_outbox_envelope_v1
        check (jsonb_typeof(envelope) = 'object')
);

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
           updated_at = now()
      from candidates
     where outbox.outbox_id = candidates.outbox_id
    returning outbox.*;
end
$$;

create index if not exists idx_cex_audit_events_trace_v2
    on public.cex_audit_events_v2 (trace_id, received_at, event_id);
create index if not exists idx_cex_audit_events_org_sequence_v2
    on public.cex_audit_events_v2 (org_id, tenant_sequence)
    where org_id is not null;
create index if not exists idx_cex_audit_events_writer_v2
    on public.cex_audit_events_v2 (writer_service_id, received_at);
create index if not exists idx_cex_audit_outbox_claim_v1
    on public.cex_audit_outbox_v1 (status, available_at, created_at)
    where status in ('pending', 'claimed', 'retry_wait');
create index if not exists idx_cex_audit_outbox_trace_v1
    on public.cex_audit_outbox_v1 (trace_id, created_at);

create or replace view public.cex_audit_outbox_summary_v1 as
select
    source_service,
    status,
    count(*)::bigint as event_count,
    min(available_at) as oldest_available_at,
    min(lease_expires_at) filter (where status = 'claimed') as oldest_lease_expiry,
    count(*) filter (where attempt_count >= max_attempts)::bigint as retry_budget_exhausted
from public.cex_audit_outbox_v1
group by source_service, status;

commit;
