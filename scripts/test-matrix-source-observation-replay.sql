-- Execute only on the disposable Matrix test database after migrations 0001/0002.
-- Every test row is rolled back. This is a behavioral SQL test, not production evidence.
begin;
DO $test$
declare
    event_id text := 'review-replay-' || md5(random()::text || clock_timestamp()::text);
    event_hash text := 'sha256:' || repeat('a', 64);
    changed_hash text := 'sha256:' || repeat('b', 64);
    result text;
    rejected boolean := false;
    observations bigint;
begin
    result := public.cex_matrix_accept_source_event_v1(event_id, event_hash, 'review-test', null);
    if result <> 'accepted' then raise exception 'first event was not accepted'; end if;
    result := public.cex_matrix_accept_source_event_v1(event_id, event_hash, 'review-test', null);
    if result <> 'replay' then raise exception 'null cursor replay failed'; end if;
    result := public.cex_matrix_accept_source_event_v1(event_id, event_hash, 'review-test', 'sync-2');
    if result <> 'replay' then raise exception 'changed observation cursor was treated as collision'; end if;
    result := public.cex_matrix_accept_source_event_v1(event_id, event_hash, 'review-test', 'sync-3');
    if result <> 'replay' then raise exception 'third observation failed'; end if;
    perform public.cex_matrix_accept_source_event_v1(event_id, event_hash, 'review-test', 'sync-2');
    select count(*) into observations from public.matrix_transport_source_observations
     where source_event_id = event_id;
    if observations <> 3 then raise exception 'observation replay is not idempotent'; end if;
    if not exists (select 1 from public.matrix_transport_inbox
                   where source_event_id = event_id and observed_cursor is null) then
        raise exception 'first observation in immutable inbox was rewritten';
    end if;
    begin
        perform public.cex_matrix_accept_source_event_v1(event_id, changed_hash, 'review-test', 'sync-4');
    exception when others then
        rejected := sqlerrm = 'matrix_source_event_identity_collision';
    end;
    if not rejected then raise exception 'changed content was accepted'; end if;
    rejected := false;
    begin
        perform public.cex_matrix_accept_source_event_v1(event_id, event_hash, 'other-stream', 'sync-4');
    exception when others then
        rejected := sqlerrm = 'matrix_source_event_identity_collision';
    end;
    if not rejected then raise exception 'cross-partition authority was accepted'; end if;
    rejected := false;
    begin
        update public.matrix_transport_source_observations set observed_cursor = 'forged'
         where source_event_id = event_id;
    exception when others then
        rejected := sqlerrm = 'matrix_immutable_history_mutation_rejected';
    end;
    if not rejected then raise exception 'immutable observation mutation was accepted'; end if;
    rejected := false;
    begin
        delete from public.matrix_transport_source_observations where source_event_id = event_id;
    exception when others then
        rejected := sqlerrm = 'matrix_immutable_history_mutation_rejected';
    end;
    if not rejected then raise exception 'immutable observation deletion was accepted'; end if;
end;
$test$;
rollback;
