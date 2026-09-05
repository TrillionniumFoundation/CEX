begin;

do $matrix_result_evidence_hardening$
declare
    delivery constant uuid := '62000000-0000-4000-8000-000000000001';
    payload_hash constant text := 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
    payload jsonb;
    result_hash text;
    evidence jsonb;
begin
    payload := jsonb_build_object(
        'task_id', 'task-hardening-1',
        'source', jsonb_build_object(
            'kind', 'matrix_message',
            'event_id', '$event-hardening-1',
            'room_id', '!room-hardening:example',
            'matrix_user_id', '@user-hardening:example',
            'identity_scope', jsonb_build_object(
                'user_id', '@wrong-user:example',
                'room_id', '!room-hardening:example'
            )
        ),
        'raw', jsonb_build_object('invocation_id', 'task-hardening-1')
    );
    result_hash := 'sha256:' || encode(
        sha256(convert_to(payload::text, 'UTF8')),
        'hex'
    );
    evidence := jsonb_build_object(
        'schema', 'cex.matrix.adapter-result-reconciliation-evidence.v1',
        'lookup_response_sha256', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
        'observed_at', '2026-09-05T12:00:00Z',
        'candidate_sha', 'test-candidate'
    );
    begin
        perform public.cex_matrix_reconcile_adapter_result_v1(
            delivery,
            '$event-hardening-1',
            payload_hash,
            '!room-hardening:example',
            '@user-hardening:example',
            'task-hardening-1',
            payload,
            result_hash,
            evidence
        );
        raise exception 'matrix_result_identity_scope_mismatch_not_rejected';
    exception
        when others then
            if sqlerrm not like '%matrix_adapter_result_identity_scope_mismatch%' then
                raise;
            end if;
    end;

    payload := jsonb_set(
        payload,
        '{source,identity_scope,user_id}',
        to_jsonb('@user-hardening:example'::text),
        false
    );
    payload := jsonb_set(
        payload,
        '{raw,invocation_id}',
        to_jsonb('different-task'::text),
        false
    );
    result_hash := 'sha256:' || encode(
        sha256(convert_to(payload::text, 'UTF8')),
        'hex'
    );
    begin
        perform public.cex_matrix_reconcile_adapter_result_v1(
            delivery,
            '$event-hardening-1',
            payload_hash,
            '!room-hardening:example',
            '@user-hardening:example',
            'task-hardening-1',
            payload,
            result_hash,
            evidence
        );
        raise exception 'matrix_result_raw_task_mismatch_not_rejected';
    exception
        when others then
            if sqlerrm not like '%matrix_adapter_result_raw_task_mismatch%' then
                raise;
            end if;
    end;

    payload := jsonb_set(
        payload,
        '{raw,invocation_id}',
        to_jsonb('task-hardening-1'::text),
        false
    );
    result_hash := 'sha256:' || encode(
        sha256(convert_to(payload::text, 'UTF8')),
        'hex'
    );
    begin
        perform public.cex_matrix_reconcile_adapter_result_v1(
            delivery,
            '$event-hardening-1',
            payload_hash,
            '!room-hardening:example',
            '@user-hardening:example',
            'task-hardening-1',
            payload,
            result_hash,
            evidence || jsonb_build_object('unexpected', true)
        );
        raise exception 'matrix_result_extra_evidence_key_not_rejected';
    exception
        when others then
            if sqlerrm not like '%invalid_matrix_adapter_result_evidence%' then
                raise;
            end if;
    end;

    begin
        perform public.cex_matrix_reconcile_adapter_result_v1(
            delivery,
            '$event-hardening-1',
            payload_hash,
            '!room-hardening:example',
            '@user-hardening:example',
            'task-hardening-1',
            payload,
            result_hash,
            jsonb_set(
                evidence,
                '{observed_at}',
                to_jsonb('not-a-timestamp'::text),
                false
            )
        );
        raise exception 'matrix_result_invalid_observed_at_not_rejected';
    exception
        when others then
            if sqlerrm not like '%matrix_adapter_result_evidence_observed_at_invalid%' then
                raise;
            end if;
    end;

    begin
        perform public.cex_matrix_reconcile_adapter_result_v1(
            delivery,
            '$event-hardening-1',
            payload_hash,
            '!room-hardening:example',
            '@user-hardening:example',
            'task-hardening-1',
            payload,
            result_hash,
            jsonb_set(
                evidence,
                '{observed_at}',
                to_jsonb('infinity'::text),
                false
            )
        );
        raise exception 'matrix_result_infinite_observed_at_not_rejected';
    exception
        when others then
            if sqlerrm not like '%matrix_adapter_result_evidence_observed_at_invalid%' then
                raise;
            end if;
    end;
end;
$matrix_result_evidence_hardening$;

rollback;
