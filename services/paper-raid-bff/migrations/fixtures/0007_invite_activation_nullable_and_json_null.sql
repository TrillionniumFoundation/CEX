\set ON_ERROR_STOP on

-- Run only against an empty, disposable PostgreSQL database. This fixture
-- proves that the optional revocation fields are valid SQL NULLs while a JSON
-- null in a required receipt field is rejected by both the validator and the
-- table CHECK.
\ir ../0003_invite_alpha_access.sql
\ir ../0007_invite_activation_authority.sql

DO $fixture$
DECLARE
    activation UUID := '70000000-0000-0000-0000-000000000001';
    invalid_activation UUID := '70000000-0000-0000-0000-000000000002';
    digest TEXT := 'sha256:' || repeat('a', 64);
    image TEXT := 'registry.invalid/paper-raid@sha256:' || repeat('b', 64);
    issued TIMESTAMPTZ := '2026-08-11 00:00:00+00';
    expires TIMESTAMPTZ := '2026-08-12 00:00:00+00';
    approval TEXT;
    approval_sha TEXT;
    valid_receipt JSONB;
    json_null_receipt JSONB;
    rejected BOOLEAN := FALSE;
BEGIN
    approval := jsonb_build_object(
        'schema', 'trnm.paper-raid.invite-alpha-activation-receipt.v1',
        'activation', 'externally_approved',
        'economy_eligibility', FALSE,
        'profile_sha256', digest,
        'runtime_acl_sha256', digest,
        'images', jsonb_build_object(
            'bff', image,
            'accessctl', image,
            'postgres', image,
            'ops', image,
            'hepta', image
        ),
        'retention', jsonb_build_object(
            'policy_id', 'retention.fixture.v1',
            'policy_sha256', digest
        ),
        'evidence', jsonb_build_object(
            'image_lock_sha256', digest,
            'release_provenance_sha256', digest,
            'runtime_acl_verification_sha256', digest
        )
    )::text;
    approval_sha := 'sha256:' || encode(
        sha256(convert_to(approval, 'UTF8')), 'hex'
    );
    valid_receipt := jsonb_build_object(
        'schema', 'trnm.paper-raid.invite-alpha-activation-receipt.v2',
        'approval_receipt_v1_sha256', approval_sha,
        'activation_id', activation,
        'profile_sha256', digest,
        'base_compose_sha256', digest,
        'runtime_acl_sha256', digest,
        'runtime_acl_state_sha256', digest,
        'bff_image', image,
        'accessctl_image', image,
        'postgres_image', image,
        'ops_image', image,
        'hepta_image', image,
        'retention_policy_id', 'retention.fixture.v1',
        'retention_policy_sha256', digest,
        'image_lock_sha256', digest,
        'release_provenance_sha256', digest,
        'runtime_acl_evidence_sha256', digest,
        'database_identity', 'database.fixture.v1',
        'deployment_identity', 'deployment.fixture.v1',
        'issued_at', issued,
        'expires_at', expires,
        'economy_eligibility', FALSE
    );

    IF (SELECT p.proisstrict
        FROM pg_proc p
        WHERE p.oid =
          'paper_raid_bff_invite_activation_row_valid_v2(jsonb,text,text,uuid,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,timestamp with time zone,timestamp with time zone,timestamp with time zone,text,boolean)'::regprocedure)
       IS DISTINCT FROM FALSE THEN
        RAISE EXCEPTION 'activation validator is not CALLED ON NULL INPUT';
    END IF;

    IF paper_raid_bff_invite_activation_row_valid_v2(
        valid_receipt, approval, approval_sha, activation, digest, digest,
        digest, digest, image, image, image, image, image,
        'retention.fixture.v1', digest, digest, digest, digest,
        'database.fixture.v1', 'deployment.fixture.v1', issued, expires,
        NULL, NULL, FALSE
    ) IS DISTINCT FROM TRUE THEN
        RAISE EXCEPTION 'valid active activation with nullable revocation fields failed';
    END IF;

    INSERT INTO paper_raid_bff_invite_activations(
        activation_id, approval_receipt_v1_sha256, profile_sha256,
        base_compose_sha256, runtime_acl_sha256, runtime_acl_state_sha256,
        bff_image, accessctl_image, postgres_image, ops_image, hepta_image,
        retention_policy_id, retention_policy_sha256, image_lock_sha256,
        release_provenance_sha256, runtime_acl_evidence_sha256,
        database_identity, deployment_identity, issued_at, expires_at,
        revoked_at, revocation_reason, economy_eligibility,
        approval_receipt_v1, receipt
    ) VALUES (
        activation, approval_sha, digest, digest, digest, digest,
        image, image, image, image, image,
        'retention.fixture.v1', digest, digest, digest, digest,
        'database.fixture.v1', 'deployment.fixture.v1', issued, expires,
        NULL, NULL, FALSE, approval, valid_receipt
    );
    DELETE FROM paper_raid_bff_invite_activations
    WHERE activation_id = activation;

    json_null_receipt := jsonb_set(
        valid_receipt,
        '{activation_id}',
        to_jsonb(invalid_activation),
        FALSE
    );
    json_null_receipt := jsonb_set(
        json_null_receipt,
        '{profile_sha256}',
        'null'::jsonb,
        FALSE
    );

    IF paper_raid_bff_invite_activation_row_valid_v2(
        json_null_receipt, approval, approval_sha, invalid_activation,
        digest, digest, digest, digest, image, image, image, image, image,
        'retention.fixture.v1', digest, digest, digest, digest,
        'database.fixture.v1', 'deployment.fixture.v1', issued, expires,
        NULL, NULL, FALSE
    ) IS DISTINCT FROM FALSE THEN
        RAISE EXCEPTION 'activation validator accepted a required JSON null';
    END IF;

    BEGIN
        INSERT INTO paper_raid_bff_invite_activations(
            activation_id, approval_receipt_v1_sha256, profile_sha256,
            base_compose_sha256, runtime_acl_sha256, runtime_acl_state_sha256,
            bff_image, accessctl_image, postgres_image, ops_image, hepta_image,
            retention_policy_id, retention_policy_sha256, image_lock_sha256,
            release_provenance_sha256, runtime_acl_evidence_sha256,
            database_identity, deployment_identity, issued_at, expires_at,
            revoked_at, revocation_reason, economy_eligibility,
            approval_receipt_v1, receipt
        ) VALUES (
            invalid_activation, approval_sha, digest, digest, digest, digest,
            image, image, image, image, image,
            'retention.fixture.v1', digest, digest, digest, digest,
            'database.fixture.v1', 'deployment.fixture.v1', issued, expires,
            NULL, NULL, FALSE, approval, json_null_receipt
        );
    EXCEPTION WHEN check_violation THEN
        rejected := TRUE;
    END;
    IF NOT rejected THEN
        RAISE EXCEPTION 'activation CHECK accepted a required JSON null';
    END IF;
    IF EXISTS (
        SELECT 1 FROM paper_raid_bff_invite_activations
        WHERE activation_id = invalid_activation
    ) THEN
        RAISE EXCEPTION 'JSON-null activation row survived rejection';
    END IF;
END
$fixture$;
