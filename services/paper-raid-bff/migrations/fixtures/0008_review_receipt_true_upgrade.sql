\set ON_ERROR_STOP on

-- Run only against an empty, disposable PostgreSQL database.  This is the
-- upgrade fixture for 0008: every prior migration is applied, valid pre-0008
-- rows are committed and snapshotted, and only then is 0008 installed.
\ir ../0001_bff_state.sql
\ir ../0002_product_telemetry.sql
\ir ../0003_invite_alpha_access.sql
\ir ../0004_agent_pairing_bridge.sql
\ir ../0005_agent_delivery_drafts.sql
\ir ../0006_accessctl_operator_audit.sql
\ir ../0007_invite_activation_authority.sql

DO $pre_upgrade_seed$
DECLARE
    fixture_grant_id UUID := '81000000-0000-0000-0000-000000000001';
    fixture_binding_id UUID := '81000000-0000-0000-0000-000000000002';
    fixture_player_id UUID := '81000000-0000-0000-0000-000000000003';
    fixture_session_id UUID := '81000000-0000-0000-0000-000000000004';
    fixture_delivery_id UUID := '81000000-0000-0000-0000-000000000005';
    fixture_capability_disclosure JSONB;
    digest_a TEXT := 'sha256:' || repeat('a', 64);
    digest_b TEXT := 'sha256:' || repeat('b', 64);
BEGIN
    IF to_regclass('paper_raid_bff_review_execution_receipts') IS NOT NULL
       OR EXISTS (
            SELECT 1 FROM paper_raid_bff_schema_capabilities
            WHERE capability = 'review_execution_receipts_v1'
       ) THEN
        RAISE EXCEPTION '0008 authority existed before the upgrade fixture applied 0008';
    END IF;

    fixture_capability_disclosure := jsonb_build_object(
        'schema', 'hepta.paper_raid.agent_capability_disclosure.v1',
        'assurance', 'self_declared_unverified',
        'capabilities', jsonb_build_array('research_session_signing'),
        'resource_classes', jsonb_build_array(),
        'max_parallel_tasks', 1
    );

    INSERT INTO paper_raid_bff_session_generation(
        subject_id, generation, updated_at
    ) VALUES (
        'fixture-pre-0008-review-actor', 7, '2026-08-11 00:00:00+00'
    );
    INSERT INTO paper_raid_bff_sessions(
        session_id, subject_id, generation, csrf_hash, expires_at,
        revoked_at, created_at, last_seen_at
    ) VALUES (
        fixture_session_id, 'fixture-pre-0008-review-actor', 7,
        decode(repeat('03', 32), 'hex'), '2026-08-12 00:00:00+00',
        NULL, '2026-08-11 00:00:00+00', '2026-08-11 00:00:00+00'
    );
    INSERT INTO paper_raid_bff_agent_pairing_grants(
        grant_id, subject_id, player_id, code_hash, state,
        pinned_request_hash, pinned_binding_id, created_at, expires_at,
        pinned_at, consumed_at, revoked_at, pair_response_status,
        pair_response_body, updated_at
    ) VALUES (
        fixture_grant_id, 'fixture-pre-0008-review-actor', fixture_player_id,
        decode(repeat('04', 32), 'hex'), 'consumed',
        decode(repeat('05', 32), 'hex'), fixture_binding_id,
        '2026-08-11 00:00:00+00', '2026-08-11 00:05:00+00',
        '2026-08-11 00:01:00+00', '2026-08-11 00:02:00+00', NULL,
        200, convert_to('{"paired":true}', 'UTF8'),
        '2026-08-11 00:02:00+00'
    );
    INSERT INTO paper_raid_bff_agent_bridge_bindings(
        binding_id, grant_id, last_pairing_grant_id, subject_id, player_id,
        agent_id, agent_key_id, capability_disclosure_hash,
        capability_disclosure, binding_record, paired_at, last_verified_at
    ) VALUES (
        fixture_binding_id, fixture_grant_id, fixture_grant_id,
        'fixture-pre-0008-review-actor', fixture_player_id,
        'fixture-pre-0008-review-agent', digest_a, digest_a,
        fixture_capability_disclosure,
        jsonb_build_object(
            'binding_id', fixture_binding_id::text,
            'player_id', fixture_player_id::text,
            'agent_id', 'fixture-pre-0008-review-agent',
            'agent_key_id', digest_a,
            'capability_disclosure_hash', digest_a,
            'capability_disclosure', fixture_capability_disclosure,
            'status', 'active'
        ),
        '2026-08-11 00:02:00+00', '2026-08-11 00:02:00+00'
    );
    INSERT INTO paper_raid_bff_agent_delivery_drafts(
        delivery_draft_id, binding_id, paper_id, work_item_id,
        expected_work_version, section_key, lease_id, lease_fencing_token,
        parent_revision_id, artifact_manifest_id, artifact_manifest_hash,
        payload_hash, state, created_at, expires_at, updated_at
    ) VALUES (
        fixture_delivery_id, fixture_binding_id,
        '81000000-0000-0000-0000-000000000006',
        '81000000-0000-0000-0000-000000000007',
        3, 'results', '81000000-0000-0000-0000-000000000008', 5,
        '81000000-0000-0000-0000-000000000009',
        '81000000-0000-0000-0000-000000000010', digest_a,
        digest_b, 'pending', '2026-08-11 00:02:00+00',
        '2026-08-11 00:12:00+00', '2026-08-11 00:02:00+00'
    );
END
$pre_upgrade_seed$;

CREATE TEMP TABLE fixture_0008_legacy_rows_before AS
SELECT 'generation'::text AS object_kind, to_jsonb(row_value) AS row_bytes
FROM paper_raid_bff_session_generation row_value
WHERE subject_id = 'fixture-pre-0008-review-actor'
UNION ALL
SELECT 'session', to_jsonb(row_value)
FROM paper_raid_bff_sessions row_value
WHERE session_id = '81000000-0000-0000-0000-000000000004'
UNION ALL
SELECT 'grant', to_jsonb(row_value)
FROM paper_raid_bff_agent_pairing_grants row_value
WHERE grant_id = '81000000-0000-0000-0000-000000000001'
UNION ALL
SELECT 'binding', to_jsonb(row_value)
FROM paper_raid_bff_agent_bridge_bindings row_value
WHERE binding_id = '81000000-0000-0000-0000-000000000002'
UNION ALL
SELECT 'delivery', to_jsonb(row_value)
FROM paper_raid_bff_agent_delivery_drafts row_value
WHERE delivery_draft_id = '81000000-0000-0000-0000-000000000005';

CREATE TEMP TABLE fixture_0008_capabilities_before AS
SELECT capability, installed_at
FROM paper_raid_bff_schema_capabilities;

DO $pre_upgrade_snapshot$
BEGIN
    IF (SELECT count(*) FROM fixture_0008_legacy_rows_before) <> 5 THEN
        RAISE EXCEPTION 'pre-0008 legacy snapshot is incomplete';
    END IF;
    IF (SELECT count(*) FROM fixture_0008_capabilities_before) <> 6 THEN
        RAISE EXCEPTION 'pre-0008 capability set drifted';
    END IF;
END
$pre_upgrade_snapshot$;

\ir ../0008_review_execution_receipts.sql

DO $post_upgrade_verify$
DECLARE
    changed_rows BIGINT;
BEGIN
    WITH rows_after AS (
        SELECT 'generation'::text AS object_kind, to_jsonb(row_value) AS row_bytes
        FROM paper_raid_bff_session_generation row_value
        WHERE subject_id = 'fixture-pre-0008-review-actor'
        UNION ALL
        SELECT 'session', to_jsonb(row_value)
        FROM paper_raid_bff_sessions row_value
        WHERE session_id = '81000000-0000-0000-0000-000000000004'
        UNION ALL
        SELECT 'grant', to_jsonb(row_value)
        FROM paper_raid_bff_agent_pairing_grants row_value
        WHERE grant_id = '81000000-0000-0000-0000-000000000001'
        UNION ALL
        SELECT 'binding', to_jsonb(row_value)
        FROM paper_raid_bff_agent_bridge_bindings row_value
        WHERE binding_id = '81000000-0000-0000-0000-000000000002'
        UNION ALL
        SELECT 'delivery', to_jsonb(row_value)
        FROM paper_raid_bff_agent_delivery_drafts row_value
        WHERE delivery_draft_id = '81000000-0000-0000-0000-000000000005'
    ), differences AS (
        (SELECT * FROM fixture_0008_legacy_rows_before EXCEPT ALL SELECT * FROM rows_after)
        UNION ALL
        (SELECT * FROM rows_after EXCEPT ALL SELECT * FROM fixture_0008_legacy_rows_before)
    )
    SELECT count(*) INTO changed_rows FROM differences;
    IF changed_rows <> 0 THEN
        RAISE EXCEPTION '0008 rewrote or lost valid pre-0008 rows';
    END IF;

    IF EXISTS (
        (SELECT * FROM fixture_0008_capabilities_before
         EXCEPT ALL
         SELECT capability, installed_at FROM paper_raid_bff_schema_capabilities)
    ) THEN
        RAISE EXCEPTION '0008 rewrote or removed a pre-0008 capability';
    END IF;
    IF (SELECT count(*) FROM paper_raid_bff_schema_capabilities) <> 7
       OR (SELECT count(*) FROM paper_raid_bff_schema_capabilities
           WHERE capability = 'review_execution_receipts_v1') <> 1 THEN
        RAISE EXCEPTION '0008 did not add exactly one Review receipt capability';
    END IF;
    IF to_regclass('paper_raid_bff_review_execution_receipts') IS NULL
       OR (SELECT count(*) FROM paper_raid_bff_review_execution_receipts) <> 0 THEN
        RAISE EXCEPTION '0008 did not create one empty Review receipt authority table';
    END IF;
END
$post_upgrade_verify$;

CREATE TEMP TABLE fixture_0008_capability_after_install AS
SELECT capability, installed_at
FROM paper_raid_bff_schema_capabilities
WHERE capability = 'review_execution_receipts_v1';

-- The migration is replay-safe and must not mutate either the legacy bytes or
-- the capability installation timestamp.
\ir ../0008_review_execution_receipts.sql

DO $replay_verify$
DECLARE
    changed_rows BIGINT;
BEGIN
    IF EXISTS (
        (SELECT * FROM fixture_0008_capability_after_install
         EXCEPT ALL
         SELECT capability, installed_at
         FROM paper_raid_bff_schema_capabilities
         WHERE capability = 'review_execution_receipts_v1')
        UNION ALL
        (SELECT capability, installed_at
         FROM paper_raid_bff_schema_capabilities
         WHERE capability = 'review_execution_receipts_v1'
         EXCEPT ALL
         SELECT * FROM fixture_0008_capability_after_install)
    ) THEN
        RAISE EXCEPTION '0008 replay rewrote or lost its capability';
    END IF;
    IF (SELECT count(*) FROM paper_raid_bff_review_execution_receipts) <> 0 THEN
        RAISE EXCEPTION '0008 replay introduced a Review receipt row';
    END IF;

    WITH rows_after_replay AS (
        SELECT 'generation'::text AS object_kind, to_jsonb(row_value) AS row_bytes
        FROM paper_raid_bff_session_generation row_value
        WHERE subject_id = 'fixture-pre-0008-review-actor'
        UNION ALL
        SELECT 'session', to_jsonb(row_value)
        FROM paper_raid_bff_sessions row_value
        WHERE session_id = '81000000-0000-0000-0000-000000000004'
        UNION ALL
        SELECT 'grant', to_jsonb(row_value)
        FROM paper_raid_bff_agent_pairing_grants row_value
        WHERE grant_id = '81000000-0000-0000-0000-000000000001'
        UNION ALL
        SELECT 'binding', to_jsonb(row_value)
        FROM paper_raid_bff_agent_bridge_bindings row_value
        WHERE binding_id = '81000000-0000-0000-0000-000000000002'
        UNION ALL
        SELECT 'delivery', to_jsonb(row_value)
        FROM paper_raid_bff_agent_delivery_drafts row_value
        WHERE delivery_draft_id = '81000000-0000-0000-0000-000000000005'
    ), differences AS (
        (SELECT * FROM fixture_0008_legacy_rows_before
         EXCEPT ALL SELECT * FROM rows_after_replay)
        UNION ALL
        (SELECT * FROM rows_after_replay
         EXCEPT ALL SELECT * FROM fixture_0008_legacy_rows_before)
    )
    SELECT count(*) INTO changed_rows FROM differences;
    IF changed_rows <> 0 THEN
        RAISE EXCEPTION '0008 replay rewrote or lost valid pre-0008 rows';
    END IF;
END
$replay_verify$;

-- Mirror the current runtime role contract for this one table: the BFF may
-- store/recover receipts, but cannot delete, truncate, reference, trigger, or
-- grant privileges. Frozen Review Bundle authority is never persisted in a
-- BFF table; it remains assignment-scoped Hepta/CAS authority.
DO $runtime_role$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'paper_raid_bff_runtime') THEN
        CREATE ROLE paper_raid_bff_runtime NOLOGIN NOINHERIT;
    END IF;
END
$runtime_role$;
REVOKE ALL ON TABLE paper_raid_bff_review_execution_receipts FROM PUBLIC;
REVOKE ALL ON TABLE paper_raid_bff_review_execution_receipts FROM paper_raid_bff_runtime;
GRANT SELECT, INSERT, UPDATE ON TABLE paper_raid_bff_review_execution_receipts
TO paper_raid_bff_runtime;

DO $runtime_acl_verify$
DECLARE
    privilege_name TEXT;
    expected BOOLEAN;
BEGIN
    FOREACH privilege_name IN ARRAY ARRAY[
        'SELECT','INSERT','UPDATE','DELETE','TRUNCATE','REFERENCES','TRIGGER','MAINTAIN'
    ]
    LOOP
        expected := privilege_name = ANY(ARRAY['SELECT','INSERT','UPDATE']::text[]);
        IF has_table_privilege(
            'paper_raid_bff_runtime',
            'paper_raid_bff_review_execution_receipts',
            privilege_name
        ) IS DISTINCT FROM expected THEN
            RAISE EXCEPTION '0008 runtime receipt ACL drifted for %', privilege_name;
        END IF;
        IF has_table_privilege(
            'paper_raid_bff_runtime',
            'paper_raid_bff_review_execution_receipts',
            privilege_name || ' WITH GRANT OPTION'
        ) THEN
            RAISE EXCEPTION '0008 runtime receipt ACL became grantable for %',
                privilege_name;
        END IF;
    END LOOP;
    IF EXISTS (
        SELECT 1
        FROM pg_class relation
        JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
        WHERE namespace.nspname = 'public'
          AND relation.relkind IN ('r','p','v','m','f')
          AND relation.relname LIKE 'paper_raid_bff%review%bundle%'
    ) THEN
        RAISE EXCEPTION 'BFF persisted a Frozen Review Bundle authority table';
    END IF;
END
$runtime_acl_verify$;
