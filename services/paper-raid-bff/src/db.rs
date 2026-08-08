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
