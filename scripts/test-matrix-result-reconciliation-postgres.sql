begin;

do $matrix_reconciliation_test$
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
    result_payload constant jsonb := jsonb_build_object(
        'task_id', 'task-result-reconciliation-1',
        'consumer_status', 'received',
        'source', jsonb_build_object(
            'kind', 'matrix_message',
            'event_id', source_event,
            'room_id', '!result-room:example',
            'matrix_user_id', '@result-user:example',
            'identity_scope', jsonb_build_object(
                'user_id', '@result-user:example',
                'room_id', '!result-room:example'
            )
        ),
        'raw', jsonb_build_object(
            'invocation_id', 'task-result-reconciliation-1',
            'status', 'accepted'
        )
    );
    result_hash text;
    evidence constant jsonb := jsonb_build_object(
        'schema', 'cex.matrix.adapter-result-reconciliation-evidence.v1',
        'lookup_response_sha256', 'sha256:3333333333333333333333333333333333333333333333333333333333333333',
        'observed_at', '2026-09-05T00:00:00Z',
        'candidate_sha', 'test-candidate'
    );
    disposition text;
begin
    delete from public.matrix_transport_adapter_result_reconciliations
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
        delivery,
        source_event,
        'matrix-relay-adapter-v1',
        payload_hash,
        payload,
        3
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

    result_hash := 'sha256:' || encode(
        sha256(convert_to(result_payload::text, 'UTF8')),
        'hex'
    );

    begin
        perform public.cex_matrix_reconcile_adapter_result_v1(
            delivery,
            source_event,
            payload_hash,
            '!result-room:example',
            '@wrong-user:example',
            'task-result-reconciliation-1',
            result_payload,
            result_hash,
            evidence
        );
        raise exception 'matrix_result_wrong_principal_was_accepted';
    exception
        when others then
            if sqlerrm not like '%matrix_adapter_result_identity_mismatch%' then
                raise;
            end if;
    end;

    if exists (
        select 1 from public.matrix_transport_adapter_result_reconciliations
         where delivery_id = delivery
    ) then
        raise exception 'matrix_result_negative_path_persisted_evidence';
    end if;
    if (select status from public.matrix_transport_outbox where delivery_id = delivery)
        is distinct from 'dead_letter'
    then
        raise exception 'matrix_result_negative_path_changed_delivery';
    end if;
end;
$matrix_reconciliation_test$;

set local role cex_matrix_reconciler_runtime;

do $matrix_reconciler_denials$
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
end;
$matrix_reconciler_denials$;

select public.cex_matrix_reconcile_adapter_result_v1(
    '61000000-0000-4000-8000-000000000001',
    '$adapter-result-reconciliation-test',
    'sha256:2222222222222222222222222222222222222222222222222222222222222222',
    '!result-room:example',
    '@result-user:example',
    'task-result-reconciliation-1',
    jsonb_build_object(
        'task_id', 'task-result-reconciliation-1',
        'consumer_status', 'received',
        'source', jsonb_build_object(
            'kind', 'matrix_message',
            'event_id', '$adapter-result-reconciliation-test',
            'room_id', '!result-room:example',
            'matrix_user_id', '@result-user:example',
            'identity_scope', jsonb_build_object(
                'user_id', '@result-user:example',
                'room_id', '!result-room:example'
            )
        ),
        'raw', jsonb_build_object(
            'invocation_id', 'task-result-reconciliation-1',
            'status', 'accepted'
        )
    ),
    'sha256:' || encode(
        sha256(convert_to(jsonb_build_object(
            'task_id', 'task-result-reconciliation-1',
            'consumer_status', 'received',
            'source', jsonb_build_object(
                'kind', 'matrix_message',
                'event_id', '$adapter-result-reconciliation-test',
                'room_id', '!result-room:example',
                'matrix_user_id', '@result-user:example',
                'identity_scope', jsonb_build_object(
                    'user_id', '@result-user:example',
                    'room_id', '!result-room:example'
                )
            ),
            'raw', jsonb_build_object(
                'invocation_id', 'task-result-reconciliation-1',
                'status', 'accepted'
            )
        )::text, 'UTF8')),
        'hex'
    ),
    jsonb_build_object(
        'schema', 'cex.matrix.adapter-result-reconciliation-evidence.v1',
        'lookup_response_sha256', 'sha256:3333333333333333333333333333333333333333333333333333333333333333',
        'observed_at', '2026-09-05T00:00:00Z',
        'candidate_sha', 'test-candidate'
    )
);

select public.cex_matrix_lookup_adapter_result_reconciliation_v1(
    '61000000-0000-4000-8000-000000000001'
);

select public.cex_matrix_reconcile_adapter_result_v1(
    '61000000-0000-4000-8000-000000000001',
    '$adapter-result-reconciliation-test',
    'sha256:2222222222222222222222222222222222222222222222222222222222222222',
    '!result-room:example',
    '@result-user:example',
    'task-result-reconciliation-1',
    jsonb_build_object(
        'task_id', 'task-result-reconciliation-1',
        'consumer_status', 'received',
        'source', jsonb_build_object(
            'kind', 'matrix_message',
            'event_id', '$adapter-result-reconciliation-test',
            'room_id', '!result-room:example',
            'matrix_user_id', '@result-user:example',
            'identity_scope', jsonb_build_object(
                'user_id', '@result-user:example',
                'room_id', '!result-room:example'
            )
        ),
        'raw', jsonb_build_object(
            'invocation_id', 'task-result-reconciliation-1',
            'status', 'accepted'
        )
    ),
    'sha256:' || encode(
        sha256(convert_to(jsonb_build_object(
            'task_id', 'task-result-reconciliation-1',
            'consumer_status', 'received',
            'source', jsonb_build_object(
                'kind', 'matrix_message',
                'event_id', '$adapter-result-reconciliation-test',
                'room_id', '!result-room:example',
                'matrix_user_id', '@result-user:example',
                'identity_scope', jsonb_build_object(
                    'user_id', '@result-user:example',
                    'room_id', '!result-room:example'
                )
            ),
            'raw', jsonb_build_object(
                'invocation_id', 'task-result-reconciliation-1',
                'status', 'accepted'
            )
        )::text, 'UTF8')),
        'hex'
    ),
    jsonb_build_object(
        'schema', 'cex.matrix.adapter-result-reconciliation-evidence.v1',
        'lookup_response_sha256', 'sha256:3333333333333333333333333333333333333333333333333333333333333333',
        'observed_at', '2026-09-05T00:00:00Z',
        'candidate_sha', 'test-candidate'
    )
);

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
           and error_code = 'adapter_result_reconciled'
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
    if not has_function_privilege(
        'cex_matrix_reconciler_runtime',
        'public.cex_matrix_reconcile_adapter_result_v1(uuid,text,text,text,text,text,jsonb,text,jsonb)',
        'execute'
    ) then
        raise exception 'matrix_reconciler_function_grant_missing';
    end if;
    if has_function_privilege(
        'public',
        'public.cex_matrix_reconcile_adapter_result_v1(uuid,text,text,text,text,text,jsonb,text,jsonb)',
        'execute'
    ) then
        raise exception 'matrix_reconciliation_function_is_public';
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
