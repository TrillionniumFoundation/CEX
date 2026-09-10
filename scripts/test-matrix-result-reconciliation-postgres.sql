begin;

do $matrix_reconciliation_setup$
declare
    source_event constant text := '$adapter-result-reconciliation-test';
    source_hash constant text := 'sha256:1111111111111111111111111111111111111111111111111111111111111111';
    payload_hash constant text := 'sha256:2222222222222222222222222222222222222222222222222222222222222222';
    delivery constant uuid := '61000000-0000-4000-8000-000000000001';
    payload constant jsonb := jsonb_build_object(
        'event_id', source_event,
        'event_type', 'm.room.message',
        'room_id', '!result-room:example',
        'sender', '@result-user:example',
        'text', 'recover exact result'
    );
    disposition text;
begin
    delete from public.matrix_transport_adapter_result_reconciliations
     where delivery_id = delivery;
    delete from public.matrix_transport_adapter_result_observations
     where delivery_id = delivery;
    delete from public.matrix_transport_delivery_history
     where delivery_id = delivery;
    delete from public.matrix_transport_outbox
     where delivery_id = delivery;
    delete from public.matrix_transport_source_observations
     where source_event_id = source_event;
    delete from public.matrix_transport_inbox
     where source_event_id = source_event;

    disposition := public.cex_matrix_accept_source_event_v1(
        source_event, source_hash, 'result-reconciliation-test', 'cursor-result-test'
    );
    if disposition not in ('accepted', 'replay') then
        raise exception 'matrix_result_test_source_not_accepted';
    end if;
    disposition := public.cex_matrix_enqueue_delivery_v1(
        delivery, source_event, 'matrix-relay-adapter-v1', payload_hash, payload, 3
    );
    if disposition not in ('enqueued', 'replay') then
        raise exception 'matrix_result_test_delivery_not_enqueued';
    end if;

    update public.matrix_transport_outbox
       set status = 'dead_letter',
           last_error_code = 'adapter_response_unknown_timeout',
           lease_owner = null,
           lease_expires_at = null,
           sent_at = null,
           updated_at = clock_timestamp()
     where delivery_id = delivery;
end;
$matrix_reconciliation_setup$;

set local role cex_matrix_reconciler_runtime;

do $matrix_reconciler_runtime_contract$
declare
    delivery constant uuid := '61000000-0000-4000-8000-000000000001';
    source_event constant text := '$adapter-result-reconciliation-test';
    payload_hash constant text := 'sha256:2222222222222222222222222222222222222222222222222222222222222222';
    room_id constant text := '!result-room:example';
    principal constant text := '@result-user:example';
    task_id constant text := 'task-result-reconciliation-1';
    request_fingerprint text;
    delivery_binding jsonb;
    result_payload jsonb;
    result_hash text;
    evidence jsonb;
    disposition text;
begin
    begin
        perform payload from public.matrix_transport_outbox limit 1;
        raise exception 'matrix_reconciler_direct_payload_read_was_allowed';
    exception
        when insufficient_privilege then null;
    end;
    begin
        update public.matrix_transport_outbox
           set updated_at = clock_timestamp()
         where false;
        raise exception 'matrix_reconciler_direct_update_was_allowed';
    exception
        when insufficient_privilege then null;
    end;
    begin
        perform * from public.cex_matrix_claim_delivery_v1('forbidden', 30, 1);
        raise exception 'matrix_reconciler_claim_delivery_was_allowed';
    exception
        when insufficient_privilege then null;
    end;

    request_fingerprint := 'sha256:' || encode(
        sha256(
            int8send(octet_length('cex.matrix.adapter-result-delivery.v1')::bigint)
                || convert_to('cex.matrix.adapter-result-delivery.v1', 'UTF8')
                || int8send(octet_length(delivery::text)::bigint)
                || convert_to(delivery::text, 'UTF8')
                || int8send(octet_length(payload_hash)::bigint)
                || convert_to(payload_hash, 'UTF8')
                || int8send(octet_length(source_event)::bigint)
                || convert_to(source_event, 'UTF8')
                || int8send(octet_length(principal)::bigint)
                || convert_to(principal, 'UTF8')
                || int8send(octet_length(room_id)::bigint)
                || convert_to(room_id, 'UTF8')
        ),
        'hex'
    );

    delivery_binding := jsonb_build_object(
        'schema', 'cex.matrix.delivery-binding.v1',
        'source', 'matrix-bot-relay-headers-v1',
        'delivery_id', delivery::text,
        'payload_sha256', payload_hash,
        'event_id', source_event,
        'room_id', room_id,
        'matrix_user_id', principal,
        'request_fingerprint', request_fingerprint
    );

    result_payload := jsonb_build_object(
        'task_id', task_id,
        'consumer_status', 'received',
        'source', jsonb_build_object(
            'kind', 'matrix_message',
            'event_id', source_event,
            'room_id', room_id,
            'matrix_user_id', principal,
            'identity_scope', jsonb_build_object(
                'user_id', principal,
                'room_id', room_id
            ),
            'metadata', jsonb_build_object(
                'event_type', 'm.room.message',
                'timestamp_ms', 1789000000000::bigint,
                'metadata', jsonb_build_object(
                    'cex_delivery_binding', delivery_binding
                ),
                'content', jsonb_build_object(
                    'msgtype', 'm.text',
                    'body', 'recover exact result'
                )
            )
        ),
        'raw', jsonb_build_object(
            'invocation_id', task_id,
            'status', 'accepted'
        )
    );
    result_hash := 'sha256:' || encode(
        sha256(convert_to(result_payload::text, 'UTF8')),
        'hex'
    );
    evidence := jsonb_build_object(
        'schema', 'cex.matrix.adapter-result-reconciliation-evidence.v2',
        'lookup_response_sha256', 'sha256:3333333333333333333333333333333333333333333333333333333333333333',
        'observed_at', clock_timestamp(),
        'candidate_sha', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
        'request_fingerprint', request_fingerprint
    );

    begin
        perform public.cex_matrix_reconcile_adapter_result_v1(
            delivery,
            source_event,
            payload_hash,
            room_id,
            principal,
            task_id,
            result_payload,
            result_hash,
            evidence
        );
        raise exception 'matrix_reconciler_v1_runtime_execute_was_allowed';
    exception
        when insufficient_privilege then null;
    end;

    begin
        perform public.cex_matrix_reconcile_adapter_result_v2(
            delivery,
            source_event,
            payload_hash,
            room_id,
            principal,
            task_id,
            request_fingerprint,
            result_payload,
            result_hash,
            evidence
        );
        raise exception 'matrix_reconciler_v2_runtime_execute_was_allowed';
    exception
        when insufficient_privilege then null;
    end;

    disposition := public.cex_matrix_reconcile_adapter_result_v3(
        delivery,
        source_event,
        payload_hash,
        room_id,
        principal,
        task_id,
        request_fingerprint,
        result_payload,
        result_hash,
        evidence
    );
    if disposition is distinct from 'reconciled' then
        raise exception 'matrix_result_v3_reconciliation_failed';
    end if;

    disposition := public.cex_matrix_reconcile_adapter_result_v3(
        delivery,
        source_event,
        payload_hash,
        room_id,
        principal,
        task_id,
        request_fingerprint,
        result_payload,
        result_hash,
        evidence
    );
    if disposition is distinct from 'replay' then
        raise exception 'matrix_result_v3_replay_not_idempotent';
    end if;
end;
$matrix_reconciler_runtime_contract$;

reset role;

do $matrix_reconciliation_assertions$
declare
    delivery constant uuid := '61000000-0000-4000-8000-000000000001';
    denied boolean;
begin
    if (select status from public.matrix_transport_outbox where delivery_id = delivery)
        is distinct from 'sent'
    then
        raise exception 'matrix_result_reconciliation_did_not_close_delivery';
    end if;
    if (select last_error_code from public.matrix_transport_outbox where delivery_id = delivery)
        is not null
    then
        raise exception 'matrix_result_reconciliation_left_failure_code';
    end if;
    if not exists (
        select 1 from public.matrix_transport_delivery_history
         where delivery_id = delivery
           and from_status = 'dead_letter'
           and to_status = 'sent'
           and error_code = 'adapter_result_reconciled_v2'
    ) then
        raise exception 'matrix_result_reconciliation_history_missing';
    end if;
    if not exists (
        select 1 from public.matrix_transport_adapter_result_reconciliations
         where delivery_id = delivery
           and task_id = 'task-result-reconciliation-1'
           and principal_user_id = '@result-user:example'
           and room_id = '!result-room:example'
    ) then
        raise exception 'matrix_result_reconciliation_evidence_missing';
    end if;

    denied := not has_table_privilege(
        'cex_matrix_poller_runtime',
        'public.matrix_transport_outbox',
        'insert'
    );
    if not denied then
        raise exception 'matrix_poller_has_direct_outbox_insert';
    end if;
    denied := not has_table_privilege(
        'cex_matrix_relay_runtime',
        'public.matrix_transport_outbox',
        'update'
    );
    if not denied then
        raise exception 'matrix_relay_has_direct_outbox_update';
    end if;
    denied := not has_table_privilege(
        'cex_matrix_reconciler_runtime',
        'public.matrix_transport_adapter_result_reconciliations',
        'insert'
    );
    if not denied then
        raise exception 'matrix_reconciler_has_direct_evidence_insert';
    end if;

    if has_function_privilege(
        'cex_matrix_reconciler_runtime',
        'public.cex_matrix_reconcile_adapter_result_v1(uuid,text,text,text,text,text,jsonb,text,jsonb)',
        'execute'
    ) then
        raise exception 'matrix_reconciler_v1_runtime_execute_not_revoked';
    end if;
    if has_function_privilege(
        'cex_matrix_reconciler_runtime',
        'public.cex_matrix_reconcile_adapter_result_v2(uuid,text,text,text,text,text,text,jsonb,text,jsonb)',
        'execute'
    ) then
        raise exception 'matrix_reconciler_v2_runtime_execute_not_revoked';
    end if;
    if not has_function_privilege(
        'cex_matrix_reconciler_runtime',
        'public.cex_matrix_reconcile_adapter_result_v3(uuid,text,text,text,text,text,text,jsonb,text,jsonb)',
        'execute'
    ) then
        raise exception 'matrix_reconciler_v3_runtime_execute_missing';
    end if;
    if has_function_privilege(
        'public',
        'public.cex_matrix_reconcile_adapter_result_v3(uuid,text,text,text,text,text,text,jsonb,text,jsonb)',
        'execute'
    ) then
        raise exception 'matrix_reconciliation_v3_function_is_public';
    end if;

    begin
        update public.matrix_transport_adapter_result_reconciliations
           set recorded_by = recorded_by
         where delivery_id = delivery;
        raise exception 'matrix_reconciliation_evidence_was_mutable';
    exception
        when others then
            if sqlerrm not like '%matrix_immutable_history_mutation_rejected%' then
                raise;
            end if;
    end;
end;
$matrix_reconciliation_assertions$;

rollback;
