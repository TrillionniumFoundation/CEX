use sqlx::postgres::PgPoolOptions;
use std::{net::SocketAddr, time::Duration};
use trnm_economy_service::{
    build_router, AppState, AuthorityRegistry, IssuerKeyRegistry, SettlementRepository,
};

#[tokio::main]
async fn main() -> Result<(), String> {
    shared_tracing::init_tracing();

    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required".to_string())?;
    let pool = PgPoolOptions::new()
        .min_connections(2)
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await
        .map_err(|error| format!("connect TRNM economy PostgreSQL: {error}"))?;

    let repository = SettlementRepository::new(pool);
    if std::env::var("TRNM_CEX_SETTLEMENT_APPLY_MIGRATIONS").as_deref() == Ok("1") {
        repository
            .apply_migration()
            .await
            .map_err(|error| format!("apply TRNM economy migration: {error:?}"))?;
    }
    if !repository.schema_ready().await {
        return Err(
            "TRNM economy schema is missing; apply migrations/0010_trnm_economy_settlement_v1.sql"
                .to_string(),
        );
    }

    let authorities = AuthorityRegistry::from_env()?;
    let issuer_keys = IssuerKeyRegistry::from_env()?;
    let state = AppState::new(repository, authorities, issuer_keys);
    let app = build_router(state);

    let host =
        std::env::var("TRNM_CEX_SETTLEMENT_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("TRNM_CEX_SETTLEMENT_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(7010);
    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|error| format!("invalid TRNM economy bind address: {error}"))?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("bind TRNM economy service: {error}"))?;

    tracing::info!(%address, "TRNM CEX settlement service listening");
    axum::serve(listener, app)
        .await
        .map_err(|error| format!("serve TRNM economy service: {error}"))
}
