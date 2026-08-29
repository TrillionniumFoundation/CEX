begin;

create table if not exists hepta_challenge_pack_activations_v1 (
    activation_id uuid primary key,
    challenge_id uuid not null,
    request_sha256 text not null,
    pack_id text not null,
    template text not null,
    ruleset_version text not null,
    ruleset_hash text not null,
    dataset_manifest_hash text not null,
    evaluator_manifest_hash text not null,
    candidate_state text not null,
    integration_base_revision text not null,
    integration_source_tree text not null,
    hepta_base_revision text not null,
    hepta_source_tree text not null,
    source_fileset_sha256 text not null,
    release_id text not null,
    release_image_lock_sha256 text not null,
    release_provenance_sha256 text not null,
    source_catalog_sha256 text not null,
    pack_manifest_sha256 text not null,
    cas_activation_receipt_sha256 text not null,
    cas_activation_catalog_patch_sha256 text not null,
    strict_review_evidence_sha256 text not null,
    cross_paper_denial_receipt_sha256 text not null,
    activated_at timestamptz not null,
    record_json jsonb not null,
    constraint hepta_challenge_pack_activations_challenge_key unique (challenge_id),
    constraint hepta_challenge_pack_activations_request_key unique (request_sha256),
    constraint hepta_challenge_pack_activations_non_nil_id_check
        check (activation_id <> '00000000-0000-0000-0000-000000000000'::uuid),
    constraint hepta_challenge_pack_activations_request_hash_check
        check (
            request_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and request_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
        ),
    constraint hepta_challenge_pack_activations_identity_check
        check (
            pack_id = 'paper-raid-evidence-audit-seeded-v1'
            and template = 'evidence-audit'
            and ruleset_version = 'paper-raid-evidence-audit-v1'
            and ruleset_hash = 'sha256:54a740273a82d56938a20db1669b236e8b3724defffe32fb91768632f7994453'
            and dataset_manifest_hash = 'sha256:1363088a76b2dd5c77b04b2620a4806100f1125c482a3e6413691d1a8c372a75'
            and evaluator_manifest_hash = 'sha256:7d7f0096261132ceda30e66586e49129aaa99bb979772ceec9b2940eda21261e'
            and source_catalog_sha256 = 'sha256:fc649c1f55ef484bc2f8baf279dec1c668eaa610691ac4234a74df1d90aaa6bb'
            and pack_manifest_sha256 = 'sha256:69a695a6a75dd71e2c53c7a832298ab2ecbcd4086fa4ef653914dd2e5770b37c'
        ),
    constraint hepta_challenge_pack_activations_candidate_state_check
        check (candidate_state in ('immutable_candidate_pending_evidence', 'verified_candidate')),
    constraint hepta_challenge_pack_activations_git_pins_check
        check (
            integration_base_revision ~ '^[0-9a-f]{40}$'
            and integration_source_tree ~ '^[0-9a-f]{40}$'
            and hepta_base_revision ~ '^[0-9a-f]{40}$'
            and hepta_source_tree ~ '^[0-9a-f]{40}$'
            and integration_base_revision <> '0000000000000000000000000000000000000000'
            and integration_source_tree <> '0000000000000000000000000000000000000000'
            and hepta_base_revision <> '0000000000000000000000000000000000000000'
            and hepta_source_tree <> '0000000000000000000000000000000000000000'
        ),
    constraint hepta_challenge_pack_activations_release_id_check
        check (release_id ~ '^[a-z0-9][a-z0-9._-]{0,127}$'),
    constraint hepta_challenge_pack_activations_evidence_digests_check
        check (
            source_fileset_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and release_image_lock_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and release_provenance_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and cas_activation_receipt_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and cas_activation_catalog_patch_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and strict_review_evidence_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and cross_paper_denial_receipt_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and source_fileset_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and release_image_lock_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and release_provenance_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and cas_activation_receipt_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and cas_activation_catalog_patch_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and strict_review_evidence_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and cross_paper_denial_receipt_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
        )
);

create index if not exists hepta_challenge_pack_activations_activated_at_idx
    on hepta_challenge_pack_activations_v1 (activated_at, challenge_id);

create or replace function hepta_validate_challenge_pack_activation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    request_json jsonb;
    candidate_json jsonb;
    evidence_json jsonb;
begin
    if jsonb_typeof(new.record_json) <> 'object'
       or (select count(*) from jsonb_object_keys(new.record_json)) <> 8 then
        raise exception 'Challenge Pack activation record must use the exact V1 object shape';
    end if;
    request_json := new.record_json -> 'request';
    candidate_json := request_json -> 'candidate';
    evidence_json := request_json -> 'evidence';
    if jsonb_typeof(request_json) <> 'object'
       or jsonb_typeof(candidate_json) <> 'object'
       or jsonb_typeof(evidence_json) <> 'object'
       or (select count(*) from jsonb_object_keys(request_json)) <> 11
       or (select count(*) from jsonb_object_keys(candidate_json)) <> 14
       or (select count(*) from jsonb_object_keys(evidence_json)) <> 14 then
        raise exception 'Challenge Pack activation nested contract shape diverged';
    end if;

    if new.record_json ->> 'schema' <> 'hepta.challenge_pack.activation_record.v1'
       or nullif(new.record_json ->> 'activation_id', '')::uuid
            is distinct from new.activation_id
       or nullif(new.record_json ->> 'challenge_id', '')::uuid
            is distinct from new.challenge_id
       or new.record_json ->> 'request_sha256' is distinct from new.request_sha256
       or new.record_json ->> 'previous_status' <> 'draft'
       or new.record_json ->> 'activated_status' <> 'open'
       or nullif(new.record_json ->> 'activated_at', '')::timestamptz
            is distinct from new.activated_at then
        raise exception 'Challenge Pack activation record relational/JSON parity failed';
    end if;

    if request_json ->> 'schema' <> 'hepta.challenge_pack.activation_request.v1'
       or request_json ->> 'template' is distinct from new.template
       or request_json ->> 'pack_id' is distinct from new.pack_id
       or request_json ->> 'expected_status' <> 'draft'
       or request_json ->> 'requested_status' <> 'open'
       or request_json ->> 'ruleset_version' is distinct from new.ruleset_version
       or request_json ->> 'ruleset_hash' is distinct from new.ruleset_hash
       or request_json ->> 'dataset_manifest_hash'
            is distinct from new.dataset_manifest_hash
       or request_json ->> 'evaluator_manifest_hash'
            is distinct from new.evaluator_manifest_hash then
        raise exception 'Challenge Pack activation request relational/JSON parity failed';
    end if;

    if candidate_json ->> 'schema' <> 'trnm.paper-raid.current-candidate-binding.v2'
       or candidate_json ->> 'state' is distinct from new.candidate_state
       or candidate_json ->> 'integration_base_revision'
            is distinct from new.integration_base_revision
       or candidate_json ->> 'integration_source_tree'
            is distinct from new.integration_source_tree
       or candidate_json ->> 'hepta_base_revision'
            is distinct from new.hepta_base_revision
       or candidate_json ->> 'hepta_source_tree'
            is distinct from new.hepta_source_tree
       or jsonb_typeof(candidate_json -> 'component_pins_authoritative')
            is distinct from 'boolean'
       or coalesce((candidate_json ->> 'component_pins_authoritative')::boolean, false) is not true
       or jsonb_typeof(candidate_json -> 'working_tree_clean')
            is distinct from 'boolean'
       or coalesce((candidate_json ->> 'working_tree_clean')::boolean, false) is not true
       or candidate_json ->> 'tracked_image_lock_status' <> 'unbound'
       or candidate_json ->> 'source_fileset_sha256'
            is distinct from new.source_fileset_sha256
       or candidate_json ->> 'release_image_lock_status' <> 'locked'
       or candidate_json ->> 'release_id' is distinct from new.release_id
       or candidate_json ->> 'release_image_lock_sha256'
            is distinct from new.release_image_lock_sha256
       or candidate_json ->> 'release_provenance_sha256'
            is distinct from new.release_provenance_sha256 then
        raise exception 'Challenge Pack activation candidate relational/JSON parity failed';
    end if;

    if evidence_json ->> 'schema' <> 'hepta.challenge_pack.activation_evidence.v1'
       or evidence_json ->> 'source_catalog_sha256'
            is distinct from new.source_catalog_sha256
       or evidence_json ->> 'pack_manifest_sha256'
            is distinct from new.pack_manifest_sha256
       or evidence_json ->> 'cas_activation_receipt_schema'
            <> 'hepta.challenge_pack.cas_activation_receipt.v1'
       or evidence_json ->> 'cas_activation_receipt_sha256'
            is distinct from new.cas_activation_receipt_sha256
       or evidence_json ->> 'cas_activation_catalog_patch_schema'
            <> 'hepta.challenge_pack.activation_catalog_patch.v2'
       or evidence_json ->> 'cas_activation_catalog_patch_sha256'
            is distinct from new.cas_activation_catalog_patch_sha256
       or jsonb_typeof(evidence_json -> 'cas_all_packs_verified')
            is distinct from 'boolean'
       or coalesce((evidence_json ->> 'cas_all_packs_verified')::boolean, false) is not true
       or jsonb_typeof(evidence_json -> 'cas_scoped_readback')
            is distinct from 'boolean'
       or coalesce((evidence_json ->> 'cas_scoped_readback')::boolean, false) is not true
       or jsonb_typeof(evidence_json -> 'cas_pack_count')
            is distinct from 'number'
       or nullif(evidence_json ->> 'cas_pack_count', '')::integer <> 3
       or jsonb_typeof(evidence_json -> 'cas_object_count')
            is distinct from 'number'
       or nullif(evidence_json ->> 'cas_object_count', '')::integer <> 26
       or evidence_json ->> 'strict_review_evidence_schema'
            <> 'trnm.paper-raid.strict-review-evidence.v1'
       or evidence_json ->> 'strict_review_evidence_sha256'
            is distinct from new.strict_review_evidence_sha256
       or evidence_json ->> 'cross_paper_denial_receipt_sha256'
            is distinct from new.cross_paper_denial_receipt_sha256 then
        raise exception 'Challenge Pack activation evidence relational/JSON parity failed';
    end if;
    return new;
end
$$;

create or replace function hepta_reject_challenge_pack_activation_mutation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog
as $$
begin
    raise exception 'Challenge Pack activation records are append-only';
end
$$;

drop trigger if exists hepta_challenge_pack_activation_validate_guard
    on hepta_challenge_pack_activations_v1;
create trigger hepta_challenge_pack_activation_validate_guard
before insert on hepta_challenge_pack_activations_v1
for each row execute function hepta_validate_challenge_pack_activation_v1();

drop trigger if exists hepta_challenge_pack_activation_immutable_guard
    on hepta_challenge_pack_activations_v1;
create trigger hepta_challenge_pack_activation_immutable_guard
before update or delete on hepta_challenge_pack_activations_v1
for each row execute function hepta_reject_challenge_pack_activation_mutation_v1();

drop trigger if exists hepta_challenge_pack_activation_truncate_guard
    on hepta_challenge_pack_activations_v1;
create trigger hepta_challenge_pack_activation_truncate_guard
before truncate on hepta_challenge_pack_activations_v1
for each statement execute function hepta_reject_challenge_pack_activation_mutation_v1();

alter table hepta_challenge_pack_activations_v1
    enable always trigger hepta_challenge_pack_activation_validate_guard;
alter table hepta_challenge_pack_activations_v1
    enable always trigger hepta_challenge_pack_activation_immutable_guard;
alter table hepta_challenge_pack_activations_v1
    enable always trigger hepta_challenge_pack_activation_truncate_guard;

revoke all on function public.hepta_validate_challenge_pack_activation_v1() from public;
revoke all on function public.hepta_reject_challenge_pack_activation_mutation_v1() from public;

commit;
