begin;

-- P0-N2: exact account opening. Non-zero value appears only as the first append-only genesis entry.

create table if not exists public.cex_account_opening_contracts_v1 (
    account_id uuid primary key references public.accounts(account_id),
    org_id uuid not null references public.organizations(org_id),
    trace_id uuid not null,
    operation_id uuid not null unique,
    idempotency_scope text not null,
    idempotency_key text not null,
    account_type text not null,
    currency_unit text not null,
    currency_scale smallint not null,
    opening_minor bigint not null,
    opening_kind text not null,
    genesis_entry_id uuid,
    source_principal text not null,
    request_fingerprint text not null,
    created_at timestamptz not null default now(),
    constraint cex_account_opening_scoped_key_v1 unique (
        org_id, idempotency_scope, idempotency_key
    ),
    constraint cex_account_opening_non_nil_v1 check (
        account_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and org_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and trace_id <> '00000000-0000-0000-0000-000000000000'::uuid
        and operation_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ),
    constraint cex_account_opening_money_v1 check (
        currency_scale between 0 and 6
        and opening_minor >= 0
        and opening_kind in ('zero_open', 'genesis')
        and (
            (opening_minor = 0 and opening_kind = 'zero_open' and genesis_entry_id is null)
            or (opening_minor > 0 and opening_kind = 'genesis' and genesis_entry_id is not null)
        )
    ),
    constraint cex_account_opening_text_v1 check (
        account_type ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
        and currency_unit ~ '^[a-z0-9][a-z0-9._-]{0,31}$'
        and length(idempotency_scope) between 1 and 160
        and idempotency_scope ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$'
        and length(idempotency_key) between 1 and 256
        and length(btrim(source_principal)) between 1 and 256
    ),
    constraint cex_account_opening_hash_v1 check (
        request_fingerprint ~ '^sha256:[0-9a-f]{64}$'
    )
);

create or replace function public.cex_account_opening_fingerprint_v1(
    p_account_id uuid,
    p_org_id uuid,
    p_trace_id uuid,
    p_operation_id uuid,
    p_idempotency_scope text,
    p_idempotency_key text,
    p_account_type text,
    p_currency_unit text,
    p_currency_scale smallint,
    p_opening_minor bigint,
    p_source_principal text
)
returns text
language sql
immutable
set search_path = pg_catalog, public
as $$
    select 'sha256:' || encode(
        digest(
            jsonb_build_object(
                'account_id', p_account_id,
                'org_id', p_org_id,
                'trace_id', p_trace_id,
                'operation_id', p_operation_id,
                'idempotency_scope', p_idempotency_scope,
                'idempotency_key', p_idempotency_key,
                'account_type', p_account_type,
                'currency_unit', p_currency_unit,
                'currency_scale', p_currency_scale,
                'opening_minor', p_opening_minor,
                'source_principal', p_source_principal,
                'schema_version', 'cex.account.opening.v1'
            )::text,
            'sha256'
        ),
        'hex'
    )
$$;

create or replace function public.cex_account_opening_result_v1(
    p_account_id uuid,
    p_replayed boolean
)
returns jsonb
language sql
stable
strict
set search_path = pg_catalog, public
as $$
    select jsonb_build_object(
        'replayed', p_replayed,
        'schema_version', 'cex.account.opening.v1',
        'account', jsonb_build_object(
            'account_id', a.account_id,
            'org_id', a.org_id,
            'account_type', a.account_type,
            'currency_unit', a.currency_unit,
            'currency_scale', a.currency_scale,
            'balance_minor', a.balance_minor::text,
            'reserved_minor', a.reserved_minor::text,
            'status', a.status,
            'created_at', a.created_at
        ),
        'opening', jsonb_build_object(
            'trace_id', o.trace_id,
            'operation_id', o.operation_id,
            'idempotency_scope', o.idempotency_scope,
            'idempotency_key', o.idempotency_key,
            'opening_minor', o.opening_minor::text,
            'opening_kind', o.opening_kind,
            'genesis_entry_id', o.genesis_entry_id,
            'source_principal', o.source_principal,
            'request_fingerprint', o.request_fingerprint,
            'created_at', o.created_at
        )
    )
    from public.accounts a
    join public.cex_account_opening_contracts_v1 o using (account_id)
    where a.account_id = p_account_id
$$;

create or replace function public.cex_guard_direct_nonzero_account_opening_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if coalesce(current_setting('cex.account_opening_v1', true), '') <> 'enabled'
       and (
           coalesce(new.balance, 0::numeric) <> 0::numeric
           or coalesce(new.reserved, 0::numeric) <> 0::numeric
           or coalesce(new.balance_minor, 0) <> 0
           or coalesce(new.reserved_minor, 0) <> 0
       ) then
        raise exception 'non-zero account opening requires cex_open_account_v2 and a genesis entry';
    end if;
    return new;
end
$$;

create or replace function public.cex_guard_genesis_entry_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    existing_count bigint;
begin
    if new.operation_kind <> 'genesis' then
        return new;
    end if;
    if coalesce(current_setting('cex.account_opening_v1', true), '') <> 'enabled' then
        raise exception 'genesis entries can only be created by cex_open_account_v2';
    end if;
    select count(*) into existing_count
      from public.ledger_entries
     where account_id = new.account_id;
    if existing_count <> 0 then
        raise exception 'genesis must be the first Ledger entry for an account';
    end if;
    return new;
end
$$;

create or replace function public.cex_reject_account_opening_contract_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'account opening contracts are append-only';
end
$$;

drop trigger if exists trg_cex_guard_direct_nonzero_account_opening_v1 on public.accounts;
create trigger trg_cex_guard_direct_nonzero_account_opening_v1
before insert on public.accounts
for each row execute function public.cex_guard_direct_nonzero_account_opening_v1();

drop trigger if exists trg_cex_guard_genesis_entry_v1 on public.ledger_entries;
create trigger trg_cex_guard_genesis_entry_v1
before insert on public.ledger_entries
for each row execute function public.cex_guard_genesis_entry_v1();

drop trigger if exists trg_cex_reject_account_opening_contract_mutation_v1
    on public.cex_account_opening_contracts_v1;
create trigger trg_cex_reject_account_opening_contract_mutation_v1
before update or delete on public.cex_account_opening_contracts_v1
for each row execute function public.cex_reject_account_opening_contract_mutation_v1();

create or replace function public.cex_open_account_v2(
    p_account_id uuid,
    p_org_id uuid,
    p_trace_id uuid,
    p_account_type text,
    p_currency_unit text,
    p_currency_scale smallint,
    p_opening_minor bigint,
    p_idempotency_scope text,
    p_idempotency_key text,
    p_source_principal text
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    operation_id_value uuid;
    genesis_entry_id_value uuid;
    fingerprint_value text;
    existing_by_operation public.cex_account_opening_contracts_v1%rowtype;
    existing_by_key public.cex_account_opening_contracts_v1%rowtype;
    opening_kind_value text;
    event_id_value uuid;
    envelope_value jsonb;
begin
    if p_account_id is null or p_org_id is null or p_trace_id is null
       or p_account_id = '00000000-0000-0000-0000-000000000000'::uuid
       or p_org_id = '00000000-0000-0000-0000-000000000000'::uuid
       or p_trace_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'account opening identifiers must be non-nil UUIDs';
    end if;
    if p_account_type is null or btrim(p_account_type) !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$' then
        raise exception 'account_type is invalid';
    end if;
    if p_currency_unit is null or btrim(p_currency_unit) !~ '^[a-z0-9][a-z0-9._-]{0,31}$' then
        raise exception 'currency_unit is invalid';
    end if;
    if p_currency_scale not between 0 and 6 then
        raise exception 'currency_scale must be between 0 and 6';
    end if;
    if p_opening_minor < 0 then
        raise exception 'opening_minor cannot be negative';
    end if;
    if p_idempotency_scope is null
       or length(btrim(p_idempotency_scope)) not between 1 and 160
       or btrim(p_idempotency_scope) !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$' then
        raise exception 'opening idempotency_scope is invalid';
    end if;
    if p_idempotency_key is null or length(btrim(p_idempotency_key)) not between 1 and 256 then
        raise exception 'opening idempotency_key must contain 1..256 characters';
    end if;
    if p_source_principal is null or length(btrim(p_source_principal)) not between 1 and 256 then
        raise exception 'opening source principal must contain 1..256 characters';
    end if;
    if not exists (select 1 from public.organizations where org_id = p_org_id) then
        raise exception using errcode = 'P0002', message = 'organization not found';
    end if;

    operation_id_value := public.cex_deterministic_uuid_v1(
        'ledger-account-opening:' || p_org_id::text || ':' || p_account_id::text || ':'
        || btrim(p_idempotency_scope) || ':' || btrim(p_idempotency_key)
    );
    genesis_entry_id_value := case when p_opening_minor > 0 then
        public.cex_deterministic_uuid_v1('ledger-genesis-entry:' || operation_id_value::text)
        else null end;
    opening_kind_value := case when p_opening_minor > 0 then 'genesis' else 'zero_open' end;
    fingerprint_value := public.cex_account_opening_fingerprint_v1(
        p_account_id, p_org_id, p_trace_id, operation_id_value,
        btrim(p_idempotency_scope), btrim(p_idempotency_key), btrim(p_account_type),
        btrim(p_currency_unit), p_currency_scale, p_opening_minor, btrim(p_source_principal)
    );

    perform pg_advisory_xact_lock(hashtextextended('cex:account-opening:' || p_account_id::text, 0));
    perform pg_advisory_xact_lock(hashtextextended(
        'cex:account-opening-key:' || p_org_id::text || ':' || btrim(p_idempotency_scope)
        || ':' || btrim(p_idempotency_key), 0
    ));

    select * into existing_by_operation
      from public.cex_account_opening_contracts_v1
     where operation_id = operation_id_value;
    select * into existing_by_key
      from public.cex_account_opening_contracts_v1
     where org_id = p_org_id
       and idempotency_scope = btrim(p_idempotency_scope)
       and idempotency_key = btrim(p_idempotency_key);

    if existing_by_operation.account_id is not null
       and existing_by_key.account_id is not null
       and existing_by_operation.account_id is distinct from existing_by_key.account_id then
        raise exception using errcode = '23505', message = 'account opening identity collision';
    end if;

    if coalesce(existing_by_operation.account_id, existing_by_key.account_id) is not null then
        if existing_by_operation.account_id is null then
            existing_by_operation := existing_by_key;
        end if;
        if existing_by_operation.account_id is distinct from p_account_id
           or existing_by_operation.org_id is distinct from p_org_id
           or existing_by_operation.trace_id is distinct from p_trace_id
           or existing_by_operation.account_type is distinct from btrim(p_account_type)
           or existing_by_operation.currency_unit is distinct from btrim(p_currency_unit)
           or existing_by_operation.currency_scale is distinct from p_currency_scale
           or existing_by_operation.opening_minor is distinct from p_opening_minor
           or existing_by_operation.idempotency_scope is distinct from btrim(p_idempotency_scope)
           or existing_by_operation.idempotency_key is distinct from btrim(p_idempotency_key)
           or existing_by_operation.source_principal is distinct from btrim(p_source_principal)
           or existing_by_operation.request_fingerprint is distinct from fingerprint_value then
            raise exception using errcode = '23505', message = 'account opening collision with different immutable content';
        end if;
        return public.cex_account_opening_result_v1(existing_by_operation.account_id, true);
    end if;

    if exists (select 1 from public.accounts where account_id = p_account_id) then
        raise exception using errcode = '23505', message = 'account exists without matching opening contract';
    end if;

    perform set_config('cex.account_opening_v1', 'enabled', true);
    insert into public.accounts (
        account_id, org_id, account_type, currency_unit, status,
        balance, reserved, currency_scale, balance_minor, reserved_minor
    ) values (
        p_account_id, p_org_id, btrim(p_account_type), btrim(p_currency_unit), 'active',
        public.cex_minor_to_numeric(p_opening_minor, p_currency_scale), 0,
        p_currency_scale, p_opening_minor, 0
    );

    insert into public.cex_account_opening_contracts_v1 (
        account_id, org_id, trace_id, operation_id, idempotency_scope, idempotency_key,
        account_type, currency_unit, currency_scale, opening_minor, opening_kind,
        genesis_entry_id, source_principal, request_fingerprint
    ) values (
        p_account_id, p_org_id, p_trace_id, operation_id_value,
        btrim(p_idempotency_scope), btrim(p_idempotency_key), btrim(p_account_type),
        btrim(p_currency_unit), p_currency_scale, p_opening_minor, opening_kind_value,
        genesis_entry_id_value, btrim(p_source_principal), fingerprint_value
    );

    if p_opening_minor > 0 then
        insert into public.ledger_entries (
            entry_id, account_id, direction, amount, reason, reference_type, reference_id,
            idempotency_key, currency_scale, amount_minor, trace_id, operation_id,
            operation_kind, idempotency_scope, source_service, source_principal,
            schema_version, provenance_mode, request_fingerprint
        ) values (
            genesis_entry_id_value, p_account_id, 'credit',
            public.cex_minor_to_numeric(p_opening_minor, p_currency_scale), 'genesis',
            'account_opening', p_account_id, btrim(p_idempotency_key), p_currency_scale,
            p_opening_minor, p_trace_id, operation_id_value, 'genesis',
            btrim(p_idempotency_scope), 'ledger-service', btrim(p_source_principal),
            'cex.ledger.effect.v1', 'explicit',
            public.cex_ledger_effect_fingerprint_v1(
                p_account_id, operation_id_value, 'genesis', p_opening_minor,
                p_currency_scale, 'account_opening', p_account_id,
                btrim(p_idempotency_scope), btrim(p_idempotency_key),
                'ledger-service', btrim(p_source_principal),
                'cex.ledger.effect.v1', 'explicit'
            )
        );
    end if;

    event_id_value := public.cex_deterministic_uuid_v1(
        'ledger-account-opened:' || operation_id_value::text
    );
    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', p_trace_id,
        'org_id', p_org_id,
        'actor_type', 'ledger-service',
        'actor_id', btrim(p_source_principal),
        'event_type', 'ledger.account.opened',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', now(),
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'ledger-service',
            'account_id', p_account_id,
            'operation_id', operation_id_value,
            'opening_kind', opening_kind_value,
            'opening_minor', p_opening_minor::text,
            'currency_unit', btrim(p_currency_unit),
            'currency_scale', p_currency_scale,
            'genesis_entry_id', genesis_entry_id_value,
            'request_fingerprint', fingerprint_value
        )
    );
    perform public.cex_enqueue_audit_outbox_v1(
        'ledger-service', event_id_value, p_trace_id, p_org_id, envelope_value, 10
    );

    return public.cex_account_opening_result_v1(p_account_id, false);
end
$$;

create index if not exists idx_cex_account_opening_org_v1
    on public.cex_account_opening_contracts_v1 (org_id, created_at, account_id);

commit;
