begin;

do $matrix_causal_binding_setup$
declare
    source_event constant text := '$adapter-causal-binding-test';
    source_hash constant text :=
        'sha256:7777777777777777777777777777777777777777777777777777777777777777';
    payload_hash constant text :=
        'sha256:8888888888888888888888888888888888888888888888888888888888888888';
    delivery constant uuid := '64000000-0000-4000-8000-000000000001';
    payload constant jsonb := jsonb_build_object(
        'event_id', source_event,
        'event_type', 'm.room.message',
        'room_id', '!causal-room:example',
        'sender', '@causal-user:example',
        'text', 'recover causally bound result'
    );
    disposition text;
begin
    disposition := public.cex_matrix_accept_source_event_v1(
        source_event,
        source_hash,
        'causal-binding-test',
        'cursor-causal-test'
    );
    if disposition not in ('accepted', 'replay') then
        raise exception 'matrix_causal_source_not_accepted';
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
        raise exception 'matrix_causal_delivery_not_enqueued';
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
$matrix_causal_binding_setup$;

set local role cex_matrix_reconciler_runtime;

do $matrix_causal_binding_honest_retry$
declare
    delivery constant uuid := '64000000-0000-4000-8000-000000000001';
    source_event constant text := '$adapter-causal-binding-test';
    payload_hash constant text :=
        'sha256:8888888888888888888888888888888888888888888888888888888888888888';
    room_id constant text := '!causal-room:example';
    principal constant text := '@causal-user:example';
    task_id constant text := 'task-causal-binding-1';
    candidate constant text := 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
    request_fingerprint text;
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

    disposition := public.cex_matrix_reconcile_adapter_result_v2(
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
            'sha256:9999999999999999999999999999999999999999999999999999999999999999',
            'observed_at',
            clock_timestamp(),
            'candidate_sha',
            candidate,
            'request_fingerprint',
            request_fingerprint
        )
    );
    if disposition is distinct from 'reconciled' then
        raise exception 'matrix_causal_first_reconciliation_failed';
    end if;

    disposition := public.cex_matrix_reconcile_adapter_result_v2(
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
            'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
            'observed_at',
            clock_timestamp(),
            'candidate_sha',
            candidate,
            'request_fingerprint',
            request_fingerprint
        )
    );
    if disposition is distinct from 'replay' then
        raise exception 'matrix_causal_honest_retry_not_idempotent';
    end if;
end;
$matrix_causal_binding_honest_retry$;

reset role;

do $matrix_causal_binding_assertions$
declare
    delivery constant uuid := '64000000-0000-4000-8000-000000000001';
    expected_fingerprint text;
    history_count bigint;
    observation_count bigint;
begin
    expected_fingerprint := 'sha256:' || encode(
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
                || int8send(
                    octet_length(
                        'sha256:8888888888888888888888888888888888888888888888888888888888888888'
                    )::bigint
                )
                || convert_to(
                    'sha256:8888888888888888888888888888888888888888888888888888888888888888',
                    'UTF8'
                )
                || int8send(
                    octet_length(
                        '$adapter-causal-binding-test'
                    )::bigint
                )
                || convert_to(
                    '$adapter-causal-binding-test',
                    'UTF8'
                )
                || int8send(
                    octet_length(
                        '@causal-user:example'
                    )::bigint
                )
                || convert_to(
                    '@causal-user:example',
                    'UTF8'
                )
                || int8send(
                    octet_length(
                        '!causal-room:example'
                    )::bigint
                )
                || convert_to(
                    '!causal-room:example',
                    'UTF8'
                )
        ),
        'hex'
    );

    if (
        select status
          from public.matrix_transport_outbox
         where delivery_id = delivery
    ) is distinct from 'sent' then
        raise exception 'matrix_causal_reconciliation_did_not_close_delivery';
    end if;

    if (
        select request_fingerprint
          from public.matrix_transport_adapter_result_reconciliations
         where delivery_id = delivery
    ) is distinct from expected_fingerprint then
        raise exception 'matrix_causal_fingerprint_not_persisted';
    end if;

    select count(*)
      into history_count
      from public.matrix_transport_delivery_history
     where delivery_id = delivery
       and from_status = 'dead_letter'
       and to_status = 'sent'
       and error_code = 'adapter_result_reconciled_v2';
    if history_count <> 1 then
        raise exception 'matrix_causal_retry_wrote_second_transition';
    end if;

    select count(*)
      into observation_count
      from public.matrix_transport_adapter_result_observations
     where delivery_id = delivery;
    if observation_count <> 2 then
        raise exception 'matrix_causal_observation_append_count_mismatch';
    end if;

    if has_function_privilege(
        'cex_matrix_reconciler_runtime',
        'public.cex_matrix_reconcile_adapter_result_v1(uuid,text,text,text,text,text,jsonb,text,jsonb)',
        'execute'
    ) then
        raise exception 'matrix_causal_v1_runtime_execute_not_revoked';
    end if;
    if not has_function_privilege(
        'cex_matrix_reconciler_runtime',
        'public.cex_matrix_reconcile_adapter_result_v2(uuid,text,text,text,text,text,text,jsonb,text,jsonb)',
        'execute'
    ) then
        raise exception 'matrix_causal_v2_runtime_execute_missing';
    end if;
end;
$matrix_causal_binding_assertions$;

set local role cex_matrix_reconciler_runtime;

do $matrix_causal_binding_hostile_inputs$
declare
    delivery constant uuid := '64000000-0000-4000-8000-000000000001';
    source_event constant text := '$adapter-causal-binding-test';
    payload_hash constant text :=
        'sha256:8888888888888888888888888888888888888888888888888888888888888888';
    room_id constant text := '!causal-room:example';
    principal constant text := '@causal-user:example';
    task_id constant text := 'task-causal-binding-1';
    candidate constant text := 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
    request_fingerprint text;
    result_payload jsonb;
    result_sha256 text;
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

    begin
        perform public.cex_matrix_reconcile_adapter_result_v2(
            delivery,
            source_event,
            payload_hash,
            room_id,
            principal,
            task_id,
            'sha256:0000000000000000000000000000000000000000000000000000000000000000',
            result_payload,
            result_sha256,
            jsonb_build_object(
                'schema',
                'cex.matrix.adapter-result-reconciliation-evidence.v2',
                'lookup_response_sha256',
                'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
                'observed_at',
                clock_timestamp(),
                'candidate_sha',
                candidate,
                'request_fingerprint',
                'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            )
        );
        raise exception 'matrix_causal_changed_fingerprint_not_rejected';
    exception
        when others then
            if sqlerrm not like
                '%matrix_adapter_result_request_fingerprint_mismatch%'
            then
                raise;
            end if;
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
            result_sha256,
            jsonb_build_object(
                'schema',
                'cex.matrix.adapter-result-reconciliation-evidence.v2',
                'lookup_response_sha256',
                'sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
                'observed_at',
                clock_timestamp() - interval '16 minutes',
                'candidate_sha',
                candidate,
                'request_fingerprint',
                request_fingerprint
            )
        );
        raise exception 'matrix_causal_stale_observation_not_rejected';
    exception
        when others then
            if sqlerrm not like
                '%matrix_adapter_result_observation_outside_window%'
            then
                raise;
            end if;
    end;

    begin
        perform public.cex_matrix_reconcile_adapter_result_v2(
            delivery,
            source_event,
            payload_hash,
            room_id,
            principal,
            'task-causal-binding-2',
            request_fingerprint,
            jsonb_set(
                jsonb_set(
                    result_payload,
                    '{task_id}',
                    '"task-causal-binding-2"'::jsonb
                ),
                '{raw,invocation_id}',
                '"task-causal-binding-2"'::jsonb
            ),
            'sha256:' || encode(
                sha256(
                    convert_to(
                        jsonb_set(
                            jsonb_set(
                                result_payload,
                                '{task_id}',
                                '"task-causal-binding-2"'::jsonb
                            ),
                            '{raw,invocation_id}',
                            '"task-causal-binding-2"'::jsonb
                        )::text,
                        'UTF8'
                    )
                ),
                'hex'
            ),
            jsonb_build_object(
                'schema',
                'cex.matrix.adapter-result-reconciliation-evidence.v2',
                'lookup_response_sha256',
                'sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
                'observed_at',
                clock_timestamp(),
                'candidate_sha',
                candidate,
                'request_fingerprint',
                request_fingerprint
            )
        );
        raise exception 'matrix_causal_changed_task_not_rejected';
    exception
        when others then
            if sqlerrm not like
                '%matrix_adapter_result_reconciliation_collision_v2%'
            then
                raise;
            end if;
    end;
end;
$matrix_causal_binding_hostile_inputs$;

reset role;

do $matrix_causal_binding_final_assertions$
declare
    delivery constant uuid := '64000000-0000-4000-8000-000000000001';
    observation_count bigint;
begin
    select count(*)
      into observation_count
      from public.matrix_transport_adapter_result_observations
     where delivery_id = delivery;
    if observation_count <> 2 then
        raise exception 'matrix_causal_rejected_input_wrote_observation';
    end if;
end;
$matrix_causal_binding_final_assertions$;

rollback;
