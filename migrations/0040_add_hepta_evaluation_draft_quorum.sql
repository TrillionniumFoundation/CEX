-- Independent Review Raid three-party evaluation aggregation.
--
-- Evaluator-authored scientific fields and signatures are immutable. The
-- draft row has two permitted lifecycle transitions, open -> finalized or
-- open -> expired after its server-owned deadline; the
-- two reviewer attestations are append-only and occupy distinct assigned
-- reviewer slots. Application-level assignment checks remain authoritative,
-- while these constraints close duplicate and concurrent write races.

alter table hepta_paper_review_assignments
    drop constraint if exists hepta_paper_review_assignments_status_check;
alter table hepta_paper_review_assignments
    add constraint hepta_paper_review_assignments_status_check
    check (status in ('claimed', 'pinned', 'consumed', 'expired'));

-- A draft pins its evaluator lease and each accepted attestation pins that
-- reviewer lease. Pinned seats outlive their original assignment deadline,
-- but only until the immutable draft deadline. Finalization consumes them;
-- an expired draft releases only its exact pinned panel.
drop index if exists hepta_paper_review_assignments_live_slot_idx;
create unique index hepta_paper_review_assignments_live_slot_idx
    on hepta_paper_review_assignments (paper_project_id, review_round, slot)
    where status in ('claimed', 'pinned');

drop index if exists hepta_paper_review_assignments_live_player_idx;
create unique index hepta_paper_review_assignments_live_player_idx
    on hepta_paper_review_assignments (paper_project_id, review_round, player_id)
    where status in ('claimed', 'pinned');

create or replace function hepta_guard_review_assignment_draft_lifecycle_v1()
returns trigger language plpgsql as $$
begin
    if not (
           (old.status = 'claimed' and new.status = 'expired'
                and old.pinned_evaluation_id is null
                and new.pinned_evaluation_id is null
                and new.updated_at >= old.expires_at)
        or (old.status = 'claimed' and new.status = 'pinned'
                and old.pinned_evaluation_id is null
                and new.pinned_evaluation_id is not null
                and new.updated_at < old.expires_at
                and exists (
                    select 1
                    from hepta_paper_evaluation_drafts d
                    where d.evaluation_id = new.pinned_evaluation_id
                      and d.paper_project_id = new.paper_project_id
                      and d.submission_id = new.submission_id
                      and d.review_round = new.review_round
                      and d.status = 'open'
                      and (
                            (new.slot = 'evaluator'
                             and d.evaluator_player_id = new.player_id)
                         or (new.slot in ('reviewer_1', 'reviewer_2') and exists (
                                select 1
                                from hepta_paper_evaluation_draft_attestations a
                                where a.evaluation_id = d.evaluation_id
                                  and a.paper_project_id = new.paper_project_id
                                  and a.submission_id = new.submission_id
                                  and a.review_round = new.review_round
                                  and a.slot = new.slot
                                  and a.reviewer_player_id = new.player_id
                            ))
                          )
                ))
        or (old.status = 'pinned' and new.status = 'consumed'
                and new.pinned_evaluation_id = old.pinned_evaluation_id
                and exists (
                    select 1
                    from hepta_paper_evaluations evaluation
                    where evaluation.evaluation_id = old.pinned_evaluation_id
                      and evaluation.paper_project_id = new.paper_project_id
                      and evaluation.submission_id = new.submission_id
                      and evaluation.version = new.review_round
                ))
        or (old.status = 'pinned' and new.status = 'expired' and exists (
            select 1
            from hepta_paper_evaluation_drafts d
            where d.evaluation_id = old.pinned_evaluation_id
              and new.pinned_evaluation_id = old.pinned_evaluation_id
              and d.paper_project_id = new.paper_project_id
              and d.submission_id = new.submission_id
              and d.review_round = new.review_round
              and d.status = 'expired'
              and d.expires_at <= new.updated_at
              and (
                    (new.slot = 'evaluator'
                     and d.evaluator_player_id = new.player_id)
                 or (new.slot in ('reviewer_1', 'reviewer_2') and exists (
                        select 1
                        from hepta_paper_evaluation_draft_attestations a
                        where a.evaluation_id = d.evaluation_id
                          and a.paper_project_id = new.paper_project_id
                          and a.submission_id = new.submission_id
                          and a.review_round = new.review_round
                          and a.slot = new.slot
                          and a.reviewer_player_id = new.player_id
                    ))
                  )
        ))
       )
       or new.version <> old.version + 1
       or new.assignment_id <> old.assignment_id
       or new.paper_project_id <> old.paper_project_id
       or new.submission_id <> old.submission_id
       or new.player_id <> old.player_id
       or new.review_round <> old.review_round
       or new.slot <> old.slot
       or (old.status <> 'claimed'
           and new.pinned_evaluation_id is distinct from old.pinned_evaluation_id)
       or new.created_at <> old.created_at
       or new.expires_at <> old.expires_at
       or new.updated_at < old.updated_at
       or (new.record_json - array['status','pinned_evaluation_id','version','updated_at'])
          is distinct from
          (old.record_json - array['status','pinned_evaluation_id','version','updated_at'])
    then
        raise exception 'review assignment identity or draft lifecycle changed'
            using errcode = '55000';
    end if;
    return new;
end;
$$;

drop trigger if exists hepta_review_assignment_draft_lifecycle_guard
    on hepta_paper_review_assignments;

create table if not exists hepta_paper_evaluation_drafts (
    evaluation_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    submission_id uuid not null,
    review_round bigint not null check (review_round > 0),
    supersedes_evaluation_id uuid null references hepta_paper_evaluations(evaluation_id),
    evaluator_player_id uuid not null references hepta_human_players(player_id),
    draft_hash text not null check (draft_hash ~ '^sha256:[0-9a-f]{64}$'),
    evaluation_signing_hash text not null check (evaluation_signing_hash ~ '^sha256:[0-9a-f]{64}$'),
    status text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    expires_at timestamptz not null,
    updated_at timestamptz not null check (updated_at >= created_at),
    finalized_at timestamptz null,
    expired_at timestamptz null,
    foreign key (submission_id, paper_project_id)
        references hepta_joint_paper_submissions(submission_id, paper_project_id) on delete cascade
);

-- 0040 may already have been applied on an unreleased alpha database. Keep
-- this file repeatable: old v1 records receive the same fixed lease they would
-- have received at creation, while their scientific hash remains v1.
alter table hepta_paper_evaluation_drafts
    add column if not exists expires_at timestamptz null;
alter table hepta_paper_evaluation_drafts
    add column if not exists expired_at timestamptz null;

update hepta_paper_evaluation_drafts
set expires_at = created_at + interval '24 hours'
where expires_at is null;

update hepta_paper_evaluation_drafts
set record_json = jsonb_set(
        jsonb_set(record_json, '{expires_at}', to_jsonb(expires_at), true),
        '{expired_at}', coalesce(record_json -> 'expired_at', 'null'::jsonb), true
    )
where record_json ->> 'expires_at' is null
   or not (record_json ? 'expired_at');

alter table hepta_paper_evaluation_drafts
    alter column expires_at set not null;

do $$
declare
    constraint_record record;
begin
    for constraint_record in
        select c.conname
        from pg_constraint c
        where c.conrelid = 'hepta_paper_evaluation_drafts'::regclass
          and c.contype = 'c'
          and pg_get_constraintdef(c.oid) like '%status%open%finalized%'
    loop
        execute format(
            'alter table hepta_paper_evaluation_drafts drop constraint %I',
            constraint_record.conname
        );
    end loop;
end;
$$;

alter table hepta_paper_evaluation_drafts
    drop constraint if exists hepta_paper_evaluation_drafts_status_check;
alter table hepta_paper_evaluation_drafts
    drop constraint if exists hepta_paper_evaluation_drafts_lease_check;
alter table hepta_paper_evaluation_drafts
    drop constraint if exists hepta_paper_evaluation_drafts_lifecycle_check;
alter table hepta_paper_evaluation_drafts
    add constraint hepta_paper_evaluation_drafts_status_check
    check (status in ('open', 'finalized', 'expired'));
alter table hepta_paper_evaluation_drafts
    add constraint hepta_paper_evaluation_drafts_lease_check
    check (expires_at > created_at);
alter table hepta_paper_evaluation_drafts
    add constraint hepta_paper_evaluation_drafts_lifecycle_check
    check (
        (status = 'open' and version = 1 and finalized_at is null and expired_at is null)
        or (status = 'finalized' and version = 2 and finalized_at is not null and expired_at is null)
        or (status = 'expired' and version = 2 and finalized_at is null and expired_at = expires_at)
    );

drop index if exists hepta_paper_evaluation_drafts_round_idx;
create unique index hepta_paper_evaluation_drafts_round_idx
    on hepta_paper_evaluation_drafts (paper_project_id, submission_id, review_round)
    where status in ('open', 'finalized');

create unique index if not exists hepta_paper_evaluation_drafts_scope_identity_idx
    on hepta_paper_evaluation_drafts
       (evaluation_id, paper_project_id, submission_id, review_round);

create table if not exists hepta_paper_evaluation_draft_attestations (
    attestation_id uuid primary key,
    evaluation_id uuid not null,
    paper_project_id uuid not null,
    submission_id uuid not null,
    review_round bigint not null check (review_round > 0),
    slot text not null check (slot in ('reviewer_1', 'reviewer_2')),
    reviewer_player_id uuid not null references hepta_human_players(player_id),
    draft_hash text not null check (draft_hash ~ '^sha256:[0-9a-f]{64}$'),
    evaluation_signing_hash text not null check (evaluation_signing_hash ~ '^sha256:[0-9a-f]{64}$'),
    record_json jsonb not null,
    created_at timestamptz not null,
    foreign key (evaluation_id, paper_project_id, submission_id, review_round)
        references hepta_paper_evaluation_drafts
          (evaluation_id, paper_project_id, submission_id, review_round)
        on delete cascade
);

create unique index if not exists hepta_paper_evaluation_draft_attestations_slot_idx
    on hepta_paper_evaluation_draft_attestations (evaluation_id, slot);

create unique index if not exists hepta_paper_evaluation_draft_attestations_reviewer_idx
    on hepta_paper_evaluation_draft_attestations (evaluation_id, reviewer_player_id);

alter table hepta_paper_review_assignments
    add column if not exists pinned_evaluation_id uuid null;

update hepta_paper_review_assignments a
set pinned_evaluation_id = d.evaluation_id
from hepta_paper_evaluation_drafts d
where a.pinned_evaluation_id is null
  and a.status in ('pinned', 'consumed')
  and a.slot = 'evaluator'
  and d.paper_project_id = a.paper_project_id
  and d.submission_id = a.submission_id
  and d.review_round = a.review_round
  and d.evaluator_player_id = a.player_id;

update hepta_paper_review_assignments a
set pinned_evaluation_id = attestation.evaluation_id
from hepta_paper_evaluation_draft_attestations attestation
where a.pinned_evaluation_id is null
  and a.status in ('pinned', 'consumed')
  and a.slot in ('reviewer_1', 'reviewer_2')
  and attestation.paper_project_id = a.paper_project_id
  and attestation.submission_id = a.submission_id
  and attestation.review_round = a.review_round
  and attestation.slot = a.slot
  and attestation.reviewer_player_id = a.player_id;

do $$
begin
    if exists (
        select 1
        from hepta_paper_review_assignments
        where status in ('pinned', 'consumed')
          and pinned_evaluation_id is null
    ) then
        raise exception 'cannot infer pinned evaluation identity for existing assignment'
            using errcode = '55000';
    end if;
end;
$$;

update hepta_paper_review_assignments
set record_json = jsonb_set(
    record_json,
    '{pinned_evaluation_id}',
    coalesce(to_jsonb(pinned_evaluation_id), 'null'::jsonb),
    true
)
where not (record_json ? 'pinned_evaluation_id')
   or record_json ->> 'pinned_evaluation_id'
      is distinct from pinned_evaluation_id::text;

alter table hepta_paper_review_assignments
    drop constraint if exists hepta_paper_review_assignments_pin_state_check;
alter table hepta_paper_review_assignments
    add constraint hepta_paper_review_assignments_pin_state_check
    check (
        (status = 'claimed' and pinned_evaluation_id is null)
        or status = 'expired'
        or (status in ('pinned', 'consumed') and pinned_evaluation_id is not null)
    );
alter table hepta_paper_review_assignments
    drop constraint if exists hepta_paper_review_assignments_pinned_evaluation_fkey;
alter table hepta_paper_review_assignments
    add constraint hepta_paper_review_assignments_pinned_evaluation_fkey
    foreign key (pinned_evaluation_id)
    references hepta_paper_evaluation_drafts(evaluation_id);

create trigger hepta_review_assignment_draft_lifecycle_guard
before update on hepta_paper_review_assignments
for each row execute function hepta_guard_review_assignment_draft_lifecycle_v1();

create or replace function hepta_guard_evaluation_draft_lifecycle_v1()
returns trigger language plpgsql as $$
begin
    if not (
           (old.status = 'open'
            and new.status = 'finalized'
            and new.finalized_at is not null
            and new.expired_at is null
            and new.updated_at = new.finalized_at
            and new.record_json ->> 'finalized_evaluation_id' = new.evaluation_id::text)
        or (old.status = 'open'
            and new.status = 'expired'
            and new.finalized_at is null
            and new.expired_at = old.expires_at
            and new.updated_at = new.expired_at
            and new.record_json ->> 'finalized_evaluation_id' is null)
       )
       or new.version <> old.version + 1
       or new.evaluation_id <> old.evaluation_id
       or new.paper_project_id <> old.paper_project_id
       or new.submission_id <> old.submission_id
       or new.review_round <> old.review_round
       or new.supersedes_evaluation_id is distinct from old.supersedes_evaluation_id
       or new.evaluator_player_id <> old.evaluator_player_id
       or new.draft_hash <> old.draft_hash
       or new.evaluation_signing_hash <> old.evaluation_signing_hash
       or new.created_at <> old.created_at
       or new.expires_at <> old.expires_at
       or (new.record_json - array['status','version','finalized_evaluation_id','updated_at','finalized_at','expired_at'])
          is distinct from
          (old.record_json - array['status','version','finalized_evaluation_id','updated_at','finalized_at','expired_at'])
    then
        raise exception 'evaluation draft immutable fields or lifecycle transition changed'
            using errcode = '55000';
    end if;
    return new;
end;
$$;

create or replace function hepta_reject_evaluation_draft_delete_v1()
returns trigger language plpgsql as $$
begin
    raise exception 'evaluation draft records are append-only'
        using errcode = '55000';
end;
$$;

create or replace function hepta_reject_evaluation_draft_truncate_v1()
returns trigger language plpgsql as $$
begin
    raise exception 'evaluation draft tables cannot be truncated'
        using errcode = '55000';
end;
$$;

drop trigger if exists hepta_evaluation_draft_lifecycle_guard
    on hepta_paper_evaluation_drafts;
create trigger hepta_evaluation_draft_lifecycle_guard
before update on hepta_paper_evaluation_drafts
for each row execute function hepta_guard_evaluation_draft_lifecycle_v1();

drop trigger if exists hepta_evaluation_draft_delete_guard
    on hepta_paper_evaluation_drafts;
create trigger hepta_evaluation_draft_delete_guard
before delete on hepta_paper_evaluation_drafts
for each row execute function hepta_reject_evaluation_draft_delete_v1();

drop trigger if exists hepta_evaluation_draft_truncate_guard
    on hepta_paper_evaluation_drafts;
create trigger hepta_evaluation_draft_truncate_guard
before truncate on hepta_paper_evaluation_drafts
for each statement execute function hepta_reject_evaluation_draft_truncate_v1();

drop trigger if exists hepta_evaluation_draft_attestation_update_guard
    on hepta_paper_evaluation_draft_attestations;
create trigger hepta_evaluation_draft_attestation_update_guard
before update on hepta_paper_evaluation_draft_attestations
for each row execute function hepta_reject_evaluation_draft_delete_v1();

drop trigger if exists hepta_evaluation_draft_attestation_delete_guard
    on hepta_paper_evaluation_draft_attestations;
create trigger hepta_evaluation_draft_attestation_delete_guard
before delete on hepta_paper_evaluation_draft_attestations
for each row execute function hepta_reject_evaluation_draft_delete_v1();

drop trigger if exists hepta_evaluation_draft_attestation_truncate_guard
    on hepta_paper_evaluation_draft_attestations;
create trigger hepta_evaluation_draft_attestation_truncate_guard
before truncate on hepta_paper_evaluation_draft_attestations
for each statement execute function hepta_reject_evaluation_draft_truncate_v1();
