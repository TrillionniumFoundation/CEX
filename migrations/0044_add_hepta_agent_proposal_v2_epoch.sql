begin;

do $$
begin
    if exists (select 1 from hepta_agent_proposals limit 1) and not (
        select count(*)=4
        from information_schema.columns
        where table_schema='public'
          and table_name='hepta_agent_proposals'
          and column_name in (
              'lease_id',
              'lease_fencing_token',
              'expected_work_version',
              'artifact_manifest_hash'
          )
    ) then
        raise exception using
            errcode = '23514',
            message = 'hepta_agent_proposal_v2_requires_empty_legacy_proposal_table',
            detail = 'V1 signatures did not bind lease/work epochs or manifest identity and cannot be upgraded into V2 records';
    end if;
end $$;

alter table hepta_agent_proposals
    add column if not exists lease_id uuid,
    add column if not exists lease_fencing_token bigint,
    add column if not exists expected_work_version bigint,
    add column if not exists artifact_manifest_hash text;

alter table hepta_agent_proposals
    alter column lease_id set not null,
    alter column lease_fencing_token set not null,
    alter column expected_work_version set not null,
    alter column artifact_manifest_hash set not null;

alter table hepta_agent_proposals
    drop constraint if exists hepta_agent_proposals_epoch_record_json_check;

do $$
begin
    if not exists (
        select 1 from pg_constraint
        where conrelid='hepta_agent_proposals'::regclass
          and conname='hepta_agent_proposals_lease_fencing_token_check'
    ) then
        alter table hepta_agent_proposals
            add constraint hepta_agent_proposals_lease_fencing_token_check
            check (lease_fencing_token between 1 and 9007199254740991);
    end if;
    if not exists (
        select 1 from pg_constraint
        where conrelid='hepta_agent_proposals'::regclass
          and conname='hepta_agent_proposals_expected_work_version_check'
    ) then
        alter table hepta_agent_proposals
            add constraint hepta_agent_proposals_expected_work_version_check
            check (expected_work_version between 1 and 9007199254740991);
    end if;
    if not exists (
        select 1 from pg_constraint
        where conrelid='hepta_agent_proposals'::regclass
          and conname='hepta_agent_proposals_payload_hash_check'
    ) then
        alter table hepta_agent_proposals
            add constraint hepta_agent_proposals_payload_hash_check
            check (payload_hash ~ '^sha256:[0-9a-f]{64}$');
    end if;
    if not exists (
        select 1 from pg_constraint
        where conrelid='hepta_agent_proposals'::regclass
          and conname='hepta_agent_proposals_artifact_manifest_hash_check'
    ) then
        alter table hepta_agent_proposals
            add constraint hepta_agent_proposals_artifact_manifest_hash_check
            check (artifact_manifest_hash ~ '^sha256:[0-9a-f]{64}$');
    end if;
    if not exists (
        select 1 from pg_constraint
        where conrelid='hepta_agent_proposals'::regclass
          and conname='hepta_agent_proposals_record_json_parity_check'
    ) then
        alter table hepta_agent_proposals
            add constraint hepta_agent_proposals_record_json_parity_check
            check (
                (record_json->>'proposal_id') is not distinct from proposal_id::text
                and (record_json->>'paper_project_id') is not distinct from paper_project_id::text
                and (record_json->>'work_item_id') is not distinct from work_item_id::text
                and (record_json->>'section_key') is not distinct from section_key
                and (record_json->>'parent_revision_id') is not distinct from parent_revision_id::text
                and (record_json->>'lease_id') is not distinct from lease_id::text
                and (record_json->>'lease_fencing_token') is not distinct from lease_fencing_token::text
                and (record_json->>'expected_work_version') is not distinct from expected_work_version::text
                and (record_json->>'proposal_kind') is not distinct from proposal_kind
                and (record_json->>'payload_hash') is not distinct from payload_hash
                and (record_json->>'artifact_manifest_id') is not distinct from artifact_manifest_id::text
                and (record_json->>'artifact_manifest_hash') is not distinct from artifact_manifest_hash
                and (record_json->>'agent_id') is not distinct from agent_id
                and (record_json->>'binding_id') is not distinct from binding_id::text
                and (record_json->>'agent_key_id') is not distinct from agent_key_id
                and (record_json->>'agent_public_key') is not distinct from agent_public_key
                and (record_json->>'signature') is not distinct from signature
                and (record_json->>'status') is not distinct from status
                and (record_json->>'version') is not distinct from version::text
                and signed_at is not distinct from to_timestamp((record_json->>'signed_at_unix')::double precision)
            );
    end if;
    if not exists (
        select 1 from pg_constraint
        where conrelid='hepta_agent_proposals'::regclass
          and conname='hepta_agent_proposals_lease_scope_fkey'
    ) then
        alter table hepta_agent_proposals
            add constraint hepta_agent_proposals_lease_scope_fkey
            foreign key (lease_id,paper_project_id)
            references hepta_section_leases(lease_id,paper_project_id);
    end if;
end $$;

create index if not exists hepta_agent_proposals_lease_epoch_v2_idx
    on hepta_agent_proposals (
        paper_project_id,
        section_key,
        lease_id,
        lease_fencing_token,
        expected_work_version
    );

commit;
