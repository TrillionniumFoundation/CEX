\set ON_ERROR_STOP on

\ir ../0003_invite_alpha_access.sql
\ir ../0004_agent_pairing_bridge.sql
\ir ../0008_review_execution_receipts.sql

CREATE FUNCTION pg_temp.fixture_0008_schema_ready()
RETURNS BOOLEAN
LANGUAGE sql
STABLE
AS $ready$
    SELECT
        (SELECT count(*) = 25
         FROM pg_attribute
         WHERE attrelid = 'paper_raid_bff_review_execution_receipts'::regclass
           AND attnum > 0 AND NOT attisdropped)
        AND (SELECT count(*) = 25
             FROM pg_attribute
             WHERE attrelid = 'paper_raid_bff_review_execution_receipts'::regclass
               AND attnum > 0 AND NOT attisdropped
               AND (attname,format_type(atttypid,atttypmod),attnotnull,
                    atthasdef,attgenerated,attidentity) IN (
                    ('receipt_id','uuid',true,false,'',''),
                    ('task_id','uuid',true,false,'',''),
                    ('binding_id','uuid',true,false,'',''),
                    ('assignment_id','uuid',true,false,'',''),
                    ('paper_id','uuid',true,false,'',''),
                    ('submission_id','uuid',true,false,'',''),
                    ('evaluation_id','uuid',true,false,'',''),
                    ('kind','text',true,false,'',''),
                    ('attempt','bigint',true,false,'',''),
                    ('fencing_token','bigint',true,false,'',''),
                    ('bundle_hash','text',true,false,'',''),
                    ('receipt_hash','text',true,false,'',''),
                    ('receipt','jsonb',true,false,'',''),
                    ('state','text',true,false,'',''),
                    ('confirmation_frame','jsonb',false,false,'',''),
                    ('confirmation_frame_hash','text',false,false,'',''),
                    ('confirmation_idempotency_key','uuid',false,false,'',''),
                    ('confirmation_hash','text',false,false,'',''),
                    ('confirmation_context_signature','text',false,false,'',''),
                    ('response_status','integer',false,false,'',''),
                    ('response_body','bytea',false,false,'',''),
                    ('created_at','timestamp with time zone',true,false,'',''),
                    ('consumed_at','timestamp with time zone',false,false,'',''),
                    ('invalidated_at','timestamp with time zone',false,false,'',''),
                    ('updated_at','timestamp with time zone',true,false,'','')
               ))
        AND (SELECT count(*) = 17
             FROM pg_constraint
             WHERE conrelid = 'paper_raid_bff_review_execution_receipts'::regclass
               AND convalidated AND conislocal AND coninhcount = 0
               AND conname IN (
                    'paper_raid_bff_review_receipts_pk',
                    'paper_raid_bff_review_receipt_kind_ck',
                    'paper_raid_bff_review_receipt_attempt_ck',
                    'paper_raid_bff_review_receipt_fencing_ck',
                    'paper_raid_bff_review_receipt_bundle_hash_ck',
                    'paper_raid_bff_review_receipt_hash_ck',
                    'paper_raid_bff_review_receipt_state_ck',
                    'paper_raid_bff_review_receipt_frame_hash_ck',
                    'paper_raid_bff_review_receipt_confirmation_hash_ck',
                    'paper_raid_bff_review_receipt_context_signature_ck',
                    'paper_raid_bff_review_receipt_response_status_ck',
                    'paper_raid_bff_review_receipt_binding_fk',
                    'paper_raid_bff_review_receipt_result_shape_ck',
                    'paper_raid_bff_review_receipt_frame_pair_ck',
                    'paper_raid_bff_review_receipt_frame_binding_ck',
                    'paper_raid_bff_review_receipt_lifecycle_ck',
                    'paper_raid_bff_review_receipt_json_binding_ck'
               ))
        AND EXISTS (
            SELECT 1 FROM pg_constraint
            WHERE conrelid = 'paper_raid_bff_review_execution_receipts'::regclass
              AND conname = 'paper_raid_bff_review_receipt_lifecycle_ck'
              AND convalidated
              AND position('confirmation_context_signature' in pg_get_constraintdef(oid)) > 0
        )
        AND (SELECT count(*) = 4
             FROM pg_index index_row
             JOIN pg_class index_relation ON index_relation.oid = index_row.indexrelid
             WHERE index_row.indrelid = 'paper_raid_bff_review_execution_receipts'::regclass
               AND index_row.indisvalid AND index_row.indisready AND index_row.indislive
               AND index_relation.relname IN (
                    'paper_raid_bff_review_receipts_pk',
                    'paper_raid_bff_one_review_receipt_per_task_attempt',
                    'paper_raid_bff_one_live_review_receipt_per_task',
                    'paper_raid_bff_review_receipt_assignment_inbox_idx'
               ))
        AND EXISTS (
            SELECT 1 FROM pg_index index_row
            JOIN pg_class index_relation ON index_relation.oid = index_row.indexrelid
            WHERE index_row.indrelid = 'paper_raid_bff_review_execution_receipts'::regclass
              AND index_relation.relname = 'paper_raid_bff_one_live_review_receipt_per_task'
              AND index_row.indisunique
              AND regexp_replace(
                    pg_get_expr(index_row.indpred,index_row.indrelid),
                    '[[:space:]]+', '', 'g'
                  ) = '(state=ANY(ARRAY[''pending''::text,''consumed''::text]))'
        )
        AND EXISTS (
            SELECT 1 FROM paper_raid_bff_schema_capabilities
            WHERE capability = 'review_execution_receipts_v1'
        )
        AND obj_description(
            'paper_raid_bff_review_execution_receipts'::regclass, 'pg_class'
        ) = 'Recoverable Agent-signed evaluator/reproducer receipts, each pinned to one assignment version and one immutable frozen review bundle.';
$ready$;

DO $tamper$
DECLARE
    rolled_back BOOLEAN;
BEGIN
    IF NOT pg_temp.fixture_0008_schema_ready() THEN
        RAISE EXCEPTION 'fresh 0008 catalog did not satisfy fixture readiness';
    END IF;

    rolled_back := FALSE;
    BEGIN
        DELETE FROM paper_raid_bff_schema_capabilities
        WHERE capability = 'review_execution_receipts_v1';
        IF pg_temp.fixture_0008_schema_ready() THEN
            RAISE EXCEPTION 'readiness accepted a missing 0008 capability';
        END IF;
        RAISE EXCEPTION 'rollback missing-capability tamper' USING ERRCODE = 'P0001';
    EXCEPTION WHEN SQLSTATE 'P0001' THEN
        IF SQLERRM <> 'rollback missing-capability tamper' THEN RAISE; END IF;
        rolled_back := TRUE;
    END;
    IF NOT rolled_back OR NOT pg_temp.fixture_0008_schema_ready() THEN
        RAISE EXCEPTION 'missing-capability tamper was not rolled back exactly';
    END IF;

    rolled_back := FALSE;
    BEGIN
        DROP INDEX paper_raid_bff_one_live_review_receipt_per_task;
        CREATE UNIQUE INDEX paper_raid_bff_one_live_review_receipt_per_task
            ON paper_raid_bff_review_execution_receipts(task_id)
            WHERE state = 'pending';
        IF pg_temp.fixture_0008_schema_ready() THEN
            RAISE EXCEPTION 'readiness accepted a pending-only live receipt index';
        END IF;
        RAISE EXCEPTION 'rollback live-index tamper' USING ERRCODE = 'P0001';
    EXCEPTION WHEN SQLSTATE 'P0001' THEN
        IF SQLERRM <> 'rollback live-index tamper' THEN RAISE; END IF;
        rolled_back := TRUE;
    END;
    IF NOT rolled_back OR NOT pg_temp.fixture_0008_schema_ready() THEN
        RAISE EXCEPTION 'live-index tamper was not rolled back exactly';
    END IF;

    rolled_back := FALSE;
    BEGIN
        ALTER TABLE paper_raid_bff_review_execution_receipts
            DROP CONSTRAINT paper_raid_bff_review_receipt_lifecycle_ck;
        ALTER TABLE paper_raid_bff_review_execution_receipts
            ADD CONSTRAINT paper_raid_bff_review_receipt_lifecycle_ck
            CHECK (TRUE) NOT VALID;
        IF pg_temp.fixture_0008_schema_ready() THEN
            RAISE EXCEPTION 'readiness accepted an unvalidated lifecycle constraint';
        END IF;
        RAISE EXCEPTION 'rollback lifecycle tamper' USING ERRCODE = 'P0001';
    EXCEPTION WHEN SQLSTATE 'P0001' THEN
        IF SQLERRM <> 'rollback lifecycle tamper' THEN RAISE; END IF;
        rolled_back := TRUE;
    END;
    IF NOT rolled_back OR NOT pg_temp.fixture_0008_schema_ready() THEN
        RAISE EXCEPTION 'lifecycle tamper was not rolled back exactly';
    END IF;

    rolled_back := FALSE;
    BEGIN
        ALTER TABLE paper_raid_bff_review_execution_receipts
            ADD COLUMN unreviewed_authority BYTEA;
        IF pg_temp.fixture_0008_schema_ready() THEN
            RAISE EXCEPTION 'readiness accepted an unreviewed authority column';
        END IF;
        RAISE EXCEPTION 'rollback column tamper' USING ERRCODE = 'P0001';
    EXCEPTION WHEN SQLSTATE 'P0001' THEN
        IF SQLERRM <> 'rollback column tamper' THEN RAISE; END IF;
        rolled_back := TRUE;
    END;
    IF NOT rolled_back OR NOT pg_temp.fixture_0008_schema_ready() THEN
        RAISE EXCEPTION 'column tamper was not rolled back exactly';
    END IF;
END
$tamper$;
