begin;

alter table public.matrix_transport_adapter_result_reconciliations
    add column if not exists request_fingerprint text;

alter table public.matrix_transport_adapter_result_reconciliations
    disable trigger matrix_adapter_result_reconciliation_immutable_v1;

update public.matrix_transport_adapter_result_reconciliations
   set request_fingerprint = 'sha256:' || encode(
       sha256(
           int8send(octet_length('cex.matrix.adapter-result-delivery.v1')::bigint)
               || convert_to('cex.matrix.adapter-result-delivery.v1', 'UTF8')
               || int8send(octet_length(delivery_id::text)::bigint)
               || convert_to(delivery_id::text, 'UTF8')
               || int8send(octet_length(payload_sha256)::bigint)
               || convert_to(payload_sha256, 'UTF8')
               || int8send(octet_length(source_event_id)::bigint)
               || convert_to(source_event_id, 'UTF8')
               || int8send(octet_length(principal_user_id)::bigint)
               || convert_to(principal_user_id, 'UTF8')
               || int8send(octet_length(room_id)::bigint)
               || convert_to(room_id, 'UTF8')
       ),
       'hex'
   )
 where request_fingerprint is null;

alter table public.matrix_transport_adapter_result_reconciliations
    enable trigger matrix_adapter_result_reconciliation_immutable_v1;

alter table public.matrix_transport_adapter_result_reconciliations
    alter column request_fingerprint set not null;

do $matrix_adapter_result_request_fingerprint_constraint$
begin
    if not exists (
        select 1
          from pg_catalog.pg_constraint
         where conrelid =
               'public.matrix_transport_adapter_result_reconciliations'::regclass
           and conname =
               'matrix_transport_adapter_result_reconciliations_request_fingerprint_check'
    ) then
        alter table public.matrix_transport_adapter_result_reconciliations
            add constraint
                matrix_transport_adapter_result_reconciliations_request_fingerprint_check
            check (request_fingerprint ~ '^sha256:[0-9a-f]{64}$');
    end if;
end;
$matrix_adapter_result_request_fingerprint_constraint$;

create table if not exists public.matrix_transport_adapter_result_observations (
    delivery_id uuid not null
        references public.matrix_transport_adapter_result_reconciliations(delivery_id)
        on delete restrict,
    request_fingerprint text not null,
    result_sha256 text not null,
    lookup_response_sha256 text not null,
    observed_at timestamptz not null,
    candidate_sha text not null,
    recorded_by text not null,
    recorded_at timestamptz not null default clock_timestamp(),
    primary key (
        delivery_id,
        lookup_response_sha256,
        observed_at,
        candidate_sha
    ),
    check (request_fingerprint ~ '^sha256:[0-9a-f]{64}$'),
    check (result_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    check (lookup_response_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    check (candidate_sha ~ '^[0-9a-f]{40}$'),
    check (isfinite(observed_at)),
    check (octet_length(recorded_by) between 1 and 256)
);

drop trigger if exists matrix_adapter_result_observation_immutable_v1
    on public.matrix_transport_adapter_result_observations;
create trigger matrix_adapter_result_observation_immutable_v1
before update or delete on public.matrix_transport_adapter_result_observations
for each row execute function public.cex_matrix_reject_immutable_mutation_v1();

revoke all on public.matrix_transport_adapter_result_observations from public;
revoke all on public.matrix_transport_adapter_result_observations
    from cex_matrix_poller_runtime,
         cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime,
         cex_matrix_operator_runtime;
grant select, insert on public.matrix_transport_adapter_result_observations
    to cex_matrix_api_owner;

create or replace function public.cex_matrix_reconcile_adapter_result_v2(
    p_delivery_id uuid,
    p_source_event_id text,
    p_payload_sha256 text,
    p_room_id text,
    p_principal_user_id text,
    p_task_id text,
    p_request_fingerprint text,
    p_result_payload jsonb,
    p_result_sha256 text,
    p_evidence_context jsonb
)
returns text
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    delivery_row public.matrix_transport_outbox%rowtype;
    existing public.matrix_transport_adapter_result_reconciliations%rowtype;
    expected_request_fingerprint text;
    computed_result_sha256 text;
    observed_at timestamptz;
    lookup_response_sha256 text;
    candidate_sha text;
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
        or p_request_fingerprint is null
        or p_request_fingerprint !~ '^sha256:[0-9a-f]{64}$'
        or p_result_payload is null
        or jsonb_typeof(p_result_payload) <> 'object'
        or pg_column_size(p_result_payload) > 1048576
        or p_result_sha256 is null
        or p_result_sha256 !~ '^sha256:[0-9a-f]{64}$'
        or p_evidence_context is null
        or jsonb_typeof(p_evidence_context) <> 'object'
        or pg_column_size(p_evidence_context) > 16384
    then
        raise exception 'invalid_matrix_adapter_result_reconciliation_v2';
    end if;

    if not p_evidence_context ?& array[
            'schema',
            'lookup_response_sha256',
            'observed_at',
            'candidate_sha',
            'request_fingerprint'
        ]
        or p_evidence_context - array[
            'schema',
            'lookup_response_sha256',
            'observed_at',
            'candidate_sha',
            'request_fingerprint'
        ] <> '{}'::jsonb
        or p_evidence_context->>'schema'
            is distinct from
            'cex.matrix.adapter-result-reconciliation-evidence.v2'
        or p_evidence_context->>'lookup_response_sha256' is null
        or p_evidence_context->>'lookup_response_sha256'
            !~ '^sha256:[0-9a-f]{64}$'
        or p_evidence_context->>'candidate_sha' is null
        or p_evidence_context->>'candidate_sha'
            !~ '^[0-9a-f]{40}$'
        or p_evidence_context->>'request_fingerprint'
            is distinct from p_request_fingerprint
    then
        raise exception 'invalid_matrix_adapter_result_evidence_v2';
    end if;

    begin
        observed_at := (p_evidence_context->>'observed_at')::timestamptz;
    exception
        when others then
            raise exception 'invalid_matrix_adapter_result_observed_at_v2';
    end;
    if observed_at is null
        or not isfinite(observed_at)
        or observed_at > clock_timestamp() + interval '5 minutes'
        or observed_at < clock_timestamp() - interval '15 minutes'
    then
        raise exception 'matrix_adapter_result_observation_outside_window';
    end if;
    lookup_response_sha256 :=
        p_evidence_context->>'lookup_response_sha256';
    candidate_sha := p_evidence_context->>'candidate_sha';

    computed_result_sha256 := 'sha256:' || encode(
        sha256(convert_to(p_result_payload::text, 'UTF8')),
        'hex'
    );
    if computed_result_sha256 is distinct from p_result_sha256 then
        raise exception 'matrix_adapter_result_hash_mismatch';
    end if;

    if p_result_payload->>'task_id' is distinct from p_task_id
        or jsonb_typeof(p_result_payload->'source') is distinct from 'object'
        or jsonb_typeof(
            p_result_payload->'source'->'identity_scope'
        ) is distinct from 'object'
        or (
            p_result_payload ? 'raw'
            and jsonb_typeof(p_result_payload->'raw') is distinct from 'object'
        )
        or p_result_payload->'source'->>'kind'
            is distinct from 'matrix_message'
        or p_result_payload->'source'->>'event_id'
            is distinct from p_source_event_id
        or p_result_payload->'source'->>'room_id'
            is distinct from p_room_id
        or p_result_payload->'source'->>'matrix_user_id'
            is distinct from p_principal_user_id
        or p_result_payload->'source'->'identity_scope'->>'user_id'
            is distinct from p_principal_user_id
        or p_result_payload->'source'->'identity_scope'->>'room_id'
            is distinct from p_room_id
        or (
            p_result_payload ? 'raw'
            and p_result_payload->'raw'->>'invocation_id' is not null
            and p_result_payload->'raw'->>'invocation_id'
                is distinct from p_task_id
        )
    then
        raise exception 'matrix_adapter_result_principal_mismatch_v2';
    end if;

    select *
      into delivery_row
      from public.matrix_transport_outbox
     where delivery_id = p_delivery_id
     for update;
    if not found then
        raise exception 'matrix_adapter_delivery_not_found';
    end if;

    if delivery_row.destination is distinct from 'matrix-relay-adapter-v1'
        or delivery_row.source_event_id is distinct from p_source_event_id
        or delivery_row.payload_sha256 is distinct from p_payload_sha256
        or delivery_row.payload->>'event_id'
            is distinct from p_source_event_id
        or delivery_row.payload->>'room_id'
            is distinct from p_room_id
        or delivery_row.payload->>'sender'
            is distinct from p_principal_user_id
    then
        raise exception 'matrix_adapter_delivery_identity_mismatch_v2';
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
                || int8send(octet_length(delivery_row.payload_sha256)::bigint)
                || convert_to(delivery_row.payload_sha256, 'UTF8')
                || int8send(octet_length(delivery_row.source_event_id)::bigint)
                || convert_to(delivery_row.source_event_id, 'UTF8')
                || int8send(octet_length(p_principal_user_id)::bigint)
                || convert_to(p_principal_user_id, 'UTF8')
                || int8send(octet_length(p_room_id)::bigint)
                || convert_to(p_room_id, 'UTF8')
        ),
        'hex'
    );
    if p_request_fingerprint is distinct from expected_request_fingerprint then
        raise exception 'matrix_adapter_result_request_fingerprint_mismatch';
    end if;

    select *
      into existing
      from public.matrix_transport_adapter_result_reconciliations
     where delivery_id = p_delivery_id;
    if found then
        if existing.source_event_id is distinct from p_source_event_id
            or existing.payload_sha256 is distinct from p_payload_sha256
            or existing.room_id is distinct from p_room_id
            or existing.principal_user_id
                is distinct from p_principal_user_id
            or existing.task_id is distinct from p_task_id
            or existing.request_fingerprint
                is distinct from p_request_fingerprint
            or existing.result_payload is distinct from p_result_payload
            or existing.result_sha256 is distinct from p_result_sha256
            or delivery_row.status is distinct from 'sent'
        then
            raise exception 'matrix_adapter_result_reconciliation_collision_v2';
        end if;

        insert into public.matrix_transport_adapter_result_observations (
            delivery_id,
            request_fingerprint,
            result_sha256,
            lookup_response_sha256,
            observed_at,
            candidate_sha,
            recorded_by
        ) values (
            p_delivery_id,
            p_request_fingerprint,
            p_result_sha256,
            lookup_response_sha256,
            observed_at,
            candidate_sha,
            session_user
        )
        on conflict do nothing;
        return 'replay';
    end if;

    if delivery_row.status is distinct from 'dead_letter'
        or delivery_row.last_error_code is null
        or not (
            delivery_row.last_error_code
                like 'adapter_response_unknown_%'
            or delivery_row.last_error_code =
                'adapter_unverified_oversized_response'
            or delivery_row.last_error_code =
                'adapter_duplicate_outcome_unknown'
            or delivery_row.last_error_code =
                'relay_internal_unknown_outcome'
        )
    then
        raise exception 'matrix_adapter_delivery_not_reconcilable';
    end if;

    insert into public.matrix_transport_adapter_result_reconciliations (
        delivery_id,
        source_event_id,
        payload_sha256,
        room_id,
        principal_user_id,
        task_id,
        request_fingerprint,
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
        p_request_fingerprint,
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
        'adapter_result_reconciled_v2'
    );

    insert into public.matrix_transport_adapter_result_observations (
        delivery_id,
        request_fingerprint,
        result_sha256,
        lookup_response_sha256,
        observed_at,
        candidate_sha,
        recorded_by
    ) values (
        p_delivery_id,
        p_request_fingerprint,
        p_result_sha256,
        lookup_response_sha256,
        observed_at,
        candidate_sha,
        session_user
    )
    on conflict do nothing;

    return 'reconciled';
end;
$$;

alter function public.cex_matrix_reconcile_adapter_result_v2(
    uuid, text, text, text, text, text, text, jsonb, text, jsonb
) owner to cex_matrix_api_owner;
alter function public.cex_matrix_reconcile_adapter_result_v2(
    uuid, text, text, text, text, text, text, jsonb, text, jsonb
) security definer;

revoke all on function public.cex_matrix_reconcile_adapter_result_v1(
    uuid, text, text, text, text, text, jsonb, text, jsonb
) from cex_matrix_reconciler_runtime;
revoke all on function public.cex_matrix_reconcile_adapter_result_v2(
    uuid, text, text, text, text, text, text, jsonb, text, jsonb
) from public;
grant execute on function public.cex_matrix_reconcile_adapter_result_v2(
    uuid, text, text, text, text, text, text, jsonb, text, jsonb
) to cex_matrix_reconciler_runtime;

commit;
