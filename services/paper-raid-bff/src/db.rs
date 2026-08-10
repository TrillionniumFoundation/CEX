use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;
use uuid::Uuid;

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
    sqlx::query_scalar::<_, bool>(
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
            AND EXISTS ( \
                SELECT 1 FROM paper_raid_bff_schema_capabilities \
                WHERE capability = 'invite_alpha_access_v1' \
            ) \
            AND EXISTS ( \
                SELECT 1 FROM paper_raid_bff_schema_capabilities \
                WHERE capability = 'agent_bridge_pairing_v1' \
            )",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AgentBridgeSchemaStatus {
    pub schema_ready: bool,
    pub integrity_ok: bool,
}

pub async fn agent_bridge_schema_status(pool: &PgPool) -> AgentBridgeSchemaStatus {
    let schema_ready = sqlx::query_scalar::<_, bool>(
        "SELECT \
            to_regclass('paper_raid_bff_agent_pairing_grants') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_bridge_bindings') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_request_uses') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_health') IS NOT NULL \
            AND to_regclass('paper_raid_bff_agent_bridge_audit') IS NOT NULL \
            AND EXISTS ( \
                SELECT 1 FROM paper_raid_bff_schema_capabilities \
                WHERE capability = 'agent_bridge_pairing_v1' \
            )",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
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
