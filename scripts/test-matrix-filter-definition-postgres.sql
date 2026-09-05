-- Complete 0001..0005 chain, isolated PostgreSQL 16 only. Never a live database.
begin;
DO $test$
declare scope jsonb := '{"schema":"cex.matrix.stream-scope.v1","homeserver":"https://matrix.example/proxy","bot_user_id":"@bot:example","filter":{"kind":"id","value":"0"}}';
        c public.matrix_transport_cursors%rowtype;
        def text := '{"room":{"timeline":{"senders":["@allowed:example"]}}}';
        hash text;
        changed text := '{}';
        changed_hash text;
        result text;
begin
    hash := 'sha256:' || encode(sha256(convert_to(def, 'UTF8')), 'hex');
    changed_hash := 'sha256:' || encode(sha256(convert_to(changed, 'UTF8')), 'hex');
    perform public.cex_matrix_acquire_cursor_lease_v1('filter-pin-fresh', 'pin-worker', 60);
    select * into strict c from public.matrix_transport_cursors where partition_id = 'filter-pin-fresh';
    perform public.cex_matrix_bind_stream_scope_v1(c.partition_id, 'pin-worker', c.lease_fence, 0, scope);
    result := public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'pin-worker', c.lease_fence, 0, '0', def, hash);
    if result <> 'bound' then raise exception 'fresh definition did not bind'; end if;
    result := public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'pin-worker', c.lease_fence, 0, '0', def, hash);
    if result <> 'replay' then raise exception 'same definition was not an exact replay'; end if;
    if (select cursor_revision from public.matrix_transport_cursors where partition_id = c.partition_id) <> 0 then
        raise exception 'definition binding advanced cursor';
    end if;

    begin
        perform public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'pin-worker', c.lease_fence, 0, '0', changed, changed_hash);
        raise exception 'changed definition reused ID-backed cursor';
    exception when others then
        if sqlerrm <> 'matrix_filter_definition_mismatch' then raise; end if;
    end;
    begin
        perform public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'other-worker', c.lease_fence, 0, '0', def, hash);
        raise exception 'wrong worker bound definition';
    exception when others then
        if sqlerrm <> 'matrix_cursor_lease_or_revision_mismatch' then raise; end if;
    end;
    begin
        perform public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'pin-worker', c.lease_fence + 1, 0, '0', def, hash);
        raise exception 'wrong fence bound definition';
    exception when others then
        if sqlerrm <> 'matrix_cursor_lease_or_revision_mismatch' then raise; end if;
    end;
    begin
        perform public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'pin-worker', c.lease_fence, 1, '0', def, hash);
        raise exception 'wrong revision bound definition';
    exception when others then
        if sqlerrm <> 'matrix_cursor_lease_or_revision_mismatch' then raise; end if;
    end;
    begin
        perform public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'pin-worker', c.lease_fence, 0, '1', def, hash);
        raise exception 'definition ID escaped stream binding';
    exception when others then
        if sqlerrm <> 'matrix_filter_definition_scope_mismatch' then raise; end if;
    end;
    begin
        update public.matrix_transport_filter_definitions set definition = changed where partition_id = c.partition_id;
        raise exception 'stored definition was mutable';
    exception when others then
        if sqlerrm <> 'matrix_immutable_history_mutation_rejected' then raise; end if;
    end;
    begin
        delete from public.matrix_transport_filter_definitions where partition_id = c.partition_id;
        raise exception 'stored definition could be deleted';
    exception when others then
        if sqlerrm <> 'matrix_immutable_history_mutation_rejected' then raise; end if;
    end;

    perform public.cex_matrix_advance_cursor_v1(c.partition_id, 'pin-worker', c.lease_fence, 0, 'pinned-next');
    result := public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'pin-worker', c.lease_fence, 1, '0', def, hash);
    if result <> 'replay' then raise exception 'pin did not survive cursor advance'; end if;

    perform public.cex_matrix_acquire_cursor_lease_v1('filter-pin-invalid', 'pin-worker', 60);
    select * into strict c from public.matrix_transport_cursors where partition_id = 'filter-pin-invalid';
    perform public.cex_matrix_bind_stream_scope_v1(c.partition_id, 'pin-worker', c.lease_fence, 0, scope);
    begin
        perform public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'pin-worker', c.lease_fence, 0, '0', def, changed_hash);
        raise exception 'database accepted digest without matching bytes';
    exception when check_violation then null;
    end;
    if exists (select 1 from public.matrix_transport_filter_definitions where partition_id = c.partition_id) then
        raise exception 'rejected definition left a durable pin';
    end if;
    begin
        perform public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'pin-worker', c.lease_fence, 0, '0', null, hash);
        raise exception 'NULL definition was accepted';
    exception when others then
        if sqlerrm <> 'matrix_cursor_lease_or_revision_mismatch' then raise; end if;
    end;
    begin
        insert into public.matrix_transport_filter_definitions
            (partition_id,filter_id,definition,definition_sha256,origin,cursor_revision_at_binding,cursor_fence_at_binding)
        values (c.partition_id,'0','null','sha256:' || encode(sha256(convert_to('null','UTF8')),'hex'),'bootstrap',0,c.lease_fence);
        raise exception 'direct insert accepted non-object definition';
    exception when check_violation then null;
    end;

    -- Legacy ID scope: do not infer old content from today's endpoint or env.
    perform public.cex_matrix_acquire_cursor_lease_v1('filter-pin-legacy', 'pin-worker', 60);
    select * into strict c from public.matrix_transport_cursors where partition_id = 'filter-pin-legacy';
    perform public.cex_matrix_bind_stream_scope_v1(c.partition_id, 'pin-worker', c.lease_fence, 0, scope);
    perform public.cex_matrix_advance_cursor_v1(c.partition_id, 'pin-worker', c.lease_fence, 0, 'old-opaque');
    begin
        perform public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'pin-worker', c.lease_fence, 1, '0', def, hash);
        raise exception 'legacy definition was auto-approved';
    exception when others then
        if sqlerrm <> 'matrix_filter_definition_legacy_review_required' then raise; end if;
    end;
    begin
        perform public.cex_matrix_approve_legacy_filter_definition_v1(c.partition_id, 1, 'old-opaque', '0', def, hash, 'sha256:' || repeat('a',64));
        raise exception 'live worker definition was approved';
    exception when others then
        if sqlerrm <> 'matrix_filter_definition_active_lease' then raise; end if;
    end;
    update public.matrix_transport_cursors set lease_expires_at = clock_timestamp() - interval '1 second'
        where partition_id = c.partition_id;
    begin
        perform public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'pin-worker', c.lease_fence, 1, '0', def, hash);
        raise exception 'expired lease bound definition';
    exception when others then
        if sqlerrm <> 'matrix_cursor_lease_or_revision_mismatch' then raise; end if;
    end;
    begin
        perform public.cex_matrix_approve_legacy_filter_definition_v1(c.partition_id, 1, 'wrong-opaque', '0', def, hash, 'sha256:' || repeat('a',64));
        raise exception 'wrong legacy cursor was approved for definition';
    exception when others then
        if sqlerrm <> 'matrix_filter_definition_cursor_mismatch' then raise; end if;
    end;
    result := public.cex_matrix_approve_legacy_filter_definition_v1(c.partition_id, 1, 'old-opaque', '0', def, hash, 'sha256:' || repeat('a',64));
    if result <> 'approved' then raise exception 'reviewed legacy pin was not recorded'; end if;
    if (select opaque_cursor from public.matrix_transport_cursors where partition_id = c.partition_id) <> 'old-opaque'
        or (select origin from public.matrix_transport_filter_definitions where partition_id = c.partition_id) <> 'reviewed_legacy' then
        raise exception 'pin approval changed cursor or lost provenance';
    end if;
    perform public.cex_matrix_acquire_cursor_lease_v1(c.partition_id, 'restart-worker', 60);
    select * into strict c from public.matrix_transport_cursors where partition_id = 'filter-pin-legacy';
    result := public.cex_matrix_bind_filter_definition_v1(c.partition_id, 'restart-worker', c.lease_fence, 1, '0', def, hash);
    if result <> 'replay' then raise exception 'approved definition did not replay after restart'; end if;
end;
$test$;

-- EXECUTE alone never authorizes a runtime role to review historical scope.
create role cex_matrix_filter_runtime_fixture nologin;
grant usage on schema public to cex_matrix_filter_runtime_fixture;
grant execute on function public.cex_matrix_approve_legacy_filter_definition_v1(text,bigint,text,text,text,text,text)
    to cex_matrix_filter_runtime_fixture;
set local role cex_matrix_filter_runtime_fixture;
DO $test$
begin
    begin
        perform public.cex_matrix_approve_legacy_filter_definition_v1('filter-pin-invalid', 0, null, '0', '{}', 'sha256:' || repeat('a',64), 'sha256:' || repeat('b',64));
        raise exception 'runtime role approved a legacy definition';
    exception when insufficient_privilege then
        if sqlerrm <> 'matrix_filter_definition_owner_required' then raise; end if;
    end;
end;
$test$;
reset role;
rollback;
