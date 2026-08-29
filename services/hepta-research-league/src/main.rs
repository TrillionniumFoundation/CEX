use hepta_research_league::{app, AppState, SecurityConfig};
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hepta_research_league=info".into()),
        )
        .init();

    let bind_addr =
        std::env::var("HEPTA_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:7011".to_string());
    let listener = TcpListener::bind(&bind_addr)
        .await
        .expect("bind Hepta Research League listener");
    info!(%bind_addr, "Hepta Research League listening");
    let security = SecurityConfig::from_env().expect("load Hepta service authentication");
    let database_url = std::env::var("HEPTA_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("HEPTA_DATABASE_URL or DATABASE_URL must be set");
    let state = AppState::connect(&database_url, security)
        .await
        .expect("initialize durable Hepta repository");
    axum::serve(listener, app(state))
        .await
        .expect("serve Hepta Research League");
}
