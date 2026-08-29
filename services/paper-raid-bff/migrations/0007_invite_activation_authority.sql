-- Invite Alpha activation authority. The original v1/v2 contract remains
-- below as inert migration history. V3 later in this migration is the only
-- contract accepted by resident readiness: it retains the root-provisioned
-- local approval and ACL evidence byte-for-byte and binds them to immutable,
-- revocable, expiring authority. The resident BFF has read-only access.

BEGIN;

CREATE OR REPLACE FUNCTION paper_raid_bff_runtime_acl_state_v2()
RETURNS TEXT
LANGUAGE sql
STABLE
PARALLEL SAFE
SET search_path = pg_catalog, public
AS $function$
WITH runtime_role AS (
    SELECT r.oid, r.rolname, r.rolcanlogin, r.rolinherit, r.rolsuper,
           r.rolcreatedb, r.rolcreaterole, r.rolreplication, r.rolbypassrls,
           COALESCE(to_jsonb(r.rolconfig), '[]'::jsonb) AS role_config
    FROM pg_roles r
    WHERE r.rolname = 'paper_raid_bff_runtime'
), relation_privileges AS (
    SELECT n.nspname AS schema_name, c.relname AS object_name, privilege_name,
           has_table_privilege(
               'paper_raid_bff_runtime', c.oid,
               privilege_name || ' WITH GRANT OPTION'
           ) AS grantable
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    CROSS JOIN unnest(ARRAY[
        'SELECT','INSERT','UPDATE','DELETE','TRUNCATE','REFERENCES','TRIGGER',
        'MAINTAIN'
    ]::text[]) AS privilege_name
    WHERE n.nspname = 'public'
      AND c.relkind IN ('r','p','v','m','f')
      AND has_table_privilege(
          'paper_raid_bff_runtime', c.oid, privilege_name
      )
), sequence_privileges AS (
    SELECT n.nspname AS schema_name, c.relname AS object_name, privilege_name,
           has_sequence_privilege(
               'paper_raid_bff_runtime', c.oid,
               privilege_name || ' WITH GRANT OPTION'
           ) AS grantable
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    CROSS JOIN unnest(ARRAY['SELECT','UPDATE','USAGE']::text[]) AS privilege_name
    WHERE n.nspname = 'public'
      AND c.relkind = 'S'
      AND has_sequence_privilege(
          'paper_raid_bff_runtime', c.oid, privilege_name
      )
), column_acl_entries AS (
    SELECT n.nspname AS schema_name, c.relname AS object_name,
           a.attname AS column_name,
           CASE WHEN acl.grantee = 0 THEN 'PUBLIC' ELSE grantee.rolname END AS grantee,
           acl.privilege_type, acl.is_grantable
    FROM pg_attribute a
    JOIN pg_class c ON c.oid = a.attrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    CROSS JOIN LATERAL aclexplode(a.attacl) acl
    LEFT JOIN pg_roles grantee ON grantee.oid = acl.grantee
    WHERE n.nspname = 'public'
      AND a.attnum > 0 AND NOT a.attisdropped
      AND (acl.grantee = 0 OR grantee.rolname = 'paper_raid_bff_runtime')
), function_privileges AS (
    SELECT n.nspname AS schema_name,
           regexp_replace(p.oid::regprocedure::text, '^public\.', '') AS object_name,
           'EXECUTE'::text AS privilege_name,
           has_function_privilege(
               'paper_raid_bff_runtime', p.oid, 'EXECUTE WITH GRANT OPTION'
           ) AS grantable
    FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'public'
      AND has_function_privilege(
          'paper_raid_bff_runtime', p.oid, 'EXECUTE'
      )
), default_acl_entries AS (
    SELECT COALESCE(n.nspname, '') AS schema_name,
           owner.rolname AS owner_name,
           d.defaclobjtype::text AS object_type,
           CASE WHEN acl.grantee = 0 THEN 'PUBLIC' ELSE grantee.rolname END AS grantee,
           acl.privilege_type, acl.is_grantable
    FROM pg_default_acl d
    JOIN pg_roles owner ON owner.oid = d.defaclrole
    LEFT JOIN pg_namespace n ON n.oid = d.defaclnamespace
    CROSS JOIN LATERAL aclexplode(d.defaclacl) acl
    LEFT JOIN pg_roles grantee ON grantee.oid = acl.grantee
    WHERE COALESCE(n.nspname, 'public') = 'public'
      AND (acl.grantee = 0 OR grantee.rolname = 'paper_raid_bff_runtime')
), database_acl_entries AS (
    SELECT CASE WHEN acl.grantee = 0 THEN 'PUBLIC' ELSE grantee.rolname END AS grantee,
           acl.privilege_type, acl.is_grantable
      FROM pg_database database_row
      CROSS JOIN LATERAL aclexplode(COALESCE(
          database_row.datacl, acldefault('d', database_row.datdba)
      )) acl
      LEFT JOIN pg_roles grantee ON grantee.oid = acl.grantee
     WHERE database_row.datname = current_database()
), schema_acl_entries AS (
    SELECT namespace.nspname AS schema_name,
           CASE WHEN acl.grantee = 0 THEN 'PUBLIC' ELSE grantee.rolname END AS grantee,
           acl.privilege_type, acl.is_grantable
      FROM pg_namespace namespace
      CROSS JOIN LATERAL aclexplode(COALESCE(
          namespace.nspacl, acldefault('n', namespace.nspowner)
      )) acl
      LEFT JOIN pg_roles grantee ON grantee.oid = acl.grantee
     WHERE acl.grantee = 0 OR grantee.rolname = 'paper_raid_bff_runtime'
), parameter_acl_entries AS (
    SELECT parameter.parname AS parameter_name,
           CASE WHEN acl.grantee = 0 THEN 'PUBLIC' ELSE grantee.rolname END AS grantee,
           acl.privilege_type, acl.is_grantable
      FROM pg_parameter_acl parameter
      CROSS JOIN LATERAL aclexplode(parameter.paracl) acl
      LEFT JOIN pg_roles grantee ON grantee.oid = acl.grantee
     WHERE acl.grantee = 0 OR grantee.rolname = 'paper_raid_bff_runtime'
), database_role_settings AS (
    SELECT COALESCE(database_row.datname, '') AS database_name,
           COALESCE(role_row.rolname, '') AS role_name,
           to_jsonb(setting.setconfig) AS settings
      FROM pg_db_role_setting setting
      LEFT JOIN pg_database database_row ON database_row.oid = setting.setdatabase
      LEFT JOIN pg_roles role_row ON role_row.oid = setting.setrole
     WHERE setting.setdatabase IN (
               0, (SELECT oid FROM pg_database WHERE datname = current_database())
           )
       AND setting.setrole IN (
               0, (SELECT oid FROM pg_roles WHERE rolname = 'paper_raid_bff_runtime')
           )
), activation_function_acl_entries AS (
    SELECT namespace.nspname AS schema_name,
           procedure.oid::regprocedure::text AS function_name,
           pg_get_userbyid(procedure.proowner) AS function_owner,
           CASE WHEN acl.grantee = 0 THEN 'PUBLIC' ELSE grantee.rolname END AS grantee,
           acl.privilege_type, acl.is_grantable
      FROM pg_proc procedure
      JOIN pg_namespace namespace ON namespace.oid = procedure.pronamespace
      CROSS JOIN LATERAL aclexplode(COALESCE(
          procedure.proacl, acldefault('f', procedure.proowner)
      )) acl
      LEFT JOIN pg_roles grantee ON grantee.oid = acl.grantee
     WHERE procedure.oid IN (
         to_regprocedure('pg_catalog.pg_control_system()'),
         to_regprocedure('public.paper_raid_bff_cluster_identity_v3()')
     )
), membership_edges AS (
    SELECT granted.rolname AS granted_role, member_role.rolname AS member_role
    FROM pg_auth_members membership
    JOIN pg_roles granted ON granted.oid = membership.roleid
    JOIN pg_roles member_role ON member_role.oid = membership.member
    WHERE granted.rolname = 'paper_raid_bff_runtime'
       OR member_role.rolname = 'paper_raid_bff_runtime'
)
SELECT jsonb_build_object(
    'schema', 'paper-raid-bff.runtime-acl-state.v2',
    'database', current_database(),
    'database_owner', (
        SELECT pg_get_userbyid(database_row.datdba)
          FROM pg_database database_row
         WHERE database_row.datname = current_database()
    ),
    'role', (SELECT jsonb_build_object(
        'name', rolname,
        'can_login', rolcanlogin,
        'inherit', rolinherit,
        'superuser', rolsuper,
        'create_database', rolcreatedb,
        'create_role', rolcreaterole,
        'replication', rolreplication,
        'bypass_rls', rolbypassrls,
        'config', role_config
    ) FROM runtime_role),
    'membership_edges', COALESCE((
        SELECT jsonb_agg(jsonb_build_array(granted_role, member_role)
                         ORDER BY granted_role, member_role)
        FROM membership_edges
    ), '[]'::jsonb),
    'database_connect', has_database_privilege(
        'paper_raid_bff_runtime', current_database(), 'CONNECT'
    ),
    'database_connect_grantable', has_database_privilege(
        'paper_raid_bff_runtime', current_database(), 'CONNECT WITH GRANT OPTION'
    ),
    'database_temporary', has_database_privilege(
        'paper_raid_bff_runtime', current_database(), 'TEMP'
    ),
    'database_temporary_grantable', has_database_privilege(
        'paper_raid_bff_runtime', current_database(), 'TEMP WITH GRANT OPTION'
    ),
    'database_create', has_database_privilege(
        'paper_raid_bff_runtime', current_database(), 'CREATE'
    ),
    'database_create_grantable', has_database_privilege(
        'paper_raid_bff_runtime', current_database(), 'CREATE WITH GRANT OPTION'
    ),
    'database_acls', COALESCE((
        SELECT jsonb_agg(jsonb_build_array(
                   grantee, privilege_type, is_grantable
               ) ORDER BY grantee, privilege_type, is_grantable)
          FROM database_acl_entries
    ), '[]'::jsonb),
    'database_role_settings', COALESCE((
        SELECT jsonb_agg(jsonb_build_array(
                   database_name, role_name, settings
               ) ORDER BY database_name, role_name, settings::text)
          FROM database_role_settings
    ), '[]'::jsonb),
    'schema_usage', has_schema_privilege(
        'paper_raid_bff_runtime', 'public', 'USAGE'
    ),
    'schema_usage_grantable', has_schema_privilege(
        'paper_raid_bff_runtime', 'public', 'USAGE WITH GRANT OPTION'
    ),
    'schema_create', has_schema_privilege(
        'paper_raid_bff_runtime', 'public', 'CREATE'
    ),
    'schema_create_grantable', has_schema_privilege(
        'paper_raid_bff_runtime', 'public', 'CREATE WITH GRANT OPTION'
    ),
    'schema_acls', COALESCE((
        SELECT jsonb_agg(jsonb_build_array(
                   schema_name, grantee, privilege_type, is_grantable
               ) ORDER BY schema_name, grantee, privilege_type, is_grantable)
          FROM schema_acl_entries
    ), '[]'::jsonb),
    'parameter_acls', COALESCE((
        SELECT jsonb_agg(jsonb_build_array(
                   parameter_name, grantee, privilege_type, is_grantable
               ) ORDER BY parameter_name, grantee, privilege_type, is_grantable)
          FROM parameter_acl_entries
    ), '[]'::jsonb),
    'activation_function_acls', COALESCE((
        SELECT jsonb_agg(jsonb_build_array(
                   schema_name, function_name, function_owner, grantee,
                   privilege_type, is_grantable
               ) ORDER BY schema_name, function_name, function_owner, grantee,
                          privilege_type, is_grantable)
          FROM activation_function_acl_entries
    ), '[]'::jsonb),
    'relations', COALESCE((
        SELECT jsonb_agg(jsonb_build_array(
                   schema_name, object_name, privilege_name, grantable
               ) ORDER BY schema_name, object_name, privilege_name, grantable)
        FROM relation_privileges
    ), '[]'::jsonb),
    'sequences', COALESCE((
        SELECT jsonb_agg(jsonb_build_array(
                   schema_name, object_name, privilege_name, grantable
               ) ORDER BY schema_name, object_name, privilege_name, grantable)
        FROM sequence_privileges
    ), '[]'::jsonb),
    'column_acls', COALESCE((
        SELECT jsonb_agg(jsonb_build_array(
                   schema_name, object_name, column_name, grantee, privilege_type,
                   is_grantable
               ) ORDER BY schema_name, object_name, column_name, grantee,
                          privilege_type, is_grantable)
        FROM column_acl_entries
    ), '[]'::jsonb),
    'functions', COALESCE((
        SELECT jsonb_agg(jsonb_build_array(
                   schema_name, object_name, privilege_name, grantable
               ) ORDER BY schema_name, object_name, privilege_name, grantable)
        FROM function_privileges
    ), '[]'::jsonb),
    'default_acls', COALESCE((
        SELECT jsonb_agg(jsonb_build_array(
                   schema_name, owner_name, object_type, grantee, privilege_type,
                   is_grantable
               ) ORDER BY schema_name, owner_name, object_type, grantee,
                          privilege_type, is_grantable)
        FROM default_acl_entries
    ), '[]'::jsonb)
)::text
$function$;

DROP VIEW IF EXISTS paper_raid_bff_runtime_acl_state_v1;
CREATE OR REPLACE VIEW paper_raid_bff_runtime_acl_state_v2
WITH (security_barrier = true)
AS
SELECT paper_raid_bff_runtime_acl_state_v2() AS canonical_state;

CREATE OR REPLACE FUNCTION paper_raid_bff_invite_activation_row_valid_v2(
    receipt_value JSONB,
    approval_receipt_v1_value TEXT,
    approval_receipt_v1_sha256_value TEXT,
    activation_id_value UUID,
    profile_sha256_value TEXT,
    base_compose_sha256_value TEXT,
    runtime_acl_sha256_value TEXT,
    runtime_acl_state_sha256_value TEXT,
    bff_image_value TEXT,
    accessctl_image_value TEXT,
    postgres_image_value TEXT,
    ops_image_value TEXT,
    hepta_image_value TEXT,
    retention_policy_id_value TEXT,
    retention_policy_sha256_value TEXT,
    image_lock_sha256_value TEXT,
    release_provenance_sha256_value TEXT,
    runtime_acl_evidence_sha256_value TEXT,
    database_identity_value TEXT,
    deployment_identity_value TEXT,
    issued_at_value TIMESTAMPTZ,
    expires_at_value TIMESTAMPTZ,
    revoked_at_value TIMESTAMPTZ,
    revocation_reason_value TEXT,
    economy_eligibility_value BOOLEAN
)
RETURNS BOOLEAN
LANGUAGE sql
IMMUTABLE
PARALLEL SAFE
CALLED ON NULL INPUT
SET search_path = pg_catalog, public
AS $function$
    SELECT COALESCE(
        approval_receipt_v1_sha256_value ~ '^sha256:[0-9a-f]{64}$'
        AND approval_receipt_v1_sha256_value = 'sha256:' || encode(
            sha256(convert_to(approval_receipt_v1_value, 'UTF8')), 'hex'
        )
        AND jsonb_typeof(approval_receipt_v1_value::jsonb) = 'object'
        AND (approval_receipt_v1_value::jsonb ->> 'schema') =
            'trnm.paper-raid.invite-alpha-activation-receipt.v1'
        AND (approval_receipt_v1_value::jsonb ->> 'activation') =
            'externally_approved'
        AND approval_receipt_v1_value::jsonb -> 'economy_eligibility' = 'false'::jsonb
        AND jsonb_typeof(receipt_value) = 'object'
        AND (SELECT count(*) FROM jsonb_object_keys(receipt_value)) = 22
        AND receipt_value ->> 'schema' =
            'trnm.paper-raid.invite-alpha-activation-receipt.v2'
        AND receipt_value ->> 'approval_receipt_v1_sha256' =
            approval_receipt_v1_sha256_value
        AND (receipt_value ->> 'activation_id')::uuid = activation_id_value
        AND receipt_value ->> 'profile_sha256' = profile_sha256_value
        AND receipt_value ->> 'base_compose_sha256' = base_compose_sha256_value
        AND receipt_value ->> 'runtime_acl_sha256' = runtime_acl_sha256_value
        AND receipt_value ->> 'runtime_acl_state_sha256' =
            runtime_acl_state_sha256_value
        AND receipt_value ->> 'bff_image' = bff_image_value
        AND receipt_value ->> 'accessctl_image' = accessctl_image_value
        AND receipt_value ->> 'postgres_image' = postgres_image_value
        AND receipt_value ->> 'ops_image' = ops_image_value
        AND receipt_value ->> 'hepta_image' = hepta_image_value
        AND receipt_value ->> 'retention_policy_id' = retention_policy_id_value
        AND receipt_value ->> 'retention_policy_sha256' = retention_policy_sha256_value
        AND receipt_value ->> 'image_lock_sha256' = image_lock_sha256_value
        AND receipt_value ->> 'release_provenance_sha256' =
            release_provenance_sha256_value
        AND receipt_value ->> 'runtime_acl_evidence_sha256' =
            runtime_acl_evidence_sha256_value
        AND receipt_value ->> 'database_identity' = database_identity_value
        AND receipt_value ->> 'deployment_identity' = deployment_identity_value
        AND (receipt_value ->> 'issued_at')::timestamptz = issued_at_value
        AND (receipt_value ->> 'expires_at')::timestamptz = expires_at_value
        AND receipt_value -> 'economy_eligibility' = 'false'::jsonb
        AND (approval_receipt_v1_value::jsonb ->> 'profile_sha256') =
            profile_sha256_value
        AND (approval_receipt_v1_value::jsonb ->> 'runtime_acl_sha256') =
            runtime_acl_sha256_value
        AND (approval_receipt_v1_value::jsonb #>> '{images,bff}') = bff_image_value
        AND (approval_receipt_v1_value::jsonb #>> '{images,accessctl}') =
            accessctl_image_value
        AND (approval_receipt_v1_value::jsonb #>> '{images,postgres}') =
            postgres_image_value
        AND (approval_receipt_v1_value::jsonb #>> '{images,ops}') = ops_image_value
        AND (approval_receipt_v1_value::jsonb #>> '{images,hepta}') = hepta_image_value
        AND (approval_receipt_v1_value::jsonb #>> '{retention,policy_id}') =
            retention_policy_id_value
        AND (approval_receipt_v1_value::jsonb #>> '{retention,policy_sha256}') =
            retention_policy_sha256_value
        AND (approval_receipt_v1_value::jsonb #>> '{evidence,image_lock_sha256}') =
            image_lock_sha256_value
        AND (approval_receipt_v1_value::jsonb #>> '{evidence,release_provenance_sha256}') =
            release_provenance_sha256_value
        AND (approval_receipt_v1_value::jsonb #>>
             '{evidence,runtime_acl_verification_sha256}') =
            runtime_acl_evidence_sha256_value
        AND profile_sha256_value ~ '^sha256:[0-9a-f]{64}$'
        AND base_compose_sha256_value ~ '^sha256:[0-9a-f]{64}$'
        AND runtime_acl_sha256_value ~ '^sha256:[0-9a-f]{64}$'
        AND runtime_acl_state_sha256_value ~ '^sha256:[0-9a-f]{64}$'
        AND retention_policy_sha256_value ~ '^sha256:[0-9a-f]{64}$'
        AND image_lock_sha256_value ~ '^sha256:[0-9a-f]{64}$'
        AND release_provenance_sha256_value ~ '^sha256:[0-9a-f]{64}$'
        AND runtime_acl_evidence_sha256_value ~ '^sha256:[0-9a-f]{64}$'
        AND bff_image_value ~ '^[a-z0-9][a-z0-9._/:@-]*@sha256:[0-9a-f]{64}$'
        AND accessctl_image_value ~ '^[a-z0-9][a-z0-9._/:@-]*@sha256:[0-9a-f]{64}$'
        AND postgres_image_value ~ '^[a-z0-9][a-z0-9._/:@-]*@sha256:[0-9a-f]{64}$'
        AND ops_image_value ~ '^[a-z0-9][a-z0-9._/:@-]*@sha256:[0-9a-f]{64}$'
        AND hepta_image_value ~ '^[a-z0-9][a-z0-9._/:@-]*@sha256:[0-9a-f]{64}$'
        AND retention_policy_id_value ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
        AND database_identity_value ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
        AND deployment_identity_value ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
        AND expires_at_value > issued_at_value
        AND (
            (revoked_at_value IS NULL AND revocation_reason_value IS NULL)
            OR (
                revoked_at_value IS NOT NULL
                AND revocation_reason_value IS NOT NULL
                AND revoked_at_value >= issued_at_value
                AND revocation_reason_value IN (
                    'operator_revoked','superseded','acl_drift'
                )
            )
        )
        AND economy_eligibility_value = FALSE
        AND NOT EXISTS (
            SELECT 1 FROM jsonb_object_keys(receipt_value) AS key
            WHERE key NOT IN (
                'schema','approval_receipt_v1_sha256','activation_id',
                'profile_sha256','base_compose_sha256','runtime_acl_sha256',
                'runtime_acl_state_sha256','bff_image','accessctl_image',
                'postgres_image','ops_image','hepta_image',
                'retention_policy_id','retention_policy_sha256',
                'image_lock_sha256','release_provenance_sha256',
                'runtime_acl_evidence_sha256','database_identity',
                'deployment_identity','issued_at','expires_at',
                'economy_eligibility'
            )
        ),
        FALSE
    )
$function$;

CREATE TABLE IF NOT EXISTS paper_raid_bff_invite_activations (
    activation_id UUID,
    approval_receipt_v1_sha256 TEXT NOT NULL,
    profile_sha256 TEXT NOT NULL,
    base_compose_sha256 TEXT NOT NULL,
    runtime_acl_sha256 TEXT NOT NULL,
    runtime_acl_state_sha256 TEXT NOT NULL,
    bff_image TEXT NOT NULL,
    accessctl_image TEXT NOT NULL,
    postgres_image TEXT NOT NULL,
    ops_image TEXT NOT NULL,
    hepta_image TEXT NOT NULL,
    retention_policy_id TEXT NOT NULL,
    retention_policy_sha256 TEXT NOT NULL,
    image_lock_sha256 TEXT NOT NULL,
    release_provenance_sha256 TEXT NOT NULL,
    runtime_acl_evidence_sha256 TEXT NOT NULL,
    database_identity TEXT NOT NULL,
    deployment_identity TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revocation_reason TEXT,
    economy_eligibility BOOLEAN NOT NULL DEFAULT FALSE,
    approval_receipt_v1 TEXT NOT NULL,
    receipt JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT paper_raid_bff_invite_activations_pkey PRIMARY KEY (activation_id),
    CONSTRAINT paper_raid_bff_invite_activation_parity_ck CHECK (
        COALESCE(
            paper_raid_bff_invite_activation_row_valid_v2(
                receipt, approval_receipt_v1, approval_receipt_v1_sha256,
                activation_id, profile_sha256, base_compose_sha256,
                runtime_acl_sha256, runtime_acl_state_sha256, bff_image,
                accessctl_image, postgres_image, ops_image, hepta_image,
                retention_policy_id, retention_policy_sha256,
                image_lock_sha256, release_provenance_sha256,
                runtime_acl_evidence_sha256, database_identity,
                deployment_identity, issued_at, expires_at, revoked_at,
                revocation_reason, economy_eligibility
            ),
            FALSE
        )
    )
);

-- CREATE TABLE IF NOT EXISTS does not refresh a pre-existing constraint.
-- Replace it atomically so an idempotent schema-migrate also upgrades an
-- earlier 0007 installation whose CHECK did not wrap a NULL result.
ALTER TABLE paper_raid_bff_invite_activations
    DROP CONSTRAINT IF EXISTS paper_raid_bff_invite_activation_parity_ck,
    ADD CONSTRAINT paper_raid_bff_invite_activation_parity_ck CHECK (
        COALESCE(
            paper_raid_bff_invite_activation_row_valid_v2(
                receipt, approval_receipt_v1, approval_receipt_v1_sha256,
                activation_id, profile_sha256, base_compose_sha256,
                runtime_acl_sha256, runtime_acl_state_sha256, bff_image,
                accessctl_image, postgres_image, ops_image, hepta_image,
                retention_policy_id, retention_policy_sha256,
                image_lock_sha256, release_provenance_sha256,
                runtime_acl_evidence_sha256, database_identity,
                deployment_identity, issued_at, expires_at, revoked_at,
                revocation_reason, economy_eligibility
            ),
            FALSE
        )
    );

CREATE UNIQUE INDEX IF NOT EXISTS paper_raid_bff_one_active_invite_activation
    ON paper_raid_bff_invite_activations ((TRUE))
    WHERE revoked_at IS NULL;

CREATE INDEX IF NOT EXISTS paper_raid_bff_invite_activation_expiry
    ON paper_raid_bff_invite_activations(expires_at)
    WHERE revoked_at IS NULL;

INSERT INTO paper_raid_bff_schema_capabilities(capability)
VALUES ('invite_activation_authority_v2')
ON CONFLICT (capability) DO NOTHING;

-- V3 is the first activation authority which treats the root-provisioned,
-- canonical approval receipt itself as the local trust authority. V2 remains
-- as inert migration history; resident Invite Alpha readiness accepts V3 only.
-- Public or remote activation is deliberately not represented by this schema.

CREATE OR REPLACE FUNCTION paper_raid_bff_cluster_identity_v3()
RETURNS TABLE(
    database_name TEXT,
    database_oid OID,
    cluster_system_identifier TEXT
)
LANGUAGE sql
STABLE
SECURITY DEFINER
PARALLEL RESTRICTED
SET search_path = pg_catalog
AS $function$
SELECT current_database()::text,
       database_row.oid,
       control_row.system_identifier::text
  FROM pg_database database_row
 CROSS JOIN pg_control_system() control_row
 WHERE database_row.datname = current_database()
$function$;

ALTER FUNCTION paper_raid_bff_cluster_identity_v3() OWNER TO paper_raid_bff;
REVOKE ALL ON FUNCTION paper_raid_bff_cluster_identity_v3() FROM PUBLIC;

CREATE OR REPLACE FUNCTION paper_raid_bff_invite_activation_row_valid_v3(
    approval_record_value JSONB,
    local_approval_value TEXT,
    local_approval_sha256_value TEXT,
    runtime_acl_evidence_value TEXT,
    runtime_acl_evidence_sha256_value TEXT,
    activation_id_value UUID,
    deployment_identity_value TEXT,
    database_name_value TEXT,
    database_oid_value OID,
    cluster_system_identifier_value TEXT,
    approval_sequence_value BIGINT,
    nonce_sha256_value TEXT,
    issued_at_value TIMESTAMPTZ,
    expires_at_value TIMESTAMPTZ,
    revoked_at_value TIMESTAMPTZ,
    revocation_reason_value TEXT,
    economy_eligibility_value BOOLEAN
)
RETURNS BOOLEAN
LANGUAGE sql
IMMUTABLE
PARALLEL SAFE
CALLED ON NULL INPUT
SET search_path = pg_catalog, public
AS $function$
SELECT COALESCE((
    approval_record_value = local_approval_value::jsonb
    AND local_approval_sha256_value = 'sha256:' || encode(
        sha256(convert_to(local_approval_value, 'UTF8')), 'hex'
    )
    AND runtime_acl_evidence_sha256_value = 'sha256:' || encode(
        sha256(convert_to(runtime_acl_evidence_value, 'UTF8')), 'hex'
    )
    AND jsonb_typeof(approval_record_value) = 'object'
    AND approval_record_value ->> 'schema' =
        'trnm.paper-raid.invite-alpha-local-approval.v3'
    AND approval_record_value ->> 'authority' = 'root_provisioned_local_only'
    AND (approval_record_value ->> 'economy_eligibility')::boolean = FALSE
    AND economy_eligibility_value = FALSE
    AND (
        SELECT count(*) = 13 AND bool_and(key = ANY(ARRAY[
            'activation','authority','base_compose_sha256','database','economy_eligibility',
            'evidence','hepta','images','profile_sha256','retention',
            'release_id','runtime_acl_sha256','schema'
        ]::text[]))
        FROM jsonb_object_keys(approval_record_value) key
    )
    AND jsonb_typeof(approval_record_value -> 'activation') = 'object'
    AND (
        SELECT count(*) = 6 AND bool_and(key = ANY(ARRAY[
            'activation_id','deployment_identity','expires_at','issued_at',
            'nonce_sha256','sequence'
        ]::text[]))
        FROM jsonb_object_keys(approval_record_value -> 'activation') key
    )
    AND approval_record_value #>> '{activation,activation_id}' =
        activation_id_value::text
    AND approval_record_value #>> '{activation,deployment_identity}' =
        deployment_identity_value
    AND jsonb_typeof(approval_record_value #> '{activation,sequence}') = 'number'
    AND (approval_record_value #>> '{activation,sequence}')::bigint =
        approval_sequence_value
    AND approval_record_value #>> '{activation,nonce_sha256}' =
        nonce_sha256_value
    AND approval_record_value #>> '{activation,issued_at}' ~
        '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$'
    AND approval_record_value #>> '{activation,expires_at}' ~
        '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$'
    AND (approval_record_value #>> '{activation,issued_at}')::timestamptz =
        issued_at_value
    AND (approval_record_value #>> '{activation,expires_at}')::timestamptz =
        expires_at_value
    AND jsonb_typeof(approval_record_value -> 'database') = 'object'
    AND (
        SELECT count(*) = 3 AND bool_and(key = ANY(ARRAY[
            'name','oid','system_identifier'
        ]::text[]))
        FROM jsonb_object_keys(approval_record_value -> 'database') key
    )
    AND approval_record_value #>> '{database,name}' = database_name_value
    AND jsonb_typeof(approval_record_value #> '{database,oid}') = 'number'
    AND (approval_record_value #>> '{database,oid}')::bigint =
        database_oid_value::bigint
    AND approval_record_value #>> '{database,system_identifier}' =
        cluster_system_identifier_value
    AND jsonb_typeof(approval_record_value -> 'evidence') = 'object'
    AND (
        SELECT count(*) = 3 AND bool_and(key = ANY(ARRAY[
            'image_lock_sha256','release_provenance_sha256',
            'runtime_acl_verification_sha256'
        ]::text[]))
        FROM jsonb_object_keys(approval_record_value -> 'evidence') key
    )
    AND jsonb_typeof(approval_record_value -> 'hepta') = 'object'
    AND (
        SELECT count(*) = 5 AND bool_and(key = ANY(ARRAY[
            'clean','committed','fileset_sha256','revision','source_tree'
        ]::text[]))
        FROM jsonb_object_keys(approval_record_value -> 'hepta') key
    )
    AND (approval_record_value #>> '{hepta,clean}')::boolean = TRUE
    AND (approval_record_value #>> '{hepta,committed}')::boolean = TRUE
    AND jsonb_typeof(approval_record_value -> 'images') = 'object'
    AND (
        SELECT count(*) = 8 AND bool_and(key = ANY(ARRAY[
            'accessctl','bff','hepta','nakama','object_store',
            'object_store_client','ops','postgres'
        ]::text[]))
        FROM jsonb_object_keys(approval_record_value -> 'images') key
    )
    AND jsonb_typeof(approval_record_value -> 'retention') = 'object'
    AND (
        SELECT count(*) = 2 AND bool_and(key = ANY(ARRAY[
            'policy_id','policy_sha256'
        ]::text[]))
        FROM jsonb_object_keys(approval_record_value -> 'retention') key
    )
    AND approval_record_value ->> 'profile_sha256' ~ '^sha256:[0-9a-f]{64}$'
    AND approval_record_value ->> 'release_id' ~ '^[a-z0-9][a-z0-9._-]{0,127}$'
    AND approval_record_value ->> 'base_compose_sha256' ~ '^sha256:[0-9a-f]{64}$'
    AND approval_record_value ->> 'runtime_acl_sha256' ~ '^sha256:[0-9a-f]{64}$'
    AND approval_record_value #>> '{hepta,fileset_sha256}' ~
        '^sha256:[0-9a-f]{64}$'
    AND approval_record_value #>> '{hepta,revision}' ~ '^[0-9a-f]{40}$'
    AND approval_record_value #>> '{hepta,source_tree}' ~ '^[0-9a-f]{40}$'
    AND approval_record_value #>> '{retention,policy_id}' ~
        '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
    AND approval_record_value #>> '{retention,policy_sha256}' ~
        '^sha256:[0-9a-f]{64}$'
    AND approval_record_value #>> '{evidence,image_lock_sha256}' ~
        '^sha256:[0-9a-f]{64}$'
    AND approval_record_value #>> '{evidence,release_provenance_sha256}' ~
        '^sha256:[0-9a-f]{64}$'
    AND approval_record_value #>> '{evidence,runtime_acl_verification_sha256}' =
        runtime_acl_evidence_sha256_value
    AND (
        SELECT bool_and(value ~
            '^[a-z0-9][a-z0-9._/:@-]*@sha256:[0-9a-f]{64}$')
        FROM jsonb_each_text(approval_record_value -> 'images') image(key, value)
    )
    AND local_approval_sha256_value ~ '^sha256:[0-9a-f]{64}$'
    AND runtime_acl_evidence_sha256_value ~ '^sha256:[0-9a-f]{64}$'
    AND nonce_sha256_value ~ '^sha256:[0-9a-f]{64}$'
    AND deployment_identity_value ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
    AND database_name_value ~ '^[A-Za-z0-9][A-Za-z0-9_-]{0,62}$'
    AND database_oid_value::bigint BETWEEN 1 AND 4294967295
    AND cluster_system_identifier_value ~ '^[1-9][0-9]{0,19}$'
    AND approval_sequence_value BETWEEN 1 AND 9007199254740991
    AND expires_at_value - issued_at_value BETWEEN
        interval '60 seconds' AND interval '86400 seconds'
    AND (
        (revoked_at_value IS NULL AND revocation_reason_value IS NULL)
        OR (
            revoked_at_value IS NOT NULL
            AND revocation_reason_value IN (
                'operator_revoked','superseded','acl_drift'
            )
            AND revoked_at_value >= issued_at_value
        )
    )
    AND jsonb_typeof(runtime_acl_evidence_value::jsonb) = 'object'
    AND (
        SELECT count(*) = 5 AND bool_and(key = ANY(ARRAY[
            'database','runtime_acl_sha256','runtime_acl_state_sha256',
            'schema','verified'
        ]::text[]))
        FROM jsonb_object_keys(runtime_acl_evidence_value::jsonb) key
    )
    AND runtime_acl_evidence_value::jsonb ->> 'schema' =
        'trnm.paper-raid.invite-alpha-runtime-acl-evidence.v2'
    AND (runtime_acl_evidence_value::jsonb ->> 'verified')::boolean = TRUE
    AND jsonb_typeof(runtime_acl_evidence_value::jsonb -> 'database') = 'object'
    AND (
        SELECT count(*) = 3 AND bool_and(key = ANY(ARRAY[
            'name','oid','system_identifier'
        ]::text[]))
        FROM jsonb_object_keys(runtime_acl_evidence_value::jsonb -> 'database') key
    )
    AND runtime_acl_evidence_value::jsonb #>> '{database,name}' =
        database_name_value
    AND (runtime_acl_evidence_value::jsonb #>> '{database,oid}')::bigint =
        database_oid_value::bigint
    AND runtime_acl_evidence_value::jsonb #>> '{database,system_identifier}' =
        cluster_system_identifier_value
    AND runtime_acl_evidence_value::jsonb ->> 'runtime_acl_sha256' =
        approval_record_value ->> 'runtime_acl_sha256'
    AND runtime_acl_evidence_value::jsonb ->> 'runtime_acl_state_sha256' ~
        '^sha256:[0-9a-f]{64}$'
), FALSE)
$function$;

ALTER FUNCTION paper_raid_bff_invite_activation_row_valid_v3(
    JSONB, TEXT, TEXT, TEXT, TEXT, UUID, TEXT, TEXT, OID, TEXT, BIGINT,
    TEXT, TIMESTAMPTZ, TIMESTAMPTZ, TIMESTAMPTZ, TEXT, BOOLEAN
) OWNER TO paper_raid_bff;
REVOKE ALL ON FUNCTION paper_raid_bff_invite_activation_row_valid_v3(
    JSONB, TEXT, TEXT, TEXT, TEXT, UUID, TEXT, TEXT, OID, TEXT, BIGINT,
    TEXT, TIMESTAMPTZ, TIMESTAMPTZ, TIMESTAMPTZ, TEXT, BOOLEAN
) FROM PUBLIC;

CREATE TABLE IF NOT EXISTS paper_raid_bff_invite_activation_authorities_v3 (
    activation_id UUID NOT NULL,
    local_approval TEXT NOT NULL,
    local_approval_sha256 TEXT NOT NULL,
    runtime_acl_evidence TEXT NOT NULL,
    runtime_acl_evidence_sha256 TEXT NOT NULL,
    deployment_identity TEXT NOT NULL,
    database_name TEXT NOT NULL,
    database_oid OID NOT NULL,
    cluster_system_identifier TEXT NOT NULL,
    approval_sequence BIGINT NOT NULL,
    nonce_sha256 TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revocation_reason TEXT,
    economy_eligibility BOOLEAN NOT NULL DEFAULT FALSE,
    approval_record JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE paper_raid_bff_invite_activation_authorities_v3
    OWNER TO paper_raid_bff;
ALTER TABLE paper_raid_bff_invite_activation_authorities_v3
    DISABLE ROW LEVEL SECURITY;
ALTER TABLE paper_raid_bff_invite_activation_authorities_v3
    NO FORCE ROW LEVEL SECURITY;

DO $reset_v3_relation_options$
DECLARE
    option_name TEXT;
BEGIN
    FOR option_name IN
        SELECT split_part(option_value, '=', 1)
          FROM pg_class relation
          CROSS JOIN LATERAL unnest(
              COALESCE(relation.reloptions, ARRAY[]::TEXT[])
          ) option_value
         WHERE relation.oid =
            'paper_raid_bff_invite_activation_authorities_v3'::regclass
    LOOP
        EXECUTE format(
            'ALTER TABLE paper_raid_bff_invite_activation_authorities_v3 RESET (%I)',
            option_name
        );
    END LOOP;
END
$reset_v3_relation_options$;

CREATE OR REPLACE FUNCTION paper_raid_bff_invite_activation_monotonic_v3()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = pg_catalog, public
AS $function$
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended(
        'paper-raid-bff.invite-activation.v3|' || NEW.deployment_identity ||
        '|' || NEW.cluster_system_identifier,
        0
    ));
    IF EXISTS (
        SELECT 1
          FROM paper_raid_bff_invite_activation_authorities_v3 existing
         WHERE existing.deployment_identity = NEW.deployment_identity
           AND existing.cluster_system_identifier = NEW.cluster_system_identifier
           AND existing.approval_sequence >= NEW.approval_sequence
           AND existing.activation_id <> NEW.activation_id
    ) THEN
        RAISE EXCEPTION 'invite activation sequence is not strictly monotonic';
    END IF;
    RETURN NEW;
END
$function$;

ALTER FUNCTION paper_raid_bff_invite_activation_monotonic_v3()
    OWNER TO paper_raid_bff;
REVOKE ALL ON FUNCTION paper_raid_bff_invite_activation_monotonic_v3()
    FROM PUBLIC;

CREATE OR REPLACE FUNCTION paper_raid_bff_invite_activation_immutable_v3()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = pg_catalog, public
AS $function$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'invite activation tombstones cannot be deleted';
    END IF;
    IF ROW(
        NEW.activation_id, NEW.local_approval, NEW.local_approval_sha256,
        NEW.runtime_acl_evidence, NEW.runtime_acl_evidence_sha256,
        NEW.deployment_identity, NEW.database_name, NEW.database_oid,
        NEW.cluster_system_identifier, NEW.approval_sequence,
        NEW.nonce_sha256, NEW.issued_at, NEW.expires_at,
        NEW.economy_eligibility, NEW.approval_record, NEW.created_at
    ) IS DISTINCT FROM ROW(
        OLD.activation_id, OLD.local_approval, OLD.local_approval_sha256,
        OLD.runtime_acl_evidence, OLD.runtime_acl_evidence_sha256,
        OLD.deployment_identity, OLD.database_name, OLD.database_oid,
        OLD.cluster_system_identifier, OLD.approval_sequence,
        OLD.nonce_sha256, OLD.issued_at, OLD.expires_at,
        OLD.economy_eligibility, OLD.approval_record, OLD.created_at
    ) THEN
        RAISE EXCEPTION 'invite activation authority is immutable';
    END IF;
    IF OLD.revoked_at IS NOT NULL AND ROW(NEW.revoked_at, NEW.revocation_reason)
        IS DISTINCT FROM ROW(OLD.revoked_at, OLD.revocation_reason) THEN
        RAISE EXCEPTION 'invite activation tombstone is immutable';
    END IF;
    IF OLD.revoked_at IS NULL AND NEW.revoked_at IS NULL AND
       NEW.revocation_reason IS NOT NULL THEN
        RAISE EXCEPTION 'invite activation revocation is incomplete';
    END IF;
    RETURN NEW;
END
$function$;

ALTER FUNCTION paper_raid_bff_invite_activation_immutable_v3()
    OWNER TO paper_raid_bff;
REVOKE ALL ON FUNCTION paper_raid_bff_invite_activation_immutable_v3()
    FROM PUBLIC;

CREATE OR REPLACE FUNCTION paper_raid_bff_invite_activation_truncate_v3()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = pg_catalog
AS $function$
BEGIN
    RAISE EXCEPTION 'invite activation tombstones cannot be truncated';
END
$function$;

ALTER FUNCTION paper_raid_bff_invite_activation_truncate_v3()
    OWNER TO paper_raid_bff;
REVOKE ALL ON FUNCTION paper_raid_bff_invite_activation_truncate_v3()
    FROM PUBLIC;

LOCK TABLE paper_raid_bff_invite_activation_authorities_v3
    IN ACCESS EXCLUSIVE MODE;

DO $validate_v3_authority_data$
DECLARE
    catalog_ok BOOLEAN;
BEGIN
    SELECT relation.relkind = 'r'
       AND relation.relpersistence = 'p'
       AND pg_get_userbyid(relation.relowner) = 'paper_raid_bff'
       AND NOT relation.relrowsecurity
       AND NOT relation.relforcerowsecurity
       AND COALESCE(relation.reloptions, ARRAY[]::TEXT[]) = ARRAY[]::TEXT[]
       AND (
           SELECT count(*) = 18 AND bool_and(
               attribute.attnum = expected.attnum
               AND attribute.attname = expected.attname
               AND attribute.atttypid = expected.atttypid
               AND attribute.attnotnull = expected.attnotnull
               AND attribute.atthasdef = expected.atthasdef
               AND COALESCE(
                   pg_get_expr(default_value.adbin, default_value.adrelid), ''
               ) = expected.default_expression
               AND attribute.attgenerated = ''
               AND attribute.attidentity = ''
           )
           FROM (VALUES
               (1,'activation_id','uuid'::regtype,TRUE,FALSE,''),
               (2,'local_approval','text'::regtype,TRUE,FALSE,''),
               (3,'local_approval_sha256','text'::regtype,TRUE,FALSE,''),
               (4,'runtime_acl_evidence','text'::regtype,TRUE,FALSE,''),
               (5,'runtime_acl_evidence_sha256','text'::regtype,TRUE,FALSE,''),
               (6,'deployment_identity','text'::regtype,TRUE,FALSE,''),
               (7,'database_name','text'::regtype,TRUE,FALSE,''),
               (8,'database_oid','oid'::regtype,TRUE,FALSE,''),
               (9,'cluster_system_identifier','text'::regtype,TRUE,FALSE,''),
               (10,'approval_sequence','int8'::regtype,TRUE,FALSE,''),
               (11,'nonce_sha256','text'::regtype,TRUE,FALSE,''),
               (12,'issued_at','timestamptz'::regtype,TRUE,FALSE,''),
               (13,'expires_at','timestamptz'::regtype,TRUE,FALSE,''),
               (14,'revoked_at','timestamptz'::regtype,FALSE,FALSE,''),
               (15,'revocation_reason','text'::regtype,FALSE,FALSE,''),
               (16,'economy_eligibility','bool'::regtype,TRUE,TRUE,'false'),
               (17,'approval_record','jsonb'::regtype,TRUE,FALSE,''),
               (18,'created_at','timestamptz'::regtype,TRUE,TRUE,'now()')
           ) expected(
               attnum, attname, atttypid, attnotnull, atthasdef,
               default_expression
           )
           LEFT JOIN pg_attribute attribute
             ON attribute.attrelid = relation.oid
            AND attribute.attnum = expected.attnum
            AND NOT attribute.attisdropped
           LEFT JOIN pg_attrdef default_value
             ON default_value.adrelid = attribute.attrelid
            AND default_value.adnum = attribute.attnum
       )
       AND (
           SELECT count(*) = 18
             FROM pg_attribute attribute
            WHERE attribute.attrelid = relation.oid
              AND attribute.attnum > 0
              AND NOT attribute.attisdropped
       )
      INTO catalog_ok
      FROM pg_class relation
      JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
     WHERE namespace.nspname = 'public'
       AND relation.relname =
           'paper_raid_bff_invite_activation_authorities_v3';
    IF catalog_ok IS DISTINCT FROM TRUE THEN
        RAISE EXCEPTION 'Invite Alpha v3 authority table catalog drifted';
    END IF;
    IF EXISTS (
        SELECT 1
          FROM paper_raid_bff_invite_activation_authorities_v3 authority
         WHERE NOT COALESCE(paper_raid_bff_invite_activation_row_valid_v3(
             authority.approval_record, authority.local_approval,
             authority.local_approval_sha256, authority.runtime_acl_evidence,
             authority.runtime_acl_evidence_sha256, authority.activation_id,
             authority.deployment_identity, authority.database_name,
             authority.database_oid, authority.cluster_system_identifier,
             authority.approval_sequence, authority.nonce_sha256,
             authority.issued_at, authority.expires_at, authority.revoked_at,
             authority.revocation_reason, authority.economy_eligibility
         ), FALSE)
    ) THEN
        RAISE EXCEPTION 'Invite Alpha v3 authority data failed parity validation';
    END IF;
    IF EXISTS (
        SELECT local_approval_sha256
          FROM paper_raid_bff_invite_activation_authorities_v3
         GROUP BY local_approval_sha256 HAVING count(*) > 1
    ) OR EXISTS (
        SELECT nonce_sha256
          FROM paper_raid_bff_invite_activation_authorities_v3
         GROUP BY nonce_sha256 HAVING count(*) > 1
    ) OR EXISTS (
        SELECT deployment_identity, cluster_system_identifier, approval_sequence
          FROM paper_raid_bff_invite_activation_authorities_v3
         GROUP BY deployment_identity, cluster_system_identifier, approval_sequence
        HAVING count(*) > 1
    ) OR (
        SELECT count(*)
          FROM paper_raid_bff_invite_activation_authorities_v3
         WHERE revoked_at IS NULL
    ) > 1 THEN
        RAISE EXCEPTION 'Invite Alpha v3 authority uniqueness drifted';
    END IF;
END
$validate_v3_authority_data$;

ALTER TABLE paper_raid_bff_invite_activation_authorities_v3
    DROP CONSTRAINT IF EXISTS
        paper_raid_bff_invite_activation_authorities_v3_pkey,
    DROP CONSTRAINT IF EXISTS
        paper_raid_bff_invite_activation_authorities_v3_local_approval_sha256_key,
    DROP CONSTRAINT IF EXISTS
        paper_raid_bff_invite_activation_authorities_v3_nonce_sha256_key,
    DROP CONSTRAINT IF EXISTS
        "paper_raid_bff_invite_activation_authorities_v3_local_approval_",
    DROP CONSTRAINT IF EXISTS
        "paper_raid_bff_invite_activation_authorities_v3_nonce_sha256_ke",
    DROP CONSTRAINT IF EXISTS paper_raid_bff_invite_v3_local_approval_key,
    DROP CONSTRAINT IF EXISTS paper_raid_bff_invite_v3_nonce_key,
    DROP CONSTRAINT IF EXISTS
        paper_raid_bff_invite_activation_v3_deployment_sequence_key,
    DROP CONSTRAINT IF EXISTS
        paper_raid_bff_invite_activation_v3_parity_ck;

-- A same-named standalone or otherwise drifted index is not repaired by
-- DROP CONSTRAINT. Remove every managed backing/index name before rebuilding
-- the authoritative catalog below.
DROP INDEX IF EXISTS
    paper_raid_bff_invite_activation_authorities_v3_pkey;
DROP INDEX IF EXISTS
    paper_raid_bff_invite_activation_authorities_v3_local_approval_sha256_key;
DROP INDEX IF EXISTS
    paper_raid_bff_invite_activation_authorities_v3_nonce_sha256_key;
DROP INDEX IF EXISTS
    "paper_raid_bff_invite_activation_authorities_v3_local_approval_";
DROP INDEX IF EXISTS
    "paper_raid_bff_invite_activation_authorities_v3_nonce_sha256_ke";
DROP INDEX IF EXISTS paper_raid_bff_invite_v3_local_approval_key;
DROP INDEX IF EXISTS paper_raid_bff_invite_v3_nonce_key;
DROP INDEX IF EXISTS
    paper_raid_bff_invite_activation_v3_deployment_sequence_key;

ALTER TABLE paper_raid_bff_invite_activation_authorities_v3
    ADD CONSTRAINT paper_raid_bff_invite_activation_authorities_v3_pkey
        PRIMARY KEY (activation_id),
    ADD CONSTRAINT paper_raid_bff_invite_v3_local_approval_key
        UNIQUE (local_approval_sha256),
    ADD CONSTRAINT paper_raid_bff_invite_v3_nonce_key
        UNIQUE (nonce_sha256),
    ADD CONSTRAINT paper_raid_bff_invite_activation_v3_deployment_sequence_key
        UNIQUE (deployment_identity, cluster_system_identifier, approval_sequence),
    ADD CONSTRAINT paper_raid_bff_invite_activation_v3_parity_ck CHECK (
        COALESCE(paper_raid_bff_invite_activation_row_valid_v3(
            approval_record, local_approval, local_approval_sha256,
            runtime_acl_evidence, runtime_acl_evidence_sha256, activation_id,
            deployment_identity, database_name, database_oid,
            cluster_system_identifier, approval_sequence, nonce_sha256,
            issued_at, expires_at, revoked_at, revocation_reason,
            economy_eligibility
        ), FALSE)
    ) NOT VALID;

ALTER TABLE paper_raid_bff_invite_activation_authorities_v3
    VALIDATE CONSTRAINT paper_raid_bff_invite_activation_v3_parity_ck;

DROP TRIGGER IF EXISTS paper_raid_bff_invite_activation_monotonic_v3
    ON paper_raid_bff_invite_activation_authorities_v3;
CREATE TRIGGER paper_raid_bff_invite_activation_monotonic_v3
BEFORE INSERT ON paper_raid_bff_invite_activation_authorities_v3
FOR EACH ROW EXECUTE FUNCTION paper_raid_bff_invite_activation_monotonic_v3();

DROP TRIGGER IF EXISTS paper_raid_bff_invite_activation_immutable_v3
    ON paper_raid_bff_invite_activation_authorities_v3;
CREATE TRIGGER paper_raid_bff_invite_activation_immutable_v3
BEFORE UPDATE OR DELETE ON paper_raid_bff_invite_activation_authorities_v3
FOR EACH ROW EXECUTE FUNCTION paper_raid_bff_invite_activation_immutable_v3();

DROP TRIGGER IF EXISTS paper_raid_bff_invite_activation_truncate_v3
    ON paper_raid_bff_invite_activation_authorities_v3;
CREATE TRIGGER paper_raid_bff_invite_activation_truncate_v3
BEFORE TRUNCATE ON paper_raid_bff_invite_activation_authorities_v3
FOR EACH STATEMENT EXECUTE FUNCTION
    paper_raid_bff_invite_activation_truncate_v3();

DROP INDEX IF EXISTS paper_raid_bff_one_active_invite_activation_v3;
CREATE UNIQUE INDEX paper_raid_bff_one_active_invite_activation_v3
    ON paper_raid_bff_invite_activation_authorities_v3 ((TRUE))
    WHERE revoked_at IS NULL;

DROP INDEX IF EXISTS paper_raid_bff_invite_activation_v3_expiry;
CREATE INDEX paper_raid_bff_invite_activation_v3_expiry
    ON paper_raid_bff_invite_activation_authorities_v3 (expires_at)
    WHERE revoked_at IS NULL;

DO $verify_v3_managed_catalog$
DECLARE
    expected_constraints CONSTANT TEXT[] := ARRAY[
        'paper_raid_bff_invite_activation_authorities_v3_pkey',
        'paper_raid_bff_invite_activation_v3_deployment_sequence_key',
        'paper_raid_bff_invite_activation_v3_parity_ck',
        'paper_raid_bff_invite_v3_local_approval_key',
        'paper_raid_bff_invite_v3_nonce_key'
    ];
    expected_indexes CONSTANT TEXT[] := ARRAY[
        'paper_raid_bff_invite_activation_authorities_v3_pkey',
        'paper_raid_bff_invite_activation_v3_deployment_sequence_key',
        'paper_raid_bff_invite_activation_v3_expiry',
        'paper_raid_bff_invite_v3_local_approval_key',
        'paper_raid_bff_invite_v3_nonce_key',
        'paper_raid_bff_one_active_invite_activation_v3'
    ];
    expected_triggers CONSTANT TEXT[] := ARRAY[
        'paper_raid_bff_invite_activation_immutable_v3',
        'paper_raid_bff_invite_activation_monotonic_v3',
        'paper_raid_bff_invite_activation_truncate_v3'
    ];
BEGIN
    IF (
        SELECT array_agg(
                   constraint_row.conname::TEXT ORDER BY constraint_row.conname
               )
          FROM pg_constraint constraint_row
         WHERE constraint_row.conrelid =
            'paper_raid_bff_invite_activation_authorities_v3'::regclass
    ) IS DISTINCT FROM expected_constraints THEN
        RAISE EXCEPTION 'Invite Alpha v3 authority constraint set drifted';
    END IF;
    IF (
        SELECT array_agg(index_class.relname::TEXT ORDER BY index_class.relname)
          FROM pg_index index_row
          JOIN pg_class index_class ON index_class.oid = index_row.indexrelid
         WHERE index_row.indrelid =
            'paper_raid_bff_invite_activation_authorities_v3'::regclass
    ) IS DISTINCT FROM expected_indexes THEN
        RAISE EXCEPTION 'Invite Alpha v3 authority index set drifted';
    END IF;
    IF (
        SELECT array_agg(trigger_row.tgname::TEXT ORDER BY trigger_row.tgname)
          FROM pg_trigger trigger_row
         WHERE trigger_row.tgrelid =
            'paper_raid_bff_invite_activation_authorities_v3'::regclass
           AND NOT trigger_row.tgisinternal
    ) IS DISTINCT FROM expected_triggers THEN
        RAISE EXCEPTION 'Invite Alpha v3 authority trigger set drifted';
    END IF;
    IF EXISTS (
        SELECT 1 FROM pg_policy policy_row
         WHERE policy_row.polrelid =
            'paper_raid_bff_invite_activation_authorities_v3'::regclass
    ) THEN
        RAISE EXCEPTION 'Invite Alpha v3 authority must not retain RLS policies';
    END IF;
END
$verify_v3_managed_catalog$;

INSERT INTO paper_raid_bff_schema_capabilities(capability)
VALUES ('invite_activation_authority_v3')
ON CONFLICT (capability) DO NOTHING;

COMMIT;
