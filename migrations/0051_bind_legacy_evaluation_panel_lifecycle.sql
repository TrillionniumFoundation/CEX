-- Bind the legacy one-shot evaluation endpoint to the same frozen Review
-- assignment lifecycle used by draft quorum finalization.
--
-- A claimed evaluator/reviewer panel may be pinned directly to an already
-- inserted immutable evaluation only when every Paper, submission, round,
-- slot and player identity matches that evaluation's durable panel. The
-- service immediately consumes all three rows in the same transaction.

begin;

-- A pinned assignment can now reference either an open evaluation draft or
-- its immutable legacy evaluation. PostgreSQL cannot express that union with
-- one foreign key; the ALWAYS lifecycle trigger below is the relational gate,
-- and pinned_evaluation_id remains immutable after the claimed transition.
alter table hepta_paper_review_assignments
    drop constraint if exists hepta_paper_review_assignments_pinned_evaluation_fkey;

create or replace function hepta_guard_review_assignment_draft_lifecycle_v1()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $function$
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
                and (
                    exists (
                        select 1
                        from public.hepta_paper_evaluation_drafts d
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
                                    from public.hepta_paper_evaluation_draft_attestations a
                                    where a.evaluation_id = d.evaluation_id
                                      and a.paper_project_id = new.paper_project_id
                                      and a.submission_id = new.submission_id
                                      and a.review_round = new.review_round
                                      and a.slot = new.slot
                                      and a.reviewer_player_id = new.player_id
                                ))
                              )
                    )
                    or exists (
                        select 1
                        from public.hepta_paper_evaluations evaluation
                        where evaluation.evaluation_id = new.pinned_evaluation_id
                          and evaluation.paper_project_id = new.paper_project_id
                          and evaluation.submission_id = new.submission_id
                          and evaluation.version = new.review_round
                          and (
                                (new.slot = 'evaluator'
                                 and evaluation.evaluator_player_id = new.player_id)
                             or (new.slot in ('reviewer_1', 'reviewer_2') and exists (
                                    select 1
                                    from public.hepta_paper_evaluation_panel_attestations a
                                    where a.evaluation_id = evaluation.evaluation_id
                                      and a.paper_project_id = new.paper_project_id
                                      and a.reviewer_player_id = new.player_id
                                ))
                              )
                    )
                ))
        or (old.status = 'pinned' and new.status = 'consumed'
                and new.pinned_evaluation_id = old.pinned_evaluation_id
                and exists (
                    select 1
                    from public.hepta_paper_evaluations evaluation
                    where evaluation.evaluation_id = old.pinned_evaluation_id
                      and evaluation.paper_project_id = new.paper_project_id
                      and evaluation.submission_id = new.submission_id
                      and evaluation.version = new.review_round
                ))
        or (old.status = 'pinned' and new.status = 'expired' and exists (
            select 1
            from public.hepta_paper_evaluation_drafts d
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
                        from public.hepta_paper_evaluation_draft_attestations a
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
        raise exception 'review assignment identity or frozen panel lifecycle changed'
            using errcode = '55000';
    end if;
    return new;
end;
$function$;

drop trigger if exists hepta_review_assignment_draft_lifecycle_guard
    on hepta_paper_review_assignments;
create trigger hepta_review_assignment_draft_lifecycle_guard
before update on hepta_paper_review_assignments
for each row execute function hepta_guard_review_assignment_draft_lifecycle_v1();

alter table hepta_paper_review_assignments
    enable always trigger hepta_review_assignment_draft_lifecycle_guard;

commit;
