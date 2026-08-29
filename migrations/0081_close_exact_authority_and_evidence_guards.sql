begin;

-- Final P0 authority guards. Exact invocations never re-enter legacy value state,
-- provider work cannot begin before a verified reserve, and release evidence is
-- admitted with type-correct status semantics.

create or replace function public.cex_normalize_exact_invocation_legacy_flags_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if exists (
        select 1
          from public.cex_invocation_ledger_contracts_v1 contract
         where contract.invocation_id = new.invocation_id
    ) then
        new.ledger_reserved := false;
        new.ledger_refunded := false;
    end if;
    return new;
end
$$;

drop trigger if exists trg_cex_normalize_exact_invocation_legacy_flags_v1
    on public.invocations;
create trigger trg_cex_normalize_exact_invocation_legacy_flags_v1
before insert or update of ledger_reserved, ledger_refunded on public.invocations
for each row execute function public.cex_normalize_exact_invocation_legacy_flags_v1();

create or replace function public.cex_clear_legacy_flags_after_exact_contract_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    update public.invocations
       set ledger_reserved = false,
           ledger_refunded = false,
           updated_at = greatest(updated_at, now())
     where invocation_id = new.invocation_id
       and (ledger_reserved or ledger_refunded);
    return new;
end
$$;

drop trigger if exists trg_cex_clear_legacy_flags_after_exact_contract_v1
    on public.cex_invocation_ledger_contracts_v1;
create trigger trg_cex_clear_legacy_flags_after_exact_contract_v1
after insert on public.cex_invocation_ledger_contracts_v1
for each row execute function public.cex_clear_legacy_flags_after_exact_contract_v1();

-- Compatibility writes are fully retired. The historical break-glass evidence
-- table remains append-only evidence, but it no longer authorizes value mutation.
create or replace function public.cex_reject_new_compatibility_ledger_writes_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if new.provenance_mode <> 'explicit' then
        raise exception 'new compatibility/legacy value writes are disabled; use exact Ledger v2';
    end if;
    return new;
end
$$;

create or replace function public.cex_reject_compatibility_breakglass_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    raise exception 'compatibility break-glass evidence is append-only and does not authorize writes';
end
$$;

drop trigger if exists trg_cex_reject_compatibility_breakglass_mutation_v1
    on public.cex_compatibility_breakglass_evidence_v1;
create trigger trg_cex_reject_compatibility_breakglass_mutation_v1
before update or delete on public.cex_compatibility_breakglass_evidence_v1
for each row execute function public.cex_reject_compatibility_breakglass_mutation_v1();

create or replace function public.cex_provider_payload_has_nonzero_legacy_money_v1(
    p_payload jsonb
)
returns boolean
language sql
immutable
strict
set search_path = pg_catalog, public
as $$
    select jsonb_typeof(p_payload) = 'object'
       and (
           not public.cex_gateway_legacy_money_is_zero_or_absent_v1(p_payload, 'reserve_amount')
           or not public.cex_gateway_legacy_money_is_zero_or_absent_v1(p_payload, 'requested_amount')
           or not public.cex_gateway_legacy_money_is_zero_or_absent_v1(p_payload, 'requested_reserve_amount')
           or not public.cex_gateway_legacy_money_is_zero_or_absent_v1(p_payload, 'amount')
           or not public.cex_gateway_legacy_money_is_zero_or_absent_v1(p_payload, 'amount_major')
           or not public.cex_gateway_legacy_money_is_zero_or_absent_v1(p_payload, 'amount_minor')
           or not public.cex_gateway_legacy_money_is_zero_or_absent_v1(p_payload, 'price')
       )
$$;

create or replace function public.cex_validate_provider_dispatch_authority_v2()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    invocation_payload jsonb;
    contract_row public.cex_invocation_ledger_contracts_v1%rowtype;
    reserve_row public.cex_gateway_ledger_reserve_commands_v1%rowtype;
begin
    -- Until a provider adapter supports an authenticated stdin/file contract,
    -- production-capable dispatch is restricted to the HTTP Ollama adapter. This
    -- prevents prompts from appearing in process argv.
    if new.provider_target not like 'ollama://%' then
        raise exception 'provider target lacks an approved non-argv prompt transport';
    end if;

    select invocation.request_payload
      into invocation_payload
      from public.invocations invocation
     where invocation.invocation_id = new.invocation_id
     for share;
    if not found then
        raise exception 'provider dispatch Invocation does not exist';
    end if;

    select * into contract_row
      from public.cex_invocation_ledger_contracts_v1
     where invocation_id = new.invocation_id
     for share;

    if found then
        select * into reserve_row
          from public.cex_gateway_ledger_reserve_commands_v1
         where invocation_id = new.invocation_id
         for share;
        if not found
           or reserve_row.execution_mode <> 'active'
           or reserve_row.status <> 'succeeded'
           or reserve_row.org_id is distinct from new.org_id
           or reserve_row.contract_hash is distinct from contract_row.contract_hash
           or contract_row.status <> 'reserved'
           or contract_row.last_operation_id is distinct from reserve_row.operation_id
           or contract_row.last_entry_id is null then
            raise exception 'provider dispatch requires verified active exact reserve evidence';
        end if;
    elsif public.cex_provider_payload_has_nonzero_legacy_money_v1(invocation_payload) then
        raise exception 'value-bearing provider dispatch requires an exact Invocation Ledger contract';
    end if;

    return new;
end
$$;

drop trigger if exists trg_cex_validate_provider_dispatch_authority_v2
    on public.cex_provider_dispatch_commands_v1;
create trigger trg_cex_validate_provider_dispatch_authority_v2
before insert on public.cex_provider_dispatch_commands_v1
for each row execute function public.cex_validate_provider_dispatch_authority_v2();

-- Sealing requires a dedicated workload role used only by the service that
-- performs real Ed25519 verification. Projection operators cannot self-assert
-- verified=true directly at the database boundary.
do $$
begin
    if not exists (select 1 from pg_roles where rolname='cex_inventory_signature_verifier') then
        create role cex_inventory_signature_verifier nologin;
    end if;
end
$$;

revoke execute on function public.cex_seal_account_opening_inventory_v1(
    uuid,text,text,text,jsonb
) from public, cex_projection_operator;
grant execute on function public.cex_seal_account_opening_inventory_v1(
    uuid,text,text,text,jsonb
) to cex_inventory_signature_verifier;

create or replace function public.cex_validate_release_evidence_status_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if new.evidence_type in ('security_review', 'release_approval') then
        if new.status <> 'approved' then
            raise exception 'review and approval evidence must use approved status';
        end if;
    elsif new.status <> 'passed' then
        raise exception 'executable release evidence must use passed status';
    end if;

    if new.artifact_uri !~ '^(https://|gh://|oci://|s3://|gs://|file://)[^[:space:]]+$' then
        raise exception 'release evidence artifact_uri must use an explicit immutable-capable scheme';
    end if;
    if not (new.details ? 'source_commit_sha')
       or not (new.details ? 'source_tree_sha')
       or new.details ->> 'source_commit_sha' !~ '^[0-9a-f]{40}$'
       or new.details ->> 'source_tree_sha' !~ '^[0-9a-f]{40}$' then
        raise exception 'release evidence must bind source_commit_sha and source_tree_sha';
    end if;
    if not exists (
        select 1
          from public.cex_release_candidates_v1 candidate
         where candidate.release_id = new.release_id
           and candidate.source_commit_sha = new.details ->> 'source_commit_sha'
           and candidate.source_tree_sha = new.details ->> 'source_tree_sha'
    ) then
        raise exception 'release evidence source identity differs from the release candidate';
    end if;
    return new;
end
$$;

drop trigger if exists trg_cex_validate_release_evidence_status_v1
    on public.cex_release_evidence_v1;
create trigger trg_cex_validate_release_evidence_status_v1
before insert on public.cex_release_evidence_v1
for each row execute function public.cex_validate_release_evidence_status_v1();

create or replace view public.cex_exact_authority_status_v1 as
select
    count(*)::bigint as exact_invocation_count,
    count(*) filter (
        where invocation.ledger_reserved or invocation.ledger_refunded
    )::bigint as exact_invocations_with_legacy_flags,
    count(*) filter (
        where reserve.command_id is null
    )::bigint as exact_invocations_without_reserve_command,
    count(*) filter (
        where contract.status = 'reserved'
          and (reserve.status <> 'succeeded' or reserve.execution_mode <> 'active')
    )::bigint as reserved_contracts_without_verified_active_command
from public.cex_invocation_ledger_contracts_v1 contract
join public.invocations invocation using (invocation_id)
left join public.cex_gateway_ledger_reserve_commands_v1 reserve using (invocation_id);

create or replace view public.cex_p0_database_role_matrix_v1 as
select * from (values
    ('cex_ledger_opening_source','exact account opening and exact reads'),
    ('cex_projection_operator','inventory build, projection capture, policy and repair evidence'),
    ('cex_inventory_signature_verifier','server-verified Ed25519 inventory seal admission'),
    ('cex_provider_dispatch_source','provider command enqueue and explicit terminal decision'),
    ('cex_provider_dispatch_worker','provider claim and durable outcome'),
    ('cex_release_evidence_admitter','external release evidence admission')
) role_matrix(role_name, responsibility);

comment on view public.cex_exact_authority_status_v1 is
    'Fail-closed status for legacy flag leakage and exact reserve authority gaps.';

commit;
