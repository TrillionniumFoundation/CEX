-- Disposable database only. The wrapper requires explicit reset consent.
begin;
truncate public.matrix_transport_send_receipts, public.matrix_transport_send_bindings,
    public.matrix_transport_poison_payloads, public.matrix_transport_cursor_history,
    public.matrix_transport_source_observations, public.matrix_transport_delivery_history,
    public.matrix_transport_outbox, public.matrix_transport_inbox,
    public.matrix_transport_poison_events, public.matrix_transport_cursors restart identity;

do $test$
declare
    a record;
    b record;
    claim record;
    recovered record;
    rejected boolean;
    result text;
    h text := 'sha256:' || repeat('a',64);
    h2 text := 'sha256:' || repeat('b',64);
    token_hash text := 'sha256:' || repeat('c',64);
    token2 text := 'sha256:' || repeat('d',64);
    adapter_id uuid := '00000000-0000-4000-8000-000000000010';
    send_id uuid := '00000000-0000-4000-8000-000000000011';
    retry_id uuid := '00000000-0000-4000-8000-000000000012';
begin
    select * into strict a from public.cex_matrix_acquire_cursor_lease_v1('sync-test','poll-a',60);
    if not public.cex_matrix_renew_cursor_lease_v1('sync-test','poll-a',a.lease_fence,0,60) then
        raise exception 'owner renewal failed';
    end if;
    if public.cex_matrix_renew_cursor_lease_v1('sync-test','wrong',a.lease_fence,0,60)
       or public.cex_matrix_renew_cursor_lease_v1('sync-test','poll-a',a.lease_fence+1,0,60)
       or public.cex_matrix_renew_cursor_lease_v1('sync-test','poll-a',a.lease_fence,1,60) then
        raise exception 'renewal accepted wrong owner fence or revision';
    end if;

    perform public.cex_matrix_record_poison_event_v1('poison-sync',h,'sync-test','missing_message_body');
    result := public.cex_matrix_store_poison_payload_v1('poison-sync',h,'sync-test','!room:e','{"type":"m.room.message"}');
    if result <> 'stored' then raise exception 'poison bytes not stored'; end if;
    result := public.cex_matrix_store_poison_payload_v1('poison-sync',h,'sync-test','!room:e','{"type":"m.room.message"}');
    if result <> 'replay' then raise exception 'poison exact replay failed'; end if;
    rejected := false;
    begin
        perform public.cex_matrix_store_poison_payload_v1('poison-sync',h,'sync-test','!room:e','{"changed":true}');
    exception when others then rejected := sqlerrm = 'matrix_poison_payload_collision'; end;
    if not rejected then raise exception 'poison byte mutation accepted'; end if;
    rejected := false;
    begin
        perform public.cex_matrix_advance_cursor_v1('sync-test','poll-a',a.lease_fence,0,'next-1');
    exception when others then rejected := sqlerrm = 'matrix_poison_requires_operator_quarantine'; end;
    if not rejected then raise exception 'unacknowledged poison advanced cursor'; end if;
    if not exists (select 1 from public.matrix_transport_poison_payloads where source_event_id = 'poison-sync')
       or exists (select 1 from public.matrix_transport_cursor_history where partition_id = 'sync-test') then
        raise exception 'poison bytes lost or blocked cursor history fabricated';
    end if;
    perform public.cex_matrix_acknowledge_poison_event_v1('poison-sync',h,'test-operator','explicit quarantine, not reprocessing');
    if not public.cex_matrix_advance_cursor_v1('sync-test','poll-a',a.lease_fence,0,'next-1') then
        raise exception 'acknowledged quarantine did not permit separate batch advancement';
    end if;
    if not exists (select 1 from public.matrix_transport_cursor_history
        where partition_id = 'sync-test' and cursor_revision = 1 and previous_cursor is null and next_cursor = 'next-1') then
        raise exception 'cursor history missing';
    end if;
    update public.matrix_transport_cursors set lease_expires_at = clock_timestamp()-interval '1 second'
        where partition_id = 'sync-test';
    if public.cex_matrix_renew_cursor_lease_v1('sync-test','poll-a',a.lease_fence,1,60) then
        raise exception 'expired lease resurrected';
    end if;
    select * into strict b from public.cex_matrix_acquire_cursor_lease_v1('sync-test','poll-b',60);
    if b.lease_fence <= a.lease_fence or b.cursor_revision <> 1 then raise exception 'takeover lost identity'; end if;

    perform public.cex_matrix_accept_source_event_v1('adapter-e',h,'sync-test','next-1');
    perform public.cex_matrix_enqueue_delivery_v1(adapter_id,'adapter-e','matrix-relay-adapter-v1',h,'{"event_id":"adapter-e"}',3);
    select * into strict claim from public.cex_matrix_claim_delivery_v1('relay-a',60,1);
    if claim.delivery_id <> adapter_id then raise exception 'wrong initial claim'; end if;
    update public.matrix_transport_outbox set lease_expires_at = clock_timestamp()-interval '1 second' where delivery_id = adapter_id;
    if exists(select 1 from public.cex_matrix_claim_delivery_v1('relay-b',60,1)) then
        raise exception 'expired adapter claim was blindly resent';
    end if;
    if not exists(select 1 from public.matrix_transport_outbox where delivery_id = adapter_id
        and status = 'dead_letter' and last_error_code = 'adapter_response_unknown_expired_claim') then
        raise exception 'adapter unknown outcome not retained';
    end if;
    if not exists(select 1 from public.matrix_transport_delivery_history where delivery_id = adapter_id
        and to_status = 'dead_letter' and owner = 'relay-a') then raise exception 'previous claim owner lost'; end if;

    perform public.cex_matrix_accept_source_event_v1('send-e',h,'sync-test','next-1');
    perform public.cex_matrix_enqueue_delivery_v1(send_id,'send-e','matrix-homeserver-v1',h,
        '{"room_id":"!room:e","projected_reply":{"msgtype":"m.text","body":"hello"}}',3);
    select * into strict claim from public.cex_matrix_claim_delivery_v1('relay-a',60,1);
    rejected := false;
    begin
        perform public.cex_matrix_finish_delivery_v1(send_id,'relay-a',claim.lease_fence,'sent',null);
    exception when others then rejected := sqlerrm = 'matrix_send_receipt_required'; end;
    if not rejected then raise exception 'send succeeded without receipt'; end if;
    rejected := false;
    begin
        perform public.cex_matrix_record_send_receipt_v1(send_id,'relay-a',claim.lease_fence,h,'!room:e','$receipt');
    exception when others then rejected := sqlerrm = 'matrix_send_binding_required'; end;
    if not rejected then raise exception 'receipt accepted without idempotency binding'; end if;
    rejected := false;
    begin
        perform public.cex_matrix_bind_send_attempt_v1(send_id,'wrong',claim.lease_fence,h,'!room:e','https://matrix.example',token_hash);
    exception when others then rejected := sqlerrm = 'matrix_delivery_claim_mismatch'; end;
    if not rejected then raise exception 'wrong owner bound send'; end if;
    result := public.cex_matrix_bind_send_attempt_v1(send_id,'relay-a',claim.lease_fence,h,'!room:e','https://matrix.example',token_hash);
    if result <> 'bound' then raise exception 'send binding failed'; end if;
    result := public.cex_matrix_bind_send_attempt_v1(send_id,'relay-a',claim.lease_fence,h,'!room:e','https://matrix.example',token_hash);
    if result <> 'replay' then raise exception 'send binding replay failed'; end if;
    rejected := false;
    begin
        perform public.cex_matrix_bind_send_attempt_v1(send_id,'relay-a',claim.lease_fence,h,'!room:e','https://matrix.example',token2);
    exception when others then rejected := sqlerrm = 'matrix_send_idempotency_scope_changed'; end;
    if not rejected then raise exception 'credential rotation silently changed retry identity'; end if;
    rejected := false;
    begin
        perform public.cex_matrix_record_send_receipt_v1(send_id,'relay-a',claim.lease_fence,h2,'!room:e','$receipt');
    exception when others then rejected := sqlerrm = 'matrix_delivery_claim_mismatch'; end;
    if not rejected then raise exception 'wrong receipt payload hash accepted'; end if;
    result := public.cex_matrix_record_send_receipt_v1(send_id,'relay-a',claim.lease_fence,h,'!room:e','$receipt');
    if result <> 'recorded' then raise exception 'receipt not recorded'; end if;
    result := public.cex_matrix_record_send_receipt_v1(send_id,'relay-a',claim.lease_fence,h,'!room:e','$receipt');
    if result <> 'replay' then raise exception 'receipt replay failed'; end if;
    rejected := false;
    begin
        perform public.cex_matrix_record_send_receipt_v1(send_id,'relay-a',claim.lease_fence,h,'!room:e','$different');
    exception when others then rejected := sqlerrm = 'matrix_send_receipt_collision'; end;
    if not rejected then raise exception 'receipt identity overwritten'; end if;
    if public.cex_matrix_finish_delivery_v1(send_id,'relay-a',claim.lease_fence,'sent',null) <> 'sent' then
        raise exception 'verified send did not complete';
    end if;

    perform public.cex_matrix_accept_source_event_v1('retry-e',h,'sync-test','next-1');
    perform public.cex_matrix_enqueue_delivery_v1(retry_id,'retry-e','matrix-homeserver-v1',h,'{"room_id":"!room:e"}',3);
    select * into strict claim from public.cex_matrix_claim_delivery_v1('relay-a',60,1);
    perform public.cex_matrix_bind_send_attempt_v1(retry_id,'relay-a',claim.lease_fence,h,'!room:e','https://matrix.example',token_hash);
    update public.matrix_transport_outbox set lease_expires_at = clock_timestamp()-interval '1 second' where delivery_id = retry_id;
    select * into strict recovered from public.cex_matrix_claim_delivery_v1('relay-b',60,1);
    if recovered.delivery_id <> retry_id or recovered.lease_fence <= claim.lease_fence then raise exception 'send retry identity lost'; end if;
    if public.cex_matrix_bind_send_attempt_v1(retry_id,'relay-b',recovered.lease_fence,h,'!room:e','https://matrix.example',token_hash) <> 'replay' then
        raise exception 'stable send binding not recovered';
    end if;
    rejected := false;
    begin
        perform public.cex_matrix_record_send_receipt_v1(retry_id,'relay-a',claim.lease_fence,h,'!room:e','$stale');
    exception when others then rejected := sqlerrm = 'matrix_delivery_claim_mismatch'; end;
    if not rejected then raise exception 'old owner acknowledged recovered send'; end if;
    rejected := false;
    begin
        delete from public.matrix_transport_send_receipts where delivery_id = send_id;
    exception when others then rejected := sqlerrm = 'matrix_immutable_history_mutation_rejected'; end;
    if not rejected then raise exception 'send receipt deleted'; end if;
    rejected := false;
    begin
        update public.matrix_transport_send_bindings set credential_sha256 = token2 where delivery_id = retry_id;
    exception when others then rejected := sqlerrm = 'matrix_immutable_history_mutation_rejected'; end;
    if not rejected then raise exception 'send binding rewritten'; end if;
end;
$test$;
rollback;
