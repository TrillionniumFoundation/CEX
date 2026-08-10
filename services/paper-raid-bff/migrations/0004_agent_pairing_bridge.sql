-- Agent Bridge pairing and per-request PoP state. Cleartext pairing codes,
-- Agent private keys, browser cookies, login credentials, bearer tokens,
-- raw proposal request bodies and durable scientific facts are never stored
-- here. Signed-request response bytes are a bounded exact-replay cache whose
-- replay validity is at most 60 seconds and expired rows are pruned during
-- signed-request admission and retention maintenance.

ALTER TABLE paper_raid_bff_quota_windows
    DROP CONSTRAINT IF EXISTS paper_raid_bff_quota_windows_quota_kind_check;

ALTER TABLE paper_raid_bff_quota_windows
    ADD CONSTRAINT paper_raid_bff_quota_windows_quota_kind_check CHECK (
        quota_kind IN (
            'login_global', 'login_bucket', 'authenticated_mutation',
            'agent_pair_global', 'agent_pair_bucket',
            'agent_request_global', 'agent_request_bucket',
            'agent_request_binding'
        )
    );

CREATE OR REPLACE FUNCTION paper_raid_bff_agent_capability_disclosure_valid_v1(
    disclosure JSONB,
    disclosure_hash TEXT
)
RETURNS BOOLEAN
LANGUAGE plpgsql
IMMUTABLE
AS $$
DECLARE
    item TEXT;
    previous_item TEXT;
    max_parallel_text TEXT;
BEGIN
    IF disclosure_hash !~ '^sha256:[0-9a-f]{64}$'
       OR jsonb_typeof(disclosure) IS DISTINCT FROM 'object'
       OR (SELECT count(*) FROM jsonb_object_keys(disclosure)) <> 5
       OR disclosure ->> 'schema'
            IS DISTINCT FROM 'hepta.paper_raid.agent_capability_disclosure.v1'
       OR disclosure ->> 'assurance' IS DISTINCT FROM 'self_declared_unverified'
       OR jsonb_typeof(disclosure -> 'capabilities') IS DISTINCT FROM 'array'
       OR jsonb_array_length(disclosure -> 'capabilities') NOT BETWEEN 1 AND 16
       OR jsonb_typeof(disclosure -> 'resource_classes') IS DISTINCT FROM 'array'
       OR jsonb_array_length(disclosure -> 'resource_classes') > 16
       OR jsonb_typeof(disclosure -> 'max_parallel_tasks') IS DISTINCT FROM 'number'
    THEN
        RETURN false;
    END IF;
    max_parallel_text := disclosure ->> 'max_parallel_tasks';
    IF max_parallel_text !~ '^[0-9]{1,2}$'
       OR max_parallel_text::integer NOT BETWEEN 1 AND 32
    THEN
        RETURN false;
    END IF;

    previous_item := NULL;
    FOR item IN SELECT jsonb_array_elements_text(disclosure -> 'capabilities')
    LOOP
        IF item IS NULL OR item NOT IN (
            'artifact_analysis', 'citation_verification', 'evidence_search',
            'experiment_execution', 'experiment_planning', 'reproduction',
            'research_session_signing', 'section_drafting'
        ) OR (
            previous_item IS NOT NULL
            AND (item COLLATE "C") <= (previous_item COLLATE "C")
        )
        THEN
            RETURN false;
        END IF;
        previous_item := item;
    END LOOP;

    previous_item := NULL;
    FOR item IN SELECT jsonb_array_elements_text(disclosure -> 'resource_classes')
    LOOP
        IF item IS NULL OR item NOT IN (
            'artifact_io', 'browser', 'code_execution', 'cpu',
            'gpu', 'network', 'sandbox'
        ) OR (
            previous_item IS NOT NULL
            AND (item COLLATE "C") <= (previous_item COLLATE "C")
        )
        THEN
            RETURN false;
        END IF;
        previous_item := item;
    END LOOP;
    RETURN true;
EXCEPTION
    WHEN others THEN
        RETURN false;
END;
$$;

CREATE TABLE IF NOT EXISTS paper_raid_bff_agent_pairing_grants (
    grant_id UUID PRIMARY KEY,
    subject_id TEXT NOT NULL,
    player_id UUID NOT NULL,
    code_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(code_hash) = 32),
    state TEXT NOT NULL CHECK (state IN ('issued', 'pinned', 'consumed', 'revoked')),
    pinned_request_hash BYTEA CHECK (
        pinned_request_hash IS NULL OR octet_length(pinned_request_hash) = 32
    ),
    pinned_binding_id UUID,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    pinned_at TIMESTAMPTZ,
    consumed_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    pair_response_status INTEGER CHECK (pair_response_status = 200),
    pair_response_body BYTEA CHECK (
        pair_response_body IS NULL OR octet_length(pair_response_body) BETWEEN 2 AND 131072
    ),
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '5 minutes'),
    CHECK (
        (state = 'issued'
            AND pinned_request_hash IS NULL AND pinned_binding_id IS NULL
            AND pinned_at IS NULL AND consumed_at IS NULL AND revoked_at IS NULL
            AND pair_response_status IS NULL AND pair_response_body IS NULL)
        OR (state = 'pinned'
            AND pinned_request_hash IS NOT NULL AND pinned_binding_id IS NOT NULL
            AND pinned_at IS NOT NULL AND consumed_at IS NULL AND revoked_at IS NULL
            AND pair_response_status IS NULL AND pair_response_body IS NULL)
        OR (state = 'consumed'
            AND pinned_request_hash IS NOT NULL AND pinned_binding_id IS NOT NULL
            AND pinned_at IS NOT NULL AND consumed_at IS NOT NULL AND revoked_at IS NULL
            AND pair_response_status = 200 AND pair_response_body IS NOT NULL)
        OR (state = 'revoked'
            AND consumed_at IS NULL AND revoked_at IS NOT NULL
            AND pair_response_status IS NULL AND pair_response_body IS NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_one_active_agent_pairing_grant
    ON paper_raid_bff_agent_pairing_grants(subject_id)
    WHERE state IN ('issued', 'pinned');

CREATE INDEX IF NOT EXISTS paper_raid_bff_agent_pairing_grants_expiry_idx
    ON paper_raid_bff_agent_pairing_grants(expires_at);

CREATE TABLE IF NOT EXISTS paper_raid_bff_agent_bridge_bindings (
    binding_id UUID PRIMARY KEY,
    grant_id UUID NOT NULL UNIQUE
        REFERENCES paper_raid_bff_agent_pairing_grants(grant_id),
    last_pairing_grant_id UUID NOT NULL UNIQUE
        REFERENCES paper_raid_bff_agent_pairing_grants(grant_id),
    subject_id TEXT NOT NULL,
    player_id UUID NOT NULL,
    agent_id TEXT NOT NULL,
    agent_key_id TEXT NOT NULL CHECK (agent_key_id ~ '^sha256:[0-9a-f]{64}$'),
    capability_disclosure_hash TEXT NOT NULL CHECK (
        capability_disclosure_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    capability_disclosure JSONB NOT NULL,
    binding_record JSONB NOT NULL CHECK (
        jsonb_typeof(binding_record) = 'object'
        AND binding_record ->> 'binding_id' = binding_id::text
        AND binding_record ->> 'player_id' = player_id::text
        AND binding_record ->> 'agent_id' = agent_id
        AND binding_record ->> 'agent_key_id' = agent_key_id
        AND binding_record ->> 'capability_disclosure_hash' = capability_disclosure_hash
        AND binding_record -> 'capability_disclosure' = capability_disclosure
        AND binding_record ->> 'status' = 'active'
    ),
    paired_at TIMESTAMPTZ NOT NULL,
    last_verified_at TIMESTAMPTZ NOT NULL,
    CHECK (paper_raid_bff_agent_capability_disclosure_valid_v1(
        capability_disclosure,
        capability_disclosure_hash
    )),
    UNIQUE(subject_id, agent_id)
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_agent_request_uses (
    binding_id UUID NOT NULL
        REFERENCES paper_raid_bff_agent_bridge_bindings(binding_id) ON DELETE CASCADE,
    nonce UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    response_status INTEGER CHECK (response_status BETWEEN 200 AND 599),
    response_body BYTEA CHECK (
        response_body IS NULL OR octet_length(response_body) BETWEEN 2 AND 2097152
    ),
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    PRIMARY KEY(binding_id, nonce),
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '60 seconds'),
    CHECK (
        (response_status IS NULL AND response_body IS NULL AND completed_at IS NULL)
        OR (response_status IS NOT NULL AND response_body IS NOT NULL AND completed_at IS NOT NULL)
    )
);

COMMENT ON TABLE paper_raid_bff_agent_request_uses IS
    'At-most-60-second exact replay cache; expired rows are deleted transactionally before every signed Agent request admission.';
COMMENT ON COLUMN paper_raid_bff_agent_request_uses.response_body IS
    'Short-lived exact response bytes for nonce replay only; never a durable Paper or scientific-fact read model.';

CREATE INDEX IF NOT EXISTS paper_raid_bff_agent_request_uses_expiry_idx
    ON paper_raid_bff_agent_request_uses(expires_at);

CREATE TABLE IF NOT EXISTS paper_raid_bff_agent_health (
    binding_id UUID PRIMARY KEY
        REFERENCES paper_raid_bff_agent_bridge_bindings(binding_id) ON DELETE CASCADE,
    assurance TEXT NOT NULL CHECK (assurance = 'self_declared_unverified'),
    status TEXT NOT NULL CHECK (status IN ('healthy', 'degraded', 'offline')),
    observed_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS paper_raid_bff_agent_bridge_audit (
    audit_id UUID PRIMARY KEY,
    subject_id TEXT,
    binding_id UUID,
    action TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'denied')),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(metadata) = 'object'),
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE OR REPLACE FUNCTION paper_raid_bff_reject_agent_bridge_audit_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'paper_raid_bff_agent_bridge_audit is append-only';
END;
$$;

DROP TRIGGER IF EXISTS paper_raid_bff_agent_bridge_audit_append_only
    ON paper_raid_bff_agent_bridge_audit;

CREATE TRIGGER paper_raid_bff_agent_bridge_audit_append_only
BEFORE UPDATE OR DELETE ON paper_raid_bff_agent_bridge_audit
FOR EACH ROW EXECUTE FUNCTION paper_raid_bff_reject_agent_bridge_audit_mutation();

INSERT INTO paper_raid_bff_schema_capabilities(capability)
VALUES ('agent_bridge_pairing_v1')
ON CONFLICT (capability) DO NOTHING;
