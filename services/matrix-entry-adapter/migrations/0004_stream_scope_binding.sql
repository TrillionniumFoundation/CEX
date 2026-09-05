begin;

-- No cursor or historical source bytes are rewritten by this upgrade.
create table if not exists public.matrix_transport_stream_scopes (
    partition_id text primary key check (octet_length(partition_id) between 1 and 256),
    scope jsonb not null check (
        jsonb_typeof(scope) = 'object'
        and scope ?& array['schema', 'homeserver', 'bot_user_id', 'filter']
        and scope - array['schema', 'homeserver', 'bot_user_id', 'filter'] = '{}'::jsonb
        and scope->'schema' = '"cex.matrix.stream-scope.v1"'::jsonb
        and jsonb_typeof(scope->'homeserver') = 'string'
        and octet_length(scope->>'homeserver') between 1 and 2048
        and jsonb_typeof(scope->'bot_user_id') = 'string'
        and octet_length(scope->>'bot_user_id') between 1 and 512
        and jsonb_typeof(scope->'filter') = 'object'
        and (
            scope->'filter' = '{"kind":"none"}'::jsonb
            or (
                (scope->'filter') ?& array['kind', 'value']
                and (scope->'filter') - array['kind', 'value'] = '{}'::jsonb
                and scope->'filter'->'kind' in ('"inline"'::jsonb, '"id"'::jsonb)
                and jsonb_typeof(scope->'filter'->'value') = 'string'
                and octet_length(scope->'filter'->>'value') between 1 and 4096
            )
        )
        and pg_column_size(scope) <= 16384
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

create or replace function public.cex_matrix_guard_stream_scope_insert_v1()
returns trigger language plpgsql set search_path = pg_catalog, public as $$
declare cursor_row public.matrix_transport_cursors%rowtype;
        scope_owner text;
begin
    select * into cursor_row from public.matrix_transport_cursors
        where partition_id = new.partition_id for update;
    if not found or new.cursor_revision_at_binding <> cursor_row.cursor_revision
        or new.cursor_fence_at_binding <> cursor_row.lease_fence then
        raise exception 'matrix_stream_scope_cursor_mismatch';
    end if;
    if new.origin = 'bootstrap' then
        if cursor_row.cursor_revision <> 0 or cursor_row.opaque_cursor is not null
            or cursor_row.lease_owner is null
            or cursor_row.lease_expires_at <= clock_timestamp()
            or exists (select 1 from public.matrix_transport_inbox where partition_id = new.partition_id)
            or exists (select 1 from public.matrix_transport_cursor_history where partition_id = new.partition_id)
            or exists (select 1 from public.matrix_transport_poison_events where partition_id = new.partition_id)
        then
            raise exception 'matrix_stream_scope_legacy_review_required';
        end if;
    else
        select pg_get_userbyid(relowner) into scope_owner from pg_class
            where oid = 'public.matrix_transport_stream_scopes'::regclass;
        if current_user <> scope_owner or new.approved_by is distinct from current_user::text then
            raise exception using errcode = '42501', message = 'matrix_stream_scope_owner_required';
        end if;
        if cursor_row.lease_expires_at > clock_timestamp() then
            raise exception 'matrix_stream_scope_active_lease';
        end if;
    end if;
    return new;
end;
$$;

drop trigger if exists matrix_transport_stream_scope_insert_v1 on public.matrix_transport_stream_scopes;
create trigger matrix_transport_stream_scope_insert_v1 before insert on public.matrix_transport_stream_scopes
    for each row execute function public.cex_matrix_guard_stream_scope_insert_v1();
drop trigger if exists matrix_transport_stream_scope_immutable_v1 on public.matrix_transport_stream_scopes;
create trigger matrix_transport_stream_scope_immutable_v1 before update or delete on public.matrix_transport_stream_scopes
    for each row execute function public.cex_matrix_reject_immutable_mutation_v1();

create or replace function public.cex_matrix_bind_stream_scope_v1(
    p_partition_id text, p_owner text, p_fence bigint, p_revision bigint, p_scope jsonb
)
returns text language plpgsql set search_path = pg_catalog, public as $$
declare cursor_row public.matrix_transport_cursors%rowtype;
        bound_scope jsonb;
begin
    select * into cursor_row from public.matrix_transport_cursors
        where partition_id = p_partition_id for update;
    if not found or p_owner is null or p_fence is null or p_revision is null or p_scope is null
        or cursor_row.lease_owner is distinct from p_owner
        or cursor_row.lease_fence is distinct from p_fence
        or cursor_row.cursor_revision is distinct from p_revision
        or cursor_row.lease_expires_at is null or cursor_row.lease_expires_at <= clock_timestamp()
    then
        raise exception 'matrix_cursor_lease_or_revision_mismatch';
    end if;
    select scope into bound_scope from public.matrix_transport_stream_scopes where partition_id = p_partition_id;
    if found then
        if bound_scope is distinct from p_scope then
            raise exception 'matrix_stream_scope_mismatch';
        end if;
        return 'replay';
    end if;
    -- The INSERT trigger also enforces the virgin-stream rule for direct writes.
    insert into public.matrix_transport_stream_scopes
        (partition_id, scope, origin, cursor_revision_at_binding, cursor_fence_at_binding)
    values (p_partition_id, p_scope, 'bootstrap', p_revision, p_fence);
    return 'bound';
end;
$$;

create or replace function public.cex_matrix_approve_legacy_stream_scope_v1(
    p_partition_id text, p_expected_revision bigint, p_expected_cursor text,
    p_scope jsonb, p_approval_sha256 text
)
returns text language plpgsql set search_path = pg_catalog, public as $$
declare cursor_row public.matrix_transport_cursors%rowtype;
        scope_owner text;
begin
    select pg_get_userbyid(relowner) into scope_owner from pg_class
        where oid = 'public.matrix_transport_stream_scopes'::regclass;
    if current_user <> scope_owner then
        raise exception using errcode = '42501', message = 'matrix_stream_scope_owner_required';
    end if;
    select * into cursor_row from public.matrix_transport_cursors
        where partition_id = p_partition_id for update;
    if not found or p_expected_revision is null or p_scope is null
        or cursor_row.cursor_revision is distinct from p_expected_revision
        or cursor_row.opaque_cursor is distinct from p_expected_cursor then
        raise exception 'matrix_stream_scope_cursor_mismatch';
    end if;
    -- Unique identity and INSERT trigger prohibit rebinding and live-worker approval.
    insert into public.matrix_transport_stream_scopes
        (partition_id, scope, origin, cursor_revision_at_binding, cursor_fence_at_binding,
         approved_by, approval_sha256)
    values (p_partition_id, p_scope, 'reviewed_legacy', cursor_row.cursor_revision,
        cursor_row.lease_fence, current_user, p_approval_sha256);
    return 'approved';
end;
$$;

revoke all on public.matrix_transport_stream_scopes from public;
revoke all on function public.cex_matrix_guard_stream_scope_insert_v1() from public;
revoke all on function public.cex_matrix_bind_stream_scope_v1(text,text,bigint,bigint,jsonb) from public;
revoke all on function public.cex_matrix_approve_legacy_stream_scope_v1(text,bigint,text,jsonb,text) from public;

commit;
