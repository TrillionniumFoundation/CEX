-- Host-local accessctl audit outcome extension. This migration does not add an
-- operator route or any player, Paper, policy, retention, or secret data.

-- Migration 0003 makes the audit ledger append-only. An upgrade can therefore
-- backfill historical rows only after proving that the installed guard is the
-- exact canonical function/trigger pair. DROP TRIGGER takes an ACCESS EXCLUSIVE
-- table lock until this migration transaction commits; any later failure rolls
-- the DROP and all backfill changes back together.
DO $block$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_proc p
        JOIN pg_language l ON l.oid = p.prolang
        WHERE p.oid = to_regprocedure(
                  'paper_raid_bff_reject_access_audit_mutation()'
              )
          AND p.prokind = 'f'
          AND p.prorettype = 'trigger'::regtype
          AND NOT p.proretset
          AND p.pronargs = 0
          AND p.provolatile = 'v'
          AND p.proparallel = 'u'
          AND NOT p.prosecdef
          AND NOT p.proleakproof
          AND p.proconfig IS NULL
          AND l.lanname = 'plpgsql'
          AND regexp_replace(p.prosrc, '[[:space:]]+', '', 'g') =
              'BEGINRAISEEXCEPTION''paper_raid_bff_access_auditisappend-only'';END;'
    ) THEN
        RAISE EXCEPTION
            'access audit append-only function definition is missing or drifted';
    END IF;

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
          AND t.tgnargs = 0
          AND octet_length(t.tgargs) = 0
          AND t.tgattr = ''::int2vector
          AND t.tgqual IS NULL
          AND t.tgconstraint = 0
          AND t.tgconstrrelid = 0
          AND t.tgconstrindid = 0
          AND NOT t.tgdeferrable
          AND NOT t.tginitdeferred
          AND t.tgoldtable IS NULL
          AND t.tgnewtable IS NULL
    ) THEN
        RAISE EXCEPTION
            'access audit append-only trigger definition is missing or drifted';
    END IF;
END;
$block$;

DROP TRIGGER paper_raid_bff_access_audit_append_only
    ON paper_raid_bff_access_audit;

ALTER TABLE paper_raid_bff_access_audit
    DROP CONSTRAINT IF EXISTS paper_raid_bff_access_audit_outcome_check;

ALTER TABLE paper_raid_bff_access_audit
    ADD CONSTRAINT paper_raid_bff_access_audit_outcome_check
    CHECK (outcome IN ('succeeded', 'denied', 'indeterminate'));

ALTER TABLE paper_raid_bff_access_audit
    ADD COLUMN IF NOT EXISTS operator_attempt_id UUID,
    ADD COLUMN IF NOT EXISTS operator_event TEXT,
    ADD COLUMN IF NOT EXISTS operator_lineage_status TEXT;

-- A pre-capability development database can contain rows written by the
-- earlier source slice. Backfill an attempt first, then link a result only
-- when its exact attempt UUID and operator subject are independently present.
-- Malformed operator rows deliberately make the validated constraint fail
-- closed; well-formed historical results with unavailable parentage are kept
-- as explicit legacy_unavailable records rather than being misrepresented as
-- linked.
UPDATE paper_raid_bff_access_audit
SET operator_attempt_id = (metadata ->> 'attempt_id')::uuid,
    operator_event = action,
    operator_lineage_status = 'linked'
WHERE action = 'operator_command_attempt'
  AND metadata ->> 'schema' = 'paper-raid-bff.operator-command-audit.v1'
  AND metadata ->> 'attempt_id' ~
      '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$';

UPDATE paper_raid_bff_access_audit result_row
SET operator_attempt_id = (result_row.metadata ->> 'attempt_id')::uuid,
    operator_event = result_row.action,
    operator_lineage_status = 'linked'
WHERE result_row.action = 'operator_command_result'
  AND result_row.metadata ->> 'schema' =
      'paper-raid-bff.operator-command-audit.v1'
  AND result_row.metadata ->> 'attempt_id' ~
      '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
  AND EXISTS (
      SELECT 1
      FROM paper_raid_bff_access_audit parent
      WHERE parent.operator_attempt_id =
                (result_row.metadata ->> 'attempt_id')::uuid
        AND parent.operator_event = 'operator_command_attempt'
        AND parent.action = 'operator_command_attempt'
        AND parent.operator_lineage_status = 'linked'
        AND parent.operator_subject = result_row.operator_subject
  );

UPDATE paper_raid_bff_access_audit
SET operator_attempt_id = NULL,
    operator_event = NULL,
    operator_lineage_status = 'legacy_unavailable'
WHERE action = 'operator_command_result'
  AND operator_lineage_status IS NULL
  AND metadata ->> 'schema' = 'paper-raid-bff.operator-command-audit.v1'
  AND metadata ->> 'attempt_id' ~
      '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$';

-- Link only an object row that already carries the exact bounded v1 lineage
-- metadata and whose committed attempt parent is independently present.
UPDATE paper_raid_bff_access_audit object_row
SET operator_attempt_id = (object_row.metadata ->> 'attempt_id')::uuid,
    operator_lineage_status = 'linked'
WHERE object_row.action IN (
        'batch_create', 'batch_paused', 'batch_active', 'batch_revoked',
        'invite_issue', 'invite_reissue', 'invite_revoke',
        'credential_rotate', 'account_suspended', 'account_active',
        'account_closed', 'account_export', 'retention_prune'
      )
  AND object_row.metadata ->> 'schema' =
      'paper-raid-bff.accessctl.object-audit.v1'
  AND object_row.metadata ->> 'attempt_id' ~
      '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
  AND EXISTS (
      SELECT 1 FROM paper_raid_bff_access_audit parent
      WHERE parent.operator_attempt_id =
                (object_row.metadata ->> 'attempt_id')::uuid
        AND parent.operator_event = 'operator_command_attempt'
        AND parent.operator_subject = object_row.operator_subject
  );

-- Historical object rows without independently provable linkage remain
-- explicitly unavailable. No attempt UUID is inferred or fabricated.
UPDATE paper_raid_bff_access_audit
SET operator_attempt_id = NULL,
    operator_event = NULL,
    operator_lineage_status = 'legacy_unavailable'
WHERE action IN (
        'batch_create', 'batch_paused', 'batch_active', 'batch_revoked',
        'invite_issue', 'invite_reissue', 'invite_revoke',
        'credential_rotate', 'account_suspended', 'account_active',
        'account_closed', 'account_export', 'retention_prune'
      )
  AND operator_lineage_status IS NULL;

UPDATE paper_raid_bff_access_audit
SET operator_lineage_status = 'not_applicable'
WHERE operator_lineage_status IS NULL;

ALTER TABLE paper_raid_bff_access_audit
    ALTER COLUMN operator_lineage_status SET DEFAULT 'not_applicable',
    ALTER COLUMN operator_lineage_status SET NOT NULL;

ALTER TABLE paper_raid_bff_access_audit
    DROP CONSTRAINT IF EXISTS paper_raid_bff_operator_audit_shape_ck;

CREATE OR REPLACE FUNCTION paper_raid_bff_operator_audit_row_valid(
    action_value TEXT,
    outcome_value TEXT,
    metadata_value JSONB,
    operator_subject_value TEXT,
    account_id_value UUID,
    operator_attempt_id_value UUID,
    operator_event_value TEXT,
    operator_lineage_status_value TEXT
)
RETURNS BOOLEAN
LANGUAGE sql
IMMUTABLE
PARALLEL SAFE
AS $function$
    SELECT COALESCE((CASE
        WHEN action_value NOT IN (
            'operator_command_attempt', 'operator_command_result',
            'batch_create', 'batch_paused', 'batch_active', 'batch_revoked',
            'invite_issue', 'invite_reissue', 'invite_revoke',
            'credential_rotate', 'account_suspended', 'account_active',
            'account_closed', 'account_export', 'retention_prune'
        ) THEN
            operator_attempt_id_value IS NULL
            AND operator_event_value IS NULL
            AND operator_lineage_status_value = 'not_applicable'
        WHEN action_value IN (
            'operator_command_attempt', 'operator_command_result'
        ) THEN
            account_id_value IS NULL
            AND operator_subject_value IS NOT NULL
            AND metadata_value ->> 'attempt_id' ~
                '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND (
                (
                    operator_attempt_id_value IS NOT NULL
                    AND operator_event_value = action_value
                    AND operator_lineage_status_value = 'linked'
                    AND metadata_value ->> 'attempt_id' =
                        operator_attempt_id_value::text
                )
                OR
                (
                    action_value = 'operator_command_result'
                    AND operator_attempt_id_value IS NULL
                    AND operator_event_value IS NULL
                    AND operator_lineage_status_value = 'legacy_unavailable'
                )
            )
            AND metadata_value = jsonb_build_object(
                'schema', 'paper-raid-bff.operator-command-audit.v1',
                'attempt_id', metadata_value ->> 'attempt_id',
                'command_code', metadata_value ->> 'command_code',
                'command_state', metadata_value ->> 'command_state',
                'reason_code', metadata_value ->> 'reason_code'
            )
            AND metadata_value ->> 'command_code' IN (
                'schema_migrate',
                'batch_create', 'batch_pause', 'batch_resume', 'batch_revoke',
                'invite_issue', 'invite_reissue', 'invite_revoke',
                'credential_rotate', 'account_suspend', 'account_reactivate',
                'account_close', 'account_export', 'prune', 'unsupported'
            )
            AND metadata_value ->> 'command_state' IN (
                'not_run', 'not_committed', 'committed', 'unknown'
            )
            AND metadata_value ->> 'reason_code' IN (
                'attempt_recorded', 'command_committed', 'command_rejected',
                'unsupported_command', 'database_operation_failed',
                'identity_mode_rejected', 'database_configuration_missing',
                'operator_configuration_missing', 'operator_configuration_rejected',
                'retention_configuration_missing', 'retention_configuration_rejected',
                'database_connect_unavailable', 'database_migration_unavailable',
                'schema_activation_required', 'command_not_run'
            )
            AND (
                (
                    action_value = 'operator_command_attempt'
                    AND outcome_value = 'succeeded'
                    AND metadata_value ->> 'command_state' = 'not_run'
                    AND metadata_value ->> 'reason_code' = 'attempt_recorded'
                )
                OR
                (
                    action_value = 'operator_command_result'
                    AND (
                        (
                            outcome_value = 'succeeded'
                            AND metadata_value ->> 'command_state' = 'committed'
                            AND metadata_value ->> 'reason_code' = 'command_committed'
                        )
                        OR
                        (
                            outcome_value = 'indeterminate'
                            AND metadata_value ->> 'command_state' = 'unknown'
                            AND metadata_value ->> 'reason_code' = 'database_operation_failed'
                        )
                        OR
                        (
                            outcome_value = 'denied'
                            AND metadata_value ->> 'command_state' IN (
                                'not_run', 'not_committed'
                            )
                            AND metadata_value ->> 'reason_code' NOT IN (
                                'attempt_recorded', 'command_committed',
                                'database_configuration_missing',
                                'operator_configuration_missing',
                                'operator_configuration_rejected',
                                'database_connect_unavailable',
                                'schema_activation_required', 'command_not_run'
                            )
                        )
                    )
                )
            )
        WHEN action_value IN (
            'batch_create', 'batch_paused', 'batch_active', 'batch_revoked',
            'invite_issue', 'invite_reissue', 'invite_revoke',
            'credential_rotate', 'account_suspended', 'account_active',
            'account_closed', 'account_export', 'retention_prune'
        ) THEN
            (
                operator_lineage_status_value = 'legacy_unavailable'
                AND operator_attempt_id_value IS NULL
                AND operator_event_value IS NULL
            )
            OR
            (
                outcome_value = 'succeeded'
                AND operator_subject_value IS NOT NULL
                AND operator_attempt_id_value IS NOT NULL
                AND operator_event_value IS NULL
                AND operator_lineage_status_value = 'linked'
                AND jsonb_typeof(metadata_value) = 'object'
                AND metadata_value ->> 'schema' =
                    'paper-raid-bff.accessctl.object-audit.v1'
                AND metadata_value ->> 'attempt_id' =
                    operator_attempt_id_value::text
            )
        ELSE FALSE
    END), FALSE)
$function$;

-- Migration-time negative contract tests: JSON null must never turn SQL NULL
-- into a passing CHECK result for any closed operator field.
DO $block$
DECLARE
    attempt UUID := '00000000-0000-0000-0000-000000000001';
    field_name TEXT;
    metadata_value JSONB;
BEGIN
    FOREACH field_name IN ARRAY ARRAY[
        'command_code', 'command_state', 'reason_code'
    ] LOOP
        metadata_value := jsonb_build_object(
            'schema', 'paper-raid-bff.operator-command-audit.v1',
            'attempt_id', attempt,
            'command_code', 'invite_issue',
            'command_state', 'not_run',
            'reason_code', 'attempt_recorded'
        ) || jsonb_build_object(field_name, NULL);
        IF paper_raid_bff_operator_audit_row_valid(
            'operator_command_attempt', 'succeeded', metadata_value,
            'migration-self-test', NULL, attempt, 'operator_command_attempt',
            'linked'
        ) IS NOT FALSE THEN
            RAISE EXCEPTION
                'operator audit JSON-null negative test failed for %', field_name;
        END IF;
    END LOOP;
END;
$block$;

ALTER TABLE paper_raid_bff_access_audit
    ADD CONSTRAINT paper_raid_bff_operator_audit_shape_ck CHECK (
        paper_raid_bff_operator_audit_row_valid(
            action,
            outcome,
            metadata,
            operator_subject,
            account_id,
            operator_attempt_id,
            operator_event,
            operator_lineage_status
        )
    );

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_one_operator_event_per_attempt
    ON paper_raid_bff_access_audit(operator_attempt_id, operator_event)
    WHERE operator_attempt_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS paper_raid_bff_object_audit_attempt_idx
    ON paper_raid_bff_access_audit(operator_attempt_id, audit_id)
    WHERE operator_attempt_id IS NOT NULL AND operator_event IS NULL;

CREATE OR REPLACE FUNCTION paper_raid_bff_require_object_audit_attempt()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $function$
BEGIN
    IF TG_OP = 'INSERT'
       AND NEW.operator_lineage_status = 'legacy_unavailable' THEN
        RAISE EXCEPTION
            'new accessctl audit cannot claim legacy-unavailable lineage';
    END IF;
    IF NEW.operator_attempt_id IS NOT NULL
       AND (
           NEW.operator_event IS NULL
           OR NEW.operator_event = 'operator_command_result'
       ) THEN
        IF NOT EXISTS (
            SELECT 1
            FROM paper_raid_bff_access_audit parent
            WHERE parent.operator_attempt_id = NEW.operator_attempt_id
              AND parent.operator_event = 'operator_command_attempt'
              AND parent.action = 'operator_command_attempt'
              AND parent.operator_subject = NEW.operator_subject
        ) THEN
            RAISE EXCEPTION
                'accessctl linked audit has no exact operator attempt parent';
        END IF;
    END IF;
    RETURN NEW;
END;
$function$;

DROP TRIGGER IF EXISTS paper_raid_bff_object_audit_attempt_parent
    ON paper_raid_bff_access_audit;
CREATE TRIGGER paper_raid_bff_object_audit_attempt_parent
BEFORE INSERT ON paper_raid_bff_access_audit
FOR EACH ROW EXECUTE FUNCTION paper_raid_bff_require_object_audit_attempt();

-- Restore the canonical 0003 append-only guard before exposing the capability.
-- This CREATE is in the same migration transaction as the DROP and backfill.
CREATE OR REPLACE FUNCTION paper_raid_bff_reject_access_audit_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'paper_raid_bff_access_audit is append-only';
END;
$$;

CREATE TRIGGER paper_raid_bff_access_audit_append_only
BEFORE UPDATE OR DELETE ON paper_raid_bff_access_audit
FOR EACH ROW EXECUTE FUNCTION paper_raid_bff_reject_access_audit_mutation();

INSERT INTO paper_raid_bff_schema_capabilities(capability)
VALUES ('accessctl_operator_audit_v1')
ON CONFLICT (capability) DO NOTHING;
