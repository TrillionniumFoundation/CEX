begin;

create table if not exists public.cex_invocation_ledger_contracts_v1 (
    invocation_id uuid primary key references public.invocations(invocation_id),
    account_id uuid not null references public.accounts(account_id),
    org_id uuid not null references public.organizations(org_id),
    trace_id uuid not null,
    currency_unit text not null,
    currency_scale smallint not null,
    amount_minor bigint not null,
    idempotency_scope text not null,
    reserve_operation_id uuid not null unique,
    consume_operation_id uuid not null unique,
    refund_operation_id uuid not null unique,
    status text not null default 'registered',
    last_operation_id uuid,
    last_entry_id uuid references public.ledger_entries(entry_id),
    source_service text not null,
    source_principal text not null,
    schema_version text not null default 'cex.invocation.ledger-contract.v1',
    contract_hash text not null,
    reserved_at timestamptz,
    settled_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint cex_invocation_ledger_contract_trace_v1
        check (trace_id <> '00000000-0000-0000-0000-000000000000'::uuid),
    constraint cex_invocation_ledger_contract_currency_v1
        check (currency_unit ~ '^[a-z][a-z0-9._-]{0,31}$'),
    constraint cex_invocation_ledger_contract_scale_v1
        check (currency_scale between 0 and 6),
    constraint cex_invocation_ledger_contract_amount_v1
        check (amount_minor > 0),
    constraint cex_invocation_ledger_contract_scope_v1
        check (
            length(idempotency_scope) between 1 and 160
            and idempotency_scope ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$'
        ),
    constraint cex_invocation_ledger_contract_operation_ids_v1
        check (
            reserve_operation_id <> consume_operation_id
            and reserve_operation_id <> refund_operation_id
            and consume_operation_id <> refund_operation_id
        ),
    constraint cex_invocation_ledger_contract_status_v1
        check (status in ('registered', 'reserved', 'consumed', 'refunded')),
    constraint cex_invocation_ledger_contract_source_service_v1
        check (source_service ~ '^[a-z0-9][a-z0-9._-]{0,127}$'),
    constraint cex_invocation_ledger_contract_source_principal_v1
        check (length(btrim(source_principal)) between 1 and 256),
    constraint cex_invocation_ledger_contract_schema_v1
        check (schema_version = 'cex.invocation.ledger-contract.v1'),
    constraint cex_invocation_ledger_contract_hash_v1
        check (contract_hash ~ '^sha256:[0-9a-f]{64}$'),
    constraint cex_invocation_ledger_contract_times_v1
        check (
            (status = 'registered' and reserved_at is null and settled_at is null)
            or
            (status = 'reserved' and reserved_at is not null and settled_at is null)
            or
            (status in ('consumed', 'refunded') and reserved_at is not null and settled_at is not null)
        ),
    constraint cex_invocation_ledger_contract_last_effect_v1
        check (
            (status = 'registered' and last_operation_id is null and last_entry_id is null)
            or
            (status <> 'registered' and last_operation_id is not null and last_entry_id is not null)
        )
);

create or replace function public.cex_invocation_ledger_contract_hash_v1(
    p_invocation_id uuid,
    p_account_id uuid,
    p_org_id uuid,
    p_trace_id uuid,
    p_currency_unit text,
    p_currency_scale smallint,
    p_amount_minor bigint,
    p_idempotency_scope text,
    p_reserve_operation_id uuid,
    p_consume_operation_id uuid,
    p_refund_operation_id uuid,
    p_source_service text,
    p_source_principal text
)
returns text
language sql
immutable
strict
set search_path = pg_catalog, public
as $$
    select 'sha256:' || encode(
        digest(
            jsonb_build_object(
                'invocation_id', p_invocation_id,
                'account_id', p_account_id,
                'org_id', p_org_id,
                'trace_id', p_trace_id,
                'currency_unit', p_currency_unit,
                'currency_scale', p_currency_scale,
                'amount_minor', p_amount_minor,
                'idempotency_scope', p_idempotency_scope,
                'reserve_operation_id', p_reserve_operation_id,
                'consume_operation_id', p_consume_operation_id,
                'refund_operation_id', p_refund_operation_id,
                'source_service', p_source_service,
                'source_principal', p_source_principal,
                'schema_version', 'cex.invocation.ledger-contract.v1'
            )::text,
            'sha256'
        ),
        'hex'
    )
$$;

create or replace function public.cex_register_invocation_ledger_contract_v1(
    p_invocation_id uuid,
    p_account_id uuid,
    p_org_id uuid,
    p_trace_id uuid,
    p_currency_unit text,
    p_currency_scale smallint,
    p_amount_minor bigint,
    p_source_service text,
    p_source_principal text
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    invocation_org_id uuid;
    account_org_id uuid;
    account_currency_unit text;
    account_currency_scale smallint;
    normalized_currency text;
    scope_value text;
    reserve_operation_id_value uuid;
    consume_operation_id_value uuid;
    refund_operation_id_value uuid;
    contract_hash_value text;
    existing_contract public.cex_invocation_ledger_contracts_v1%rowtype;
    inserted_contract public.cex_invocation_ledger_contracts_v1%rowtype;
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
       or p_trace_id = '00000000-0000-0000-8000-000000000000'::uuid then
        raise exception 'invocation Ledger contract identifiers must be non-nil UUIDs';
    end if;
    if p_trace_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'invocation Ledger contract trace_id must be non-nil';
    end if;
    if p_amount_minor <= 0 then
        raise exception 'invocation Ledger contract amount_minor must be positive';
    end if;
    if p_currency_scale not between 0 and 6 then
        raise exception 'invocation Ledger contract currency_scale must be between 0 and 6';
    end if;

    normalized_currency := lower(btrim(p_currency_unit));
    if normalized_currency !~ '^[a-z][a-z0-9._-]{0,31}$' then
        raise exception 'invocation Ledger contract currency_unit is invalid';
    end if;
    if p_source_service is null
       or btrim(p_source_service) !~ '^[a-z0-9][a-z0-9._-]{0,127}$' then
        raise exception 'invocation Ledger contract source_service is invalid';
    end if;
    if p_source_principal is null
       or length(btrim(p_source_principal)) not between 1 and 256 then
        raise exception 'invocation Ledger contract source_principal must contain 1..256 characters';
    end if;

    perform pg_advisory_xact_lock(
        hashtextextended('cex:invocation-ledger-contract:' || p_invocation_id::text, 0)
    );

    select org_id
      into invocation_org_id
      from public.invocations
     where invocation_id = p_invocation_id;
    if not found then
        raise exception using
            errcode = 'P0002',
            message = 'invocation not found for Ledger contract';
    end if;

    select org_id, currency_unit, currency_scale
      into account_org_id, account_currency_unit, account_currency_scale
      from public.accounts
     where account_id = p_account_id;
    if not found then
        raise exception using
            errcode = 'P0002',
            message = 'account not found for Invocation Ledger contract';
    end if;

    if invocation_org_id is distinct from p_org_id
       or account_org_id is distinct from p_org_id then
        raise exception 'Invocation Ledger contract tenant binding mismatch';
    end if;
    if account_currency_unit is distinct from normalized_currency
       or account_currency_scale is distinct from p_currency_scale then
        raise exception 'Invocation Ledger contract currency differs from account contract';
    end if;

    scope_value := 'org:' || p_org_id::text || ':invocation:' || p_invocation_id::text;
    reserve_operation_id_value := public.cex_deterministic_uuid_v1(
        'cex:ledger-operation:' || scope_value || ':reserve'
    );
    consume_operation_id_value := public.cex_deterministic_uuid_v1(
        'cex:ledger-operation:' || scope_value || ':consume'
    );
    refund_operation_id_value := public.cex_deterministic_uuid_v1(
        'cex:ledger-operation:' || scope_value || ':refund'
    );

    contract_hash_value := public.cex_invocation_ledger_contract_hash_v1(
        p_invocation_id,
        p_account_id,
        p_org_id,
        p_trace_id,
        normalized_currency,
        p_currency_scale,
        p_amount_minor,
        scope_value,
        reserve_operation_id_value,
        consume_operation_id_value,
        refund_operation_id_value,
        btrim(p_source_service),
        btrim(p_source_principal)
    );

    select *
      into existing_contract
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = p_invocation_id
     for update;

    if found then
        if existing_contract.account_id is distinct from p_account_id
           or existing_contract.org_id is distinct from p_org_id
           or existing_contract.trace_id is distinct from p_trace_id
           or existing_contract.currency_unit is distinct from normalized_currency
           or existing_contract.currency_scale is distinct from p_currency_scale
           or existing_contract.amount_minor is distinct from p_amount_minor
           or existing_contract.idempotency_scope is distinct from scope_value
           or existing_contract.reserve_operation_id is distinct from reserve_operation_id_value
           or existing_contract.consume_operation_id is distinct from consume_operation_id_value
           or existing_contract.refund_operation_id is distinct from refund_operation_id_value
           or existing_contract.source_service is distinct from btrim(p_source_service)
           or existing_contract.source_principal is distinct from btrim(p_source_principal)
           or existing_contract.contract_hash is distinct from contract_hash_value then
            raise exception using
                errcode = '23505',
                message = 'Invocation Ledger contract collision with different immutable content';
        end if;

        return jsonb_build_object(
            'replayed', true,
            'contract', to_jsonb(existing_contract)
        );
    end if;

    insert into public.cex_invocation_ledger_contracts_v1 (
        invocation_id,
        account_id,
        org_id,
        trace_id,
        currency_unit,
        currency_scale,
        amount_minor,
        idempotency_scope,
        reserve_operation_id,
        consume_operation_id,
        refund_operation_id,
        source_service,
        source_principal,
        contract_hash
    ) values (
        p_invocation_id,
        p_account_id,
        p_org_id,
        p_trace_id,
        normalized_currency,
        p_currency_scale,
        p_amount_minor,
        scope_value,
        reserve_operation_id_value,
        consume_operation_id_value,
        refund_operation_id_value,
        btrim(p_source_service),
        btrim(p_source_principal),
        contract_hash_value
    ) returning * into inserted_contract;

    audit_event_id := public.cex_deterministic_uuid_v1(
        'invocation-ledger-contract-registered:' || p_invocation_id::text
    );
    audit_envelope := jsonb_build_object(
        'event_id', audit_event_id,
        'trace_id', p_trace_id,
        'org_id', p_org_id,
        'actor_type', btrim(p_source_service),
        'actor_id', btrim(p_source_principal),
        'event_type', 'invocation.ledger_contract.registered',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', inserted_contract.created_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', btrim(p_source_service),
            'invocation_id', p_invocation_id,
            'account_id', p_account_id,
            'currency_unit', normalized_currency,
            'currency_scale', p_currency_scale,
            'amount_minor', p_amount_minor,
            'idempotency_scope', scope_value,
            'reserve_operation_id', reserve_operation_id_value,
            'consume_operation_id', consume_operation_id_value,
            'refund_operation_id', refund_operation_id_value,
            'contract_hash', contract_hash_value
        )
    );

    perform public.cex_enqueue_audit_outbox_v1(
        btrim(p_source_service),
        audit_event_id,
        p_trace_id,
        p_org_id,
        audit_envelope,
        10
    );

    return jsonb_build_object(
        'replayed', false,
        'contract', to_jsonb(inserted_contract)
    );
end
$$;

create or replace function public.cex_invocation_ledger_effect_request_v1(
    p_invocation_id uuid,
    p_operation_kind text
)
returns jsonb
language plpgsql
stable
set search_path = pg_catalog, public
as $$
declare
    contract_row public.cex_invocation_ledger_contracts_v1%rowtype;
    operation_id_value uuid;
begin
    if p_operation_kind not in ('reserve', 'consume', 'refund') then
        raise exception 'unsupported Invocation Ledger operation kind';
    end if;

    select *
      into contract_row
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = p_invocation_id;
    if not found then
        raise exception using
            errcode = 'P0002',
            message = 'Invocation Ledger contract not found';
    end if;

    if p_operation_kind = 'reserve'
       and contract_row.status not in ('registered', 'reserved') then
        raise exception 'Invocation Ledger reserve is invalid after terminal settlement';
    elsif p_operation_kind = 'consume'
       and contract_row.status not in ('reserved', 'consumed') then
        raise exception 'Invocation Ledger consume requires a reserved contract';
    elsif p_operation_kind = 'refund'
       and contract_row.status not in ('reserved', 'refunded') then
        raise exception 'Invocation Ledger refund requires a reserved contract';
    end if;

    operation_id_value := case p_operation_kind
        when 'reserve' then contract_row.reserve_operation_id
        when 'consume' then contract_row.consume_operation_id
        else contract_row.refund_operation_id
    end;

    return jsonb_build_object(
        'account_id', contract_row.account_id,
        'trace_id', contract_row.trace_id,
        'operation_id', operation_id_value,
        'operation_kind', p_operation_kind,
        'currency_unit', contract_row.currency_unit,
        'currency_scale', contract_row.currency_scale,
        'amount_minor', contract_row.amount_minor::text,
        'reference_type', 'invocation',
        'reference_id', contract_row.invocation_id,
        'idempotency_scope', contract_row.idempotency_scope,
        'idempotency_key', p_operation_kind
    );
end
$$;

create or replace function public.cex_bind_invocation_ledger_effect_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    contract_row public.cex_invocation_ledger_contracts_v1%rowtype;
    expected_operation_id uuid;
    next_status text;
    effect_time timestamptz := coalesce(new.created_at, clock_timestamp());
begin
    if new.reference_type is distinct from 'invocation'
       or new.reference_id is null
       or new.operation_kind not in ('reserve', 'consume', 'refund') then
        return new;
    end if;

    select *
      into contract_row
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = new.reference_id
     for update;
    if not found then
        return new;
    end if;

    expected_operation_id := case new.operation_kind
        when 'reserve' then contract_row.reserve_operation_id
        when 'consume' then contract_row.consume_operation_id
        else contract_row.refund_operation_id
    end;

    if new.account_id is distinct from contract_row.account_id
       or new.trace_id is distinct from contract_row.trace_id
       or new.operation_id is distinct from expected_operation_id
       or new.amount_minor is distinct from contract_row.amount_minor
       or new.currency_scale is distinct from contract_row.currency_scale
       or new.idempotency_scope is distinct from contract_row.idempotency_scope
       or new.idempotency_key is distinct from new.operation_kind then
        raise exception 'Ledger effect differs from the immutable Invocation contract';
    end if;

    next_status := case new.operation_kind
        when 'reserve' then 'reserved'
        when 'consume' then 'consumed'
        else 'refunded'
    end;

    if new.operation_kind = 'reserve' and contract_row.status <> 'registered' then
        raise exception 'Invocation Ledger contract cannot reserve from status %', contract_row.status;
    elsif new.operation_kind in ('consume', 'refund') and contract_row.status <> 'reserved' then
        raise exception 'Invocation Ledger contract cannot settle from status %', contract_row.status;
    end if;

    update public.cex_invocation_ledger_contracts_v1
       set status = next_status,
           last_operation_id = new.operation_id,
           last_entry_id = new.entry_id,
           reserved_at = case
               when new.operation_kind = 'reserve' then effect_time
               else reserved_at
           end,
           settled_at = case
               when new.operation_kind in ('consume', 'refund') then effect_time
               else null
           end,
           updated_at = effect_time
     where invocation_id = contract_row.invocation_id;

    return new;
end
$$;

drop trigger if exists trg_cex_bind_invocation_ledger_effect_v1
    on public.ledger_entries;
create trigger trg_cex_bind_invocation_ledger_effect_v1
after insert on public.ledger_entries
for each row execute function public.cex_bind_invocation_ledger_effect_v1();

create index if not exists idx_cex_invocation_ledger_contract_trace_v1
    on public.cex_invocation_ledger_contracts_v1 (trace_id, invocation_id);
create index if not exists idx_cex_invocation_ledger_contract_status_v1
    on public.cex_invocation_ledger_contracts_v1 (status, updated_at, invocation_id);
create index if not exists idx_cex_invocation_ledger_contract_account_v1
    on public.cex_invocation_ledger_contracts_v1 (account_id, created_at, invocation_id);

create or replace view public.cex_invocation_ledger_contract_status_v1 as
select
    count(*)::bigint as total_contracts,
    count(*) filter (where status = 'registered')::bigint as registered_contracts,
    count(*) filter (where status = 'reserved')::bigint as reserved_contracts,
    count(*) filter (where status = 'consumed')::bigint as consumed_contracts,
    count(*) filter (where status = 'refunded')::bigint as refunded_contracts,
    count(*) filter (
        where status <> 'registered'
          and not exists (
              select 1
                from public.ledger_entries entry
               where entry.entry_id = cex_invocation_ledger_contracts_v1.last_entry_id
                 and entry.operation_id = cex_invocation_ledger_contracts_v1.last_operation_id
          )
    )::bigint as missing_effect_evidence,
    min(created_at) as oldest_contract_at,
    min(updated_at) filter (where status = 'reserved') as oldest_reserved_at
from public.cex_invocation_ledger_contracts_v1;

commit;
