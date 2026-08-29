\set ON_ERROR_STOP on

-- Run only against an empty, disposable PostgreSQL database. This fixture
-- constructs the real 0003 schema, seeds pre-0006 audit history, then applies
-- 0006 exactly as an upgrade migration.
\ir ../0003_invite_alpha_access.sql

INSERT INTO paper_raid_bff_access_audit(
    audit_id, operator_subject, account_id, action, outcome, metadata
) VALUES
(
    '10000000-0000-0000-0000-000000000001', NULL, NULL,
    'login', 'succeeded', '{"legacy":"non_accessctl"}'::jsonb
),
(
    '10000000-0000-0000-0000-000000000002', 'legacy-operator', NULL,
    'invite_issue', 'succeeded', '{"legacy":"unlinked_object"}'::jsonb
),
(
    '10000000-0000-0000-0000-000000000003', 'linked-operator', NULL,
    'operator_command_attempt', 'succeeded',
    '{
      "schema":"paper-raid-bff.operator-command-audit.v1",
      "attempt_id":"20000000-0000-0000-0000-000000000001",
      "command_code":"invite_issue",
      "command_state":"not_run",
      "reason_code":"attempt_recorded"
    }'::jsonb
),
(
    '10000000-0000-0000-0000-000000000004', 'linked-operator', NULL,
    'invite_reissue', 'succeeded',
    '{
      "schema":"paper-raid-bff.accessctl.object-audit.v1",
      "attempt_id":"20000000-0000-0000-0000-000000000001",
      "reason":"fixture"
    }'::jsonb
),
(
    '10000000-0000-0000-0000-000000000005', 'linked-operator', NULL,
    'operator_command_result', 'succeeded',
    '{
      "schema":"paper-raid-bff.operator-command-audit.v1",
      "attempt_id":"20000000-0000-0000-0000-000000000001",
      "command_code":"invite_issue",
      "command_state":"committed",
      "reason_code":"command_committed"
    }'::jsonb
),
(
    '10000000-0000-0000-0000-000000000006', 'orphan-operator', NULL,
    'operator_command_result', 'denied',
    '{
      "schema":"paper-raid-bff.operator-command-audit.v1",
      "attempt_id":"20000000-0000-0000-0000-000000000002",
      "command_code":"invite_issue",
      "command_state":"not_committed",
      "reason_code":"command_rejected"
    }'::jsonb
),
(
    '10000000-0000-0000-0000-000000000007', 'different-operator', NULL,
    'operator_command_result', 'denied',
    '{
      "schema":"paper-raid-bff.operator-command-audit.v1",
      "attempt_id":"20000000-0000-0000-0000-000000000001",
      "command_code":"invite_issue",
      "command_state":"not_committed",
      "reason_code":"command_rejected"
    }'::jsonb
);

\ir ../0006_accessctl_operator_audit.sql

DO $fixture$
DECLARE
    rejected_message TEXT;
BEGIN
    IF (SELECT operator_lineage_status
        FROM paper_raid_bff_access_audit
        WHERE audit_id = '10000000-0000-0000-0000-000000000001')
       IS DISTINCT FROM 'not_applicable' THEN
        RAISE EXCEPTION 'non-accessctl history was not marked not_applicable';
    END IF;

    IF (SELECT jsonb_build_array(
            operator_attempt_id, operator_event, operator_lineage_status
        )
        FROM paper_raid_bff_access_audit
        WHERE audit_id = '10000000-0000-0000-0000-000000000002')
       IS DISTINCT FROM '[null,null,"legacy_unavailable"]'::jsonb THEN
        RAISE EXCEPTION 'unlinked object history was not preserved as legacy_unavailable';
    END IF;

    IF (SELECT jsonb_build_array(
            operator_attempt_id, operator_event, operator_lineage_status
        )
        FROM paper_raid_bff_access_audit
        WHERE audit_id = '10000000-0000-0000-0000-000000000003')
       IS DISTINCT FROM
          '["20000000-0000-0000-0000-000000000001","operator_command_attempt","linked"]'::jsonb THEN
        RAISE EXCEPTION 'operator attempt history was not linked exactly';
    END IF;

    IF (SELECT jsonb_build_array(
            operator_attempt_id, operator_event, operator_lineage_status
        )
        FROM paper_raid_bff_access_audit
        WHERE audit_id = '10000000-0000-0000-0000-000000000004')
       IS DISTINCT FROM
          '["20000000-0000-0000-0000-000000000001",null,"linked"]'::jsonb THEN
        RAISE EXCEPTION 'object history was not linked to its exact parent';
    END IF;

    IF (SELECT jsonb_build_array(
            operator_attempt_id, operator_event, operator_lineage_status
        )
        FROM paper_raid_bff_access_audit
        WHERE audit_id = '10000000-0000-0000-0000-000000000005')
       IS DISTINCT FROM
          '["20000000-0000-0000-0000-000000000001","operator_command_result","linked"]'::jsonb THEN
        RAISE EXCEPTION 'operator result history was not linked to its exact parent';
    END IF;

    IF (SELECT jsonb_build_array(
            operator_attempt_id, operator_event, operator_lineage_status
        )
        FROM paper_raid_bff_access_audit
        WHERE audit_id = '10000000-0000-0000-0000-000000000006')
       IS DISTINCT FROM '[null,null,"legacy_unavailable"]'::jsonb THEN
        RAISE EXCEPTION 'orphan result history was not downgraded to legacy_unavailable';
    END IF;

    IF (SELECT jsonb_build_array(
            operator_attempt_id, operator_event, operator_lineage_status
        )
        FROM paper_raid_bff_access_audit
        WHERE audit_id = '10000000-0000-0000-0000-000000000007')
       IS DISTINCT FROM '[null,null,"legacy_unavailable"]'::jsonb THEN
        RAISE EXCEPTION 'subject-mismatched result history was not downgraded to legacy_unavailable';
    END IF;

    BEGIN
        UPDATE paper_raid_bff_access_audit
        SET metadata = metadata
        WHERE audit_id = '10000000-0000-0000-0000-000000000001';
        RAISE EXCEPTION 'append-only update unexpectedly succeeded';
    EXCEPTION WHEN raise_exception THEN
        GET STACKED DIAGNOSTICS rejected_message = MESSAGE_TEXT;
        IF rejected_message IS DISTINCT FROM
           'paper_raid_bff_access_audit is append-only' THEN
            RAISE EXCEPTION 'canonical append-only trigger was not restored: %',
                rejected_message;
        END IF;
    END;

    BEGIN
        INSERT INTO paper_raid_bff_access_audit(
            audit_id, operator_subject, account_id, action, outcome, metadata,
            operator_attempt_id, operator_event, operator_lineage_status
        ) VALUES (
            '10000000-0000-0000-0000-000000000008', 'new-operator', NULL,
            'invite_issue', 'succeeded', '{}'::jsonb,
            NULL, NULL, 'legacy_unavailable'
        );
        RAISE EXCEPTION 'new legacy_unavailable row unexpectedly succeeded';
    EXCEPTION WHEN raise_exception THEN
        GET STACKED DIAGNOSTICS rejected_message = MESSAGE_TEXT;
        IF rejected_message IS DISTINCT FROM
           'new accessctl audit cannot claim legacy-unavailable lineage' THEN
            RAISE EXCEPTION 'new legacy lineage rejection drifted: %',
                rejected_message;
        END IF;
    END;

    BEGIN
        INSERT INTO paper_raid_bff_access_audit(
            audit_id, operator_subject, account_id, action, outcome, metadata,
            operator_attempt_id, operator_event, operator_lineage_status
        ) VALUES (
            '10000000-0000-0000-0000-000000000009', 'new-operator', NULL,
            'operator_command_result', 'denied',
            '{
              "schema":"paper-raid-bff.operator-command-audit.v1",
              "attempt_id":"20000000-0000-0000-0000-000000000003",
              "command_code":"invite_issue",
              "command_state":"not_committed",
              "reason_code":"command_rejected"
            }'::jsonb,
            '20000000-0000-0000-0000-000000000003',
            'operator_command_result', 'linked'
        );
        RAISE EXCEPTION 'new orphan operator result unexpectedly succeeded';
    EXCEPTION WHEN raise_exception THEN
        GET STACKED DIAGNOSTICS rejected_message = MESSAGE_TEXT;
        IF rejected_message IS DISTINCT FROM
           'accessctl linked audit has no exact operator attempt parent' THEN
            RAISE EXCEPTION 'new orphan result rejection drifted: %',
                rejected_message;
        END IF;
    END;

    BEGIN
        INSERT INTO paper_raid_bff_access_audit(
            audit_id, operator_subject, account_id, action, outcome, metadata,
            operator_attempt_id, operator_event, operator_lineage_status
        ) VALUES (
            '10000000-0000-0000-0000-000000000010', 'different-operator', NULL,
            'operator_command_result', 'denied',
            '{
              "schema":"paper-raid-bff.operator-command-audit.v1",
              "attempt_id":"20000000-0000-0000-0000-000000000001",
              "command_code":"invite_issue",
              "command_state":"not_committed",
              "reason_code":"command_rejected"
            }'::jsonb,
            '20000000-0000-0000-0000-000000000001',
            'operator_command_result', 'linked'
        );
        RAISE EXCEPTION 'new subject-mismatched operator result unexpectedly succeeded';
    EXCEPTION WHEN raise_exception THEN
        GET STACKED DIAGNOSTICS rejected_message = MESSAGE_TEXT;
        IF rejected_message IS DISTINCT FROM
           'accessctl linked audit has no exact operator attempt parent' THEN
            RAISE EXCEPTION 'new subject mismatch rejection drifted: %',
                rejected_message;
        END IF;
    END;
END;
$fixture$;
