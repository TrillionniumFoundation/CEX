begin;

do $matrix_embedded_binding_prerequisites$
begin
    if to_regprocedure(
        'public.cex_matrix_reconcile_adapter_result_v2(uuid,text,text,text,text,text,text,jsonb,text,jsonb)'
    ) is null then
        raise exception 'matrix_adapter_result_v2_function_missing';
    end if;
    if not exists (select 1 from pg_roles where rolname = 'cex_matrix_api_owner') then
        raise exception 'matrix_api_owner_role_missing';
    end if;
    if not exists (select 1 from pg_roles where rolname = 'cex_matrix_reconciler_runtime') then
        raise exception 'matrix_reconciler_runtime_role_missing';
    end if;
end;
$matrix_embedded_binding_prerequisites$;

create or replace function public.cex_matrix_reconcile_adapter_result_v3(
    p_delivery_id uuid,
    p_source_event_id text,
    p_payload_sha256 text,
    p_room_id text,
    p_matrix_principal text,
    p_task_id text,
    p_request_fingerprint text,
    p_result_payload jsonb,
    p_result_sha256 text,
    p_evidence_context jsonb
)
returns text
language plpgsql
security definer
set search_path = pg_catalog, public
as $matrix_reconcile_v3$
declare
    embedded_binding jsonb;
    expected_request_fingerprint text;
begin
    if jsonb_typeof(p_result_payload) is distinct from 'object' then
        raise exception 'matrix_adapter_result_embedded_binding_invalid_v3';
    end if;

    embedded_binding := p_result_payload #>
        '{source,metadata,metadata,cex_delivery_binding}';
    if jsonb_typeof(embedded_binding) is distinct from 'object'
        or jsonb_object_length(embedded_binding) <> 8
        or not embedded_binding ?& array[
            'schema',
            'source',
            'delivery_id',
            'payload_sha256',
            'event_id',
            'room_id',
            'matrix_user_id',
            'request_fingerprint'
        ]
        or embedded_binding ->> 'schema'
            is distinct from 'cex.matrix.delivery-binding.v1'
        or embedded_binding ->> 'source'
            is distinct from 'matrix-bot-relay-headers-v1'
        or embedded_binding ->> 'delivery_id'
            is distinct from p_delivery_id::text
        or embedded_binding ->> 'payload_sha256'
            is distinct from p_payload_sha256
        or embedded_binding ->> 'event_id'
            is distinct from p_source_event_id
        or embedded_binding ->> 'room_id'
            is distinct from p_room_id
        or embedded_binding ->> 'matrix_user_id'
            is distinct from p_matrix_principal
        or embedded_binding ->> 'request_fingerprint'
            is distinct from p_request_fingerprint
    then
        raise exception 'matrix_adapter_result_embedded_binding_invalid_v3';
    end if;

    expected_request_fingerprint := 'sha256:' || encode(
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
                || int8send(octet_length(p_delivery_id::text)::bigint)
                || convert_to(p_delivery_id::text, 'UTF8')
                || int8send(octet_length(p_payload_sha256)::bigint)
                || convert_to(p_payload_sha256, 'UTF8')
                || int8send(octet_length(p_source_event_id)::bigint)
                || convert_to(p_source_event_id, 'UTF8')
                || int8send(octet_length(p_matrix_principal)::bigint)
                || convert_to(p_matrix_principal, 'UTF8')
                || int8send(octet_length(p_room_id)::bigint)
                || convert_to(p_room_id, 'UTF8')
        ),
        'hex'
    );
    if p_request_fingerprint is distinct from expected_request_fingerprint then
        raise exception 'matrix_adapter_result_request_fingerprint_mismatch_v3';
    end if;

    return public.cex_matrix_reconcile_adapter_result_v2(
        p_delivery_id,
        p_source_event_id,
        p_payload_sha256,
        p_room_id,
        p_matrix_principal,
        p_task_id,
        p_request_fingerprint,
        p_result_payload,
        p_result_sha256,
        p_evidence_context
    );
end;
$matrix_reconcile_v3$;

alter function public.cex_matrix_reconcile_adapter_result_v3(
    uuid,
    text,
    text,
    text,
    text,
    text,
    text,
    jsonb,
    text,
    jsonb
) owner to cex_matrix_api_owner;

revoke all on function public.cex_matrix_reconcile_adapter_result_v2(
    uuid,
    text,
    text,
    text,
    text,
    text,
    text,
    jsonb,
    text,
    jsonb
) from public, cex_matrix_reconciler_runtime;

revoke all on function public.cex_matrix_reconcile_adapter_result_v3(
    uuid,
    text,
    text,
    text,
    text,
    text,
    text,
    jsonb,
    text,
    jsonb
) from public;

grant execute on function public.cex_matrix_reconcile_adapter_result_v3(
    uuid,
    text,
    text,
    text,
    text,
    text,
    text,
    jsonb,
    text,
    jsonb
) to cex_matrix_reconciler_runtime;

comment on function public.cex_matrix_reconcile_adapter_result_v3(
    uuid,
    text,
    text,
    text,
    text,
    text,
    text,
    jsonb,
    text,
    jsonb
) is
'Validates the relay-origin delivery binding embedded in the durable consumer result, then delegates to the row-locking v2 reconciliation core. Runtime access to v2 is revoked.';

commit;
