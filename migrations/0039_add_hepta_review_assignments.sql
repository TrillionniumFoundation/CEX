-- Independent Review Raid assignment authority.
--
-- Assignment claims retain append-only identity while their lease lifecycle
-- advances from claimed to expired. They intentionally do not grant Research
-- Team or Paper Room membership; they authorize only the frozen review bundle
-- and review projection exposed by the review_v4 API.

create unique index if not exists hepta_joint_paper_submission_project_identity_idx
    on hepta_joint_paper_submissions (submission_id, paper_project_id);

create table if not exists hepta_paper_review_assignments (
    assignment_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    submission_id uuid not null,
    player_id uuid not null references hepta_human_players(player_id),
    review_round bigint not null check (review_round > 0),
    slot text not null check (slot in ('evaluator', 'reviewer_1', 'reviewer_2', 'reproducer')),
    status text not null check (status in ('claimed', 'expired')),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    expires_at timestamptz not null check (expires_at > created_at),
    updated_at timestamptz not null check (updated_at >= created_at),
    foreign key (submission_id, paper_project_id)
        references hepta_joint_paper_submissions(submission_id, paper_project_id) on delete cascade
);

create unique index if not exists hepta_paper_review_assignments_live_slot_idx
    on hepta_paper_review_assignments (paper_project_id, review_round, slot)
    where status = 'claimed';

create unique index if not exists hepta_paper_review_assignments_live_player_idx
    on hepta_paper_review_assignments (paper_project_id, review_round, player_id)
    where status = 'claimed';

create index if not exists hepta_paper_review_assignments_player_idx
    on hepta_paper_review_assignments (player_id, status, expires_at, assignment_id);
