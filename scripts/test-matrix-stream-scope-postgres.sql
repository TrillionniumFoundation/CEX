-- Run against the complete 0001..0004 chain in a disposable PostgreSQL 16 DB.
-- These assertions do not call a homeserver or establish deployment privileges.
begin;

DO $test$
declare scope jsonb := '{"schema":"cex.matrix.stream-scope.v1","homeserver":"https://matrix.example/proxy","bot_user_id":"@bot:example","filter":{"kind":"id","value":"0"}}';
        cursor_row public.matrix_transport_cursors%rowtype;
        result text;
begin
    perform public.cex_matrix_acquire_cursor_lease_v1('scope-fresh', 'scope-worker', 60);
    select * into strict cursor_row from public.matrix_transport_cursors where partition_id = 'scope-fresh';
    result := public.cex_matrix_bind_stream_scope_v1('scope-fresh', 'scope-worker', cursor_row.lease_fence, 0, scope);
    if result <> 'bound' then raise exception 'fresh stream did not bind'; end if;
    result := public.cex_matrix_bind_stream_scope_v1('scope-fresh', 'scope-worker', cursor_row.lease_fence, 0, scope);
    if result <> 'replay' then raise exception 'same scope was not an exact replay'; end if;
    if (select cursor_revision from public.matrix_transport_cursors where partition_id = 'scope-fresh') <> 0 then
        raise exception 'binding advanced the cursor';
    end if;

    begin
        perform public.cex_matrix_bind_stream_scope_v1('scope-fresh', 'scope-worker', cursor_row.lease_fence, 0,
            jsonb_set(scope, '{filter,value}', '"changed"'));
        raise exception 'changed filter reused cursor';
    exception when others then
        if sqlerrm <> 'matrix_stream_scope_mismatch' then raise; end if;
    end;
    begin
        perform public.cex_matrix_bind_stream_scope_v1('scope-fresh', 'wrong-owner', cursor_row.lease_fence, 0, scope);
        raise exception 'wrong owner bound scope';
    exception when others then
        if sqlerrm <> 'matrix_cursor_lease_or_revision_mismatch' then raise; end if;
    end;
    begin
        update public.matrix_transport_stream_scopes as stored set scope = jsonb_set(stored.scope, '{homeserver}', '"https://other.example"')
            where partition_id = 'scope-fresh';
        raise exception 'scope was mutable';
    exception when others then
        if sqlerrm <> 'matrix_immutable_history_mutation_rejected' then raise; end if;
    end;
    begin
        delete from public.matrix_transport_stream_scopes where partition_id = 'scope-fresh';
        raise exception 'scope evidence was deleted';
    exception when others then
        if sqlerrm <> 'matrix_immutable_history_mutation_rejected' then raise; end if;
    end;

    -- Manufacture legacy history using the unchanged old cursor API. The
    -- migration must not infer its historical account/filter from current env.
    perform public.cex_matrix_acquire_cursor_lease_v1('scope-legacy', 'scope-worker', 60);
    select * into strict cursor_row from public.matrix_transport_cursors where partition_id = 'scope-legacy';
    perform public.cex_matrix_advance_cursor_v1('scope-legacy', 'scope-worker', cursor_row.lease_fence, 0, 'opaque-legacy');
    begin
        perform public.cex_matrix_bind_stream_scope_v1('scope-legacy', 'scope-worker', cursor_row.lease_fence, 1, scope);
        raise exception 'legacy cursor was auto-approved';
    exception when others then
        if sqlerrm <> 'matrix_stream_scope_legacy_review_required' then raise; end if;
    end;
    begin
        perform public.cex_matrix_approve_legacy_stream_scope_v1('scope-legacy', 1, 'opaque-legacy', scope, 'sha256:' || repeat('a',64));
        raise exception 'live worker scope was approved';
    exception when others then
        if sqlerrm <> 'matrix_stream_scope_active_lease' then raise; end if;
    end;
    update public.matrix_transport_cursors set lease_expires_at = clock_timestamp() - interval '1 second'
        where partition_id = 'scope-legacy';
    begin
        perform public.cex_matrix_approve_legacy_stream_scope_v1('scope-legacy', 1, 'wrong-cursor', scope, 'sha256:' || repeat('a',64));
        raise exception 'wrong legacy cursor was approved';
    exception when others then
        if sqlerrm <> 'matrix_stream_scope_cursor_mismatch' then raise; end if;
    end;
    perform public.cex_matrix_approve_legacy_stream_scope_v1('scope-legacy', 1, 'opaque-legacy', scope, 'sha256:' || repeat('a',64));
    if (select origin from public.matrix_transport_stream_scopes where partition_id = 'scope-legacy') <> 'reviewed_legacy'
        or (select opaque_cursor from public.matrix_transport_cursors where partition_id = 'scope-legacy') <> 'opaque-legacy' then
        raise exception 'review changed legacy position or lost provenance';
    end if;
    perform public.cex_matrix_acquire_cursor_lease_v1('scope-legacy', 'scope-new-worker', 60);
    select * into strict cursor_row from public.matrix_transport_cursors where partition_id = 'scope-legacy';
    perform public.cex_matrix_bind_stream_scope_v1('scope-legacy', 'scope-new-worker', cursor_row.lease_fence, 1, scope);

    -- NULL schema/kind fields must fail a CHECK, not pass through SQL NULL logic.
    perform public.cex_matrix_acquire_cursor_lease_v1('scope-malformed', 'scope-worker', 60);
    select * into strict cursor_row from public.matrix_transport_cursors where partition_id = 'scope-malformed';
    begin
        perform public.cex_matrix_bind_stream_scope_v1('scope-malformed', 'scope-worker', cursor_row.lease_fence, 0,
            jsonb_set(scope, '{schema}', 'null'));
        raise exception 'NULL schema bypassed scope shape';
    exception when check_violation then null;
    end;
    begin
        perform public.cex_matrix_bind_stream_scope_v1('scope-malformed', 'scope-worker', cursor_row.lease_fence, 0,
            jsonb_set(scope, '{filter,kind}', 'null'));
        raise exception 'NULL filter kind bypassed scope shape';
    exception when check_violation then null;
    end;
end;
$test$;

-- Even an accidental EXECUTE grant cannot turn a non-owner into a scope approver.
-- This role exists only inside the transaction and is removed by ROLLBACK.
create role cex_matrix_scope_runtime_fixture nologin;
grant usage on schema public to cex_matrix_scope_runtime_fixture;
grant execute on function public.cex_matrix_approve_legacy_stream_scope_v1(text,bigint,text,jsonb,text)
    to cex_matrix_scope_runtime_fixture;
set local role cex_matrix_scope_runtime_fixture;
DO $test$
begin
    begin
        perform public.cex_matrix_approve_legacy_stream_scope_v1('scope-malformed', 0, null, '{}'::jsonb, 'sha256:' || repeat('a',64));
        raise exception 'runtime role approved a legacy scope';
    exception when insufficient_privilege then
        if sqlerrm <> 'matrix_stream_scope_owner_required' then raise; end if;
    end;
end;
$test$;
reset role;
rollback;
