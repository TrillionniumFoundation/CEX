-- Authoritative Paper Raid ChallengeRuleset V1.
--
-- The complete typed ruleset is retained inside PaperProject.record_json.
-- These columns are independently constrained projections used to prevent a
-- Paper from silently switching rules, duration, grace, or terminal outcome.

alter table hepta_paper_projects
    add column if not exists challenge_ruleset_snapshot_hash text,
    add column if not exists deadline_at timestamptz,
    add column if not exists grace_expires_at timestamptz,
    add column if not exists outcome text not null default 'in_progress',
    add column if not exists outcome_reason text,
    add column if not exists terminal_at timestamptz;

update hepta_paper_projects
set outcome = 'submission_ready',
    terminal_at = updated_at,
    record_json = jsonb_set(
        jsonb_set(record_json, '{outcome}', '"submission_ready"'::jsonb, true),
        '{terminal_at}',
        to_jsonb(updated_at),
        true
    )
where phase = 'submission_ready'
  and outcome = 'in_progress';

alter table hepta_paper_projects
    drop constraint if exists hepta_paper_projects_ruleset_snapshot_hash_check,
    add constraint hepta_paper_projects_ruleset_snapshot_hash_check check (
        challenge_ruleset_snapshot_hash is null
        or challenge_ruleset_snapshot_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    drop constraint if exists hepta_paper_projects_deadline_order_check,
    add constraint hepta_paper_projects_deadline_order_check check (
        (deadline_at is null and grace_expires_at is null)
        or (
            deadline_at is not null
            and grace_expires_at is not null
            and grace_expires_at >= deadline_at
        )
    ),
    drop constraint if exists hepta_paper_projects_outcome_check,
    add constraint hepta_paper_projects_outcome_check check (
        outcome in ('in_progress','submission_ready','failed','expired','abandoned')
    ),
    drop constraint if exists hepta_paper_projects_terminal_shape_check,
    add constraint hepta_paper_projects_terminal_shape_check check (
        (
            outcome = 'in_progress'
            and outcome_reason is null
            and terminal_at is null
            and phase <> 'submission_ready'
        )
        or (
            outcome = 'submission_ready'
            and outcome_reason is null
            and terminal_at is not null
            and phase = 'submission_ready'
            and (grace_expires_at is null or terminal_at < grace_expires_at)
        )
        or (
            outcome in ('failed','abandoned')
            and outcome_reason is not null
            and length(outcome_reason) between 1 and 512
            and terminal_at is not null
            and phase <> 'submission_ready'
            and (grace_expires_at is null or terminal_at < grace_expires_at)
        )
        or (
            outcome = 'expired'
            and outcome_reason is not null
            and length(outcome_reason) between 1 and 512
            and terminal_at is not null
            and phase <> 'submission_ready'
            and grace_expires_at is not null
            and terminal_at >= grace_expires_at
        )
    );

create or replace function hepta_guard_paper_challenge_ruleset_v1()
returns trigger
language plpgsql
as $$
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
            or (new.record_json->>'deadline_at')::timestamptz
                is distinct from new.deadline_at
            or (new.record_json->>'grace_expires_at')::timestamptz
                is distinct from new.grace_expires_at
        then
            raise exception 'paper record_json ruleset snapshot projection mismatch';
        end if;
        if new.record_json->'challenge_ruleset_snapshot'->>'schema'
                is distinct from 'hepta.paper_raid.challenge_ruleset_snapshot.v1'
        then
            raise exception 'paper challenge ruleset snapshot schema mismatch';
        end if;
        if new.record_json->'challenge_ruleset_snapshot'->>'enforcement' = 'authoritative_v1' then
            if jsonb_typeof(new.record_json->'challenge_ruleset_snapshot'->'ruleset')
                    is distinct from 'object'
                or new.deadline_at is null
                or new.grace_expires_at is null
            then
                raise exception 'authoritative paper challenge requires typed rules and deadlines';
            end if;
        elsif new.record_json->'challenge_ruleset_snapshot'->>'enforcement' = 'legacy_unranked' then
            if jsonb_typeof(new.record_json->'challenge_ruleset_snapshot'->'ruleset')
                    is distinct from 'null'
                or new.deadline_at is not null
                or new.grace_expires_at is not null
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
    then
        raise exception 'paper record_json outcome projection mismatch';
    end if;

    if tg_op = 'UPDATE' and old.outcome <> 'in_progress' and (
        new.outcome is distinct from old.outcome
        or new.outcome_reason is distinct from old.outcome_reason
        or new.terminal_at is distinct from old.terminal_at
        or new.phase is distinct from old.phase
    ) then
        raise exception 'paper terminal challenge outcome is immutable, including reason, time, and phase';
    end if;
    if tg_op = 'UPDATE' and old.outcome = 'in_progress' and new.outcome <> old.outcome
        and new.outcome not in ('submission_ready','failed','expired','abandoned')
    then
        raise exception 'invalid paper challenge outcome transition';
    end if;
    return new;
end;
$$;

drop trigger if exists hepta_guard_paper_challenge_ruleset_v1_trigger
    on hepta_paper_projects;
create trigger hepta_guard_paper_challenge_ruleset_v1_trigger
before insert or update on hepta_paper_projects
for each row execute function hepta_guard_paper_challenge_ruleset_v1();

create index if not exists hepta_paper_projects_active_deadline_v1_idx
    on hepta_paper_projects (grace_expires_at, paper_project_id)
    where outcome = 'in_progress' and grace_expires_at is not null;

comment on column hepta_paper_projects.challenge_ruleset_snapshot_hash is
    'Canonical hash of the exact typed or legacy-unranked ruleset snapshot frozen at Paper creation.';
comment on column hepta_paper_projects.outcome is
    'Independent gameplay terminal outcome; scientific finality remains in the Paper finality projection.';
