\set ON_ERROR_STOP on

-- Run only against an empty, disposable PostgreSQL database. This fixture
-- builds the real Agent Bridge schema in fresh state, applies 0008, and proves
-- that a consumed Review receipt cannot exist without the second, canonical
-- confirmation-context signature.
\ir ../0003_invite_alpha_access.sql
\ir ../0004_agent_pairing_bridge.sql
\ir ../0008_review_execution_receipts.sql

DO $fixture$
DECLARE
    fixture_grant_id UUID := '80000000-0000-0000-0000-000000000001';
    fixture_binding_id UUID := '80000000-0000-0000-0000-000000000002';
    fixture_player_id UUID := '80000000-0000-0000-0000-000000000003';
    fixture_receipt_id UUID := '80000000-0000-0000-0000-000000000100';
    fixture_task_id UUID := '80000000-0000-0000-0000-000000000101';
    fixture_assignment_id UUID := '80000000-0000-0000-0000-000000000102';
    fixture_paper_id UUID := '80000000-0000-0000-0000-000000000103';
    fixture_submission_id UUID := '80000000-0000-0000-0000-000000000104';
    fixture_evaluation_id UUID := '80000000-0000-0000-0000-000000000105';
    fixture_confirmation_idempotency_key UUID :=
        '80000000-0000-0000-0000-000000000106';
    bundle_digest TEXT := 'sha256:' || repeat('a', 64);
    receipt_digest TEXT := 'sha256:' || repeat('b', 64);
    canonical_signature TEXT := repeat('A', 86) || '==';
    noncanonical_signature TEXT := repeat('A', 85) || '-==';
    fixture_capability_disclosure JSONB;
    fixture_receipt JSONB;
    fixture_confirmation_frame JSONB;
    constraint_definition TEXT;
    rejected_constraint TEXT;
    rejected BOOLEAN;
BEGIN
    IF to_regclass('paper_raid_bff_review_execution_receipts') IS NULL THEN
        RAISE EXCEPTION '0008 review receipt table is missing';
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_attribute
        WHERE attrelid = 'paper_raid_bff_review_execution_receipts'::regclass
          AND attname = 'confirmation_context_signature'
          AND format_type(atttypid, atttypmod) = 'text'
          AND NOT attnotnull
          AND NOT attisdropped
    ) THEN
        RAISE EXCEPTION '0008 confirmation context signature column drifted';
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM paper_raid_bff_schema_capabilities
        WHERE capability = 'review_execution_receipts_v1'
    ) THEN
        RAISE EXCEPTION '0008 schema capability is missing';
    END IF;

    SELECT regexp_replace(pg_get_constraintdef(c.oid), '[[:space:]]+', '', 'g')
    INTO constraint_definition
    FROM pg_constraint c
    WHERE c.conrelid = 'paper_raid_bff_review_execution_receipts'::regclass
      AND c.conname = 'paper_raid_bff_review_receipt_context_signature_ck'
      AND c.contype = 'c'
      AND c.convalidated;
    IF constraint_definition IS NULL
       OR constraint_definition NOT LIKE
          '%length(confirmation_context_signature)=88%'
       OR constraint_definition NOT LIKE
          '%confirmation_context_signature~%[A-Za-z0-9+/]{86}==%' THEN
        RAISE EXCEPTION '0008 context signature CHECK is missing or drifted: %',
            constraint_definition;
    END IF;

    SELECT regexp_replace(pg_get_constraintdef(c.oid), '[[:space:]]+', '', 'g')
    INTO constraint_definition
    FROM pg_constraint c
    WHERE c.conrelid = 'paper_raid_bff_review_execution_receipts'::regclass
      AND c.conname = 'paper_raid_bff_review_receipt_frame_binding_ck'
      AND c.contype = 'c'
      AND c.convalidated;
    IF constraint_definition IS NULL
       OR constraint_definition NOT LIKE '%receipt_context_signing_bytes%'
       OR constraint_definition NOT LIKE '%jsonb_typeof%string%' THEN
        RAISE EXCEPTION '0008 context signing bytes CHECK is missing or drifted: %',
            constraint_definition;
    END IF;

    SELECT regexp_replace(pg_get_constraintdef(c.oid), '[[:space:]]+', '', 'g')
    INTO constraint_definition
    FROM pg_constraint c
    WHERE c.conrelid = 'paper_raid_bff_review_execution_receipts'::regclass
      AND c.conname = 'paper_raid_bff_review_receipt_lifecycle_ck'
      AND c.contype = 'c'
      AND c.convalidated;
    IF constraint_definition IS NULL
       OR constraint_definition NOT LIKE
          '%state=''pending''%confirmation_context_signatureISNULL%'
       OR constraint_definition NOT LIKE
          '%state=''consumed''%confirmation_context_signatureISNOTNULL%' THEN
        RAISE EXCEPTION '0008 context signature lifecycle CHECK drifted: %',
            constraint_definition;
    END IF;

    fixture_capability_disclosure := jsonb_build_object(
        'schema', 'hepta.paper_raid.agent_capability_disclosure.v1',
        'assurance', 'self_declared_unverified',
        'capabilities', jsonb_build_array('research_session_signing'),
        'resource_classes', jsonb_build_array(),
        'max_parallel_tasks', 1
    );

    INSERT INTO paper_raid_bff_agent_pairing_grants(
        grant_id, subject_id, player_id, code_hash, state,
        pinned_request_hash, pinned_binding_id, created_at, expires_at,
        pinned_at, consumed_at, revoked_at, pair_response_status,
        pair_response_body, updated_at
    ) VALUES (
        fixture_grant_id, 'fixture-review-actor', fixture_player_id,
        decode(repeat('01', 32), 'hex'), 'consumed',
        decode(repeat('02', 32), 'hex'), fixture_binding_id,
        '2026-08-11 00:00:00+00', '2026-08-11 00:05:00+00',
        '2026-08-11 00:01:00+00', '2026-08-11 00:02:00+00', NULL,
        200, convert_to('{}', 'UTF8'), '2026-08-11 00:02:00+00'
    );

    INSERT INTO paper_raid_bff_agent_bridge_bindings(
        binding_id, grant_id, last_pairing_grant_id, subject_id, player_id,
        agent_id, agent_key_id, capability_disclosure_hash,
        capability_disclosure, binding_record, paired_at, last_verified_at
    ) VALUES (
        fixture_binding_id, fixture_grant_id, fixture_grant_id,
        'fixture-review-actor', fixture_player_id,
        'fixture-review-agent', bundle_digest, bundle_digest,
        fixture_capability_disclosure,
        jsonb_build_object(
            'binding_id', fixture_binding_id::text,
            'player_id', fixture_player_id::text,
            'agent_id', 'fixture-review-agent',
            'agent_key_id', bundle_digest,
            'capability_disclosure_hash', bundle_digest,
            'capability_disclosure', fixture_capability_disclosure,
            'status', 'active'
        ),
        '2026-08-11 00:02:00+00', '2026-08-11 00:02:00+00'
    );

    fixture_receipt := jsonb_build_object(
        'schema', 'hepta.paper_raid.agent_bridge.review_receipt_request.v1',
        'idempotency_key', fixture_receipt_id::text,
        'receipt', jsonb_build_object(
            'schema', 'hepta.paper_raid.review_execution_receipt.v1',
            'receipt_id', fixture_receipt_id::text,
            'task_id', fixture_task_id::text,
            'binding_id', fixture_binding_id::text,
            'assignment_id', fixture_assignment_id::text,
            'paper_project_id', fixture_paper_id::text,
            'submission_id', fixture_submission_id::text,
            'evaluation_id', fixture_evaluation_id::text,
            'kind', 'evaluate',
            'attempt', 1,
            'fencing_token', 1,
            'bundle_hash', bundle_digest
        )
    );
    fixture_confirmation_frame := jsonb_build_object(
        'schema', 'hepta.paper_raid.review_receipt_confirmation_frame.v1',
        'command', 'create_paper_evaluation_draft',
        'resource_id', fixture_paper_id::text,
        'child_id', NULL,
        'receipt_context_signing_bytes',
            'eyJzY2hlbWEiOiJmaXh0dXJlLnYxIn0=',
        'receipt_context', jsonb_build_object(
            'schema', 'hepta.paper_raid.review_receipt_confirmation_context.v1',
            'receipt_id', fixture_receipt_id::text,
            'receipt_hash', receipt_digest,
            'task_id', fixture_task_id::text,
            'assignment_id', fixture_assignment_id::text,
            'assignment_version', 1,
            'paper_project_id', fixture_paper_id::text,
            'submission_id', fixture_submission_id::text,
            'evaluation_id', fixture_evaluation_id::text,
            'kind', 'evaluate',
            'bundle_hash', bundle_digest,
            'candidate_passed', TRUE
        )
    );

    INSERT INTO paper_raid_bff_review_execution_receipts(
        receipt_id, task_id, binding_id, assignment_id, paper_id,
        submission_id, evaluation_id, kind, attempt, fencing_token,
        bundle_hash, receipt_hash, receipt, state, created_at, updated_at
    ) VALUES (
        fixture_receipt_id, fixture_task_id, fixture_binding_id,
        fixture_assignment_id, fixture_paper_id, fixture_submission_id,
        fixture_evaluation_id, 'evaluate', 1, 1,
        bundle_digest, receipt_digest, fixture_receipt, 'pending',
        '2026-08-11 00:03:00+00', '2026-08-11 00:03:00+00'
    );
    IF (SELECT jsonb_build_array(
            state, confirmation_frame, confirmation_context_signature,
            consumed_at, invalidated_at, response_status, response_body
        )
        FROM paper_raid_bff_review_execution_receipts
        WHERE paper_raid_bff_review_execution_receipts.receipt_id =
            fixture_receipt_id)
       IS DISTINCT FROM
          '["pending",null,null,null,null,null,null]'::jsonb THEN
        RAISE EXCEPTION '0008 pending lifecycle row drifted';
    END IF;

    rejected := FALSE;
    BEGIN
        UPDATE paper_raid_bff_review_execution_receipts
        SET confirmation_context_signature = canonical_signature
        WHERE paper_raid_bff_review_execution_receipts.receipt_id =
            fixture_receipt_id;
    EXCEPTION WHEN check_violation THEN
        GET STACKED DIAGNOSTICS rejected_constraint = CONSTRAINT_NAME;
        IF rejected_constraint IS DISTINCT FROM
           'paper_raid_bff_review_receipt_lifecycle_ck' THEN
            RAISE EXCEPTION 'pending context signature failed through wrong CHECK: %',
                rejected_constraint;
        END IF;
        rejected := TRUE;
    END;
    IF NOT rejected THEN
        RAISE EXCEPTION 'pending receipt accepted a context signature';
    END IF;

    rejected := FALSE;
    BEGIN
        UPDATE paper_raid_bff_review_execution_receipts
        SET state = 'consumed',
            confirmation_frame = fixture_confirmation_frame,
            confirmation_frame_hash = bundle_digest,
            confirmation_idempotency_key =
                fixture_confirmation_idempotency_key,
            confirmation_hash = bundle_digest,
            response_status = 200,
            response_body = convert_to('{}', 'UTF8'),
            consumed_at = '2026-08-11 00:04:00+00',
            updated_at = '2026-08-11 00:04:00+00'
        WHERE paper_raid_bff_review_execution_receipts.receipt_id =
            fixture_receipt_id;
    EXCEPTION WHEN check_violation THEN
        GET STACKED DIAGNOSTICS rejected_constraint = CONSTRAINT_NAME;
        IF rejected_constraint IS DISTINCT FROM
           'paper_raid_bff_review_receipt_lifecycle_ck' THEN
            RAISE EXCEPTION 'missing context signature failed through wrong CHECK: %',
                rejected_constraint;
        END IF;
        rejected := TRUE;
    END;
    IF NOT rejected THEN
        RAISE EXCEPTION 'consumed receipt accepted a missing context signature';
    END IF;

    rejected := FALSE;
    BEGIN
        UPDATE paper_raid_bff_review_execution_receipts
        SET state = 'consumed',
            confirmation_frame = fixture_confirmation_frame,
            confirmation_frame_hash = bundle_digest,
            confirmation_idempotency_key =
                fixture_confirmation_idempotency_key,
            confirmation_hash = bundle_digest,
            confirmation_context_signature = noncanonical_signature,
            response_status = 200,
            response_body = convert_to('{}', 'UTF8'),
            consumed_at = '2026-08-11 00:04:00+00',
            updated_at = '2026-08-11 00:04:00+00'
        WHERE paper_raid_bff_review_execution_receipts.receipt_id =
            fixture_receipt_id;
    EXCEPTION WHEN check_violation THEN
        GET STACKED DIAGNOSTICS rejected_constraint = CONSTRAINT_NAME;
        IF rejected_constraint IS DISTINCT FROM
           'paper_raid_bff_review_receipt_context_signature_ck' THEN
            RAISE EXCEPTION 'non-canonical context signature failed through wrong CHECK: %',
                rejected_constraint;
        END IF;
        rejected := TRUE;
    END;
    IF NOT rejected THEN
        RAISE EXCEPTION 'consumed receipt accepted a non-canonical context signature';
    END IF;

    rejected := FALSE;
    BEGIN
        UPDATE paper_raid_bff_review_execution_receipts
        SET state = 'consumed',
            confirmation_frame = fixture_confirmation_frame -
                'receipt_context_signing_bytes',
            confirmation_frame_hash = bundle_digest,
            confirmation_idempotency_key =
                fixture_confirmation_idempotency_key,
            confirmation_hash = bundle_digest,
            confirmation_context_signature = canonical_signature,
            response_status = 200,
            response_body = convert_to('{}', 'UTF8'),
            consumed_at = '2026-08-11 00:04:00+00',
            updated_at = '2026-08-11 00:04:00+00'
        WHERE paper_raid_bff_review_execution_receipts.receipt_id =
            fixture_receipt_id;
    EXCEPTION WHEN check_violation THEN
        GET STACKED DIAGNOSTICS rejected_constraint = CONSTRAINT_NAME;
        IF rejected_constraint IS DISTINCT FROM
           'paper_raid_bff_review_receipt_frame_binding_ck' THEN
            RAISE EXCEPTION 'missing context signing bytes failed through wrong CHECK: %',
                rejected_constraint;
        END IF;
        rejected := TRUE;
    END;
    IF NOT rejected THEN
        RAISE EXCEPTION 'consumed receipt accepted missing context signing bytes';
    END IF;

    UPDATE paper_raid_bff_review_execution_receipts
    SET state = 'consumed',
        confirmation_frame = fixture_confirmation_frame,
        confirmation_frame_hash = bundle_digest,
        confirmation_idempotency_key = fixture_confirmation_idempotency_key,
        confirmation_hash = bundle_digest,
        confirmation_context_signature = canonical_signature,
        response_status = 200,
        response_body = convert_to('{}', 'UTF8'),
        consumed_at = '2026-08-11 00:04:00+00',
        updated_at = '2026-08-11 00:04:00+00'
    WHERE paper_raid_bff_review_execution_receipts.receipt_id =
        fixture_receipt_id;

    IF (SELECT jsonb_build_array(
            state, confirmation_context_signature, response_status,
            encode(response_body, 'escape'), consumed_at IS NOT NULL,
            invalidated_at IS NULL
        )
        FROM paper_raid_bff_review_execution_receipts
        WHERE paper_raid_bff_review_execution_receipts.receipt_id =
            fixture_receipt_id)
       IS DISTINCT FROM jsonb_build_array(
            'consumed', canonical_signature, 200, '{}', TRUE, TRUE
          ) THEN
        RAISE EXCEPTION 'valid consumed receipt was not persisted exactly';
    END IF;

    rejected := FALSE;
    BEGIN
        UPDATE paper_raid_bff_review_execution_receipts
        SET confirmation_context_signature = NULL
        WHERE paper_raid_bff_review_execution_receipts.receipt_id =
            fixture_receipt_id;
    EXCEPTION WHEN check_violation THEN
        GET STACKED DIAGNOSTICS rejected_constraint = CONSTRAINT_NAME;
        IF rejected_constraint IS DISTINCT FROM
           'paper_raid_bff_review_receipt_lifecycle_ck' THEN
            RAISE EXCEPTION 'consumed signature removal failed through wrong CHECK: %',
                rejected_constraint;
        END IF;
        rejected := TRUE;
    END;
    IF NOT rejected THEN
        RAISE EXCEPTION 'consumed receipt allowed context signature removal';
    END IF;
END;
$fixture$;
