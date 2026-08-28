begin;

-- P0 evidence closure: externally verified Audit checkpoints and default-deny release qualification.

create table if not exists public.cex_audit_chain_checkpoints_v1 (
    checkpoint_id uuid primary key,
    org_id uuid not null references public.organizations(org_id),
    tenant_sequence bigint not null,
    chain_head_hash text not null,
    signer_key_id text not null,
    signer_public_key_sha256 text not null,
    signature_algorithm text not null,
    signature_base64 text not null,
    verification_evidence jsonb not null,
    admitted_by text not null,
    admitted_at timestamptz not null default now(),
    unique (org_id, tenant_sequence),
    constraint cex_audit_checkpoint_hash_v1 check (
        chain_head_hash ~ '^sha256:[0-9a-f]{64}$'
        and signer_public_key_sha256 ~ '^sha256:[0-9a-f]{64}$'
    ),
    constraint cex_audit_checkpoint_verify_v1 check (
        signature_algorithm='ed25519'
        and jsonb_typeof(verification_evidence)='object'
        and verification_evidence->>'verified'='true'
        and verification_evidence->>'algorithm'='ed25519'
        and length(btrim(admitted_by)) between 1 and 256
    )
);

create or replace function public.cex_admit_audit_chain_checkpoint_v1(
    p_org_id uuid,
    p_tenant_sequence bigint,
    p_chain_head_hash text,
    p_signer_key_id text,
    p_public_key_sha256 text,
    p_signature_base64 text,
    p_verification_evidence jsonb,
    p_admitted_by text
)
returns public.cex_audit_chain_checkpoints_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare current_sequence bigint; current_hash text;
    checkpoint_row public.cex_audit_chain_checkpoints_v1%rowtype;
begin
    if p_tenant_sequence <= 0 then raise exception 'checkpoint sequence must be positive'; end if;
    if p_chain_head_hash !~ '^sha256:[0-9a-f]{64}$' or p_public_key_sha256 !~ '^sha256:[0-9a-f]{64}$' then
        raise exception 'checkpoint digest invalid';
    end if;
    if jsonb_typeof(p_verification_evidence)<>'object'
       or p_verification_evidence->>'verified'<>'true'
       or p_verification_evidence->>'algorithm'<>'ed25519' then
        raise exception 'checkpoint requires verified Ed25519 evidence';
    end if;
    select last_sequence, last_event_hash into current_sequence, current_hash
      from public.cex_audit_chain_heads_v2
     where chain_key = 'org:' || p_org_id::text;
    if current_sequence is distinct from p_tenant_sequence or current_hash is distinct from p_chain_head_hash then
        raise exception 'checkpoint does not match current tenant Audit chain head';
    end if;
    insert into public.cex_audit_chain_checkpoints_v1 (
        checkpoint_id,org_id,tenant_sequence,chain_head_hash,signer_key_id,
        signer_public_key_sha256,signature_algorithm,signature_base64,
        verification_evidence,admitted_by
    ) values (
        public.cex_deterministic_uuid_v1('audit-checkpoint:'||p_org_id::text||':'||p_tenant_sequence::text),
        p_org_id,p_tenant_sequence,p_chain_head_hash,btrim(p_signer_key_id),
        p_public_key_sha256,'ed25519',p_signature_base64,p_verification_evidence,btrim(p_admitted_by)
    ) on conflict (org_id,tenant_sequence) do nothing
    returning * into checkpoint_row;
    if checkpoint_row.checkpoint_id is null then
        select * into checkpoint_row from public.cex_audit_chain_checkpoints_v1
         where org_id=p_org_id and tenant_sequence=p_tenant_sequence;
        if checkpoint_row.chain_head_hash is distinct from p_chain_head_hash
           or checkpoint_row.signer_key_id is distinct from btrim(p_signer_key_id)
           or checkpoint_row.signature_base64 is distinct from p_signature_base64 then
            raise exception using errcode='23505', message='Audit checkpoint collision';
        end if;
    end if;
    return checkpoint_row;
end
$$;

create table if not exists public.cex_release_candidates_v1 (
    release_id text primary key,
    source_commit_sha text not null,
    source_tree_sha text not null,
    migration_head text not null,
    created_by text not null,
    created_at timestamptz not null default now(),
    constraint cex_release_candidate_identity_v1 check (
        release_id ~ '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$'
        and source_commit_sha ~ '^[0-9a-f]{40}$'
        and source_tree_sha ~ '^[0-9a-f]{40}$'
        and migration_head = '0080_add_audit_checkpoint_and_release_qualification.sql'
        and length(btrim(created_by)) between 1 and 256
    )
);

create table if not exists public.cex_release_evidence_v1 (
    evidence_id uuid primary key,
    release_id text not null references public.cex_release_candidates_v1(release_id),
    evidence_type text not null,
    status text not null,
    artifact_uri text not null,
    artifact_sha256 text not null,
    admitted_by text not null,
    admitted_at timestamptz not null default now(),
    details jsonb not null default '{}'::jsonb,
    unique (release_id,evidence_type),
    constraint cex_release_evidence_type_v1 check (evidence_type in (
        'hosted_ci','migration_fresh','migration_upgrade','fault_injection','backup_restore',
        'production_like_soak','sbom','build_provenance','security_review','release_approval'
    )),
    constraint cex_release_evidence_status_v1 check (status in ('passed','approved')),
    constraint cex_release_evidence_hash_v1 check (artifact_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    constraint cex_release_evidence_text_v1 check (
        length(artifact_uri) between 1 and 2048 and length(btrim(admitted_by)) between 1 and 256
        and jsonb_typeof(details)='object'
    )
);

create or replace function public.cex_create_release_candidate_v1(
    p_release_id text,p_commit_sha text,p_tree_sha text,p_created_by text
)
returns public.cex_release_candidates_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare row_value public.cex_release_candidates_v1%rowtype;
begin
    insert into public.cex_release_candidates_v1 (
        release_id,source_commit_sha,source_tree_sha,migration_head,created_by
    ) values (btrim(p_release_id),p_commit_sha,p_tree_sha,
        '0080_add_audit_checkpoint_and_release_qualification.sql',btrim(p_created_by))
    on conflict (release_id) do nothing returning * into row_value;
    if row_value.release_id is null then
        select * into row_value from public.cex_release_candidates_v1 where release_id=btrim(p_release_id);
        if row_value.source_commit_sha is distinct from p_commit_sha
           or row_value.source_tree_sha is distinct from p_tree_sha then
            raise exception using errcode='23505', message='release candidate identity collision';
        end if;
    end if;
    return row_value;
end
$$;

create or replace function public.cex_admit_release_evidence_v1(
    p_release_id text,p_evidence_type text,p_status text,p_artifact_uri text,
    p_artifact_sha256 text,p_admitted_by text,p_details jsonb default '{}'::jsonb
)
returns public.cex_release_evidence_v1
language plpgsql
set search_path = pg_catalog, public
as $$
declare row_value public.cex_release_evidence_v1%rowtype;
begin
    insert into public.cex_release_evidence_v1 (
        evidence_id,release_id,evidence_type,status,artifact_uri,artifact_sha256,admitted_by,details
    ) values (
        public.cex_deterministic_uuid_v1('release-evidence:'||p_release_id||':'||p_evidence_type),
        p_release_id,p_evidence_type,p_status,p_artifact_uri,p_artifact_sha256,btrim(p_admitted_by),p_details
    ) on conflict (release_id,evidence_type) do nothing returning * into row_value;
    if row_value.evidence_id is null then
        select * into row_value from public.cex_release_evidence_v1
         where release_id=p_release_id and evidence_type=p_evidence_type;
        if row_value.status is distinct from p_status
           or row_value.artifact_uri is distinct from p_artifact_uri
           or row_value.artifact_sha256 is distinct from p_artifact_sha256 then
            raise exception using errcode='23505', message='release evidence collision';
        end if;
    end if;
    return row_value;
end
$$;

create or replace view public.cex_release_qualification_v1 as
select
    candidate.release_id,
    candidate.source_commit_sha,
    candidate.source_tree_sha,
    candidate.migration_head,
    count(evidence.evidence_type)::bigint as admitted_evidence_count,
    array_agg(required.evidence_type order by required.evidence_type)
        filter (where evidence.evidence_type is null) as missing_evidence,
    count(*) filter (where evidence.evidence_type is null)=0 as eligible
from public.cex_release_candidates_v1 candidate
cross join (values
    ('hosted_ci'),('migration_fresh'),('migration_upgrade'),('fault_injection'),
    ('backup_restore'),('production_like_soak'),('sbom'),('build_provenance'),
    ('security_review'),('release_approval')
) required(evidence_type)
left join public.cex_release_evidence_v1 evidence
  on evidence.release_id=candidate.release_id and evidence.evidence_type=required.evidence_type
group by candidate.release_id,candidate.source_commit_sha,candidate.source_tree_sha,candidate.migration_head;

create or replace function public.cex_reject_p0_evidence_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin raise exception 'P0 release and checkpoint evidence is append-only'; end
$$;

drop trigger if exists trg_cex_reject_audit_checkpoint_mutation_v1 on public.cex_audit_chain_checkpoints_v1;
create trigger trg_cex_reject_audit_checkpoint_mutation_v1
before update or delete on public.cex_audit_chain_checkpoints_v1
for each row execute function public.cex_reject_p0_evidence_mutation_v1();
drop trigger if exists trg_cex_reject_release_candidate_mutation_v1 on public.cex_release_candidates_v1;
create trigger trg_cex_reject_release_candidate_mutation_v1
before update or delete on public.cex_release_candidates_v1
for each row execute function public.cex_reject_p0_evidence_mutation_v1();
drop trigger if exists trg_cex_reject_release_evidence_mutation_v1 on public.cex_release_evidence_v1;
create trigger trg_cex_reject_release_evidence_mutation_v1
before update or delete on public.cex_release_evidence_v1
for each row execute function public.cex_reject_p0_evidence_mutation_v1();

revoke execute on function public.cex_admit_audit_chain_checkpoint_v1(uuid,bigint,text,text,text,text,jsonb,text) from public;
revoke execute on function public.cex_create_release_candidate_v1(text,text,text,text) from public;
revoke execute on function public.cex_admit_release_evidence_v1(text,text,text,text,text,text,jsonb) from public;
grant execute on function public.cex_admit_audit_chain_checkpoint_v1(uuid,bigint,text,text,text,text,jsonb,text),
    public.cex_create_release_candidate_v1(text,text,text,text),
    public.cex_admit_release_evidence_v1(text,text,text,text,text,text,jsonb)
    to cex_release_evidence_admitter;

commit;
