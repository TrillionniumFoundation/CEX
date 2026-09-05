begin;

-- Additive: keep the original ID-backed stream scope and cursor unchanged.
-- Definition text is the exact UTF-8 body accepted by the updated poller.
create table if not exists public.matrix_transport_filter_definitions (
    partition_id text primary key check (octet_length(partition_id) between 1 and 256),
    filter_id text not null check (octet_length(filter_id) between 1 and 1024),
    definition text not null check (
        octet_length(definition) between 2 and 4096
        and left(definition, 1) = '{'
        and jsonb_typeof(definition::jsonb) = 'object'
    ),
    definition_sha256 text not null check (
        definition_sha256 ~ '^sha256:[0-9a-f]{64}$'
        and definition_sha256 = 'sha256:' || encode(sha256(convert_to(definition, 'UTF8')), 'hex')
    ),
    origin text not null check (origin in ('bootstrap', 'reviewed_legacy')),
    cursor_revision_at_binding bigint not null check (cursor_revision_at_binding >= 0),
    cursor_fence_at_binding bigint not null check (cursor_fence_at_binding >= 0),
    approved_by text,
    approval_sha256 text,
    recorded_at timestamptz not null default clock_timestamp(),
    check (
        (origin = 'bootstrap' and approved_by is null and approval_sha256 is null)
        or (origin = 'reviewed_legacy' and approved_by is not null
            and octet_length(approved_by) between 1 and 256
            and approval_sha256 is not null and approval_sha256 ~ '^sha256:[0-9a-f]{64}$')
    )
);

create or replace function public.cex_matrix_guard_filter_definition_insert_v1()
returns trigger language plpgsql set search_path = pg_catalog, public as $$
declare cursor_row public.matrix_transport_cursors%rowtype;
        stream_scope jsonb;
        table_owner text;
begin
    select * into cursor_row from public.matrix_transport_cursors
        where partition_id = new.partition_id for update;
    if not found
        or new.cursor_revision_at_binding is distinct from cursor_row.cursor_revision
        or new.cursor_fence_at_binding is distinct from cursor_row.lease_fence then
        raise exception 'matrix_filter_definition_cursor_mismatch';
    end if;
    select scope into stream_scope from public.matrix_transport_stream_scopes
        where partition_id = new.partition_id;
    if not found or stream_scope->'filter'->>'kind' is distinct from 'id'
        or stream_scope->'filter'->>'value' is distinct from new.filter_id then
        raise exception 'matrix_filter_definition_scope_mismatch';
    end if;
    if new.origin = 'bootstrap' then
        if cursor_row.cursor_revision <> 0 or cursor_row.opaque_cursor is not null
            or cursor_row.lease_owner is null or cursor_row.lease_expires_at is null
            or cursor_row.lease_expires_at <= clock_timestamp()
            or exists (select 1 from public.matrix_transport_inbox where partition_id = new.partition_id)
            or exists (select 1 from public.matrix_transport_cursor_history where partition_id = new.partition_id)
            or exists (select 1 from public.matrix_transport_poison_events where partition_id = new.partition_id)
        then
            raise exception 'matrix_filter_definition_legacy_review_required';
        end if;
    else
        select pg_get_userbyid(relowner) into table_owner from pg_class
            where oid = 'public.matrix_transport_filter_definitions'::regclass;
        if current_user <> table_owner or new.approved_by is distinct from current_user::text then
            raise exception using errcode = '42501', message = 'matrix_filter_definition_owner_required';
        end if;
        if cursor_row.lease_expires_at > clock_timestamp() then
            raise exception 'matrix_filter_definition_active_lease';
        end if;
    end if;
    return new;
end;
$$;

drop trigger if exists matrix_transport_filter_definition_insert_v1 on public.matrix_transport_filter_definitions;
create trigger matrix_transport_filter_definition_insert_v1 before insert on public.matrix_transport_filter_definitions
    for each row execute function public.cex_matrix_guard_filter_definition_insert_v1();
drop trigger if exists matrix_transport_filter_definition_immutable_v1 on public.matrix_transport_filter_definitions;
create trigger matrix_transport_filter_definition_immutable_v1 before update or delete on public.matrix_transport_filter_definitions
    for each row execute function public.cex_matrix_reject_immutable_mutation_v1();

create or replace function public.cex_matrix_bind_filter_definition_v1(
    p_partition_id text, p_owner text, p_fence bigint, p_revision bigint,
    p_filter_id text, p_definition text, p_sha256 text
)
returns text language plpgsql set search_path = pg_catalog, public as $$
declare cursor_row public.matrix_transport_cursors%rowtype;
        bound public.matrix_transport_filter_definitions%rowtype;
        stream_scope jsonb;
begin
    select * into cursor_row from public.matrix_transport_cursors
        where partition_id = p_partition_id for update;
    if not found or p_owner is null or p_fence is null or p_revision is null
        or p_filter_id is null or p_definition is null or p_sha256 is null
        or cursor_row.lease_owner is distinct from p_owner
        or cursor_row.lease_fence is distinct from p_fence
        or cursor_row.cursor_revision is distinct from p_revision
        or cursor_row.lease_expires_at is null or cursor_row.lease_expires_at <= clock_timestamp()
    then
        raise exception 'matrix_cursor_lease_or_revision_mismatch';
    end if;
    select scope into stream_scope from public.matrix_transport_stream_scopes
        where partition_id = p_partition_id;
    if not found or stream_scope->'filter'->>'kind' is distinct from 'id'
        or stream_scope->'filter'->>'value' is distinct from p_filter_id then
        raise exception 'matrix_filter_definition_scope_mismatch';
    end if;
    select * into bound from public.matrix_transport_filter_definitions where partition_id = p_partition_id;
    if found then
        if bound.filter_id is distinct from p_filter_id
            or bound.definition is distinct from p_definition
            or bound.definition_sha256 is distinct from p_sha256 then
            raise exception 'matrix_filter_definition_mismatch';
        end if;
        return 'replay';
    end if;
    -- CHECK and INSERT trigger also protect callers with direct INSERT grants.
    insert into public.matrix_transport_filter_definitions
        (partition_id, filter_id, definition, definition_sha256, origin,
         cursor_revision_at_binding, cursor_fence_at_binding)
    values (p_partition_id, p_filter_id, p_definition, p_sha256, 'bootstrap', p_revision, p_fence);
    return 'bound';
end;
$$;

create or replace function public.cex_matrix_approve_legacy_filter_definition_v1(
    p_partition_id text, p_expected_revision bigint, p_expected_cursor text,
    p_filter_id text, p_definition text, p_sha256 text, p_approval_sha256 text
)
returns text language plpgsql set search_path = pg_catalog, public as $$
declare cursor_row public.matrix_transport_cursors%rowtype;
        table_owner text;
begin
    select pg_get_userbyid(relowner) into table_owner from pg_class
        where oid = 'public.matrix_transport_filter_definitions'::regclass;
    if current_user <> table_owner then
        raise exception using errcode = '42501', message = 'matrix_filter_definition_owner_required';
    end if;
    select * into cursor_row from public.matrix_transport_cursors
        where partition_id = p_partition_id for update;
    if not found or p_expected_revision is null
        or cursor_row.cursor_revision is distinct from p_expected_revision
        or cursor_row.opaque_cursor is distinct from p_expected_cursor then
        raise exception 'matrix_filter_definition_cursor_mismatch';
    end if;
    -- INSERT trigger rejects live leases. Unique identity prohibits a repin.
    insert into public.matrix_transport_filter_definitions
        (partition_id, filter_id, definition, definition_sha256, origin,
         cursor_revision_at_binding, cursor_fence_at_binding, approved_by, approval_sha256)
    values (p_partition_id, p_filter_id, p_definition, p_sha256, 'reviewed_legacy',
        cursor_row.cursor_revision, cursor_row.lease_fence, current_user, p_approval_sha256);
    return 'approved';
end;
$$;

revoke all on public.matrix_transport_filter_definitions from public;
revoke all on function public.cex_matrix_guard_filter_definition_insert_v1() from public;
revoke all on function public.cex_matrix_bind_filter_definition_v1(text,text,bigint,bigint,text,text,text) from public;
revoke all on function public.cex_matrix_approve_legacy_filter_definition_v1(text,bigint,text,text,text,text,text) from public;

commit;
