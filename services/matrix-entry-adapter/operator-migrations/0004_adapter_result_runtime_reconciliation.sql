begin;

-- Close the runtime reconciliation gap left by 0003. The previous replacement
-- validated p_result_payload but omitted it from the INSERT into the NOT NULL
-- result_payload column, so every first successful reconciliation failed at
-- runtime. This replacement preserves the v1 signature, restores exact payload
-- persistence, compares it on replay, and admits every relay error explicitly
-- classified as an unknown adapter outcome.
create or replace function public.cex_matrix_reconcile_adapter_result_v1(
    p_delivery_id uuid,
    p_source_event_id text,
    p_payload_sha256 text,
    p_room_id text,
    p_principal_user_id text,
    p_task_id text,
    p_result_payload jsonb,
    p_result_sha256 text,
    p_evidence_context jsonb
)
returns text
language plpgsql
security definer
set search_path = pg_catalog, public
as $$
declare
    delivery_row public.matrix_transport_outbox%rowtype;
    existing public.matrix_transport_adapter_result_reconciliations%rowtype;
    expected_result_sha256 text;
    observed_at timestamptz;
begin
    if p_delivery_id is null then
        raise exception 'invalid_matrix_adapter_result_delivery';
    end if;
    if p_source_event_id is null
        or octet_length(p_source_event_id) not between 2 and 512
        or left(p_source_event_id, 1) <> '$'
        or p_source_event_id ~ '[[:space:][:cntrl:]]'
    then
        raise exception 'invalid_matrix_adapter_result_source';
    end if;
    if p_payload_sha256 is null
        or p_payload_sha256 !~ '^sha256:[0-9a-f]{64}$'
    then
        raise exception 'invalid_matrix_adapter_result_payload_hash';
    end if;
    if p_room_id is null
        or octet_length(p_room_id) not between 2 and 512
        or left(p_room_id, 1) <> '!'
        or p_room_id ~ '[[:space:][:cntrl:]]'
    then
        raise exception 'invalid_matrix_adapter_result_room';
    end if;
    if p_principal_user_id is null
        or octet_length(p_principal_user_id) not between 2 and 512
        or left(p_principal_user_id, 1) <> '@'
        or p_principal_user_id ~ '[[:space:][:cntrl:]]'
    then
        raise exception 'invalid_matrix_adapter_result_principal';
    end if;
    if p_task_id is null or octet_length(p_task_id) not between 1 and 128 then
        raise exception 'invalid_matrix_adapter_result_task';
    end if;
    if p_result_payload is null
        or jsonb_typeof(p_result_payload) <> 'object'
        or pg_column_size(p_result_payload) > 1048576
    then
        raise exception 'invalid_matrix_adapter_result_payload';
    end if;
    if p_result_sha256 is null
        or p_result_sha256 !~ '^sha256:[0-9a-f]{64}$'
    then
        raise exception 'invalid_matrix_adapter_result_hash';
    end if;

    expected_result_sha256 := 'sha256:' || encode(
        sha256(convert_to(p_result_payload::text, 'UTF8')),
        'hex'
    );
    if p_result_sha256 <> expected_result_sha256 then
        raise exception 'matrix_adapter_result_hash_mismatch';
    end if;

    if p_result_payload->>'task_id' is distinct from p_task_id
        or p_result_payload->'source'->>'kind' is distinct from 'matrix_message'
        or p_result_payload->'source'->>'event_id' is distinct from p_source_event_id
        or p_result_payload->'source'->>'room_id' is distinct from p_room_id
        or p_result_payload->'source'->>'matrix_user_id'
            is distinct from p_principal_user_id
    then
        raise exception 'matrix_adapter_result_identity_mismatch';
    end if;
    if jsonb_typeof(p_result_payload->'source'->'identity_scope')
            is distinct from 'object'
        or p_result_payload->'source'->'identity_scope'->>'user_id'
            is distinct from p_principal_user_id
        or p_result_payload->'source'->'identity_scope'->>'room_id'
            is distinct from p_room_id
    then
        raise exception 'matrix_adapter_result_identity_scope_mismatch';
    end if;
    if p_result_payload ? 'raw' then
        if jsonb_typeof(p_result_payload->'raw') is distinct from 'object' then
            raise exception 'matrix_adapter_result_raw_invalid';
        end if;
        if p_result_payload->'raw' ? 'invocation_id'
            and p_result_payload->'raw'->>'invocation_id' is distinct from p_task_id
        then
            raise exception 'matrix_adapter_result_raw_task_mismatch';
        end if;
    end if;

    if p_evidence_context is null
        or jsonb_typeof(p_evidence_context) <> 'object'
        or pg_column_size(p_evidence_context) > 16384
        or not p_evidence_context ?& array[
            'schema', 'lookup_response_sha256', 'observed_at', 'candidate_sha'
        ]
        or p_evidence_context - array[
            'schema', 'lookup_response_sha256', 'observed_at', 'candidate_sha'
        ] <> '{}'::jsonb
        or p_evidence_context->>'schema'
            is distinct from 'cex.matrix.adapter-result-reconciliation-evidence.v1'
        or jsonb_typeof(p_evidence_context->'lookup_response_sha256')
            is distinct from 'string'
        or p_evidence_context->>'lookup_response_sha256'
            !~ '^sha256:[0-9a-f]{64}$'
        or jsonb_typeof(p_evidence_context->'candidate_sha') is distinct from 'string'
        or octet_length(p_evidence_context->>'candidate_sha') not between 1 and 256
        or (p_evidence_context->>'candidate_sha') ~ '[[:cntrl:]]'
        or jsonb_typeof(p_evidence_context->'observed_at') is distinct from 'string'
    then
        raise exception 'invalid_matrix_adapter_result_evidence';
    end if;
    begin
        observed_at := (p_evidence_context->>'observed_at')::timestamptz;
    exception
        when invalid_datetime_format or datetime_field_overflow then
            raise exception 'matrix_adapter_result_evidence_observed_at_invalid';
    end;
    if not isfinite(observed_at)
        or observed_at > clock_timestamp() + interval '5 minutes'
    then
        raise exception 'matrix_adapter_result_evidence_observed_at_invalid';
    end if;

    select * into delivery_row
      from public.matrix_transport_outbox
     where delivery_id = p_delivery_id
     for update;
    if not found
        or delivery_row.source_event_id is distinct from p_source_event_id
        or delivery_row.payload_sha256 is distinct from p_payload_sha256
        or delivery_row.destination is distinct from 'matrix-relay-adapter-v1'
        or delivery_row.payload->>'event_id' is distinct from p_source_event_id
        or delivery_row.payload->>'room_id' is distinct from p_room_id
        or delivery_row.payload->>'sender' is distinct from p_principal_user_id
    then
        raise exception 'matrix_adapter_result_principal_mismatch';
    end if;

    select * into existing
      from public.matrix_transport_adapter_result_reconciliations
     where delivery_id = p_delivery_id;
    if found then
        if existing.source_event_id is distinct from p_source_event_id
            or existing.payload_sha256 is distinct from p_payload_sha256
            or existing.room_id is distinct from p_room_id
            or existing.principal_user_id is distinct from p_principal_user_id
            or existing.task_id is distinct from p_task_id
            or existing.result_payload is distinct from p_result_payload
            or existing.result_sha256 is distinct from p_result_sha256
            or existing.evidence_context is distinct from p_evidence_context
            or delivery_row.status is distinct from 'sent'
        then
            raise exception 'matrix_adapter_result_reconciliation_collision';
        end if;
        return 'replay';
    end if;

    if delivery_row.status <> 'dead_letter'
        or delivery_row.last_error_code is null
        or not (
            delivery_row.last_error_code like 'adapter_response_unknown_%'
            or delivery_row.last_error_code in (
                'adapter_unverified_oversized_response',
                'adapter_duplicate_outcome_unknown',
                'relay_internal_unknown_outcome'
            )
        )
    then
        raise exception 'matrix_adapter_result_not_reconcilable';
    end if;

    insert into public.matrix_transport_adapter_result_reconciliations (
        delivery_id,
        source_event_id,
        payload_sha256,
        room_id,
        principal_user_id,
        task_id,
        result_payload,
        result_sha256,
        evidence_context,
        recorded_by
    ) values (
        p_delivery_id,
        p_source_event_id,
        p_payload_sha256,
        p_room_id,
        p_principal_user_id,
        p_task_id,
        p_result_payload,
        p_result_sha256,
        p_evidence_context,
        session_user
    );

    update public.matrix_transport_outbox
       set status = 'sent',
           lease_owner = null,
           lease_expires_at = null,
           last_error_code = null,
           sent_at = clock_timestamp(),
           updated_at = clock_timestamp()
     where delivery_id = p_delivery_id
       and status = 'dead_letter';
    if not found then
        raise exception 'matrix_adapter_delivery_reconciliation_race';
    end if;

    insert into public.matrix_transport_delivery_history (
        delivery_id,
        lease_fence,
        from_status,
        to_status,
        owner,
        error_code
    ) values (
        p_delivery_id,
        delivery_row.lease_fence,
        'dead_letter',
        'sent',
        session_user,
        'adapter_result_reconciled'
    );
    return 'reconciled';
end;
$$;

alter function public.cex_matrix_reconcile_adapter_result_v1(
    uuid, text, text, text, text, text, jsonb, text, jsonb
) owner to cex_matrix_api_owner;
alter function public.cex_matrix_reconcile_adapter_result_v1(
    uuid, text, text, text, text, text, jsonb, text, jsonb
) security definer;
revoke all on function public.cex_matrix_reconcile_adapter_result_v1(
    uuid, text, text, text, text, text, jsonb, text, jsonb
) from public;
grant execute on function public.cex_matrix_reconcile_adapter_result_v1(
    uuid, text, text, text, text, text, jsonb, text, jsonb
) to cex_matrix_reconciler_runtime;

commit;
