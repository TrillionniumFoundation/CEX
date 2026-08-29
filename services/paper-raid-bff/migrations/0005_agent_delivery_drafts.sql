-- Short-lived, Agent-declared delivery bindings. These rows are control-plane
-- intents, not scientific facts: the authoritative Paper, work assignment,
-- section head/lease and artifact manifest remain in Hepta and are revalidated
-- before every inbox projection and proposal submission.

CREATE TABLE IF NOT EXISTS paper_raid_bff_agent_delivery_drafts (
    delivery_draft_id UUID
        CONSTRAINT paper_raid_bff_agent_delivery_drafts_pk PRIMARY KEY,
    binding_id UUID NOT NULL,
    paper_id UUID NOT NULL,
    work_item_id UUID NOT NULL,
    expected_work_version BIGINT NOT NULL
        CONSTRAINT paper_raid_bff_agent_delivery_work_version_ck CHECK (
            expected_work_version > 0
            AND expected_work_version <= 9007199254740991
        ),
    section_key TEXT NOT NULL CONSTRAINT paper_raid_bff_agent_delivery_section_key_ck CHECK (
        section_key ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
    ),
    lease_id UUID NOT NULL,
    lease_fencing_token BIGINT NOT NULL
        CONSTRAINT paper_raid_bff_agent_delivery_fencing_ck CHECK (
            lease_fencing_token > 0
            AND lease_fencing_token <= 9007199254740991
        ),
    parent_revision_id UUID NOT NULL,
    artifact_manifest_id UUID NOT NULL,
    artifact_manifest_hash TEXT NOT NULL
        CONSTRAINT paper_raid_bff_agent_delivery_manifest_hash_ck CHECK (
        artifact_manifest_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    payload_hash TEXT NOT NULL
        CONSTRAINT paper_raid_bff_agent_delivery_payload_hash_ck CHECK (
        payload_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    state TEXT NOT NULL CONSTRAINT paper_raid_bff_agent_delivery_state_ck CHECK (
        state IN ('pending', 'submitting', 'consumed', 'invalidated', 'expired')
    ),
    proposal_body_hash TEXT
        CONSTRAINT paper_raid_bff_agent_delivery_body_hash_ck CHECK (
        proposal_body_hash IS NULL OR proposal_body_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    proposal_id UUID,
    proposal_idempotency_key UUID,
    proposal_signed_at_unix BIGINT,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    submitting_at TIMESTAMPTZ,
    consumed_at TIMESTAMPTZ,
    invalidated_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT paper_raid_bff_agent_delivery_binding_fk
        FOREIGN KEY (binding_id)
        REFERENCES paper_raid_bff_agent_bridge_bindings(binding_id) ON DELETE CASCADE,
    CONSTRAINT paper_raid_bff_agent_delivery_ttl_ck
        CHECK (expires_at > created_at AND expires_at <= created_at + interval '15 minutes'),
    CONSTRAINT paper_raid_bff_agent_delivery_lifecycle_ck CHECK (
        (state = 'pending' AND proposal_body_hash IS NULL AND proposal_id IS NULL
            AND proposal_idempotency_key IS NULL AND proposal_signed_at_unix IS NULL
            AND submitting_at IS NULL AND consumed_at IS NULL AND invalidated_at IS NULL)
        OR (state = 'submitting' AND proposal_body_hash IS NOT NULL AND proposal_id IS NOT NULL
            AND proposal_idempotency_key IS NOT NULL AND proposal_signed_at_unix IS NOT NULL
            AND proposal_signed_at_unix = floor(extract(epoch FROM created_at))::bigint
            AND submitting_at IS NOT NULL AND consumed_at IS NULL AND invalidated_at IS NULL)
        OR (state = 'consumed' AND proposal_body_hash IS NOT NULL AND proposal_id IS NOT NULL
            AND proposal_idempotency_key IS NOT NULL AND proposal_signed_at_unix IS NOT NULL
            AND proposal_signed_at_unix = floor(extract(epoch FROM created_at))::bigint
            AND submitting_at IS NOT NULL AND consumed_at IS NOT NULL AND invalidated_at IS NULL)
        OR (state IN ('invalidated', 'expired')
            AND proposal_body_hash IS NULL AND proposal_id IS NULL
            AND proposal_idempotency_key IS NULL AND proposal_signed_at_unix IS NULL
            AND submitting_at IS NULL AND consumed_at IS NULL AND invalidated_at IS NOT NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_one_pending_agent_delivery_tuple
    ON paper_raid_bff_agent_delivery_drafts (
        binding_id, paper_id, work_item_id, section_key,
        parent_revision_id, artifact_manifest_id, payload_hash
    )
    WHERE state IN ('pending', 'submitting', 'consumed');

CREATE INDEX IF NOT EXISTS paper_raid_bff_agent_delivery_drafts_inbox_idx
    ON paper_raid_bff_agent_delivery_drafts(binding_id, paper_id, expires_at)
    WHERE state IN ('pending', 'submitting', 'consumed');

COMMENT ON TABLE paper_raid_bff_agent_delivery_drafts IS
    'At-most-15-minute Agent-declared delivery intents. Every projection and submission is revalidated against authoritative Hepta state; expired/invalid rows are never candidates.';

INSERT INTO paper_raid_bff_schema_capabilities(capability)
VALUES ('agent_bridge_delivery_drafts_v1')
ON CONFLICT (capability) DO NOTHING;
