\set ON_ERROR_STOP on

-- Run only against an empty, disposable PostgreSQL database. A hostile
-- STRICT replacement returns SQL NULL for every active row because its
-- revocation fields are NULL. The outer CHECK must still reject that row,
-- while runtime readiness separately requires proisstrict = false.
\ir ../0003_invite_alpha_access.sql
\ir ../0007_invite_activation_authority.sql

ALTER FUNCTION paper_raid_bff_invite_activation_row_valid_v2(
    JSONB, TEXT, TEXT, UUID, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT,
    TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT,
    TIMESTAMPTZ, TIMESTAMPTZ, TIMESTAMPTZ, TEXT, BOOLEAN
) STRICT;

DO $fixture$
DECLARE
    activation UUID := '70000000-0000-0000-0000-000000000003';
    digest TEXT := 'sha256:' || repeat('c', 64);
    image TEXT := 'registry.invalid/paper-raid@sha256:' || repeat('d', 64);
    rejected BOOLEAN := FALSE;
BEGIN
    IF (SELECT p.proisstrict
        FROM pg_proc p
        WHERE p.oid =
          'paper_raid_bff_invite_activation_row_valid_v2(jsonb,text,text,uuid,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,timestamp with time zone,timestamp with time zone,timestamp with time zone,text,boolean)'::regprocedure)
       IS DISTINCT FROM TRUE THEN
        RAISE EXCEPTION 'STRICT activation validator tamper was not installed';
    END IF;

    IF EXISTS (
        SELECT 1 FROM pg_proc p
        WHERE p.oid =
          'paper_raid_bff_invite_activation_row_valid_v2(jsonb,text,text,uuid,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,timestamp with time zone,timestamp with time zone,timestamp with time zone,text,boolean)'::regprocedure
          AND NOT p.proisstrict
    ) THEN
        RAISE EXCEPTION 'STRICT validator passed the non-strict readiness predicate';
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
            activation, digest, digest, digest, digest, digest,
            image, image, image, image, image,
            'retention.fixture.v1', digest, digest, digest, digest,
            'database.fixture.v1', 'deployment.fixture.v1',
            '2026-08-11 00:00:00+00', '2026-08-12 00:00:00+00',
            NULL, NULL, FALSE, '{}', '{}'::jsonb
        );
    EXCEPTION WHEN check_violation THEN
        rejected := TRUE;
    END;
    IF NOT rejected THEN
        RAISE EXCEPTION 'outer activation CHECK accepted a STRICT NULL result';
    END IF;
    IF EXISTS (
        SELECT 1 FROM paper_raid_bff_invite_activations
        WHERE activation_id = activation
    ) THEN
        RAISE EXCEPTION 'STRICT-tampered activation row survived rejection';
    END IF;
END
$fixture$;

-- Idempotent schema activation must repair both the function property and the
-- pre-existing table constraint, rather than relying on CREATE TABLE IF NOT
-- EXISTS to update the latter.
\ir ../0007_invite_activation_authority.sql

DO $repair$
BEGIN
    IF (SELECT p.proisstrict
        FROM pg_proc p
        WHERE p.oid =
          'paper_raid_bff_invite_activation_row_valid_v2(jsonb,text,text,uuid,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,timestamp with time zone,timestamp with time zone,timestamp with time zone,text,boolean)'::regprocedure)
       IS DISTINCT FROM FALSE THEN
        RAISE EXCEPTION 'idempotent 0007 did not restore CALLED ON NULL INPUT';
    END IF;
    IF (SELECT regexp_replace(
            pg_get_constraintdef(c.oid), '[[:space:]]+', '', 'g'
        )
        FROM pg_constraint c
        WHERE c.conrelid = 'paper_raid_bff_invite_activations'::regclass
          AND c.conname = 'paper_raid_bff_invite_activation_parity_ck')
       NOT LIKE 'CHECK(COALESCE(paper_raid_bff_invite_activation_row_valid_v2(%' THEN
        RAISE EXCEPTION 'idempotent 0007 did not restore fail-closed CHECK';
    END IF;
END
$repair$;
