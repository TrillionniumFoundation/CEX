begin;

-- A V1 projection discarded the reproduction and Appeal-resolution identities.
-- Those facts cannot be reconstructed from the projection itself, so an
-- already-finalized deployment must explicitly re-verify instead of receiving
-- a guessed V2 binding during migration.
do $$
begin
    if not exists (
        select 1 from pg_attribute
        where attrelid = 'hepta_paper_chain_finality_projections'::regclass
          and attname = 'reproduction_id' and not attisdropped
    ) and exists (select 1 from hepta_paper_chain_finality_projections) then
        raise exception
            '0048 consumer-finality V2 refuses to infer bindings for existing V1 projections; re-verification is required';
    end if;
end
$$;

alter table hepta_paper_chain_finality_projections
    add column if not exists reproduction_id uuid,
    add column if not exists appeal_resolution_id uuid;

do $$
begin
    if exists (
        select 1 from hepta_paper_chain_finality_projections
        where version <> 2
           or reproduction_id is null
           or record_json ->> 'schema'
                <> 'hepta.paper_raid.chain_finality_projection.v2'
           or nullif(record_json ->> 'evaluation_id', '')::uuid
                is distinct from evaluation_id
           or nullif(record_json ->> 'reproduction_id', '')::uuid
                is distinct from reproduction_id
           or nullif(record_json ->> 'appeal_resolution_id', '')::uuid
                is distinct from appeal_resolution_id
    ) then
        raise exception
            '0048 consumer-finality V2 found a legacy or relational/JSON-divergent projection';
    end if;
end
$$;

alter table hepta_paper_chain_finality_projections
    alter column reproduction_id set not null;

alter table hepta_paper_chain_finality_projections
    drop constraint if exists hepta_paper_chain_finality_projections_version_check;
alter table hepta_paper_chain_finality_projections
    add constraint hepta_paper_chain_finality_projections_version_check
    check (version = 2);

create unique index if not exists hepta_paper_appeal_resolutions_resolution_paper_idx
    on hepta_paper_appeal_resolutions (resolution_id, paper_project_id);

do $$
begin
    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_projections'::regclass
          and conname = 'hepta_paper_chain_finality_projection_reproduction_fkey'
    ) then
        alter table hepta_paper_chain_finality_projections
            add constraint hepta_paper_chain_finality_projection_reproduction_fkey
            foreign key (reproduction_id, paper_project_id)
            references hepta_paper_reproductions (reproduction_id, paper_project_id);
    end if;
    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_projections'::regclass
          and conname = 'hepta_paper_chain_finality_projection_resolution_fkey'
    ) then
        alter table hepta_paper_chain_finality_projections
            add constraint hepta_paper_chain_finality_projection_resolution_fkey
            foreign key (appeal_resolution_id, paper_project_id)
            references hepta_paper_appeal_resolutions (resolution_id, paper_project_id);
    end if;
end
$$;

create or replace function hepta_validate_consumer_finality_v2_projection()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    reproduction_record record;
    evaluation_record record;
    resolution_record record;
    appeal_record record;
begin
    if new.status <> 'verified_finality'
       or new.version <> 2
       or new.record_json ->> 'schema'
            <> 'hepta.paper_raid.chain_finality_projection.v2'
       or nullif(new.record_json ->> 'paper_project_id', '')::uuid
            is distinct from new.paper_project_id
       or nullif(new.record_json ->> 'evaluation_id', '')::uuid
            is distinct from new.evaluation_id
       or nullif(new.record_json ->> 'reproduction_id', '')::uuid
            is distinct from new.reproduction_id
       or nullif(new.record_json ->> 'appeal_resolution_id', '')::uuid
            is distinct from new.appeal_resolution_id
       or nullif(new.record_json ->> 'version', '')::bigint <> 2
       or new.record_json ->> 'status' <> 'verified_finality'
       or coalesce((new.record_json ->> 'ranking_eligible')::boolean, true)
       or coalesce((new.record_json ->> 'reward_eligible')::boolean, true)
       or coalesce((new.record_json ->> 'score_eligible')::boolean, true)
       or coalesce((new.record_json ->> 'economic_eligible')::boolean, true) then
        raise exception 'consumer-finality V2 projection relational/JSON parity failed';
    end if;

    select evaluation_id, paper_project_id
      into reproduction_record
      from public.hepta_paper_reproductions
     where reproduction_id = new.reproduction_id;
    if not found
       or reproduction_record.paper_project_id <> new.paper_project_id
       or reproduction_record.evaluation_id <> new.evaluation_id then
        raise exception 'consumer-finality V2 reproduction binding is not exact';
    end if;

    select supersedes_evaluation_id
      into evaluation_record
      from public.hepta_paper_evaluations
     where evaluation_id = new.evaluation_id
       and paper_project_id = new.paper_project_id;
    if not found then
        raise exception 'consumer-finality V2 evaluation binding is missing';
    end if;

    if new.appeal_resolution_id is null then
        if evaluation_record.supersedes_evaluation_id is not null
           or exists (
                select 1 from public.hepta_paper_appeals
                where evaluation_id = new.evaluation_id
                  and paper_project_id = new.paper_project_id
           ) then
            raise exception 'consumer-finality V2 null resolution is valid only for an unappealed root';
        end if;
        return new;
    end if;

    select appeal_id, outcome, superseding_evaluation_id
      into resolution_record
      from public.hepta_paper_appeal_resolutions
     where resolution_id = new.appeal_resolution_id
       and paper_project_id = new.paper_project_id;
    if not found then
        raise exception 'consumer-finality V2 resolution binding is missing';
    end if;
    select evaluation_id
      into appeal_record
      from public.hepta_paper_appeals
     where appeal_id = resolution_record.appeal_id
       and paper_project_id = new.paper_project_id;
    if not found then
        raise exception 'consumer-finality V2 Appeal binding is missing';
    end if;

    if resolution_record.outcome = 'denied' then
        if appeal_record.evaluation_id <> new.evaluation_id
           or resolution_record.superseding_evaluation_id is not null then
            raise exception 'consumer-finality V2 denial does not bind the effective evaluation';
        end if;
    elsif resolution_record.outcome = 'upheld' then
        if resolution_record.superseding_evaluation_id <> new.evaluation_id
           or evaluation_record.supersedes_evaluation_id <> appeal_record.evaluation_id then
            raise exception 'consumer-finality V2 uphold does not activate the effective evaluation';
        end if;
    else
        raise exception 'consumer-finality V2 resolution outcome is unsupported';
    end if;
    return new;
end
$$;

drop trigger if exists hepta_consumer_finality_v2_projection_guard
    on hepta_paper_chain_finality_projections;
create trigger hepta_consumer_finality_v2_projection_guard
before insert or update on hepta_paper_chain_finality_projections
for each row execute function hepta_validate_consumer_finality_v2_projection();

commit;
