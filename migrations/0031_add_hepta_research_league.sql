-- Hepta Research League durable control-plane state.
-- The singleton snapshot is deliberately locked inside every write transaction:
-- it gives all service instances one serializable aggregate boundary while the
-- versioned domain objects are still evolving. Cross-module delivery uses the
-- transactional outbox/inbox tables below and never a distributed transaction.

create table if not exists hepta_league_state (
    state_key text primary key,
    revision bigint not null check (revision >= 0),
    state_json jsonb not null,
    updated_at timestamptz not null default now()
);

-- The original development snapshot stored one-way hashes for bearer
-- match_token values. Signed Nakama authorizations are a different trust
-- boundary and cannot be reconstructed from those hashes. Invalidate only
-- legacy development authorizations during the contract upgrade;
-- all other durable League state remains untouched.
update hepta_league_state
set state_json = jsonb_set(state_json, '{match_authorizations}', '{}'::jsonb, true),
    revision = revision + 1,
    updated_at = now()
where jsonb_path_exists(state_json, '$.match_authorizations.*.token_hash');

create table if not exists hepta_outbox (
    event_id uuid primary key,
    event_type text not null,
    aggregate_id text not null,
    aggregate_version bigint not null check (aggregate_version > 0),
    correlation_id uuid not null,
    causation_id uuid,
    idempotency_key text not null unique,
    schema_version text not null,
    producer text not null,
    payload_hash text not null,
    payload jsonb not null,
    occurred_at timestamptz not null,
    available_at timestamptz not null default now(),
    lease_owner text,
    lease_expires_at timestamptz,
    attempt_count integer not null default 0 check (attempt_count >= 0),
    delivered_at timestamptz,
    last_error text
);

create index if not exists hepta_outbox_delivery_idx
    on hepta_outbox (available_at, occurred_at)
    where delivered_at is null;

create table if not exists hepta_inbox (
    consumer text not null,
    event_id uuid not null,
    event_type text not null,
    payload_hash text not null,
    processed_at timestamptz not null default now(),
    primary key (consumer, event_id)
);

create table if not exists hepta_evaluator_manifests (
    evaluator_manifest_id uuid primary key,
    challenge_id uuid not null,
    version text not null,
    manifest_hash text not null,
    manifest_json jsonb not null,
    created_at timestamptz not null default now(),
    unique (challenge_id, version),
    unique (challenge_id, manifest_hash)
);

create table if not exists hepta_evaluation_reports (
    evaluation_report_id uuid primary key,
    submission_id uuid not null,
    evaluator_manifest_id uuid not null references hepta_evaluator_manifests(evaluator_manifest_id),
    report_hash text not null,
    score_micros bigint not null,
    report_json jsonb not null,
    created_at timestamptz not null default now(),
    unique (submission_id, evaluator_manifest_id)
);

create table if not exists hepta_reproduction_reports (
    reproduction_report_id uuid primary key,
    evaluation_report_id uuid not null references hepta_evaluation_reports(evaluation_report_id),
    report_hash text not null,
    report_json jsonb not null,
    created_at timestamptz not null default now()
);

create table if not exists hepta_appeal_cases (
    appeal_id uuid primary key,
    evaluation_report_id uuid not null references hepta_evaluation_reports(evaluation_report_id),
    status text not null,
    case_json jsonb not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists hepta_module_receipts (
    receipt_id uuid primary key,
    source_module text not null check (source_module in ('nakama', 'trnm')),
    source_event_id uuid not null,
    aggregate_id text not null,
    receipt_json jsonb not null,
    received_at timestamptz not null default now(),
    unique (source_module, source_event_id)
);
