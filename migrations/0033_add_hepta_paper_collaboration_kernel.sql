-- Hepta Paper Raid v3 collaboration kernel.
--
-- Artifact bytes remain in external content-addressed storage or Git. These
-- tables retain only canonical metadata, hashes, immutable provenance and
-- versioned decisions. Every command also uses
-- hepta_paper_raid_idempotency and writes hepta_outbox in the same
-- transaction; no record below is stored in the legacy singleton JSON state.

create table if not exists hepta_matchmaking_tickets (
    ticket_id uuid primary key,
    player_id uuid not null references hepta_human_players(player_id),
    challenge_id uuid not null,
    requested_team_size integer not null check (requested_team_size between 3 and 5),
    roles text[] not null check (cardinality(roles) > 0),
    availability_hash text not null,
    status text not null check (status in ('queued', 'matched', 'cancelled', 'expired')),
    matched_proposal_id uuid,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    updated_at timestamptz not null
);

create unique index if not exists hepta_matchmaking_one_live_ticket_idx
    on hepta_matchmaking_tickets (player_id, challenge_id)
    where status = 'queued';

create index if not exists hepta_matchmaking_queue_idx
    on hepta_matchmaking_tickets (challenge_id, requested_team_size, created_at, ticket_id)
    where status = 'queued';

create table if not exists hepta_team_proposals (
    proposal_id uuid primary key,
    challenge_id uuid not null,
    requested_team_size integer not null check (requested_team_size between 3 and 5),
    deterministic_match_key text not null unique,
    status text not null check (status in ('proposed', 'accepted', 'declined', 'expired')),
    member_player_ids uuid[] not null check (
        cardinality(member_player_ids) between 3 and 5
        and cardinality(member_player_ids) = requested_team_size
    ),
    source_ticket_ids uuid[] not null check (
        cardinality(source_ticket_ids) between 3 and 5
        and cardinality(source_ticket_ids) = requested_team_size
    ),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    updated_at timestamptz not null
);

create table if not exists hepta_team_proposal_decisions (
    decision_id uuid primary key,
    proposal_id uuid not null references hepta_team_proposals(proposal_id) on delete cascade,
    player_id uuid not null references hepta_human_players(player_id),
    decision text not null check (decision in ('accept', 'decline')),
    proposal_version bigint not null check (proposal_version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (proposal_id, player_id)
);

alter table hepta_matchmaking_tickets
    drop constraint if exists hepta_matchmaking_tickets_matched_proposal_id_fkey;
alter table hepta_matchmaking_tickets
    add constraint hepta_matchmaking_tickets_matched_proposal_id_fkey
    foreign key (matched_proposal_id) references hepta_team_proposals(proposal_id);

create table if not exists hepta_artifact_manifests (
    manifest_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    binding_schema text not null check (binding_schema = 'hepta.paper_raid.artifact_manifest_binding.v1'),
    source_bundle_schema text not null check (source_bundle_schema = 'paper-raid.artifact-bundle.v1'),
    source_bundle_id text not null,
    source_challenge_id text not null,
    source_created_at text not null,
    source_manifest_sha256 text not null check (source_manifest_sha256 ~ '^[0-9a-f]{64}$'),
    manifest_hash text not null,
    object_count integer not null check (object_count > 0 and object_count <= 4096),
    storage_location_count integer not null check (storage_location_count = object_count),
    total_size_bytes bigint not null check (total_size_bytes >= 0),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (paper_project_id, source_manifest_sha256),
    unique (manifest_id, paper_project_id)
);

create unique index if not exists hepta_artifact_manifest_paper_root_idx
    on hepta_artifact_manifests (paper_project_id, manifest_hash);

create table if not exists hepta_paper_revision_artifact_bindings (
    revision_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    manifest_id uuid not null,
    artifact_manifest_hash text not null,
    source_logical_path text not null,
    source_manifest_hash text not null,
    bibliography_logical_path text not null,
    bibliography_hash text not null,
    claim_evidence_graph_logical_path text not null,
    claim_evidence_graph_hash text not null,
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (revision_id, paper_project_id),
    foreign key (revision_id, paper_project_id)
        references hepta_paper_revisions(revision_id, paper_project_id) on delete cascade,
    foreign key (manifest_id, paper_project_id)
        references hepta_artifact_manifests(manifest_id, paper_project_id),
    unique (paper_project_id, revision_id, manifest_id)
);

create table if not exists hepta_evidence_cards (
    evidence_card_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    source_uri text not null,
    source_hash text not null,
    locator text not null,
    license text not null,
    verified_by_player_id uuid not null references hepta_human_players(player_id),
    verification_key_id text not null,
    verification_signature text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (paper_project_id, source_hash, locator),
    unique (evidence_card_id, paper_project_id)
);

create table if not exists hepta_citation_records (
    citation_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    evidence_card_id uuid not null,
    doi text,
    canonical_url text,
    source_hash text not null,
    locator text not null,
    license text not null,
    verified_by_player_id uuid not null references hepta_human_players(player_id),
    verification_key_id text not null,
    verification_signature text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    check (doi is not null or canonical_url is not null),
    foreign key (evidence_card_id, paper_project_id)
        references hepta_evidence_cards(evidence_card_id, paper_project_id)
);

create unique index if not exists hepta_paper_work_items_project_scope_idx
    on hepta_paper_work_items (work_item_id, paper_project_id);

create unique index if not exists hepta_agent_bindings_agent_scope_idx
    on hepta_agent_bindings (binding_id, agent_id);

-- One namespace covers immutable whole-paper revisions from v2 and section
-- revisions from this migration.  It lets every parent/head reference carry
-- paper_project_id in its foreign key instead of accepting a cross-paper UUID.
create table if not exists hepta_collaboration_revision_refs (
    revision_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    ref_kind text not null check (ref_kind in ('paper', 'section')),
    section_key text,
    created_at timestamptz not null,
    unique (revision_id, paper_project_id),
    check (
        (ref_kind = 'paper' and section_key is null)
        or (ref_kind = 'section' and section_key is not null)
    )
);

insert into hepta_collaboration_revision_refs (
    revision_id, paper_project_id, ref_kind, section_key, created_at
)
select revision_id, paper_project_id, 'paper', null, created_at
from hepta_paper_revisions
on conflict (revision_id) do nothing;

create or replace function hepta_register_paper_revision_ref()
returns trigger language plpgsql as $$
begin
    insert into hepta_collaboration_revision_refs (
        revision_id, paper_project_id, ref_kind, section_key, created_at
    ) values (new.revision_id, new.paper_project_id, 'paper', null, new.created_at);
    return new;
end;
$$;

drop trigger if exists hepta_paper_revision_ref_trigger on hepta_paper_revisions;
create trigger hepta_paper_revision_ref_trigger
after insert on hepta_paper_revisions
for each row execute function hepta_register_paper_revision_ref();

create table if not exists hepta_experiment_plans (
    experiment_plan_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    protocol_snapshot_hash text not null,
    code_manifest_id uuid not null,
    dataset_manifest_id uuid not null,
    environment_manifest_id uuid not null,
    seed_policy_hash text not null,
    stopping_rule_hash text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (experiment_plan_id, paper_project_id),
    foreign key (code_manifest_id, paper_project_id)
        references hepta_artifact_manifests(manifest_id, paper_project_id),
    foreign key (dataset_manifest_id, paper_project_id)
        references hepta_artifact_manifests(manifest_id, paper_project_id),
    foreign key (environment_manifest_id, paper_project_id)
        references hepta_artifact_manifests(manifest_id, paper_project_id)
);

create table if not exists hepta_run_records (
    run_record_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    experiment_plan_id uuid not null,
    status text not null check (status in ('succeeded', 'failed', 'cancelled')),
    seed bigint not null,
    parameters_hash text not null,
    logs_manifest_id uuid not null,
    outputs_manifest_id uuid,
    metrics_hash text,
    failure_hash text,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (run_record_id, paper_project_id),
    foreign key (experiment_plan_id, paper_project_id)
        references hepta_experiment_plans(experiment_plan_id, paper_project_id),
    foreign key (logs_manifest_id, paper_project_id)
        references hepta_artifact_manifests(manifest_id, paper_project_id),
    foreign key (outputs_manifest_id, paper_project_id)
        references hepta_artifact_manifests(manifest_id, paper_project_id),
    check (
        (status = 'succeeded' and outputs_manifest_id is not null and metrics_hash is not null and failure_hash is null)
        or (status in ('failed', 'cancelled') and failure_hash is not null)
    )
);

create table if not exists hepta_figure_lineage (
    figure_lineage_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    figure_key text not null,
    figure_manifest_id uuid not null,
    run_record_ids uuid[] not null check (cardinality(run_record_ids) > 0),
    transform_hash text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    unique (paper_project_id, figure_key),
    unique (figure_lineage_id, paper_project_id),
    foreign key (figure_manifest_id, paper_project_id)
        references hepta_artifact_manifests(manifest_id, paper_project_id)
);

create table if not exists hepta_claim_records (
    claim_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    claim_key text not null,
    claim_kind text not null check (claim_kind in ('main', 'numeric', 'figure', 'supporting', 'limitation')),
    statement_hash text not null,
    evidence_card_ids uuid[] not null default '{}',
    run_record_ids uuid[] not null default '{}',
    figure_lineage_ids uuid[] not null default '{}',
    status text not null check (status in ('proposed', 'verified', 'challenged', 'rejected')),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    unique (paper_project_id, claim_key),
    unique (claim_id, paper_project_id),
    check (
        claim_kind not in ('main', 'numeric', 'figure')
        or cardinality(evidence_card_ids) + cardinality(run_record_ids) + cardinality(figure_lineage_ids) > 0
    )
);

create table if not exists hepta_section_leases (
    lease_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    section_key text not null,
    holder_player_id uuid not null references hepta_human_players(player_id),
    holder_binding_id uuid not null,
    fencing_token bigint not null check (fencing_token > 0),
    status text not null check (status in ('active', 'released', 'expired', 'consumed')),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    acquired_at timestamptz not null,
    expires_at timestamptz not null,
    updated_at timestamptz not null,
    unique (lease_id, paper_project_id),
    foreign key (holder_binding_id, holder_player_id)
        references hepta_agent_bindings(binding_id, player_id)
);

create unique index if not exists hepta_section_one_active_lease_idx
    on hepta_section_leases (paper_project_id, section_key)
    where status = 'active';

create table if not exists hepta_agent_proposals (
    proposal_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    work_item_id uuid not null,
    section_key text not null,
    parent_revision_id uuid not null,
    proposal_kind text not null check (proposal_kind in ('proposal', 'delivery')),
    payload_hash text not null,
    artifact_manifest_id uuid not null,
    agent_id text not null,
    binding_id uuid not null,
    agent_key_id text not null,
    agent_public_key text not null,
    signature text not null,
    status text not null check (status in ('submitted', 'accepted', 'rework', 'rejected', 'superseded')),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    signed_at timestamptz not null,
    updated_at timestamptz not null,
    unique (proposal_id, paper_project_id),
    foreign key (work_item_id, paper_project_id)
        references hepta_paper_work_items(work_item_id, paper_project_id),
    foreign key (binding_id, agent_id)
        references hepta_agent_bindings(binding_id, agent_id),
    foreign key (artifact_manifest_id, paper_project_id)
        references hepta_artifact_manifests(manifest_id, paper_project_id),
    foreign key (parent_revision_id, paper_project_id)
        references hepta_collaboration_revision_refs(revision_id, paper_project_id)
);

create table if not exists hepta_human_decisions (
    decision_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    proposal_id uuid not null,
    player_id uuid not null references hepta_human_players(player_id),
    decision text not null check (decision in ('accept', 'rework', 'reject')),
    reason_hash text not null,
    signing_key_id text not null,
    signing_public_key text not null,
    signing_public_key_hash text not null,
    signature text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    signed_at timestamptz not null,
    unique (proposal_id, player_id),
    foreign key (proposal_id, paper_project_id)
        references hepta_agent_proposals(proposal_id, paper_project_id)
);

create table if not exists hepta_section_revisions (
    section_revision_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    section_key text not null,
    parent_revision_id uuid not null,
    proposal_id uuid not null,
    lease_id uuid not null,
    fencing_token bigint not null check (fencing_token > 0),
    patch_manifest_id uuid not null,
    patch_hash text not null,
    status text not null check (status in ('proposed', 'approved', 'rework', 'rejected', 'merged', 'superseded')),
    version bigint not null check (version > 0),
    record_json jsonb not null,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    unique (section_revision_id, paper_project_id),
    foreign key (proposal_id, paper_project_id)
        references hepta_agent_proposals(proposal_id, paper_project_id),
    foreign key (lease_id, paper_project_id)
        references hepta_section_leases(lease_id, paper_project_id),
    foreign key (patch_manifest_id, paper_project_id)
        references hepta_artifact_manifests(manifest_id, paper_project_id),
    foreign key (parent_revision_id, paper_project_id)
        references hepta_collaboration_revision_refs(revision_id, paper_project_id)
);

create or replace function hepta_register_section_revision_ref()
returns trigger language plpgsql as $$
begin
    insert into hepta_collaboration_revision_refs (
        revision_id, paper_project_id, ref_kind, section_key, created_at
    ) values (
        new.section_revision_id, new.paper_project_id, 'section',
        new.section_key, new.created_at
    );
    return new;
end;
$$;

drop trigger if exists hepta_section_revision_ref_trigger on hepta_section_revisions;
create trigger hepta_section_revision_ref_trigger
after insert on hepta_section_revisions
for each row execute function hepta_register_section_revision_ref();

create table if not exists hepta_section_heads (
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    section_key text not null,
    base_paper_revision_id uuid not null,
    current_head_revision_id uuid not null,
    fencing_token bigint not null check (fencing_token >= 0),
    version bigint not null check (version > 0),
    updated_at timestamptz not null,
    primary key (paper_project_id, section_key),
    foreign key (base_paper_revision_id, paper_project_id)
        references hepta_paper_revisions(revision_id, paper_project_id),
    foreign key (current_head_revision_id, paper_project_id)
        references hepta_collaboration_revision_refs(revision_id, paper_project_id)
);

create table if not exists hepta_section_reviews (
    review_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    section_revision_id uuid not null,
    reviewer_player_id uuid not null references hepta_human_players(player_id),
    verdict text not null check (verdict in ('approve', 'rework', 'reject')),
    review_hash text not null,
    signing_key_id text not null,
    signing_public_key text not null,
    signing_public_key_hash text not null,
    signature text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    signed_at timestamptz not null,
    unique (section_revision_id, reviewer_player_id),
    foreign key (section_revision_id, paper_project_id)
        references hepta_section_revisions(section_revision_id, paper_project_id)
);

create table if not exists hepta_section_merges (
    merge_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    section_key text not null,
    section_revision_id uuid not null,
    parent_revision_id uuid not null,
    merged_section_revision_id uuid not null,
    lease_id uuid not null,
    fencing_token bigint not null check (fencing_token > 0),
    merged_by_player_id uuid not null references hepta_human_players(player_id),
    signing_key_id text not null,
    signing_public_key text not null,
    signing_public_key_hash text not null,
    signature text not null,
    version bigint not null check (version > 0),
    record_json jsonb not null,
    merged_at timestamptz not null,
    unique (paper_project_id, section_key, merged_section_revision_id),
    check (merged_section_revision_id = section_revision_id),
    foreign key (section_revision_id, paper_project_id)
        references hepta_section_revisions(section_revision_id, paper_project_id),
    foreign key (parent_revision_id, paper_project_id)
        references hepta_collaboration_revision_refs(revision_id, paper_project_id),
    foreign key (merged_section_revision_id, paper_project_id)
        references hepta_collaboration_revision_refs(revision_id, paper_project_id),
    foreign key (lease_id, paper_project_id)
        references hepta_section_leases(lease_id, paper_project_id)
);

create index if not exists hepta_section_merge_head_idx
    on hepta_section_merges (paper_project_id, section_key, merged_at desc, merge_id desc);

create table if not exists hepta_paper_room_events (
    cursor bigserial primary key,
    event_id uuid not null unique,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id) on delete cascade,
    event_type text not null,
    aggregate_id uuid not null,
    aggregate_version bigint not null check (aggregate_version > 0),
    payload jsonb not null,
    occurred_at timestamptz not null
);

create index if not exists hepta_paper_room_events_paper_cursor_idx
    on hepta_paper_room_events (paper_project_id, cursor);
