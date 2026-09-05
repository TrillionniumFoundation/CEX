begin;

do $matrix_roles$
declare
    role_name text;
    role_state record;
begin
    foreach role_name in array array[
        'cex_matrix_api_owner',
        'cex_matrix_poller_runtime',
        'cex_matrix_relay_runtime',
        'cex_matrix_reconciler_runtime',
        'cex_matrix_operator_runtime'
    ] loop
        select rolcanlogin, rolsuper, rolcreatedb, rolcreaterole, rolreplication,
               rolbypassrls
          into role_state
          from pg_catalog.pg_roles
         where rolname = role_name;
        if not found then
            execute format('create role %I nologin', role_name);
        elsif role_state.rolcanlogin
            or role_state.rolsuper
            or role_state.rolcreatedb
            or role_state.rolcreaterole
            or role_state.rolreplication
            or role_state.rolbypassrls
        then
            raise exception 'matrix_runtime_role_collision';
        end if;
    end loop;
end;
$matrix_roles$;

alter role cex_matrix_api_owner nologin noinherit nosuperuser nocreatedb
    nocreaterole noreplication nobypassrls;
alter role cex_matrix_poller_runtime nologin nosuperuser nocreatedb
    nocreaterole noreplication nobypassrls;
alter role cex_matrix_relay_runtime nologin nosuperuser nocreatedb
    nocreaterole noreplication nobypassrls;
alter role cex_matrix_reconciler_runtime nologin nosuperuser nocreatedb
    nocreaterole noreplication nobypassrls;
alter role cex_matrix_operator_runtime nologin nosuperuser nocreatedb
    nocreaterole noreplication nobypassrls;

grant usage on schema public to cex_matrix_api_owner;
grant usage on schema public to cex_matrix_poller_runtime;
grant usage on schema public to cex_matrix_relay_runtime;
grant usage on schema public to cex_matrix_reconciler_runtime;
grant usage on schema public to cex_matrix_operator_runtime;

revoke all on public.matrix_transport_cursors from public;
revoke all on public.matrix_transport_inbox from public;
revoke all on public.matrix_transport_outbox from public;
revoke all on public.matrix_transport_delivery_history from public;
revoke all on public.matrix_transport_poison_events from public;
revoke all on public.matrix_transport_source_observations from public;
revoke all on public.matrix_transport_poison_payloads from public;
revoke all on public.matrix_transport_cursor_history from public;
revoke all on public.matrix_transport_send_bindings from public;
revoke all on public.matrix_transport_send_receipts from public;
revoke all on public.matrix_transport_stream_scopes from public;
revoke all on public.matrix_transport_filter_definitions from public;
revoke all on public.matrix_transport_adapter_result_reconciliations from public;

revoke all on public.matrix_transport_cursors
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_inbox
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_outbox
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_delivery_history
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_poison_events
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_source_observations
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_poison_payloads
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_cursor_history
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_send_bindings
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_send_receipts
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_stream_scopes
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_filter_definitions
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;
revoke all on public.matrix_transport_adapter_result_reconciliations
    from cex_matrix_poller_runtime, cex_matrix_relay_runtime,
         cex_matrix_reconciler_runtime, cex_matrix_operator_runtime;

grant select, insert, update on public.matrix_transport_cursors
    to cex_matrix_api_owner;
grant select, insert, update on public.matrix_transport_inbox
    to cex_matrix_api_owner;
grant select, insert, update on public.matrix_transport_outbox
    to cex_matrix_api_owner;
grant select, insert on public.matrix_transport_delivery_history
    to cex_matrix_api_owner;
grant select, insert, update on public.matrix_transport_poison_events
    to cex_matrix_api_owner;
grant select, insert on public.matrix_transport_source_observations
    to cex_matrix_api_owner;
grant select, insert on public.matrix_transport_poison_payloads
    to cex_matrix_api_owner;
grant select, insert on public.matrix_transport_cursor_history
    to cex_matrix_api_owner;
grant select, insert on public.matrix_transport_send_bindings
    to cex_matrix_api_owner;
grant select, insert on public.matrix_transport_send_receipts
    to cex_matrix_api_owner;
grant select, insert on public.matrix_transport_stream_scopes
    to cex_matrix_api_owner;
grant select, insert on public.matrix_transport_filter_definitions
    to cex_matrix_api_owner;
grant select, insert on public.matrix_transport_adapter_result_reconciliations
    to cex_matrix_api_owner;
grant usage, select on sequence public.matrix_transport_delivery_history_history_id_seq
    to cex_matrix_api_owner;
grant usage, select on sequence public.matrix_transport_source_observations_observation_id_seq
    to cex_matrix_api_owner;

alter function public.cex_matrix_acquire_cursor_lease_v1(text, text, integer)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_acquire_cursor_lease_v1(text, text, integer)
    security definer;
alter function public.cex_matrix_advance_cursor_v1(text, text, bigint, bigint, text)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_advance_cursor_v1(text, text, bigint, bigint, text)
    security definer;
alter function public.cex_matrix_accept_source_event_v1(text, text, text, text)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_accept_source_event_v1(text, text, text, text)
    security definer;
alter function public.cex_matrix_enqueue_delivery_v1(uuid, text, text, text, jsonb, integer)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_enqueue_delivery_v1(uuid, text, text, text, jsonb, integer)
    security definer;
alter function public.cex_matrix_register_delivery_and_advance_v1(
    text, text, bigint, bigint, text, text, text, text,
    uuid, text, text, jsonb, integer
) owner to cex_matrix_api_owner;
alter function public.cex_matrix_register_delivery_and_advance_v1(
    text, text, bigint, bigint, text, text, text, text,
    uuid, text, text, jsonb, integer
) security definer;
alter function public.cex_matrix_claim_delivery_v1(text, integer, integer)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_claim_delivery_v1(text, integer, integer)
    security definer;
alter function public.cex_matrix_finish_delivery_v1(uuid, text, bigint, text, text)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_finish_delivery_v1(uuid, text, bigint, text, text)
    security definer;
alter function public.cex_matrix_lookup_delivery_v1(uuid, text)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_lookup_delivery_v1(uuid, text)
    security definer;
alter function public.cex_matrix_record_poison_event_v1(text, text, text, text)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_record_poison_event_v1(text, text, text, text)
    security definer;
alter function public.cex_matrix_acknowledge_poison_event_v1(text, text, text, text)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_acknowledge_poison_event_v1(text, text, text, text)
    security definer;
alter function public.cex_matrix_renew_cursor_lease_v1(text, text, bigint, bigint, integer)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_renew_cursor_lease_v1(text, text, bigint, bigint, integer)
    security definer;
alter function public.cex_matrix_store_poison_payload_v1(text, text, text, text, jsonb)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_store_poison_payload_v1(text, text, text, text, jsonb)
    security definer;
alter function public.cex_matrix_bind_send_attempt_v1(
    uuid, text, bigint, text, text, text, text
) owner to cex_matrix_api_owner;
alter function public.cex_matrix_bind_send_attempt_v1(
    uuid, text, bigint, text, text, text, text
) security definer;
alter function public.cex_matrix_record_send_receipt_v1(
    uuid, text, bigint, text, text, text
) owner to cex_matrix_api_owner;
alter function public.cex_matrix_record_send_receipt_v1(
    uuid, text, bigint, text, text, text
) security definer;
alter function public.cex_matrix_bind_stream_scope_v1(text, text, bigint, bigint, jsonb)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_bind_stream_scope_v1(text, text, bigint, bigint, jsonb)
    security definer;
alter function public.cex_matrix_bind_filter_definition_v1(
    text, text, bigint, bigint, text, text, text
) owner to cex_matrix_api_owner;
alter function public.cex_matrix_bind_filter_definition_v1(
    text, text, bigint, bigint, text, text, text
) security definer;
alter function public.cex_matrix_reconcile_adapter_result_v1(
    uuid, text, text, text, text, text, jsonb, text, jsonb
) owner to cex_matrix_api_owner;
alter function public.cex_matrix_reconcile_adapter_result_v1(
    uuid, text, text, text, text, text, jsonb, text, jsonb
) security definer;
alter function public.cex_matrix_lookup_adapter_result_reconciliation_v1(uuid)
    owner to cex_matrix_api_owner;
alter function public.cex_matrix_lookup_adapter_result_reconciliation_v1(uuid)
    security definer;

revoke all on function public.cex_matrix_acquire_cursor_lease_v1(text, text, integer)
    from public;
revoke all on function public.cex_matrix_advance_cursor_v1(text, text, bigint, bigint, text)
    from public;
revoke all on function public.cex_matrix_accept_source_event_v1(text, text, text, text)
    from public;
revoke all on function public.cex_matrix_enqueue_delivery_v1(uuid, text, text, text, jsonb, integer)
    from public;
revoke all on function public.cex_matrix_register_delivery_and_advance_v1(
    text, text, bigint, bigint, text, text, text, text,
    uuid, text, text, jsonb, integer
) from public;
revoke all on function public.cex_matrix_claim_delivery_v1(text, integer, integer)
    from public;
revoke all on function public.cex_matrix_finish_delivery_v1(uuid, text, bigint, text, text)
    from public;
revoke all on function public.cex_matrix_lookup_delivery_v1(uuid, text)
    from public;
revoke all on function public.cex_matrix_record_poison_event_v1(text, text, text, text)
    from public;
revoke all on function public.cex_matrix_acknowledge_poison_event_v1(text, text, text, text)
    from public;
revoke all on function public.cex_matrix_renew_cursor_lease_v1(text, text, bigint, bigint, integer)
    from public;
revoke all on function public.cex_matrix_store_poison_payload_v1(text, text, text, text, jsonb)
    from public;
revoke all on function public.cex_matrix_bind_send_attempt_v1(
    uuid, text, bigint, text, text, text, text
) from public;
revoke all on function public.cex_matrix_record_send_receipt_v1(
    uuid, text, bigint, text, text, text
) from public;
revoke all on function public.cex_matrix_bind_stream_scope_v1(text, text, bigint, bigint, jsonb)
    from public;
revoke all on function public.cex_matrix_bind_filter_definition_v1(
    text, text, bigint, bigint, text, text, text
) from public;
revoke all on function public.cex_matrix_reconcile_adapter_result_v1(
    uuid, text, text, text, text, text, jsonb, text, jsonb
) from public;
revoke all on function public.cex_matrix_lookup_adapter_result_reconciliation_v1(uuid)
    from public;

grant execute on function public.cex_matrix_acquire_cursor_lease_v1(text, text, integer)
    to cex_matrix_poller_runtime;
grant execute on function public.cex_matrix_advance_cursor_v1(text, text, bigint, bigint, text)
    to cex_matrix_poller_runtime;
grant execute on function public.cex_matrix_accept_source_event_v1(text, text, text, text)
    to cex_matrix_poller_runtime, cex_matrix_relay_runtime;
grant execute on function public.cex_matrix_enqueue_delivery_v1(uuid, text, text, text, jsonb, integer)
    to cex_matrix_poller_runtime, cex_matrix_relay_runtime;
grant execute on function public.cex_matrix_register_delivery_and_advance_v1(
    text, text, bigint, bigint, text, text, text, text,
    uuid, text, text, jsonb, integer
) to cex_matrix_poller_runtime;
grant execute on function public.cex_matrix_renew_cursor_lease_v1(
    text, text, bigint, bigint, integer
) to cex_matrix_poller_runtime;
grant execute on function public.cex_matrix_store_poison_payload_v1(
    text, text, text, text, jsonb
) to cex_matrix_poller_runtime;
grant execute on function public.cex_matrix_bind_stream_scope_v1(
    text, text, bigint, bigint, jsonb
) to cex_matrix_poller_runtime;
grant execute on function public.cex_matrix_bind_filter_definition_v1(
    text, text, bigint, bigint, text, text, text
) to cex_matrix_poller_runtime;
grant execute on function public.cex_matrix_record_poison_event_v1(text, text, text, text)
    to cex_matrix_poller_runtime, cex_matrix_relay_runtime;

grant execute on function public.cex_matrix_claim_delivery_v1(text, integer, integer)
    to cex_matrix_relay_runtime;
grant execute on function public.cex_matrix_finish_delivery_v1(uuid, text, bigint, text, text)
    to cex_matrix_relay_runtime;
grant execute on function public.cex_matrix_lookup_delivery_v1(uuid, text)
    to cex_matrix_relay_runtime;
grant execute on function public.cex_matrix_bind_send_attempt_v1(
    uuid, text, bigint, text, text, text, text
) to cex_matrix_relay_runtime;
grant execute on function public.cex_matrix_record_send_receipt_v1(
    uuid, text, bigint, text, text, text
) to cex_matrix_relay_runtime;

grant execute on function public.cex_matrix_reconcile_adapter_result_v1(
    uuid, text, text, text, text, text, jsonb, text, jsonb
) to cex_matrix_reconciler_runtime;
grant execute on function public.cex_matrix_lookup_adapter_result_reconciliation_v1(uuid)
    to cex_matrix_reconciler_runtime;
grant execute on function public.cex_matrix_acknowledge_poison_event_v1(text, text, text, text)
    to cex_matrix_operator_runtime;

grant select (
    partition_id, lease_owner, lease_fence, cursor_revision, lease_expires_at
) on public.matrix_transport_cursors to cex_matrix_poller_runtime;
grant update (updated_at) on public.matrix_transport_cursors
    to cex_matrix_poller_runtime;
grant select (partition_id, acknowledged_at)
    on public.matrix_transport_poison_events to cex_matrix_poller_runtime;

grant select (status) on public.matrix_transport_outbox
    to cex_matrix_relay_runtime;
grant select (source_event_id, source_event_sha256, partition_id)
    on public.matrix_transport_inbox to cex_matrix_relay_runtime;

commit;
