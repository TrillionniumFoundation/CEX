-- Hepta Paper Raid v2 canonical Team + Paper aggregates.
--
-- Unlike the legacy v1 development snapshot, every durable v2 aggregate has
-- its own row and version. Writers use `... where version = expected_version`
-- and the per-command idempotency ledger below; no service-wide advisory lock
-- is required. Domain events are inserted into the existing hepta_outbox in
-- the same PostgreSQL transaction as the aggregate mutation.

create table if not exists hepta_human_players (
    player_id uuid primary key,
    subject_id text not null unique,
    nakama_user_id uuid not null unique,
    signing_key_id text not null,
    signing_public_key text not null,
    signing_public_key_hash text not null,
    status text not null check (status in ('active', 'suspended')),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    updated_at timestamptz not null
);

create table if not exists hepta_human_signing_keys (
    player_id uuid not null references hepta_human_players(player_id) on delete cascade,
    signing_key_id text not null,
    signing_public_key text not null,
    signing_public_key_hash text not null,
    status text not null check (status in ('active', 'rotated', 'revoked')),
    version bigint not null check (version > 0),
    registered_at timestamptz not null,
    retired_at timestamptz,
    revoked_at timestamptz,
    revocation_reason_hash text,
    record_json jsonb not null,
    primary key (player_id, signing_key_id)
);

create unique index if not exists hepta_human_signing_keys_one_active_idx
    on hepta_human_signing_keys (player_id)
    where status = 'active';

create table if not exists hepta_agent_bindings (
    binding_id uuid primary key,
    player_id uuid not null references hepta_human_players(player_id),
    agent_id text not null,
    status text not null check (status in ('active', 'revoked')),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    unique (player_id, agent_id),
    unique (binding_id, player_id)
);

create unique index if not exists hepta_agent_bindings_one_active_agent_idx
    on hepta_agent_bindings (agent_id)
    where status = 'active';

create table if not exists hepta_research_teams (
    team_id uuid primary key,
    challenge_id uuid not null,
    collaboration_compact_hash text not null,
    status text not null check (status in ('forming', 'locked', 'archived')),
    roster_version bigint not null check (roster_version > 0),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    updated_at timestamptz not null
);

create table if not exists hepta_research_team_members (
    team_id uuid not null references hepta_research_teams(team_id) on delete cascade,
    participant_slot integer not null check (participant_slot between 1 and 5),
    player_id uuid not null references hepta_human_players(player_id),
    binding_id uuid not null,
    role text not null,
    joined_at timestamptz not null,
    primary key (team_id, participant_slot),
    unique (team_id, player_id),
    unique (team_id, binding_id),
    foreign key (binding_id, player_id)
        references hepta_agent_bindings(binding_id, player_id)
);

create table if not exists hepta_research_team_member_acceptances (
    acceptance_id uuid primary key,
    team_id uuid not null references hepta_research_teams(team_id) on delete cascade,
    challenge_id uuid not null,
    roster_version bigint not null check (roster_version > 0),
    participant_slot integer not null check (participant_slot between 1 and 5),
    player_id uuid not null references hepta_human_players(player_id),
    binding_id uuid not null,
    collaboration_compact_hash text not null,
    signing_key_id text not null,
    signing_public_key text not null,
    signing_public_key_hash text not null,
    signature text not null,
    record_json jsonb not null,
    accepted_at timestamptz not null,
    superseded_at timestamptz,
    foreign key (team_id, participant_slot)
        references hepta_research_team_members(team_id, participant_slot),
    foreign key (binding_id, player_id)
        references hepta_agent_bindings(binding_id, player_id)
);

create unique index if not exists hepta_team_member_one_current_acceptance_idx
    on hepta_research_team_member_acceptances (team_id, player_id)
    where superseded_at is null;

create unique index if not exists hepta_team_slot_one_current_acceptance_idx
    on hepta_research_team_member_acceptances (team_id, participant_slot)
    where superseded_at is null;

create table if not exists hepta_paper_projects (
    paper_project_id uuid primary key,
    team_id uuid not null references hepta_research_teams(team_id),
    challenge_id uuid not null,
    phase text not null check (phase in (
        'forming', 'preregistering', 'researching', 'experimenting',
        'drafting', 'integrity_review', 'reproducing', 'author_approval',
        'integrity_hold', 'submission_ready'
    )),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    unique (team_id)
);

create table if not exists hepta_paper_work_items (
    work_item_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    assigned_player_id uuid references hepta_human_players(player_id),
    assigned_binding_id uuid,
    status text not null check (status in ('planned', 'in_progress', 'review', 'accepted', 'rejected', 'cancelled')),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    check (
        (assigned_player_id is null and assigned_binding_id is null)
        or (assigned_player_id is not null and assigned_binding_id is not null)
    ),
    foreign key (assigned_binding_id, assigned_player_id)
        references hepta_agent_bindings(binding_id, player_id)
);

create table if not exists hepta_paper_revisions (
    revision_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    parent_revision_id uuid,
    revision_number bigint not null check (revision_number > 0),
    status text not null check (status in ('draft', 'release_candidate', 'superseded')),
    release_candidate_hash text,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    unique (paper_project_id, revision_number),
    unique (revision_id, paper_project_id),
    foreign key (parent_revision_id, paper_project_id)
        references hepta_paper_revisions(revision_id, paper_project_id)
);

create unique index if not exists hepta_paper_one_release_candidate_idx
    on hepta_paper_revisions (paper_project_id)
    where status = 'release_candidate';

create table if not exists hepta_authorship_consents (
    consent_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    revision_id uuid not null,
    player_id uuid not null references hepta_human_players(player_id),
    signing_key_id text not null,
    signing_public_key text not null,
    signing_public_key_hash text not null,
    release_candidate_hash text not null,
    signature text not null,
    record_json jsonb not null,
    signed_at timestamptz not null,
    unique (paper_project_id, revision_id, player_id),
    foreign key (revision_id, paper_project_id)
        references hepta_paper_revisions(revision_id, paper_project_id)
);

create table if not exists hepta_joint_paper_submissions (
    submission_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id),
    revision_id uuid not null,
    release_candidate_hash text not null,
    paper_bundle_hash text not null unique,
    status text not null check (status in ('submission_ready', 'integrity_hold', 'withdrawn')),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (paper_project_id, revision_id),
    foreign key (revision_id, paper_project_id)
        references hepta_paper_revisions(revision_id, paper_project_id)
);

create unique index if not exists hepta_joint_paper_one_ready_submission_idx
    on hepta_joint_paper_submissions (paper_project_id)
    where status = 'submission_ready';

create table if not exists hepta_paper_raid_idempotency (
    operation text not null,
    idempotency_key text not null,
    request_hash text not null,
    aggregate_id uuid,
    response_status integer not null,
    response_json jsonb not null,
    created_at timestamptz not null default now(),
    primary key (operation, idempotency_key)
);

create index if not exists hepta_paper_raid_idempotency_created_idx
    on hepta_paper_raid_idempotency (created_at);

create unique index if not exists hepta_paper_projects_project_team_idx
    on hepta_paper_projects (paper_project_id, team_id);

create table if not exists hepta_research_session_authorization_sets (
    authorization_set_id uuid primary key,
    session_id text not null,
    team_id uuid not null references hepta_research_teams(team_id),
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id),
    challenge_id uuid not null,
    team_roster_version bigint not null check (team_roster_version > 0),
    roster_version bigint not null check (roster_version > 0),
    roster_root text not null,
    supersedes_roster_version bigint,
    replaced_participant_slot integer check (replaced_participant_slot between 1 and 5),
    status text not null check (status in ('issued', 'consumed', 'completed', 'superseded', 'expired')),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    issued_at timestamptz not null,
    expires_at timestamptz not null,
    consumed_at timestamptz,
    unique (session_id, roster_version),
    unique (session_id, roster_version, roster_root),
    check (
        (roster_version = 1 and supersedes_roster_version is null and replaced_participant_slot is null)
        or (
            roster_version > 1
            and supersedes_roster_version = roster_version - 1
            and replaced_participant_slot is not null
        )
    ),
    foreign key (paper_project_id, team_id)
        references hepta_paper_projects(paper_project_id, team_id)
);

-- Team roster_version is an immutable authorship roster snapshot. The
-- roster_version here is the per-logical-session authorization epoch. History
-- is retained, but only one epoch may be live for a session and paper.
create unique index if not exists hepta_research_session_one_live_epoch_idx
    on hepta_research_session_authorization_sets (session_id)
    where status in ('issued', 'consumed');

create unique index if not exists hepta_research_paper_one_live_session_epoch_idx
    on hepta_research_session_authorization_sets (paper_project_id)
    where status in ('issued', 'consumed');

create table if not exists hepta_research_session_authorizations (
    authorization_id uuid primary key,
    authorization_set_id uuid not null references hepta_research_session_authorization_sets(authorization_set_id) on delete cascade,
    session_id text not null,
    roster_version bigint not null check (roster_version > 0),
    participant_slot integer not null check (participant_slot between 1 and 5),
    player_id uuid not null references hepta_human_players(player_id),
    binding_id uuid not null,
    agent_id text not null,
    consumed_at timestamptz,
    record_json jsonb not null,
    unique (authorization_set_id, participant_slot),
    unique (authorization_set_id, player_id),
    unique (authorization_set_id, agent_id),
    foreign key (session_id, roster_version)
        references hepta_research_session_authorization_sets(session_id, roster_version),
    foreign key (binding_id, player_id)
        references hepta_agent_bindings(binding_id, player_id)
);

create table if not exists hepta_research_session_consumption_receipts (
    authorization_set_id uuid primary key references hepta_research_session_authorization_sets(authorization_set_id),
    session_id text not null,
    roster_version bigint not null check (roster_version > 0),
    roster_root text not null,
    receipt_hash text not null unique,
    record_json jsonb not null,
    consumed_at timestamptz not null,
    foreign key (session_id, roster_version)
        references hepta_research_session_authorization_sets(session_id, roster_version)
);

create table if not exists hepta_nakama_research_session_completions (
    commitment_id text primary key,
    authorization_set_id uuid not null unique references hepta_research_session_authorization_sets(authorization_set_id),
    session_id text not null,
    team_id uuid not null references hepta_research_teams(team_id),
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id),
    challenge_id uuid not null,
    roster_version bigint not null check (roster_version > 0),
    roster_root text not null,
    event_count bigint not null check (event_count > 0),
    event_root text not null,
    archive_hash text not null,
    ruleset_hash text not null,
    challenge_snapshot_hash text not null,
    authority_key_id text not null,
    record_json jsonb not null,
    verified_at timestamptz not null,
    foreign key (session_id, roster_version)
        references hepta_research_session_authorization_sets(session_id, roster_version),
    foreign key (paper_project_id, team_id)
        references hepta_paper_projects(paper_project_id, team_id)
);
