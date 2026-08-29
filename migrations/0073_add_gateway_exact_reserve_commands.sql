begin;

-- P0-N6 exact Gateway reserve registration and durable command execution.
-- The source transaction registers the immutable 0066 contract and inserts one
-- reserve command. Ledger HTTP is performed only after the claim transaction commits.

create table if not exists public.cex_gateway_ledger_reserve_commands_v1 (
    command_id uuid primary key,
    invocation_id uuid not null references public.invocations(invocation_id),
    org_id uuid not null references public.organizations(org_id),
    operation_id uuid not null unique,
    contract_hash text not null,
    request_payload jsonb not null,
    request_fingerprint text not null,
    execution_mode text not null default 'shadow',
    status text not null default 'pending',
    attempt_count integer not null default 0,
    max_attempts integer not null default 5,
    available_at timestamptz not null default now(),
    claimed_by text,
    lease_expires_at timestamptz,
    last_http_status integer,
    last_error_code text,
    last_error_message text,
    ledger_receipt jsonb,
    ledger_receipt_hash text,
    ledger_replayed boolean,
    operator_acknowledged_by text,
    operator_acknowledged_reason text,
    operator_acknowledged_at timestamptz,
    last_requeued_by text,
    last_requeue_reason text,
    last_requeue_additional_attempts integer,
    last_requeued_at timestamptz,
    requeue_count integer not null default 0,
    dead_lettered_at timestamptz,
    completed_at timestamptz,
    source_service text not null default 'gateway-service',
    source_principal text not null,
    schema_version text not null default 'cex.gateway.ledger-reserve-command.v1',
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint cex_gateway_ledger_reserve_invocation_unique_v1 unique (invocation_id),
    constraint cex_gateway_ledger_reserve_non_nil_ids_v1 check (
        command_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and invocation_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and org_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and operation_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ),
    constraint cex_gateway_ledger_reserve_mode_v1
        check (execution_mode in ('shadow', 'active')),
    constraint cex_gateway_ledger_reserve_status_v1 check (status in (
        'pending', 'claimed', 'retry_wait', 'succeeded',
        'reconcile_required', 'dead_letter', 'cancelled'
    )),
    constraint cex_gateway_ledger_reserve_attempts_v1 check (
        max_attempts between 1 and 100
        and attempt_count between 0 and max_attempts
        and requeue_count between 0 and 100
    ),
    constraint cex_gateway_ledger_reserve_request_object_v1
        check (jsonb_typeof(request_payload) = 'object'),
    constraint cex_gateway_ledger_reserve_receipt_object_v1
        check (ledger_receipt is null or jsonb_typeof(ledger_receipt) = 'object'),
    constraint cex_gateway_ledger_reserve_hashes_v1 check (
        contract_hash ~ '^sha256:[0-9a-f]{64}$'
        and request_fingerprint ~ '^sha256:[0-9a-f]{64}$'
        and (ledger_receipt_hash is null or ledger_receipt_hash ~ '^sha256:[0-9a-f]{64}$')
    ),
    constraint cex_gateway_ledger_reserve_source_v1 check (
        source_service = 'gateway-service'
        and length(btrim(source_principal)) between 1 and 256
        and schema_version = 'cex.gateway.ledger-reserve-command.v1'
    ),
    constraint cex_gateway_ledger_reserve_claim_shape_v1 check (
        (status = 'claimed' and claimed_by is not null and lease_expires_at is not null)
        or (status <> 'claimed' and claimed_by is null and lease_expires_at is null)
    ),
    constraint cex_gateway_ledger_reserve_success_shape_v1 check (
        (
            status = 'succeeded'
            and completed_at is not null
            and ledger_receipt is not null
            and ledger_receipt_hash is not null
            and ledger_replayed is not null
        )
        or (
            status <> 'succeeded'
            and completed_at is null
            and ledger_receipt is null
            and ledger_receipt_hash is null
            and ledger_replayed is null
        )
    ),
    constraint cex_gateway_ledger_reserve_dead_letter_shape_v1 check (
        (status = 'dead_letter' and dead_lettered_at is not null)
        or (status <> 'dead_letter' and dead_lettered_at is null)
    ),
    constraint cex_gateway_ledger_reserve_ack_shape_v1 check (
        (
            operator_acknowledged_at is null
            and operator_acknowledged_by is null
            and operator_acknowledged_reason is null
        )
        or (
            status in ('reconcile_required', 'dead_letter')
            and operator_acknowledged_at is not null
            and operator_acknowledged_by is not null
            and operator_acknowledged_reason is not null
        )
    ),
    constraint cex_gateway_ledger_reserve_requeue_shape_v1 check (
        (
            requeue_count = 0
            and last_requeued_at is null
            and last_requeued_by is null
            and last_requeue_reason is null
            and last_requeue_additional_attempts is null
        )
        or (
            requeue_count > 0
            and last_requeued_at is not null
            and last_requeued_by is not null
            and last_requeue_reason is not null
            and last_requeue_additional_attempts between 1 and 20
        )
    ),
    constraint cex_gateway_ledger_reserve_text_bounds_v1 check (
        (claimed_by is null or length(btrim(claimed_by)) between 1 and 128)
        and (last_error_code is null or length(last_error_code) between 1 and 128)
        and (last_error_message is null or length(last_error_message) <= 2000)
        and (
            operator_acknowledged_by is null
            or length(btrim(operator_acknowledged_by)) between 1 and 256
        )
        and (
            operator_acknowledged_reason is null
            or length(btrim(operator_acknowledged_reason)) between 1 and 1000
        )
        and (last_requeued_by is null or length(btrim(last_requeued_by)) between 1 and 256)
        and (last_requeue_reason is null or length(btrim(last_requeue_reason)) between 1 and 1000)
    ),
    constraint cex_gateway_ledger_reserve_http_v1
        check (last_http_status is null or last_http_status between 100 and 599),
    constraint cex_gateway_ledger_reserve_request_binding_v1 check (
        request_payload ->> 'reference_type' = 'invocation'
        and request_payload ->> 'reference_id' = invocation_id::text
        and request_payload ->> 'operation_id' = operation_id::text
        and request_payload ->> 'operation_kind' = 'reserve'
    )
);

create table if not exists public.cex_gateway_ledger_reserve_transitions_v1 (
    transition_id bigserial primary key,
    command_id uuid not null references public.cex_gateway_ledger_reserve_commands_v1(command_id),
    from_status text,
    to_status text not null,
    attempt_count integer not null,
    worker_id text,
    error_code text,
    receipt_hash text,
    occurred_at timestamptz not null default now(),
    constraint cex_gateway_ledger_reserve_transition_status_v1 check (
        (from_status is null or from_status in (
            'pending', 'claimed', 'retry_wait', 'succeeded',
            'reconcile_required', 'dead_letter', 'cancelled'
        ))
        and to_status in (
            'pending', 'claimed', 'retry_wait', 'succeeded',
            'reconcile_required', 'dead_letter', 'cancelled'
        )
    ),
    constraint cex_gateway_ledger_reserve_transition_attempt_v1
        check (attempt_count between 0 and 100),
    constraint cex_gateway_ledger_reserve_transition_text_v1 check (
        (worker_id is null or length(btrim(worker_id)) between 1 and 128)
        and (error_code is null or length(error_code) between 1 and 128)
    ),
    constraint cex_gateway_ledger_reserve_transition_hash_v1
        check (receipt_hash is null or receipt_hash ~ '^sha256:[0-9a-f]{64}$')
);

create or replace function public.cex_gateway_ledger_reserve_request_fingerprint_v1(
    p_request_payload jsonb
)
returns text
language sql
immutable
strict
set search_path = pg_catalog, public
as $$
    select 'sha256:' || encode(digest(p_request_payload::text, 'sha256'), 'hex')
$$;

create or replace function public.cex_gateway_ledger_reserve_receipt_hash_v1(
    p_receipt jsonb
)
returns text
language sql
immutable
strict
set search_path = pg_catalog, public
as $$
    select 'sha256:' || encode(digest(p_receipt::text, 'sha256'), 'hex')
$$;

create or replace function public.cex_gateway_legacy_money_is_zero_or_absent_v1(
    p_record jsonb,
    p_key text
)
returns boolean
language sql
immutable
strict
set search_path = pg_catalog, public
as $$
    select
        not (p_record ? p_key)
        or p_record -> p_key = 'null'::jsonb
        or coalesce(p_record ->> p_key, '') ~ '^-?0+(\.0+)?$'
$$;

create or replace function public.cex_validate_gateway_ledger_reserve_insert_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    invocation_org_id uuid;
    contract_row public.cex_invocation_ledger_contracts_v1%rowtype;
    expected_request jsonb;
    expected_operation_id uuid;
    expected_command_id uuid;
    expected_fingerprint text;
begin
    select org_id into invocation_org_id
      from public.invocations
     where invocation_id = new.invocation_id;
    if not found then
        raise exception 'Gateway reserve invocation binding does not exist';
    end if;
    if invocation_org_id is distinct from new.org_id then
        raise exception 'Gateway reserve invocation/tenant binding mismatch';
    end if;

    select * into contract_row
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = new.invocation_id;
    if not found then
        raise exception 'Gateway reserve durable Invocation contract does not exist';
    end if;
    if contract_row.org_id is distinct from new.org_id
       or contract_row.contract_hash is distinct from new.contract_hash
       or contract_row.status not in ('registered', 'reserved') then
        raise exception 'Gateway reserve command differs from durable Invocation contract';
    end if;

    expected_request := public.cex_invocation_ledger_effect_request_v1(
        new.invocation_id, 'reserve'
    );
    expected_operation_id := (expected_request ->> 'operation_id')::uuid;
    expected_command_id := public.cex_deterministic_uuid_v1(
        'cex:gateway-ledger-reserve:' || new.invocation_id::text
    );
    expected_fingerprint :=
        public.cex_gateway_ledger_reserve_request_fingerprint_v1(expected_request);

    if new.command_id is distinct from expected_command_id
       or new.operation_id is distinct from expected_operation_id
       or new.request_payload is distinct from expected_request
       or new.request_fingerprint is distinct from expected_fingerprint then
        raise exception 'Gateway reserve immutable request projection is invalid';
    end if;
    return new;
end
$$;

create or replace function public.cex_guard_gateway_ledger_reserve_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if new.command_id is distinct from old.command_id
       or new.invocation_id is distinct from old.invocation_id
       or new.org_id is distinct from old.org_id
       or new.operation_id is distinct from old.operation_id
       or new.contract_hash is distinct from old.contract_hash
       or new.request_payload is distinct from old.request_payload
       or new.request_fingerprint is distinct from old.request_fingerprint
       or new.source_service is distinct from old.source_service
       or new.source_principal is distinct from old.source_principal
       or new.schema_version is distinct from old.schema_version
       or new.created_at is distinct from old.created_at then
        raise exception 'Gateway Ledger reserve immutable fields cannot be changed';
    end if;
    if new.execution_mode is distinct from old.execution_mode
       and not (
           old.execution_mode = 'shadow'
           and new.execution_mode = 'active'
           and old.status = 'pending'
           and old.attempt_count = 0
       ) then
        raise exception 'Gateway reserve mode can only promote untouched shadow commands';
    end if;
    return new;
end
$$;

create or replace function public.cex_validate_gateway_ledger_reserve_transition_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if new.status is not distinct from old.status then
        return new;
    end if;
    if old.status = 'pending' and new.status in ('claimed', 'cancelled') then
        return new;
    elsif old.status = 'retry_wait' and new.status in ('claimed', 'dead_letter', 'cancelled') then
        return new;
    elsif old.status = 'claimed' and new.status in (
        'succeeded', 'retry_wait', 'reconcile_required', 'dead_letter', 'cancelled'
    ) then
        return new;
    elsif old.status in ('reconcile_required', 'dead_letter')
       and new.status in ('pending', 'cancelled') then
        return new;
    end if;
    raise exception 'invalid Gateway reserve transition % -> % for command %',
        old.status, new.status, old.command_id;
end
$$;

create or replace function public.cex_record_gateway_ledger_reserve_transition_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if tg_op = 'INSERT' then
        insert into public.cex_gateway_ledger_reserve_transitions_v1 (
            command_id, from_status, to_status, attempt_count, worker_id, error_code, receipt_hash
        ) values (
            new.command_id, null, new.status, new.attempt_count,
            new.claimed_by, new.last_error_code, new.ledger_receipt_hash
        );
    elsif new.status is distinct from old.status then
        insert into public.cex_gateway_ledger_reserve_transitions_v1 (
            command_id, from_status, to_status, attempt_count, worker_id, error_code, receipt_hash
        ) values (
            new.command_id, old.status, new.status, new.attempt_count,
            coalesce(old.claimed_by, new.claimed_by), new.last_error_code, new.ledger_receipt_hash
        );
    end if;
    return new;
end
$$;

create or replace function public.cex_reject_gateway_ledger_reserve_delete_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'Gateway Ledger reserve commands cannot be deleted';
end
$$;

create or replace function public.cex_reject_gateway_ledger_reserve_transition_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'Gateway Ledger reserve transition evidence is append-only';
end
$$;

drop trigger if exists trg_cex_validate_gateway_ledger_reserve_insert_v1
    on public.cex_gateway_ledger_reserve_commands_v1;
create trigger trg_cex_validate_gateway_ledger_reserve_insert_v1
before insert on public.cex_gateway_ledger_reserve_commands_v1
for each row execute function public.cex_validate_gateway_ledger_reserve_insert_v1();

drop trigger if exists trg_cex_guard_gateway_ledger_reserve_mutation_v1
    on public.cex_gateway_ledger_reserve_commands_v1;
create trigger trg_cex_guard_gateway_ledger_reserve_mutation_v1
before update on public.cex_gateway_ledger_reserve_commands_v1
for each row execute function public.cex_guard_gateway_ledger_reserve_mutation_v1();

drop trigger if exists trg_cex_validate_gateway_ledger_reserve_transition_v1
    on public.cex_gateway_ledger_reserve_commands_v1;
create trigger trg_cex_validate_gateway_ledger_reserve_transition_v1
before update of status on public.cex_gateway_ledger_reserve_commands_v1
for each row execute function public.cex_validate_gateway_ledger_reserve_transition_v1();

drop trigger if exists trg_cex_record_gateway_ledger_reserve_transition_v1
    on public.cex_gateway_ledger_reserve_commands_v1;
create trigger trg_cex_record_gateway_ledger_reserve_transition_v1
after insert or update of status on public.cex_gateway_ledger_reserve_commands_v1
for each row execute function public.cex_record_gateway_ledger_reserve_transition_v1();

drop trigger if exists trg_cex_reject_gateway_ledger_reserve_delete_v1
    on public.cex_gateway_ledger_reserve_commands_v1;
create trigger trg_cex_reject_gateway_ledger_reserve_delete_v1
before delete on public.cex_gateway_ledger_reserve_commands_v1
for each row execute function public.cex_reject_gateway_ledger_reserve_delete_v1();

drop trigger if exists trg_cex_reject_gateway_ledger_reserve_transition_mutation_v1
    on public.cex_gateway_ledger_reserve_transitions_v1;
create trigger trg_cex_reject_gateway_ledger_reserve_transition_mutation_v1
before update or delete on public.cex_gateway_ledger_reserve_transitions_v1
for each row execute function public.cex_reject_gateway_ledger_reserve_transition_mutation_v1();

create or replace function public.cex_prepare_gateway_exact_reserve_v1(
    p_invocation_id uuid,
    p_account_id uuid,
    p_org_id uuid,
    p_trace_id uuid,
    p_currency_unit text,
    p_currency_scale smallint,
    p_amount_minor bigint,
    p_source_principal text,
    p_execution_mode text default 'shadow',
    p_max_attempts integer default 5
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    invocation_org_id uuid;
    invocation_record jsonb;
    invocation_payload jsonb;
    contract_row public.cex_invocation_ledger_contracts_v1%rowtype;
    request_value jsonb;
    operation_id_value uuid;
    request_fingerprint_value text;
    command_id_value uuid;
    existing_command public.cex_gateway_ledger_reserve_commands_v1%rowtype;
    inserted_command public.cex_gateway_ledger_reserve_commands_v1%rowtype;
    audit_event_id uuid;
    audit_envelope jsonb;
begin
    if p_invocation_id is null
       or p_account_id is null
       or p_org_id is null
       or p_trace_id is null
       or p_invocation_id = '00000000-0000-0000-0000-000000000000'::uuid
       or p_account_id = '00000000-0000-0000-0000-000000000000'::uuid
       or p_org_id = '00000000-0000-0000-0000-000000000000'::uuid
       or p_trace_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'Gateway exact reserve identifiers must be non-nil';
    end if;
    if p_amount_minor <= 0 then
        raise exception 'Gateway exact reserve amount_minor must be positive';
    end if;
    if p_currency_scale not between 0 and 6 then
        raise exception 'Gateway exact reserve currency_scale must be between 0 and 6';
    end if;
    if lower(btrim(p_currency_unit)) !~ '^[a-z][a-z0-9._-]{0,31}$' then
        raise exception 'Gateway exact reserve currency_unit is invalid';
    end if;
    if p_source_principal is null
       or length(btrim(p_source_principal)) not between 1 and 256 then
        raise exception 'Gateway exact reserve source principal must contain 1..256 characters';
    end if;
    if p_execution_mode not in ('shadow', 'active') then
        raise exception 'Gateway exact reserve execution mode must be shadow or active';
    end if;
    if p_max_attempts not between 1 and 100 then
        raise exception 'Gateway exact reserve max_attempts must be between 1 and 100';
    end if;

    perform pg_advisory_xact_lock(
        hashtextextended('cex:gateway-ledger-reserve:' || p_invocation_id::text, 0)
    );

    select invocation.org_id, to_jsonb(invocation)
      into invocation_org_id, invocation_record
      from public.invocations invocation
     where invocation.invocation_id = p_invocation_id
     for update;
    if not found then
        raise exception using errcode = 'P0002', message = 'Invocation not found';
    end if;
    if invocation_org_id is distinct from p_org_id then
        raise exception 'Gateway exact reserve Invocation tenant mismatch';
    end if;

    if not public.cex_gateway_legacy_money_is_zero_or_absent_v1(
           invocation_record, 'requested_amount'
       )
       or not public.cex_gateway_legacy_money_is_zero_or_absent_v1(
           invocation_record, 'requested_reserve_amount'
       )
       or not public.cex_gateway_legacy_money_is_zero_or_absent_v1(
           invocation_record, 'reserve_amount'
       ) then
        raise exception 'Gateway exact reserve rejects an Invocation carrying nonzero legacy money';
    end if;

    invocation_payload := invocation_record -> 'request_payload';
    if jsonb_typeof(invocation_payload) = 'object'
       and invocation_payload ?| array[
           'requested_amount', 'requested_reserve_amount', 'reserve_amount',
           'amount', 'amount_major', 'price'
       ] then
        raise exception 'Gateway exact reserve rejects dual exact/legacy money input';
    end if;

    perform public.cex_register_invocation_ledger_contract_v1(
        p_invocation_id,
        p_account_id,
        p_org_id,
        p_trace_id,
        lower(btrim(p_currency_unit)),
        p_currency_scale,
        p_amount_minor,
        'gateway-service',
        btrim(p_source_principal)
    );

    select * into contract_row
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = p_invocation_id
     for share;

    request_value := public.cex_invocation_ledger_effect_request_v1(
        p_invocation_id, 'reserve'
    );
    operation_id_value := (request_value ->> 'operation_id')::uuid;
    request_fingerprint_value :=
        public.cex_gateway_ledger_reserve_request_fingerprint_v1(request_value);
    command_id_value := public.cex_deterministic_uuid_v1(
        'cex:gateway-ledger-reserve:' || p_invocation_id::text
    );

    select * into existing_command
      from public.cex_gateway_ledger_reserve_commands_v1
     where invocation_id = p_invocation_id
     for update;
    if found then
        if existing_command.command_id is distinct from command_id_value
           or existing_command.org_id is distinct from p_org_id
           or existing_command.operation_id is distinct from operation_id_value
           or existing_command.contract_hash is distinct from contract_row.contract_hash
           or existing_command.request_payload is distinct from request_value
           or existing_command.request_fingerprint is distinct from request_fingerprint_value
           or existing_command.source_principal is distinct from btrim(p_source_principal) then
            raise exception using
                errcode = '23505',
                message = 'Gateway exact reserve command collision with different immutable content';
        end if;
        return jsonb_build_object(
            'replayed', true,
            'contract', to_jsonb(contract_row),
            'command', to_jsonb(existing_command)
        );
    end if;

    insert into public.cex_gateway_ledger_reserve_commands_v1 (
        command_id, invocation_id, org_id, operation_id, contract_hash,
        request_payload, request_fingerprint, execution_mode, max_attempts,
        source_principal
    ) values (
        command_id_value, p_invocation_id, p_org_id, operation_id_value,
        contract_row.contract_hash, request_value, request_fingerprint_value,
        p_execution_mode, p_max_attempts, btrim(p_source_principal)
    ) returning * into inserted_command;

    audit_event_id := public.cex_deterministic_uuid_v1(
        'gateway-ledger-reserve-commanded:' || command_id_value::text
    );
    audit_envelope := jsonb_build_object(
        'event_id', audit_event_id,
        'trace_id', p_trace_id,
        'org_id', p_org_id,
        'actor_type', 'gateway-service',
        'actor_id', btrim(p_source_principal),
        'event_type', 'gateway.ledger_reserve.commanded',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', inserted_command.created_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'gateway-service',
            'command_id', command_id_value,
            'invocation_id', p_invocation_id,
            'operation_id', operation_id_value,
            'execution_mode', p_execution_mode,
            'request_fingerprint', request_fingerprint_value,
            'contract_hash', contract_row.contract_hash,
            'currency_unit', contract_row.currency_unit,
            'currency_scale', contract_row.currency_scale,
            'amount_minor', contract_row.amount_minor::text
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'gateway-service', audit_event_id, p_trace_id, p_org_id, audit_envelope, 10
    );

    return jsonb_build_object(
        'replayed', false,
        'contract', to_jsonb(contract_row),
        'command', to_jsonb(inserted_command)
    );
end
$$;

create or replace function public.cex_promote_gateway_exact_reserve_v1(
    p_command_id uuid,
    p_actor text
)
returns public.cex_gateway_ledger_reserve_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    command_row public.cex_gateway_ledger_reserve_commands_v1%rowtype;
    audit_event_id uuid;
    audit_envelope jsonb;
begin
    if p_actor is null or length(btrim(p_actor)) not between 1 and 256 then
        raise exception 'Gateway reserve promotion actor must contain 1..256 characters';
    end if;
    select * into command_row
      from public.cex_gateway_ledger_reserve_commands_v1
     where command_id = p_command_id
     for update;
    if not found then
        raise exception using errcode = 'P0002', message = 'Gateway reserve command not found';
    end if;
    if command_row.execution_mode = 'active' then
        return command_row;
    end if;
    if command_row.status <> 'pending' or command_row.attempt_count <> 0 then
        raise exception 'only untouched pending shadow reserve commands can be promoted';
    end if;

    update public.cex_gateway_ledger_reserve_commands_v1
       set execution_mode = 'active', updated_at = now()
     where command_id = p_command_id
    returning * into command_row;

    audit_event_id := public.cex_deterministic_uuid_v1(
        'gateway-ledger-reserve-promoted:' || command_row.command_id::text
    );
    audit_envelope := jsonb_build_object(
        'event_id', audit_event_id,
        'trace_id', command_row.request_payload ->> 'trace_id',
        'org_id', command_row.org_id,
        'actor_type', 'operator',
        'actor_id', btrim(p_actor),
        'event_type', 'gateway.ledger_reserve.promoted',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', command_row.updated_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'gateway-service',
            'command_id', command_row.command_id,
            'invocation_id', command_row.invocation_id,
            'operation_id', command_row.operation_id,
            'execution_mode', command_row.execution_mode
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'gateway-service', audit_event_id,
        (command_row.request_payload ->> 'trace_id')::uuid,
        command_row.org_id, audit_envelope, 10
    );
    return command_row;
end
$$;

create or replace function public.cex_enqueue_gateway_exact_reserve_status_audit_v1(
    p_command_id uuid,
    p_actor_id text,
    p_event_suffix text
)
returns void
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    command_row public.cex_gateway_ledger_reserve_commands_v1%rowtype;
    audit_event_id uuid;
    audit_envelope jsonb;
begin
    if p_actor_id is null or length(btrim(p_actor_id)) not between 1 and 256 then
        raise exception 'Gateway reserve status Audit actor must contain 1..256 characters';
    end if;
    if p_event_suffix is null or p_event_suffix !~ '^[a-z][a-z0-9_]{0,63}$' then
        raise exception 'Gateway reserve status Audit suffix is invalid';
    end if;
    select * into command_row
      from public.cex_gateway_ledger_reserve_commands_v1
     where command_id = p_command_id;
    if not found then
        raise exception using errcode = 'P0002', message = 'Gateway reserve command not found';
    end if;

    audit_event_id := public.cex_deterministic_uuid_v1(
        'gateway-ledger-reserve-status:' || command_row.command_id::text
        || ':' || command_row.attempt_count::text || ':' || p_event_suffix
    );
    audit_envelope := jsonb_build_object(
        'event_id', audit_event_id,
        'trace_id', command_row.request_payload ->> 'trace_id',
        'org_id', command_row.org_id,
        'actor_type', 'gateway-reserve-worker',
        'actor_id', btrim(p_actor_id),
        'event_type', 'gateway.ledger_reserve.' || p_event_suffix,
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', command_row.updated_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'gateway-service',
            'command_id', command_row.command_id,
            'invocation_id', command_row.invocation_id,
            'operation_id', command_row.operation_id,
            'attempt_count', command_row.attempt_count,
            'status', command_row.status,
            'http_status', command_row.last_http_status,
            'error_code', command_row.last_error_code,
            'receipt_hash', command_row.ledger_receipt_hash,
            'ledger_replayed', command_row.ledger_replayed
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'gateway-service', audit_event_id,
        (command_row.request_payload ->> 'trace_id')::uuid,
        command_row.org_id, audit_envelope, 10
    );
end
$$;

create or replace function public.cex_claim_gateway_exact_reserves_v1(
    p_worker_id text,
    p_limit integer default 10,
    p_lease_seconds integer default 60
)
returns setof public.cex_gateway_ledger_reserve_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    recovered public.cex_gateway_ledger_reserve_commands_v1%rowtype;
begin
    if p_worker_id is null or length(btrim(p_worker_id)) not between 1 and 128 then
        raise exception 'Gateway reserve worker_id must contain 1..128 characters';
    end if;
    if p_limit not between 1 and 100 then
        raise exception 'Gateway reserve claim limit must be between 1 and 100';
    end if;
    if p_lease_seconds not between 5 and 3600 then
        raise exception 'Gateway reserve lease must be between 5 and 3600 seconds';
    end if;

    for recovered in
        with expired as (
            select command_id
              from public.cex_gateway_ledger_reserve_commands_v1
             where execution_mode = 'active'
               and status = 'claimed'
               and lease_expires_at <= now()
               and attempt_count < max_attempts
             order by lease_expires_at, created_at, command_id
             for update skip locked
             limit p_limit
        )
        update public.cex_gateway_ledger_reserve_commands_v1 command
           set status = 'retry_wait',
               claimed_by = null,
               lease_expires_at = null,
               available_at = now(),
               last_error_code = 'claim_lease_expired_exact_replay',
               last_error_message = 'claim lease expired; exact reserve replay remains safe',
               dead_lettered_at = null,
               updated_at = now()
          from expired
         where command.command_id = expired.command_id
        returning command.*
    loop
        perform public.cex_enqueue_gateway_exact_reserve_status_audit_v1(
            recovered.command_id, 'lease-recovery', 'retry_wait'
        );
    end loop;

    for recovered in
        with expired as (
            select command_id
              from public.cex_gateway_ledger_reserve_commands_v1
             where execution_mode = 'active'
               and status = 'claimed'
               and lease_expires_at <= now()
               and attempt_count >= max_attempts
             order by lease_expires_at, created_at, command_id
             for update skip locked
             limit p_limit
        )
        update public.cex_gateway_ledger_reserve_commands_v1 command
           set status = 'reconcile_required',
               claimed_by = null,
               lease_expires_at = null,
               last_error_code = 'claim_lease_expired_after_final_attempt_unknown_outcome',
               last_error_message = 'final reserve lease expired; inspect contract before replay',
               dead_lettered_at = null,
               updated_at = now()
          from expired
         where command.command_id = expired.command_id
        returning command.*
    loop
        perform public.cex_enqueue_gateway_exact_reserve_status_audit_v1(
            recovered.command_id, 'lease-recovery', 'reconcile_required'
        );
    end loop;

    return query
    with candidates as (
        select command_id
          from public.cex_gateway_ledger_reserve_commands_v1
         where execution_mode = 'active'
           and status in ('pending', 'retry_wait')
           and attempt_count < max_attempts
           and available_at <= now()
         order by available_at, created_at, command_id
         for update skip locked
         limit p_limit
    )
    update public.cex_gateway_ledger_reserve_commands_v1 command
       set status = 'claimed',
           attempt_count = command.attempt_count + 1,
           claimed_by = btrim(p_worker_id),
           lease_expires_at = now() + make_interval(secs => p_lease_seconds),
           updated_at = now()
      from candidates
     where command.command_id = candidates.command_id
    returning command.*;
end
$$;

create or replace function public.cex_validate_gateway_exact_reserve_receipt_v1(
    p_command_id uuid,
    p_receipt jsonb
)
returns void
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    command_row public.cex_gateway_ledger_reserve_commands_v1%rowtype;
    contract_row public.cex_invocation_ledger_contracts_v1%rowtype;
    receipt_entry_id uuid;
begin
    if p_receipt is null or jsonb_typeof(p_receipt) <> 'object' then
        raise exception 'Gateway reserve receipt must be a JSON object';
    end if;
    select * into command_row
      from public.cex_gateway_ledger_reserve_commands_v1
     where command_id = p_command_id;
    if not found then
        raise exception using errcode = 'P0002', message = 'Gateway reserve command not found';
    end if;
    if public.cex_gateway_ledger_reserve_request_fingerprint_v1(
           command_row.request_payload
       ) is distinct from command_row.request_fingerprint then
        raise exception 'Gateway reserve request fingerprint mismatch';
    end if;

    if p_receipt #>> '{effect,account_id}'
           is distinct from command_row.request_payload ->> 'account_id'
       or p_receipt #>> '{effect,trace_id}'
           is distinct from command_row.request_payload ->> 'trace_id'
       or p_receipt #>> '{effect,operation_id}' is distinct from command_row.operation_id::text
       or p_receipt #>> '{effect,operation_kind}' is distinct from 'reserve'
       or p_receipt #>> '{effect,idempotency_scope}'
           is distinct from command_row.request_payload ->> 'idempotency_scope'
       or p_receipt #>> '{effect,idempotency_key}'
           is distinct from command_row.request_payload ->> 'idempotency_key'
       or p_receipt #>> '{effect,amount_minor}'
           is distinct from command_row.request_payload ->> 'amount_minor'
       or p_receipt #>> '{effect,currency_scale}'
           is distinct from command_row.request_payload ->> 'currency_scale'
       or p_receipt #>> '{account,currency_unit}'
           is distinct from command_row.request_payload ->> 'currency_unit'
       or p_receipt #>> '{account,currency_scale}'
           is distinct from command_row.request_payload ->> 'currency_scale' then
        raise exception 'Ledger receipt differs from immutable Gateway reserve request';
    end if;

    begin
        receipt_entry_id := (p_receipt #>> '{effect,entry_id}')::uuid;
    exception when others then
        raise exception 'Gateway reserve receipt entry_id is missing or invalid';
    end;
    if receipt_entry_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'Gateway reserve receipt entry_id must be non-nil';
    end if;

    select * into contract_row
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = command_row.invocation_id
     for share;
    if not found then
        raise exception 'durable Invocation Ledger contract disappeared';
    end if;
    if contract_row.contract_hash is distinct from command_row.contract_hash
       or contract_row.status is distinct from 'reserved'
       or contract_row.last_operation_id is distinct from command_row.operation_id
       or contract_row.last_entry_id is distinct from receipt_entry_id then
        raise exception 'Gateway reserve receipt lacks expected durable contract evidence';
    end if;
end
$$;

create or replace function public.cex_finish_gateway_exact_reserve_v1(
    p_command_id uuid,
    p_worker_id text,
    p_outcome text,
    p_error_code text default null,
    p_error_message text default null,
    p_http_status integer default null,
    p_receipt jsonb default null,
    p_replayed boolean default null,
    p_retry_after_seconds integer default null
)
returns public.cex_gateway_ledger_reserve_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    command_row public.cex_gateway_ledger_reserve_commands_v1%rowtype;
    receipt_hash_value text;
    retry_delay_seconds integer;
    next_status text;
begin
    if p_worker_id is null or length(btrim(p_worker_id)) not between 1 and 128 then
        raise exception 'Gateway reserve worker_id must contain 1..128 characters';
    end if;
    if p_outcome not in ('succeeded', 'retry_wait', 'reconcile_required', 'dead_letter') then
        raise exception 'unsupported Gateway reserve outcome';
    end if;
    if p_http_status is not null and p_http_status not between 100 and 599 then
        raise exception 'Gateway reserve HTTP status is invalid';
    end if;
    if p_retry_after_seconds is not null and p_retry_after_seconds not between 1 and 3600 then
        raise exception 'Gateway reserve retry delay must be between 1 and 3600 seconds';
    end if;

    select * into command_row
      from public.cex_gateway_ledger_reserve_commands_v1
     where command_id = p_command_id
     for update;
    if not found then
        raise exception using errcode = 'P0002', message = 'Gateway reserve command not found';
    end if;

    if command_row.status = 'succeeded' and p_outcome = 'succeeded' then
        receipt_hash_value := public.cex_gateway_ledger_reserve_receipt_hash_v1(p_receipt);
        if command_row.ledger_receipt_hash is distinct from receipt_hash_value
           or command_row.ledger_receipt is distinct from p_receipt
           or command_row.ledger_replayed is distinct from p_replayed then
            raise exception using
                errcode = '23505',
                message = 'Gateway reserve completion collision with different receipt';
        end if;
        return command_row;
    end if;

    if command_row.status <> 'claimed'
       or command_row.claimed_by is distinct from btrim(p_worker_id) then
        raise exception 'Gateway reserve completion requires ownership of an active claim';
    end if;
    if command_row.lease_expires_at is null or command_row.lease_expires_at <= now() then
        raise exception 'Gateway reserve completion claim lease has expired';
    end if;

    if p_outcome = 'succeeded' then
        if p_replayed is null then
            raise exception 'verified Gateway reserve success must include replayed flag';
        end if;
        perform public.cex_validate_gateway_exact_reserve_receipt_v1(p_command_id, p_receipt);
        receipt_hash_value := public.cex_gateway_ledger_reserve_receipt_hash_v1(p_receipt);
        next_status := 'succeeded';
        update public.cex_gateway_ledger_reserve_commands_v1
           set status = next_status,
               claimed_by = null,
               lease_expires_at = null,
               last_http_status = p_http_status,
               last_error_code = null,
               last_error_message = null,
               ledger_receipt = p_receipt,
               ledger_receipt_hash = receipt_hash_value,
               ledger_replayed = p_replayed,
               dead_lettered_at = null,
               completed_at = now(),
               updated_at = now()
         where command_id = p_command_id
        returning * into command_row;
    elsif p_outcome = 'retry_wait' and command_row.attempt_count < command_row.max_attempts then
        retry_delay_seconds := greatest(
            coalesce(p_retry_after_seconds, 0),
            least(3600, power(2, least(greatest(command_row.attempt_count - 1, 0), 10))::integer)
        );
        next_status := 'retry_wait';
        update public.cex_gateway_ledger_reserve_commands_v1
           set status = next_status,
               claimed_by = null,
               lease_expires_at = null,
               available_at = now() + make_interval(secs => retry_delay_seconds),
               last_http_status = p_http_status,
               last_error_code = left(coalesce(nullif(btrim(p_error_code), ''), 'retryable_failure'), 128),
               last_error_message = left(coalesce(p_error_message, ''), 2000),
               dead_lettered_at = null,
               updated_at = now()
         where command_id = p_command_id
        returning * into command_row;
    elsif p_outcome = 'reconcile_required' then
        next_status := 'reconcile_required';
        update public.cex_gateway_ledger_reserve_commands_v1
           set status = next_status,
               claimed_by = null,
               lease_expires_at = null,
               last_http_status = p_http_status,
               last_error_code = left(coalesce(nullif(btrim(p_error_code), ''), 'unknown_remote_outcome'), 128),
               last_error_message = left(coalesce(p_error_message, ''), 2000),
               updated_at = now()
         where command_id = p_command_id
        returning * into command_row;
    elsif p_outcome = 'retry_wait' then
        next_status := 'reconcile_required';
        update public.cex_gateway_ledger_reserve_commands_v1
           set status = next_status,
               claimed_by = null,
               lease_expires_at = null,
               last_http_status = p_http_status,
               last_error_code = 'retry_budget_exhausted_unknown_outcome',
               last_error_message = left(coalesce(
                   p_error_message,
                   'automatic reserve attempts exhausted; inspect durable contract before replay'
               ), 2000),
               updated_at = now()
         where command_id = p_command_id
        returning * into command_row;
    else
        next_status := 'dead_letter';
        update public.cex_gateway_ledger_reserve_commands_v1
           set status = next_status,
               claimed_by = null,
               lease_expires_at = null,
               last_http_status = p_http_status,
               last_error_code = left(coalesce(nullif(btrim(p_error_code), ''), 'permanent_rejection'), 128),
               last_error_message = left(coalesce(p_error_message, ''), 2000),
               dead_lettered_at = now(),
               updated_at = now()
         where command_id = p_command_id
        returning * into command_row;
    end if;

    perform public.cex_enqueue_gateway_exact_reserve_status_audit_v1(
        command_row.command_id, btrim(p_worker_id), next_status
    );
    return command_row;
end
$$;

create or replace function public.cex_acknowledge_gateway_exact_reserve_v1(
    p_command_id uuid,
    p_actor text,
    p_reason text
)
returns public.cex_gateway_ledger_reserve_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    command_row public.cex_gateway_ledger_reserve_commands_v1%rowtype;
    audit_event_id uuid;
    audit_envelope jsonb;
begin
    if p_actor is null or length(btrim(p_actor)) not between 1 and 256 then
        raise exception 'Gateway reserve acknowledgement actor must contain 1..256 characters';
    end if;
    if p_reason is null or length(btrim(p_reason)) not between 1 and 1000 then
        raise exception 'Gateway reserve acknowledgement reason must contain 1..1000 characters';
    end if;
    select * into command_row
      from public.cex_gateway_ledger_reserve_commands_v1
     where command_id = p_command_id
     for update;
    if not found then
        raise exception using errcode = 'P0002', message = 'Gateway reserve command not found';
    end if;
    if command_row.status not in ('reconcile_required', 'dead_letter') then
        raise exception 'only reconciliation/dead-letter reserve commands can be acknowledged';
    end if;
    if command_row.operator_acknowledged_at is not null then
        if command_row.operator_acknowledged_by is distinct from btrim(p_actor)
           or command_row.operator_acknowledged_reason is distinct from btrim(p_reason) then
            raise exception using errcode = '23505', message = 'Gateway reserve acknowledgement collision';
        end if;
        return command_row;
    end if;

    update public.cex_gateway_ledger_reserve_commands_v1
       set operator_acknowledged_by = btrim(p_actor),
           operator_acknowledged_reason = btrim(p_reason),
           operator_acknowledged_at = now(),
           updated_at = now()
     where command_id = p_command_id
    returning * into command_row;

    audit_event_id := public.cex_deterministic_uuid_v1(
        'gateway-ledger-reserve-acknowledged:' || command_row.command_id::text
        || ':' || command_row.requeue_count::text
    );
    audit_envelope := jsonb_build_object(
        'event_id', audit_event_id,
        'trace_id', command_row.request_payload ->> 'trace_id',
        'org_id', command_row.org_id,
        'actor_type', 'operator',
        'actor_id', btrim(p_actor),
        'event_type', 'gateway.ledger_reserve.acknowledged',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', command_row.operator_acknowledged_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'gateway-service',
            'command_id', command_row.command_id,
            'invocation_id', command_row.invocation_id,
            'operation_id', command_row.operation_id,
            'status', command_row.status,
            'reason', command_row.operator_acknowledged_reason
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'gateway-service', audit_event_id,
        (command_row.request_payload ->> 'trace_id')::uuid,
        command_row.org_id, audit_envelope, 10
    );
    return command_row;
end
$$;

create or replace function public.cex_requeue_gateway_exact_reserve_v1(
    p_command_id uuid,
    p_actor text,
    p_reason text,
    p_additional_attempts integer default 1
)
returns public.cex_gateway_ledger_reserve_commands_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    command_row public.cex_gateway_ledger_reserve_commands_v1%rowtype;
    acknowledged_by_value text;
    acknowledged_reason_value text;
    acknowledged_at_value timestamptz;
    audit_event_id uuid;
    audit_envelope jsonb;
begin
    if p_actor is null or length(btrim(p_actor)) not between 1 and 256 then
        raise exception 'Gateway reserve requeue actor must contain 1..256 characters';
    end if;
    if p_reason is null or length(btrim(p_reason)) not between 1 and 1000 then
        raise exception 'Gateway reserve requeue reason must contain 1..1000 characters';
    end if;
    if p_additional_attempts not between 1 and 20 then
        raise exception 'Gateway reserve additional attempts must be between 1 and 20';
    end if;

    select * into command_row
      from public.cex_gateway_ledger_reserve_commands_v1
     where command_id = p_command_id
     for update;
    if not found then
        raise exception using errcode = 'P0002', message = 'Gateway reserve command not found';
    end if;
    if command_row.status not in ('reconcile_required', 'dead_letter') then
        if command_row.status = 'pending'
           and command_row.last_requeued_by is not distinct from btrim(p_actor)
           and command_row.last_requeue_reason is not distinct from btrim(p_reason)
           and command_row.last_requeue_additional_attempts is not distinct from p_additional_attempts then
            return command_row;
        end if;
        raise exception using
            errcode = '23505',
            message = 'Gateway reserve requeue collision or command is not recoverable';
    end if;
    if command_row.operator_acknowledged_at is null then
        raise exception 'operator acknowledgement is required before Gateway reserve requeue';
    end if;
    if command_row.max_attempts + p_additional_attempts > 100 then
        raise exception 'Gateway reserve requeue would exceed maximum attempt budget';
    end if;

    acknowledged_by_value := command_row.operator_acknowledged_by;
    acknowledged_reason_value := command_row.operator_acknowledged_reason;
    acknowledged_at_value := command_row.operator_acknowledged_at;

    update public.cex_gateway_ledger_reserve_commands_v1
       set status = 'pending',
           max_attempts = max_attempts + p_additional_attempts,
           available_at = now(),
           claimed_by = null,
           lease_expires_at = null,
           dead_lettered_at = null,
           requeue_count = requeue_count + 1,
           last_requeued_by = btrim(p_actor),
           last_requeue_reason = btrim(p_reason),
           last_requeue_additional_attempts = p_additional_attempts,
           last_requeued_at = now(),
           operator_acknowledged_by = null,
           operator_acknowledged_reason = null,
           operator_acknowledged_at = null,
           updated_at = now()
     where command_id = p_command_id
    returning * into command_row;

    audit_event_id := public.cex_deterministic_uuid_v1(
        'gateway-ledger-reserve-requeued:' || command_row.command_id::text
        || ':' || command_row.requeue_count::text
    );
    audit_envelope := jsonb_build_object(
        'event_id', audit_event_id,
        'trace_id', command_row.request_payload ->> 'trace_id',
        'org_id', command_row.org_id,
        'actor_type', 'operator',
        'actor_id', btrim(p_actor),
        'event_type', 'gateway.ledger_reserve.requeued',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', command_row.updated_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'gateway-service',
            'command_id', command_row.command_id,
            'invocation_id', command_row.invocation_id,
            'operation_id', command_row.operation_id,
            'requeue_count', command_row.requeue_count,
            'max_attempts', command_row.max_attempts,
            'acknowledged_by', acknowledged_by_value,
            'acknowledged_reason', acknowledged_reason_value,
            'acknowledged_at', acknowledged_at_value,
            'requeued_by', command_row.last_requeued_by,
            'requeue_reason', command_row.last_requeue_reason,
            'additional_attempts', command_row.last_requeue_additional_attempts
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'gateway-service', audit_event_id,
        (command_row.request_payload ->> 'trace_id')::uuid,
        command_row.org_id, audit_envelope, 10
    );
    return command_row;
end
$$;

create index if not exists idx_cex_gateway_ledger_reserve_claim_v1
    on public.cex_gateway_ledger_reserve_commands_v1 (
        execution_mode, status, available_at, created_at, command_id
    ) where status in ('pending', 'claimed', 'retry_wait');

create index if not exists idx_cex_gateway_ledger_reserve_org_v1
    on public.cex_gateway_ledger_reserve_commands_v1 (
        org_id, status, updated_at, command_id
    );

create index if not exists idx_cex_gateway_ledger_reserve_operator_v1
    on public.cex_gateway_ledger_reserve_commands_v1 (
        status, operator_acknowledged_at, updated_at, command_id
    ) where status in ('reconcile_required', 'dead_letter');

create index if not exists idx_cex_gateway_ledger_reserve_transitions_command_v1
    on public.cex_gateway_ledger_reserve_transitions_v1 (command_id, transition_id);

create or replace view public.cex_gateway_ledger_reserve_status_v1 as
select
    execution_mode,
    status,
    count(*)::bigint as command_count,
    count(*) filter (where attempt_count >= max_attempts)::bigint as retry_budget_exhausted,
    count(*) filter (
        where status in ('reconcile_required', 'dead_letter')
          and operator_acknowledged_at is null
    )::bigint as unacknowledged_operator_items,
    min(available_at) filter (where status in ('pending', 'retry_wait')) as oldest_available_at,
    min(lease_expires_at) filter (where status = 'claimed') as oldest_lease_expiry,
    min(updated_at) filter (
        where status in ('reconcile_required', 'dead_letter')
          and operator_acknowledged_at is null
    ) as oldest_unacknowledged_at
from public.cex_gateway_ledger_reserve_commands_v1
group by execution_mode, status;

comment on table public.cex_gateway_ledger_reserve_commands_v1 is
    'P0-N6 immutable exact reserve commands. Network I/O must occur after claim commit.';
comment on function public.cex_prepare_gateway_exact_reserve_v1(
    uuid, uuid, uuid, uuid, text, smallint, bigint, text, text, integer
) is
    'Atomically registers the 0066 exact contract and inserts/replays one durable reserve command.';

commit;
