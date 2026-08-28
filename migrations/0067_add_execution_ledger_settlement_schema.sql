begin;

-- P0-N5 settlement command tables and immutable fingerprints.

-- P0-N5 expand phase. Persist an exact terminal consume/refund command, commit its
-- claim before HTTP, then store a verified receipt or explicit unknown outcome.

create table if not exists public.cex_execution_ledger_settlement_commands_v1 (
    command_id uuid primary key,
    invocation_id uuid not null references public.invocations(invocation_id),
    execution_id uuid not null references public.executions(execution_id),
    org_id uuid not null references public.organizations(org_id),
    action text not null,
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
    source_service text not null default 'execution-service',
    source_principal text not null,
    schema_version text not null default 'cex.execution.ledger-settlement-command.v1',
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint cex_execution_ledger_settlement_invocation_unique_v1 unique (invocation_id),
    constraint cex_execution_ledger_settlement_non_nil_ids_v1 check (
        command_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and invocation_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and execution_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and org_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and operation_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ),
    constraint cex_execution_ledger_settlement_action_v1
        check (action in ('consume', 'refund')),
    constraint cex_execution_ledger_settlement_mode_v1
        check (execution_mode in ('shadow', 'active')),
    constraint cex_execution_ledger_settlement_status_v1 check (status in (
        'pending', 'claimed', 'retry_wait', 'succeeded',
        'reconcile_required', 'dead_letter', 'cancelled'
    )),
    constraint cex_execution_ledger_settlement_attempts_v1 check (
        max_attempts between 1 and 100
        and attempt_count between 0 and max_attempts
        and requeue_count between 0 and 100
    ),
    constraint cex_execution_ledger_settlement_request_object_v1
        check (jsonb_typeof(request_payload) = 'object'),
    constraint cex_execution_ledger_settlement_receipt_object_v1
        check (ledger_receipt is null or jsonb_typeof(ledger_receipt) = 'object'),
    constraint cex_execution_ledger_settlement_hashes_v1 check (
        contract_hash ~ '^sha256:[0-9a-f]{64}$'
        and request_fingerprint ~ '^sha256:[0-9a-f]{64}$'
        and (ledger_receipt_hash is null or ledger_receipt_hash ~ '^sha256:[0-9a-f]{64}$')
    ),
    constraint cex_execution_ledger_settlement_source_v1 check (
        source_service = 'execution-service'
        and length(btrim(source_principal)) between 1 and 256
        and schema_version = 'cex.execution.ledger-settlement-command.v1'
    ),
    constraint cex_execution_ledger_settlement_claim_shape_v1 check (
        (status = 'claimed' and claimed_by is not null and lease_expires_at is not null)
        or (status <> 'claimed' and claimed_by is null and lease_expires_at is null)
    ),
    constraint cex_execution_ledger_settlement_success_shape_v1 check (
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
    constraint cex_execution_ledger_settlement_dead_letter_shape_v1 check (
        (status = 'dead_letter' and dead_lettered_at is not null)
        or (status <> 'dead_letter' and dead_lettered_at is null)
    ),
    constraint cex_execution_ledger_settlement_ack_shape_v1 check (
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
    constraint cex_execution_ledger_settlement_requeue_shape_v1 check (
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
    constraint cex_execution_ledger_settlement_text_bounds_v1 check (
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
    constraint cex_execution_ledger_settlement_http_v1
        check (last_http_status is null or last_http_status between 100 and 599),
    constraint cex_execution_ledger_settlement_request_binding_v1 check (
        request_payload ->> 'reference_type' = 'invocation'
        and request_payload ->> 'reference_id' = invocation_id::text
        and request_payload ->> 'operation_id' = operation_id::text
        and request_payload ->> 'operation_kind' = action
    )
);

create table if not exists public.cex_execution_ledger_settlement_transitions_v1 (
    transition_id bigserial primary key,
    command_id uuid not null references public.cex_execution_ledger_settlement_commands_v1(command_id),
    from_status text,
    to_status text not null,
    attempt_count integer not null,
    worker_id text,
    error_code text,
    receipt_hash text,
    occurred_at timestamptz not null default now(),
    constraint cex_execution_ledger_settlement_transition_status_v1 check (
        (from_status is null or from_status in (
            'pending', 'claimed', 'retry_wait', 'succeeded',
            'reconcile_required', 'dead_letter', 'cancelled'
        ))
        and to_status in (
            'pending', 'claimed', 'retry_wait', 'succeeded',
            'reconcile_required', 'dead_letter', 'cancelled'
        )
    ),
    constraint cex_execution_ledger_settlement_transition_attempt_v1
        check (attempt_count between 0 and 100),
    constraint cex_execution_ledger_settlement_transition_text_v1 check (
        (worker_id is null or length(btrim(worker_id)) between 1 and 128)
        and (error_code is null or length(error_code) between 1 and 128)
    ),
    constraint cex_execution_ledger_settlement_transition_hash_v1
        check (receipt_hash is null or receipt_hash ~ '^sha256:[0-9a-f]{64}$')
);

create or replace function public.cex_execution_ledger_settlement_request_fingerprint_v1(
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

create or replace function public.cex_execution_ledger_settlement_receipt_hash_v1(
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

commit;
