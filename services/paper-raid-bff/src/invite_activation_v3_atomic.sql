-- One PostgreSQL statement is the Invite Alpha activation trust boundary.
-- Exact catalog/function-source facts, the selected authority row, raw byte
-- digests, configured pins, live cluster identity, expiry/revocation state, and
-- the canonical runtime ACL are evaluated under the same statement snapshot.
WITH catalog_exact AS MATERIALIZED (
    WITH authority_table AS (
                SELECT relation.*
                  FROM pg_class relation
                  JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
                 WHERE namespace.nspname = 'public'
                   AND relation.relname =
                       'paper_raid_bff_invite_activation_authorities_v3'
            ), expected_columns AS (
                SELECT * FROM (VALUES
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
            ), expected_functions AS (
                SELECT * FROM (VALUES
                    ('paper_raid_bff_invite_activation_row_valid_v3(jsonb,text,text,text,text,uuid,text,text,oid,text,bigint,text,timestamp with time zone,timestamp with time zone,timestamp with time zone,text,boolean)'::regprocedure,'sql','i','s',FALSE,'boolean'::regtype,FALSE,ARRAY['search_path=pg_catalog, public']::text[],TRUE),
                    ('paper_raid_bff_cluster_identity_v3()'::regprocedure,'sql','s','r',TRUE,'record'::regtype,TRUE,ARRAY['search_path=pg_catalog']::text[],TRUE),
                    ('paper_raid_bff_runtime_acl_state_v2()'::regprocedure,'sql','s','s',FALSE,'text'::regtype,FALSE,ARRAY['search_path=pg_catalog, public']::text[],TRUE),
                    ('paper_raid_bff_invite_activation_monotonic_v3()'::regprocedure,'plpgsql','v','u',FALSE,'trigger'::regtype,FALSE,ARRAY['search_path=pg_catalog, public']::text[],FALSE),
                    ('paper_raid_bff_invite_activation_immutable_v3()'::regprocedure,'plpgsql','v','u',FALSE,'trigger'::regtype,FALSE,ARRAY['search_path=pg_catalog, public']::text[],FALSE),
                    ('paper_raid_bff_invite_activation_truncate_v3()'::regprocedure,'plpgsql','v','u',FALSE,'trigger'::regtype,FALSE,ARRAY['search_path=pg_catalog']::text[],FALSE)
                ) expected(
                    function_oid, language_name, volatility, parallel_safety,
                    security_definer, return_type, returns_set, config,
                    runtime_execute
                )
            )
            SELECT
                (SELECT count(*) = 1 AND bool_and(
                    relkind = 'r' AND relpersistence = 'p'
                    AND pg_get_userbyid(relowner) = 'paper_raid_bff'
                    AND NOT relrowsecurity AND NOT relforcerowsecurity
                    AND COALESCE(reloptions, ARRAY[]::text[]) = ARRAY[]::text[]
                ) FROM authority_table)
                AND NOT EXISTS (
                    SELECT 1 FROM pg_policy policy_row
                    JOIN authority_table relation
                      ON relation.oid = policy_row.polrelid
                )
                AND (SELECT count(*) = 18 FROM expected_columns)
                AND (SELECT count(*) = 18 FROM pg_attribute attribute
                     JOIN authority_table relation ON relation.oid = attribute.attrelid
                     WHERE attribute.attnum > 0 AND NOT attribute.attisdropped)
                AND (SELECT count(*) = 18 AND bool_and(
                    attribute.attname = expected.attname
                    AND attribute.atttypid = expected.atttypid
                    AND attribute.attnotnull = expected.attnotnull
                    AND attribute.atthasdef = expected.atthasdef
                    AND COALESCE(
                        pg_get_expr(default_value.adbin, default_value.adrelid), ''
                    ) = expected.default_expression
                    AND attribute.attgenerated = '' AND attribute.attidentity = ''
                ) FROM expected_columns expected
                JOIN authority_table relation ON TRUE
                LEFT JOIN pg_attribute attribute
                  ON attribute.attrelid = relation.oid
                 AND attribute.attnum = expected.attnum
                 AND NOT attribute.attisdropped
                LEFT JOIN pg_attrdef default_value
                  ON default_value.adrelid = attribute.attrelid
                 AND default_value.adnum = attribute.attnum)
                AND (SELECT count(*) = 5 AND bool_and(
                    constraint_row.convalidated
                    AND constraint_row.conislocal
                    AND constraint_row.coninhcount = 0
                    AND constraint_row.connoinherit =
                        (constraint_row.contype IN ('p','u'))
                    AND CASE constraint_row.conname
                      WHEN 'paper_raid_bff_invite_activation_authorities_v3_pkey' THEN
                        constraint_row.contype = 'p'
                        AND regexp_replace(pg_get_constraintdef(
                            constraint_row.oid, TRUE
                        ), '[[:space:]]+', '', 'g') = 'PRIMARYKEY(activation_id)'
                      WHEN 'paper_raid_bff_invite_v3_local_approval_key' THEN
                        constraint_row.contype = 'u'
                        AND regexp_replace(pg_get_constraintdef(
                            constraint_row.oid, TRUE
                        ), '[[:space:]]+', '', 'g') = 'UNIQUE(local_approval_sha256)'
                      WHEN 'paper_raid_bff_invite_v3_nonce_key' THEN
                        constraint_row.contype = 'u'
                        AND regexp_replace(pg_get_constraintdef(
                            constraint_row.oid, TRUE
                        ), '[[:space:]]+', '', 'g') = 'UNIQUE(nonce_sha256)'
                      WHEN 'paper_raid_bff_invite_activation_v3_deployment_sequence_key' THEN
                        constraint_row.contype = 'u'
                        AND regexp_replace(pg_get_constraintdef(
                            constraint_row.oid, TRUE
                        ), '[[:space:]]+', '', 'g') =
                            'UNIQUE(deployment_identity,cluster_system_identifier,approval_sequence)'
                      WHEN 'paper_raid_bff_invite_activation_v3_parity_ck' THEN
                        constraint_row.contype = 'c'
                        AND constraint_row.conindid = 0
                        AND regexp_replace(pg_get_constraintdef(
                            constraint_row.oid, TRUE
                        ), '[[:space:]]+', '', 'g') =
                            'CHECK(COALESCE(paper_raid_bff_invite_activation_row_valid_v3(approval_record,local_approval,local_approval_sha256,runtime_acl_evidence,runtime_acl_evidence_sha256,activation_id,deployment_identity,database_name,database_oid,cluster_system_identifier,approval_sequence,nonce_sha256,issued_at,expires_at,revoked_at,revocation_reason,economy_eligibility),false))'
                      ELSE FALSE
                    END
                ) FROM pg_constraint constraint_row
                JOIN authority_table relation
                  ON relation.oid = constraint_row.conrelid)
                AND (SELECT count(*) = 6 AND bool_and(
                    index_row.indisvalid AND index_row.indisready
                    AND index_row.indislive AND index_row.indimmediate
                    AND NOT index_row.indisexclusion
                    AND NOT index_row.indisclustered
                    AND index_class.relkind = 'i'
                    AND pg_get_userbyid(index_class.relowner) = 'paper_raid_bff'
                    AND CASE index_class.relname
                      WHEN 'paper_raid_bff_invite_activation_authorities_v3_pkey' THEN
                        index_row.indisunique AND index_row.indisprimary
                        AND index_row.indnkeyatts = 1 AND index_row.indnatts = 1
                        AND index_row.indkey::text = '1'
                        AND index_row.indoption::text = '0'
                        AND pg_get_indexdef(index_row.indexrelid, 1, TRUE) = 'activation_id'
                        AND index_row.indexprs IS NULL AND index_row.indpred IS NULL
                      WHEN 'paper_raid_bff_invite_v3_local_approval_key' THEN
                        index_row.indisunique AND NOT index_row.indisprimary
                        AND index_row.indnkeyatts = 1 AND index_row.indnatts = 1
                        AND index_row.indkey::text = '3'
                        AND index_row.indoption::text = '0'
                        AND pg_get_indexdef(index_row.indexrelid, 1, TRUE) = 'local_approval_sha256'
                        AND index_row.indexprs IS NULL AND index_row.indpred IS NULL
                      WHEN 'paper_raid_bff_invite_v3_nonce_key' THEN
                        index_row.indisunique AND NOT index_row.indisprimary
                        AND index_row.indnkeyatts = 1 AND index_row.indnatts = 1
                        AND index_row.indkey::text = '11'
                        AND index_row.indoption::text = '0'
                        AND pg_get_indexdef(index_row.indexrelid, 1, TRUE) = 'nonce_sha256'
                        AND index_row.indexprs IS NULL AND index_row.indpred IS NULL
                      WHEN 'paper_raid_bff_invite_activation_v3_deployment_sequence_key' THEN
                        index_row.indisunique AND NOT index_row.indisprimary
                        AND index_row.indnkeyatts = 3 AND index_row.indnatts = 3
                        AND index_row.indkey::text = '6 9 10'
                        AND index_row.indoption::text = '0 0 0'
                        AND pg_get_indexdef(index_row.indexrelid, 1, TRUE) = 'deployment_identity'
                        AND pg_get_indexdef(index_row.indexrelid, 2, TRUE) = 'cluster_system_identifier'
                        AND pg_get_indexdef(index_row.indexrelid, 3, TRUE) = 'approval_sequence'
                        AND index_row.indexprs IS NULL AND index_row.indpred IS NULL
                      WHEN 'paper_raid_bff_one_active_invite_activation_v3' THEN
                        index_row.indisunique AND NOT index_row.indisprimary
                        AND index_row.indnkeyatts = 1 AND index_row.indnatts = 1
                        AND index_row.indkey::text = '0'
                        AND index_row.indoption::text = '0'
                        AND regexp_replace(pg_get_expr(
                            index_row.indexprs, index_row.indrelid
                        ), '[[:space:]]+', '', 'g') IN ('true','(true)')
                        AND regexp_replace(pg_get_expr(
                            index_row.indpred, index_row.indrelid
                        ), '[[:space:]]+', '', 'g') = '(revoked_atISNULL)'
                      WHEN 'paper_raid_bff_invite_activation_v3_expiry' THEN
                        NOT index_row.indisunique AND NOT index_row.indisprimary
                        AND index_row.indnkeyatts = 1 AND index_row.indnatts = 1
                        AND index_row.indkey::text = '13'
                        AND index_row.indoption::text = '0'
                        AND pg_get_indexdef(index_row.indexrelid, 1, TRUE) = 'expires_at'
                        AND index_row.indexprs IS NULL
                        AND regexp_replace(pg_get_expr(
                            index_row.indpred, index_row.indrelid
                        ), '[[:space:]]+', '', 'g') = '(revoked_atISNULL)'
                      ELSE FALSE
                    END
                ) FROM pg_index index_row
                JOIN authority_table relation ON relation.oid = index_row.indrelid
                JOIN pg_class index_class ON index_class.oid = index_row.indexrelid)
                AND (SELECT count(*) = 3 AND bool_and(
                    NOT trigger_row.tgisinternal AND trigger_row.tgenabled = 'O'
                    AND trigger_row.tgconstraint = 0
                    AND NOT trigger_row.tgdeferrable
                    AND NOT trigger_row.tginitdeferred
                    AND trigger_row.tgnargs = 0
                    AND trigger_row.tgqual IS NULL
                    AND trigger_row.tgoldtable IS NULL
                    AND trigger_row.tgnewtable IS NULL
                    AND CASE trigger_row.tgname
                      WHEN 'paper_raid_bff_invite_activation_monotonic_v3' THEN
                        trigger_row.tgfoid =
                            'paper_raid_bff_invite_activation_monotonic_v3()'::regprocedure
                        AND trigger_row.tgtype = 7
                      WHEN 'paper_raid_bff_invite_activation_immutable_v3' THEN
                        trigger_row.tgfoid =
                            'paper_raid_bff_invite_activation_immutable_v3()'::regprocedure
                        AND trigger_row.tgtype = 27
                      WHEN 'paper_raid_bff_invite_activation_truncate_v3' THEN
                        trigger_row.tgfoid =
                            'paper_raid_bff_invite_activation_truncate_v3()'::regprocedure
                        AND trigger_row.tgtype = 34
                      ELSE FALSE
                    END
                ) FROM pg_trigger trigger_row
                JOIN authority_table relation ON relation.oid = trigger_row.tgrelid
                WHERE NOT trigger_row.tgisinternal)
                AND (SELECT count(*) = 6 FROM pg_proc procedure
                     JOIN pg_namespace namespace ON namespace.oid = procedure.pronamespace
                     WHERE namespace.nspname = 'public'
                       AND procedure.proname IN (
                           'paper_raid_bff_invite_activation_row_valid_v3',
                           'paper_raid_bff_cluster_identity_v3',
                           'paper_raid_bff_runtime_acl_state_v2',
                           'paper_raid_bff_invite_activation_monotonic_v3',
                           'paper_raid_bff_invite_activation_immutable_v3',
                           'paper_raid_bff_invite_activation_truncate_v3'
                       ))
                AND (SELECT count(*) = 6 AND bool_and(
                    language_row.lanname = expected.language_name
                    AND procedure.prokind = 'f'
                    AND procedure.provolatile = expected.volatility
                    AND procedure.proparallel = expected.parallel_safety
                    AND procedure.prosecdef = expected.security_definer
                    AND NOT procedure.proisstrict AND NOT procedure.proleakproof
                    AND procedure.prorettype = expected.return_type
                    AND procedure.proretset = expected.returns_set
                    AND procedure.proconfig = expected.config
                    AND pg_get_userbyid(procedure.proowner) = 'paper_raid_bff'
                    AND (
                        SELECT count(*) = CASE WHEN expected.runtime_execute THEN 2 ELSE 1 END
                           AND bool_and(
                               acl.privilege_type = 'EXECUTE'
                               AND (
                                   (acl.grantee = procedure.proowner
                                       AND NOT acl.is_grantable)
                                   OR (
                                       expected.runtime_execute
                                       AND grantee.rolname = 'paper_raid_bff_runtime'
                                       AND NOT acl.is_grantable
                                   )
                               )
                           )
                          FROM aclexplode(COALESCE(
                              procedure.proacl,
                              acldefault('f', procedure.proowner)
                          )) acl
                          LEFT JOIN pg_roles grantee ON grantee.oid = acl.grantee
                    )
                ) FROM expected_functions expected
                JOIN pg_proc procedure ON procedure.oid = expected.function_oid
                JOIN pg_language language_row ON language_row.oid = procedure.prolang)
                AND EXISTS (
                    SELECT 1
                      FROM pg_proc procedure
                      JOIN pg_namespace namespace ON namespace.oid = procedure.pronamespace
                      JOIN pg_roles owner_role ON owner_role.oid = procedure.proowner
                     WHERE procedure.oid = 'pg_catalog.pg_control_system()'::regprocedure
                       AND namespace.nspname = 'pg_catalog'
                       AND owner_role.rolsuper
                       AND (
                           SELECT count(*) = CASE
                                      WHEN owner_role.rolname = 'paper_raid_bff'
                                      THEN 1 ELSE 2
                                  END
                              AND bool_and(
                               acl.privilege_type = 'EXECUTE'
                               AND (
                                   (acl.grantee = procedure.proowner
                                       AND NOT acl.is_grantable)
                                   OR (
                                       grantee.rolname = 'paper_raid_bff'
                                       AND NOT acl.is_grantable
                                   )
                               )
                           )
                             FROM aclexplode(COALESCE(
                                 procedure.proacl,
                                 acldefault('f', procedure.proowner)
                             )) acl
                             LEFT JOIN pg_roles grantee ON grantee.oid = acl.grantee
                       )
                       AND NOT has_function_privilege(
                           'paper_raid_bff_runtime', procedure.oid, 'EXECUTE'
                       )
                )
                AND EXISTS (
                    SELECT 1
                      FROM pg_class relation
                      JOIN pg_namespace namespace
                        ON namespace.oid = relation.relnamespace
                     WHERE namespace.nspname = 'public'
                       AND relation.relname =
                           'paper_raid_bff_runtime_acl_state_v2'
                       AND relation.relkind = 'v'
                       AND relation.relpersistence = 'p'
                       AND NOT relation.relrowsecurity
                       AND NOT relation.relforcerowsecurity
                       AND relation.reloptions =
                           ARRAY['security_barrier=true']::text[]
                       AND pg_get_userbyid(relation.relowner) = 'paper_raid_bff'
                       AND regexp_replace(
                           rtrim(
                               pg_get_viewdef(relation.oid, true), chr(59)
                           ),
                           '[[:space:]]+', '', 'g'
                       ) =
                           'SELECTpaper_raid_bff_runtime_acl_state_v2()AScanonical_state'
                       AND (
                           SELECT count(*) = 1 AND bool_and(
                               attribute.attnum = 1
                               AND attribute.attname = 'canonical_state'
                               AND attribute.atttypid = 'text'::regtype
                               AND NOT attribute.attnotnull
                               AND NOT attribute.atthasdef
                               AND attribute.attgenerated = ''
                               AND attribute.attidentity = ''
                           )
                             FROM pg_attribute attribute
                            WHERE attribute.attrelid = relation.oid
                              AND attribute.attnum > 0
                              AND NOT attribute.attisdropped
                       )
                )
                AND EXISTS (SELECT 1 FROM paper_raid_bff_schema_capabilities
                            WHERE capability = 'invite_activation_authority_v3')
), installed_sources AS MATERIALIZED (
    SELECT COALESCE(
        (SELECT prosrc FROM pg_proc WHERE oid =
            'paper_raid_bff_invite_activation_row_valid_v3(jsonb,text,text,text,text,uuid,text,text,oid,text,bigint,text,timestamp with time zone,timestamp with time zone,timestamp with time zone,text,boolean)'::regprocedure) = $30
        AND (SELECT prosrc FROM pg_proc WHERE oid =
            'paper_raid_bff_cluster_identity_v3()'::regprocedure) = $31
        AND (SELECT prosrc FROM pg_proc WHERE oid =
            'paper_raid_bff_invite_activation_monotonic_v3()'::regprocedure) = $32
        AND (SELECT prosrc FROM pg_proc WHERE oid =
            'paper_raid_bff_invite_activation_immutable_v3()'::regprocedure) = $33
        AND (SELECT prosrc FROM pg_proc WHERE oid =
            'paper_raid_bff_invite_activation_truncate_v3()'::regprocedure) = $34
        AND (SELECT prosrc FROM pg_proc WHERE oid =
            'paper_raid_bff_runtime_acl_state_v2()'::regprocedure) = $35,
        FALSE
    ) AS exact
), selected_authority AS MATERIALIZED (
    SELECT *
      FROM paper_raid_bff_invite_activation_authorities_v3
     WHERE activation_id = $1
     FOR SHARE
), live_identity AS MATERIALIZED (
    SELECT (paper_raid_bff_cluster_identity_v3()).*
), live_acl AS MATERIALIZED (
    SELECT canonical_state,
           'sha256:' || encode(
               sha256(convert_to(canonical_state, 'UTF8')), 'hex'
           ) AS runtime_acl_state_sha256
      FROM paper_raid_bff_runtime_acl_state_v2
)
SELECT CASE
    WHEN COALESCE((SELECT * FROM catalog_exact), FALSE)
     AND COALESCE((SELECT exact FROM installed_sources), FALSE)
    THEN EXISTS (
        SELECT 1
          FROM selected_authority authority
          JOIN live_identity identity ON TRUE
          JOIN live_acl acl
            ON acl.runtime_acl_state_sha256 = $8
         WHERE COALESCE(paper_raid_bff_invite_activation_row_valid_v3(
                   authority.approval_record,
                   authority.local_approval,
                   authority.local_approval_sha256,
                   authority.runtime_acl_evidence,
                   authority.runtime_acl_evidence_sha256,
                   authority.activation_id,
                   authority.deployment_identity,
                   authority.database_name,
                   authority.database_oid,
                   authority.cluster_system_identifier,
                   authority.approval_sequence,
                   authority.nonce_sha256,
                   authority.issued_at,
                   authority.expires_at,
                   authority.revoked_at,
                   authority.revocation_reason,
                   authority.economy_eligibility
               ), FALSE)
           AND authority.local_approval_sha256 = 'sha256:' || encode(
                   sha256(convert_to(authority.local_approval, 'UTF8')), 'hex'
               )
           AND authority.runtime_acl_evidence_sha256 = 'sha256:' || encode(
                   sha256(convert_to(
                       authority.runtime_acl_evidence, 'UTF8'
                   )), 'hex'
               )
           AND authority.local_approval_sha256 = $2
           AND authority.approval_sequence = $3
           AND authority.nonce_sha256 = $4
           AND authority.approval_record ->> 'profile_sha256' = $5
           AND authority.approval_record ->> 'base_compose_sha256' = $6
           AND authority.approval_record ->> 'runtime_acl_sha256' = $7
           AND authority.runtime_acl_evidence::jsonb ->>
                'runtime_acl_state_sha256' = $8
           AND authority.approval_record #>> '{retention,policy_id}' = $9
           AND authority.approval_record #>> '{retention,policy_sha256}' = $10
           AND authority.approval_record #>> '{evidence,image_lock_sha256}' = $11
           AND authority.approval_record #>> '{evidence,release_provenance_sha256}' = $12
           AND authority.runtime_acl_evidence_sha256 = $13
           AND authority.database_name = $14
           AND authority.database_oid::bigint = $15
           AND authority.cluster_system_identifier = $16
           AND authority.deployment_identity = $17
           AND authority.approval_record ->> 'release_id' = $18
           AND authority.approval_record #>> '{images,postgres}' = $19
           AND authority.approval_record #>> '{images,object_store}' = $20
           AND authority.approval_record #>> '{images,object_store_client}' = $21
           AND authority.approval_record #>> '{images,ops}' = $22
           AND authority.approval_record #>> '{images,nakama}' = $23
           AND authority.approval_record #>> '{images,hepta}' = $24
           AND authority.approval_record #>> '{images,bff}' = $25
           AND authority.approval_record #>> '{images,accessctl}' = $26
           AND authority.approval_record #>> '{hepta,revision}' = $27
           AND authority.approval_record #>> '{hepta,source_tree}' = $28
           AND authority.approval_record #>> '{hepta,fileset_sha256}' = $29
           AND authority.approval_record #>>
                '{evidence,runtime_acl_verification_sha256}' = $13
           AND authority.runtime_acl_evidence::jsonb ->>
                'runtime_acl_sha256' = $7
           AND authority.runtime_acl_evidence::jsonb #>>
                '{database,name}' = $14
           AND (authority.runtime_acl_evidence::jsonb #>>
                '{database,oid}')::bigint = $15
           AND authority.runtime_acl_evidence::jsonb #>>
                '{database,system_identifier}' = $16
           AND identity.database_name = authority.database_name
           AND identity.database_oid = authority.database_oid
           AND identity.cluster_system_identifier =
                authority.cluster_system_identifier
           AND authority.revoked_at IS NULL
           AND authority.revocation_reason IS NULL
           AND authority.issued_at <= statement_timestamp()
           AND authority.expires_at > statement_timestamp()
           AND authority.expires_at - authority.issued_at BETWEEN
                interval '60 seconds' AND interval '86400 seconds'
           AND authority.economy_eligibility = FALSE
    )
    ELSE FALSE
END;
