begin;

-- P0-N3: immutable, tenant-scoped inventory. Historical value provenance is observed, never fabricated.

create table if not exists public.cex_account_opening_inventory_runs_v1 (
    run_id uuid primary key,
    org_id uuid not null references public.organizations(org_id),
    status text not null default 'draft',
    account_count bigint not null default 0,
    exact_account_count bigint not null default 0,
    unknown_provenance_count bigint not null default 0,
    corrupt_account_count bigint not null default 0,
    fabricated_history boolean not null default false,
    inventory_digest text,
    signer_key_id text,
    signer_public_key_sha256 text,
    signature_algorithm text,
    signature_base64 text,
    verification_evidence jsonb,
    built_by text not null,
    built_at timestamptz not null default now(),
    sealed_at timestamptz,
    constraint cex_account_inventory_status_v1 check (status in ('draft', 'sealed')),
    constraint cex_account_inventory_no_fabrication_v1 check (fabricated_history = false),
    constraint cex_account_inventory_digest_v1 check (
        inventory_digest is null or inventory_digest ~ '^sha256:[0-9a-f]{64}$'
    ),
    constraint cex_account_inventory_seal_shape_v1 check (
        (status = 'draft' and sealed_at is null and signature_base64 is null)
        or (
            status = 'sealed' and sealed_at is not null
            and inventory_digest is not null
            and signer_key_id is not null
            and signer_public_key_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and signature_algorithm = 'ed25519'
            and signature_base64 is not null
            and jsonb_typeof(verification_evidence) = 'object'
            and verification_evidence ->> 'verified' = 'true'
        )
    )
);

create table if not exists public.cex_account_opening_inventory_items_v1 (
    run_id uuid not null references public.cex_account_opening_inventory_runs_v1(run_id),
    account_id uuid not null references public.accounts(account_id),
    org_id uuid not null,
    summary_balance_minor bigint not null,
    summary_reserved_minor bigint not null,
    currency_unit text not null,
    currency_scale smallint not null,
    opening_operation_id uuid,
    opening_kind text,
    genesis_entry_id uuid,
    exact_entry_count bigint not null,
    compatibility_entry_count bigint not null,
    classification text not null,
    item_digest text not null,
    captured_at timestamptz not null default now(),
    primary key (run_id, account_id),
    constraint cex_account_inventory_item_org_v1 check (
        org_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ),
    constraint cex_account_inventory_item_counts_v1 check (
        exact_entry_count >= 0 and compatibility_entry_count >= 0
    ),
    constraint cex_account_inventory_item_class_v1 check (classification in (
        'exact_genesis_bound', 'exact_zero_opening', 'missing_genesis_entry',
        'opening_evidence_mismatch', 'legacy_unbound_nonzero',
        'legacy_unbound_with_entries', 'zero_unbound'
    )),
    constraint cex_account_inventory_item_digest_v1 check (
        item_digest ~ '^sha256:[0-9a-f]{64}$'
    )
);

create or replace function public.cex_account_inventory_item_digest_v1(
    p_account_id uuid,
    p_org_id uuid,
    p_balance_minor bigint,
    p_reserved_minor bigint,
    p_currency_unit text,
    p_currency_scale smallint,
    p_opening_operation_id uuid,
    p_opening_kind text,
    p_genesis_entry_id uuid,
    p_exact_entry_count bigint,
    p_compatibility_entry_count bigint,
    p_classification text
)
returns text
language sql
immutable
set search_path = pg_catalog, public
as $$
    select 'sha256:' || encode(digest(jsonb_build_object(
        'account_id', p_account_id,
        'org_id', p_org_id,
        'summary_balance_minor', p_balance_minor,
        'summary_reserved_minor', p_reserved_minor,
        'currency_unit', p_currency_unit,
        'currency_scale', p_currency_scale,
        'opening_operation_id', p_opening_operation_id,
        'opening_kind', p_opening_kind,
        'genesis_entry_id', p_genesis_entry_id,
        'exact_entry_count', p_exact_entry_count,
        'compatibility_entry_count', p_compatibility_entry_count,
        'classification', p_classification,
        'fabricated_history', false
    )::text, 'sha256'), 'hex')
$$;

create or replace function public.cex_build_account_opening_inventory_v1(
    p_org_id uuid,
    p_built_by text
)
returns public.cex_account_opening_inventory_runs_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    run_row public.cex_account_opening_inventory_runs_v1%rowtype;
    digest_value text;
begin
    if p_org_id is null or p_org_id = '00000000-0000-0000-0000-000000000000'::uuid then
        raise exception 'inventory org_id must be non-nil';
    end if;
    if p_built_by is null or length(btrim(p_built_by)) not between 1 and 256 then
        raise exception 'inventory builder must contain 1..256 characters';
    end if;
    if not exists (select 1 from public.organizations where org_id = p_org_id) then
        raise exception using errcode = 'P0002', message = 'organization not found';
    end if;

    insert into public.cex_account_opening_inventory_runs_v1 (
        run_id, org_id, built_by
    ) values (gen_random_uuid(), p_org_id, btrim(p_built_by))
    returning * into run_row;

    with account_facts as (
        select
            a.account_id,
            a.org_id,
            a.balance_minor,
            a.reserved_minor,
            a.currency_unit,
            a.currency_scale,
            o.operation_id as opening_operation_id,
            o.opening_kind,
            o.genesis_entry_id,
            count(e.entry_id) filter (where e.provenance_mode = 'explicit')::bigint as exact_count,
            count(e.entry_id) filter (where e.provenance_mode <> 'explicit')::bigint as compatibility_count,
            count(e.entry_id) filter (where e.operation_kind = 'genesis')::bigint as genesis_count,
            count(e.entry_id)::bigint as total_count
        from public.accounts a
        left join public.cex_account_opening_contracts_v1 o using (account_id)
        left join public.ledger_entries e using (account_id)
        where a.org_id = p_org_id
        group by a.account_id, a.org_id, a.balance_minor, a.reserved_minor,
                 a.currency_unit, a.currency_scale, o.operation_id, o.opening_kind,
                 o.genesis_entry_id
    ), classified as (
        select *, case
            when opening_kind = 'genesis' and genesis_count = 1 then 'exact_genesis_bound'
            when opening_kind = 'zero_open' and genesis_count = 0 then 'exact_zero_opening'
            when opening_kind = 'genesis' and genesis_count = 0 then 'missing_genesis_entry'
            when opening_kind is not null then 'opening_evidence_mismatch'
            when total_count > 0 and (balance_minor <> 0 or reserved_minor <> 0)
                then 'legacy_unbound_with_entries'
            when balance_minor <> 0 or reserved_minor <> 0 then 'legacy_unbound_nonzero'
            else 'zero_unbound'
        end as classification
        from account_facts
    )
    insert into public.cex_account_opening_inventory_items_v1 (
        run_id, account_id, org_id, summary_balance_minor, summary_reserved_minor,
        currency_unit, currency_scale, opening_operation_id, opening_kind,
        genesis_entry_id, exact_entry_count, compatibility_entry_count,
        classification, item_digest
    )
    select
        run_row.run_id, account_id, org_id, balance_minor, reserved_minor,
        currency_unit, currency_scale, opening_operation_id, opening_kind,
        genesis_entry_id, exact_count, compatibility_count, classification,
        public.cex_account_inventory_item_digest_v1(
            account_id, org_id, balance_minor, reserved_minor, currency_unit,
            currency_scale, opening_operation_id, opening_kind, genesis_entry_id,
            exact_count, compatibility_count, classification
        )
    from classified;

    select 'sha256:' || encode(digest(coalesce(
        string_agg(item_digest, '' order by account_id), ''
    ), 'sha256'), 'hex')
      into digest_value
      from public.cex_account_opening_inventory_items_v1
     where run_id = run_row.run_id;

    update public.cex_account_opening_inventory_runs_v1 run
       set account_count = stats.account_count,
           exact_account_count = stats.exact_account_count,
           unknown_provenance_count = stats.unknown_provenance_count,
           corrupt_account_count = stats.corrupt_account_count,
           inventory_digest = digest_value
      from (
          select
              count(*)::bigint as account_count,
              count(*) filter (where classification in ('exact_genesis_bound','exact_zero_opening'))::bigint
                  as exact_account_count,
              coalesce(sum(compatibility_entry_count), 0)::bigint
                  + count(*) filter (where classification like 'legacy_%')::bigint
                  as unknown_provenance_count,
              count(*) filter (where classification in ('missing_genesis_entry','opening_evidence_mismatch'))::bigint
                  as corrupt_account_count
          from public.cex_account_opening_inventory_items_v1
          where run_id = run_row.run_id
      ) stats
     where run.run_id = run_row.run_id
    returning run.* into run_row;

    return run_row;
end
$$;

create or replace function public.cex_guard_account_inventory_run_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if tg_op = 'DELETE' then
        raise exception 'account inventory runs cannot be deleted';
    end if;
    if coalesce(current_setting('cex.account_inventory_seal_v1', true), '') <> 'enabled' then
        raise exception 'account inventory runs can only be updated by the seal function';
    end if;
    if old.status <> 'draft' or new.status <> 'sealed'
       or new.run_id is distinct from old.run_id
       or new.org_id is distinct from old.org_id
       or new.inventory_digest is distinct from old.inventory_digest
       or new.account_count is distinct from old.account_count
       or new.exact_account_count is distinct from old.exact_account_count
       or new.unknown_provenance_count is distinct from old.unknown_provenance_count
       or new.corrupt_account_count is distinct from old.corrupt_account_count
       or new.fabricated_history is distinct from false
       or new.built_by is distinct from old.built_by
       or new.built_at is distinct from old.built_at then
        raise exception 'invalid account inventory seal mutation';
    end if;
    return new;
end
$$;

create or replace function public.cex_reject_account_inventory_item_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'account inventory items are append-only';
end
$$;

drop trigger if exists trg_cex_guard_account_inventory_run_v1
    on public.cex_account_opening_inventory_runs_v1;
create trigger trg_cex_guard_account_inventory_run_v1
before update or delete on public.cex_account_opening_inventory_runs_v1
for each row execute function public.cex_guard_account_inventory_run_v1();

drop trigger if exists trg_cex_reject_account_inventory_item_mutation_v1
    on public.cex_account_opening_inventory_items_v1;
create trigger trg_cex_reject_account_inventory_item_mutation_v1
before update or delete on public.cex_account_opening_inventory_items_v1
for each row execute function public.cex_reject_account_inventory_item_mutation_v1();

create or replace function public.cex_seal_account_opening_inventory_v1(
    p_run_id uuid,
    p_signer_key_id text,
    p_public_key_sha256 text,
    p_signature_base64 text,
    p_verification_evidence jsonb
)
returns public.cex_account_opening_inventory_runs_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    run_row public.cex_account_opening_inventory_runs_v1%rowtype;
begin
    if p_signer_key_id is null or length(btrim(p_signer_key_id)) not between 1 and 128 then
        raise exception 'inventory signer key ID is invalid';
    end if;
    if p_public_key_sha256 !~ '^sha256:[0-9a-f]{64}$' then
        raise exception 'inventory public key digest is invalid';
    end if;
    if p_signature_base64 is null or length(p_signature_base64) not between 80 and 256 then
        raise exception 'inventory signature is invalid';
    end if;
    if jsonb_typeof(p_verification_evidence) <> 'object'
       or p_verification_evidence ->> 'verified' <> 'true'
       or p_verification_evidence ->> 'algorithm' <> 'ed25519' then
        raise exception 'inventory requires verified Ed25519 evidence';
    end if;

    select * into run_row
      from public.cex_account_opening_inventory_runs_v1
     where run_id = p_run_id
     for update;
    if not found then
        raise exception using errcode = 'P0002', message = 'inventory run not found';
    end if;
    if run_row.status = 'sealed' then
        if run_row.signer_key_id is distinct from btrim(p_signer_key_id)
           or run_row.signer_public_key_sha256 is distinct from p_public_key_sha256
           or run_row.signature_base64 is distinct from p_signature_base64
           or run_row.verification_evidence is distinct from p_verification_evidence then
            raise exception using errcode = '23505', message = 'inventory seal collision';
        end if;
        return run_row;
    end if;

    perform set_config('cex.account_inventory_seal_v1', 'enabled', true);
    update public.cex_account_opening_inventory_runs_v1
       set status = 'sealed',
           signer_key_id = btrim(p_signer_key_id),
           signer_public_key_sha256 = p_public_key_sha256,
           signature_algorithm = 'ed25519',
           signature_base64 = p_signature_base64,
           verification_evidence = p_verification_evidence,
           sealed_at = now()
     where run_id = p_run_id
    returning * into run_row;
    return run_row;
end
$$;

create index if not exists idx_cex_account_inventory_org_v1
    on public.cex_account_opening_inventory_runs_v1 (org_id, built_at desc, run_id);

commit;
