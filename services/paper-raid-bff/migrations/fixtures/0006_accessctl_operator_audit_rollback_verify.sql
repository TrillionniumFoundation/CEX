\set ON_ERROR_STOP on

-- Run from a new PostgreSQL connection after the expected failure in
-- 0006_accessctl_operator_audit_rollback_after_drop.sql.
DO $fixture$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_trigger t
        WHERE t.tgname = 'paper_raid_bff_access_audit_append_only'
          AND t.tgrelid = 'paper_raid_bff_access_audit'::regclass
          AND NOT t.tgisinternal
          AND t.tgenabled = 'O'
          AND t.tgtype = 27
          AND t.tgfoid =
              'paper_raid_bff_reject_access_audit_mutation()'::regprocedure
    ) THEN
        RAISE EXCEPTION 'rollback did not restore the canonical append-only guard';
    END IF;

    IF (SELECT metadata
        FROM paper_raid_bff_access_audit
        WHERE audit_id = '30000000-0000-0000-0000-000000000001')
       IS DISTINCT FROM '{"fixture":"before_0006"}'::jsonb THEN
        RAISE EXCEPTION 'rollback did not restore the pre-migration audit row';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM pg_attribute
        WHERE attrelid = 'paper_raid_bff_access_audit'::regclass
          AND attname IN (
              'operator_attempt_id', 'operator_event',
              'operator_lineage_status'
          )
          AND attnum > 0
          AND NOT attisdropped
    ) THEN
        RAISE EXCEPTION 'rollback left 0006 audit columns installed';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'paper_raid_bff_access_audit'::regclass
          AND conname = 'paper_raid_bff_access_audit_outcome_check'
          AND contype = 'c'
          AND convalidated
          AND regexp_replace(
              pg_get_constraintdef(oid), '[[:space:]]+', '', 'g'
          ) = 'CHECK((outcome=ANY(ARRAY[''succeeded''::text,''denied''::text])))'
    ) THEN
        RAISE EXCEPTION 'rollback did not restore the pre-0006 outcome constraint';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM paper_raid_bff_schema_capabilities
        WHERE capability = 'accessctl_operator_audit_v1'
    ) THEN
        RAISE EXCEPTION 'rollback left the 0006 capability exposed';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM paper_raid_bff_schema_capabilities
        WHERE capability = 'invite_alpha_access_v1'
    ) THEN
        RAISE EXCEPTION 'rollback lost the pre-0006 capability';
    END IF;
END;
$fixture$;
