begin;

create table if not exists public.matrix_transport_adapter_result_reconciliations (
    delivery_id uuid primary key
        references public.matrix_transport_outbox(delivery_id) on delete restrict,
    source_event_id text not null,
    payload_sha256 text not null,
    room_id text not null,
    principal_user_id text not null,
    task_id text not null,
    result_payload jsonb not null,
    result_sha256 text not null,
    evidence_context jsonb not null,
    recorded_by text not null,
    recorded_at timestamptz not null default clock_timestamp(),
    unique (source_event_id),
    check (octet_length(source_event_id) between 2 and 512),
    check (left(source_event_id, 1) = '$'),
    check (source_event_id !~ '[[:space:][:cntrl:]]'),
    check (payload_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    check (octet_length(room_id) between 2 and 512),
    check (left(room_id, 1) = '!'),
    check (room_id !~ '[[:space:][:cntrl:]]'),
    check (octet_length(principal_user_id) between 2 and 512),
    check (left(principal_user_id, 1) = '@'),
    check (principal_user_id !~ '[[:space:][:cntrl:]]'),
    check (octet_length(task_id) between 1 and 128),
    check (pg_column_size(result_payload) <= 1048576),
    check (result_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    check (jsonb_typeof(evidence_context) = 'object'),
    check (pg_column_size(evidence_context) <= 16384),
    check (octet_length(recorded_by) between 1 and 256)
);

drop trigger if exists matrix_adapter_result_reconciliation_immutable_v1
    on public.matrix_transport_adapter_result_reconciliations;
create trigger matrix_adapter_result_reconciliation_immutable_v1
before update or delete on public.matrix_transport_adapter_result_reconciliations
for each row execute function public.cex_matrix_reject_immutable_mutation_v1();

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
set search_path = pg_catalog, public
as $$
declare
    delivery public.matrix_transport_outbox%rowtype;
    prior public.matrix_transport_adapter_result_reconciliations%rowtype;
    computed_result_sha256 text;
    caller_identity text;
begin
    if p_delivery_id is null
        or p_source_event_id is null
        or octet_length(p_source_event_id) not between 2 and 512
        or left(p_source_event_id, 1) <> '$'
        or p_source_event_id ~ '[[:space:][:cntrl:]]'
        or p_payload_sha256 is null
        or p_payload_sha256 !~ '^sha256:[0-9a-f]{64}$'
        or p_room_id is null
        or octet_length(p_room_id) not between 2 and 512
        or left(p_room_id, 1) <> '!'
        or p_room_id ~ '[[:space:][:cntrl:]]'
        or p_principal_user_id is null
        or octet_length(p_principal_user_id) not between 2 and 512
        or left(p_principal_user_id, 1) <> '@'
        or p_principal_user_id ~ '[[:space:][:cntrl:]]'
        or p_task_id is null
        or octet_length(p_task_id) not between 1 and 128
        or p_result_payload is null
        or jsonb_typeof(p_result_payload) <> 'object'
        or pg_column_size(p_result_payload) > 1048576
        or p_result_sha256 is null
        or p_result_sha256 !~ '^sha256:[0-9a-f]{64}$'
        or p_evidence_context is null
        or jsonb_typeof(p_evidence_context) <> 'object'
        or pg_column_size(p_evidence_context) > 16384
    then
        raise exception 'invalid_matrix_adapter_result_reconciliation';
    end if;

    if p_evidence_context->>'schema'
            is distinct from 'cex.matrix.adapter-result-reconciliation-evidence.v1'
        or p_evidence_context->>'lookup_response_sha256'
            !~ '^sha256:[0-9a-f]{64}$'
        or p_evidence_context->>'observed_at' is null
        or octet_length(p_evidence_context->>'observed_at') not between 1 and 128
    then
        raise exception 'invalid_matrix_adapter_result_evidence';
    end if;

    computed_result_sha256 := 'sha256:' || encode(
        sha256(convert_to(p_result_payload::text, 'UTF8')),
        'hex'
    );
    if computed_result_sha256 is distinct from p_result_sha256 then
        raise exception 'matrix_adapter_result_hash_mismatch';
    end if;

    if p_result_payload->>'task_id' is distinct from p_task_id
        or p_result_payload->'source'->>'kind' is distinct from 'matrix_message'
        or p_result_payload->'source'->>'event_id' is distinct from p_source_event_id
        or p_result_payload->'source'->>'room_id' is distinct from p_room_id
        or p_result_payload->'source'->>'matrix_user_id'
            is distinct from p_principal_user_id
    then
        raise exception 'matrix_adapter_result_principal_mismatch';
    end if;

    select * into delivery
      from public.matrix_transport_outbox
     where delivery_id = p_delivery_id
     for update;
    if not found then
        raise exception 'matrix_adapter_delivery_not_found';
    end if;

    if delivery.destination is distinct from 'matrix-relay-adapter-v1'
        or delivery.source_event_id is distinct from p_source_event_id
        or delivery.payload_sha256 is distinct from p_payload_sha256
        or delivery.payload->>'event_id' is distinct from p_source_event_id
        or delivery.payload->>'room_id' is distinct from p_room_id
        or delivery.payload->>'sender' is distinct from p_principal_user_id
    then
        raise exception 'matrix_adapter_delivery_identity_mismatch';
    end if;

    select * into prior
      from public.matrix_transport_adapter_result_reconciliations
     where delivery_id = p_delivery_id;
    if found then
        if prior.source_event_id is distinct from p_source_event_id
            or prior.payload_sha256 is distinct from p_payload_sha256
            or prior.room_id is distinct from p_room_id
            or prior.principal_user_id is distinct from p_principal_user_id
            or prior.task_id is distinct from p_task_id
            or prior.result_payload is distinct from p_result_payload
            or prior.result_sha256 is distinct from p_result_sha256
            or prior.evidence_context is distinct from p_evidence_context
            or delivery.status is distinct from 'sent'
        then
            raise exception 'matrix_adapter_result_reconciliation_collision';
        end if;
        return 'replay';
    end if;

    if delivery.status is distinct from 'dead_letter'
        or delivery.last_error_code is null
        or not (
            delivery.last_error_code like 'adapter_response_unknown_%'
            or delivery.last_error_code = 'adapter_unverified_oversized_response'
            or delivery.last_error_code = 'adapter_duplicate_outcome_unknown'
        )
    then
        raise exception 'matrix_adapter_delivery_not_reconcilable';
    end if;

    caller_identity := session_user::text;
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
        caller_identity
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
        delivery.lease_fence,
        'dead_letter',
        'sent',
        caller_identity,
        'adapter_result_reconciled'
    );

    return 'reconciled';
end;
$$;

create or replace function public.cex_matrix_lookup_adapter_result_reconciliation_v1(
    p_delivery_id uuid
)
returns table (
    delivery_id uuid,
    source_event_id text,
    payload_sha256 text,
    room_id text,
    principal_user_id text,
    task_id text,
    result_payload jsonb,
    result_sha256 text,
    evidence_context jsonb,
    recorded_by text,
    recorded_at timestamptz
)
language sql
stable
set search_path = pg_catalog, public
as $$
    select evidence.delivery_id,
           evidence.source_event_id,
           evidence.payload_sha256,
           evidence.room_id,
           evidence.principal_user_id,
           evidence.task_id,
           evidence.result_payload,
           evidence.result_sha256,
           evidence.evidence_context,
           evidence.recorded_by,
           evidence.recorded_at
      from public.matrix_transport_adapter_result_reconciliations as evidence
     where evidence.delivery_id = p_delivery_id
$$;

revoke all on public.matrix_transport_adapter_result_reconciliations from public;
revoke all on function public.cex_matrix_reconcile_adapter_result_v1(
    uuid, text, text, text, text, text, jsonb, text, jsonb
) from public;
revoke all on function public.cex_matrix_lookup_adapter_result_reconciliation_v1(uuid)
    from public;

commit;
