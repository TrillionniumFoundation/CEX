\set ON_ERROR_STOP on

-- Expected to fail. The post-b5 PostgreSQL runner must execute this fixture
-- in a disposable database, then open a new connection and run the matching
-- rollback verification fixture. The explicit transaction models the
-- sqlx::raw_sql migration boundary and proves that a failure after DROP rolls
-- the guard, rows, constraint, columns, and capability back together.
\ir ../0003_invite_alpha_access.sql

INSERT INTO paper_raid_bff_access_audit(
    audit_id, operator_subject, account_id, action, outcome, metadata
) VALUES (
    '30000000-0000-0000-0000-000000000001',
    'rollback-fixture-operator', NULL, 'invite_issue', 'succeeded',
    '{"fixture":"before_0006"}'::jsonb
);

BEGIN;

DROP TRIGGER paper_raid_bff_access_audit_append_only
    ON paper_raid_bff_access_audit;

ALTER TABLE paper_raid_bff_access_audit
    DROP CONSTRAINT paper_raid_bff_access_audit_outcome_check;

ALTER TABLE paper_raid_bff_access_audit
    ADD CONSTRAINT paper_raid_bff_access_audit_outcome_check
    CHECK (outcome IN ('succeeded', 'denied', 'indeterminate'));

ALTER TABLE paper_raid_bff_access_audit
    ADD COLUMN operator_attempt_id UUID,
    ADD COLUMN operator_event TEXT,
    ADD COLUMN operator_lineage_status TEXT;

UPDATE paper_raid_bff_access_audit
SET metadata = '{"fixture":"mutated_after_drop"}'::jsonb
WHERE audit_id = '30000000-0000-0000-0000-000000000001';

INSERT INTO paper_raid_bff_schema_capabilities(capability)
VALUES ('accessctl_operator_audit_v1');

DO $fixture$
BEGIN
    RAISE EXCEPTION '0006 fixture deliberate failure after DROP';
END;
$fixture$;
