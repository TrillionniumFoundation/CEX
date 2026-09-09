begin;

do $matrix_runtime_reconciliation_setup$
declare
    source_event constant text := '$adapter-runtime-reconciliation-test';
    source_hash constant text := 'sha256:4444444444444444444444444444444444444444444444444444444444444444';
    payload_hash constant text := 'sha256:5555555555555555555555555555555555555555555555555555555555555555';
    delivery constant uuid := '63000000-0000-4000-8000-000000000001';
    payload constant jsonb := jsonb_build_object(
        'event_id', source_event,
        'event_type', 'm.room.message',
        'room_id', '!runtime-room:example',
        'sender', '@runtime-user:example',
        'text', 'recover runtime result'
    );
    disposition text;
begin
    disposition := public.cex_matrix_accept_source_event_v1(
        source_event,
        source_hash,
        'runtime-reconciliation-test',
        'cursor-runtime-test'
    );
    if disposition not in ('accepted', 'replay') then
        raise exception 'matrix_runtime_source_not_accepted';
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
        raise exception 'matrix_runtime_delivery_not_enqueued';
    end if;
    update public.matrix_transport_outbox
       set status = 'dead_letter',
           last_error_code = 'adapter_response_unknown_network',
           lease_owner = null,
           lease_expires_at = null,
           sent_at = null,
           updated_at = clock_timestamp()
     where delivery_id = delivery;
end;
$matrix_runtime_reconciliation_setup$;

set local role cex_matrix_reconciler_runtime;

select public.cex_matrix_reconcile_adapter_result_v1(
    '63000000-0000-4000-8000-000000000001',
    '$adapter-runtime-reconciliation-test',
    'sha256:5555555555555555555555555555555555555555555555555555555555555555',
    '!runtime-room:example',
    '@runtime-user:example',
    'task-runtime-reconciliation-1',
    jsonb_build_object(
        'task_id', 'task-runtime-reconciliation-1',
        'consumer_status', 'received',
        'source', jsonb_build_object(
            'kind', 'matrix_message',
            'event_id', '$adapter-runtime-reconciliation-test',
            'room_id', '!runtime-room:example',
            'matrix_user_id', '@runtime-user:example',
            'identity_scope', jsonb_build_object(
                'user_id', '@runtime-user:example',
                'room_id', '!runtime-room:example'
            )
        ),
        'raw', jsonb_build_object(
            'invocation_id', 'task-runtime-reconciliation-1',
            'status', 'accepted'
        )
    ),
    'sha256:' || encode(
        sha256(convert_to(jsonb_build_object(
            'task_id', 'task-runtime-reconciliation-1',
            'consumer_status', 'received',
            'source', jsonb_build_object(
                'kind', 'matrix_message',
                'event_id', '$adapter-runtime-reconciliation-test',
                'room_id', '!runtime-room:example',
                'matrix_user_id', '@runtime-user:example',
                'identity_scope', jsonb_build_object(
                    'user_id', '@runtime-user:example',
                    'room_id', '!runtime-room:example'
                )
            ),
            'raw', jsonb_build_object(
                'invocation_id', 'task-runtime-reconciliation-1',
                'status', 'accepted'
            )
        )::text, 'UTF8')),
        'hex'
    ),
    jsonb_build_object(
        'schema', 'cex.matrix.adapter-result-reconciliation-evidence.v1',
        'lookup_response_sha256', 'sha256:6666666666666666666666666666666666666666666666666666666666666666',
        'observed_at', '2026-09-09T00:00:00Z',
        'candidate_sha', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
    )
);

-- Exact replay must be side-effect free and preserve the stored result bytes.
select public.cex_matrix_reconcile_adapter_result_v1(
    '63000000-0000-4000-8000-000000000001',
    '$adapter-runtime-reconciliation-test',
    'sha256:5555555555555555555555555555555555555555555555555555555555555555',
    '!runtime-room:example',
    '@runtime-user:example',
    'task-runtime-reconciliation-1',
    jsonb_build_object(
        'task_id', 'task-runtime-reconciliation-1',
        'consumer_status', 'received',
        'source', jsonb_build_object(
            'kind', 'matrix_message',
            'event_id', '$adapter-runtime-reconciliation-test',
            'room_id', '!runtime-room:example',
            'matrix_user_id', '@runtime-user:example',
            'identity_scope', jsonb_build_object(
                'user_id', '@runtime-user:example',
                'room_id', '!runtime-room:example'
            )
        ),
        'raw', jsonb_build_object(
            'invocation_id', 'task-runtime-reconciliation-1',
            'status', 'accepted'
        )
    ),
    'sha256:' || encode(
        sha256(convert_to(jsonb_build_object(
            'task_id', 'task-runtime-reconciliation-1',
            'consumer_status', 'received',
            'source', jsonb_build_object(
                'kind', 'matrix_message',
                'event_id', '$adapter-runtime-reconciliation-test',
                'room_id', '!runtime-room:example',
                'matrix_user_id', '@runtime-user:example',
                'identity_scope', jsonb_build_object(
                    'user_id', '@runtime-user:example',
                    'room_id', '!runtime-room:example'
                )
            ),
            'raw', jsonb_build_object(
                'invocation_id', 'task-runtime-reconciliation-1',
                'status', 'accepted'
            )
        )::text, 'UTF8')),
        'hex'
    ),
    jsonb_build_object(
        'schema', 'cex.matrix.adapter-result-reconciliation-evidence.v1',
        'lookup_response_sha256', 'sha256:6666666666666666666666666666666666666666666666666666666666666666',
        'observed_at', '2026-09-09T00:00:00Z',
        'candidate_sha', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
    )
);

reset role;

do $matrix_runtime_reconciliation_assertions$
declare
    delivery constant uuid := '63000000-0000-4000-8000-000000000001';
    expected jsonb := jsonb_build_object(
        'task_id', 'task-runtime-reconciliation-1',
        'consumer_status', 'received',
        'source', jsonb_build_object(
            'kind', 'matrix_message',
            'event_id', '$adapter-runtime-reconciliation-test',
            'room_id', '!runtime-room:example',
            'matrix_user_id', '@runtime-user:example',
            'identity_scope', jsonb_build_object(
                'user_id', '@runtime-user:example',
                'room_id', '!runtime-room:example'
            )
        ),
        'raw', jsonb_build_object(
            'invocation_id', 'task-runtime-reconciliation-1',
            'status', 'accepted'
        )
    );
    history_count bigint;
begin
    if (select status from public.matrix_transport_outbox where delivery_id = delivery)
        is distinct from 'sent'
    then
        raise exception 'matrix_runtime_reconciliation_did_not_close_delivery';
    end if;
    if (select result_payload
          from public.matrix_transport_adapter_result_reconciliations
         where delivery_id = delivery) is distinct from expected
    then
        raise exception 'matrix_runtime_reconciliation_payload_not_persisted';
    end if;
    select count(*) into history_count
      from public.matrix_transport_delivery_history
     where delivery_id = delivery
       and from_status = 'dead_letter'
       and to_status = 'sent'
       and error_code = 'adapter_result_reconciled';
    if history_count <> 1 then
        raise exception 'matrix_runtime_reconciliation_replay_wrote_history';
    end if;

    begin
        perform public.cex_matrix_reconcile_adapter_result_v1(
            delivery,
            '$adapter-runtime-reconciliation-test',
            'sha256:5555555555555555555555555555555555555555555555555555555555555555',
            '!runtime-room:example',
            '@runtime-user:example',
            'task-runtime-reconciliation-1',
            expected || jsonb_build_object('changed', true),
            'sha256:' || encode(
                sha256(convert_to((expected || jsonb_build_object('changed', true))::text, 'UTF8')),
                'hex'
            ),
            jsonb_build_object(
                'schema', 'cex.matrix.adapter-result-reconciliation-evidence.v1',
                'lookup_response_sha256', 'sha256:6666666666666666666666666666666666666666666666666666666666666666',
                'observed_at', '2026-09-09T00:00:00Z',
                'candidate_sha', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
            )
        );
        raise exception 'matrix_runtime_reconciliation_collision_not_rejected';
    exception
        when others then
            if sqlerrm not like '%matrix_adapter_result_reconciliation_collision%' then
                raise;
            end if;
    end;
end;
$matrix_runtime_reconciliation_assertions$;

rollback;
