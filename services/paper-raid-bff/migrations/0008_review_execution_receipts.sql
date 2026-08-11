-- Durable, Agent-signed execution receipts for evaluator/reproducer Review Raid tasks.
-- The scientific authority remains in Hepta.  BFF rows are a recoverable transport outbox and
-- are revalidated against the exact active assignment/frozen bundle before browser projection.

CREATE TABLE IF NOT EXISTS paper_raid_bff_review_execution_receipts (
    receipt_id UUID CONSTRAINT paper_raid_bff_review_receipts_pk PRIMARY KEY,
    task_id UUID NOT NULL,
    binding_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    paper_id UUID NOT NULL,
    submission_id UUID NOT NULL,
    evaluation_id UUID NOT NULL,
    kind TEXT NOT NULL CONSTRAINT paper_raid_bff_review_receipt_kind_ck
        CHECK (kind IN ('evaluate','reproduce')),
    attempt BIGINT NOT NULL CONSTRAINT paper_raid_bff_review_receipt_attempt_ck
        CHECK (attempt > 0 AND attempt <= 9007199254740991),
    fencing_token BIGINT NOT NULL CONSTRAINT paper_raid_bff_review_receipt_fencing_ck
        CHECK (fencing_token > 0 AND fencing_token <= 9007199254740991),
    bundle_hash TEXT NOT NULL CONSTRAINT paper_raid_bff_review_receipt_bundle_hash_ck
        CHECK (bundle_hash ~ '^sha256:[0-9a-f]{64}$'),
    receipt_hash TEXT NOT NULL CONSTRAINT paper_raid_bff_review_receipt_hash_ck
        CHECK (receipt_hash ~ '^sha256:[0-9a-f]{64}$'),
    receipt JSONB NOT NULL,
    state TEXT NOT NULL CONSTRAINT paper_raid_bff_review_receipt_state_ck
        CHECK (state IN ('pending','consumed','invalidated')),
    -- The first human-signing request freezes one server-derived frame.  Retrying after a
    -- dropped response returns these exact bytes; a fresh wall-clock signature frame would
    -- otherwise change the Hepta idempotency body for the same receipt.
    confirmation_frame JSONB,
    confirmation_frame_hash TEXT CONSTRAINT paper_raid_bff_review_receipt_frame_hash_ck
        CHECK (confirmation_frame_hash IS NULL
            OR confirmation_frame_hash ~ '^sha256:[0-9a-f]{64}$'),
    confirmation_idempotency_key UUID,
    confirmation_hash TEXT CONSTRAINT paper_raid_bff_review_receipt_confirmation_hash_ck
        CHECK (confirmation_hash IS NULL OR confirmation_hash ~ '^sha256:[0-9a-f]{64}$'),
    confirmation_context_signature TEXT
        CONSTRAINT paper_raid_bff_review_receipt_context_signature_ck
        CHECK (confirmation_context_signature IS NULL OR (
            length(confirmation_context_signature) = 88
            AND confirmation_context_signature ~ '^[A-Za-z0-9+/]{86}==$'
        )),
    response_status INTEGER CONSTRAINT paper_raid_bff_review_receipt_response_status_ck
        CHECK (response_status IS NULL OR response_status BETWEEN 200 AND 299),
    response_body BYTEA,
    created_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    invalidated_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT paper_raid_bff_review_receipt_binding_fk FOREIGN KEY (binding_id)
        REFERENCES paper_raid_bff_agent_bridge_bindings(binding_id) ON DELETE CASCADE,
    CONSTRAINT paper_raid_bff_review_receipt_result_shape_ck CHECK (evaluation_id IS NOT NULL),
    CONSTRAINT paper_raid_bff_review_receipt_frame_pair_ck CHECK (
        (confirmation_frame IS NULL) = (confirmation_frame_hash IS NULL)
        AND (confirmation_frame IS NULL) = (confirmation_idempotency_key IS NULL)
    ),
    CONSTRAINT paper_raid_bff_review_receipt_frame_binding_ck CHECK (
        confirmation_frame IS NULL OR (
            jsonb_typeof(confirmation_frame) IS NOT DISTINCT FROM 'object'
            AND confirmation_frame ->> 'schema' IS NOT DISTINCT FROM
                'hepta.paper_raid.review_receipt_confirmation_frame.v1'
            AND jsonb_typeof(confirmation_frame -> 'receipt_context_signing_bytes')
                IS NOT DISTINCT FROM 'string'
            AND confirmation_frame -> 'receipt_context' ->> 'schema' IS NOT DISTINCT FROM
                'hepta.paper_raid.review_receipt_confirmation_context.v1'
            AND confirmation_frame -> 'receipt_context' ->> 'receipt_id'
                IS NOT DISTINCT FROM receipt_id::text
            AND confirmation_frame -> 'receipt_context' ->> 'receipt_hash'
                IS NOT DISTINCT FROM receipt_hash
            AND confirmation_frame -> 'receipt_context' ->> 'task_id'
                IS NOT DISTINCT FROM task_id::text
            AND confirmation_frame -> 'receipt_context' ->> 'assignment_id'
                IS NOT DISTINCT FROM assignment_id::text
            AND (confirmation_frame -> 'receipt_context' ->> 'assignment_version')::bigint
                IS NOT DISTINCT FROM fencing_token
            AND confirmation_frame -> 'receipt_context' ->> 'paper_project_id'
                IS NOT DISTINCT FROM paper_id::text
            AND confirmation_frame -> 'receipt_context' ->> 'submission_id'
                IS NOT DISTINCT FROM submission_id::text
            AND confirmation_frame -> 'receipt_context' ->> 'evaluation_id'
                IS NOT DISTINCT FROM evaluation_id::text
            AND confirmation_frame -> 'receipt_context' ->> 'kind'
                IS NOT DISTINCT FROM kind
            AND confirmation_frame -> 'receipt_context' ->> 'bundle_hash'
                IS NOT DISTINCT FROM bundle_hash
            AND confirmation_frame ->> 'resource_id' IS NOT DISTINCT FROM paper_id::text
            AND ((kind='evaluate'
                    AND confirmation_frame ->> 'command' IS NOT DISTINCT FROM
                        'create_paper_evaluation_draft'
                    AND confirmation_frame -> 'child_id' IS NOT DISTINCT FROM 'null'::jsonb
                    AND jsonb_typeof(confirmation_frame -> 'receipt_context' -> 'candidate_passed')
                        IS NOT DISTINCT FROM 'boolean')
                OR (kind='reproduce'
                    AND confirmation_frame ->> 'command' IS NOT DISTINCT FROM
                        'submit_reproduction'
                    AND confirmation_frame ->> 'child_id' IS NOT DISTINCT FROM evaluation_id::text
                    AND confirmation_frame -> 'receipt_context' -> 'candidate_passed'
                        IS NOT DISTINCT FROM 'null'::jsonb))
        )
    ),
    CONSTRAINT paper_raid_bff_review_receipt_lifecycle_ck CHECK (
        (state='pending' AND consumed_at IS NULL AND invalidated_at IS NULL
            AND confirmation_hash IS NULL AND confirmation_context_signature IS NULL
            AND response_status IS NULL AND response_body IS NULL)
        OR (state='consumed' AND consumed_at IS NOT NULL AND invalidated_at IS NULL
            AND confirmation_frame IS NOT NULL AND confirmation_frame_hash IS NOT NULL
            AND confirmation_idempotency_key IS NOT NULL
            AND confirmation_hash IS NOT NULL AND confirmation_context_signature IS NOT NULL
            AND response_status IS NOT NULL
            AND response_body IS NOT NULL)
        OR (state='invalidated' AND consumed_at IS NULL AND invalidated_at IS NOT NULL
            AND confirmation_hash IS NULL AND confirmation_context_signature IS NULL
            AND response_status IS NULL AND response_body IS NULL)
    ),
    CONSTRAINT paper_raid_bff_review_receipt_json_binding_ck CHECK (
        receipt ->> 'schema' IS NOT DISTINCT FROM 'hepta.paper_raid.agent_bridge.review_receipt_request.v1'
        AND receipt ->> 'idempotency_key' IS NOT DISTINCT FROM receipt_id::text
        AND receipt -> 'receipt' ->> 'schema' IS NOT DISTINCT FROM 'hepta.paper_raid.review_execution_receipt.v1'
        AND receipt -> 'receipt' ->> 'receipt_id' IS NOT DISTINCT FROM receipt_id::text
        AND receipt -> 'receipt' ->> 'task_id' IS NOT DISTINCT FROM task_id::text
        AND receipt -> 'receipt' ->> 'binding_id' IS NOT DISTINCT FROM binding_id::text
        AND receipt -> 'receipt' ->> 'assignment_id' IS NOT DISTINCT FROM assignment_id::text
        AND receipt -> 'receipt' ->> 'paper_project_id' IS NOT DISTINCT FROM paper_id::text
        AND receipt -> 'receipt' ->> 'submission_id' IS NOT DISTINCT FROM submission_id::text
        AND receipt -> 'receipt' ->> 'evaluation_id' IS NOT DISTINCT FROM evaluation_id::text
        AND receipt -> 'receipt' ->> 'kind' IS NOT DISTINCT FROM kind
        AND (receipt -> 'receipt' ->> 'attempt')::bigint IS NOT DISTINCT FROM attempt
        AND (receipt -> 'receipt' ->> 'fencing_token')::bigint IS NOT DISTINCT FROM fencing_token
        AND receipt -> 'receipt' ->> 'bundle_hash' IS NOT DISTINCT FROM bundle_hash
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_one_review_receipt_per_task_attempt
    ON paper_raid_bff_review_execution_receipts(task_id,attempt);

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_one_live_review_receipt_per_task
    ON paper_raid_bff_review_execution_receipts(task_id)
    WHERE state IN ('pending','consumed');

CREATE INDEX IF NOT EXISTS paper_raid_bff_review_receipt_assignment_inbox_idx
    ON paper_raid_bff_review_execution_receipts(assignment_id,state,created_at DESC);

INSERT INTO paper_raid_bff_schema_capabilities(capability)
VALUES ('review_execution_receipts_v1')
ON CONFLICT (capability) DO NOTHING;

COMMENT ON TABLE paper_raid_bff_review_execution_receipts IS
    'Recoverable Agent-signed evaluator/reproducer receipts, each pinned to one assignment version and one immutable frozen review bundle.';
