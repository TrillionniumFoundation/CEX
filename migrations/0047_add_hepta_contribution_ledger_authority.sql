begin;

-- Promotion, rather than the later ledger write, owns the caller-supplied
-- global ID.  Keep this table deliberately small: startup verifies the exact
-- physical shape and migration 0047 refuses to guess around foreign columns.
create table if not exists hepta_paper_contribution_ledger_reservations (
    contribution_ledger_id uuid primary key,
    paper_project_id uuid not null,
    release_candidate_hash text not null,
    created_at timestamptz not null,
    constraint hepta_contribution_ledger_reservations_non_nil_id_check check (
        contribution_ledger_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ),
    constraint hepta_contribution_ledger_reservations_release_hash_check check (
        release_candidate_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    constraint hepta_contribution_ledger_reservations_ownership_key
        unique (contribution_ledger_id, paper_project_id, release_candidate_hash),
    constraint hepta_contribution_ledger_reservations_paper_fkey
        foreign key (paper_project_id)
        references hepta_paper_projects(paper_project_id)
);

-- Freeze every relation whose rows contribute to the upgrade decision.  This
-- closes proposal-acceptance, legacy-candidate, reservation and ledger races
-- until the exact catalog has been rebuilt and committed.
lock table
    hepta_agent_proposals,
    hepta_paper_projects,
    hepta_paper_revisions,
    hepta_paper_contribution_ledgers,
    hepta_paper_contribution_ledger_reservations
in access exclusive mode;

alter table hepta_paper_contribution_ledgers
    add column if not exists entries_json jsonb;

-- A dropped or injected column changes attnum as well as the live-column list.
-- Refuse both conditions instead of silently attaching scientific authority to
-- an unknown physical layout.  Defaults are repaired below; identity and
-- generated columns cannot be repaired without rewriting their meaning.
do $block$
declare
    ledger_shape jsonb;
    reservation_shape jsonb;
begin
    select jsonb_agg(
               jsonb_build_array(
                   attribute.attnum,
                   attribute.attname,
                   format_type(attribute.atttypid, attribute.atttypmod),
                   attribute.attidentity,
                   attribute.attgenerated
               ) order by attribute.attnum
           )
      into ledger_shape
      from pg_attribute attribute
     where attribute.attrelid = 'hepta_paper_contribution_ledgers'::regclass
       and attribute.attnum > 0
       and not attribute.attisdropped;

    if ledger_shape is distinct from '[
        [1,"contribution_ledger_id","uuid","",""],
        [2,"paper_project_id","uuid","",""],
        [3,"release_candidate_hash","text","",""],
        [4,"ledger_hash","text","",""],
        [5,"version","bigint","",""],
        [6,"record_json","jsonb","",""],
        [7,"created_at","timestamp with time zone","",""],
        [8,"entries_json","jsonb","",""]
    ]'::jsonb then
        raise exception using
            errcode = '55000',
            message = 'contribution_ledger_table_shape_requires_operator_review',
            detail = coalesce(ledger_shape::text, 'missing relation');
    end if;

    select jsonb_agg(
               jsonb_build_array(
                   attribute.attnum,
                   attribute.attname,
                   format_type(attribute.atttypid, attribute.atttypmod),
                   attribute.attidentity,
                   attribute.attgenerated
               ) order by attribute.attnum
           )
      into reservation_shape
      from pg_attribute attribute
     where attribute.attrelid = 'hepta_paper_contribution_ledger_reservations'::regclass
       and attribute.attnum > 0
       and not attribute.attisdropped;

    if reservation_shape is distinct from '[
        [1,"contribution_ledger_id","uuid","",""],
        [2,"paper_project_id","uuid","",""],
        [3,"release_candidate_hash","text","",""],
        [4,"created_at","timestamp with time zone","",""]
    ]'::jsonb then
        raise exception using
            errcode = '55000',
            message = 'contribution_reservation_table_shape_requires_operator_review',
            detail = coalesce(reservation_shape::text, 'missing relation');
    end if;
end
$block$;

-- These are ordinary permanent heap tables with no policy, inheritance,
-- rewrite-rule or storage-option escape hatch.  Owner and effective tablespace
-- must match the pre-existing ledger authority; startup verifies them again.
do $block$
declare
    unsafe_table text;
begin
    select relation.relname
      into unsafe_table
      from pg_class relation
      join pg_namespace namespace on namespace.oid = relation.relnamespace
      join pg_am access_method on access_method.oid = relation.relam
     where namespace.nspname = 'public'
       and relation.relname in (
           'hepta_paper_contribution_ledgers',
           'hepta_paper_contribution_ledger_reservations'
       )
       and (
           relation.relkind <> 'r'
           or relation.relpersistence <> 'p'
           or relation.relispartition
           or relation.relrowsecurity
           or relation.relforcerowsecurity
           or relation.reloftype <> 0
           or relation.relreplident <> 'd'
           or coalesce(relation.reloptions, array[]::text[]) <> array[]::text[]
           or access_method.amname <> 'heap'
           or exists (
               select 1 from pg_policy policy where policy.polrelid = relation.oid
           )
           or exists (
               select 1 from pg_inherits inheritance
                where inheritance.inhrelid = relation.oid
                   or inheritance.inhparent = relation.oid
           )
           or exists (
               select 1 from pg_rewrite rewrite_rule
                where rewrite_rule.ev_class = relation.oid
                  and rewrite_rule.rulename <> '_RETURN'
           )
       )
     limit 1;

    if unsafe_table is not null then
        raise exception using
            errcode = '55000',
            message = 'contribution_authority_table_metadata_requires_operator_review',
            detail = unsafe_table || ' is not a plain permanent policy-free heap table';
    end if;

    if exists (
        select 1
          from pg_class reservation
          join pg_namespace reservation_namespace
            on reservation_namespace.oid = reservation.relnamespace
          join pg_class ledger
            on ledger.relname = 'hepta_paper_contribution_ledgers'
          join pg_namespace ledger_namespace
            on ledger_namespace.oid = ledger.relnamespace
         where reservation_namespace.nspname = 'public'
           and reservation.relname = 'hepta_paper_contribution_ledger_reservations'
           and ledger_namespace.nspname = 'public'
           and (
               reservation.relowner <> ledger.relowner
               or reservation.reltablespace <> ledger.reltablespace
               or ledger.relowner <> (
                   select project.relowner
                     from pg_class project
                     join pg_namespace project_namespace
                       on project_namespace.oid = project.relnamespace
                    where project_namespace.nspname = 'public'
                      and project.relname = 'hepta_paper_projects'
               )
           )
    ) then
        raise exception using
            errcode = '55000',
            message = 'contribution_authority_owner_or_tablespace_requires_operator_review',
            detail = 'Reservation, ledger and Paper authority must retain one owner and one explicit tablespace identity';
    end if;
end
$block$;

-- Repair defaults/nullability only after exact names, order and types are
-- established.  Existing bad rows fail the NOT NULL operations atomically.
alter table hepta_paper_contribution_ledgers
    alter column contribution_ledger_id drop default,
    alter column paper_project_id drop default,
    alter column release_candidate_hash drop default,
    alter column ledger_hash drop default,
    alter column version drop default,
    alter column record_json drop default,
    alter column created_at drop default,
    alter column entries_json drop default;

alter table hepta_paper_contribution_ledger_reservations
    alter column contribution_ledger_id drop default,
    alter column paper_project_id drop default,
    alter column release_candidate_hash drop default,
    alter column created_at drop default;

update hepta_paper_contribution_ledgers
set entries_json = record_json->'entries'
where entries_json is null;

do $block$
begin
    if exists (
        select 1
          from hepta_paper_contribution_ledgers
         where contribution_ledger_id is null
            or paper_project_id is null
            or release_candidate_hash is null
            or ledger_hash is null
            or version is null
            or record_json is null
            or created_at is null
            or entries_json is null
    ) or exists (
        select 1
          from hepta_paper_contribution_ledger_reservations
         where contribution_ledger_id is null
            or paper_project_id is null
            or release_candidate_hash is null
            or created_at is null
    ) then
        raise exception using
            errcode = '23502',
            message = 'contribution_authority_null_row_requires_operator_review';
    end if;
end
$block$;

alter table hepta_paper_contribution_ledgers
    alter column contribution_ledger_id set not null,
    alter column paper_project_id set not null,
    alter column release_candidate_hash set not null,
    alter column ledger_hash set not null,
    alter column version set not null,
    alter column record_json set not null,
    alter column created_at set not null,
    alter column entries_json set not null;

alter table hepta_paper_contribution_ledger_reservations
    alter column contribution_ledger_id set not null,
    alter column paper_project_id set not null,
    alter column release_candidate_hash set not null,
    alter column created_at set not null;

-- One verified artifact is one scientific fact.  Locking the proposal table
-- above makes this preflight and the replacement partial index one decision.
do $block$
begin
    if exists (
        select 1
          from hepta_agent_proposals
         where status = 'accepted'
         group by artifact_manifest_id
        having count(*) > 1
    ) then
        raise exception using
            errcode = '23514',
            message = 'duplicate_accepted_artifact_contribution_requires_operator_review',
            detail = 'One artifact manifest may have only one accepted Agent proposal globally; scientific credit cannot be repaired automatically';
    end if;
end
$block$;

drop index if exists public.hepta_agent_proposals_one_accepted_artifact_manifest_idx;
create unique index hepta_agent_proposals_one_accepted_artifact_manifest_idx
    on hepta_agent_proposals using btree (artifact_manifest_id)
    where status = 'accepted';

-- A pre-0047 release candidate did not retain the caller's ledger ID until
-- the ledger was frozen.  A candidate promoted after 0047 instead owns an
-- exact Paper/release reservation before its ledger is frozen, so migration
-- replay must accept that legitimate live window.  Only a candidate with
-- neither authority record is legacy-ambiguous and cannot be repaired.
do $block$
begin
    if exists (
        select 1
          from hepta_paper_revisions revision
          join hepta_paper_projects paper
            on paper.paper_project_id = revision.paper_project_id
          left join hepta_paper_contribution_ledgers ledger
            on ledger.paper_project_id = revision.paper_project_id
           and ledger.release_candidate_hash = revision.release_candidate_hash
          left join hepta_paper_contribution_ledger_reservations reservation
            on reservation.paper_project_id = revision.paper_project_id
           and reservation.release_candidate_hash = revision.release_candidate_hash
         where revision.status = 'release_candidate'
           and coalesce(paper.record_json->>'outcome', 'in_progress')
               not in ('failed', 'expired', 'abandoned')
           and ledger.contribution_ledger_id is null
           and reservation.contribution_ledger_id is null
    ) then
        raise exception using
            errcode = '23514',
            message = 'legacy_release_candidate_missing_contribution_ledger',
            detail = 'Pre-0047 promoted candidates must have a frozen contribution ledger or an exact 0047 reservation, or be explicitly retired before upgrade; the missing global ledger ID cannot be invented';
    end if;
end
$block$;

-- All constraints on these two tables are part of the managed scientific
-- authority catalog.  Rebuild them under explicit stable names.  An external
-- dependency causes DROP to fail closed rather than cascading away evidence.
do $block$
declare
    managed_constraint record;
begin
    for managed_constraint in
        select constraint_row.conname
          from pg_constraint constraint_row
         where constraint_row.conrelid = 'hepta_paper_contribution_ledgers'::regclass
         order by constraint_row.conname
    loop
        execute format(
            'alter table hepta_paper_contribution_ledgers drop constraint %I',
            managed_constraint.conname
        );
    end loop;

    for managed_constraint in
        select constraint_row.conname
          from pg_constraint constraint_row
         where constraint_row.conrelid = 'hepta_paper_contribution_ledger_reservations'::regclass
         order by constraint_row.conname
    loop
        execute format(
            'alter table hepta_paper_contribution_ledger_reservations drop constraint %I',
            managed_constraint.conname
        );
    end loop;
end
$block$;

-- Validate pre-existing reservations before using them as ownership facts.
do $block$
begin
    if exists (
        select 1
          from hepta_paper_contribution_ledger_reservations reservation
         where reservation.contribution_ledger_id = '00000000-0000-0000-0000-000000000000'::uuid
            or reservation.release_candidate_hash !~ '^sha256:[0-9a-f]{64}$'
            or not exists (
                select 1 from hepta_paper_projects paper
                 where paper.paper_project_id = reservation.paper_project_id
            )
    ) or exists (
        select 1
          from hepta_paper_contribution_ledger_reservations
         group by contribution_ledger_id
        having count(*) > 1
    ) then
        raise exception using
            errcode = '23514',
            message = 'invalid_contribution_ledger_reservation_requires_operator_review';
    end if;

    if exists (
        select 1
          from hepta_paper_contribution_ledgers ledger
          join hepta_paper_contribution_ledger_reservations reservation
            on reservation.contribution_ledger_id = ledger.contribution_ledger_id
         where reservation.paper_project_id <> ledger.paper_project_id
            or reservation.release_candidate_hash <> ledger.release_candidate_hash
    ) then
        raise exception using
            errcode = '23514',
            message = 'contribution_ledger_reservation_backfill_mismatch',
            detail = 'Existing frozen ledger ownership conflicts with the global contribution-ledger ID reservation catalog';
    end if;
end
$block$;

insert into hepta_paper_contribution_ledger_reservations (
    contribution_ledger_id, paper_project_id, release_candidate_hash, created_at
)
select ledger.contribution_ledger_id,
       ledger.paper_project_id,
       ledger.release_candidate_hash,
       ledger.created_at
  from hepta_paper_contribution_ledgers ledger
 where not exists (
     select 1
       from hepta_paper_contribution_ledger_reservations reservation
      where reservation.contribution_ledger_id = ledger.contribution_ledger_id
 );

do $block$
begin
    if exists (
        select 1
          from hepta_paper_contribution_ledgers ledger
          left join hepta_paper_contribution_ledger_reservations reservation
            on reservation.contribution_ledger_id = ledger.contribution_ledger_id
           and reservation.paper_project_id = ledger.paper_project_id
           and reservation.release_candidate_hash = ledger.release_candidate_hash
         where reservation.contribution_ledger_id is null
    ) then
        raise exception using
            errcode = '23514',
            message = 'contribution_ledger_reservation_backfill_mismatch',
            detail = 'Every frozen ledger must retain its exact global reservation triple';
    end if;
end
$block$;

alter table hepta_paper_contribution_ledger_reservations
    add constraint hepta_paper_contribution_ledger_reservations_pkey
        primary key (contribution_ledger_id),
    add constraint hepta_contribution_ledger_reservations_ownership_key
        unique (contribution_ledger_id, paper_project_id, release_candidate_hash),
    add constraint hepta_contribution_ledger_reservations_non_nil_id_check check (
        contribution_ledger_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ) not valid,
    add constraint hepta_contribution_ledger_reservations_release_hash_check check (
        release_candidate_hash ~ '^sha256:[0-9a-f]{64}$'
    ) not valid,
    add constraint hepta_contribution_ledger_reservations_paper_fkey
        foreign key (paper_project_id)
        references hepta_paper_projects(paper_project_id)
        on update no action on delete no action
        not valid;

alter table hepta_paper_contribution_ledgers
    add constraint hepta_paper_contribution_ledgers_pkey
        primary key (contribution_ledger_id),
    add constraint hepta_paper_contribution_ledgers_paper_release_key
        unique (paper_project_id, release_candidate_hash),
    add constraint hepta_paper_contribution_ledgers_paper_hash_key
        unique (paper_project_id, ledger_hash),
    add constraint hepta_paper_contribution_ledgers_paper_fkey
        foreign key (paper_project_id)
        references hepta_paper_projects(paper_project_id)
        on update no action on delete cascade
        not valid,
    add constraint hepta_paper_contribution_ledgers_version_check check (
        version > 0
    ) not valid,
    add constraint hepta_paper_contribution_ledgers_non_nil_id_check check (
        contribution_ledger_id <> '00000000-0000-0000-0000-000000000000'::uuid
    ) not valid,
    add constraint hepta_paper_contribution_ledgers_release_hash_check check (
        release_candidate_hash ~ '^sha256:[0-9a-f]{64}$'
    ) not valid,
    add constraint hepta_paper_contribution_ledgers_ledger_hash_check check (
        ledger_hash ~ '^sha256:[0-9a-f]{64}$'
    ) not valid,
    add constraint hepta_paper_contribution_ledgers_entries_json_check check (
        jsonb_typeof(entries_json) is not distinct from 'array'
    ) not valid,
    add constraint hepta_paper_contribution_ledgers_record_json_parity_check check (
        (record_json->>'schema') is not distinct from 'hepta.paper_raid.contribution_ledger.v1'
        and (record_json->>'contribution_ledger_id') is not distinct from contribution_ledger_id::text
        and (record_json->>'paper_project_id') is not distinct from paper_project_id::text
        and (record_json->>'release_candidate_hash') is not distinct from release_candidate_hash
        and (record_json->'entries') is not distinct from entries_json
        and (record_json->>'ledger_hash') is not distinct from ledger_hash
        and (record_json->>'version') is not distinct from version::text
        and (record_json->>'created_at')::timestamptz is not distinct from created_at
    ) not valid,
    add constraint hepta_paper_contribution_ledgers_reservation_fkey
        foreign key (contribution_ledger_id, paper_project_id, release_candidate_hash)
        references hepta_paper_contribution_ledger_reservations(
            contribution_ledger_id, paper_project_id, release_candidate_hash
        )
        on update no action on delete no action
        not valid;

alter table hepta_paper_contribution_ledger_reservations
    validate constraint hepta_contribution_ledger_reservations_non_nil_id_check;
alter table hepta_paper_contribution_ledger_reservations
    validate constraint hepta_contribution_ledger_reservations_release_hash_check;
alter table hepta_paper_contribution_ledger_reservations
    validate constraint hepta_contribution_ledger_reservations_paper_fkey;

alter table hepta_paper_contribution_ledgers
    validate constraint hepta_paper_contribution_ledgers_paper_fkey;
alter table hepta_paper_contribution_ledgers
    validate constraint hepta_paper_contribution_ledgers_version_check;
alter table hepta_paper_contribution_ledgers
    validate constraint hepta_paper_contribution_ledgers_non_nil_id_check;
alter table hepta_paper_contribution_ledgers
    validate constraint hepta_paper_contribution_ledgers_release_hash_check;
alter table hepta_paper_contribution_ledgers
    validate constraint hepta_paper_contribution_ledgers_ledger_hash_check;
alter table hepta_paper_contribution_ledgers
    validate constraint hepta_paper_contribution_ledgers_entries_json_check;
alter table hepta_paper_contribution_ledgers
    validate constraint hepta_paper_contribution_ledgers_record_json_parity_check;
alter table hepta_paper_contribution_ledgers
    validate constraint hepta_paper_contribution_ledgers_reservation_fkey;

-- No other user trigger is permitted on the two immutable authority tables.
-- Known 0047 guards are recreated below; an unknown trigger needs explicit
-- operator review because it could change INSERT semantics.
do $block$
begin
    if exists (
        select 1
          from pg_trigger trigger_row
         where trigger_row.tgrelid in (
             'hepta_paper_contribution_ledgers'::regclass,
             'hepta_paper_contribution_ledger_reservations'::regclass
         )
           and not trigger_row.tgisinternal
           and trigger_row.tgname not in (
               'hepta_contribution_ledger_reservation_immutable_guard',
               'hepta_contribution_ledger_reservation_truncate_guard',
               'hepta_paper_contribution_ledger_immutable_guard',
               'hepta_paper_contribution_ledger_truncate_guard'
           )
    ) then
        raise exception using
            errcode = '55000',
            message = 'unexpected_contribution_authority_trigger_requires_operator_review';
    end if;

    if exists (
        select 1
          from pg_proc function_row
          join pg_namespace namespace on namespace.oid = function_row.pronamespace
         where namespace.nspname = 'public'
           and function_row.proname = 'hepta_reject_frozen_contribution_authority_mutation'
           and function_row.pronargs <> 0
    ) then
        raise exception using
            errcode = '55000',
            message = 'unexpected_contribution_authority_guard_overload_requires_operator_review';
    end if;
end
$block$;

create or replace function hepta_reject_frozen_contribution_authority_mutation()
returns trigger
language plpgsql
volatile
parallel unsafe
security invoker
as $$
begin
    raise exception using
        errcode = '55000',
        message = 'frozen_contribution_authority_is_immutable',
        detail = tg_table_name || ' rows are append-only scientific authority';
end
$$;

alter function hepta_reject_frozen_contribution_authority_mutation() reset all;

do $block$
declare
    authority_owner text;
begin
    select owner_role.rolname
      into strict authority_owner
      from pg_class ledger
      join pg_namespace namespace on namespace.oid = ledger.relnamespace
      join pg_roles owner_role on owner_role.oid = ledger.relowner
     where namespace.nspname = 'public'
       and ledger.relname = 'hepta_paper_contribution_ledgers';
    execute format(
        'alter function hepta_reject_frozen_contribution_authority_mutation() owner to %I',
        authority_owner
    );
end
$block$;

drop trigger if exists hepta_contribution_ledger_reservation_immutable_guard
    on hepta_paper_contribution_ledger_reservations;
drop trigger if exists hepta_contribution_ledger_reservation_truncate_guard
    on hepta_paper_contribution_ledger_reservations;
drop trigger if exists hepta_paper_contribution_ledger_immutable_guard
    on hepta_paper_contribution_ledgers;
drop trigger if exists hepta_paper_contribution_ledger_truncate_guard
    on hepta_paper_contribution_ledgers;

create trigger hepta_contribution_ledger_reservation_immutable_guard
before update or delete on hepta_paper_contribution_ledger_reservations
for each row execute function hepta_reject_frozen_contribution_authority_mutation();
create trigger hepta_contribution_ledger_reservation_truncate_guard
before truncate on hepta_paper_contribution_ledger_reservations
for each statement execute function hepta_reject_frozen_contribution_authority_mutation();
create trigger hepta_paper_contribution_ledger_immutable_guard
before update or delete on hepta_paper_contribution_ledgers
for each row execute function hepta_reject_frozen_contribution_authority_mutation();
create trigger hepta_paper_contribution_ledger_truncate_guard
before truncate on hepta_paper_contribution_ledgers
for each statement execute function hepta_reject_frozen_contribution_authority_mutation();

alter table hepta_paper_contribution_ledger_reservations
    enable always trigger hepta_contribution_ledger_reservation_immutable_guard;
alter table hepta_paper_contribution_ledger_reservations
    enable always trigger hepta_contribution_ledger_reservation_truncate_guard;
alter table hepta_paper_contribution_ledgers
    enable always trigger hepta_paper_contribution_ledger_immutable_guard;
alter table hepta_paper_contribution_ledgers
    enable always trigger hepta_paper_contribution_ledger_truncate_guard;

commit;
