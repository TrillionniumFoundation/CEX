begin;

-- The immutable source identity is the event, its content hash and its stream
-- partition. A cursor is an observation position, not part of that identity.
-- Keep every existing inbox row byte-for-byte unchanged and append observations.
-- Like the existing transport outbox, admission procedures establish the source
-- relationship. Runtime roles must not have DELETE/TRUNCATE or unrestricted DML.
-- There is no cascading deletion or rewrite of historical source observations.
create table if not exists public.matrix_transport_source_observations (
    observation_id bigserial primary key,
    source_event_id text not null,
    partition_id text not null,
    observed_cursor text,
    observed_at timestamptz not null default clock_timestamp(),
    check (octet_length(source_event_id) between 1 and 512),
    check (octet_length(partition_id) between 1 and 256),
    check (observed_cursor is null or octet_length(observed_cursor) <= 8192),
    unique nulls not distinct (source_event_id, partition_id, observed_cursor)
);

insert into public.matrix_transport_source_observations
    (source_event_id, partition_id, observed_cursor, observed_at)
select source_event_id, partition_id, observed_cursor, accepted_at
from public.matrix_transport_inbox
on conflict (source_event_id, partition_id, observed_cursor) do nothing;

drop trigger if exists matrix_transport_source_observations_immutable_v1
    on public.matrix_transport_source_observations;
create trigger matrix_transport_source_observations_immutable_v1
before update or delete on public.matrix_transport_source_observations
for each row execute function public.cex_matrix_reject_immutable_mutation_v1();

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
    disposition text;
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
        source_event_id, source_event_sha256, partition_id, observed_cursor
    ) values (
        p_source_event_id, p_source_event_sha256, p_partition_id, p_observed_cursor
    )
    on conflict (source_event_id) do nothing;

    if found then
        disposition := 'accepted';
    else
        select source_event_sha256, partition_id
          into existing_hash, existing_partition
          from public.matrix_transport_inbox
         where source_event_id = p_source_event_id;
        if existing_hash is distinct from p_source_event_sha256
           or existing_partition is distinct from p_partition_id then
            raise exception 'matrix_source_event_identity_collision';
        end if;
        disposition := 'replay';
    end if;

    insert into public.matrix_transport_source_observations (
        source_event_id, partition_id, observed_cursor
    ) values (
        p_source_event_id, p_partition_id, p_observed_cursor
    )
    on conflict (source_event_id, partition_id, observed_cursor) do nothing;
    return disposition;
end;
$$;

revoke all on table public.matrix_transport_source_observations from public;
revoke all on sequence public.matrix_transport_source_observations_observation_id_seq from public;
revoke all on function public.cex_matrix_accept_source_event_v1(text, text, text, text) from public;

commit;
