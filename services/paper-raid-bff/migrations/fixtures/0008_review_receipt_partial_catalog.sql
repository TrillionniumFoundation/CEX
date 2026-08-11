\set ON_ERROR_STOP on

-- CREATE TABLE IF NOT EXISTS must never turn a hostile/partial pre-existing
-- table into a ready schema merely by adding the capability and indexes.
\ir ../0003_invite_alpha_access.sql
\ir ../0004_agent_pairing_bridge.sql

CREATE TABLE paper_raid_bff_review_execution_receipts (
    receipt_id UUID PRIMARY KEY,
    task_id UUID NOT NULL,
    attempt BIGINT NOT NULL,
    assignment_id UUID NOT NULL,
    state TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

\ir ../0008_review_execution_receipts.sql

DO $partial_catalog$
DECLARE
    fixture_schema_ready BOOLEAN;
BEGIN
    fixture_schema_ready :=
        to_regclass('paper_raid_bff_review_execution_receipts') IS NOT NULL
        AND (SELECT count(*) = 25
             FROM pg_attribute
             WHERE attrelid = 'paper_raid_bff_review_execution_receipts'::regclass
               AND attnum > 0 AND NOT attisdropped)
        AND EXISTS (
            SELECT 1 FROM pg_constraint
            WHERE conrelid = 'paper_raid_bff_review_execution_receipts'::regclass
              AND conname = 'paper_raid_bff_review_receipt_json_binding_ck'
              AND contype = 'c' AND convalidated
        )
        AND EXISTS (
            SELECT 1 FROM paper_raid_bff_schema_capabilities
            WHERE capability = 'review_execution_receipts_v1'
        );
    IF fixture_schema_ready THEN
        RAISE EXCEPTION 'partial 0008 catalog was accepted as ready';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM paper_raid_bff_schema_capabilities
        WHERE capability = 'review_execution_receipts_v1'
    ) OR to_regclass('paper_raid_bff_one_live_review_receipt_per_task') IS NULL THEN
        RAISE EXCEPTION 'partial-catalog fixture did not exercise the deceptive capability/index state';
    END IF;
    IF EXISTS (
        SELECT 1 FROM pg_attribute
        WHERE attrelid = 'paper_raid_bff_review_execution_receipts'::regclass
          AND attname IN ('binding_id','paper_id','receipt','confirmation_context_signature')
          AND NOT attisdropped
    ) THEN
        RAISE EXCEPTION 'partial-catalog fixture unexpectedly acquired omitted authority columns';
    END IF;
END
$partial_catalog$;
