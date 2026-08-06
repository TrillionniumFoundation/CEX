-- Canonical CometBFT/AppHash Receipt V2 ingestion for Paper Raid.
--
-- Trust anchors are admitted only after their hash is pinned in the running
-- service configuration. Canonical wire bytes are retained verbatim; JSONB is
-- intentionally not used for receipt or anchor evidence.

create table if not exists hepta_trnm_cometbft_trust_anchors (
    anchor_hash text primary key,
    chain_id text not null,
    trusted_height bigint not null check (trusted_height > 0),
    canonical_anchor bytea not null,
    canonical_sha256 text not null unique,
    admitted_at timestamptz not null
);

create table if not exists hepta_paper_chain_finality_inbox (
    receipt_hash text primary key,
    canonical_sha256 text not null,
    anchor_hash text not null references hepta_trnm_cometbft_trust_anchors(anchor_hash),
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id),
    received_at timestamptz not null
);

create table if not exists hepta_paper_chain_receipts (
    receipt_hash text primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id),
    local_command_id uuid not null unique,
    command_idempotency_key text not null unique,
    command_fingerprint text not null,
    paper_binding_fingerprint text not null,
    anchor_hash text not null references hepta_trnm_cometbft_trust_anchors(anchor_hash),
    chain_id text not null,
    execution_height bigint not null check (execution_height > 0),
    commitment_height bigint not null check (commitment_height = execution_height + 1),
    comet_tx_hash text not null,
    app_hash text not null,
    canonical_receipt bytea not null,
    canonical_sha256 text not null,
    verified_at timestamptz not null,
    record_json jsonb not null
);

create table if not exists hepta_paper_chain_finality_projections (
    local_command_id uuid primary key,
    paper_project_id uuid not null references hepta_paper_projects(paper_project_id),
    evaluation_id uuid not null,
    receipt_hash text not null unique references hepta_paper_chain_receipts(receipt_hash),
    status text not null check (status in ('verified_finality')),
    version bigint not null check (version = 1),
    record_json jsonb not null,
    updated_at timestamptz not null,
    unique (paper_project_id, evaluation_id),
    foreign key (evaluation_id, paper_project_id)
        references hepta_paper_evaluations(evaluation_id, paper_project_id)
);

create index if not exists hepta_paper_chain_receipts_verified_idx
    on hepta_paper_chain_receipts (verified_at, receipt_hash);

create index if not exists hepta_paper_chain_finality_history_idx
    on hepta_paper_chain_finality_projections (paper_project_id, updated_at, local_command_id);
