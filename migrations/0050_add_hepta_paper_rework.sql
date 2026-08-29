-- Immutable Author rework lineage and server-owned 24 hour rework lease.
--
-- A rejected ready submission remains immutable and is withdrawn only after
-- an exact signed rework record exists. A replacement submission gets a new
-- root Review round (1); Appeal replacement rounds remain within one
-- submission lineage.

begin;

alter table hepta_paper_projects
    add column if not exists active_rework_id uuid,
    add column if not exists active_rework_cycle bigint,
    add column if not exists rework_expires_at timestamptz;

alter table hepta_paper_projects
    drop constraint if exists hepta_paper_projects_active_rework_shape_check,
    add constraint hepta_paper_projects_active_rework_shape_check check (
        (active_rework_id is null and active_rework_cycle is null and rework_expires_at is null)
        or
        (active_rework_id is not null and active_rework_cycle >= 2 and rework_expires_at is not null)
    ),
    drop constraint if exists hepta_paper_projects_terminal_shape_check,
    add constraint hepta_paper_projects_terminal_shape_check check (
        (
            outcome = 'in_progress' and outcome_reason is null and terminal_at is null
            and phase <> 'submission_ready'
        )
        or (
            outcome = 'submission_ready' and outcome_reason is null and terminal_at is not null
            and phase = 'submission_ready'
        )
        or (
            outcome in ('failed','abandoned') and outcome_reason is not null
            and length(outcome_reason) between 1 and 512 and terminal_at is not null
            and phase <> 'submission_ready'
        )
        or (
            outcome = 'expired' and outcome_reason is not null
            and length(outcome_reason) between 1 and 512 and terminal_at is not null
            and phase <> 'submission_ready'
        )
    );

create table if not exists hepta_paper_reworks (
    rework_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id),
    rejected_evaluation_id uuid not null,
    rejected_submission_id uuid not null,
    rejected_revision_id uuid not null,
    rejected_release_candidate_hash text not null
        check (rejected_release_candidate_hash ~ '^sha256:[0-9a-f]{64}$'),
    rejected_paper_bundle_hash text not null
        check (rejected_paper_bundle_hash ~ '^sha256:[0-9a-f]{64}$'),
    rejected_rework_content_commitment_sha256 text not null
        check (
            rejected_rework_content_commitment_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and rejected_rework_content_commitment_sha256
                <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
        ),
    rework_cycle bigint not null check (rework_cycle between 2 and 9007199254740991),
    author_player_id uuid not null references hepta_human_players(player_id),
    signing_key_id text not null,
    signing_public_key text not null,
    signing_public_key_hash text not null
        check (signing_public_key_hash ~ '^sha256:[0-9a-f]{64}$'),
    reason_hash text not null check (reason_hash ~ '^sha256:[0-9a-f]{64}$'),
    signed_at_unix bigint not null check (signed_at_unix >= 0),
    request_hash text not null unique check (request_hash ~ '^sha256:[0-9a-f]{64}$'),
    signature text not null,
    rework_expires_at timestamptz not null,
    version bigint not null check (version = 1),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (paper_project_id, rework_cycle),
    unique (rejected_evaluation_id),
    unique (rejected_submission_id),
    unique (rework_id, paper_project_id),
    foreign key (rejected_evaluation_id, rejected_submission_id, paper_project_id)
        references hepta_paper_evaluations(evaluation_id, submission_id, paper_project_id),
    foreign key (rejected_submission_id, paper_project_id)
        references hepta_joint_paper_submissions(submission_id, paper_project_id),
    foreign key (rejected_revision_id, paper_project_id)
        references hepta_paper_revisions(revision_id, paper_project_id),
    foreign key (author_player_id, signing_key_id)
        references hepta_human_signing_keys(player_id, signing_key_id),
    check (rework_expires_at = created_at + interval '24 hours')
);

create table if not exists hepta_paper_rework_resubmissions (
    rework_id uuid primary key,
    paper_project_id uuid not null,
    rejected_submission_id uuid not null,
    replacement_submission_id uuid not null,
    replacement_revision_id uuid not null,
    replacement_release_candidate_hash text not null
        check (replacement_release_candidate_hash ~ '^sha256:[0-9a-f]{64}$'),
    replacement_paper_bundle_hash text not null
        check (replacement_paper_bundle_hash ~ '^sha256:[0-9a-f]{64}$'),
    replacement_rework_content_commitment_sha256 text not null
        check (
            replacement_rework_content_commitment_sha256 ~ '^sha256:[0-9a-f]{64}$'
            and replacement_rework_content_commitment_sha256
                <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
        ),
    replacement_review_round bigint not null check (replacement_review_round = 1),
    version bigint not null check (version = 1),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (replacement_submission_id),
    unique (replacement_revision_id),
    unique (replacement_release_candidate_hash),
    unique (replacement_paper_bundle_hash),
    foreign key (rework_id, paper_project_id)
        references hepta_paper_reworks(rework_id, paper_project_id),
    foreign key (rejected_submission_id, paper_project_id)
        references hepta_joint_paper_submissions(submission_id, paper_project_id),
    foreign key (replacement_submission_id, paper_project_id)
        references hepta_joint_paper_submissions(submission_id, paper_project_id),
    foreign key (replacement_revision_id, paper_project_id)
        references hepta_paper_revisions(revision_id, paper_project_id),
    check (replacement_submission_id <> rejected_submission_id)
);

alter table hepta_paper_reworks
    drop constraint if exists hepta_paper_reworks_content_commitment_nonzero_check,
    add constraint hepta_paper_reworks_content_commitment_nonzero_check check (
        rejected_rework_content_commitment_sha256
            <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
    );
alter table hepta_paper_rework_resubmissions
    drop constraint if exists hepta_paper_rework_resubmissions_content_commitment_nonzero_che,
    drop constraint if exists hepta_rework_resubmission_content_commitment_nonzero_check,
    add constraint hepta_rework_resubmission_content_commitment_nonzero_check check (
        replacement_rework_content_commitment_sha256
            <> 'sha256:0000000000000000000000000000000000000000000000000000000000000000'
    );

-- PostgreSQL does not permit a partial-index predicate with a subquery. Keep
-- the intended one-active-row invariant in the guarded insert functions below.

create index if not exists hepta_paper_reworks_project_idx
    on hepta_paper_reworks (paper_project_id, rework_cycle, rework_id);

create or replace function hepta_paper_rework_content_projection(submission_record jsonb)
returns jsonb
language sql
immutable
strict
set search_path = pg_catalog
as $function$
    select pg_catalog.jsonb_build_object(
        'schema', 'hepta.paper_raid.rework_content_commitment.v1',
        'ruleset_hash', submission_record->'paper_bundle'->'release_candidate'->'ruleset_hash',
        'challenge_snapshot_hash', submission_record->'paper_bundle'->'release_candidate'->'challenge_snapshot_hash',
        'title', submission_record->'paper_bundle'->'release_candidate'->'title',
        'abstract_text', submission_record->'paper_bundle'->'release_candidate'->'abstract_text',
        'target_format', submission_record->'paper_bundle'->'release_candidate'->'target_format',
        'source_manifest_hash', submission_record->'paper_bundle'->'release_candidate'->'source_manifest_hash',
        'artifact_manifest_hash', submission_record->'paper_bundle'->'release_candidate'->'artifact_manifest_hash',
        'bibliography_hash', submission_record->'paper_bundle'->'release_candidate'->'bibliography_hash',
        'claim_evidence_graph_hash', submission_record->'paper_bundle'->'release_candidate'->'claim_evidence_graph_hash',
        'collaboration_compact_hash', submission_record->'paper_bundle'->'release_candidate'->'collaboration_compact_hash',
        'research_protocol_snapshot_hash', submission_record->'paper_bundle'->'release_candidate'->'research_protocol_snapshot_hash',
        'ethics_disclosure_hash', submission_record->'paper_bundle'->'release_candidate'->'ethics_disclosure_hash',
        'coi_disclosure_hash', submission_record->'paper_bundle'->'release_candidate'->'coi_disclosure_hash',
        'contribution_ledger_hash', submission_record->'paper_bundle'->'release_candidate'->'contribution_ledger_hash',
        'ai_disclosure_hash', submission_record->'paper_bundle'->'release_candidate'->'ai_disclosure_hash',
        'license', submission_record->'paper_bundle'->'release_candidate'->'license'
    )
$function$;

-- The Rust contract sorts object keys by UTF-8 scalar order and emits compact
-- JSON before SHA-256. This projection is a flat string-valued object, so a
-- C-collated ordered jsonb_each fold is byte-for-byte the same canonical
-- encoding (including JSON string escaping) rather than merely a semantic
-- JSON comparison.
create or replace function hepta_paper_rework_content_commitment_sha256(
    submission_record jsonb
)
returns text
language sql
immutable
strict
set search_path = pg_catalog
as $function$
    select 'sha256:' || pg_catalog.encode(
        pg_catalog.sha256(
            pg_catalog.convert_to(
                '{' || pg_catalog.string_agg(
                    pg_catalog.to_json(key)::text || ':' || value::text,
                    ',' order by key collate "C"
                ) || '}',
                'UTF8'
            )
        ),
        'hex'
    )
    from pg_catalog.jsonb_each(
        public.hepta_paper_rework_content_projection(submission_record)
    ) as entry(key, value)
$function$;

create or replace function hepta_validate_paper_rework_insert()
returns trigger
language plpgsql
security definer
set search_path = pg_catalog
as $function$
declare
    paper public.hepta_paper_projects%rowtype;
    submission public.hepta_joint_paper_submissions%rowtype;
    evaluation public.hepta_paper_evaluations%rowtype;
    signing_key public.hepta_human_signing_keys%rowtype;
    previous_cycle bigint;
    expected_cycle bigint;
begin
    select * into paper from public.hepta_paper_projects
    where paper_project_id = new.paper_project_id for update;
    if not found then
        raise exception 'Paper rework references a missing Paper';
    end if;
    perform public.hepta_assert_paper_finality_v2_source_unsealed(new.paper_project_id);

    select * into submission from public.hepta_joint_paper_submissions
    where submission_id = new.rejected_submission_id
      and paper_project_id = new.paper_project_id for update;
    select * into evaluation from public.hepta_paper_evaluations
    where evaluation_id = new.rejected_evaluation_id
      and submission_id = new.rejected_submission_id
      and paper_project_id = new.paper_project_id for share;
    select * into signing_key from public.hepta_human_signing_keys
    where player_id = new.author_player_id and signing_key_id = new.signing_key_id
    for share;

    select coalesce(max(rework_cycle), 1) into previous_cycle
    from public.hepta_paper_reworks where paper_project_id = new.paper_project_id;
    if previous_cycle >= 9007199254740991 then
        raise exception using errcode='22003', message='hepta_paper_rework_cycle_overflow';
    end if;
    expected_cycle := previous_cycle + 1;

    if paper.phase <> 'submission_ready' or paper.outcome <> 'submission_ready'
       or paper.terminal_at is null or paper.active_rework_id is not null
       or paper.active_rework_cycle is not null or paper.rework_expires_at is not null
       or submission.status <> 'submission_ready'
       or evaluation.status <> 'rejected'
       or evaluation.release_candidate_hash <> new.rejected_release_candidate_hash
       or evaluation.paper_bundle_hash <> new.rejected_paper_bundle_hash
       or submission.revision_id <> new.rejected_revision_id
       or submission.release_candidate_hash <> new.rejected_release_candidate_hash
       or submission.paper_bundle_hash <> new.rejected_paper_bundle_hash
       or public.hepta_paper_rework_content_commitment_sha256(submission.record_json)
            <> new.rejected_rework_content_commitment_sha256
       or new.rework_cycle <> expected_cycle
       or signing_key.player_id is null or signing_key.status <> 'active'
       or signing_key.retired_at is not null or signing_key.revoked_at is not null
       or signing_key.signing_public_key <> new.signing_public_key
       or signing_key.signing_public_key_hash <> new.signing_public_key_hash
       or not exists (
            select 1 from public.hepta_human_players p
            where p.player_id = new.author_player_id and p.status = 'active'
              and p.signing_key_id = new.signing_key_id
              and p.signing_public_key = new.signing_public_key
              and p.signing_public_key_hash = new.signing_public_key_hash
       )
       or exists (
            select 1 from public.hepta_paper_evaluations child
            where child.supersedes_evaluation_id = new.rejected_evaluation_id
       )
       or exists (
            select 1 from public.hepta_paper_appeals a
            left join public.hepta_paper_appeal_resolutions r on r.appeal_id = a.appeal_id
            where a.evaluation_id = new.rejected_evaluation_id and r.resolution_id is null
       )
       or exists (
            select 1 from public.hepta_paper_review_assignments a
            where a.submission_id = new.rejected_submission_id
              and a.status in ('claimed','pinned')
              and (a.status = 'pinned' or a.expires_at > new.created_at)
       )
       or exists (
            select 1 from public.hepta_paper_reworks w
            left join public.hepta_paper_rework_resubmissions s on s.rework_id = w.rework_id
            where w.paper_project_id = new.paper_project_id and s.rework_id is null
       )
       or not exists (
            select 1
            from jsonb_array_elements(submission.record_json->'paper_bundle'->'author_consents') c
            where (c->>'player_id')::uuid = new.author_player_id
       )
    then
        raise exception using errcode='55000', message='hepta_paper_rework_state_invalid';
    end if;

    if new.record_json->>'schema' is distinct from 'hepta.paper_raid.rework_record.v1'
       or (new.record_json->>'rework_id')::uuid is distinct from new.rework_id
       or (new.record_json->>'paper_project_id')::uuid is distinct from new.paper_project_id
       or (new.record_json->>'rejected_evaluation_id')::uuid is distinct from new.rejected_evaluation_id
       or (new.record_json->>'rejected_submission_id')::uuid is distinct from new.rejected_submission_id
       or (new.record_json->>'rejected_revision_id')::uuid is distinct from new.rejected_revision_id
       or new.record_json->>'rejected_release_candidate_hash' is distinct from new.rejected_release_candidate_hash
       or new.record_json->>'rejected_paper_bundle_hash' is distinct from new.rejected_paper_bundle_hash
       or new.record_json->>'rejected_rework_content_commitment_sha256'
            is distinct from new.rejected_rework_content_commitment_sha256
       or (new.record_json->>'rework_cycle')::bigint is distinct from new.rework_cycle
       or (new.record_json->>'author_player_id')::uuid is distinct from new.author_player_id
       or new.record_json->>'signing_key_id' is distinct from new.signing_key_id
       or new.record_json->>'signing_public_key' is distinct from new.signing_public_key
       or new.record_json->>'signing_public_key_hash' is distinct from new.signing_public_key_hash
       or new.record_json->>'reason_hash' is distinct from new.reason_hash
       or (new.record_json->>'signed_at_unix')::bigint is distinct from new.signed_at_unix
       or new.record_json->>'signature' is distinct from new.signature
       or new.record_json->>'request_hash' is distinct from new.request_hash
       or (new.record_json->>'rework_expires_at')::timestamptz is distinct from new.rework_expires_at
       or (new.record_json->>'version')::bigint is distinct from new.version
       or (new.record_json->>'created_at')::timestamptz is distinct from new.created_at
    then
        raise exception using errcode='55000', message='hepta_paper_rework_record_json_mismatch';
    end if;
    return new;
end;
$function$;

create or replace function hepta_validate_paper_rework_resubmission_insert()
returns trigger
language plpgsql
security definer
set search_path = pg_catalog
as $function$
declare
    paper public.hepta_paper_projects%rowtype;
    rework public.hepta_paper_reworks%rowtype;
    rejected public.hepta_joint_paper_submissions%rowtype;
    replacement public.hepta_joint_paper_submissions%rowtype;
begin
    select * into paper from public.hepta_paper_projects
    where paper_project_id = new.paper_project_id for update;
    perform public.hepta_assert_paper_finality_v2_source_unsealed(new.paper_project_id);
    select * into rework from public.hepta_paper_reworks
    where rework_id = new.rework_id and paper_project_id = new.paper_project_id for share;
    select * into rejected from public.hepta_joint_paper_submissions
    where submission_id = new.rejected_submission_id and paper_project_id = new.paper_project_id for share;
    select * into replacement from public.hepta_joint_paper_submissions
    where submission_id = new.replacement_submission_id and paper_project_id = new.paper_project_id for share;

    if paper.active_rework_id is distinct from new.rework_id
       or paper.active_rework_cycle is distinct from rework.rework_cycle
       or paper.rework_expires_at is distinct from rework.rework_expires_at
       or new.created_at >= rework.rework_expires_at
       or rejected.status <> 'withdrawn'
       or replacement.status <> 'submission_ready'
       or replacement.revision_id <> new.replacement_revision_id
       or replacement.release_candidate_hash <> new.replacement_release_candidate_hash
       or replacement.paper_bundle_hash <> new.replacement_paper_bundle_hash
       or new.rejected_submission_id <> rework.rejected_submission_id
       or new.replacement_release_candidate_hash = rework.rejected_release_candidate_hash
       or new.replacement_paper_bundle_hash = rework.rejected_paper_bundle_hash
       or new.replacement_rework_content_commitment_sha256
            = rework.rejected_rework_content_commitment_sha256
       or public.hepta_paper_rework_content_commitment_sha256(replacement.record_json)
            <> new.replacement_rework_content_commitment_sha256
       or public.hepta_paper_rework_content_projection(replacement.record_json)
            = public.hepta_paper_rework_content_projection(rejected.record_json)
       or new.replacement_review_round <> 1
    then
        raise exception using errcode='55000', message='hepta_paper_rework_resubmission_state_invalid';
    end if;

    if new.record_json->>'schema' is distinct from 'hepta.paper_raid.rework_resubmission.v1'
       or (new.record_json->>'rework_id')::uuid is distinct from new.rework_id
       or (new.record_json->>'paper_project_id')::uuid is distinct from new.paper_project_id
       or (new.record_json->>'rejected_submission_id')::uuid is distinct from new.rejected_submission_id
       or (new.record_json->>'replacement_submission_id')::uuid is distinct from new.replacement_submission_id
       or (new.record_json->>'replacement_revision_id')::uuid is distinct from new.replacement_revision_id
       or new.record_json->>'replacement_release_candidate_hash'
            is distinct from new.replacement_release_candidate_hash
       or new.record_json->>'replacement_paper_bundle_hash'
            is distinct from new.replacement_paper_bundle_hash
       or new.record_json->>'replacement_rework_content_commitment_sha256'
            is distinct from new.replacement_rework_content_commitment_sha256
       or (new.record_json->>'replacement_review_round')::bigint
            is distinct from new.replacement_review_round
       or (new.record_json->>'version')::bigint is distinct from new.version
       or (new.record_json->>'created_at')::timestamptz is distinct from new.created_at
    then
        raise exception using errcode='55000', message='hepta_paper_rework_resubmission_record_json_mismatch';
    end if;
    return new;
end;
$function$;

create or replace function hepta_guard_joint_submission_rework_withdrawal()
returns trigger
language plpgsql
as $function$
begin
    if new is not distinct from old then
        return new;
    end if;
    if old.status <> 'submission_ready' or new.status <> 'withdrawn'
       or new.submission_id <> old.submission_id
       or new.paper_project_id <> old.paper_project_id
       or new.revision_id <> old.revision_id
       or new.release_candidate_hash <> old.release_candidate_hash
       or new.paper_bundle_hash <> old.paper_bundle_hash
       or new.created_at <> old.created_at
       or (new.record_json - 'status') is distinct from (old.record_json - 'status')
       or new.record_json->>'status' is distinct from 'withdrawn'
       or not exists (
            select 1 from public.hepta_paper_reworks w
            where w.rejected_submission_id = old.submission_id
              and w.paper_project_id = old.paper_project_id
       )
    then
        raise exception using errcode='55000', message='hepta_joint_submission_mutation_forbidden';
    end if;
    return new;
end;
$function$;

create or replace function hepta_reject_paper_rework_mutation()
returns trigger
language plpgsql
as $function$
begin
    raise exception using errcode='55000', message='hepta_paper_rework_immutable';
end;
$function$;

-- Finality V2 exposes the complete rework lineage in its immutable binding.
-- This relational check prevents a direct preparation insert from omitting or
-- forging the old/new scientific commitments while retaining a plausible
-- opaque source_fingerprint.
create or replace function hepta_validate_paper_rework_finality_lineage()
returns trigger
language plpgsql
security definer
set search_path = pg_catalog
as $function$
declare
    rework public.hepta_paper_reworks%rowtype;
    resubmission public.hepta_paper_rework_resubmissions%rowtype;
    lineage jsonb;
    has_rework boolean;
begin
    select s.* into resubmission
    from public.hepta_paper_rework_resubmissions s
    where s.paper_project_id = new.paper_project_id
      and s.replacement_submission_id = new.submission_id;
    has_rework := found;
    if has_rework then
        select w.* into strict rework
        from public.hepta_paper_reworks w
        where w.rework_id = resubmission.rework_id;
    end if;

    lineage := new.record_json #> '{binding,rework_lineage}';
    if has_rework then
        if jsonb_typeof(lineage) is distinct from 'object'
           or lineage->>'schema'
                is distinct from 'hepta.paper_raid.trnm_finality_rework_lineage.v1'
           or (lineage->>'rework_id')::uuid is distinct from rework.rework_id
           or (lineage->>'rework_cycle')::bigint is distinct from rework.rework_cycle
           or (lineage->>'rejected_submission_id')::uuid
                is distinct from rework.rejected_submission_id
           or (lineage->>'replacement_submission_id')::uuid
                is distinct from resubmission.replacement_submission_id
           or (lineage->>'rejected_revision_id')::uuid
                is distinct from rework.rejected_revision_id
           or (lineage->>'replacement_revision_id')::uuid
                is distinct from resubmission.replacement_revision_id
           or lineage->>'rejected_release_candidate_hash'
                is distinct from rework.rejected_release_candidate_hash
           or lineage->>'replacement_release_candidate_hash'
                is distinct from resubmission.replacement_release_candidate_hash
           or lineage->>'rejected_paper_bundle_hash'
                is distinct from rework.rejected_paper_bundle_hash
           or lineage->>'replacement_paper_bundle_hash'
                is distinct from resubmission.replacement_paper_bundle_hash
           or lineage->>'rejected_rework_content_commitment_sha256'
                is distinct from rework.rejected_rework_content_commitment_sha256
           or lineage->>'replacement_rework_content_commitment_sha256'
                is distinct from resubmission.replacement_rework_content_commitment_sha256
        then
            raise exception using
                errcode='23514',
                message='hepta_paper_finality_v2_rework_lineage_mismatch';
        end if;
    elsif lineage is not null then
        raise exception using
            errcode='23514',
            message='hepta_paper_finality_v2_unexpected_rework_lineage';
    end if;
    return new;
end;
$function$;

revoke all on function public.hepta_validate_paper_rework_insert() from public;
revoke all on function public.hepta_validate_paper_rework_resubmission_insert() from public;
revoke all on function public.hepta_validate_paper_rework_finality_lineage() from public;

-- Preserve ChallengeRuleset V1 invariants while admitting the exact
-- submission_ready -> in_progress rework transition and independent lease.
create or replace function hepta_guard_paper_challenge_ruleset_v1()
returns trigger
language plpgsql
as $function$
declare
    rework_start boolean := false;
    rework_completion boolean := false;
    active_deadline timestamptz;
begin
    if tg_op = 'UPDATE' and (
        new.challenge_ruleset_snapshot_hash is distinct from old.challenge_ruleset_snapshot_hash
        or new.deadline_at is distinct from old.deadline_at
        or new.grace_expires_at is distinct from old.grace_expires_at
    ) then
        raise exception 'paper challenge ruleset snapshot and deadlines are immutable';
    end if;

    if new.challenge_ruleset_snapshot_hash is not null then
        if not (new.record_json ? 'challenge_ruleset_snapshot')
            or new.record_json->>'challenge_ruleset_snapshot_hash'
                is distinct from new.challenge_ruleset_snapshot_hash
            or (new.record_json->>'deadline_at')::timestamptz is distinct from new.deadline_at
            or (new.record_json->>'grace_expires_at')::timestamptz is distinct from new.grace_expires_at
        then raise exception 'paper record_json ruleset snapshot projection mismatch'; end if;
        if new.record_json->'challenge_ruleset_snapshot'->>'schema'
                is distinct from 'hepta.paper_raid.challenge_ruleset_snapshot.v1'
        then
            raise exception 'paper challenge ruleset snapshot schema mismatch';
        end if;
        if new.record_json->'challenge_ruleset_snapshot'->>'enforcement' = 'authoritative_v1' then
            if jsonb_typeof(new.record_json->'challenge_ruleset_snapshot'->'ruleset')
                    is distinct from 'object'
                or new.deadline_at is null or new.grace_expires_at is null
            then
                raise exception 'authoritative paper challenge requires typed rules and deadlines';
            end if;
        elsif new.record_json->'challenge_ruleset_snapshot'->>'enforcement' = 'legacy_unranked' then
            if jsonb_typeof(new.record_json->'challenge_ruleset_snapshot'->'ruleset')
                    is distinct from 'null'
                or new.deadline_at is not null or new.grace_expires_at is not null
            then
                raise exception 'legacy-unranked paper challenge cannot invent typed rules or deadlines';
            end if;
        else
            raise exception 'paper challenge ruleset enforcement mode is invalid';
        end if;
    end if;

    if coalesce(new.record_json->>'outcome', 'in_progress') is distinct from new.outcome
       or nullif(new.record_json->>'outcome_reason', '') is distinct from new.outcome_reason
       or (new.record_json->>'terminal_at')::timestamptz is distinct from new.terminal_at
       or (new.record_json->>'active_rework_id')::uuid is distinct from new.active_rework_id
       or (new.record_json->>'active_rework_cycle')::bigint is distinct from new.active_rework_cycle
       or (new.record_json->>'rework_expires_at')::timestamptz is distinct from new.rework_expires_at
    then
        raise exception 'paper record_json outcome or rework lease projection mismatch';
    end if;

    if tg_op = 'INSERT' then
        if new.active_rework_id is not null then
            raise exception 'new Paper cannot invent an active rework lease';
        end if;
        return new;
    end if;

    rework_start := old.outcome = 'submission_ready'
        and new.outcome = 'in_progress' and new.phase = 'drafting'
        and old.active_rework_id is null and new.active_rework_id is not null
        and new.terminal_at is null and new.outcome_reason is null
        and exists (
            select 1 from public.hepta_paper_reworks w
            join public.hepta_joint_paper_submissions s on s.submission_id = w.rejected_submission_id
            where w.rework_id = new.active_rework_id
              and w.paper_project_id = new.paper_project_id
              and w.rework_cycle = new.active_rework_cycle
              and w.rework_expires_at = new.rework_expires_at
              and s.status = 'withdrawn'
        );

    rework_completion := old.outcome = 'in_progress' and old.active_rework_id is not null
        and new.outcome = 'submission_ready' and new.phase = 'submission_ready'
        and new.active_rework_id is null and new.active_rework_cycle is null
        and new.rework_expires_at is null
        and exists (
            select 1 from public.hepta_paper_rework_resubmissions s
            where s.rework_id = old.active_rework_id
              and s.paper_project_id = old.paper_project_id
        );

    if old.outcome <> 'in_progress' and (
        new.outcome is distinct from old.outcome
        or new.outcome_reason is distinct from old.outcome_reason
        or new.terminal_at is distinct from old.terminal_at
        or new.phase is distinct from old.phase
    ) and not rework_start then
        raise exception 'paper terminal challenge outcome is immutable except exact Author rework';
    end if;

    if old.active_rework_id is null and new.active_rework_id is not null and not rework_start then
        raise exception 'Paper active rework lease requires exact signed lineage';
    end if;
    if old.active_rework_id is not null and new.outcome = 'in_progress' and (
        new.active_rework_id is distinct from old.active_rework_id
        or new.active_rework_cycle is distinct from old.active_rework_cycle
        or new.rework_expires_at is distinct from old.rework_expires_at
    ) then
        raise exception 'active Paper rework lease is immutable';
    end if;
    if old.active_rework_id is not null and new.active_rework_id is null
       and new.outcome = 'in_progress' then
        raise exception 'active Paper rework lease cannot disappear before terminal outcome';
    end if;

    if old.outcome = 'in_progress' and new.outcome <> old.outcome then
        active_deadline := case when old.active_rework_id is not null
            then old.rework_expires_at else old.grace_expires_at end;
        if new.outcome in ('submission_ready','failed','abandoned')
           and active_deadline is not null and new.terminal_at >= active_deadline then
            raise exception 'Paper non-expiry terminal transition crossed active deadline';
        end if;
        if new.outcome = 'expired' then
            if active_deadline is null or new.terminal_at is distinct from active_deadline then
                raise exception 'Paper expiry must bind exact active deadline';
            end if;
            if old.active_rework_id is not null
               and new.outcome_reason is distinct from 'rework_window_elapsed' then
                raise exception 'Paper rework expiry reason mismatch';
            end if;
        end if;
        if new.outcome = 'submission_ready' and old.active_rework_id is not null
           and not rework_completion then
            raise exception 'Paper rework completion lost immutable resubmission lineage';
        end if;
    end if;
    return new;
end;
$function$;

revoke all on function public.hepta_guard_paper_challenge_ruleset_v1() from public;

drop trigger if exists hepta_validate_paper_rework_insert_trigger on hepta_paper_reworks;
create trigger hepta_validate_paper_rework_insert_trigger
before insert on hepta_paper_reworks
for each row execute function hepta_validate_paper_rework_insert();

drop trigger if exists hepta_validate_paper_rework_resubmission_insert_trigger
    on hepta_paper_rework_resubmissions;
create trigger hepta_validate_paper_rework_resubmission_insert_trigger
before insert on hepta_paper_rework_resubmissions
for each row execute function hepta_validate_paper_rework_resubmission_insert();

drop trigger if exists hepta_paper_reworks_immutable_trigger on hepta_paper_reworks;
create trigger hepta_paper_reworks_immutable_trigger
before update or delete on hepta_paper_reworks
for each row execute function hepta_reject_paper_rework_mutation();

drop trigger if exists hepta_paper_rework_resubmissions_immutable_trigger
    on hepta_paper_rework_resubmissions;
create trigger hepta_paper_rework_resubmissions_immutable_trigger
before update or delete on hepta_paper_rework_resubmissions
for each row execute function hepta_reject_paper_rework_mutation();

drop trigger if exists hepta_joint_submission_rework_withdrawal_trigger
    on hepta_joint_paper_submissions;
create trigger hepta_joint_submission_rework_withdrawal_trigger
before update on hepta_joint_paper_submissions
for each row execute function hepta_guard_joint_submission_rework_withdrawal();

drop trigger if exists hepta_paper_reworks_finality_v2_source_guard on hepta_paper_reworks;
create trigger hepta_paper_reworks_finality_v2_source_guard
before insert or update or delete on hepta_paper_reworks
for each row execute function hepta_reject_paper_finality_v2_source_mutation();

drop trigger if exists hepta_paper_rework_resubmissions_finality_v2_source_guard
    on hepta_paper_rework_resubmissions;
create trigger hepta_paper_rework_resubmissions_finality_v2_source_guard
before insert or update or delete on hepta_paper_rework_resubmissions
for each row execute function hepta_reject_paper_finality_v2_source_mutation();

drop trigger if exists hepta_paper_reworks_truncate_guard on hepta_paper_reworks;
create trigger hepta_paper_reworks_truncate_guard
before truncate on hepta_paper_reworks
for each statement execute function hepta_reject_paper_finality_v2_truncate();

drop trigger if exists hepta_paper_rework_resubmissions_truncate_guard
    on hepta_paper_rework_resubmissions;
create trigger hepta_paper_rework_resubmissions_truncate_guard
before truncate on hepta_paper_rework_resubmissions
for each statement execute function hepta_reject_paper_finality_v2_truncate();

drop trigger if exists hepta_paper_rework_finality_v2_lineage_guard
    on hepta_paper_chain_finality_preparations_v2;
create trigger hepta_paper_rework_finality_v2_lineage_guard
before insert on hepta_paper_chain_finality_preparations_v2
for each row execute function hepta_validate_paper_rework_finality_lineage();

alter table hepta_paper_reworks
    enable always trigger hepta_validate_paper_rework_insert_trigger;
alter table hepta_paper_reworks
    enable always trigger hepta_paper_reworks_immutable_trigger;
alter table hepta_paper_reworks
    enable always trigger hepta_paper_reworks_finality_v2_source_guard;
alter table hepta_paper_reworks
    enable always trigger hepta_paper_reworks_truncate_guard;
alter table hepta_paper_rework_resubmissions
    enable always trigger hepta_validate_paper_rework_resubmission_insert_trigger;
alter table hepta_paper_rework_resubmissions
    enable always trigger hepta_paper_rework_resubmissions_immutable_trigger;
alter table hepta_paper_rework_resubmissions
    enable always trigger hepta_paper_rework_resubmissions_finality_v2_source_guard;
alter table hepta_paper_rework_resubmissions
    enable always trigger hepta_paper_rework_resubmissions_truncate_guard;
alter table hepta_joint_paper_submissions
    enable always trigger hepta_joint_submission_rework_withdrawal_trigger;
alter table hepta_paper_chain_finality_preparations_v2
    enable always trigger hepta_paper_rework_finality_v2_lineage_guard;
alter table hepta_paper_projects
    enable always trigger hepta_guard_paper_challenge_ruleset_v1_trigger;

create index if not exists hepta_paper_projects_active_rework_deadline_idx
    on hepta_paper_projects (rework_expires_at, paper_project_id)
    where outcome='in_progress' and active_rework_id is not null;

comment on column hepta_paper_projects.rework_expires_at is
    'Server-owned 24 hour Author rework lease; never mutates original challenge grace_expires_at.';
comment on column hepta_paper_reworks.rejected_rework_content_commitment_sha256 is
    'Identity-free rejected scientific content commitment; replacement must differ.';

commit;
