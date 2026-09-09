begin;

set local role cex_matrix_reconciler_runtime;

do $matrix_task_invocation_binding$
declare
    delivery constant uuid := '66000000-0000-4000-8000-000000000001';
    source_event constant text := '$adapter-task-invocation-binding-test';
    payload_hash constant text :=
        'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
    room_id constant text := '!task-room:example';
    principal constant text := '@task-user:example';
    task_id constant text := 'task-invocation-binding-1';
    request_fingerprint constant text :=
        'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
    result_sha256 constant text :=
        'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc';
    evidence constant jsonb := jsonb_build_object(
        'schema', 'cex.matrix.adapter-result-reconciliation-evidence.v2',
        'lookup_response_sha256',
        'sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
        'observed_at', clock_timestamp(),
        'candidate_sha', 'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
        'request_fingerprint', request_fingerprint
    );
    payload jsonb;
begin
    payload := jsonb_build_object(
        'task_id', task_id,
        'source', jsonb_build_object('kind', 'matrix_message')
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
            payload,
            result_sha256,
            evidence
        );
        raise exception 'matrix_task_invocation_missing_accepted';
    exception
        when others then
            if sqlerrm = 'matrix_task_invocation_missing_accepted' then
                raise;
            end if;
            if sqlerrm <> 'matrix_adapter_result_task_invocation_mismatch_v3' then
                raise exception
                    'matrix_task_invocation_missing_unexpected:%', sqlerrm;
            end if;
    end;

    payload := jsonb_build_object(
        'task_id', task_id,
        'source', jsonb_build_object('kind', 'matrix_message'),
        'raw', jsonb_build_object('invocation_id', 'other-task')
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
            payload,
            result_sha256,
            evidence
        );
        raise exception 'matrix_task_invocation_mismatch_accepted';
    exception
        when others then
            if sqlerrm = 'matrix_task_invocation_mismatch_accepted' then
                raise;
            end if;
            if sqlerrm <> 'matrix_adapter_result_task_invocation_mismatch_v3' then
                raise exception
                    'matrix_task_invocation_mismatch_unexpected:%', sqlerrm;
            end if;
    end;

    payload := jsonb_build_object(
        'task_id', task_id,
        'source', jsonb_build_object('kind', 'matrix_message'),
        'raw', jsonb_build_object('invocation_id', task_id)
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
            payload,
            result_sha256,
            evidence
        );
        raise exception 'matrix_missing_delivery_binding_accepted';
    exception
        when others then
            if sqlerrm = 'matrix_missing_delivery_binding_accepted' then
                raise;
            end if;
            if sqlerrm <> 'matrix_adapter_result_embedded_binding_invalid_v3' then
                raise exception
                    'matrix_task_invocation_valid_path_unexpected:%', sqlerrm;
            end if;
    end;
end;
$matrix_task_invocation_binding$;

reset role;

rollback;
