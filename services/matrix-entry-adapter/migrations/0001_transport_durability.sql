begin;

create table if not exists public.matrix_transport_cursors (
    partition_id text primary key,
    opaque_cursor text,
    cursor_revision bigint not null default 0 check (cursor_revision >= 0),
    lease_owner text,
    lease_fence bigint not null default 0 check (lease_fence >= 0),
    lease_expires_at timestamptz,
    updated_at timestamptz not null default clock_timestamp(),
    check (octet_length(partition_id) between 1 and 256),
    check (opaque_cursor is null or octet_length(opaque_cursor) <= 8192),
    check (lease_owner is null or octet_length(lease_owner) between 1 and 256),
    check ((lease_owner is null) = (lease_expires_at is null))
);

create table if not exists public.matrix_transport_inbox (
    source_event_id text primary key,
    source_event_sha256 text not null,
    partition_id text not null,
    observed_cursor text,
    accepted_at timestamptz not null default clock_timestamp(),
    check (octet_length(source_event_id) between 1 and 512),
    check (source_event_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    check (octet_length(partition_id) between 1 and 256),
    check (observed_cursor is null or octet_length(observed_cursor) <= 8192)
);

create table if not exists public.matrix_transport_outbox (
    delivery_id uuid primary key,
    source_event_id text not null,
    destination text not null,
    payload_sha256 text not null,
    payload jsonb not null,
    status text not null default 'pending'
        check (status in ('pending', 'claimed', 'sent', 'dead_letter')),
    attempt_count integer not null default 0 check (attempt_count >= 0),
    max_attempts integer not null check (max_attempts between 1 and 100),
    lease_owner text,
    lease_fence bigint not null default 0 check (lease_fence >= 0),
    lease_expires_at timestamptz,
    last_error_code text,
    created_at timestamptz not null default clock_timestamp(),
    updated_at timestamptz not null default clock_timestamp(),
    sent_at timestamptz,
    unique (source_event_id, destination),
    check (octet_length(source_event_id) between 1 and 512),
    check (octet_length(destination) between 1 and 512),
    check (payload_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    check (pg_column_size(payload) <= 1048576),
    check (lease_owner is null or octet_length(lease_owner) between 1 and 256),
    check (last_error_code is null or octet_length(last_error_code) between 1 and 128),
    check ((status = 'claimed') = (lease_owner is not null)),
    check ((status = 'claimed') = (lease_expires_at is not null)),
    check ((status = 'sent') = (sent_at is not null))
);

create index if not exists matrix_transport_outbox_claim_idx
    on public.matrix_transport_outbox (status, lease_expires_at, created_at, delivery_id)
    where status in ('pending', 'claimed');

create table if not exists public.matrix_transport_delivery_history (
    history_id bigserial primary key,
    delivery_id uuid not null,
    lease_fence bigint not null check (lease_fence >= 0),
    from_status text,
    to_status text not null,
    owner text,
    error_code text,
    occurred_at timestamptz not null default clock_timestamp(),
    check (from_status is null or from_status in ('pending', 'claimed', 'sent', 'dead_letter')),
    check (to_status in ('pending', 'claimed', 'sent', 'dead_letter')),
    check (owner is null or octet_length(owner) between 1 and 256),
    check (error_code is null or octet_length(error_code) between 1 and 128)
);

create index if not exists matrix_transport_delivery_history_delivery_idx
    on public.matrix_transport_delivery_history (delivery_id, history_id);

create table if not exists public.matrix_transport_poison_events (
    source_event_id text primary key,
    source_event_sha256 text not null,
    partition_id text not null,
    failure_code text not null,
    observation_count bigint not null default 1 check (observation_count >= 1),
    first_observed_at timestamptz not null default clock_timestamp(),
    last_observed_at timestamptz not null default clock_timestamp(),
    acknowledged_by text,
    acknowledged_at timestamptz,
    acknowledgement_note text,
    check (octet_length(source_event_id) between 1 and 512),
    check (source_event_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    check (octet_length(partition_id) between 1 and 256),
    check (octet_length(failure_code) between 1 and 128),
    check (acknowledged_by is null or octet_length(acknowledged_by) between 1 and 256),
    check ((acknowledged_by is null) = (acknowledged_at is null)),
    check (acknowledgement_note is null or octet_length(acknowledgement_note) <= 4096)
);

create or replace function public.cex_matrix_reject_immutable_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'matrix_immutable_history_mutation_rejected';
end;
$$;

create or replace function public.cex_matrix_guard_outbox_identity_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if new.delivery_id is distinct from old.delivery_id
       or new.source_event_id is distinct from old.source_event_id
       or new.destination is distinct from old.destination
       or new.payload_sha256 is distinct from old.payload_sha256
       or new.payload is distinct from old.payload
       or new.max_attempts is distinct from old.max_attempts
       or new.created_at is distinct from old.created_at then
        raise exception 'matrix_delivery_identity_mutation_rejected';
    end if;
    return new;
end;
$$;

drop trigger if exists matrix_transport_inbox_immutable_v1
    on public.matrix_transport_inbox;
create trigger matrix_transport_inbox_immutable_v1
before update or delete on public.matrix_transport_inbox
for each row execute function public.cex_matrix_reject_immutable_mutation_v1();

drop trigger if exists matrix_transport_delivery_history_immutable_v1
    on public.matrix_transport_delivery_history;
create trigger matrix_transport_delivery_history_immutable_v1
before update or delete on public.matrix_transport_delivery_history
for each row execute function public.cex_matrix_reject_immutable_mutation_v1();

drop trigger if exists matrix_transport_outbox_identity_guard_v1
    on public.matrix_transport_outbox;
create trigger matrix_transport_outbox_identity_guard_v1
before update on public.matrix_transport_outbox
for each row execute function public.cex_matrix_guard_outbox_identity_v1();

create or replace function public.cex_matrix_acquire_cursor_lease_v1(
    p_partition_id text,
    p_owner text,
    p_lease_seconds integer
)
returns table (
    opaque_cursor text,
    cursor_revision bigint,
    lease_fence bigint,
    lease_expires_at timestamptz
)
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if p_partition_id is null or octet_length(p_partition_id) not between 1 and 256 then
        raise exception 'invalid_matrix_partition';
    end if;
    if p_owner is null or octet_length(p_owner) not between 1 and 256 then
        raise exception 'invalid_matrix_lease_owner';
    end if;
    if p_lease_seconds not between 5 and 3600 then
        raise exception 'invalid_matrix_lease_seconds';
    end if;

    return query
    insert into public.matrix_transport_cursors as cursor_state (
        partition_id,
        lease_owner,
        lease_fence,
        lease_expires_at,
        updated_at
    ) values (
        p_partition_id,
        p_owner,
        1,
        clock_timestamp() + make_interval(secs => p_lease_seconds),
        clock_timestamp()
    )
    on conflict (partition_id) do update
       set lease_owner = excluded.lease_owner,
           lease_fence = cursor_state.lease_fence + 1,
           lease_expires_at = excluded.lease_expires_at,
           updated_at = clock_timestamp()
     where cursor_state.lease_owner = p_owner
        or cursor_state.lease_expires_at <= clock_timestamp()
    returning cursor_state.opaque_cursor,
              cursor_state.cursor_revision,
              cursor_state.lease_fence,
              cursor_state.lease_expires_at;
end;
$$;

create or replace function public.cex_matrix_advance_cursor_v1(
    p_partition_id text,
    p_owner text,
    p_lease_fence bigint,
    p_expected_revision bigint,
    p_next_cursor text
)
returns boolean
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    updated_count integer;
begin
    if p_partition_id is null or octet_length(p_partition_id) not between 1 and 256 then
        raise exception 'invalid_matrix_partition';
    end if;
    if p_owner is null or octet_length(p_owner) not between 1 and 256 then
        raise exception 'invalid_matrix_lease_owner';
    end if;
    if p_lease_fence is null or p_lease_fence < 1 then
        raise exception 'invalid_matrix_lease_fence';
    end if;
    if p_expected_revision is null or p_expected_revision < 0 then
        raise exception 'invalid_matrix_cursor_revision';
    end if;
    if p_next_cursor is null or octet_length(p_next_cursor) > 8192 then
        raise exception 'invalid_matrix_next_cursor';
    end if;

    update public.matrix_transport_cursors
       set opaque_cursor = p_next_cursor,
           cursor_revision = cursor_revision + 1,
           updated_at = clock_timestamp()
     where partition_id = p_partition_id
       and lease_owner = p_owner
       and lease_fence = p_lease_fence
       and cursor_revision = p_expected_revision
       and lease_expires_at > clock_timestamp();
    get diagnostics updated_count = row_count;
    return updated_count = 1;
end;
$$;

create or replace function public.cex_matrix_accept_source_event_v1(
    p_source_event_id text,
    p_source_event_sha256 text,
    p_partition_id text,
    p_observed_cursor text
)
returns text
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    existing_hash text;
    existing_partition text;
    existing_cursor text;
begin
    if p_source_event_id is null or octet_length(p_source_event_id) not between 1 and 512 then
        raise exception 'invalid_matrix_source_event_id';
    end if;
    if p_source_event_sha256 is null
       or p_source_event_sha256 !~ '^sha256:[0-9a-f]{64}$' then
        raise exception 'invalid_matrix_source_event_hash';
    end if;
    if p_partition_id is null or octet_length(p_partition_id) not between 1 and 256 then
        raise exception 'invalid_matrix_partition';
    end if;
    if p_observed_cursor is not null and octet_length(p_observed_cursor) > 8192 then
        raise exception 'invalid_matrix_observed_cursor';
    end if;

    insert into public.matrix_transport_inbox (
        source_event_id,
        source_event_sha256,
        partition_id,
        observed_cursor
    ) values (
        p_source_event_id,
        p_source_event_sha256,
        p_partition_id,
        p_observed_cursor
    )
    on conflict (source_event_id) do nothing;

    if found then
        return 'accepted';
    end if;

    select source_event_sha256, partition_id, observed_cursor
      into existing_hash, existing_partition, existing_cursor
      from public.matrix_transport_inbox
     where source_event_id = p_source_event_id;
    if existing_hash = p_source_event_sha256
       and existing_partition = p_partition_id
       and existing_cursor is not distinct from p_observed_cursor then
        return 'replay';
    end if;
    raise exception 'matrix_source_event_identity_collision';
end;
$$;

create or replace function public.cex_matrix_enqueue_delivery_v1(
    p_delivery_id uuid,
    p_source_event_id text,
    p_destination text,
    p_payload_sha256 text,
    p_payload jsonb,
    p_max_attempts integer
)
returns text
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    existing_hash text;
    existing_delivery_id uuid;
    existing_payload jsonb;
    existing_max_attempts integer;
begin
    if p_delivery_id is null then
        raise exception 'invalid_matrix_delivery_id';
    end if;
    if p_source_event_id is null or octet_length(p_source_event_id) not between 1 and 512 then
        raise exception 'invalid_matrix_source_event_id';
    end if;
    if p_destination is null or octet_length(p_destination) not between 1 and 512 then
        raise exception 'invalid_matrix_destination';
    end if;
    if p_payload_sha256 is null
       or p_payload_sha256 !~ '^sha256:[0-9a-f]{64}$' then
        raise exception 'invalid_matrix_payload_hash';
    end if;
    if p_payload is null or pg_column_size(p_payload) > 1048576 then
        raise exception 'invalid_matrix_payload';
    end if;
    if p_max_attempts not between 1 and 100 then
        raise exception 'invalid_matrix_delivery_attempt_budget';
    end if;
    if not exists (
        select 1
          from public.matrix_transport_inbox
         where source_event_id = p_source_event_id
    ) then
        raise exception 'matrix_source_event_not_accepted';
    end if;

    insert into public.matrix_transport_outbox (
        delivery_id,
        source_event_id,
        destination,
        payload_sha256,
        payload,
        max_attempts
    ) values (
        p_delivery_id,
        p_source_event_id,
        p_destination,
        p_payload_sha256,
        p_payload,
        p_max_attempts
    )
    on conflict (source_event_id, destination) do nothing;

    if found then
        insert into public.matrix_transport_delivery_history (
            delivery_id,
            lease_fence,
            from_status,
            to_status
        ) values (p_delivery_id, 0, null, 'pending');
        return 'enqueued';
    end if;

    select delivery_id, payload_sha256, payload, max_attempts
      into existing_delivery_id, existing_hash, existing_payload, existing_max_attempts
      from public.matrix_transport_outbox
     where source_event_id = p_source_event_id
       and destination = p_destination;
    if existing_delivery_id = p_delivery_id
       and existing_hash = p_payload_sha256
       and existing_payload = p_payload
       and existing_max_attempts = p_max_attempts then
        return 'replay';
    end if;
    raise exception 'matrix_delivery_identity_collision';
end;
$$;

create or replace function public.cex_matrix_register_delivery_and_advance_v1(
    p_partition_id text,
    p_owner text,
    p_lease_fence bigint,
    p_expected_revision bigint,
    p_next_cursor text,
    p_source_event_id text,
    p_source_event_sha256 text,
    p_observed_cursor text,
    p_delivery_id uuid,
    p_destination text,
    p_payload_sha256 text,
    p_payload jsonb,
    p_max_attempts integer
)
returns table (
    event_disposition text,
    delivery_disposition text,
    next_cursor_revision bigint
)
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    locked_revision bigint;
begin
    select cursor_state.cursor_revision
      into locked_revision
      from public.matrix_transport_cursors as cursor_state
     where cursor_state.partition_id = p_partition_id
       and cursor_state.lease_owner = p_owner
       and cursor_state.lease_fence = p_lease_fence
       and cursor_state.cursor_revision = p_expected_revision
       and cursor_state.lease_expires_at > clock_timestamp()
     for update;

    if not found then
        raise exception 'matrix_cursor_lease_or_revision_mismatch';
    end if;

    event_disposition := public.cex_matrix_accept_source_event_v1(
        p_source_event_id,
        p_source_event_sha256,
        p_partition_id,
        p_observed_cursor
    );
    delivery_disposition := public.cex_matrix_enqueue_delivery_v1(
        p_delivery_id,
        p_source_event_id,
        p_destination,
        p_payload_sha256,
        p_payload,
        p_max_attempts
    );

    if not public.cex_matrix_advance_cursor_v1(
        p_partition_id,
        p_owner,
        p_lease_fence,
        p_expected_revision,
        p_next_cursor
    ) then
        raise exception 'matrix_cursor_lease_or_revision_mismatch';
    end if;

    next_cursor_revision := locked_revision + 1;
    return next;
end;
$$;

create or replace function public.cex_matrix_claim_delivery_v1(
    p_owner text,
    p_lease_seconds integer,
    p_limit integer
)
returns table (
    delivery_id uuid,
    source_event_id text,
    destination text,
    payload_sha256 text,
    payload jsonb,
    attempt_count integer,
    max_attempts integer,
    lease_fence bigint,
    lease_expires_at timestamptz
)
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if p_owner is null or octet_length(p_owner) not between 1 and 256 then
        raise exception 'invalid_matrix_delivery_owner';
    end if;
    if p_lease_seconds not between 5 and 3600 then
        raise exception 'invalid_matrix_delivery_lease';
    end if;
    if p_limit not between 1 and 100 then
        raise exception 'invalid_matrix_delivery_limit';
    end if;

    with exhausted as (
        update public.matrix_transport_outbox as outbox
           set status = 'dead_letter',
               lease_owner = null,
               lease_expires_at = null,
               last_error_code = 'lease_expired_retry_exhausted',
               updated_at = clock_timestamp()
         where outbox.status = 'claimed'
           and outbox.lease_expires_at <= clock_timestamp()
           and outbox.attempt_count >= outbox.max_attempts
        returning outbox.delivery_id,
                  outbox.lease_fence,
                  outbox.lease_owner
    )
    insert into public.matrix_transport_delivery_history (
        delivery_id,
        lease_fence,
        from_status,
        to_status,
        owner,
        error_code
    )
    select exhausted.delivery_id,
           exhausted.lease_fence,
           'claimed',
           'dead_letter',
           exhausted.lease_owner,
           'lease_expired_retry_exhausted'
      from exhausted;

    return query
    with selected as (
        select outbox.delivery_id,
               outbox.status as previous_status
          from public.matrix_transport_outbox as outbox
         where (outbox.status = 'pending'
             or (outbox.status = 'claimed' and outbox.lease_expires_at <= clock_timestamp()))
           and outbox.attempt_count < outbox.max_attempts
         order by outbox.created_at, outbox.delivery_id
         for update skip locked
         limit p_limit
    ), claimed as (
        update public.matrix_transport_outbox as outbox
           set status = 'claimed',
               attempt_count = outbox.attempt_count + 1,
               lease_owner = p_owner,
               lease_fence = outbox.lease_fence + 1,
               lease_expires_at = clock_timestamp() + make_interval(secs => p_lease_seconds),
               last_error_code = null,
               updated_at = clock_timestamp()
          from selected
         where outbox.delivery_id = selected.delivery_id
        returning outbox.delivery_id,
                  outbox.source_event_id,
                  outbox.destination,
                  outbox.payload_sha256,
                  outbox.payload,
                  outbox.attempt_count,
                  outbox.max_attempts,
                  outbox.lease_fence,
                  outbox.lease_expires_at,
                  selected.previous_status
    ), history as (
        insert into public.matrix_transport_delivery_history (
            delivery_id,
            lease_fence,
            from_status,
            to_status,
            owner
        )
        select claimed.delivery_id,
               claimed.lease_fence,
               claimed.previous_status,
               'claimed',
               p_owner
          from claimed
        returning delivery_id
    )
    select claimed.delivery_id,
           claimed.source_event_id,
           claimed.destination,
           claimed.payload_sha256,
           claimed.payload,
           claimed.attempt_count,
           claimed.max_attempts,
           claimed.lease_fence,
           claimed.lease_expires_at
      from claimed
      join history using (delivery_id);
end;
$$;

create or replace function public.cex_matrix_finish_delivery_v1(
    p_delivery_id uuid,
    p_owner text,
    p_lease_fence bigint,
    p_outcome text,
    p_error_code text
)
returns text
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    current_attempt integer;
    current_max integer;
    next_status text;
begin
    if p_delivery_id is null then
        raise exception 'invalid_matrix_delivery_id';
    end if;
    if p_owner is null or octet_length(p_owner) not between 1 and 256 then
        raise exception 'invalid_matrix_delivery_owner';
    end if;
    if p_lease_fence is null or p_lease_fence < 1 then
        raise exception 'invalid_matrix_lease_fence';
    end if;
    if p_outcome not in ('sent', 'retryable_failure', 'permanent_failure') then
        raise exception 'invalid_matrix_delivery_outcome';
    end if;
    if p_outcome <> 'sent'
       and (p_error_code is null or octet_length(p_error_code) not between 1 and 128) then
        raise exception 'invalid_matrix_delivery_error_code';
    end if;

    select attempt_count, max_attempts
      into current_attempt, current_max
      from public.matrix_transport_outbox
     where delivery_id = p_delivery_id
       and status = 'claimed'
       and lease_owner = p_owner
       and lease_fence = p_lease_fence
       and lease_expires_at > clock_timestamp()
     for update;
    if not found then
        raise exception 'matrix_delivery_claim_mismatch';
    end if;

    if p_outcome = 'sent' then
        next_status := 'sent';
    elsif p_outcome = 'permanent_failure' or current_attempt >= current_max then
        next_status := 'dead_letter';
    else
        next_status := 'pending';
    end if;

    update public.matrix_transport_outbox
       set status = next_status,
           lease_owner = null,
           lease_expires_at = null,
           last_error_code = case when next_status = 'sent' then null else p_error_code end,
           sent_at = case when next_status = 'sent' then clock_timestamp() else null end,
           updated_at = clock_timestamp()
     where delivery_id = p_delivery_id;

    insert into public.matrix_transport_delivery_history (
        delivery_id,
        lease_fence,
        from_status,
        to_status,
        owner,
        error_code
    ) values (
        p_delivery_id,
        p_lease_fence,
        'claimed',
        next_status,
        p_owner,
        case when next_status = 'sent' then null else p_error_code end
    );
    return next_status;
end;
$$;

create or replace function public.cex_matrix_lookup_delivery_v1(
    p_delivery_id uuid,
    p_payload_sha256 text
)
returns table (
    delivery_id uuid,
    source_event_id text,
    destination text,
    payload_sha256 text,
    payload jsonb,
    status text,
    attempt_count integer,
    max_attempts integer,
    lease_owner text,
    lease_fence bigint,
    lease_expires_at timestamptz,
    last_error_code text,
    created_at timestamptz,
    updated_at timestamptz,
    sent_at timestamptz
)
language sql
stable
set search_path = pg_catalog, public
as $$
    select outbox.delivery_id,
           outbox.source_event_id,
           outbox.destination,
           outbox.payload_sha256,
           outbox.payload,
           outbox.status,
           outbox.attempt_count,
           outbox.max_attempts,
           outbox.lease_owner,
           outbox.lease_fence,
           outbox.lease_expires_at,
           outbox.last_error_code,
           outbox.created_at,
           outbox.updated_at,
           outbox.sent_at
      from public.matrix_transport_outbox as outbox
     where outbox.delivery_id = p_delivery_id
       and outbox.payload_sha256 = p_payload_sha256
$$;

create or replace function public.cex_matrix_record_poison_event_v1(
    p_source_event_id text,
    p_source_event_sha256 text,
    p_partition_id text,
    p_failure_code text
)
returns bigint
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    result_count bigint;
begin
    if p_source_event_id is null or octet_length(p_source_event_id) not between 1 and 512 then
        raise exception 'invalid_matrix_source_event_id';
    end if;
    if p_source_event_sha256 is null
       or p_source_event_sha256 !~ '^sha256:[0-9a-f]{64}$' then
        raise exception 'invalid_matrix_source_event_hash';
    end if;
    if p_partition_id is null or octet_length(p_partition_id) not between 1 and 256 then
        raise exception 'invalid_matrix_partition';
    end if;
    if p_failure_code is null or octet_length(p_failure_code) not between 1 and 128 then
        raise exception 'invalid_matrix_failure_code';
    end if;

    insert into public.matrix_transport_poison_events as poison (
        source_event_id,
        source_event_sha256,
        partition_id,
        failure_code
    ) values (
        p_source_event_id,
        p_source_event_sha256,
        p_partition_id,
        p_failure_code
    )
    on conflict (source_event_id) do update
       set observation_count = poison.observation_count + 1,
           last_observed_at = clock_timestamp()
     where poison.source_event_sha256 = excluded.source_event_sha256
       and poison.partition_id = excluded.partition_id
       and poison.failure_code = excluded.failure_code
    returning observation_count into result_count;

    if result_count is null then
        raise exception 'matrix_poison_event_identity_collision';
    end if;
    return result_count;
end;
$$;

create or replace function public.cex_matrix_acknowledge_poison_event_v1(
    p_source_event_id text,
    p_source_event_sha256 text,
    p_operator text,
    p_note text
)
returns boolean
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    updated_count integer;
begin
    if p_source_event_id is null or octet_length(p_source_event_id) not between 1 and 512 then
        raise exception 'invalid_matrix_source_event_id';
    end if;
    if p_source_event_sha256 is null
       or p_source_event_sha256 !~ '^sha256:[0-9a-f]{64}$' then
        raise exception 'invalid_matrix_source_event_hash';
    end if;
    if p_operator is null or octet_length(p_operator) not between 1 and 256 then
        raise exception 'invalid_matrix_poison_operator';
    end if;
    if p_note is not null and octet_length(p_note) > 4096 then
        raise exception 'invalid_matrix_poison_note';
    end if;

    update public.matrix_transport_poison_events
       set acknowledged_by = p_operator,
           acknowledged_at = clock_timestamp(),
           acknowledgement_note = p_note,
           last_observed_at = clock_timestamp()
     where source_event_id = p_source_event_id
       and source_event_sha256 = p_source_event_sha256
       and acknowledged_at is null;
    get diagnostics updated_count = row_count;
    return updated_count = 1;
end;
$$;

revoke all on function public.cex_matrix_reject_immutable_mutation_v1() from public;
revoke all on function public.cex_matrix_guard_outbox_identity_v1() from public;
revoke all on function public.cex_matrix_acquire_cursor_lease_v1(text, text, integer) from public;
revoke all on function public.cex_matrix_advance_cursor_v1(text, text, bigint, bigint, text) from public;
revoke all on function public.cex_matrix_accept_source_event_v1(text, text, text, text) from public;
revoke all on function public.cex_matrix_enqueue_delivery_v1(uuid, text, text, text, jsonb, integer) from public;
revoke all on function public.cex_matrix_register_delivery_and_advance_v1(text, text, bigint, bigint, text, text, text, text, uuid, text, text, jsonb, integer) from public;
revoke all on function public.cex_matrix_claim_delivery_v1(text, integer, integer) from public;
revoke all on function public.cex_matrix_finish_delivery_v1(uuid, text, bigint, text, text) from public;
revoke all on function public.cex_matrix_lookup_delivery_v1(uuid, text) from public;
revoke all on function public.cex_matrix_record_poison_event_v1(text, text, text, text) from public;
revoke all on function public.cex_matrix_acknowledge_poison_event_v1(text, text, text, text) from public;

commit;
