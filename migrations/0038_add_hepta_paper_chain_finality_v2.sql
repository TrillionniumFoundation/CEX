-- Versioned preparation boundary for Chain App-v6 Paper Raid finality.
--
-- A preparation is deliberately not a queued/signed Chain command. It freezes
-- the complete locally verified scientific-finality tuple while the canonical
-- Chain Receipt verifier is upgraded to understand the independent Paper V2
-- transaction lane. Status cannot be promoted by this migration.
--
-- The irreversible source seal is anchored on the pre-existing Paper row.
-- This is intentional: a REPEATABLE READ transaction opened before this
-- migration can miss a newly inserted seal row, but it cannot lock an older
-- version of a Paper row after that row has been updated by preparation.

-- Composite identities used below make it impossible to assemble an otherwise
-- valid preparation from records that belong to different Papers or
-- submissions.
do $block$
begin
    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_trnm_cometbft_trust_anchors'::regclass
          and conname = 'hepta_trnm_trust_anchor_chain_unique'
    ) then
        alter table hepta_trnm_cometbft_trust_anchors
            add constraint hepta_trnm_trust_anchor_chain_unique
            unique (anchor_hash, chain_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_joint_paper_submissions'::regclass
          and conname = 'hepta_joint_submissions_id_paper_unique'
    ) then
        alter table hepta_joint_paper_submissions
            add constraint hepta_joint_submissions_id_paper_unique
            unique (submission_id, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_evaluations'::regclass
          and conname = 'hepta_paper_evaluations_id_submission_paper_unique'
    ) then
        alter table hepta_paper_evaluations
            add constraint hepta_paper_evaluations_id_submission_paper_unique
            unique (evaluation_id, submission_id, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_evaluations'::regclass
          and conname = 'hepta_paper_evaluations_submission_paper_fkey'
    ) then
        alter table hepta_paper_evaluations
            add constraint hepta_paper_evaluations_submission_paper_fkey
            foreign key (submission_id, paper_project_id)
            references hepta_joint_paper_submissions(submission_id, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_evaluations'::regclass
          and conname = 'hepta_paper_evaluations_supersedes_same_submission_fkey'
    ) then
        alter table hepta_paper_evaluations
            add constraint hepta_paper_evaluations_supersedes_same_submission_fkey
            foreign key (supersedes_evaluation_id, submission_id, paper_project_id)
            references hepta_paper_evaluations(
                evaluation_id, submission_id, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_reproductions'::regclass
          and conname = 'hepta_paper_reproductions_id_evaluation_paper_unique'
    ) then
        alter table hepta_paper_reproductions
            add constraint hepta_paper_reproductions_id_evaluation_paper_unique
            unique (reproduction_id, evaluation_id, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_reproductions'::regclass
          and conname = 'hepta_paper_reproductions_supersedes_same_evaluation_fkey'
    ) then
        alter table hepta_paper_reproductions
            add constraint hepta_paper_reproductions_supersedes_same_evaluation_fkey
            foreign key (supersedes_reproduction_id, evaluation_id, paper_project_id)
            references hepta_paper_reproductions(
                reproduction_id, evaluation_id, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_research_session_authorization_sets'::regclass
          and conname = 'hepta_research_auth_set_session_roster_paper_unique'
    ) then
        alter table hepta_research_session_authorization_sets
            add constraint hepta_research_auth_set_session_roster_paper_unique
            unique (session_id, roster_version, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_research_session_authorization_sets'::regclass
          and conname = 'hepta_research_auth_set_identity_epoch_paper_unique'
    ) then
        alter table hepta_research_session_authorization_sets
            add constraint hepta_research_auth_set_identity_epoch_paper_unique
            unique (
                authorization_set_id, session_id, roster_version, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_nakama_research_session_completions'::regclass
          and conname = 'hepta_nakama_completion_match_evidence_paper_unique'
    ) then
        alter table hepta_nakama_research_session_completions
            add constraint hepta_nakama_completion_match_evidence_paper_unique
            unique (commitment_id, session_id, roster_version, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_nakama_research_session_completions'::regclass
          and conname = 'hepta_nakama_completion_authorization_epoch_fkey'
    ) then
        alter table hepta_nakama_research_session_completions
            add constraint hepta_nakama_completion_authorization_epoch_fkey
            foreign key (session_id, roster_version, paper_project_id)
            references hepta_research_session_authorization_sets(
                session_id, roster_version, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_nakama_research_session_completions'::regclass
          and conname = 'hepta_nakama_completion_authorization_identity_fkey'
    ) then
        alter table hepta_nakama_research_session_completions
            add constraint hepta_nakama_completion_authorization_identity_fkey
            foreign key (
                authorization_set_id, session_id, roster_version, paper_project_id
            ) references hepta_research_session_authorization_sets(
                authorization_set_id, session_id, roster_version, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_appeals'::regclass
          and conname = 'hepta_paper_appeals_id_evaluation_paper_unique'
    ) then
        alter table hepta_paper_appeals
            add constraint hepta_paper_appeals_id_evaluation_paper_unique
            unique (appeal_id, evaluation_id, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_appeals'::regclass
          and conname = 'hepta_paper_appeals_id_paper_unique'
    ) then
        alter table hepta_paper_appeals
            add constraint hepta_paper_appeals_id_paper_unique
            unique (appeal_id, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_appeal_resolutions'::regclass
          and conname = 'hepta_paper_resolutions_id_appeal_paper_unique'
    ) then
        alter table hepta_paper_appeal_resolutions
            add constraint hepta_paper_resolutions_id_appeal_paper_unique
            unique (resolution_id, appeal_id, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_appeal_resolutions'::regclass
          and conname = 'hepta_paper_resolutions_superseding_evaluation_paper_fkey'
    ) then
        alter table hepta_paper_appeal_resolutions
            add constraint hepta_paper_resolutions_superseding_evaluation_paper_fkey
            foreign key (superseding_evaluation_id, paper_project_id)
            references hepta_paper_evaluations(evaluation_id, paper_project_id);
    end if;
end;
$block$;

-- Constraint names alone are not an integrity boundary: an owner can replace
-- a CHECK/FK/UNIQUE with a weaker constraint under the same name.  Keep one
-- compact fingerprint over every V2 evidence-table constraint plus every
-- composite source/anchor constraint introduced by this migration.  Both the
-- replay itself and the resident/release catalog gates compare the result with
-- the audited PostgreSQL 17 canonical catalog below.
create or replace function public.hepta_paper_finality_v2_constraint_catalog_fingerprint()
returns table(constraint_count bigint, catalog_sha256 text)
language sql
stable
set search_path = pg_catalog
as $function$
    with external_constraint(relation_id, constraint_name) as (values
        (pg_catalog.to_regclass('public.hepta_trnm_cometbft_trust_anchors'),
         'hepta_trnm_trust_anchor_chain_unique'),
        (pg_catalog.to_regclass('public.hepta_joint_paper_submissions'),
         'hepta_joint_submissions_id_paper_unique'),
        (pg_catalog.to_regclass('public.hepta_paper_evaluations'),
         'hepta_paper_evaluations_id_submission_paper_unique'),
        (pg_catalog.to_regclass('public.hepta_paper_evaluations'),
         'hepta_paper_evaluations_submission_paper_fkey'),
        (pg_catalog.to_regclass('public.hepta_paper_evaluations'),
         'hepta_paper_evaluations_supersedes_same_submission_fkey'),
        (pg_catalog.to_regclass('public.hepta_paper_reproductions'),
         'hepta_paper_reproductions_id_evaluation_paper_unique'),
        (pg_catalog.to_regclass('public.hepta_paper_reproductions'),
         'hepta_paper_reproductions_supersedes_same_evaluation_fkey'),
        (pg_catalog.to_regclass('public.hepta_research_session_authorization_sets'),
         'hepta_research_auth_set_session_roster_paper_unique'),
        (pg_catalog.to_regclass('public.hepta_research_session_authorization_sets'),
         'hepta_research_auth_set_identity_epoch_paper_unique'),
        (pg_catalog.to_regclass('public.hepta_nakama_research_session_completions'),
         'hepta_nakama_completion_match_evidence_paper_unique'),
        (pg_catalog.to_regclass('public.hepta_nakama_research_session_completions'),
         'hepta_nakama_completion_authorization_epoch_fkey'),
        (pg_catalog.to_regclass('public.hepta_nakama_research_session_completions'),
         'hepta_nakama_completion_authorization_identity_fkey'),
        (pg_catalog.to_regclass('public.hepta_paper_appeals'),
         'hepta_paper_appeals_id_evaluation_paper_unique'),
        (pg_catalog.to_regclass('public.hepta_paper_appeals'),
         'hepta_paper_appeals_id_paper_unique'),
        (pg_catalog.to_regclass('public.hepta_paper_appeal_resolutions'),
         'hepta_paper_resolutions_id_appeal_paper_unique'),
        (pg_catalog.to_regclass('public.hepta_paper_appeal_resolutions'),
         'hepta_paper_resolutions_superseding_evaluation_paper_fkey'),
        (pg_catalog.to_regclass('public.hepta_paper_projects'),
         'hepta_paper_projects_finality_v2_seal_coherent'),
        (pg_catalog.to_regclass('public.hepta_paper_projects'),
         'hepta_paper_projects_finality_v2_seal_preparation_fkey')
    ), managed_constraint as (
        select
            namespace.nspname as schema_name,
            relation.relname as relation_name,
            constraint_row.oid,
            constraint_row.conname,
            constraint_row.contype,
            constraint_row.convalidated,
            constraint_row.condeferrable,
            constraint_row.condeferred
        from pg_catalog.pg_constraint as constraint_row
        join pg_catalog.pg_class as relation
          on relation.oid = constraint_row.conrelid
        join pg_catalog.pg_namespace as namespace
          on namespace.oid = relation.relnamespace
        where constraint_row.conrelid in (
            pg_catalog.to_regclass(
                'public.hepta_trnm_cometbft_time_checkpoints_v1'
            ),
            pg_catalog.to_regclass(
                'public.hepta_paper_chain_finality_window_arms_v2'
            ),
            pg_catalog.to_regclass(
                'public.hepta_paper_chain_finality_preparations_v2'
            )
        ) or (constraint_row.conrelid, constraint_row.conname) in (
            select relation_id, constraint_name from external_constraint
        )
    ), canonical_line as (
        select pg_catalog.format(
            '%s.%s|%s|%s|%s|%s|%s|%s',
            schema_name,
            relation_name,
            conname,
            contype,
            convalidated,
            condeferrable,
            condeferred,
            pg_catalog.replace(
                pg_catalog.pg_get_constraintdef(oid, false),
                'public.',
                ''
            )
        ) as line
        from managed_constraint
    )
    select
        count(*)::bigint,
        pg_catalog.encode(
            pg_catalog.sha256(pg_catalog.convert_to(
                pg_catalog.string_agg(line, E'\n' order by line),
                'UTF8'
            )),
            'hex'
        )
    from canonical_line;
$function$;

revoke all on function
    public.hepta_paper_finality_v2_constraint_catalog_fingerprint()
    from public;

-- A time checkpoint is a dynamically verified CometBFT light-finality proof,
-- not a static trust anchor.  The anchor remains provenance for verification;
-- business deadlines bind the consensus time of the verified checkpoint.
create table if not exists hepta_trnm_cometbft_time_checkpoints_v1 (
    checkpoint_hash text primary key,
    trust_anchor_hash text not null,
    chain_id text not null,
    height bigint not null check (height > 0),
    header_hash text not null,
    consensus_time_unix_ms bigint not null check (consensus_time_unix_ms >= 0),
    canonical_proof jsonb not null,
    canonical_proof_sha256 text not null unique,
    locally_verified_at_unix_ms bigint not null check (
        locally_verified_at_unix_ms >= 0
    ),
    record_json jsonb not null,
    constraint hepta_trnm_time_checkpoint_anchor_chain_fkey
        foreign key (trust_anchor_hash, chain_id)
        references hepta_trnm_cometbft_trust_anchors(anchor_hash, chain_id),
    constraint hepta_trnm_time_checkpoint_chain_height_unique
        unique (chain_id, height),
    constraint hepta_trnm_time_checkpoint_identity_unique
        unique (
            checkpoint_hash,
            trust_anchor_hash,
            chain_id,
            height,
            header_hash,
            consensus_time_unix_ms
        ),
    constraint hepta_trnm_time_checkpoint_canonical_values_check check (
        checkpoint_hash ~ '^sha256:[0-9a-f]{64}$'
        and trust_anchor_hash ~ '^[0-9a-f]{64}$'
        and length(chain_id) between 1 and 128
        and header_hash ~ '^[0-9a-f]{64}$'
        and canonical_proof_sha256 ~ '^sha256:[0-9a-f]{64}$'
        and consensus_time_unix_ms <= locally_verified_at_unix_ms
        and jsonb_typeof(canonical_proof) = 'object'
        and coalesce((
            jsonb_typeof(record_json) = 'object'
            and record_json ->> 'schema'
                = 'hepta.paper_raid.trnm_chain_time_checkpoint.v1'
            and record_json ->> 'checkpoint_hash' = checkpoint_hash
            and record_json ->> 'trust_anchor_hash' = trust_anchor_hash
            and record_json ->> 'chain_id' = chain_id
            and record_json ->> 'height' = height::text
            and record_json ->> 'header_hash' = header_hash
            and record_json ->> 'consensus_time_unix_ms'
                = consensus_time_unix_ms::text
            and record_json ->> 'canonical_proof_sha256'
                = canonical_proof_sha256
            and record_json ->> 'locally_verified_at_unix_ms'
                = locally_verified_at_unix_ms::text
        ), false)
    )
);

create index if not exists hepta_trnm_time_checkpoints_local_high_water_idx
    on hepta_trnm_cometbft_time_checkpoints_v1
       (locally_verified_at_unix_ms desc, checkpoint_hash);

-- Arming observes one exact scientific-finality source fingerprint at a
-- fresh, consensus-authenticated Chain time.  It is immutable evidence, but
-- deliberately does not seal the Paper source tuple; final preparation does.
create table if not exists hepta_paper_chain_finality_window_arms_v2 (
    arm_id uuid primary key,
    paper_project_id uuid not null
        references hepta_paper_projects(paper_project_id),
    submission_id uuid not null,
    evaluation_id uuid not null,
    latest_reproduction_id uuid not null,
    research_session_id text not null,
    research_session_roster_version bigint not null check (
        research_session_roster_version > 0
    ),
    source_fingerprint text not null unique,
    appeal_status text not null check (
        appeal_status in ('closed_no_appeal', 'resolved_denied', 'resolved_upheld')
    ),
    appeal_id uuid,
    appeal_resolution_id uuid,
    start_checkpoint_hash text not null,
    start_anchor_hash text not null,
    start_chain_id text not null,
    start_height bigint not null check (start_height > 0),
    start_header_hash text not null,
    start_consensus_time_unix_ms bigint not null check (
        start_consensus_time_unix_ms >= 0
    ),
    observed_max_checkpoint_height bigint not null check (
        observed_max_checkpoint_height > 0
    ),
    max_chain_time_lag_ms bigint not null check (max_chain_time_lag_ms > 0),
    earliest_final_checkpoint_time_unix_ms bigint not null check (
        earliest_final_checkpoint_time_unix_ms >= 0
    ),
    idempotency_key text not null unique,
    request_hash text not null,
    record_json jsonb not null
);

create table if not exists hepta_paper_chain_finality_preparations_v2 (
    preparation_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id),
    submission_id uuid not null,
    evaluation_id uuid not null,
    latest_reproduction_id uuid not null,
    arm_id uuid not null,
    source_fingerprint text not null,
    final_checkpoint_hash text not null,
    final_anchor_hash text not null,
    final_chain_id text not null,
    final_height bigint not null check (final_height > 0),
    final_header_hash text not null,
    final_consensus_time_unix_ms bigint not null check (
        final_consensus_time_unix_ms >= 0
    ),
    research_session_id text not null,
    research_session_roster_version bigint not null check (
        research_session_roster_version > 0
    ),
    match_evidence_commitment_id text not null,
    appeal_status text not null check (
        appeal_status in ('closed_no_appeal', 'resolved_denied', 'resolved_upheld')
    ),
    appeal_id uuid,
    appealed_evaluation_id uuid,
    appeal_resolution_id uuid,
    commitment_id text not null unique,
    binding_fingerprint text not null unique,
    idempotency_key text not null unique,
    request_hash text not null,
    status text not null check (status = 'awaiting_chain_verifier_upgrade'),
    scientific_finality boolean not null check (scientific_finality),
    score_eligible boolean not null check (not score_eligible),
    ranking_eligible boolean not null check (not ranking_eligible),
    reward_eligible boolean not null check (not reward_eligible),
    economic_eligible boolean not null check (not economic_eligible),
    record_json jsonb not null,
    created_at timestamptz not null
);

-- Trigger repair has to precede the early-table backfill below.  In
-- particular, replaying this migration against its own ENABLE ALWAYS
-- immutability trigger must not make the canonical-column backfill impossible.
drop trigger if exists hepta_trnm_time_checkpoint_v1_immutable_guard
    on hepta_trnm_cometbft_time_checkpoints_v1;
drop trigger if exists hepta_trnm_time_checkpoint_v1_progress_guard
    on hepta_trnm_cometbft_time_checkpoints_v1;
drop trigger if exists hepta_trnm_time_checkpoint_v1_truncate_guard
    on hepta_trnm_cometbft_time_checkpoints_v1;
drop trigger if exists hepta_paper_finality_v2_window_arm_guard
    on hepta_paper_chain_finality_window_arms_v2;
drop trigger if exists hepta_paper_finality_v2_window_arm_immutable_guard
    on hepta_paper_chain_finality_window_arms_v2;
drop trigger if exists hepta_paper_finality_v2_window_arm_truncate_guard
    on hepta_paper_chain_finality_window_arms_v2;
drop trigger if exists hepta_paper_finality_v2_preparation_guard
    on hepta_paper_chain_finality_preparations_v2;
drop trigger if exists hepta_paper_finality_v2_preparation_seal_guard
    on hepta_paper_chain_finality_preparations_v2;
drop trigger if exists hepta_paper_finality_v2_preparation_immutable_guard
    on hepta_paper_chain_finality_preparations_v2;
drop trigger if exists hepta_paper_finality_v2_preparation_truncate_guard
    on hepta_paper_chain_finality_preparations_v2;
drop trigger if exists hepta_paper_projects_finality_v2_source_guard
    on hepta_paper_projects;
drop trigger if exists hepta_joint_submissions_finality_v2_source_guard
    on hepta_joint_paper_submissions;
drop trigger if exists hepta_paper_evaluations_finality_v2_source_guard
    on hepta_paper_evaluations;
drop trigger if exists hepta_paper_reproductions_finality_v2_source_guard
    on hepta_paper_reproductions;
drop trigger if exists hepta_paper_appeals_finality_v2_source_guard
    on hepta_paper_appeals;
drop trigger if exists hepta_paper_resolutions_finality_v2_source_guard
    on hepta_paper_appeal_resolutions;
drop trigger if exists hepta_research_auth_sets_finality_v2_source_guard
    on hepta_research_session_authorization_sets;
drop trigger if exists hepta_nakama_completions_finality_v2_source_guard
    on hepta_nakama_research_session_completions;

do $block$
declare
    guarded_table regclass;
    truncate_trigger_name text;
begin
    for guarded_table, truncate_trigger_name in
        select guarded.guarded_table, guarded.truncate_trigger_name
        from (values
            ('hepta_paper_projects'::regclass,
             'hepta_paper_projects_finality_v2_truncate_guard'),
            ('hepta_joint_paper_submissions'::regclass,
             'hepta_joint_submissions_finality_v2_truncate_guard'),
            ('hepta_paper_evaluations'::regclass,
             'hepta_paper_evaluations_finality_v2_truncate_guard'),
            ('hepta_paper_reproductions'::regclass,
             'hepta_paper_reproductions_finality_v2_truncate_guard'),
            ('hepta_paper_appeals'::regclass,
             'hepta_paper_appeals_finality_v2_truncate_guard'),
            ('hepta_paper_appeal_resolutions'::regclass,
             'hepta_paper_resolutions_finality_v2_truncate_guard'),
            ('hepta_research_session_authorization_sets'::regclass,
             'hepta_research_auth_sets_finality_v2_truncate_guard'),
            ('hepta_nakama_research_session_completions'::regclass,
             'hepta_nakama_completions_finality_v2_truncate_guard')
        ) as guarded(guarded_table, truncate_trigger_name)
    loop
        execute format(
            'drop trigger if exists %I on %s',
            truncate_trigger_name,
            guarded_table
        );
    end loop;
end;
$block$;

-- Forward-upgrade an early development copy of 0038.  Missing values are
-- recovered from its immutable record; malformed or incomplete old rows fail
-- the subsequent NOT NULL/CHECK validation instead of being guessed.
-- In particular, a static trust-anchor hash is never synthesized into a
-- dynamic checkpoint hash: a truly pre-arm preparation must be discarded in
-- its development database and regenerated through the two-stage protocol.
alter table hepta_paper_chain_finality_preparations_v2
    drop constraint if exists hepta_paper_finality_v2_canonical_values_check,
    drop constraint if exists hepta_paper_finality_v2_record_json_check;

alter table hepta_paper_chain_finality_preparations_v2
    add column if not exists latest_reproduction_id uuid,
    add column if not exists arm_id uuid,
    add column if not exists source_fingerprint text,
    add column if not exists final_checkpoint_hash text,
    add column if not exists final_anchor_hash text,
    add column if not exists final_chain_id text,
    add column if not exists final_height bigint,
    add column if not exists final_header_hash text,
    add column if not exists final_consensus_time_unix_ms bigint,
    add column if not exists research_session_id text,
    add column if not exists research_session_roster_version bigint,
    add column if not exists match_evidence_commitment_id text,
    add column if not exists appeal_status text,
    add column if not exists appeal_id uuid,
    add column if not exists appealed_evaluation_id uuid,
    add column if not exists appeal_resolution_id uuid,
    add column if not exists request_hash text,
    add column if not exists scientific_finality boolean,
    add column if not exists score_eligible boolean,
    add column if not exists ranking_eligible boolean,
    add column if not exists reward_eligible boolean,
    add column if not exists economic_eligible boolean;

update hepta_paper_chain_finality_preparations_v2
set latest_reproduction_id = coalesce(
        latest_reproduction_id,
        nullif(record_json #>> '{binding,latest_reproduction_id}', '')::uuid
    ),
    arm_id = coalesce(
        arm_id,
        nullif(record_json #>> '{binding,window_arm_id}', '')::uuid
    ),
    source_fingerprint = coalesce(
        source_fingerprint,
        record_json #>> '{binding,source_fingerprint}'
    ),
    final_checkpoint_hash = coalesce(
        final_checkpoint_hash,
        record_json #>> '{binding,final_checkpoint_hash}'
    ),
    final_anchor_hash = coalesce(
        final_anchor_hash,
        record_json #>> '{binding,final_checkpoint_anchor_hash}'
    ),
    final_chain_id = coalesce(
        final_chain_id,
        record_json #>> '{binding,final_checkpoint_chain_id}',
        record_json #>> '{binding,start_checkpoint_chain_id}'
    ),
    final_height = coalesce(
        final_height,
        nullif(record_json #>> '{binding,final_checkpoint_height}', '')::bigint
    ),
    final_header_hash = coalesce(
        final_header_hash,
        record_json #>> '{binding,final_checkpoint_header_hash}'
    ),
    final_consensus_time_unix_ms = coalesce(
        final_consensus_time_unix_ms,
        nullif(
            record_json #>> '{binding,final_checkpoint_consensus_time_unix_ms}',
            ''
        )::bigint
    ),
    research_session_id = coalesce(
        research_session_id,
        record_json #>> '{binding,research_session_id}'
    ),
    research_session_roster_version = coalesce(
        research_session_roster_version,
        nullif(record_json #>> '{binding,research_session_roster_version}', '')::bigint
    ),
    match_evidence_commitment_id = coalesce(
        match_evidence_commitment_id,
        record_json #>> '{binding,match_evidence_commitment_id}'
    ),
    appeal_status = coalesce(
        appeal_status,
        record_json #>> '{binding,appeal_status}'
    ),
    appeal_id = coalesce(
        appeal_id,
        nullif(record_json #>> '{binding,appeal_id}', '')::uuid
    ),
    appealed_evaluation_id = coalesce(
        appealed_evaluation_id,
        nullif(record_json #>> '{binding,appealed_evaluation_id}', '')::uuid
    ),
    appeal_resolution_id = coalesce(
        appeal_resolution_id,
        nullif(record_json #>> '{binding,appeal_resolution_id}', '')::uuid
    ),
    request_hash = coalesce(request_hash, record_json ->> 'request_hash'),
    scientific_finality = coalesce(
        scientific_finality,
        nullif(record_json #>> '{binding,scientific_finality}', '')::boolean
    ),
    score_eligible = coalesce(
        score_eligible,
        nullif(record_json #>> '{binding,score_eligible}', '')::boolean
    ),
    ranking_eligible = coalesce(
        ranking_eligible,
        nullif(record_json #>> '{binding,ranking_eligible}', '')::boolean
    ),
    reward_eligible = coalesce(
        reward_eligible,
        nullif(record_json #>> '{binding,reward_eligible}', '')::boolean
    ),
    economic_eligible = coalesce(
        economic_eligible,
        nullif(record_json #>> '{binding,economic_eligible}', '')::boolean
    )
where latest_reproduction_id is null
   or arm_id is null
   or source_fingerprint is null
   or final_checkpoint_hash is null
   or final_anchor_hash is null
   or final_chain_id is null
   or final_height is null
   or final_header_hash is null
   or final_consensus_time_unix_ms is null
   or research_session_id is null
   or research_session_roster_version is null
   or match_evidence_commitment_id is null
   or appeal_status is null
   or (
        appeal_id is null
        and record_json #>> '{binding,appeal_id}' is not null
   )
   or (
        appealed_evaluation_id is null
        and record_json #>> '{binding,appealed_evaluation_id}' is not null
   )
   or (
        appeal_resolution_id is null
        and record_json #>> '{binding,appeal_resolution_id}' is not null
   )
   or request_hash is null
   or scientific_finality is null
   or score_eligible is null
   or ranking_eligible is null
   or reward_eligible is null
   or economic_eligible is null;

alter table hepta_paper_chain_finality_preparations_v2
    alter column latest_reproduction_id set not null,
    alter column arm_id set not null,
    alter column source_fingerprint set not null,
    alter column final_checkpoint_hash set not null,
    alter column final_anchor_hash set not null,
    alter column final_chain_id set not null,
    alter column final_height set not null,
    alter column final_header_hash set not null,
    alter column final_consensus_time_unix_ms set not null,
    alter column research_session_id set not null,
    alter column research_session_roster_version set not null,
    alter column match_evidence_commitment_id set not null,
    alter column appeal_status set not null,
    alter column request_hash set not null,
    alter column scientific_finality set not null,
    alter column score_eligible set not null,
    alter column ranking_eligible set not null,
    alter column reward_eligible set not null,
    alter column economic_eligible set not null;

do $block$
begin
    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_window_arms_v2'::regclass
          and conname = 'hepta_paper_finality_v2_arm_identity_unique'
    ) then
        alter table hepta_paper_chain_finality_window_arms_v2
            add constraint hepta_paper_finality_v2_arm_identity_unique
            unique (
                arm_id,
                paper_project_id,
                submission_id,
                evaluation_id,
                latest_reproduction_id,
                research_session_id,
                research_session_roster_version,
                source_fingerprint
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_window_arms_v2'::regclass
          and conname = 'hepta_paper_finality_v2_arm_submission_fkey'
    ) then
        alter table hepta_paper_chain_finality_window_arms_v2
            add constraint hepta_paper_finality_v2_arm_submission_fkey
            foreign key (submission_id, paper_project_id)
            references hepta_joint_paper_submissions(submission_id, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_window_arms_v2'::regclass
          and conname = 'hepta_paper_finality_v2_arm_evaluation_fkey'
    ) then
        alter table hepta_paper_chain_finality_window_arms_v2
            add constraint hepta_paper_finality_v2_arm_evaluation_fkey
            foreign key (evaluation_id, submission_id, paper_project_id)
            references hepta_paper_evaluations(
                evaluation_id, submission_id, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_window_arms_v2'::regclass
          and conname = 'hepta_paper_finality_v2_arm_reproduction_fkey'
    ) then
        alter table hepta_paper_chain_finality_window_arms_v2
            add constraint hepta_paper_finality_v2_arm_reproduction_fkey
            foreign key (latest_reproduction_id, evaluation_id, paper_project_id)
            references hepta_paper_reproductions(
                reproduction_id, evaluation_id, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_window_arms_v2'::regclass
          and conname = 'hepta_paper_finality_v2_arm_authorization_fkey'
    ) then
        alter table hepta_paper_chain_finality_window_arms_v2
            add constraint hepta_paper_finality_v2_arm_authorization_fkey
            foreign key (
                research_session_id,
                research_session_roster_version,
                paper_project_id
            ) references hepta_research_session_authorization_sets(
                session_id, roster_version, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_window_arms_v2'::regclass
          and conname = 'hepta_paper_finality_v2_arm_appeal_fkey'
    ) then
        alter table hepta_paper_chain_finality_window_arms_v2
            add constraint hepta_paper_finality_v2_arm_appeal_fkey
            foreign key (appeal_id, paper_project_id)
            references hepta_paper_appeals(appeal_id, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_window_arms_v2'::regclass
          and conname = 'hepta_paper_finality_v2_arm_resolution_fkey'
    ) then
        alter table hepta_paper_chain_finality_window_arms_v2
            add constraint hepta_paper_finality_v2_arm_resolution_fkey
            foreign key (appeal_resolution_id, appeal_id, paper_project_id)
            references hepta_paper_appeal_resolutions(
                resolution_id, appeal_id, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_window_arms_v2'::regclass
          and conname = 'hepta_paper_finality_v2_arm_checkpoint_fkey'
    ) then
        alter table hepta_paper_chain_finality_window_arms_v2
            add constraint hepta_paper_finality_v2_arm_checkpoint_fkey
            foreign key (
                start_checkpoint_hash,
                start_anchor_hash,
                start_chain_id,
                start_height,
                start_header_hash,
                start_consensus_time_unix_ms
            ) references hepta_trnm_cometbft_time_checkpoints_v1(
                checkpoint_hash,
                trust_anchor_hash,
                chain_id,
                height,
                header_hash,
                consensus_time_unix_ms
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_window_arms_v2'::regclass
          and conname = 'hepta_paper_finality_v2_arm_values_check'
    ) then
        alter table hepta_paper_chain_finality_window_arms_v2
            add constraint hepta_paper_finality_v2_arm_values_check check (
                source_fingerprint ~ '^sha256:[0-9a-f]{64}$'
                and start_checkpoint_hash ~ '^sha256:[0-9a-f]{64}$'
                and start_anchor_hash ~ '^[0-9a-f]{64}$'
                and length(start_chain_id) between 1 and 128
                and start_header_hash ~ '^[0-9a-f]{64}$'
                and request_hash ~ '^sha256:[0-9a-f]{64}$'
                and observed_max_checkpoint_height = start_height
                and max_chain_time_lag_ms = 900000
                and earliest_final_checkpoint_time_unix_ms
                    = start_consensus_time_unix_ms
                      + max_chain_time_lag_ms
                      + case
                            when appeal_status = 'closed_no_appeal' then 86400000
                            else 0
                        end
                and (
                    (appeal_status = 'closed_no_appeal'
                        and appeal_id is null
                        and appeal_resolution_id is null)
                    or
                    (appeal_status in ('resolved_denied', 'resolved_upheld')
                        and appeal_id is not null
                        and appeal_resolution_id is not null)
                )
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_window_arms_v2'::regclass
          and conname = 'hepta_paper_finality_v2_arm_record_json_check'
    ) then
        alter table hepta_paper_chain_finality_window_arms_v2
            add constraint hepta_paper_finality_v2_arm_record_json_check check (
                coalesce((
                    jsonb_typeof(record_json) = 'object'
                    and jsonb_typeof(record_json -> 'start_checkpoint') = 'object'
                    and record_json ->> 'schema'
                        = 'hepta.paper_raid.trnm_finality_window_arm.v2'
                    and record_json ->> 'arm_id' = arm_id::text
                    and record_json ->> 'paper_project_id' = paper_project_id::text
                    and record_json ->> 'submission_id' = submission_id::text
                    and record_json ->> 'evaluation_id' = evaluation_id::text
                    and record_json ->> 'latest_reproduction_id'
                        = latest_reproduction_id::text
                    and record_json ->> 'research_session_id' = research_session_id
                    and record_json ->> 'research_session_roster_version'
                        = research_session_roster_version::text
                    and record_json ->> 'source_fingerprint' = source_fingerprint
                    and record_json ->> 'appeal_status' = appeal_status
                    and nullif(record_json ->> 'appeal_id', '')::uuid
                        is not distinct from appeal_id
                    and nullif(record_json ->> 'appeal_resolution_id', '')::uuid
                        is not distinct from appeal_resolution_id
                    and record_json #>> '{start_checkpoint,checkpoint_hash}'
                        = start_checkpoint_hash
                    and record_json #>> '{start_checkpoint,trust_anchor_hash}'
                        = start_anchor_hash
                    and record_json #>> '{start_checkpoint,chain_id}' = start_chain_id
                    and record_json #>> '{start_checkpoint,height}' = start_height::text
                    and record_json #>> '{start_checkpoint,header_hash}'
                        = start_header_hash
                    and record_json #>> '{start_checkpoint,consensus_time_unix_ms}'
                        = start_consensus_time_unix_ms::text
                    and record_json ->> 'observed_max_checkpoint_height'
                        = observed_max_checkpoint_height::text
                    and record_json ->> 'max_chain_time_lag_ms'
                        = max_chain_time_lag_ms::text
                    and record_json ->> 'earliest_final_checkpoint_time_unix_ms'
                        = earliest_final_checkpoint_time_unix_ms::text
                    and record_json ->> 'idempotency_key' = idempotency_key
                    and record_json ->> 'request_hash' = request_hash
                ), false)
            );
    end if;
end;
$block$;

create index if not exists hepta_paper_finality_window_arms_v2_history_idx
    on hepta_paper_chain_finality_window_arms_v2
       (paper_project_id, start_height, arm_id);

do $block$
begin
    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_one_preparation_per_paper'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_one_preparation_per_paper
            unique (paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_one_preparation_per_arm'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_one_preparation_per_arm
            unique (arm_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_arm_fkey'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_arm_fkey
            foreign key (
                arm_id,
                paper_project_id,
                submission_id,
                evaluation_id,
                latest_reproduction_id,
                research_session_id,
                research_session_roster_version,
                source_fingerprint
            ) references hepta_paper_chain_finality_window_arms_v2(
                arm_id,
                paper_project_id,
                submission_id,
                evaluation_id,
                latest_reproduction_id,
                research_session_id,
                research_session_roster_version,
                source_fingerprint
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_final_checkpoint_fkey'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_final_checkpoint_fkey
            foreign key (
                final_checkpoint_hash,
                final_anchor_hash,
                final_chain_id,
                final_height,
                final_header_hash,
                final_consensus_time_unix_ms
            ) references hepta_trnm_cometbft_time_checkpoints_v1(
                checkpoint_hash,
                trust_anchor_hash,
                chain_id,
                height,
                header_hash,
                consensus_time_unix_ms
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_anchor_identity_unique'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_anchor_identity_unique
            unique (preparation_id, paper_project_id, commitment_id, created_at);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_submission_paper_fkey'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_submission_paper_fkey
            foreign key (submission_id, paper_project_id)
            references hepta_joint_paper_submissions(submission_id, paper_project_id);
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_evaluation_submission_paper_fkey'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_evaluation_submission_paper_fkey
            foreign key (evaluation_id, submission_id, paper_project_id)
            references hepta_paper_evaluations(
                evaluation_id, submission_id, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_latest_reproduction_fkey'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_latest_reproduction_fkey
            foreign key (latest_reproduction_id, evaluation_id, paper_project_id)
            references hepta_paper_reproductions(
                reproduction_id, evaluation_id, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_appeal_evaluation_paper_fkey'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_appeal_evaluation_paper_fkey
            foreign key (appeal_id, appealed_evaluation_id, paper_project_id)
            references hepta_paper_appeals(
                appeal_id, evaluation_id, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_appeal_resolution_fkey'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_appeal_resolution_fkey
            foreign key (appeal_resolution_id, appeal_id, paper_project_id)
            references hepta_paper_appeal_resolutions(
                resolution_id, appeal_id, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_authorization_epoch_fkey'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_authorization_epoch_fkey
            foreign key (
                research_session_id,
                research_session_roster_version,
                paper_project_id
            ) references hepta_research_session_authorization_sets(
                session_id, roster_version, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_match_evidence_fkey'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_match_evidence_fkey
            foreign key (
                match_evidence_commitment_id,
                research_session_id,
                research_session_roster_version,
                paper_project_id
            ) references hepta_nakama_research_session_completions(
                commitment_id, session_id, roster_version, paper_project_id
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_canonical_values_check'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_canonical_values_check check (
                request_hash ~ '^sha256:[0-9a-f]{64}$'
                and commitment_id ~ '^sha256:[0-9a-f]{64}$'
                and binding_fingerprint ~ '^sha256:[0-9a-f]{64}$'
                and source_fingerprint ~ '^sha256:[0-9a-f]{64}$'
                and final_checkpoint_hash ~ '^sha256:[0-9a-f]{64}$'
                and final_anchor_hash ~ '^[0-9a-f]{64}$'
                and length(final_chain_id) between 1 and 128
                and final_height > 0
                and final_header_hash ~ '^[0-9a-f]{64}$'
                and final_consensus_time_unix_ms >= 0
                and floor(extract(epoch from created_at) * 1000)::bigint
                    = final_consensus_time_unix_ms
                and match_evidence_commitment_id ~ '^sha256:[0-9a-f]{64}$'
                and research_session_roster_version > 0
                and appeal_status in (
                    'closed_no_appeal', 'resolved_denied', 'resolved_upheld'
                )
                and (
                    (appeal_status = 'closed_no_appeal'
                        and appeal_id is null
                        and appealed_evaluation_id is null
                        and appeal_resolution_id is null)
                    or
                    (appeal_status = 'resolved_denied'
                        and appeal_id is not null
                        and appealed_evaluation_id is not null
                        and appealed_evaluation_id = evaluation_id
                        and appeal_resolution_id is not null)
                    or
                    (appeal_status = 'resolved_upheld'
                        and appeal_id is not null
                        and appealed_evaluation_id is not null
                        and appealed_evaluation_id <> evaluation_id
                        and appeal_resolution_id is not null)
                )
                and scientific_finality
                and not score_eligible
                and not ranking_eligible
                and not reward_eligible
                and not economic_eligible
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_chain_finality_preparations_v2'::regclass
          and conname = 'hepta_paper_finality_v2_record_json_check'
    ) then
        alter table hepta_paper_chain_finality_preparations_v2
            add constraint hepta_paper_finality_v2_record_json_check check (coalesce((
                jsonb_typeof(record_json) = 'object'
                and jsonb_typeof(record_json -> 'binding') = 'object'
                and record_json ->> 'schema'
                    = 'hepta.paper_raid.trnm_finality_preparation.v2'
                and record_json ->> 'preparation_id' = preparation_id::text
                and record_json ->> 'idempotency_key' = idempotency_key
                and record_json ->> 'request_hash' = request_hash
                and record_json ->> 'binding_fingerprint' = binding_fingerprint
                and record_json ->> 'status' = 'awaiting_chain_verifier_upgrade'
                and date_trunc(
                    'microseconds',
                    (record_json ->> 'created_at')::timestamptz
                ) = created_at
                and record_json #>> '{binding,schema}'
                    = 'hepta.paper_raid.trnm_command_binding.v2'
                and record_json #>> '{binding,commitment_id}' = commitment_id
                and record_json #>> '{binding,source_fingerprint}'
                    = source_fingerprint
                and record_json #>> '{binding,window_arm_id}' = arm_id::text
                and record_json #>> '{binding,paper_project_id}'
                    = paper_project_id::text
                and record_json #>> '{binding,submission_id}' = submission_id::text
                and record_json #>> '{binding,evaluation_id}' = evaluation_id::text
                and record_json #>> '{binding,latest_reproduction_id}'
                    = latest_reproduction_id::text
                and record_json #>> '{binding,research_session_id}'
                    = research_session_id
                and record_json #>> '{binding,research_session_roster_version}'
                    = research_session_roster_version::text
                and record_json #>> '{binding,match_evidence_commitment_id}'
                    = match_evidence_commitment_id
                and record_json #>> '{binding,appeal_status}' = appeal_status
                and nullif(record_json #>> '{binding,appeal_id}', '')::uuid
                    is not distinct from appeal_id
                and nullif(record_json #>> '{binding,appealed_evaluation_id}', '')::uuid
                    is not distinct from appealed_evaluation_id
                and nullif(record_json #>> '{binding,appeal_resolution_id}', '')::uuid
                    is not distinct from appeal_resolution_id
                and record_json #>> '{binding,final_checkpoint_hash}'
                    = final_checkpoint_hash
                and record_json #>> '{binding,final_checkpoint_anchor_hash}'
                    = final_anchor_hash
                and record_json #>> '{binding,start_checkpoint_chain_id}'
                    = final_chain_id
                and record_json #>> '{binding,final_checkpoint_chain_id}'
                    = final_chain_id
                and record_json #>> '{binding,final_checkpoint_height}'
                    = final_height::text
                and record_json #>> '{binding,final_checkpoint_header_hash}'
                    = final_header_hash
                and record_json #>> '{binding,final_checkpoint_consensus_time_unix_ms}'
                    = final_consensus_time_unix_ms::text
                and (record_json #>> '{binding,scientific_finality}')::boolean
                    = scientific_finality
                and (record_json #>> '{binding,score_eligible}')::boolean
                    = score_eligible
                and (record_json #>> '{binding,ranking_eligible}')::boolean
                    = ranking_eligible
                and (record_json #>> '{binding,reward_eligible}')::boolean
                    = reward_eligible
                and (record_json #>> '{binding,economic_eligible}')::boolean
                    = economic_eligible
            ), false));
    end if;
end;
$block$;

create index if not exists hepta_paper_chain_finality_preparations_v2_history_idx
    on hepta_paper_chain_finality_preparations_v2
       (paper_project_id, created_at, preparation_id);

-- The pre-existing Paper row is the serialization-conflict anchor.  The four
-- fields are either all empty (unsealed) or all populated (sealed).
alter table hepta_paper_projects
    add column if not exists finality_v2_seal_epoch smallint not null default 0,
    add column if not exists finality_v2_seal_preparation_id uuid,
    add column if not exists finality_v2_seal_commitment_id text,
    add column if not exists finality_v2_sealed_at timestamptz;

-- Upgrade an early preparation table by anchoring every existing immutable
-- preparation. The one-Paper unique constraint above makes this deterministic.
update hepta_paper_projects as paper
set finality_v2_seal_epoch = 1,
    finality_v2_seal_preparation_id = preparation.preparation_id,
    finality_v2_seal_commitment_id = preparation.commitment_id,
    finality_v2_sealed_at = preparation.created_at
from hepta_paper_chain_finality_preparations_v2 as preparation
where paper.paper_project_id = preparation.paper_project_id
  and paper.finality_v2_seal_epoch = 0;

do $block$
begin
    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_projects'::regclass
          and conname = 'hepta_paper_projects_finality_v2_seal_coherent'
    ) then
        alter table hepta_paper_projects
            add constraint hepta_paper_projects_finality_v2_seal_coherent check (
                (
                    finality_v2_seal_epoch = 0
                    and finality_v2_seal_preparation_id is null
                    and finality_v2_seal_commitment_id is null
                    and finality_v2_sealed_at is null
                )
                or
                (
                    finality_v2_seal_epoch = 1
                    and finality_v2_seal_preparation_id is not null
                    and finality_v2_seal_commitment_id is not null
                    and finality_v2_sealed_at is not null
                )
            );
    end if;

    if not exists (
        select 1 from pg_constraint
        where conrelid = 'hepta_paper_projects'::regclass
          and conname = 'hepta_paper_projects_finality_v2_seal_preparation_fkey'
    ) then
        alter table hepta_paper_projects
            add constraint hepta_paper_projects_finality_v2_seal_preparation_fkey
            foreign key (
                finality_v2_seal_preparation_id,
                paper_project_id,
                finality_v2_seal_commitment_id,
                finality_v2_sealed_at
            ) references hepta_paper_chain_finality_preparations_v2(
                preparation_id, paper_project_id, commitment_id, created_at
            );
    end if;
end;
$block$;

create or replace function hepta_reject_paper_finality_v2_evidence_mutation()
returns trigger
language plpgsql
as $function$
begin
    raise exception using
        errcode = '55000',
        message = 'hepta_paper_chain_finality_v2_evidence_immutable',
        detail = format(
            'Verified Chain-time checkpoint or window-arm table %s is immutable',
            tg_table_name
        );
end;
$function$;

create or replace function hepta_validate_paper_finality_v2_time_checkpoint()
returns trigger
language plpgsql
as $function$
declare
    latest_record record;
    local_clock_high_water bigint;
begin
    -- Match the Rust admission lock so old binaries and direct SQL cannot race
    -- a lower height or regressed time around the application-level check.
    perform pg_advisory_xact_lock(5210757408431361349);

    select checkpoint.height, checkpoint.consensus_time_unix_ms
    into latest_record
    from hepta_trnm_cometbft_time_checkpoints_v1 as checkpoint
    where checkpoint.chain_id = new.chain_id
    order by checkpoint.height desc
    limit 1;

    if found and new.height <= latest_record.height then
        raise exception using
            errcode = '23514',
            message = 'hepta_paper_chain_time_checkpoint_height_replay',
            detail = 'Authenticated Chain-time checkpoints must strictly advance height';
    end if;
    if found and new.consensus_time_unix_ms < latest_record.consensus_time_unix_ms then
        raise exception using
            errcode = '23514',
            message = 'hepta_paper_chain_time_checkpoint_time_regression',
            detail = 'Authenticated Chain-time checkpoints cannot move consensus time backward';
    end if;

    select max(checkpoint.locally_verified_at_unix_ms)
    into local_clock_high_water
    from hepta_trnm_cometbft_time_checkpoints_v1 as checkpoint;
    if local_clock_high_water is not null
       and new.locally_verified_at_unix_ms < local_clock_high_water then
        raise exception using
            errcode = '23514',
            message = 'hepta_paper_chain_time_local_clock_rollback',
            detail = 'Local light-client verification time cannot move below its durable high-water';
    end if;

    return new;
end;
$function$;

create or replace function hepta_paper_finality_v2_lock_window_arm()
returns trigger
language plpgsql
security definer
set search_path = pg_catalog
as $function$
declare
    seal_epoch smallint;
    checkpoint_record record;
begin
    -- Use the exact admission serialization boundary even if an old binary or
    -- privileged maintenance client bypasses the Rust handler.
    perform pg_catalog.pg_advisory_xact_lock(5210757408431361349);

    select paper.finality_v2_seal_epoch
    into seal_epoch
    from public.hepta_paper_projects as paper
    where paper.paper_project_id = new.paper_project_id
    for update;

    if not found then
        raise exception using
            errcode = '23503',
            message = 'hepta_paper_chain_finality_v2_paper_not_found',
            detail = 'The Paper row must exist before its Chain-time window is armed';
    end if;

    if seal_epoch <> 0 then
        raise exception using
            errcode = '55000',
            message = 'hepta_paper_chain_finality_v2_source_sealed',
            detail = 'A sealed Paper cannot acquire another Chain-time window arm';
    end if;

    select checkpoint.canonical_proof_sha256,
           checkpoint.locally_verified_at_unix_ms,
           (
               select max(candidate.height)
               from public.hepta_trnm_cometbft_time_checkpoints_v1 as candidate
               where candidate.chain_id = checkpoint.chain_id
           ) as max_chain_height
    into checkpoint_record
    from public.hepta_trnm_cometbft_time_checkpoints_v1 as checkpoint
    where checkpoint.checkpoint_hash = new.start_checkpoint_hash
    for share;

    if not found then
        raise exception using
            errcode = '23503',
            message = 'hepta_paper_chain_finality_v2_checkpoint_not_found',
            detail = 'Window arm must reference a verified Chain-time checkpoint';
    end if;

    if not coalesce((
        new.record_json #>> '{start_checkpoint,schema}'
            = 'hepta.paper_raid.trnm_chain_time_checkpoint.v1'
        and new.record_json #>> '{start_checkpoint,canonical_proof_sha256}'
            = checkpoint_record.canonical_proof_sha256
        and new.record_json #>> '{start_checkpoint,locally_verified_at_unix_ms}'
            = checkpoint_record.locally_verified_at_unix_ms::text
    ), false) then
        raise exception using
            errcode = '23514',
            message = 'hepta_paper_chain_finality_v2_checkpoint_record_mismatch',
            detail = 'Window-arm record does not reproduce its verified checkpoint';
    end if;

    if new.start_height is distinct from checkpoint_record.max_chain_height then
        raise exception using
            errcode = '23514',
            message = 'hepta_paper_chain_finality_v2_start_checkpoint_not_latest',
            detail = 'Window arm must bind the highest admitted checkpoint for its Chain';
    end if;

    return new;
end;
$function$;

create or replace function hepta_paper_finality_v2_lock_preparation()
returns trigger
language plpgsql
security definer
set search_path = pg_catalog
as $function$
declare
    seal_epoch smallint;
    arm_record record;
    evaluation_supersedes_id uuid;
    resolution_outcome text;
    resolution_superseding_evaluation_id uuid;
begin
    select paper.finality_v2_seal_epoch
    into seal_epoch
    from public.hepta_paper_projects as paper
    where paper.paper_project_id = new.paper_project_id
    for update;

    if not found then
        raise exception using
            errcode = '23503',
            message = 'hepta_paper_chain_finality_v2_paper_not_found',
            detail = 'The Paper row must exist before finality preparation';
    end if;

    if seal_epoch <> 0 then
        raise exception using
            errcode = '55000',
            message = 'hepta_paper_chain_finality_v2_source_sealed',
            detail = 'An immutable Paper V2 finality preparation already binds this Paper';
    end if;

    select arm.*
    into arm_record
    from public.hepta_paper_chain_finality_window_arms_v2 as arm
    where arm.arm_id = new.arm_id
    for share;

    if not found then
        raise exception using
            errcode = '23503',
            message = 'hepta_paper_chain_finality_v2_arm_not_found',
            detail = 'Final preparation must reference an immutable Chain-time window arm';
    end if;

    if arm_record.paper_project_id <> new.paper_project_id
       or arm_record.submission_id <> new.submission_id
       or arm_record.evaluation_id <> new.evaluation_id
       or arm_record.latest_reproduction_id <> new.latest_reproduction_id
       or arm_record.research_session_id <> new.research_session_id
       or arm_record.research_session_roster_version
            <> new.research_session_roster_version
       or arm_record.source_fingerprint <> new.source_fingerprint
       or arm_record.appeal_status <> new.appeal_status
       or arm_record.appeal_id is distinct from new.appeal_id
       or arm_record.appeal_resolution_id
            is distinct from new.appeal_resolution_id then
        raise exception using
            errcode = '23514',
            message = 'hepta_paper_chain_finality_v2_arm_binding_mismatch',
            detail = 'Preparation canonical columns do not reproduce the exact window arm';
    end if;

    if new.final_chain_id <> arm_record.start_chain_id
       or new.final_height <= arm_record.observed_max_checkpoint_height
       or new.final_height <= arm_record.start_height
       or new.final_consensus_time_unix_ms
            < arm_record.earliest_final_checkpoint_time_unix_ms then
        raise exception using
            errcode = '23514',
            message = 'hepta_paper_chain_finality_v2_checkpoint_not_causal',
            detail = 'Final checkpoint does not advance the arm or close its Chain-time window';
    end if;

    if new.appeal_status <> 'closed_no_appeal' then
        select evaluation.supersedes_evaluation_id
        into evaluation_supersedes_id
        from public.hepta_paper_evaluations as evaluation
        where evaluation.evaluation_id = new.evaluation_id
          and evaluation.submission_id = new.submission_id
          and evaluation.paper_project_id = new.paper_project_id;

        select resolution.outcome, resolution.superseding_evaluation_id
        into resolution_outcome, resolution_superseding_evaluation_id
        from public.hepta_paper_appeal_resolutions as resolution
        where resolution.resolution_id = new.appeal_resolution_id
          and resolution.appeal_id = new.appeal_id
          and resolution.paper_project_id = new.paper_project_id;

        if new.appeal_status = 'resolved_denied'
           and (
                resolution_outcome is distinct from 'denied'
                or new.appealed_evaluation_id is distinct from new.evaluation_id
                or evaluation_supersedes_id is not null
                or resolution_superseding_evaluation_id is not null
           ) then
            raise exception using
                errcode = '23514',
                message = 'hepta_paper_chain_finality_v2_denied_appeal_mismatch',
                detail = 'Denied Appeal must preserve the appealed final evaluation';
        end if;

        if new.appeal_status = 'resolved_upheld'
           and (
                resolution_outcome is distinct from 'upheld'
                or evaluation_supersedes_id
                    is distinct from new.appealed_evaluation_id
                or resolution_superseding_evaluation_id
                    is distinct from new.evaluation_id
           ) then
            raise exception using
                errcode = '23514',
                message = 'hepta_paper_chain_finality_v2_upheld_appeal_mismatch',
                detail = 'Upheld Appeal must bind its exact direct replacement evaluation';
        end if;
    end if;

    if not coalesce((
        new.record_json #>> '{binding,start_checkpoint_hash}'
            = arm_record.start_checkpoint_hash
        and new.record_json #>> '{binding,start_checkpoint_anchor_hash}'
            = arm_record.start_anchor_hash
        and new.record_json #>> '{binding,start_checkpoint_chain_id}'
            = arm_record.start_chain_id
        and new.record_json #>> '{binding,start_checkpoint_height}'
            = arm_record.start_height::text
        and new.record_json #>> '{binding,start_checkpoint_header_hash}'
            = arm_record.start_header_hash
        and new.record_json #>> '{binding,start_checkpoint_consensus_time_unix_ms}'
            = arm_record.start_consensus_time_unix_ms::text
        and new.record_json #>> '{binding,max_chain_time_lag_ms}'
            = arm_record.max_chain_time_lag_ms::text
        and new.record_json #>> '{binding,appeal_window_closes_at_unix_ms}'
            = arm_record.earliest_final_checkpoint_time_unix_ms::text
    ), false) then
        raise exception using
            errcode = '23514',
            message = 'hepta_paper_chain_finality_v2_arm_record_mismatch',
            detail = 'Preparation record does not reproduce the arm checkpoint and deadline';
    end if;

    return new;
end;
$function$;

create or replace function hepta_paper_finality_v2_apply_seal()
returns trigger
language plpgsql
security definer
set search_path = pg_catalog
as $function$
declare
    updated_count bigint;
begin
    update public.hepta_paper_projects
    set finality_v2_seal_epoch = 1,
        finality_v2_seal_preparation_id = new.preparation_id,
        finality_v2_seal_commitment_id = new.commitment_id,
        finality_v2_sealed_at = new.created_at
    where paper_project_id = new.paper_project_id
      and finality_v2_seal_epoch = 0;

    get diagnostics updated_count = row_count;
    if updated_count <> 1 then
        raise exception using
            errcode = '55000',
            message = 'hepta_paper_chain_finality_v2_source_sealed',
            detail = 'Paper V2 source seal could not be applied atomically';
    end if;

    return new;
end;
$function$;

create or replace function hepta_reject_paper_finality_v2_preparation_mutation()
returns trigger
language plpgsql
as $function$
begin
    raise exception using
        errcode = '55000',
        message = 'hepta_paper_chain_finality_v2_preparation_immutable',
        detail = 'Paper V2 finality preparations cannot be updated or deleted';
end;
$function$;

create or replace function hepta_guard_paper_finality_v2_anchor_mutation()
returns trigger
language plpgsql
set search_path = pg_catalog
as $function$
declare
    business_old jsonb;
    business_new jsonb;
begin
    if tg_op = 'DELETE' then
        if old.finality_v2_seal_epoch = 1 then
            raise exception using
                errcode = '55000',
                message = 'hepta_paper_chain_finality_v2_source_sealed',
                detail = 'A sealed Paper cannot be deleted';
        end if;
        return old;
    end if;

    if old.finality_v2_seal_epoch = 1 then
        raise exception using
            errcode = '55000',
            message = 'hepta_paper_chain_finality_v2_source_sealed',
            detail = 'A sealed Paper cannot be updated, including by a no-op update';
    end if;

    if new.finality_v2_seal_epoch = 0
       and new.finality_v2_seal_preparation_id is null
       and new.finality_v2_seal_commitment_id is null
       and new.finality_v2_sealed_at is null then
        return new;
    end if;

    business_old := to_jsonb(old) - array[
        'finality_v2_seal_epoch',
        'finality_v2_seal_preparation_id',
        'finality_v2_seal_commitment_id',
        'finality_v2_sealed_at'
    ];
    business_new := to_jsonb(new) - array[
        'finality_v2_seal_epoch',
        'finality_v2_seal_preparation_id',
        'finality_v2_seal_commitment_id',
        'finality_v2_sealed_at'
    ];

    if new.finality_v2_seal_epoch = 1
       and business_new = business_old
       and exists (
            select 1
            from public.hepta_paper_chain_finality_preparations_v2 as preparation
            where preparation.preparation_id = new.finality_v2_seal_preparation_id
              and preparation.paper_project_id = new.paper_project_id
              and preparation.commitment_id = new.finality_v2_seal_commitment_id
              and preparation.created_at = new.finality_v2_sealed_at
       ) then
        return new;
    end if;

    raise exception using
        errcode = '55000',
        message = 'hepta_paper_chain_finality_v2_anchor_mutation_forbidden',
        detail = 'Paper finality seal columns may only change during matching preparation insertion';
end;
$function$;

create or replace function hepta_assert_paper_finality_v2_source_unsealed(
    target_paper_id uuid
)
returns void
language plpgsql
security definer
set search_path = pg_catalog
as $function$
declare
    seal_epoch smallint;
begin
    if target_paper_id is null then
        return;
    end if;

    select paper.finality_v2_seal_epoch
    into seal_epoch
    from public.hepta_paper_projects as paper
    where paper.paper_project_id = target_paper_id
    for update;

    if found and seal_epoch <> 0 then
        raise exception using
            errcode = '55000',
            message = 'hepta_paper_chain_finality_v2_source_sealed',
            detail = 'An immutable Paper V2 finality preparation already binds this Paper';
    end if;
end;
$function$;

create or replace function hepta_reject_paper_finality_v2_source_mutation()
returns trigger
language plpgsql
security definer
set search_path = pg_catalog
as $function$
declare
    old_paper_id uuid;
    new_paper_id uuid;
begin
    if tg_op <> 'INSERT' then
        old_paper_id := old.paper_project_id;
    end if;
    if tg_op <> 'DELETE' then
        new_paper_id := new.paper_project_id;
    end if;

    -- Lock two-paper UPDATEs in UUID order to avoid cross-Paper deadlock.
    if old_paper_id is not null
       and new_paper_id is not null
       and old_paper_id <> new_paper_id then
        if old_paper_id < new_paper_id then
            perform public.hepta_assert_paper_finality_v2_source_unsealed(old_paper_id);
            perform public.hepta_assert_paper_finality_v2_source_unsealed(new_paper_id);
        else
            perform public.hepta_assert_paper_finality_v2_source_unsealed(new_paper_id);
            perform public.hepta_assert_paper_finality_v2_source_unsealed(old_paper_id);
        end if;
    else
        perform public.hepta_assert_paper_finality_v2_source_unsealed(
            coalesce(new_paper_id, old_paper_id)
        );
    end if;

    if tg_op = 'DELETE' then
        return old;
    end if;
    return new;
end;
$function$;

-- PostgreSQL grants function EXECUTE to PUBLIC by default. These definer
-- helpers are trigger-owned capabilities, except for the explicit anchor lock
-- granted narrowly to the finality writer by the one-shot migrator.
revoke all on function public.hepta_paper_finality_v2_lock_window_arm() from public;
revoke all on function public.hepta_paper_finality_v2_lock_preparation() from public;
revoke all on function public.hepta_paper_finality_v2_apply_seal() from public;
revoke all on function public.hepta_assert_paper_finality_v2_source_unsealed(uuid) from public;
revoke all on function public.hepta_reject_paper_finality_v2_source_mutation() from public;

create or replace function hepta_reject_paper_finality_v2_truncate()
returns trigger
language plpgsql
as $function$
begin
    raise exception using
        errcode = '55000',
        message = 'hepta_paper_chain_finality_v2_truncate_forbidden',
        detail = format(
            'TRUNCATE is forbidden for immutable V2 evidence or source table %s',
            tg_table_name
        );
end;
$function$;

create trigger hepta_trnm_time_checkpoint_v1_progress_guard
before insert on hepta_trnm_cometbft_time_checkpoints_v1
for each row execute function hepta_validate_paper_finality_v2_time_checkpoint();

create trigger hepta_trnm_time_checkpoint_v1_immutable_guard
before update or delete on hepta_trnm_cometbft_time_checkpoints_v1
for each row execute function hepta_reject_paper_finality_v2_evidence_mutation();

create trigger hepta_paper_finality_v2_window_arm_guard
before insert on hepta_paper_chain_finality_window_arms_v2
for each row execute function hepta_paper_finality_v2_lock_window_arm();

create trigger hepta_paper_finality_v2_window_arm_immutable_guard
before update or delete on hepta_paper_chain_finality_window_arms_v2
for each row execute function hepta_reject_paper_finality_v2_evidence_mutation();

create trigger hepta_paper_finality_v2_preparation_guard
before insert on hepta_paper_chain_finality_preparations_v2
for each row execute function hepta_paper_finality_v2_lock_preparation();

create trigger hepta_paper_finality_v2_preparation_seal_guard
after insert on hepta_paper_chain_finality_preparations_v2
for each row execute function hepta_paper_finality_v2_apply_seal();

create trigger hepta_paper_finality_v2_preparation_immutable_guard
before update or delete on hepta_paper_chain_finality_preparations_v2
for each row execute function hepta_reject_paper_finality_v2_preparation_mutation();

create trigger hepta_paper_projects_finality_v2_source_guard
before update or delete on hepta_paper_projects
for each row execute function hepta_guard_paper_finality_v2_anchor_mutation();

create trigger hepta_joint_submissions_finality_v2_source_guard
before insert or update or delete on hepta_joint_paper_submissions
for each row execute function hepta_reject_paper_finality_v2_source_mutation();

create trigger hepta_paper_evaluations_finality_v2_source_guard
before insert or update or delete on hepta_paper_evaluations
for each row execute function hepta_reject_paper_finality_v2_source_mutation();

create trigger hepta_paper_reproductions_finality_v2_source_guard
before insert or update or delete on hepta_paper_reproductions
for each row execute function hepta_reject_paper_finality_v2_source_mutation();

create trigger hepta_paper_appeals_finality_v2_source_guard
before insert or update or delete on hepta_paper_appeals
for each row execute function hepta_reject_paper_finality_v2_source_mutation();

create trigger hepta_paper_resolutions_finality_v2_source_guard
before insert or update or delete on hepta_paper_appeal_resolutions
for each row execute function hepta_reject_paper_finality_v2_source_mutation();

create trigger hepta_research_auth_sets_finality_v2_source_guard
before insert or update or delete on hepta_research_session_authorization_sets
for each row execute function hepta_reject_paper_finality_v2_source_mutation();

create trigger hepta_nakama_completions_finality_v2_source_guard
before insert or update or delete on hepta_nakama_research_session_completions
for each row execute function hepta_reject_paper_finality_v2_source_mutation();

do $block$
declare
    guarded_table regclass;
    truncate_trigger_name text;
begin
    for guarded_table, truncate_trigger_name in
        select guarded.guarded_table, guarded.truncate_trigger_name
        from (values
            ('hepta_trnm_cometbft_time_checkpoints_v1'::regclass,
             'hepta_trnm_time_checkpoint_v1_truncate_guard'),
            ('hepta_paper_chain_finality_window_arms_v2'::regclass,
             'hepta_paper_finality_v2_window_arm_truncate_guard'),
            ('hepta_paper_chain_finality_preparations_v2'::regclass,
             'hepta_paper_finality_v2_preparation_truncate_guard'),
            ('hepta_paper_projects'::regclass,
             'hepta_paper_projects_finality_v2_truncate_guard'),
            ('hepta_joint_paper_submissions'::regclass,
             'hepta_joint_submissions_finality_v2_truncate_guard'),
            ('hepta_paper_evaluations'::regclass,
             'hepta_paper_evaluations_finality_v2_truncate_guard'),
            ('hepta_paper_reproductions'::regclass,
             'hepta_paper_reproductions_finality_v2_truncate_guard'),
            ('hepta_paper_appeals'::regclass,
             'hepta_paper_appeals_finality_v2_truncate_guard'),
            ('hepta_paper_appeal_resolutions'::regclass,
             'hepta_paper_resolutions_finality_v2_truncate_guard'),
            ('hepta_research_session_authorization_sets'::regclass,
             'hepta_research_auth_sets_finality_v2_truncate_guard'),
            ('hepta_nakama_research_session_completions'::regclass,
             'hepta_nakama_completions_finality_v2_truncate_guard')
        ) as guarded(guarded_table, truncate_trigger_name)
    loop
        execute format(
            'create trigger %I before truncate on %s '
            'for each statement execute function '
            'hepta_reject_paper_finality_v2_truncate()',
            truncate_trigger_name,
            guarded_table
        );
    end loop;
end;
$block$;

do $block$
declare
    guarded_table regclass;
    trigger_name text;
begin
    for guarded_table, trigger_name in
        select guarded.guarded_table, guarded.trigger_name
        from (values
            ('hepta_trnm_cometbft_time_checkpoints_v1'::regclass,
             'hepta_trnm_time_checkpoint_v1_progress_guard'),
            ('hepta_trnm_cometbft_time_checkpoints_v1'::regclass,
             'hepta_trnm_time_checkpoint_v1_immutable_guard'),
            ('hepta_trnm_cometbft_time_checkpoints_v1'::regclass,
             'hepta_trnm_time_checkpoint_v1_truncate_guard'),
            ('hepta_paper_chain_finality_window_arms_v2'::regclass,
             'hepta_paper_finality_v2_window_arm_guard'),
            ('hepta_paper_chain_finality_window_arms_v2'::regclass,
             'hepta_paper_finality_v2_window_arm_immutable_guard'),
            ('hepta_paper_chain_finality_window_arms_v2'::regclass,
             'hepta_paper_finality_v2_window_arm_truncate_guard'),
            ('hepta_paper_chain_finality_preparations_v2'::regclass,
             'hepta_paper_finality_v2_preparation_guard'),
            ('hepta_paper_chain_finality_preparations_v2'::regclass,
             'hepta_paper_finality_v2_preparation_seal_guard'),
            ('hepta_paper_chain_finality_preparations_v2'::regclass,
             'hepta_paper_finality_v2_preparation_immutable_guard'),
            ('hepta_paper_chain_finality_preparations_v2'::regclass,
             'hepta_paper_finality_v2_preparation_truncate_guard'),
            ('hepta_paper_projects'::regclass,
             'hepta_paper_projects_finality_v2_source_guard'),
            ('hepta_paper_projects'::regclass,
             'hepta_paper_projects_finality_v2_truncate_guard'),
            ('hepta_joint_paper_submissions'::regclass,
             'hepta_joint_submissions_finality_v2_source_guard'),
            ('hepta_joint_paper_submissions'::regclass,
             'hepta_joint_submissions_finality_v2_truncate_guard'),
            ('hepta_paper_evaluations'::regclass,
             'hepta_paper_evaluations_finality_v2_source_guard'),
            ('hepta_paper_evaluations'::regclass,
             'hepta_paper_evaluations_finality_v2_truncate_guard'),
            ('hepta_paper_reproductions'::regclass,
             'hepta_paper_reproductions_finality_v2_source_guard'),
            ('hepta_paper_reproductions'::regclass,
             'hepta_paper_reproductions_finality_v2_truncate_guard'),
            ('hepta_paper_appeals'::regclass,
             'hepta_paper_appeals_finality_v2_source_guard'),
            ('hepta_paper_appeals'::regclass,
             'hepta_paper_appeals_finality_v2_truncate_guard'),
            ('hepta_paper_appeal_resolutions'::regclass,
             'hepta_paper_resolutions_finality_v2_source_guard'),
            ('hepta_paper_appeal_resolutions'::regclass,
             'hepta_paper_resolutions_finality_v2_truncate_guard'),
            ('hepta_research_session_authorization_sets'::regclass,
             'hepta_research_auth_sets_finality_v2_source_guard'),
            ('hepta_research_session_authorization_sets'::regclass,
             'hepta_research_auth_sets_finality_v2_truncate_guard'),
            ('hepta_nakama_research_session_completions'::regclass,
             'hepta_nakama_completions_finality_v2_source_guard'),
            ('hepta_nakama_research_session_completions'::regclass,
             'hepta_nakama_completions_finality_v2_truncate_guard')
        ) as guarded(guarded_table, trigger_name)
    loop
        execute format(
            'alter table %s enable always trigger %I',
            guarded_table,
            trigger_name
        );
    end loop;
end;
$block$;

do $block$
declare
    actual_count bigint;
    actual_sha256 text;
begin
    select constraint_count, catalog_sha256
    into actual_count, actual_sha256
    from public.hepta_paper_finality_v2_constraint_catalog_fingerprint();

    if actual_count <> 77
       or actual_sha256 <> '910d4454106f5722ad44c6c9095bf48d585dfaa9501fc40d9ef377fd57c3f3ba'
    then
        raise exception using
            errcode = '55000',
            message = 'hepta_paper_chain_finality_v2_constraint_catalog_mismatch',
            detail = format(
                'expected 77 constraints with sha256 %s, got %s with sha256 %s',
                '910d4454106f5722ad44c6c9095bf48d585dfaa9501fc40d9ef377fd57c3f3ba',
                actual_count,
                coalesce(actual_sha256, '<null>')
            );
    end if;
end;
$block$;
