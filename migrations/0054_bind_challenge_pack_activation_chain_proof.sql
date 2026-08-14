begin;

do $$
declare
    proof_column_count integer;
    incompatible_existing_record boolean;
begin
    if to_regclass('public.hepta_challenge_pack_activations_v1') is null then
        raise exception 'Challenge Pack activation V1 table must exist before 0054';
    end if;
    select count(*)::integer
      into proof_column_count
      from pg_attribute
     where attrelid = 'public.hepta_challenge_pack_activations_v1'::regclass
       and attname in (
         'strict_review_chain_proof_manifest_sha256',
         'strict_review_chain_proof_fileset_sha256',
         'strict_review_terminal_bundle_schema',
         'strict_review_terminal_bundle_sha256'
       )
       and attnum > 0
       and not attisdropped;
    if exists (select 1 from public.hepta_challenge_pack_activations_v1) then
        if proof_column_count <> 4 then
            raise exception 'Existing activation records cannot be upgraded without Chain proof authority';
        end if;
        execute $check$
            select exists (
              select 1
                from public.hepta_challenge_pack_activations_v1 activation
               where case
                 when jsonb_typeof(activation.record_json #> '{request,evidence}')
                        is distinct from 'object'
                   then true
                 else
                   (select count(*)
                      from jsonb_object_keys(
                        activation.record_json #> '{request,evidence}'
                      )) <> 18
                   or activation.record_json #>>
                        '{request,evidence,strict_review_chain_proof_manifest_sha256}'
                        is distinct from activation.strict_review_chain_proof_manifest_sha256
                   or activation.record_json #>>
                        '{request,evidence,strict_review_chain_proof_fileset_sha256}'
                        is distinct from activation.strict_review_chain_proof_fileset_sha256
                   or activation.record_json #>>
                        '{request,evidence,strict_review_terminal_bundle_schema}'
                        is distinct from activation.strict_review_terminal_bundle_schema
                   or activation.record_json #>>
                        '{request,evidence,strict_review_terminal_bundle_schema}'
                        <> 'trnm.paper-raid.strict-review-terminal-bundle-binding.v1'
                   or activation.record_json #>>
                        '{request,evidence,strict_review_terminal_bundle_sha256}'
                        is distinct from activation.strict_review_terminal_bundle_sha256
               end
            )
        $check$ into incompatible_existing_record;
        if incompatible_existing_record then
            raise exception 'Existing activation records are not exact 0054 Chain proof records';
        end if;
    end if;
end
$$;

alter table public.hepta_challenge_pack_activations_v1
    add column if not exists strict_review_chain_proof_manifest_sha256 text,
    add column if not exists strict_review_chain_proof_fileset_sha256 text,
    add column if not exists strict_review_terminal_bundle_schema text,
    add column if not exists strict_review_terminal_bundle_sha256 text;

alter table public.hepta_challenge_pack_activations_v1
    alter column strict_review_chain_proof_manifest_sha256 set not null,
    alter column strict_review_chain_proof_fileset_sha256 set not null,
    alter column strict_review_terminal_bundle_schema set not null,
    alter column strict_review_terminal_bundle_sha256 set not null;

do $$
declare
    invalid_columns bigint;
begin
    select count(*)::bigint
      into invalid_columns
      from pg_attribute attribute
      left join pg_attrdef attribute_default
        on attribute_default.adrelid = attribute.attrelid
       and attribute_default.adnum = attribute.attnum
     where attribute.attrelid = 'public.hepta_challenge_pack_activations_v1'::regclass
       and attribute.attname in (
         'strict_review_chain_proof_manifest_sha256',
         'strict_review_chain_proof_fileset_sha256',
         'strict_review_terminal_bundle_schema',
         'strict_review_terminal_bundle_sha256'
       )
       and attribute.attnum > 0
       and not attribute.attisdropped
       and (
         attribute.atttypid <> 'text'::regtype
         or attribute.atttypmod <> -1
         or not attribute.attnotnull
         or attribute_default.oid is not null
       );
    if invalid_columns <> 0 then
        raise exception 'Challenge Pack activation Chain proof columns are non-canonical';
    end if;
end
$$;

alter table public.hepta_challenge_pack_activations_v1
    drop constraint if exists hepta_challenge_pack_activations_evidence_digests_check;
alter table public.hepta_challenge_pack_activations_v1
    add constraint hepta_challenge_pack_activations_evidence_digests_check
        check (
            source_fileset_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and release_image_lock_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and release_provenance_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and cas_activation_receipt_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and cas_activation_catalog_patch_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and strict_review_evidence_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and strict_review_chain_proof_manifest_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and strict_review_chain_proof_fileset_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and strict_review_terminal_bundle_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and cross_paper_denial_receipt_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and source_fileset_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and release_image_lock_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and release_provenance_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and cas_activation_receipt_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and cas_activation_catalog_patch_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and strict_review_evidence_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and strict_review_chain_proof_manifest_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and strict_review_chain_proof_fileset_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and strict_review_terminal_bundle_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
            and cross_paper_denial_receipt_sha256 <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
        );

create or replace function public.hepta_validate_challenge_pack_activation_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    request_json jsonb;
    candidate_json jsonb;
    evidence_json jsonb;
begin
    if jsonb_typeof(new.record_json) is distinct from 'object'
       or (select count(*) from jsonb_object_keys(new.record_json)) <> 8 then
        raise exception 'Challenge Pack activation record must use the exact V1 object shape';
    end if;
    request_json := new.record_json -> 'request';
    candidate_json := request_json -> 'candidate';
    evidence_json := request_json -> 'evidence';
    if jsonb_typeof(request_json) is distinct from 'object'
       or jsonb_typeof(candidate_json) is distinct from 'object'
       or jsonb_typeof(evidence_json) is distinct from 'object'
       or (select count(*) from jsonb_object_keys(request_json)) <> 11
       or (select count(*) from jsonb_object_keys(candidate_json)) <> 14
       or (select count(*) from jsonb_object_keys(evidence_json)) <> 18 then
        raise exception 'Challenge Pack activation nested contract shape diverged';
    end if;

    if new.record_json ->> 'schema'
            is distinct from 'hepta.challenge_pack.activation_record.v1'
       or nullif(new.record_json ->> 'activation_id', '')::uuid
            is distinct from new.activation_id
       or nullif(new.record_json ->> 'challenge_id', '')::uuid
            is distinct from new.challenge_id
       or new.record_json ->> 'request_sha256' is distinct from new.request_sha256
       or new.record_json ->> 'previous_status' is distinct from 'draft'
       or new.record_json ->> 'activated_status' is distinct from 'open'
       or nullif(new.record_json ->> 'activated_at', '')::timestamptz
            is distinct from new.activated_at then
        raise exception 'Challenge Pack activation record relational/JSON parity failed';
    end if;

    if request_json ->> 'schema'
            is distinct from 'hepta.challenge_pack.activation_request.v1'
       or request_json ->> 'template' is distinct from new.template
       or request_json ->> 'pack_id' is distinct from new.pack_id
       or request_json ->> 'expected_status' is distinct from 'draft'
       or request_json ->> 'requested_status' is distinct from 'open'
       or request_json ->> 'ruleset_version' is distinct from new.ruleset_version
       or request_json ->> 'ruleset_hash' is distinct from new.ruleset_hash
       or request_json ->> 'dataset_manifest_hash'
            is distinct from new.dataset_manifest_hash
       or request_json ->> 'evaluator_manifest_hash'
            is distinct from new.evaluator_manifest_hash then
        raise exception 'Challenge Pack activation request relational/JSON parity failed';
    end if;

    if candidate_json ->> 'schema'
            is distinct from 'trnm.paper-raid.current-candidate-binding.v2'
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
       or candidate_json ->> 'tracked_image_lock_status' is distinct from 'unbound'
       or candidate_json ->> 'source_fileset_sha256'
            is distinct from new.source_fileset_sha256
       or candidate_json ->> 'release_image_lock_status' is distinct from 'locked'
       or candidate_json ->> 'release_id' is distinct from new.release_id
       or candidate_json ->> 'release_image_lock_sha256'
            is distinct from new.release_image_lock_sha256
       or candidate_json ->> 'release_provenance_sha256'
            is distinct from new.release_provenance_sha256 then
        raise exception 'Challenge Pack activation candidate relational/JSON parity failed';
    end if;

    if evidence_json ->> 'schema'
            is distinct from 'hepta.challenge_pack.activation_evidence.v1'
       or evidence_json ->> 'source_catalog_sha256'
            is distinct from new.source_catalog_sha256
       or evidence_json ->> 'pack_manifest_sha256'
            is distinct from new.pack_manifest_sha256
       or evidence_json ->> 'cas_activation_receipt_schema'
            is distinct from 'hepta.challenge_pack.cas_activation_receipt.v1'
       or evidence_json ->> 'cas_activation_receipt_sha256'
            is distinct from new.cas_activation_receipt_sha256
       or evidence_json ->> 'cas_activation_catalog_patch_schema'
            is distinct from 'hepta.challenge_pack.activation_catalog_patch.v2'
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
            is distinct from 'trnm.paper-raid.strict-review-evidence.v1'
       or evidence_json ->> 'strict_review_evidence_sha256'
            is distinct from new.strict_review_evidence_sha256
       or evidence_json ->> 'strict_review_chain_proof_manifest_sha256'
            is distinct from new.strict_review_chain_proof_manifest_sha256
       or evidence_json ->> 'strict_review_chain_proof_fileset_sha256'
            is distinct from new.strict_review_chain_proof_fileset_sha256
       or evidence_json ->> 'strict_review_terminal_bundle_schema'
            is distinct from 'trnm.paper-raid.strict-review-terminal-bundle-binding.v1'
       or evidence_json ->> 'strict_review_terminal_bundle_schema'
            is distinct from new.strict_review_terminal_bundle_schema
       or evidence_json ->> 'strict_review_terminal_bundle_sha256'
            is distinct from new.strict_review_terminal_bundle_sha256
       or evidence_json ->> 'cross_paper_denial_receipt_sha256'
            is distinct from new.cross_paper_denial_receipt_sha256 then
        raise exception 'Challenge Pack activation evidence relational/JSON parity failed';
    end if;
    return new;
end
$$;

revoke all on function public.hepta_validate_challenge_pack_activation_v1() from public;

commit;
