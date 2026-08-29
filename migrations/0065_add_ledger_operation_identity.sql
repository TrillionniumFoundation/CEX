begin;

-- P0-N1 expand phase: every ledger entry receives immutable operation provenance.
-- Existing rows are explicitly labelled entry-scoped legacy facts; no cross-service
-- trace is inferred from arbitrary historical reference text.

alter table public.ledger_entries
    add column if not exists trace_id uuid,
    add column if not exists operation_id uuid,
    add column if not exists operation_kind text,
    add column if not exists idempotency_scope text,
    add column if not exists source_service text,
    add column if not exists source_principal text,
    add column if not exists schema_version text,
    add column if not exists provenance_mode text,
    add column if not exists request_fingerprint text;

create or replace function public.cex_ledger_effect_fingerprint_v1(
    p_account_id uuid,
    p_operation_id uuid,
    p_operation_kind text,
    p_amount_minor bigint,
    p_currency_scale smallint,
    p_reference_type text,
    p_reference_id uuid,
    p_idempotency_scope text,
    p_idempotency_key text,
    p_source_service text,
    p_source_principal text,
    p_schema_version text,
    p_provenance_mode text
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
                'operation_id', p_operation_id,
                'operation_kind', p_operation_kind,
                'amount_minor', p_amount_minor,
                'currency_scale', p_currency_scale,
                'reference_type', coalesce(p_reference_type, ''),
                'reference_id', coalesce(p_reference_id::text, ''),
                'idempotency_scope', p_idempotency_scope,
                'idempotency_key', coalesce(p_idempotency_key, ''),
                'source_service', p_source_service,
                'source_principal', p_source_principal,
                'schema_version', p_schema_version,
                'provenance_mode', p_provenance_mode
            )::text,
            'sha256'
        ),
        'hex'
    )
$$;

update public.ledger_entries
   set trace_id = coalesce(
           trace_id,
           public.cex_deterministic_uuid_v1(
               'ledger-legacy-trace:' || entry_id::text
           )
       ),
       operation_id = coalesce(
           operation_id,
           public.cex_deterministic_uuid_v1(
               'ledger-legacy-operation:' || entry_id::text
           )
       ),
       operation_kind = coalesce(operation_kind, 'legacy_entry'),
       idempotency_scope = coalesce(idempotency_scope, 'legacy:global'),
       source_service = coalesce(source_service, 'ledger-legacy-backfill'),
       source_principal = coalesce(source_principal, 'legacy-entry'),
       schema_version = coalesce(schema_version, 'cex.ledger.effect.v1'),
       provenance_mode = coalesce(provenance_mode, 'legacy_entry_scoped')
 where trace_id is null
    or operation_id is null
    or operation_kind is null
    or idempotency_scope is null
    or source_service is null
    or source_principal is null
    or schema_version is null
    or provenance_mode is null;

update public.ledger_entries
   set request_fingerprint = public.cex_ledger_effect_fingerprint_v1(
       account_id,
       operation_id,
       operation_kind,
       amount_minor,
       currency_scale,
       reference_type,
       reference_id,
       idempotency_scope,
       idempotency_key,
       source_service,
       source_principal,
       schema_version,
       provenance_mode
   )
 where request_fingerprint is null;

create or replace function public.cex_prepare_ledger_entry_provenance_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    calculated_fingerprint text;
    compatibility_scope text;
begin
    if new.entry_id is null then
        new.entry_id := gen_random_uuid();
    end if;
    if new.currency_scale is null then
        new.currency_scale := 6;
    end if;
    if new.amount_minor is null then
        new.amount_minor := public.cex_numeric_to_minor(new.amount, new.currency_scale);
    end if;

    new.operation_kind := coalesce(
        nullif(btrim(new.operation_kind), ''),
        case lower(coalesce(new.reason, ''))
            when 'reserve' then 'reserve'
            when 'consume' then 'consume'
            when 'refund' then 'refund'
            when 'grant' then 'grant'
            when 'genesis' then 'genesis'
            else 'legacy_entry'
        end
    );

    compatibility_scope := 'legacy-v1:' || new.account_id::text || ':' || new.operation_kind;
    new.idempotency_scope := coalesce(
        nullif(btrim(new.idempotency_scope), ''),
        compatibility_scope
    );

    if new.operation_id is null then
        new.operation_id := case
            when nullif(btrim(new.idempotency_key), '') is not null then
                public.cex_deterministic_uuid_v1(
                    'ledger-operation:' || new.idempotency_scope || ':' || btrim(new.idempotency_key)
                )
            else public.cex_deterministic_uuid_v1(
                'ledger-entry-operation:' || new.entry_id::text
            )
        end;
    end if;

    new.trace_id := coalesce(new.trace_id, new.operation_id);
    new.source_service := coalesce(
        nullif(btrim(new.source_service), ''),
        'ledger-service'
    );
    new.source_principal := coalesce(
        nullif(btrim(new.source_principal), ''),
        'legacy-v1-api'
    );
    new.schema_version := coalesce(
        nullif(btrim(new.schema_version), ''),
        'cex.ledger.effect.v1'
    );
    new.provenance_mode := coalesce(
        nullif(btrim(new.provenance_mode), ''),
        'operation_scoped_compatibility'
    );

    calculated_fingerprint := public.cex_ledger_effect_fingerprint_v1(
        new.account_id,
        new.operation_id,
        new.operation_kind,
        new.amount_minor,
        new.currency_scale,
        new.reference_type,
        new.reference_id,
        new.idempotency_scope,
        new.idempotency_key,
        new.source_service,
        new.source_principal,
        new.schema_version,
        new.provenance_mode
    );

    if new.request_fingerprint is null then
        new.request_fingerprint := calculated_fingerprint;
    elsif new.request_fingerprint is distinct from calculated_fingerprint then
        raise exception 'ledger operation request fingerprint mismatch';
    end if;

    return new;
end
$$;

drop trigger if exists trg_cex_prepare_ledger_entry_provenance_v1
    on public.ledger_entries;
create trigger trg_cex_prepare_ledger_entry_provenance_v1
before insert on public.ledger_entries
for each row execute function public.cex_prepare_ledger_entry_provenance_v1();

alter table public.ledger_entries
    alter column trace_id set not null,
    alter column operation_id set not null,
    alter column operation_kind set not null,
    alter column idempotency_scope set not null,
    alter column source_service set not null,
    alter column source_principal set not null,
    alter column schema_version set not null,
    alter column provenance_mode set not null,
    alter column request_fingerprint set not null;

do $$
begin
    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_trace_non_nil_v1'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_trace_non_nil_v1
            check (trace_id <> '00000000-0000-0000-0000-000000000000'::uuid);
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_operation_non_nil_v1'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_operation_non_nil_v1
            check (operation_id <> '00000000-0000-0000-0000-000000000000'::uuid);
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_operation_kind_v1'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_operation_kind_v1
            check (operation_kind in (
                'reserve', 'consume', 'refund', 'grant', 'genesis', 'legacy_entry'
            ));
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_idempotency_scope_v1'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_idempotency_scope_v1
            check (
                length(idempotency_scope) between 1 and 160
                and idempotency_scope ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$'
            );
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_source_service_v1'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_source_service_v1
            check (source_service ~ '^[a-z0-9][a-z0-9._-]{0,127}$');
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_source_principal_v1'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_source_principal_v1
            check (length(btrim(source_principal)) between 1 and 256);
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_schema_version_v1'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_schema_version_v1
            check (schema_version = 'cex.ledger.effect.v1');
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_provenance_mode_v1'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_provenance_mode_v1
            check (provenance_mode in (
                'explicit',
                'operation_scoped_compatibility',
                'legacy_entry_scoped'
            ));
    end if;

    if not exists (
        select 1 from pg_constraint
         where conrelid = 'public.ledger_entries'::regclass
           and conname = 'ledger_entries_request_fingerprint_v1'
    ) then
        alter table public.ledger_entries
            add constraint ledger_entries_request_fingerprint_v1
            check (request_fingerprint ~ '^sha256:[0-9a-f]{64}$');
    end if;
end
$$;

-- Replace the legacy global key namespace with the explicit scoped namespace.
drop index if exists public.idx_ledger_entries_idempotency_key;

create unique index if not exists idx_ledger_entries_operation_id_v1
    on public.ledger_entries (operation_id);
create unique index if not exists idx_ledger_entries_scoped_idempotency_v1
    on public.ledger_entries (idempotency_scope, idempotency_key)
    where idempotency_key is not null;
create index if not exists idx_ledger_entries_trace_v1
    on public.ledger_entries (trace_id, created_at, entry_id);
create index if not exists idx_ledger_entries_operation_kind_v1
    on public.ledger_entries (operation_kind, created_at, entry_id);

create or replace function public.cex_enqueue_ledger_entry_audit_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    account_org_id uuid;
    account_balance_minor bigint;
    account_reserved_minor bigint;
    account_currency_unit text;
    event_id_value uuid;
    envelope_value jsonb;
begin
    select org_id, balance_minor, reserved_minor, currency_unit
      into account_org_id, account_balance_minor, account_reserved_minor, account_currency_unit
      from public.accounts
     where account_id = new.account_id;

    if not found then
        raise exception 'ledger Audit enqueue cannot resolve account tenancy';
    end if;

    event_id_value := public.cex_deterministic_uuid_v1(
        'ledger-service:effect:' || new.operation_id::text
    );

    envelope_value := jsonb_build_object(
        'event_id', event_id_value,
        'trace_id', new.trace_id,
        'org_id', account_org_id,
        'actor_type', new.source_service,
        'actor_id', new.source_principal,
        'event_type', 'ledger.effect.persisted',
        'schema_version', 'cex.audit.event.v2',
        'occurred_at', new.created_at,
        'payload', jsonb_build_object(
            '_cex_audit_source_service', 'ledger-service',
            'entry_id', new.entry_id,
            'account_id', new.account_id,
            'operation_id', new.operation_id,
            'operation_kind', new.operation_kind,
            'idempotency_scope', new.idempotency_scope,
            'idempotency_key', new.idempotency_key,
            'direction', new.direction,
            'amount_minor', new.amount_minor,
            'currency_scale', new.currency_scale,
            'currency_unit', account_currency_unit,
            'reference_type', new.reference_type,
            'reference_id', new.reference_id,
            'source_service', new.source_service,
            'source_principal', new.source_principal,
            'ledger_schema_version', new.schema_version,
            'provenance_mode', new.provenance_mode,
            'request_fingerprint', new.request_fingerprint,
            'balance_minor_after', account_balance_minor,
            'reserved_minor_after', account_reserved_minor
        )
    );

    perform public.cex_enqueue_audit_outbox_v1(
        'ledger-service',
        event_id_value,
        new.trace_id,
        account_org_id,
        envelope_value,
        10
    );

    return new;
end
$$;

drop trigger if exists trg_cex_enqueue_ledger_entry_audit_v1
    on public.ledger_entries;
create trigger trg_cex_enqueue_ledger_entry_audit_v1
after insert on public.ledger_entries
for each row execute function public.cex_enqueue_ledger_entry_audit_v1();

create or replace function public.cex_reject_ledger_entry_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'ledger_entries is append-only';
end
$$;

drop trigger if exists trg_cex_ledger_entries_append_only_v1
    on public.ledger_entries;
create trigger trg_cex_ledger_entries_append_only_v1
before update or delete on public.ledger_entries
for each row execute function public.cex_reject_ledger_entry_mutation_v1();

create or replace function public.cex_ledger_effect_result_v1(
    p_entry_id uuid,
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
        'account', jsonb_build_object(
            'account_id', account.account_id,
            'org_id', account.org_id,
            'account_type', account.account_type,
            'currency_unit', account.currency_unit,
            'currency_scale', account.currency_scale,
            'balance_minor', account.balance_minor,
            'reserved_minor', account.reserved_minor
        ),
        'effect', jsonb_build_object(
            'entry_id', entry.entry_id,
            'account_id', entry.account_id,
            'trace_id', entry.trace_id,
            'operation_id', entry.operation_id,
            'operation_kind', entry.operation_kind,
            'idempotency_scope', entry.idempotency_scope,
            'idempotency_key', entry.idempotency_key,
            'direction', entry.direction,
            'amount_minor', entry.amount_minor,
            'currency_scale', entry.currency_scale,
            'reference_type', entry.reference_type,
            'reference_id', entry.reference_id,
            'source_service', entry.source_service,
            'source_principal', entry.source_principal,
            'schema_version', entry.schema_version,
            'provenance_mode', entry.provenance_mode,
            'request_fingerprint', entry.request_fingerprint,
            'created_at', entry.created_at
        )
    )
      from public.ledger_entries entry
      join public.accounts account on account.account_id = entry.account_id
     where entry.entry_id = p_entry_id
$$;

create or replace function public.cex_apply_ledger_effect_v1(
    p_account_id uuid,
    p_trace_id uuid,
    p_operation_id uuid,
    p_operation_kind text,
    p_amount_minor bigint,
    p_currency_scale smallint,
    p_reference_type text,
    p_reference_id uuid,
    p_idempotency_scope text,
    p_idempotency_key text,
    p_source_service text,
    p_source_principal text,
    p_provenance_mode text default 'explicit'
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    operation_entry_id uuid;
    idempotency_entry_id uuid;
    existing_entry public.ledger_entries%rowtype;
    new_entry_id uuid;
    calculated_fingerprint text;
    account_row public.accounts%rowtype;
    available_minor bigint;
    next_balance_minor bigint;
    next_reserved_minor bigint;
    direction_value text;
begin
    if p_account_id is null
       or p_trace_id is null
       or p_operation_id is null
       or p_account_id = '00000000-0000-0000-0000-000000000000'::uuid
       or p_trace_id = '00000000-0000-0000-0000-000000000000'::uuid
       or p_operation_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'ledger operation identifiers must be non-nil UUIDs';
    end if;
    if p_operation_kind not in ('reserve', 'consume', 'refund', 'grant') then
        raise exception 'unsupported ledger operation_kind';
    end if;
    if p_amount_minor <= 0 then
        raise exception 'ledger amount_minor must be positive';
    end if;
    if p_currency_scale not between 0 and 6 then
        raise exception 'ledger currency_scale must be between 0 and 6';
    end if;
    if p_idempotency_scope is null
       or length(btrim(p_idempotency_scope)) not between 1 and 160
       or btrim(p_idempotency_scope) !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$' then
        raise exception 'ledger idempotency_scope is invalid';
    end if;
    if p_idempotency_key is null
       or length(btrim(p_idempotency_key)) not between 1 and 256 then
        raise exception 'ledger idempotency_key must contain 1..256 characters';
    end if;
    if p_source_service is null
       or btrim(p_source_service) !~ '^[a-z0-9][a-z0-9._-]{0,127}$' then
        raise exception 'ledger source_service is invalid';
    end if;
    if p_source_principal is null
       or length(btrim(p_source_principal)) not between 1 and 256 then
        raise exception 'ledger source_principal must contain 1..256 characters';
    end if;
    if p_provenance_mode not in ('explicit', 'operation_scoped_compatibility') then
        raise exception 'ledger provenance_mode is invalid';
    end if;
    if (p_reference_type is null) is distinct from (p_reference_id is null) then
        raise exception 'ledger reference_type and reference_id must be supplied together';
    end if;
    if p_reference_type is not null
       and (
           length(btrim(p_reference_type)) not between 1 and 128
           or btrim(p_reference_type) !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
       ) then
        raise exception 'ledger reference_type is invalid';
    end if;

    perform pg_advisory_xact_lock(
        hashtextextended('cex:ledger-operation:' || p_operation_id::text, 0)
    );
    perform pg_advisory_xact_lock(
        hashtextextended(
            'cex:ledger-idempotency:' || btrim(p_idempotency_scope) || ':' || btrim(p_idempotency_key),
            0
        )
    );

    select entry_id
      into operation_entry_id
      from public.ledger_entries
     where operation_id = p_operation_id;

    select entry_id
      into idempotency_entry_id
      from public.ledger_entries
     where idempotency_scope = btrim(p_idempotency_scope)
       and idempotency_key = btrim(p_idempotency_key);

    if operation_entry_id is not null
       and idempotency_entry_id is not null
       and operation_entry_id is distinct from idempotency_entry_id then
        raise exception using
            errcode = '23505',
            message = 'ledger operation collision: operation_id and scoped key identify different effects';
    end if;

    calculated_fingerprint := public.cex_ledger_effect_fingerprint_v1(
        p_account_id,
        p_operation_id,
        p_operation_kind,
        p_amount_minor,
        p_currency_scale,
        p_reference_type,
        p_reference_id,
        btrim(p_idempotency_scope),
        btrim(p_idempotency_key),
        btrim(p_source_service),
        btrim(p_source_principal),
        'cex.ledger.effect.v1',
        p_provenance_mode
    );

    if coalesce(operation_entry_id, idempotency_entry_id) is not null then
        select *
          into existing_entry
          from public.ledger_entries
         where entry_id = coalesce(operation_entry_id, idempotency_entry_id)
         for share;

        if existing_entry.account_id is distinct from p_account_id
           or existing_entry.operation_id is distinct from p_operation_id
           or existing_entry.operation_kind is distinct from p_operation_kind
           or existing_entry.amount_minor is distinct from p_amount_minor
           or existing_entry.currency_scale is distinct from p_currency_scale
           or existing_entry.reference_type is distinct from p_reference_type
           or existing_entry.reference_id is distinct from p_reference_id
           or existing_entry.idempotency_scope is distinct from btrim(p_idempotency_scope)
           or existing_entry.idempotency_key is distinct from btrim(p_idempotency_key)
           or existing_entry.source_service is distinct from btrim(p_source_service)
           or existing_entry.source_principal is distinct from btrim(p_source_principal)
           or existing_entry.schema_version is distinct from 'cex.ledger.effect.v1'
           or existing_entry.provenance_mode is distinct from p_provenance_mode
           or existing_entry.request_fingerprint is distinct from calculated_fingerprint then
            raise exception using
                errcode = '23505',
                message = 'ledger operation collision with different immutable content';
        end if;

        return public.cex_ledger_effect_result_v1(existing_entry.entry_id, true);
    end if;

    select *
      into account_row
      from public.accounts
     where account_id = p_account_id
     for update;

    if not found then
        raise exception using
            errcode = 'P0002',
            message = 'ledger account not found';
    end if;
    if account_row.status <> 'active' then
        raise exception 'ledger account is not active';
    end if;
    if account_row.currency_scale is distinct from p_currency_scale then
        raise exception 'ledger currency_scale differs from account scale';
    end if;

    available_minor := account_row.balance_minor - account_row.reserved_minor;
    next_balance_minor := account_row.balance_minor;
    next_reserved_minor := account_row.reserved_minor;

    case p_operation_kind
        when 'reserve' then
            if available_minor < p_amount_minor then
                raise exception 'ledger insufficient available minor units';
            end if;
            next_reserved_minor := next_reserved_minor + p_amount_minor;
            direction_value := 'debit';
        when 'consume' then
            if next_reserved_minor < p_amount_minor then
                raise exception 'ledger insufficient reserved minor units';
            end if;
            next_reserved_minor := next_reserved_minor - p_amount_minor;
            next_balance_minor := next_balance_minor - p_amount_minor;
            direction_value := 'debit';
        when 'refund' then
            if next_reserved_minor < p_amount_minor then
                raise exception 'ledger insufficient reserved minor units';
            end if;
            next_reserved_minor := next_reserved_minor - p_amount_minor;
            direction_value := 'credit';
        when 'grant' then
            next_balance_minor := next_balance_minor + p_amount_minor;
            direction_value := 'credit';
    end case;

    update public.accounts
       set balance_minor = next_balance_minor,
           reserved_minor = next_reserved_minor
     where account_id = p_account_id;

    new_entry_id := public.cex_deterministic_uuid_v1(
        'ledger-entry:' || p_operation_id::text
    );

    insert into public.ledger_entries (
        entry_id,
        account_id,
        direction,
        amount,
        reason,
        reference_type,
        reference_id,
        idempotency_key,
        currency_scale,
        amount_minor,
        trace_id,
        operation_id,
        operation_kind,
        idempotency_scope,
        source_service,
        source_principal,
        schema_version,
        provenance_mode,
        request_fingerprint
    ) values (
        new_entry_id,
        p_account_id,
        direction_value,
        public.cex_minor_to_numeric(p_amount_minor, p_currency_scale),
        p_operation_kind,
        p_reference_type,
        p_reference_id,
        btrim(p_idempotency_key),
        p_currency_scale,
        p_amount_minor,
        p_trace_id,
        p_operation_id,
        p_operation_kind,
        btrim(p_idempotency_scope),
        btrim(p_source_service),
        btrim(p_source_principal),
        'cex.ledger.effect.v1',
        p_provenance_mode,
        calculated_fingerprint
    );

    return public.cex_ledger_effect_result_v1(new_entry_id, false);
end
$$;

create or replace view public.cex_ledger_operation_identity_status_v1 as
select
    count(*)::bigint as total_entries,
    count(*) filter (where provenance_mode = 'explicit')::bigint as explicit_entries,
    count(*) filter (
        where provenance_mode = 'operation_scoped_compatibility'
    )::bigint as compatibility_entries,
    count(*) filter (
        where provenance_mode = 'legacy_entry_scoped'
    )::bigint as legacy_entry_scoped_entries,
    count(*) filter (
        where trace_id is null
           or operation_id is null
           or operation_kind is null
           or idempotency_scope is null
           or source_service is null
           or source_principal is null
           or schema_version is null
           or provenance_mode is null
           or request_fingerprint is null
    )::bigint as missing_provenance_entries,
    count(distinct operation_id)::bigint as distinct_operation_ids,
    count(*) filter (where idempotency_key is not null)::bigint as idempotent_entries,
    count(distinct (idempotency_scope, idempotency_key)) filter (
        where idempotency_key is not null
    )::bigint as distinct_scoped_idempotency_keys
from public.ledger_entries;

commit;
