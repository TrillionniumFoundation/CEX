-- Paper Raid P5: immutable, Paper-scoped evaluation/reproduction/appeal
-- authority.  These tables intentionally do not reuse the legacy singleton
-- workflow maps or the generic 0031 submission evaluation tables.

create table if not exists hepta_paper_contribution_ledgers (
    contribution_ledger_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    release_candidate_hash text not null,
    ledger_hash text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (paper_project_id, release_candidate_hash),
    unique (paper_project_id, ledger_hash)
);

create table if not exists hepta_paper_evaluations (
    evaluation_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    submission_id uuid not null references hepta_joint_paper_submissions(submission_id),
    release_candidate_hash text not null,
    paper_bundle_hash text not null,
    supersedes_evaluation_id uuid references hepta_paper_evaluations(evaluation_id),
    evaluator_player_id uuid not null references hepta_human_players(player_id),
    tolerance_policy_hash text not null,
    paper_score_hash text not null,
    score_bps integer not null check (score_bps between 0 and 10000),
    eligible boolean not null,
    status text not null check (status in ('accepted', 'rejected', 'not_eligible')),
    settlement_state text not null check (settlement_state in ('pending_finality', 'challenged', 'resolved')),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (evaluation_id, paper_project_id),
    check (supersedes_evaluation_id is null or supersedes_evaluation_id <> evaluation_id)
);

create unique index if not exists hepta_paper_initial_evaluation_idx
    on hepta_paper_evaluations (submission_id)
    where supersedes_evaluation_id is null;

create unique index if not exists hepta_paper_evaluation_supersedes_idx
    on hepta_paper_evaluations (supersedes_evaluation_id)
    where supersedes_evaluation_id is not null;

create table if not exists hepta_paper_evaluation_panel_attestations (
    attestation_id uuid primary key,
    evaluation_id uuid not null,
    paper_project_id uuid not null,
    reviewer_player_id uuid not null references hepta_human_players(player_id),
    verdict text not null check (verdict in ('approve', 'reject')),
    coi_attestation_hash text not null,
    signing_key_id text not null,
    signing_public_key_hash text not null,
    signature text not null,
    record_json jsonb not null,
    signed_at timestamptz not null,
    unique (evaluation_id, reviewer_player_id),
    foreign key (evaluation_id, paper_project_id)
        references hepta_paper_evaluations(evaluation_id, paper_project_id) on delete cascade
);

create table if not exists hepta_paper_scores (
    evaluation_id uuid primary key references hepta_paper_evaluations(evaluation_id) on delete cascade,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    score_bps integer not null check (score_bps between 0 and 10000),
    eligible boolean not null,
    score_hash text not null,
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (evaluation_id, paper_project_id)
);

create table if not exists hepta_paper_raid_scores (
    raid_score_id uuid primary key,
    evaluation_id uuid not null references hepta_paper_evaluations(evaluation_id) on delete cascade,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    team_xp bigint not null check (team_xp >= 0),
    player_xp_json jsonb not null,
    score_hash text not null,
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (evaluation_id)
);

create table if not exists hepta_paper_reproductions (
    reproduction_id uuid primary key,
    evaluation_id uuid not null,
    paper_project_id uuid not null,
    reproducer_player_id uuid not null references hepta_human_players(player_id),
    release_candidate_hash text not null,
    paper_bundle_hash text not null,
    tolerance_policy_hash text not null,
    reproduced boolean not null,
    supersedes_reproduction_id uuid references hepta_paper_reproductions(reproduction_id),
    report_hash text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (reproduction_id, paper_project_id),
    foreign key (evaluation_id, paper_project_id)
        references hepta_paper_evaluations(evaluation_id, paper_project_id) on delete cascade,
    check (supersedes_reproduction_id is null or supersedes_reproduction_id <> reproduction_id)
);

create unique index if not exists hepta_paper_reproduction_initial_idx
    on hepta_paper_reproductions (evaluation_id, reproducer_player_id)
    where supersedes_reproduction_id is null;

create unique index if not exists hepta_paper_reproduction_supersedes_idx
    on hepta_paper_reproductions (supersedes_reproduction_id)
    where supersedes_reproduction_id is not null;

create table if not exists hepta_paper_appeals (
    appeal_id uuid primary key,
    evaluation_id uuid not null,
    paper_project_id uuid not null,
    appellant_player_id uuid not null references hepta_human_players(player_id),
    release_candidate_hash text not null,
    grounds_hash text not null,
    evidence_manifest_hash text not null,
    signature text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (evaluation_id),
    unique (appeal_id, paper_project_id),
    foreign key (evaluation_id, paper_project_id)
        references hepta_paper_evaluations(evaluation_id, paper_project_id) on delete cascade
);

create table if not exists hepta_paper_appeal_resolutions (
    resolution_id uuid primary key,
    appeal_id uuid not null,
    paper_project_id uuid not null,
    resolver_player_id uuid not null references hepta_human_players(player_id),
    outcome text not null check (outcome in ('upheld', 'denied')),
    superseding_evaluation_id uuid references hepta_paper_evaluations(evaluation_id),
    decision_hash text not null,
    signature text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (appeal_id),
    foreign key (appeal_id, paper_project_id)
        references hepta_paper_appeals(appeal_id, paper_project_id) on delete cascade,
    check (
        (outcome = 'upheld' and superseding_evaluation_id is not null)
        or (outcome = 'denied' and superseding_evaluation_id is null)
    )
);

create index if not exists hepta_paper_evaluations_project_idx
    on hepta_paper_evaluations (paper_project_id, created_at, evaluation_id);
create index if not exists hepta_paper_reproductions_project_idx
    on hepta_paper_reproductions (paper_project_id, created_at, reproduction_id);
create index if not exists hepta_paper_appeals_project_idx
    on hepta_paper_appeals (paper_project_id, created_at, appeal_id);
