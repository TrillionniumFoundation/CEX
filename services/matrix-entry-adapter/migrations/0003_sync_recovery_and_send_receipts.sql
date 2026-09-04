begin;

-- Additive transport repair. Existing inbox/outbox identities and numbered
-- Ledger migration head 0088 are not rewritten. No runtime privileges granted.
create table if not exists public.matrix_transport_poison_payloads (
    source_event_id text primary key,
    source_event_sha256 text not null check (source_event_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    partition_id text not null check (octet_length(partition_id) between 1 and 256),
    room_id text not null check (octet_length(room_id) between 1 and 512),
    source_payload jsonb not null check (pg_column_size(source_payload) <= 4194304),
    recorded_at timestamptz not null default clock_timestamp(),
    check (octet_length(source_event_id) between 1 and 512)
);

create table if not exists public.matrix_transport_cursor_history (
    partition_id text not null,
    cursor_revision bigint not null check (cursor_revision > 0),
    previous_cursor text,
    next_cursor text not null check (octet_length(next_cursor) between 1 and 8192),
    lease_owner text not null,
    lease_fence bigint not null,
    committed_at timestamptz not null default clock_timestamp(),
    primary key (partition_id, cursor_revision)
);

create or replace function public.cex_matrix_renew_cursor_lease_v1(
    p_partition_id text, p_owner text, p_fence bigint, p_revision bigint, p_lease_seconds integer
)
returns boolean language plpgsql set search_path = pg_catalog, public as $$
declare changed integer;
begin
    if p_partition_id is null or octet_length(p_partition_id) not between 1 and 256
       or p_owner is null or octet_length(p_owner) not between 1 and 256
       or p_fence is null or p_fence < 1 or p_revision is null or p_revision < 0
       or p_lease_seconds is null or p_lease_seconds not between 5 and 3600 then
        raise exception 'invalid_matrix_cursor_renewal';
    end if;
    update public.matrix_transport_cursors
       set lease_expires_at = clock_timestamp() + make_interval(secs => p_lease_seconds),
           updated_at = clock_timestamp()
     where partition_id = p_partition_id and lease_owner = p_owner
       and lease_fence = p_fence and cursor_revision = p_revision
       and lease_expires_at > clock_timestamp();
    get diagnostics changed = row_count;
    return changed = 1;
end;
$$;

create or replace function public.cex_matrix_store_poison_payload_v1(
    p_source_event_id text, p_source_hash text, p_partition_id text, p_room_id text, p_payload jsonb
)
returns text language plpgsql set search_path = pg_catalog, public as $$
declare current_row public.matrix_transport_poison_payloads%rowtype;
begin
    if p_source_event_id is null or p_source_hash is null or p_partition_id is null
       or p_room_id is null or p_payload is null
       or pg_column_size(p_payload) > 4194304 then
        raise exception 'invalid_matrix_poison_payload';
    end if;
    if not exists (select 1 from public.matrix_transport_poison_events
        where source_event_id = p_source_event_id and source_event_sha256 = p_source_hash
          and partition_id = p_partition_id) then
        raise exception 'matrix_poison_observation_required';
    end if;
    insert into public.matrix_transport_poison_payloads
        (source_event_id, source_event_sha256, partition_id, room_id, source_payload)
    values (p_source_event_id, p_source_hash, p_partition_id, p_room_id, p_payload)
    on conflict (source_event_id) do nothing;
    if found then return 'stored'; end if;
    select * into strict current_row from public.matrix_transport_poison_payloads
        where source_event_id = p_source_event_id;
    if current_row.source_event_sha256 is distinct from p_source_hash
       or current_row.partition_id is distinct from p_partition_id
       or current_row.room_id is distinct from p_room_id
       or current_row.source_payload is distinct from p_payload then
        raise exception 'matrix_poison_payload_collision';
    end if;
    return 'replay';
end;
$$;

create or replace function public.cex_matrix_guard_cursor_advance_v1()
returns trigger language plpgsql set search_path = pg_catalog, public as $$
begin
    if new.cursor_revision is distinct from old.cursor_revision
       or new.opaque_cursor is distinct from old.opaque_cursor then
        if new.cursor_revision <> old.cursor_revision + 1
           or new.opaque_cursor is null or octet_length(new.opaque_cursor) not between 1 and 8192
           or old.lease_expires_at is null or old.lease_expires_at <= clock_timestamp()
           or new.lease_owner is distinct from old.lease_owner
           or new.lease_fence is distinct from old.lease_fence then
            raise exception 'matrix_cursor_lease_or_revision_mismatch';
        end if;
        if exists (select 1 from public.matrix_transport_poison_events
            where partition_id = old.partition_id and acknowledged_at is null) then
            raise exception 'matrix_poison_requires_operator_quarantine';
        end if;
    end if;
    return new;
end;
$$;

create or replace function public.cex_matrix_append_cursor_history_v1()
returns trigger language plpgsql set search_path = pg_catalog, public as $$
begin
    if new.cursor_revision is distinct from old.cursor_revision then
        insert into public.matrix_transport_cursor_history
            (partition_id, cursor_revision, previous_cursor, next_cursor, lease_owner, lease_fence)
        values (new.partition_id, new.cursor_revision, old.opaque_cursor,
                new.opaque_cursor, new.lease_owner, new.lease_fence);
    end if;
    return new;
end;
$$;

drop trigger if exists matrix_cursor_advance_guard_v1 on public.matrix_transport_cursors;
create trigger matrix_cursor_advance_guard_v1 before update on public.matrix_transport_cursors
for each row execute function public.cex_matrix_guard_cursor_advance_v1();
drop trigger if exists matrix_cursor_history_append_v1 on public.matrix_transport_cursors;
create trigger matrix_cursor_history_append_v1 after update on public.matrix_transport_cursors
for each row execute function public.cex_matrix_append_cursor_history_v1();

create table if not exists public.matrix_transport_send_bindings (
    delivery_id uuid primary key,
    payload_sha256 text not null check (payload_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    room_id text not null check (octet_length(room_id) between 1 and 512),
    homeserver text not null check (octet_length(homeserver) between 1 and 2048),
    credential_sha256 text not null check (credential_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    bound_at timestamptz not null default clock_timestamp()
);

create table if not exists public.matrix_transport_send_receipts (
    delivery_id uuid primary key,
    payload_sha256 text not null check (payload_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    room_id text not null,
    event_id text not null check (octet_length(event_id) between 2 and 512 and left(event_id, 1) = '$'
                                  and event_id !~ '[[:space:][:cntrl:]]'),
    recorded_at timestamptz not null default clock_timestamp()
);

create or replace function public.cex_matrix_bind_send_attempt_v1(
    p_delivery_id uuid, p_owner text, p_fence bigint, p_payload_hash text,
    p_room_id text, p_homeserver text, p_credential_hash text
)
returns text language plpgsql set search_path = pg_catalog, public as $$
declare prior public.matrix_transport_send_bindings%rowtype;
begin
    if p_delivery_id is null or p_owner is null or p_fence is null
       or p_payload_hash is null or p_room_id is null or p_homeserver is null
       or p_credential_hash is null then
        raise exception 'invalid_matrix_send_binding';
    end if;
    perform 1 from public.matrix_transport_outbox
      where delivery_id = p_delivery_id and status = 'claimed'
        and destination = 'matrix-homeserver-v1' and lease_owner = p_owner
        and lease_fence = p_fence and lease_expires_at > clock_timestamp()
        and payload_sha256 = p_payload_hash and payload ->> 'room_id' = p_room_id for update;
    if not found then raise exception 'matrix_delivery_claim_mismatch'; end if;
    insert into public.matrix_transport_send_bindings
        (delivery_id, payload_sha256, room_id, homeserver, credential_sha256)
    values (p_delivery_id, p_payload_hash, p_room_id, p_homeserver, p_credential_hash)
    on conflict (delivery_id) do nothing;
    if found then return 'bound'; end if;
    select * into strict prior from public.matrix_transport_send_bindings where delivery_id = p_delivery_id;
    if prior.payload_sha256 is distinct from p_payload_hash or prior.room_id is distinct from p_room_id
       or prior.homeserver is distinct from p_homeserver or prior.credential_sha256 is distinct from p_credential_hash then
        raise exception 'matrix_send_idempotency_scope_changed';
    end if;
    return 'replay';
end;
$$;

create or replace function public.cex_matrix_record_send_receipt_v1(
    p_delivery_id uuid, p_owner text, p_fence bigint, p_payload_hash text, p_room_id text, p_event_id text
)
returns text language plpgsql set search_path = pg_catalog, public as $$
declare prior public.matrix_transport_send_receipts%rowtype;
begin
    if p_delivery_id is null or p_owner is null or p_fence is null or p_payload_hash is null
       or p_room_id is null or p_event_id is null then raise exception 'invalid_matrix_send_receipt'; end if;
    perform 1 from public.matrix_transport_outbox
      where delivery_id = p_delivery_id and status = 'claimed' and destination = 'matrix-homeserver-v1'
        and lease_owner = p_owner and lease_fence = p_fence and lease_expires_at > clock_timestamp()
        and payload_sha256 = p_payload_hash and payload ->> 'room_id' = p_room_id for update;
    if not found then raise exception 'matrix_delivery_claim_mismatch'; end if;
    if not exists (select 1 from public.matrix_transport_send_bindings
        where delivery_id = p_delivery_id and payload_sha256 = p_payload_hash and room_id = p_room_id) then
        raise exception 'matrix_send_binding_required';
    end if;
    insert into public.matrix_transport_send_receipts (delivery_id, payload_sha256, room_id, event_id)
    values (p_delivery_id, p_payload_hash, p_room_id, p_event_id)
    on conflict (delivery_id) do nothing;
    if found then return 'recorded'; end if;
    select * into strict prior from public.matrix_transport_send_receipts where delivery_id = p_delivery_id;
    if prior.payload_sha256 is distinct from p_payload_hash or prior.room_id is distinct from p_room_id
       or prior.event_id is distinct from p_event_id then raise exception 'matrix_send_receipt_collision'; end if;
    return 'replay';
end;
$$;

create or replace function public.cex_matrix_guard_sent_receipt_v1()
returns trigger language plpgsql set search_path = pg_catalog, public as $$
begin
    if new.destination = 'matrix-homeserver-v1' and new.status = 'sent' and old.status <> 'sent' then
        if not exists (select 1 from public.matrix_transport_send_receipts r
            join public.matrix_transport_send_bindings b using (delivery_id)
            where r.delivery_id = new.delivery_id and r.payload_sha256 = new.payload_sha256
              and b.payload_sha256 = new.payload_sha256 and r.room_id = new.payload ->> 'room_id'
              and b.room_id = r.room_id) then
            raise exception 'matrix_send_receipt_required';
        end if;
    end if;
    return new;
end;
$$;

drop trigger if exists matrix_sent_receipt_guard_v1 on public.matrix_transport_outbox;
create trigger matrix_sent_receipt_guard_v1 before update on public.matrix_transport_outbox
for each row execute function public.cex_matrix_guard_sent_receipt_v1();

-- A crashed adapter claim may already have admitted a business effect. Unlike
-- Matrix transaction-ID sends, adapter exactly-once replay is not yet qualified.
-- Hold these claims instead of silently resending after lease expiry.
create or replace function public.cex_matrix_claim_delivery_v1(
    p_owner text, p_lease_seconds integer, p_limit integer
)
returns table (delivery_id uuid, source_event_id text, destination text, payload_sha256 text,
    payload jsonb, attempt_count integer, max_attempts integer, lease_fence bigint, lease_expires_at timestamptz)
language plpgsql set search_path = pg_catalog, public as $$
begin
    if p_owner is null or octet_length(p_owner) not between 1 and 256
       or p_lease_seconds is null or p_lease_seconds not between 5 and 3600
       or p_limit is null or p_limit not between 1 and 100 then
        raise exception 'invalid_matrix_delivery_claim';
    end if;
    with stale as (
        select o.delivery_id, o.lease_owner as previous_owner,
               case when o.destination = 'matrix-relay-adapter-v1'
                    then 'adapter_response_unknown_expired_claim' else 'lease_expired_retry_exhausted' end as failure
        from public.matrix_transport_outbox o
        where o.status = 'claimed' and o.lease_expires_at <= clock_timestamp()
          and (o.attempt_count >= o.max_attempts or o.destination = 'matrix-relay-adapter-v1')
        for update skip locked limit p_limit
    ), held as (
        update public.matrix_transport_outbox o set status = 'dead_letter', lease_owner = null,
            lease_expires_at = null, last_error_code = stale.failure, updated_at = clock_timestamp()
        from stale where o.delivery_id = stale.delivery_id
        returning o.delivery_id, o.lease_fence, stale.previous_owner, stale.failure
    )
    insert into public.matrix_transport_delivery_history
        (delivery_id, lease_fence, from_status, to_status, owner, error_code)
    select held.delivery_id, held.lease_fence, 'claimed', 'dead_letter', held.previous_owner, held.failure from held;

    return query
    with selected as (
        select o.delivery_id, o.status as previous_status from public.matrix_transport_outbox o
        where (o.status = 'pending' or (o.status = 'claimed' and o.lease_expires_at <= clock_timestamp()
            and o.destination <> 'matrix-relay-adapter-v1'))
          and o.attempt_count < o.max_attempts
        order by o.created_at, o.delivery_id for update skip locked limit p_limit
    ), claimed as (
        update public.matrix_transport_outbox o set status = 'claimed', attempt_count = o.attempt_count + 1,
            lease_owner = p_owner, lease_fence = o.lease_fence + 1,
            lease_expires_at = clock_timestamp() + make_interval(secs => p_lease_seconds),
            last_error_code = null, updated_at = clock_timestamp()
        from selected where o.delivery_id = selected.delivery_id
        returning o.delivery_id, o.source_event_id, o.destination, o.payload_sha256, o.payload,
                  o.attempt_count, o.max_attempts, o.lease_fence, o.lease_expires_at, selected.previous_status
    ), history as (
        insert into public.matrix_transport_delivery_history
            (delivery_id, lease_fence, from_status, to_status, owner)
        select c.delivery_id, c.lease_fence, c.previous_status, 'claimed', p_owner from claimed c
        returning matrix_transport_delivery_history.delivery_id
    )
    select c.delivery_id, c.source_event_id, c.destination, c.payload_sha256, c.payload,
           c.attempt_count, c.max_attempts, c.lease_fence, c.lease_expires_at
    from claimed c join history h on h.delivery_id = c.delivery_id;
end;
$$;

-- Preserve all new evidence rows. No blanket runtime privileges or definer
-- elevation is introduced; deployment roles require a separate reviewed grant.
drop trigger if exists matrix_poison_payloads_immutable on public.matrix_transport_poison_payloads;
create trigger matrix_poison_payloads_immutable before update or delete on public.matrix_transport_poison_payloads
for each row execute function public.cex_matrix_reject_immutable_mutation_v1();
drop trigger if exists matrix_cursor_history_immutable on public.matrix_transport_cursor_history;
create trigger matrix_cursor_history_immutable before update or delete on public.matrix_transport_cursor_history
for each row execute function public.cex_matrix_reject_immutable_mutation_v1();
drop trigger if exists matrix_send_bindings_immutable on public.matrix_transport_send_bindings;
create trigger matrix_send_bindings_immutable before update or delete on public.matrix_transport_send_bindings
for each row execute function public.cex_matrix_reject_immutable_mutation_v1();
drop trigger if exists matrix_send_receipts_immutable on public.matrix_transport_send_receipts;
create trigger matrix_send_receipts_immutable before update or delete on public.matrix_transport_send_receipts
for each row execute function public.cex_matrix_reject_immutable_mutation_v1();

revoke all on public.matrix_transport_poison_payloads, public.matrix_transport_cursor_history,
    public.matrix_transport_send_bindings, public.matrix_transport_send_receipts from public;
revoke all on function public.cex_matrix_renew_cursor_lease_v1(text,text,bigint,bigint,integer) from public;
revoke all on function public.cex_matrix_store_poison_payload_v1(text,text,text,text,jsonb) from public;
revoke all on function public.cex_matrix_guard_cursor_advance_v1() from public;
revoke all on function public.cex_matrix_append_cursor_history_v1() from public;
revoke all on function public.cex_matrix_bind_send_attempt_v1(uuid,text,bigint,text,text,text,text) from public;
revoke all on function public.cex_matrix_record_send_receipt_v1(uuid,text,bigint,text,text,text) from public;
revoke all on function public.cex_matrix_guard_sent_receipt_v1() from public;
revoke all on function public.cex_matrix_claim_delivery_v1(text,integer,integer) from public;

commit;
