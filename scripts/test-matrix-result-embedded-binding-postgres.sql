begin;

do $matrix_embedded_binding_setup$
declare
    source_event constant text := '$adapter-embedded-binding-test';
    source_hash constant text :=
        'sha256:abababababababababababababababababababababababababababababababab';
    payload_hash constant text :=
        'sha256:cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd';
    delivery constant uuid := '65000000-0000-4000-8000-000000000001';
    payload constant jsonb := jsonb_build_object(
        'event_id', source_event,
        'event_type', 'm.room.message',
        'room_id', '!delivery-room:example',
        'sender', '@delivery-user:example',
        'text', 'recover embedded delivery binding',
        'content', jsonb_build_object(
            'msgtype', 'm.text',
            'body', 'recover embedded delivery binding'
        ),
        'timestamp_ms', 1789000000000::bigint,
        'metadata', jsonb_build_object('upstream', 'matrix-bot-relay')
    );
    disposition text;
begin
    disposition := public.cex_matrix_accept_source_event_v1(
        source_event,
        source_hash,
        'embedded-binding-test',
        'cursor-embedded-binding-test'
    );
    if disposition not in ('accepted', 'replay') then
        raise exception 'matrix_embedded_binding_source_not_accepted';
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
        raise exception 'matrix_embedded_binding_delivery_not_enqueued';
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
$matrix_embedded_binding_setup$;

set local role cex_matrix_reconciler_runtime;

do $matrix_embedded_binding_reconcile$
declare
    delivery constant uuid := '65000000-0000-4000-8000-000000000001';
    source_event constant text := '$adapter-embedded-binding-test';
    payload_hash constant text :=
        'sha256:cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd';
    room_id constant text := '!delivery-room:example';
    principal constant text := '@delivery-user:example';
    task_id constant text := 'task-embedded-binding-1';
    candidate constant text := 'cccccccccccccccccccccccccccccccccccccccc';
    request_fingerprint text;
    delivery_binding jsonb;
    result_payload jsonb;
    result_sha256 text;
    disposition text;
begin
    request_fingerprint := 'sha256:' || encode(
        sha256(
            int8send(
                octet_length(
                    'cex.matrix.adapter-result-delivery.v1'
                )::bigint
            )
                || convert_to(
                    'cex.matrix.adapter-result-delivery.v1',
                    'UTF8'
                )
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
                    'cex_delivery_binding', delivery_binding,
                    'upstream', 'matrix-bot-relay'
                ),
                'content', jsonb_build_object(
                    'msgtype', 'm.text',
                    'body', 'recover embedded delivery binding'
                )
            )
        ),
        'raw', jsonb_build_object(
            'invocation_id', task_id,
            'status', 'accepted'
        )
    );
    result_sha256 := 'sha256:' || encode(
        sha256(convert_to(result_payload::text, 'UTF8')),
        'hex'
    );

    disposition := public.cex_matrix_reconcile_adapter_result_v3(
        delivery,
        source_event,
        payload_hash,
        room_id,
        principal,
        task_id,
        request_fingerprint,
        result_payload,
        result_sha256,
        jsonb_build_object(
            'schema',
            'cex.matrix.adapter-result-reconciliation-evidence.v2',
            'lookup_response_sha256',
            'sha256:dededededededededededededededededededededededededededededededede',
            'observed_at',
            clock_timestamp(),
            'candidate_sha',
            candidate,
            'request_fingerprint',
            request_fingerprint
        )
    );
    if disposition is distinct from 'reconciled' then
        raise exception 'matrix_embedded_binding_first_reconciliation_failed';
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
        result_sha256,
        jsonb_build_object(
            'schema',
            'cex.matrix.adapter-result-reconciliation-evidence.v2',
            'lookup_response_sha256',
            'sha256:efefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefef',
            'observed_at',
            clock_timestamp(),
            'candidate_sha',
            candidate,
            'request_fingerprint',
            request_fingerprint
        )
    );
    if disposition is distinct from 'replay' then
        raise exception 'matrix_embedded_binding_honest_retry_not_idempotent';
    end if;
end;
$matrix_embedded_binding_reconcile$;

reset role;

do $matrix_embedded_binding_assertions$
declare
    delivery constant uuid := '65000000-0000-4000-8000-000000000001';
    history_count bigint;
    observation_count bigint;
begin
    if (
        select status
          from public.matrix_transport_outbox
         where delivery_id = delivery
    ) is distinct from 'sent' then
        raise exception 'matrix_embedded_binding_delivery_not_closed';
    end if;

    select count(*)
      into history_count
      from public.matrix_transport_delivery_history
     where delivery_id = delivery
       and from_status = 'dead_letter'
       and to_status = 'sent'
       and error_code = 'adapter_result_reconciled_v2';
    if history_count <> 1 then
        raise exception 'matrix_embedded_binding_transition_count_mismatch';
    end if;

    select count(*)
      into observation_count
      from public.matrix_transport_adapter_result_observations
     where delivery_id = delivery;
    if observation_count <> 2 then
        raise exception 'matrix_embedded_binding_observation_count_mismatch';
    end if;

    if has_function_privilege(
        'cex_matrix_reconciler_runtime',
        'public.cex_matrix_reconcile_adapter_result_v2(uuid,text,text,text,text,text,text,jsonb,text,jsonb)',
        'execute'
    ) then
        raise exception 'matrix_embedded_binding_v2_runtime_execute_not_revoked';
    end if;
    if not has_function_privilege(
        'cex_matrix_reconciler_runtime',
        'public.cex_matrix_reconcile_adapter_result_v3(uuid,text,text,text,text,text,text,jsonb,text,jsonb)',
        'execute'
    ) then
        raise exception 'matrix_embedded_binding_v3_runtime_execute_missing';
    end if;
end;
$matrix_embedded_binding_assertions$;

set local role cex_matrix_reconciler_runtime;

do $matrix_embedded_binding_hostile_inputs$
declare
    delivery constant uuid := '65000000-0000-4000-8000-000000000001';
    source_event constant text := '$adapter-embedded-binding-test';
    payload_hash constant text :=
        'sha256:cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd';
    room_id constant text := '!delivery-room:example';
    principal constant text := '@delivery-user:example';
    task_id constant text := 'task-embedded-binding-1';
    candidate constant text := 'cccccccccccccccccccccccccccccccccccccccc';
    request_fingerprint text;
    delivery_binding jsonb;
    result_payload jsonb;
    hostile_payload jsonb;
    hostile_sha256 text;
    evidence jsonb;
begin
    request_fingerprint := 'sha256:' || encode(
        sha256(
            int8send(
                octet_length(
                    'cex.matrix.adapter-result-delivery.v1'
                )::bigint
            )
                || convert_to(
                    'cex.matrix.adapter-result-delivery.v1',
                    'UTF8'
                )
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
                    'body', 'recover embedded delivery binding'
                )
            )
        ),
        'raw', jsonb_build_object(
            'invocation_id', task_id,
            'status', 'accepted'
        )
    );
    evidence := jsonb_build_object(
        'schema',
        'cex.matrix.adapter-result-reconciliation-evidence.v2',
        'lookup_response_sha256',
        'sha256:f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0',
        'observed_at',
        clock_timestamp(),
        'candidate_sha',
        candidate,
        'request_fingerprint',
        request_fingerprint
    );

    hostile_payload := result_payload #- '{source,metadata,metadata,cex_delivery_binding}';
    hostile_sha256 := 'sha256:' || encode(
        sha256(convert_to(hostile_payload::text, 'UTF8')),
        'hex'
    );
    begin
        perform public.cex_matrix_reconcile_adapter_result_v3(
            delivery,
            source_event,
            payload_hash,
            room_id,
            principal,
            task_id,
            request_fingerprint,
            hostile_payload,
            hostile_sha256,
            evidence
        );
        raise exception 'matrix_embedded_binding_missing_binding_accepted';
    exception
        when others then
            if sqlerrm is distinct from
                'matrix_adapter_result_embedded_binding_invalid_v3' then
                raise;
            end if;
    end;

    hostile_payload := jsonb_set(
        result_payload,
        '{source,metadata,metadata,cex_delivery_binding,payload_sha256}',
        to_jsonb(
            'sha256:0000000000000000000000000000000000000000000000000000000000000000'::text
        )
    );
    hostile_sha256 := 'sha256:' || encode(
        sha256(convert_to(hostile_payload::text, 'UTF8')),
        'hex'
    );
    begin
        perform public.cex_matrix_reconcile_adapter_result_v3(
            delivery,
            source_event,
            payload_hash,
            room_id,
            principal,
            task_id,
            request_fingerprint,
            hostile_payload,
            hostile_sha256,
            evidence
        );
        raise exception 'matrix_embedded_binding_changed_payload_accepted';
    exception
        when others then
            if sqlerrm is distinct from
                'matrix_adapter_result_embedded_binding_invalid_v3' then
                raise;
            end if;
    end;

    hostile_payload := jsonb_set(
        result_payload,
        '{source,metadata,metadata,cex_delivery_binding,forged}',
        'true'::jsonb,
        true
    );
    hostile_sha256 := 'sha256:' || encode(
        sha256(convert_to(hostile_payload::text, 'UTF8')),
        'hex'
    );
    begin
        perform public.cex_matrix_reconcile_adapter_result_v3(
            delivery,
            source_event,
            payload_hash,
            room_id,
            principal,
            task_id,
            request_fingerprint,
            hostile_payload,
            hostile_sha256,
            evidence
        );
        raise exception 'matrix_embedded_binding_extra_field_accepted';
    exception
        when others then
            if sqlerrm is distinct from
                'matrix_adapter_result_embedded_binding_invalid_v3' then
                raise;
            end if;
    end;
end;
$matrix_embedded_binding_hostile_inputs$;

reset role;

do $matrix_embedded_binding_no_side_effects$
declare
    observation_count bigint;
begin
    select count(*)
      into observation_count
      from public.matrix_transport_adapter_result_observations
     where delivery_id = '65000000-0000-4000-8000-000000000001'::uuid;
    if observation_count <> 2 then
        raise exception 'matrix_embedded_binding_hostile_input_wrote_observation';
    end if;
end;
$matrix_embedded_binding_no_side_effects$;

rollback;
