use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;
use uuid::Uuid;

use crate::config::InviteAlphaConfig;

pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .min_connections(1)
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(3))
        .connect(database_url)
        .await
}

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(include_str!("../migrations/0001_bff_state.sql"))
        .execute(pool)
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0002_product_telemetry.sql"))
        .execute(pool)
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0003_invite_alpha_access.sql"))
        .execute(pool)
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0004_agent_pairing_bridge.sql"))
        .execute(pool)
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0005_agent_delivery_drafts.sql"))
        .execute(pool)
        .await?;
    sqlx::raw_sql(include_str!(
        "../migrations/0006_accessctl_operator_audit.sql"
    ))
    .execute(pool)
    .await?;
    sqlx::raw_sql(include_str!(
        "../migrations/0007_invite_activation_authority.sql"
    ))
    .execute(pool)
    .await?;
    sqlx::raw_sql(include_str!(
        "../migrations/0008_review_execution_receipts.sql"
    ))
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct ProductEvent<'a> {
    pub event_id: Uuid,
    pub session_id: Option<Uuid>,
    pub player_id: Uuid,
    pub event_name: &'a str,
    pub challenge_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
    pub paper_id: Option<Uuid>,
    pub phase: Option<&'a str>,
    pub source: &'a str,
}

pub async fn record_product_event(
    pool: &PgPool,
    event: ProductEvent<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO paper_raid_bff_product_events (
            event_id, session_id, player_id, event_name, challenge_id,
            team_id, paper_id, phase, source
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(event.event_id)
    .bind(event.session_id)
    .bind(event.player_id)
    .bind(event.event_name)
    .bind(event.challenge_id)
    .bind(event.team_id)
    .bind(event.paper_id)
    .bind(event.phase)
    .bind(event.source)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn ready(pool: &PgPool) -> bool {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .is_ok()
}

pub async fn invite_schema_ready(pool: &PgPool) -> bool {
    let catalog_ready = sqlx::query_scalar::<_, bool>(
        "SELECT \
            to_regclass('paper_raid_bff_sessions') IS NOT NULL \
            AND to_regclass('paper_raid_bff_accounts') IS NOT NULL \
            AND to_regclass('paper_raid_bff_invites') IS NOT NULL \
            AND to_regclass('paper_raid_bff_login_credentials') IS NOT NULL \
            AND to_regclass('paper_raid_bff_access_audit') IS NOT NULL \
            AND to_regclass('paper_raid_bff_quota_windows') IS NOT NULL \
            AND to_regclass('paper_raid_bff_retention_runs') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_pairing_grants') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_bridge_bindings') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_request_uses') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_delivery_drafts') IS NOT NULL \
            AND to_regclass('paper_raid_bff_review_execution_receipts') IS NOT NULL \
            AND to_regclass('paper_raid_bff_one_pending_agent_delivery_tuple') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_delivery_drafts_inbox_idx') IS NOT NULL \
            AND to_regclass('paper_raid_bff_object_audit_attempt_idx') IS NOT NULL \
            AND EXISTS ( \
                SELECT 1 FROM paper_raid_bff_schema_capabilities \
                WHERE capability = 'invite_alpha_access_v1' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM paper_raid_bff_schema_capabilities \
                WHERE capability = 'agent_bridge_pairing_v1' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM paper_raid_bff_schema_capabilities \
                WHERE capability = 'agent_bridge_delivery_drafts_v1' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM paper_raid_bff_schema_capabilities \
                WHERE capability = 'accessctl_operator_audit_v1' \
            ) \
            AND ( \
                SELECT count(*) = 3 FROM pg_attribute a \
                JOIN (VALUES \
                    ('operator_attempt_id'::text, 'uuid'::regtype), \
                    ('operator_event'::text, 'text'::regtype), \
                    ('operator_lineage_status'::text, 'text'::regtype) \
                ) AS expected(attname, atttypid) \
                  ON expected.attname = a.attname AND expected.atttypid = a.atttypid \
                WHERE a.attrelid = 'paper_raid_bff_access_audit'::regclass \
                  AND a.attnum > 0 AND NOT a.attisdropped \
                  AND (a.attname = 'operator_lineage_status') = a.attnotnull \
                  AND a.attgenerated = '' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_proc p \
                JOIN pg_language l ON l.oid = p.prolang \
                WHERE p.oid = to_regprocedure( \
                    'paper_raid_bff_operator_audit_row_valid(text,text,jsonb,text,uuid,uuid,text,text)' \
                ) \
                  AND p.prorettype = 'boolean'::regtype \
                  AND p.pronargs = 8 \
                  AND p.proargtypes::text = '25 25 3802 25 2950 2950 25 25' \
                  AND p.provolatile = 'i' AND p.proparallel = 's' \
                  AND NOT p.prosecdef AND NOT p.proleakproof \
                  AND l.lanname = 'sql' \
            ) \
            AND ( \
                SELECT count(*) = 2 AND bool_and( \
                    CASE conname \
                        WHEN 'paper_raid_bff_access_audit_outcome_check' THEN \
                            regexp_replace( \
                                pg_get_constraintdef(oid), '[[:space:]]+', '', 'g' \
                            ) = 'CHECK((outcome=ANY(ARRAY[''succeeded''::text,''denied''::text,''indeterminate''::text])))' \
                        WHEN 'paper_raid_bff_operator_audit_shape_ck' THEN \
                            regexp_replace( \
                                pg_get_constraintdef(oid), '[[:space:]]+', '', 'g' \
                            ) = 'CHECK(paper_raid_bff_operator_audit_row_valid(action,outcome,metadata,operator_subject,account_id,operator_attempt_id,operator_event,operator_lineage_status))' \
                        ELSE FALSE \
                    END \
                ) FROM pg_constraint \
                WHERE conrelid = 'paper_raid_bff_access_audit'::regclass \
                  AND contype = 'c' AND convalidated \
                  AND conname IN ( \
                    'paper_raid_bff_access_audit_outcome_check', \
                    'paper_raid_bff_operator_audit_shape_ck' \
                  ) \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_index i \
                JOIN pg_class idx ON idx.oid = i.indexrelid \
                WHERE i.indrelid = 'paper_raid_bff_access_audit'::regclass \
                  AND idx.relname = 'paper_raid_bff_one_operator_event_per_attempt' \
                  AND i.indisunique AND i.indisvalid AND i.indisready AND i.indislive \
                  AND i.indnkeyatts = 2 AND i.indnatts = 2 \
                  AND pg_get_indexdef(i.indexrelid, 1, true) = 'operator_attempt_id' \
                  AND pg_get_indexdef(i.indexrelid, 2, true) = 'operator_event' \
                  AND regexp_replace( \
                    pg_get_expr(i.indpred, i.indrelid), '[[:space:]]+', '', 'g' \
                  ) = '(operator_attempt_idISNOTNULL)' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_trigger t \
                WHERE t.tgname = 'paper_raid_bff_access_audit_append_only' \
                  AND t.tgrelid = 'paper_raid_bff_access_audit'::regclass \
                  AND NOT t.tgisinternal AND t.tgenabled = 'O' AND t.tgtype = 27 \
                  AND t.tgfoid = 'paper_raid_bff_reject_access_audit_mutation()'::regprocedure \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_index i \
                JOIN pg_class idx ON idx.oid = i.indexrelid \
                WHERE i.indrelid = 'paper_raid_bff_access_audit'::regclass \
                  AND idx.relname = 'paper_raid_bff_object_audit_attempt_idx' \
                  AND NOT i.indisunique AND i.indisvalid AND i.indisready AND i.indislive \
                  AND i.indnkeyatts = 2 AND i.indnatts = 2 \
                  AND pg_get_indexdef(i.indexrelid, 1, true) = 'operator_attempt_id' \
                  AND pg_get_indexdef(i.indexrelid, 2, true) = 'audit_id' \
                  AND regexp_replace( \
                    pg_get_expr(i.indpred, i.indrelid), '[[:space:]]+', '', 'g' \
                  ) = '((operator_attempt_idISNOTNULL)AND(operator_eventISNULL))' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_trigger t \
                WHERE t.tgname = 'paper_raid_bff_object_audit_attempt_parent' \
                  AND t.tgrelid = 'paper_raid_bff_access_audit'::regclass \
                  AND NOT t.tgisinternal AND t.tgenabled = 'O' AND t.tgtype = 7 \
                  AND t.tgfoid = 'paper_raid_bff_require_object_audit_attempt()'::regprocedure \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_proc p \
                JOIN pg_language l ON l.oid = p.prolang \
                WHERE p.oid = 'paper_raid_bff_reject_access_audit_mutation()'::regprocedure \
                  AND p.prorettype = 'trigger'::regtype AND p.pronargs = 0 \
                  AND p.provolatile = 'v' AND NOT p.prosecdef \
                  AND l.lanname = 'plpgsql' \
                  AND regexp_replace(p.prosrc, '[[:space:]]+', '', 'g') = \
                    'BEGINRAISEEXCEPTION''paper_raid_bff_access_auditisappend-only'';END;' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_proc p \
                JOIN pg_language l ON l.oid = p.prolang \
                WHERE p.oid = 'paper_raid_bff_require_object_audit_attempt()'::regprocedure \
                  AND p.prorettype = 'trigger'::regtype AND p.pronargs = 0 \
                  AND p.provolatile = 'v' AND NOT p.prosecdef \
                  AND l.lanname = 'plpgsql' \
            )",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if !catalog_ready {
        return false;
    }
    let installed_validator_source = sqlx::query_scalar::<_, String>(
        "SELECT p.prosrc FROM pg_proc p \
         WHERE p.oid = to_regprocedure( \
            'paper_raid_bff_operator_audit_row_valid(text,text,jsonb,text,uuid,uuid,text,text)' \
         )",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let validator_exact = installed_validator_source
        .zip(expected_operator_audit_validator_source())
        .is_some_and(|(installed, expected)| normalize_sql(&installed) == normalize_sql(expected));
    if !validator_exact {
        return false;
    }
    let installed_parent_source = sqlx::query_scalar::<_, String>(
        "SELECT p.prosrc FROM pg_proc p \
         WHERE p.oid = 'paper_raid_bff_require_object_audit_attempt()'::regprocedure",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let operator_lineage_exact = installed_parent_source
        .zip(expected_object_audit_parent_source())
        .is_some_and(|(installed, expected)| normalize_sql(&installed) == normalize_sql(expected));
    operator_lineage_exact && invite_activation_schema_ready(pool).await
}

fn expected_operator_audit_validator_source() -> Option<&'static str> {
    include_str!("../migrations/0006_accessctl_operator_audit.sql")
        .split_once("AS $function$")?
        .1
        .split_once("$function$;")
        .map(|(source, _)| source)
}

fn expected_object_audit_parent_source() -> Option<&'static str> {
    include_str!("../migrations/0006_accessctl_operator_audit.sql")
        .split_once("CREATE OR REPLACE FUNCTION paper_raid_bff_require_object_audit_attempt()")?
        .1
        .split_once("AS $function$")?
        .1
        .split_once("$function$;")
        .map(|(source, _)| source)
}

fn normalize_sql(value: &str) -> String {
    value.split_whitespace().collect()
}

pub async fn invite_activation_schema_ready(pool: &PgPool) -> bool {
    let catalog_exact = sqlx::query_scalar::<_, bool>(
        "SELECT \
            to_regclass('paper_raid_bff_invite_activations') IS NOT NULL \
            AND to_regclass('paper_raid_bff_runtime_acl_state_v2') IS NOT NULL \
            AND to_regclass('paper_raid_bff_one_active_invite_activation') IS NOT NULL \
            AND to_regclass('paper_raid_bff_invite_activation_expiry') IS NOT NULL \
            AND EXISTS ( \
                SELECT 1 FROM paper_raid_bff_schema_capabilities \
                WHERE capability = 'invite_activation_authority_v2' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_class c \
                JOIN pg_namespace n ON n.oid = c.relnamespace \
                WHERE n.nspname = 'public' \
                  AND c.relname = 'paper_raid_bff_invite_activations' \
                  AND c.relkind = 'r' AND c.relpersistence = 'p' \
                  AND NOT c.relrowsecurity AND NOT c.relforcerowsecurity \
                  AND c.reloptions IS NULL \
                  AND pg_get_userbyid(c.relowner) = 'paper_raid_bff' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_proc p JOIN pg_language l ON l.oid = p.prolang \
                WHERE p.oid = to_regprocedure( \
                    'paper_raid_bff_invite_activation_row_valid_v2(jsonb,text,text,uuid,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,timestamp with time zone,timestamp with time zone,timestamp with time zone,text,boolean)' \
                ) \
                  AND p.prorettype = 'boolean'::regtype \
                  AND p.pronargs = 25 AND p.provolatile = 'i' \
                  AND p.proparallel = 's' AND NOT p.prosecdef \
                  AND NOT p.proisstrict AND NOT p.proleakproof \
                  AND l.lanname = 'sql' \
                  AND p.proconfig = ARRAY['search_path=pg_catalog, public']::text[] \
                  AND pg_get_userbyid(p.proowner) = 'paper_raid_bff' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_proc p JOIN pg_language l ON l.oid = p.prolang \
                WHERE p.oid = to_regprocedure( \
                    'paper_raid_bff_runtime_acl_state_v2()' \
                ) \
                  AND p.prorettype = 'text'::regtype \
                  AND p.pronargs = 0 AND p.provolatile = 's' \
                  AND p.proparallel = 's' AND NOT p.prosecdef \
                  AND NOT p.proleakproof AND l.lanname = 'sql' \
                  AND p.proconfig = ARRAY['search_path=pg_catalog, public']::text[] \
                  AND pg_get_userbyid(p.proowner) = 'paper_raid_bff' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_class c \
                JOIN pg_namespace n ON n.oid = c.relnamespace \
                WHERE n.nspname = 'public' \
                  AND c.relname = 'paper_raid_bff_runtime_acl_state_v2' \
                  AND c.relkind = 'v' \
                  AND c.reloptions = ARRAY['security_barrier=true']::text[] \
                  AND pg_get_userbyid(c.relowner) = 'paper_raid_bff' \
                  AND regexp_replace( \
                    pg_get_viewdef(c.oid, true), '[[:space:]]+', '', 'g' \
                  ) = 'SELECTpaper_raid_bff_runtime_acl_state_v2()AScanonical_state;' \
                  AND ( \
                    SELECT count(*) = 1 AND bool_and( \
                        a.attnum = 1 AND a.attname = 'canonical_state' \
                        AND a.atttypid = 'text'::regtype \
                        AND NOT a.attnotnull AND NOT a.atthasdef \
                        AND a.attgenerated = '' AND a.attidentity = '' \
                    ) FROM pg_attribute a \
                    WHERE a.attrelid = c.oid \
                      AND a.attnum > 0 AND NOT a.attisdropped \
                  ) \
            ) \
            AND ( \
                SELECT count(a.attnum) = 26 AND bool_and( \
                    a.attname = expected.attname \
                    AND a.atttypid = expected.atttypid \
                    AND a.attnotnull = expected.attnotnull \
                    AND a.atthasdef = expected.atthasdef \
                    AND COALESCE(pg_get_expr(d.adbin, d.adrelid), '') = expected.default_expr \
                    AND a.attgenerated = '' AND a.attidentity = '' \
                ) \
                FROM (VALUES \
                    (1,'activation_id','uuid'::regtype,TRUE,FALSE,''), \
                    (2,'approval_receipt_v1_sha256','text'::regtype,TRUE,FALSE,''), \
                    (3,'profile_sha256','text'::regtype,TRUE,FALSE,''), \
                    (4,'base_compose_sha256','text'::regtype,TRUE,FALSE,''), \
                    (5,'runtime_acl_sha256','text'::regtype,TRUE,FALSE,''), \
                    (6,'runtime_acl_state_sha256','text'::regtype,TRUE,FALSE,''), \
                    (7,'bff_image','text'::regtype,TRUE,FALSE,''), \
                    (8,'accessctl_image','text'::regtype,TRUE,FALSE,''), \
                    (9,'postgres_image','text'::regtype,TRUE,FALSE,''), \
                    (10,'ops_image','text'::regtype,TRUE,FALSE,''), \
                    (11,'hepta_image','text'::regtype,TRUE,FALSE,''), \
                    (12,'retention_policy_id','text'::regtype,TRUE,FALSE,''), \
                    (13,'retention_policy_sha256','text'::regtype,TRUE,FALSE,''), \
                    (14,'image_lock_sha256','text'::regtype,TRUE,FALSE,''), \
                    (15,'release_provenance_sha256','text'::regtype,TRUE,FALSE,''), \
                    (16,'runtime_acl_evidence_sha256','text'::regtype,TRUE,FALSE,''), \
                    (17,'database_identity','text'::regtype,TRUE,FALSE,''), \
                    (18,'deployment_identity','text'::regtype,TRUE,FALSE,''), \
                    (19,'issued_at','timestamp with time zone'::regtype,TRUE,FALSE,''), \
                    (20,'expires_at','timestamp with time zone'::regtype,TRUE,FALSE,''), \
                    (21,'revoked_at','timestamp with time zone'::regtype,FALSE,FALSE,''), \
                    (22,'revocation_reason','text'::regtype,FALSE,FALSE,''), \
                    (23,'economy_eligibility','boolean'::regtype,TRUE,TRUE,'false'), \
                    (24,'approval_receipt_v1','text'::regtype,TRUE,FALSE,''), \
                    (25,'receipt','jsonb'::regtype,TRUE,FALSE,''), \
                    (26,'created_at','timestamp with time zone'::regtype,TRUE,TRUE,'now()') \
                ) AS expected(attnum,attname,atttypid,attnotnull,atthasdef,default_expr) \
                LEFT JOIN pg_attribute a \
                  ON a.attrelid = 'paper_raid_bff_invite_activations'::regclass \
                 AND a.attnum = expected.attnum \
                 AND NOT a.attisdropped \
                LEFT JOIN pg_attrdef d \
                  ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
            ) \
            AND ( \
                SELECT count(*) = 26 FROM pg_attribute \
                WHERE attrelid = 'paper_raid_bff_invite_activations'::regclass \
                  AND attnum > 0 AND NOT attisdropped \
            ) \
            AND ( \
                SELECT count(*) = 2 AND bool_and( \
                    CASE conname \
                      WHEN 'paper_raid_bff_invite_activations_pkey' THEN \
                        contype = 'p' AND convalidated \
                        AND regexp_replace( \
                          pg_get_constraintdef(oid), '[[:space:]]+', '', 'g' \
                        ) = 'PRIMARYKEY(activation_id)' \
                      WHEN 'paper_raid_bff_invite_activation_parity_ck' THEN \
                        contype = 'c' AND convalidated \
                        AND regexp_replace( \
                          pg_get_constraintdef(oid), '[[:space:]]+', '', 'g' \
                        ) = 'CHECK(COALESCE(paper_raid_bff_invite_activation_row_valid_v2(receipt,approval_receipt_v1,approval_receipt_v1_sha256,activation_id,profile_sha256,base_compose_sha256,runtime_acl_sha256,runtime_acl_state_sha256,bff_image,accessctl_image,postgres_image,ops_image,hepta_image,retention_policy_id,retention_policy_sha256,image_lock_sha256,release_provenance_sha256,runtime_acl_evidence_sha256,database_identity,deployment_identity,issued_at,expires_at,revoked_at,revocation_reason,economy_eligibility),false))' \
                      ELSE FALSE \
                    END \
                ) FROM pg_constraint \
                WHERE conrelid = 'paper_raid_bff_invite_activations'::regclass \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_index i JOIN pg_class idx ON idx.oid = i.indexrelid \
                WHERE i.indrelid = 'paper_raid_bff_invite_activations'::regclass \
                  AND idx.relname = 'paper_raid_bff_one_active_invite_activation' \
                  AND i.indisunique AND i.indisvalid AND i.indisready AND i.indislive \
                  AND i.indnkeyatts = 1 AND i.indnatts = 1 \
                  AND i.indkey::text = '0' \
                  AND regexp_replace( \
                    pg_get_expr(i.indexprs, i.indrelid), '[[:space:]]+', '', 'g' \
                  ) IN ('true','(true)') \
                  AND regexp_replace( \
                    pg_get_expr(i.indpred, i.indrelid), '[[:space:]]+', '', 'g' \
                  ) = '(revoked_atISNULL)' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_index i JOIN pg_class idx ON idx.oid = i.indexrelid \
                WHERE i.indrelid = 'paper_raid_bff_invite_activations'::regclass \
                  AND idx.relname = 'paper_raid_bff_invite_activation_expiry' \
                  AND NOT i.indisunique AND i.indisvalid AND i.indisready AND i.indislive \
                  AND i.indnkeyatts = 1 AND i.indnatts = 1 \
                  AND pg_get_indexdef(i.indexrelid, 1, true) = 'expires_at' \
                  AND regexp_replace( \
                    pg_get_expr(i.indpred, i.indrelid), '[[:space:]]+', '', 'g' \
                  ) = '(revoked_atISNULL)' \
            ) \
            AND ( \
                SELECT count(*) = 3 FROM pg_index \
                WHERE indrelid = 'paper_raid_bff_invite_activations'::regclass \
            )",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if !catalog_exact {
        return false;
    }
    let installed_sources = sqlx::query_as::<_, (String, String)>(
        "SELECT \
          (SELECT prosrc FROM pg_proc \
           WHERE oid = 'paper_raid_bff_runtime_acl_state_v2()'::regprocedure), \
          (SELECT prosrc FROM pg_proc \
           WHERE oid = 'paper_raid_bff_invite_activation_row_valid_v2(jsonb,text,text,uuid,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,text,timestamp with time zone,timestamp with time zone,timestamp with time zone,text,boolean)'::regprocedure)",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let v2_catalog_exact = installed_sources
        .zip(expected_runtime_acl_source().zip(expected_invite_activation_validator_source()))
        .is_some_and(|((acl, validator), (expected_acl, expected_validator))| {
            acl.trim() == expected_acl.trim() && validator.trim() == expected_validator.trim()
        });
    v2_catalog_exact && invite_activation_v3_schema_ready(pool).await
}

fn expected_runtime_acl_source() -> Option<&'static str> {
    include_str!("../migrations/0007_invite_activation_authority.sql")
        .split_once("CREATE OR REPLACE FUNCTION paper_raid_bff_runtime_acl_state_v2()")?
        .1
        .split_once("AS $function$")?
        .1
        .split_once("$function$;")
        .map(|(source, _)| source)
}

fn expected_invite_activation_validator_source() -> Option<&'static str> {
    include_str!("../migrations/0007_invite_activation_authority.sql")
        .split_once("CREATE OR REPLACE FUNCTION paper_raid_bff_invite_activation_row_valid_v2(")?
        .1
        .split_once("AS $function$")?
        .1
        .split_once("$function$;")
        .map(|(source, _)| source)
}

async fn invite_activation_v3_schema_ready(pool: &PgPool) -> bool {
    let catalog_exact = sqlx::query_scalar::<_, bool>(
        r#"WITH authority_table AS (
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
            AND (SELECT count(*) = 5 FROM pg_proc procedure
                 JOIN pg_namespace namespace ON namespace.oid = procedure.pronamespace
                 WHERE namespace.nspname = 'public'
                   AND procedure.proname IN (
                       'paper_raid_bff_invite_activation_row_valid_v3',
                       'paper_raid_bff_cluster_identity_v3',
                       'paper_raid_bff_invite_activation_monotonic_v3',
                       'paper_raid_bff_invite_activation_immutable_v3',
                       'paper_raid_bff_invite_activation_truncate_v3'
                   ))
            AND (SELECT count(*) = 5 AND bool_and(
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
            AND EXISTS (SELECT 1 FROM paper_raid_bff_schema_capabilities
                        WHERE capability = 'invite_activation_authority_v3')"#,
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if !catalog_exact {
        return false;
    }
    let installed_sources = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT \
          (SELECT prosrc FROM pg_proc WHERE oid = \
            'paper_raid_bff_invite_activation_row_valid_v3(jsonb,text,text,text,text,uuid,text,text,oid,text,bigint,text,timestamp with time zone,timestamp with time zone,timestamp with time zone,text,boolean)'::regprocedure), \
          (SELECT prosrc FROM pg_proc WHERE oid = \
            'paper_raid_bff_cluster_identity_v3()'::regprocedure), \
          (SELECT prosrc FROM pg_proc WHERE oid = \
            'paper_raid_bff_invite_activation_monotonic_v3()'::regprocedure), \
          (SELECT prosrc FROM pg_proc WHERE oid = \
            'paper_raid_bff_invite_activation_immutable_v3()'::regprocedure), \
          (SELECT prosrc FROM pg_proc WHERE oid = \
            'paper_raid_bff_invite_activation_truncate_v3()'::regprocedure)",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    installed_sources
        .zip(expected_invite_activation_v3_sources())
        .is_some_and(
            |((validator, cluster, monotonic, immutable, truncate), expected)| {
                validator.trim() == expected.0.trim()
                    && cluster.trim() == expected.1.trim()
                    && monotonic.trim() == expected.2.trim()
                    && immutable.trim() == expected.3.trim()
                    && truncate.trim() == expected.4.trim()
            },
        )
}

fn expected_invite_activation_v3_sources() -> Option<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
)> {
    let migration = include_str!("../migrations/0007_invite_activation_authority.sql");
    let source = |marker: &str| {
        migration
            .split_once(marker)?
            .1
            .split_once("AS $function$")?
            .1
            .split_once("$function$;")
            .map(|(body, _)| body)
    };
    Some((
        source("CREATE OR REPLACE FUNCTION paper_raid_bff_invite_activation_row_valid_v3(")?,
        source("CREATE OR REPLACE FUNCTION paper_raid_bff_cluster_identity_v3()")?,
        source("CREATE OR REPLACE FUNCTION paper_raid_bff_invite_activation_monotonic_v3()")?,
        source("CREATE OR REPLACE FUNCTION paper_raid_bff_invite_activation_immutable_v3()")?,
        source("CREATE OR REPLACE FUNCTION paper_raid_bff_invite_activation_truncate_v3()")?,
    ))
}

pub async fn invite_activation_ready(pool: &PgPool, invite: &InviteAlphaConfig) -> bool {
    let Some(expected_sources) = expected_invite_activation_v3_sources() else {
        return false;
    };
    let Some(expected_acl_source) = expected_runtime_acl_source() else {
        return false;
    };
    let pins = &invite.activation;
    sqlx::query_scalar::<_, bool>(include_str!("invite_activation_v3_atomic.sql"))
        .bind(pins.activation_id)
        .bind(&pins.local_approval_sha256)
        .bind(i64::try_from(pins.approval_sequence).unwrap_or_default())
        .bind(&pins.nonce_sha256)
        .bind(&pins.profile_sha256)
        .bind(&pins.base_compose_sha256)
        .bind(&pins.runtime_acl_sha256)
        .bind(&pins.runtime_acl_state_sha256)
        .bind(&invite.retention_policy_id)
        .bind(&pins.retention_policy_sha256)
        .bind(&pins.image_lock_sha256)
        .bind(&pins.release_provenance_sha256)
        .bind(&pins.runtime_acl_evidence_sha256)
        .bind(&pins.database_name)
        .bind(i64::from(pins.database_oid))
        .bind(&pins.cluster_system_identifier)
        .bind(&pins.deployment_identity)
        .bind(&pins.release_id)
        .bind(&pins.postgres_image)
        .bind(&pins.object_store_image)
        .bind(&pins.object_store_client_image)
        .bind(&pins.ops_image)
        .bind(&pins.nakama_image)
        .bind(&pins.hepta_image)
        .bind(&pins.bff_image)
        .bind(&pins.accessctl_image)
        .bind(&pins.hepta_revision)
        .bind(&pins.hepta_source_tree)
        .bind(&pins.hepta_fileset_sha256)
        .bind(expected_sources.0)
        .bind(expected_sources.1)
        .bind(expected_sources.2)
        .bind(expected_sources.3)
        .bind(expected_sources.4)
        .bind(expected_acl_source)
        .fetch_one(pool)
        .await
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AgentBridgeSchemaStatus {
    pub schema_ready: bool,
    pub integrity_ok: bool,
}

const REVIEW_RECEIPT_SCHEMA_READY_SQL: &str = r#"
WITH review_table AS MATERIALIZED (
    SELECT relation.*
      FROM pg_class relation
      JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
     WHERE namespace.nspname = 'public'
       AND relation.relname = 'paper_raid_bff_review_execution_receipts'
), expected_columns AS (
    SELECT * FROM (VALUES
        (1,'receipt_id','uuid'::regtype,TRUE),
        (2,'task_id','uuid'::regtype,TRUE),
        (3,'binding_id','uuid'::regtype,TRUE),
        (4,'assignment_id','uuid'::regtype,TRUE),
        (5,'paper_id','uuid'::regtype,TRUE),
        (6,'submission_id','uuid'::regtype,TRUE),
        (7,'evaluation_id','uuid'::regtype,TRUE),
        (8,'kind','text'::regtype,TRUE),
        (9,'attempt','int8'::regtype,TRUE),
        (10,'fencing_token','int8'::regtype,TRUE),
        (11,'bundle_hash','text'::regtype,TRUE),
        (12,'receipt_hash','text'::regtype,TRUE),
        (13,'receipt','jsonb'::regtype,TRUE),
        (14,'state','text'::regtype,TRUE),
        (15,'confirmation_frame','jsonb'::regtype,FALSE),
        (16,'confirmation_frame_hash','text'::regtype,FALSE),
        (17,'confirmation_idempotency_key','uuid'::regtype,FALSE),
        (18,'confirmation_hash','text'::regtype,FALSE),
        (19,'confirmation_context_signature','text'::regtype,FALSE),
        (20,'response_status','int4'::regtype,FALSE),
        (21,'response_body','bytea'::regtype,FALSE),
        (22,'created_at','timestamptz'::regtype,TRUE),
        (23,'consumed_at','timestamptz'::regtype,FALSE),
        (24,'invalidated_at','timestamptz'::regtype,FALSE),
        (25,'updated_at','timestamptz'::regtype,TRUE)
    ) expected(attnum,attname,atttypid,attnotnull)
)
SELECT
    (SELECT count(*) = 1 AND bool_and(COALESCE(
        relation.relkind = 'r'
        AND relation.relpersistence = 'p'
        AND NOT relation.relrowsecurity
        AND NOT relation.relforcerowsecurity
        AND COALESCE(relation.reloptions, ARRAY[]::text[]) = ARRAY[]::text[]
        AND obj_description(relation.oid, 'pg_class') =
            'Recoverable Agent-signed evaluator/reproducer receipts, each pinned to one assignment version and one immutable frozen review bundle.',
        FALSE
    )) FROM review_table relation)
    AND NOT EXISTS (
        SELECT 1 FROM pg_policy policy_row
        JOIN review_table relation ON relation.oid = policy_row.polrelid
    )
    AND (SELECT count(*) = 25 FROM expected_columns)
    AND (SELECT count(*) = 25 FROM pg_attribute attribute
         JOIN review_table relation ON relation.oid = attribute.attrelid
         WHERE attribute.attnum > 0 AND NOT attribute.attisdropped)
    AND (SELECT count(*) = 25 AND bool_and(COALESCE(
        attribute.attname = expected.attname
        AND attribute.atttypid = expected.atttypid
        AND attribute.attnotnull = expected.attnotnull
        AND NOT attribute.atthasdef
        AND default_value.oid IS NULL
        AND attribute.attgenerated = ''
        AND attribute.attidentity = '',
        FALSE
    )) FROM expected_columns expected
    JOIN review_table relation ON TRUE
    LEFT JOIN pg_attribute attribute
      ON attribute.attrelid = relation.oid
     AND attribute.attnum = expected.attnum
     AND NOT attribute.attisdropped
    LEFT JOIN pg_attrdef default_value
      ON default_value.adrelid = attribute.attrelid
     AND default_value.adnum = attribute.attnum)
    AND (SELECT count(*) = 17 AND bool_and(COALESCE(
        constraint_row.convalidated
        AND constraint_row.conislocal
        AND constraint_row.coninhcount = 0
        AND NOT constraint_row.condeferrable
        AND NOT constraint_row.condeferred
        AND constraint_row.connoinherit = (constraint_row.contype IN ('p','f'))
        AND CASE constraint_row.conname
          WHEN 'paper_raid_bff_review_receipts_pk' THEN
            constraint_row.contype = 'p'
            AND constraint_row.conkey::text = '{1}'
            AND constraint_row.conindid = to_regclass(
                'public.paper_raid_bff_review_receipts_pk'
            )
          WHEN 'paper_raid_bff_review_receipt_binding_fk' THEN
            constraint_row.contype = 'f'
            AND constraint_row.conkey::text = '{3}'
            AND constraint_row.confkey::text = '{1}'
            AND constraint_row.confrelid = to_regclass(
                'public.paper_raid_bff_agent_bridge_bindings'
            )
            AND constraint_row.conindid = to_regclass(
                'public.paper_raid_bff_agent_bridge_bindings_pkey'
            )
            AND constraint_row.confupdtype = 'a'
            AND constraint_row.confdeltype = 'c'
            AND constraint_row.confmatchtype = 's'
          WHEN 'paper_raid_bff_review_receipt_kind_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_attempt_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_fencing_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_bundle_hash_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_hash_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_state_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_frame_hash_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_confirmation_hash_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_context_signature_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_response_status_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_result_shape_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_frame_pair_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_frame_binding_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_lifecycle_ck' THEN
            constraint_row.contype = 'c'
          WHEN 'paper_raid_bff_review_receipt_json_binding_ck' THEN
            constraint_row.contype = 'c'
          ELSE FALSE
        END,
        FALSE
    )) FROM pg_constraint constraint_row
    JOIN review_table relation ON relation.oid = constraint_row.conrelid)
    AND (SELECT count(*) = 4 AND bool_and(COALESCE(
        index_row.indisvalid
        AND index_row.indisready
        AND index_row.indislive
        AND index_row.indimmediate
        AND NOT index_row.indisexclusion
        AND NOT index_row.indisclustered
        AND NOT index_row.indisreplident
        AND NOT index_row.indnullsnotdistinct
        AND index_class.relkind = 'i'
        AND access_method.amname = 'btree'
        AND CASE index_class.relname
          WHEN 'paper_raid_bff_review_receipts_pk' THEN
            index_row.indisunique
            AND index_row.indisprimary
            AND index_row.indnkeyatts = 1
            AND index_row.indnatts = 1
            AND index_row.indkey::text = '1'
            AND index_row.indoption::text = '0'
            AND pg_get_indexdef(index_row.indexrelid, 1, TRUE) = 'receipt_id'
            AND index_row.indexprs IS NULL
            AND index_row.indpred IS NULL
          WHEN 'paper_raid_bff_one_review_receipt_per_task_attempt' THEN
            index_row.indisunique
            AND NOT index_row.indisprimary
            AND index_row.indnkeyatts = 2
            AND index_row.indnatts = 2
            AND index_row.indkey::text = '2 9'
            AND index_row.indoption::text = '0 0'
            AND pg_get_indexdef(index_row.indexrelid, 1, TRUE) = 'task_id'
            AND pg_get_indexdef(index_row.indexrelid, 2, TRUE) = 'attempt'
            AND index_row.indexprs IS NULL
            AND index_row.indpred IS NULL
          WHEN 'paper_raid_bff_one_live_review_receipt_per_task' THEN
            index_row.indisunique
            AND NOT index_row.indisprimary
            AND index_row.indnkeyatts = 1
            AND index_row.indnatts = 1
            AND index_row.indkey::text = '2'
            AND index_row.indoption::text = '0'
            AND pg_get_indexdef(index_row.indexrelid, 1, TRUE) = 'task_id'
            AND index_row.indexprs IS NULL
            AND regexp_replace(
                pg_get_expr(index_row.indpred, index_row.indrelid),
                '[[:space:]]+', '', 'g'
            ) = '(state=ANY(ARRAY[''pending''::text,''consumed''::text]))'
          WHEN 'paper_raid_bff_review_receipt_assignment_inbox_idx' THEN
            NOT index_row.indisunique
            AND NOT index_row.indisprimary
            AND index_row.indnkeyatts = 3
            AND index_row.indnatts = 3
            AND index_row.indkey::text = '4 14 22'
            AND index_row.indoption::text = '0 0 3'
            AND pg_get_indexdef(index_row.indexrelid, 1, TRUE) = 'assignment_id'
            AND pg_get_indexdef(index_row.indexrelid, 2, TRUE) = 'state'
            AND pg_get_indexdef(index_row.indexrelid, 3, TRUE) = 'created_at'
            AND index_row.indexprs IS NULL
            AND index_row.indpred IS NULL
          ELSE FALSE
        END,
        FALSE
    )) FROM pg_index index_row
    JOIN review_table relation ON relation.oid = index_row.indrelid
    JOIN pg_class index_class ON index_class.oid = index_row.indexrelid
    JOIN pg_am access_method ON access_method.oid = index_class.relam)
    AND EXISTS (
        SELECT 1 FROM paper_raid_bff_schema_capabilities
         WHERE capability = 'review_execution_receipts_v1'
    )
"#;

fn compact_review_catalog_definition(definition: &str) -> String {
    definition
        .chars()
        .filter(|character| !matches!(character, ' ' | '\n' | '\r' | '\t' | '(' | ')' | '"'))
        .collect::<String>()
        .to_ascii_lowercase()
        .replace("public.", "")
        .replace("::text", "")
}

fn review_not_distinct_clause(left: &str, right: &str, deparsed: bool) -> String {
    if deparsed {
        format!("not{left}isdistinctfrom{right}")
    } else {
        format!("{left}isnotdistinctfrom{right}")
    }
}

fn review_json_binding_definition(deparsed: bool) -> String {
    let clauses = [
        (
            "receipt->>'schema'",
            "'hepta.paper_raid.agent_bridge.review_receipt_request.v1'",
        ),
        ("receipt->>'idempotency_key'", "receipt_id"),
        (
            "receipt->'receipt'->>'schema'",
            "'hepta.paper_raid.review_execution_receipt.v1'",
        ),
        ("receipt->'receipt'->>'receipt_id'", "receipt_id"),
        ("receipt->'receipt'->>'task_id'", "task_id"),
        ("receipt->'receipt'->>'binding_id'", "binding_id"),
        ("receipt->'receipt'->>'assignment_id'", "assignment_id"),
        ("receipt->'receipt'->>'paper_project_id'", "paper_id"),
        ("receipt->'receipt'->>'submission_id'", "submission_id"),
        ("receipt->'receipt'->>'evaluation_id'", "evaluation_id"),
        ("receipt->'receipt'->>'kind'", "kind"),
        ("receipt->'receipt'->>'attempt'::bigint", "attempt"),
        (
            "receipt->'receipt'->>'fencing_token'::bigint",
            "fencing_token",
        ),
        ("receipt->'receipt'->>'bundle_hash'", "bundle_hash"),
    ];
    format!(
        "check{}",
        clauses
            .iter()
            .map(|(left, right)| review_not_distinct_clause(left, right, deparsed))
            .collect::<Vec<_>>()
            .join("and")
    )
}

fn review_frame_binding_definition(deparsed: bool) -> String {
    let common = [
        ("jsonb_typeofconfirmation_frame", "'object'"),
        (
            "confirmation_frame->>'schema'",
            "'hepta.paper_raid.review_receipt_confirmation_frame.v1'",
        ),
        (
            "jsonb_typeofconfirmation_frame->'receipt_context_signing_bytes'",
            "'string'",
        ),
        (
            "confirmation_frame->'receipt_context'->>'schema'",
            "'hepta.paper_raid.review_receipt_confirmation_context.v1'",
        ),
        (
            "confirmation_frame->'receipt_context'->>'receipt_id'",
            "receipt_id",
        ),
        (
            "confirmation_frame->'receipt_context'->>'receipt_hash'",
            "receipt_hash",
        ),
        (
            "confirmation_frame->'receipt_context'->>'task_id'",
            "task_id",
        ),
        (
            "confirmation_frame->'receipt_context'->>'assignment_id'",
            "assignment_id",
        ),
        (
            "confirmation_frame->'receipt_context'->>'assignment_version'::bigint",
            "fencing_token",
        ),
        (
            "confirmation_frame->'receipt_context'->>'paper_project_id'",
            "paper_id",
        ),
        (
            "confirmation_frame->'receipt_context'->>'submission_id'",
            "submission_id",
        ),
        (
            "confirmation_frame->'receipt_context'->>'evaluation_id'",
            "evaluation_id",
        ),
        ("confirmation_frame->'receipt_context'->>'kind'", "kind"),
        (
            "confirmation_frame->'receipt_context'->>'bundle_hash'",
            "bundle_hash",
        ),
        ("confirmation_frame->>'resource_id'", "paper_id"),
    ]
    .iter()
    .map(|(left, right)| review_not_distinct_clause(left, right, deparsed))
    .collect::<Vec<_>>()
    .join("and");
    let evaluator = [
        (
            "confirmation_frame->>'command'",
            "'create_paper_evaluation_draft'",
        ),
        ("confirmation_frame->'child_id'", "'null'::jsonb"),
        (
            "jsonb_typeofconfirmation_frame->'receipt_context'->'candidate_passed'",
            "'boolean'",
        ),
    ]
    .iter()
    .map(|(left, right)| review_not_distinct_clause(left, right, deparsed))
    .collect::<Vec<_>>()
    .join("and");
    let reproducer = [
        ("confirmation_frame->>'command'", "'submit_reproduction'"),
        ("confirmation_frame->>'child_id'", "evaluation_id"),
        (
            "confirmation_frame->'receipt_context'->'candidate_passed'",
            "'null'::jsonb",
        ),
    ]
    .iter()
    .map(|(left, right)| review_not_distinct_clause(left, right, deparsed))
    .collect::<Vec<_>>()
    .join("and");
    format!(
        "checkconfirmation_frameisnullor{common}andkind='evaluate'and{evaluator}\
         orkind='reproduce'and{reproducer}"
    )
}

fn review_receipt_constraint_definition_is_exact(name: &str, definition: &str) -> bool {
    let compact = compact_review_catalog_definition(definition);
    match name {
        "paper_raid_bff_review_receipts_pk" => compact == "primarykeyreceipt_id",
        "paper_raid_bff_review_receipt_binding_fk" => {
            compact
                == "foreignkeybinding_idreferencespaper_raid_bff_agent_bridge_bindingsbinding_idondeletecascade"
        }
        "paper_raid_bff_review_receipt_kind_ck" => {
            compact == "checkkind=anyarray['evaluate','reproduce']"
        }
        "paper_raid_bff_review_receipt_attempt_ck" => {
            compact == "checkattempt>0andattempt<='9007199254740991'::bigint"
        }
        "paper_raid_bff_review_receipt_fencing_ck" => {
            compact == "checkfencing_token>0andfencing_token<='9007199254740991'::bigint"
        }
        "paper_raid_bff_review_receipt_bundle_hash_ck" => {
            compact == "checkbundle_hash~'^sha256:[0-9a-f]{64}$'"
        }
        "paper_raid_bff_review_receipt_hash_ck" => {
            compact == "checkreceipt_hash~'^sha256:[0-9a-f]{64}$'"
        }
        "paper_raid_bff_review_receipt_state_ck" => {
            compact == "checkstate=anyarray['pending','consumed','invalidated']"
        }
        "paper_raid_bff_review_receipt_frame_hash_ck" => {
            compact
                == "checkconfirmation_frame_hashisnullorconfirmation_frame_hash~'^sha256:[0-9a-f]{64}$'"
        }
        "paper_raid_bff_review_receipt_confirmation_hash_ck" => {
            compact
                == "checkconfirmation_hashisnullorconfirmation_hash~'^sha256:[0-9a-f]{64}$'"
        }
        "paper_raid_bff_review_receipt_context_signature_ck" => {
            compact
                == "checkconfirmation_context_signatureisnullorlengthconfirmation_context_signature=88andconfirmation_context_signature~'^[a-za-z0-9+/]{86}==$'"
        }
        "paper_raid_bff_review_receipt_response_status_ck" => {
            compact
                == "checkresponse_statusisnullorresponse_status>=200andresponse_status<=299"
        }
        "paper_raid_bff_review_receipt_result_shape_ck" => {
            compact == "checkevaluation_idisnotnull"
        }
        "paper_raid_bff_review_receipt_frame_pair_ck" => {
            compact
                == "checkconfirmation_frameisnull=confirmation_frame_hashisnullandconfirmation_frameisnull=confirmation_idempotency_keyisnull"
        }
        "paper_raid_bff_review_receipt_frame_binding_ck" => {
            compact == review_frame_binding_definition(false)
                || compact == review_frame_binding_definition(true)
        }
        "paper_raid_bff_review_receipt_lifecycle_ck" => {
            compact
                == "checkstate='pending'andconsumed_atisnullandinvalidated_atisnullandconfirmation_hashisnullandconfirmation_context_signatureisnullandresponse_statusisnullandresponse_bodyisnullorstate='consumed'andconsumed_atisnotnullandinvalidated_atisnullandconfirmation_frameisnotnullandconfirmation_frame_hashisnotnullandconfirmation_idempotency_keyisnotnullandconfirmation_hashisnotnullandconfirmation_context_signatureisnotnullandresponse_statusisnotnullandresponse_bodyisnotnullorstate='invalidated'andconsumed_atisnullandinvalidated_atisnotnullandconfirmation_hashisnullandconfirmation_context_signatureisnullandresponse_statusisnullandresponse_bodyisnull"
        }
        "paper_raid_bff_review_receipt_json_binding_ck" => {
            compact == review_json_binding_definition(false)
                || compact == review_json_binding_definition(true)
        }
        _ => false,
    }
}

async fn review_receipt_schema_ready(pool: &PgPool) -> bool {
    let catalog_exact = sqlx::query_scalar::<_, bool>(REVIEW_RECEIPT_SCHEMA_READY_SQL)
        .fetch_one(pool)
        .await
        .unwrap_or(false);
    if !catalog_exact {
        return false;
    }
    let definitions = sqlx::query_as::<_, (String, String)>(
        "SELECT constraint_row.conname, \
                pg_get_constraintdef(constraint_row.oid, FALSE) \
           FROM pg_constraint constraint_row \
           JOIN pg_class relation ON relation.oid = constraint_row.conrelid \
           JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace \
          WHERE namespace.nspname = 'public' \
            AND relation.relname = 'paper_raid_bff_review_execution_receipts' \
          ORDER BY constraint_row.conname",
    )
    .fetch_all(pool)
    .await;
    definitions.is_ok_and(|definitions| {
        definitions.len() == 17
            && definitions.iter().all(|(name, definition)| {
                review_receipt_constraint_definition_is_exact(name, definition)
            })
    })
}

pub async fn agent_bridge_schema_status(pool: &PgPool) -> AgentBridgeSchemaStatus {
    let base_schema_ready = sqlx::query_scalar::<_, bool>(
        "SELECT \
            to_regclass('paper_raid_bff_agent_pairing_grants') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_bridge_bindings') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_request_uses') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_health') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_bridge_audit') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_delivery_drafts') IS NOT NULL \
            AND ( \
                SELECT count(*) = 2 FROM pg_index i \
                JOIN pg_class idx ON idx.oid = i.indexrelid \
                WHERE i.indrelid = 'paper_raid_bff_agent_delivery_drafts'::regclass \
                  AND idx.relname IN ( \
                    'paper_raid_bff_one_pending_agent_delivery_tuple', \
                    'paper_raid_bff_agent_delivery_drafts_inbox_idx' \
                  ) \
                  AND i.indisvalid AND i.indisready AND i.indislive \
                  AND ( \
                    (idx.relname = 'paper_raid_bff_one_pending_agent_delivery_tuple' \
                        AND i.indisunique \
                        AND i.indnkeyatts = 7 AND i.indnatts = 7 \
                        AND i.indkey::text = '2 3 4 6 9 10 12' \
                        AND regexp_replace( \
                            pg_get_expr(i.indpred, i.indrelid), \
                            '[[:space:]]+', '', 'g' \
                        ) = '(state=ANY(ARRAY[''pending''::text,''submitting''::text,''consumed''::text]))') \
                    OR (idx.relname = 'paper_raid_bff_agent_delivery_drafts_inbox_idx' \
                        AND NOT i.indisunique \
                        AND i.indnkeyatts = 3 AND i.indnatts = 3 \
                        AND i.indkey::text = '2 3 19' \
                        AND regexp_replace( \
                            pg_get_expr(i.indpred, i.indrelid), \
                            '[[:space:]]+', '', 'g' \
                        ) = '(state=ANY(ARRAY[''pending''::text,''submitting''::text,''consumed''::text]))') \
                  ) \
            ) \
            AND ( \
                SELECT count(*) = 11 FROM pg_constraint \
                WHERE conrelid = 'paper_raid_bff_agent_delivery_drafts'::regclass \
                  AND convalidated \
                  AND conname IN ( \
                    'paper_raid_bff_agent_delivery_drafts_pk', \
                    'paper_raid_bff_agent_delivery_section_key_ck', \
                    'paper_raid_bff_agent_delivery_fencing_ck', \
                    'paper_raid_bff_agent_delivery_work_version_ck', \
                    'paper_raid_bff_agent_delivery_manifest_hash_ck', \
                    'paper_raid_bff_agent_delivery_payload_hash_ck', \
                    'paper_raid_bff_agent_delivery_state_ck', \
                    'paper_raid_bff_agent_delivery_body_hash_ck', \
                    'paper_raid_bff_agent_delivery_binding_fk', \
                    'paper_raid_bff_agent_delivery_ttl_ck', \
                    'paper_raid_bff_agent_delivery_lifecycle_ck' \
                  ) \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_constraint \
                WHERE conrelid = 'paper_raid_bff_agent_delivery_drafts'::regclass \
                  AND conname = 'paper_raid_bff_agent_delivery_fencing_ck' \
                  AND regexp_replace(pg_get_constraintdef(oid), '[[:space:]]+', '', 'g') \
                    = 'CHECK(((lease_fencing_token>0)AND(lease_fencing_token<=''9007199254740991''::bigint)))' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM pg_constraint \
                WHERE conrelid = 'paper_raid_bff_agent_delivery_drafts'::regclass \
                  AND conname = 'paper_raid_bff_agent_delivery_work_version_ck' \
                  AND regexp_replace(pg_get_constraintdef(oid), '[[:space:]]+', '', 'g') \
                    = 'CHECK(((expected_work_version>0)AND(expected_work_version<=''9007199254740991''::bigint)))' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM paper_raid_bff_schema_capabilities \
                WHERE capability = 'agent_bridge_pairing_v1' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM paper_raid_bff_schema_capabilities \
                WHERE capability = 'agent_bridge_delivery_drafts_v1' \
            )",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    let schema_ready = base_schema_ready && review_receipt_schema_ready(pool).await;
    if !schema_ready {
        return AgentBridgeSchemaStatus::default();
    }
    let integrity_ok = sqlx::query_scalar::<_, bool>(
        "SELECT \
            EXISTS ( \
                SELECT 1 FROM pg_trigger \
                WHERE tgname = 'paper_raid_bff_agent_bridge_audit_append_only' \
                  AND tgrelid = 'paper_raid_bff_agent_bridge_audit'::regclass \
                  AND NOT tgisinternal \
            ) \
            AND NOT EXISTS ( \
                SELECT 1 FROM paper_raid_bff_agent_pairing_grants \
                WHERE octet_length(code_hash) <> 32 \
                   OR expires_at <= created_at \
                   OR expires_at > created_at + interval '5 minutes' \
                   OR (state = 'consumed') IS DISTINCT FROM ( \
                        pair_response_status = 200 AND pair_response_body IS NOT NULL \
                   ) \
            ) \
            AND NOT EXISTS ( \
                SELECT 1 FROM paper_raid_bff_agent_bridge_bindings \
                WHERE NOT paper_raid_bff_agent_capability_disclosure_valid_v1( \
                        capability_disclosure, capability_disclosure_hash \
                      ) \
                   OR capability_disclosure ->> 'assurance' \
                    IS DISTINCT FROM 'self_declared_unverified' \
                   OR binding_record ->> 'binding_id' IS DISTINCT FROM binding_id::text \
                   OR binding_record ->> 'player_id' IS DISTINCT FROM player_id::text \
                   OR binding_record ->> 'agent_id' IS DISTINCT FROM agent_id \
                   OR binding_record ->> 'agent_key_id' IS DISTINCT FROM agent_key_id \
                   OR binding_record ->> 'capability_disclosure_hash' \
                        IS DISTINCT FROM capability_disclosure_hash \
                   OR binding_record -> 'capability_disclosure' \
                        IS DISTINCT FROM capability_disclosure \
                   OR binding_record ->> 'status' IS DISTINCT FROM 'active' \
            ) \
            AND NOT EXISTS ( \
                SELECT 1 \
                FROM paper_raid_bff_agent_bridge_bindings b \
                JOIN paper_raid_bff_agent_pairing_grants g \
                  ON g.grant_id = b.last_pairing_grant_id \
                WHERE g.subject_id IS DISTINCT FROM b.subject_id \
                   OR g.player_id IS DISTINCT FROM b.player_id \
                   OR g.state IS DISTINCT FROM 'consumed' \
                   OR g.pinned_binding_id IS DISTINCT FROM b.binding_id \
            ) \
            AND NOT EXISTS ( \
                SELECT 1 FROM paper_raid_bff_agent_request_uses \
                WHERE octet_length(request_hash) <> 32 \
                   OR expires_at <= created_at \
                   OR expires_at > created_at + interval '60 seconds' \
                   OR (response_status IS NULL) IS DISTINCT FROM (response_body IS NULL) \
                   OR (response_status IS NULL) IS DISTINCT FROM (completed_at IS NULL) \
            ) \
            AND NOT EXISTS ( \
                SELECT 1 FROM paper_raid_bff_agent_delivery_drafts \
                WHERE expires_at <= created_at \
                   OR expires_at > created_at + interval '15 minutes' \
                   OR lease_fencing_token <= 0 \
                   OR lease_fencing_token > 9007199254740991 \
                   OR expected_work_version <= 0 \
                   OR expected_work_version > 9007199254740991 \
                   OR artifact_manifest_hash !~ '^sha256:[0-9a-f]{64}$' \
                   OR payload_hash !~ '^sha256:[0-9a-f]{64}$' \
                   OR (proposal_body_hash IS NOT NULL \
                       AND proposal_body_hash !~ '^sha256:[0-9a-f]{64}$') \
                   OR (state = 'pending' AND (proposal_body_hash IS NOT NULL \
                       OR proposal_id IS NOT NULL OR proposal_idempotency_key IS NOT NULL \
                       OR proposal_signed_at_unix IS NOT NULL OR submitting_at IS NOT NULL \
                       OR consumed_at IS NOT NULL OR invalidated_at IS NOT NULL)) \
                   OR (state = 'submitting' AND (proposal_body_hash IS NULL \
                       OR proposal_id IS NULL OR proposal_idempotency_key IS NULL \
                       OR proposal_signed_at_unix IS NULL OR submitting_at IS NULL \
                       OR proposal_signed_at_unix \
                            <> floor(extract(epoch FROM created_at))::bigint \
                       OR consumed_at IS NOT NULL OR invalidated_at IS NOT NULL)) \
                   OR (state = 'consumed' AND (proposal_body_hash IS NULL \
                       OR proposal_id IS NULL OR proposal_idempotency_key IS NULL \
                       OR proposal_signed_at_unix IS NULL OR submitting_at IS NULL \
                       OR proposal_signed_at_unix \
                            <> floor(extract(epoch FROM created_at))::bigint \
                       OR consumed_at IS NULL OR invalidated_at IS NOT NULL)) \
                   OR (state IN ('invalidated','expired') \
                       AND (proposal_body_hash IS NOT NULL OR proposal_id IS NOT NULL \
                       OR proposal_idempotency_key IS NOT NULL \
                       OR proposal_signed_at_unix IS NOT NULL OR submitting_at IS NOT NULL \
                       OR consumed_at IS NOT NULL OR invalidated_at IS NULL)) \
            ) \
            AND NOT EXISTS ( \
                SELECT 1 FROM paper_raid_bff_review_execution_receipts \
                WHERE attempt <= 0 OR attempt > 9007199254740991 \
                   OR fencing_token <= 0 OR fencing_token > 9007199254740991 \
                   OR bundle_hash !~ '^sha256:[0-9a-f]{64}$' \
                   OR receipt_hash !~ '^sha256:[0-9a-f]{64}$' \
                   OR receipt ->> 'schema' IS DISTINCT FROM \
                        'hepta.paper_raid.agent_bridge.review_receipt_request.v1' \
                   OR receipt ->> 'idempotency_key' IS DISTINCT FROM receipt_id::text \
                   OR receipt -> 'receipt' ->> 'schema' IS DISTINCT FROM \
                        'hepta.paper_raid.review_execution_receipt.v1' \
                   OR receipt -> 'receipt' ->> 'receipt_id' IS DISTINCT FROM receipt_id::text \
                   OR receipt -> 'receipt' ->> 'task_id' IS DISTINCT FROM task_id::text \
                   OR receipt -> 'receipt' ->> 'binding_id' IS DISTINCT FROM binding_id::text \
                   OR receipt -> 'receipt' ->> 'assignment_id' IS DISTINCT FROM assignment_id::text \
                   OR receipt -> 'receipt' ->> 'paper_project_id' IS DISTINCT FROM paper_id::text \
                   OR receipt -> 'receipt' ->> 'submission_id' IS DISTINCT FROM submission_id::text \
                   OR receipt -> 'receipt' ->> 'evaluation_id' IS DISTINCT FROM evaluation_id::text \
                   OR receipt -> 'receipt' ->> 'kind' IS DISTINCT FROM kind \
                   OR (confirmation_frame IS NULL) IS DISTINCT FROM \
                        (confirmation_frame_hash IS NULL) \
                   OR (confirmation_frame IS NULL) IS DISTINCT FROM \
                        (confirmation_idempotency_key IS NULL) \
                   OR (confirmation_frame IS NOT NULL AND ( \
                        confirmation_frame ->> 'schema' IS DISTINCT FROM \
                            'hepta.paper_raid.review_receipt_confirmation_frame.v1' \
                        OR jsonb_typeof(confirmation_frame -> 'receipt_context_signing_bytes') \
                            IS DISTINCT FROM 'string' \
                        OR confirmation_frame -> 'receipt_context' ->> 'schema' IS DISTINCT FROM \
                            'hepta.paper_raid.review_receipt_confirmation_context.v1' \
                        OR confirmation_frame -> 'receipt_context' ->> 'receipt_id' \
                            IS DISTINCT FROM receipt_id::text \
                        OR confirmation_frame -> 'receipt_context' ->> 'receipt_hash' \
                            IS DISTINCT FROM receipt_hash \
                        OR confirmation_frame -> 'receipt_context' ->> 'task_id' \
                            IS DISTINCT FROM task_id::text \
                        OR confirmation_frame -> 'receipt_context' ->> 'assignment_id' \
                            IS DISTINCT FROM assignment_id::text \
                        OR confirmation_frame -> 'receipt_context' ->> 'paper_project_id' \
                            IS DISTINCT FROM paper_id::text \
                        OR confirmation_frame -> 'receipt_context' ->> 'submission_id' \
                            IS DISTINCT FROM submission_id::text \
                        OR confirmation_frame -> 'receipt_context' ->> 'evaluation_id' \
                            IS DISTINCT FROM evaluation_id::text \
                        OR confirmation_frame -> 'receipt_context' ->> 'kind' \
                            IS DISTINCT FROM kind \
                        OR confirmation_frame -> 'receipt_context' ->> 'bundle_hash' \
                            IS DISTINCT FROM bundle_hash \
                   )) \
                   OR (confirmation_frame_hash IS NOT NULL AND \
                        confirmation_frame_hash !~ '^sha256:[0-9a-f]{64}$') \
                   OR (confirmation_hash IS NOT NULL AND \
                        confirmation_hash !~ '^sha256:[0-9a-f]{64}$') \
                   OR (confirmation_context_signature IS NOT NULL AND ( \
                        length(confirmation_context_signature) <> 88 \
                        OR confirmation_context_signature !~ '^[A-Za-z0-9+/]{86}==$')) \
                   OR (response_status IS NOT NULL AND \
                        (response_status < 200 OR response_status > 299)) \
                   OR (state='pending' AND (consumed_at IS NOT NULL OR invalidated_at IS NOT NULL \
                        OR confirmation_hash IS NOT NULL \
                        OR confirmation_context_signature IS NOT NULL OR response_status IS NOT NULL \
                        OR response_body IS NOT NULL)) \
                   OR (state='consumed' AND (consumed_at IS NULL OR invalidated_at IS NOT NULL \
                        OR confirmation_frame IS NULL OR confirmation_frame_hash IS NULL \
                        OR confirmation_idempotency_key IS NULL OR confirmation_hash IS NULL \
                        OR confirmation_context_signature IS NULL \
                        OR response_status IS NULL OR response_body IS NULL)) \
                   OR (state='invalidated' AND (consumed_at IS NOT NULL OR invalidated_at IS NULL \
                        OR confirmation_hash IS NOT NULL \
                        OR confirmation_context_signature IS NOT NULL OR response_status IS NOT NULL \
                        OR response_body IS NOT NULL)) \
            ) \
            AND NOT EXISTS ( \
                SELECT 1 FROM ( \
                    SELECT task_id,attempt,state, \
                        lag(attempt) OVER (PARTITION BY task_id ORDER BY attempt) AS previous_attempt, \
                        lead(attempt) OVER (PARTITION BY task_id ORDER BY attempt) AS next_attempt \
                    FROM paper_raid_bff_review_execution_receipts \
                ) sequence \
                WHERE (sequence.previous_attempt IS NULL AND sequence.attempt <> 1) \
                   OR (sequence.previous_attempt IS NOT NULL \
                       AND sequence.attempt <> sequence.previous_attempt + 1) \
                   OR (sequence.next_attempt IS NOT NULL AND sequence.state <> 'invalidated') \
            )",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    AgentBridgeSchemaStatus {
        schema_ready,
        integrity_ok,
    }
}

#[cfg(test)]
mod review_receipt_catalog_tests {
    use super::*;

    fn readiness_contract_error(statement: &str) -> Option<&'static str> {
        for (marker, error) in [
            (
                "count(*) = 25 FROM expected_columns",
                "expected column count",
            ),
            ("count(*) = 25 FROM pg_attribute", "actual column count"),
            ("count(*) = 17 AND bool_and(", "constraint count"),
            ("count(*) = 4 AND bool_and(", "index count"),
            (
                "index_row.indkey::text = '2 9'",
                "task-attempt index columns",
            ),
            ("index_row.indkey::text = '4 14 22'", "inbox index columns"),
            ("index_row.indoption::text = '0 0 3'", "inbox sort order"),
            (
                "pg_get_indexdef(index_row.indexrelid, 3, TRUE) = 'created_at'",
                "inbox timestamp expression",
            ),
            (
                "(state=ANY(ARRAY[''pending''::text,''consumed''::text]))",
                "one-live predicate",
            ),
            ("constraint_row.confdeltype = 'c'", "binding cascade"),
            ("obj_description(relation.oid, 'pg_class')", "table comment"),
        ] {
            if !statement.contains(marker) {
                return Some(error);
            }
        }
        if statement.matches("bool_and(COALESCE(").count() != 4 {
            return Some("NULL-fail-closed aggregates");
        }
        None
    }

    #[test]
    fn review_receipt_catalog_query_rejects_hostile_shape_mutants() {
        assert_eq!(
            readiness_contract_error(REVIEW_RECEIPT_SCHEMA_READY_SQL),
            None
        );
        for (name, from, to) in [
            (
                "missing column",
                "count(*) = 25 FROM expected_columns",
                "count(*) = 24 FROM expected_columns",
            ),
            (
                "missing constraint",
                "count(*) = 17 AND bool_and(",
                "count(*) = 16 AND bool_and(",
            ),
            (
                "extra index",
                "count(*) = 4 AND bool_and(",
                "count(*) = 5 AND bool_and(",
            ),
            (
                "task-attempt drift",
                "index_row.indkey::text = '2 9'",
                "index_row.indkey::text = '2 8'",
            ),
            (
                "inbox order drift",
                "index_row.indoption::text = '0 0 3'",
                "index_row.indoption::text = '0 0 0'",
            ),
            (
                "live predicate drift",
                "(state=ANY(ARRAY[''pending''::text,''consumed''::text]))",
                "(state='pending'::text)",
            ),
            (
                "foreign-key action drift",
                "constraint_row.confdeltype = 'c'",
                "constraint_row.confdeltype = 'a'",
            ),
            ("NULL-skipping aggregate", "bool_and(COALESCE(", "bool_and("),
        ] {
            let mutant = REVIEW_RECEIPT_SCHEMA_READY_SQL.replacen(from, to, 1);
            assert_ne!(mutant, REVIEW_RECEIPT_SCHEMA_READY_SQL, "unbuilt {name}");
            assert!(
                readiness_contract_error(&mutant).is_some(),
                "readiness contract accepted hostile mutant: {name}"
            );
        }
    }

    #[test]
    fn review_receipt_constraint_definitions_reject_hostile_mutants() {
        let exact = [
            (
                "paper_raid_bff_review_receipts_pk",
                "PRIMARY KEY (receipt_id)".to_string(),
            ),
            (
                "paper_raid_bff_review_receipt_binding_fk",
                "FOREIGN KEY (binding_id) REFERENCES paper_raid_bff_agent_bridge_bindings(binding_id) ON DELETE CASCADE".to_string(),
            ),
            (
                "paper_raid_bff_review_receipt_attempt_ck",
                "CHECK ((attempt > 0) AND (attempt <= '9007199254740991'::bigint))".to_string(),
            ),
            (
                "paper_raid_bff_review_receipt_frame_binding_ck",
                review_frame_binding_definition(false),
            ),
            (
                "paper_raid_bff_review_receipt_json_binding_ck",
                review_json_binding_definition(false),
            ),
        ];
        for (name, definition) in &exact {
            assert!(
                review_receipt_constraint_definition_is_exact(name, definition),
                "canonical definition rejected: {name}"
            );
        }

        let hostile = [
            (
                "paper_raid_bff_review_receipt_binding_fk",
                exact[1]
                    .1
                    .replace("ON DELETE CASCADE", "ON DELETE SET NULL"),
            ),
            (
                "paper_raid_bff_review_receipt_attempt_ck",
                exact[2].1.replace("9007199254740991", "9007199254740992"),
            ),
            (
                "paper_raid_bff_review_receipt_frame_binding_ck",
                exact[3]
                    .1
                    .replace("create_paper_evaluation_draft", "submit_reproduction"),
            ),
            (
                "paper_raid_bff_review_receipt_json_binding_ck",
                exact[4].1.replace(
                    "receipt->'receipt'->>'evaluation_id'isnotdistinctfromevaluation_idand",
                    "",
                ),
            ),
        ];
        for (name, definition) in hostile {
            let canonical = exact
                .iter()
                .find_map(|(canonical_name, canonical_definition)| {
                    (canonical_name == &name).then_some(canonical_definition)
                })
                .expect("hostile constraint must have a canonical definition");
            assert_ne!(
                definition, *canonical,
                "hostile constraint mutant was not constructed: {name}"
            );
            assert!(
                !review_receipt_constraint_definition_is_exact(name, &definition),
                "constraint guard accepted hostile definition: {name}"
            );
        }
        assert!(!review_receipt_constraint_definition_is_exact(
            "paper_raid_bff_review_receipt_unknown_ck",
            "CHECK (TRUE)",
        ));
    }
}

#[cfg(test)]
mod invite_activation_atomic_tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeSet;

    #[test]
    fn v3_product_readiness_is_one_exhaustive_postgres_statement() {
        let statement = include_str!("invite_activation_v3_atomic.sql");
        assert_eq!(statement.matches(';').count(), 1);
        assert!(statement.trim_end().ends_with(';'));
        let statement_without_whitespace = statement
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();

        let binds = statement
            .split('$')
            .skip(1)
            .filter_map(|suffix| {
                let digits: String = suffix
                    .chars()
                    .take_while(|character| character.is_ascii_digit())
                    .collect();
                (!digits.is_empty()).then(|| digits.parse::<u8>().expect("bind number"))
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(binds, (1_u8..=35).collect());

        for required in [
            "WITH catalog_exact AS MATERIALIZED (",
            "installed_sources AS MATERIALIZED (",
            "selected_authority AS MATERIALIZED (",
            "FOR SHARE",
            "live_identity AS MATERIALIZED (",
            "live_acl AS MATERIALIZED (",
            "paper_raid_bff_invite_activation_row_valid_v3(",
            "sha256(convert_to(authority.local_approval, 'UTF8'))",
            "authority.revoked_at IS NULL",
            "authority.expires_at > statement_timestamp()",
            "authority.economy_eligibility = FALSE",
        ] {
            assert!(statement.contains(required), "missing {required}");
        }
        let runtime_acl_evidence_digest = "authority.runtime_acl_evidence_sha256=\
'sha256:'||encode(sha256(convert_to(authority.runtime_acl_evidence,'UTF8')),'hex')";
        assert!(
            statement_without_whitespace.contains(runtime_acl_evidence_digest),
            "runtime ACL evidence digest binding is missing or drifted",
        );

        let sources = expected_invite_activation_v3_sources()
            .expect("migration-owned Invite activation function sources");
        for (bind, source) in [
            ("$30", sources.0),
            ("$31", sources.1),
            ("$32", sources.2),
            ("$33", sources.3),
            ("$34", sources.4),
            (
                "$35",
                expected_runtime_acl_source().expect("migration-owned runtime ACL source"),
            ),
        ] {
            assert!(statement.contains(bind));
            assert!(!source.trim().is_empty());
        }
    }

    fn sha256_text(value: &str) -> String {
        format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
    }

    #[tokio::test]
    async fn real_postgres_v3_atomic_catalog_authority_and_revocation_gate() {
        let Ok(database_url) = std::env::var("PAPER_RAID_BFF_TEST_DATABASE_URL") else {
            eprintln!("PAPER_RAID_BFF_TEST_DATABASE_URL is unset; atomic activation gate skipped");
            return;
        };
        let pool = connect(&database_url)
            .await
            .expect("connect test PostgreSQL");
        sqlx::raw_sql(
            "DO $role$ BEGIN IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = \
             'paper_raid_bff_runtime') THEN CREATE ROLE paper_raid_bff_runtime NOLOGIN; \
             END IF; END $role$;",
        )
        .execute(&pool)
        .await
        .expect("create test runtime role");
        migrate(&pool).await.expect("migrate test PostgreSQL");
        sqlx::raw_sql(
            "REVOKE ALL PRIVILEGES ON ALL FUNCTIONS IN SCHEMA public FROM PUBLIC; \
             REVOKE ALL ON FUNCTION pg_catalog.pg_control_system() FROM PUBLIC; \
             GRANT EXECUTE ON FUNCTION pg_catalog.pg_control_system() \
             TO paper_raid_bff; \
             GRANT EXECUTE ON FUNCTION \
             paper_raid_bff_invite_activation_row_valid_v3( \
                JSONB, TEXT, TEXT, TEXT, TEXT, UUID, TEXT, TEXT, OID, TEXT, \
                BIGINT, TEXT, TIMESTAMPTZ, TIMESTAMPTZ, TIMESTAMPTZ, TEXT, BOOLEAN \
             ) TO paper_raid_bff_runtime; \
             GRANT EXECUTE ON FUNCTION paper_raid_bff_cluster_identity_v3() \
             TO paper_raid_bff_runtime; \
             GRANT EXECUTE ON FUNCTION paper_raid_bff_runtime_acl_state_v2() \
             TO paper_raid_bff_runtime;",
        )
        .execute(&pool)
        .await
        .expect("grant approved runtime activation reads");

        let (database_name, database_oid, cluster_system_identifier, issued_at, expires_at) =
            sqlx::query_as::<_, (String, i64, String, DateTime<Utc>, DateTime<Utc>)>(
                "SELECT identity.database_name, identity.database_oid::bigint, \
                 identity.cluster_system_identifier, \
                 date_trunc('second', statement_timestamp()), \
                 date_trunc('second', statement_timestamp()) + interval '1 hour' \
                 FROM paper_raid_bff_cluster_identity_v3() identity",
            )
            .fetch_one(&pool)
            .await
            .expect("read test cluster identity");
        let database_oid = u32::try_from(database_oid).expect("database OID");
        let canonical_acl = sqlx::query_scalar::<_, String>(
            "SELECT canonical_state FROM paper_raid_bff_runtime_acl_state_v2",
        )
        .fetch_one(&pool)
        .await
        .expect("read canonical runtime ACL");
        let runtime_acl_state_sha256 = sha256_text(&canonical_acl);

        let activation_id = Uuid::new_v4();
        let deployment_identity = "atomic-readiness-postgres-test";
        let approval_sequence =
            u64::try_from(Utc::now().timestamp_micros()).expect("positive test approval sequence");
        let nonce_sha256 = sha256_text(&Uuid::new_v4().to_string());
        let fixed_sha256 = format!("sha256:{}", "1".repeat(64));
        let alternate_sha256 = format!("sha256:{}", "2".repeat(64));
        let image = |name: &str| format!("registry.test/{name}@sha256:{}", "3".repeat(64));
        let revision = "4".repeat(40);
        let source_tree = "5".repeat(40);
        let issued_at_text = issued_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let expires_at_text = expires_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let runtime_acl_evidence = json!({
            "database": {
                "name": database_name,
                "oid": database_oid,
                "system_identifier": cluster_system_identifier,
            },
            "runtime_acl_sha256": fixed_sha256,
            "runtime_acl_state_sha256": runtime_acl_state_sha256,
            "schema": "trnm.paper-raid.invite-alpha-runtime-acl-evidence.v2",
            "verified": true,
        });
        let runtime_acl_evidence =
            serde_json::to_string(&runtime_acl_evidence).expect("serialize ACL evidence");
        let runtime_acl_evidence_sha256 = sha256_text(&runtime_acl_evidence);
        let approval = json!({
            "activation": {
                "activation_id": activation_id,
                "deployment_identity": deployment_identity,
                "expires_at": expires_at_text,
                "issued_at": issued_at_text,
                "nonce_sha256": nonce_sha256,
                "sequence": approval_sequence,
            },
            "authority": "root_provisioned_local_only",
            "base_compose_sha256": alternate_sha256,
            "database": {
                "name": database_name,
                "oid": database_oid,
                "system_identifier": cluster_system_identifier,
            },
            "economy_eligibility": false,
            "evidence": {
                "image_lock_sha256": fixed_sha256,
                "release_provenance_sha256": alternate_sha256,
                "runtime_acl_verification_sha256": runtime_acl_evidence_sha256,
            },
            "hepta": {
                "clean": true,
                "committed": true,
                "fileset_sha256": fixed_sha256,
                "revision": revision,
                "source_tree": source_tree,
            },
            "images": {
                "accessctl": image("accessctl"),
                "bff": image("bff"),
                "hepta": image("hepta"),
                "nakama": image("nakama"),
                "object_store": image("object-store"),
                "object_store_client": image("object-store-client"),
                "ops": image("ops"),
                "postgres": image("postgres"),
            },
            "profile_sha256": fixed_sha256,
            "release_id": "paper-raid-atomic-readiness-test",
            "retention": {
                "policy_id": "atomic-readiness-test-v1",
                "policy_sha256": alternate_sha256,
            },
            "runtime_acl_sha256": fixed_sha256,
            "schema": "trnm.paper-raid.invite-alpha-local-approval.v3",
        });
        let local_approval = serde_json::to_string(&approval).expect("serialize approval");
        let local_approval_sha256 = sha256_text(&local_approval);

        sqlx::query(
            "INSERT INTO paper_raid_bff_invite_activation_authorities_v3 ( \
                activation_id, local_approval, local_approval_sha256, \
                runtime_acl_evidence, runtime_acl_evidence_sha256, \
                deployment_identity, database_name, database_oid, \
                cluster_system_identifier, approval_sequence, nonce_sha256, \
                issued_at, expires_at, economy_eligibility, approval_record \
             ) VALUES ( \
                $1,$2,$3,$4,$5,$6,$7,($8::bigint)::oid,$9,$10,$11,$12,$13,FALSE,$14 \
             )",
        )
        .bind(activation_id)
        .bind(&local_approval)
        .bind(&local_approval_sha256)
        .bind(&runtime_acl_evidence)
        .bind(&runtime_acl_evidence_sha256)
        .bind(deployment_identity)
        .bind(&database_name)
        .bind(i64::from(database_oid))
        .bind(&cluster_system_identifier)
        .bind(i64::try_from(approval_sequence).expect("JSON-safe approval sequence"))
        .bind(&nonce_sha256)
        .bind(issued_at)
        .bind(expires_at)
        .bind(&approval)
        .execute(&pool)
        .await
        .expect("insert valid v3 authority");

        let invite = InviteAlphaConfig {
            quota: crate::config::DurableQuotaConfig {
                login_window: Duration::from_secs(60),
                login_global_limit: 1,
                login_bucket_limit: 1,
                mutation_window: Duration::from_secs(60),
                mutation_account_limit: 1,
            },
            retention_policy_id: "atomic-readiness-test-v1".to_string(),
            activation: crate::config::InviteActivationPins {
                activation_id,
                release_id: "paper-raid-atomic-readiness-test".to_string(),
                local_approval_sha256,
                approval_sequence,
                nonce_sha256,
                profile_sha256: fixed_sha256.clone(),
                base_compose_sha256: alternate_sha256.clone(),
                runtime_acl_sha256: fixed_sha256.clone(),
                runtime_acl_state_sha256,
                retention_policy_sha256: alternate_sha256.clone(),
                image_lock_sha256: fixed_sha256.clone(),
                release_provenance_sha256: alternate_sha256.clone(),
                runtime_acl_evidence_sha256,
                hepta_revision: revision,
                hepta_source_tree: source_tree,
                hepta_fileset_sha256: fixed_sha256.clone(),
                database_name,
                database_oid,
                cluster_system_identifier,
                deployment_identity: deployment_identity.to_string(),
                postgres_image: image("postgres"),
                object_store_image: image("object-store"),
                object_store_client_image: image("object-store-client"),
                ops_image: image("ops"),
                nakama_image: image("nakama"),
                hepta_image: image("hepta"),
                bff_image: image("bff"),
                accessctl_image: image("accessctl"),
            },
        };
        assert!(invite_activation_ready(&pool, &invite).await);

        let mut ddl = pool.begin().await.expect("begin catalog drift transaction");
        sqlx::raw_sql(
            "CREATE OR REPLACE FUNCTION paper_raid_bff_invite_activation_row_valid_v3( \
                approval_record_value JSONB, local_approval_value TEXT, \
                local_approval_sha256_value TEXT, runtime_acl_evidence_value TEXT, \
                runtime_acl_evidence_sha256_value TEXT, activation_id_value UUID, \
                deployment_identity_value TEXT, database_name_value TEXT, \
                database_oid_value OID, cluster_system_identifier_value TEXT, \
                approval_sequence_value BIGINT, nonce_sha256_value TEXT, \
                issued_at_value TIMESTAMPTZ, expires_at_value TIMESTAMPTZ, \
                revoked_at_value TIMESTAMPTZ, revocation_reason_value TEXT, \
                economy_eligibility_value BOOLEAN \
             ) RETURNS BOOLEAN LANGUAGE sql IMMUTABLE PARALLEL SAFE \
             CALLED ON NULL INPUT SET search_path = pg_catalog, public \
             AS $function$ SELECT FALSE $function$;",
        )
        .execute(&mut *ddl)
        .await
        .expect("stage catalog source drift");
        let readiness_pool = pool.clone();
        let readiness_invite = invite.clone();
        let readiness = tokio::spawn(async move {
            invite_activation_ready(&readiness_pool, &readiness_invite).await
        });
        tokio::task::yield_now().await;
        ddl.commit().await.expect("commit catalog source drift");
        // The overlapping statement may legally own the pre-DDL or post-DDL
        // MVCC snapshot; it must complete without a mixed-snapshot error. The
        // first statement that starts after commit must reject the drift.
        let _snapshot_result = readiness.await.expect("join concurrent readiness");
        assert!(!invite_activation_ready(&pool, &invite).await);

        migrate(&pool).await.expect("restore exact catalog source");
        assert!(invite_activation_ready(&pool, &invite).await);
        sqlx::query(
            "UPDATE paper_raid_bff_invite_activation_authorities_v3 \
                SET revoked_at = statement_timestamp(), \
                    revocation_reason = 'operator_revoked' \
              WHERE activation_id = $1",
        )
        .bind(activation_id)
        .execute(&pool)
        .await
        .expect("revoke authority");
        assert!(!invite_activation_ready(&pool, &invite).await);

        migrate(&pool)
            .await
            .expect("idempotently restore exact catalog after revocation");
        assert!(!invite_activation_ready(&pool, &invite).await);
    }
}
