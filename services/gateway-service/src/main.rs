use axum::Router;
use gateway_service::{
    build_router,
    infrastructure::{clients::ServiceClients, state::AppState},
};
use shared_config::GatewayConfig;
use shared_tracing::init_tracing;

#[tokio::main]
async fn main() {
    init_tracing();

    let config = GatewayConfig::from_env();
    let clients = ServiceClients::new(
        config.identity_base_url.clone(),
        config.ledger_base_url.clone(),
        config.execution_base_url.clone(),
        config.audit_base_url.clone(),
        config.capability_base_url.clone(),
    );
    let state = AppState::from_env(clients).await;
    let app: Router = build_router(state);

    let listener = tokio::net::TcpListener::bind(config.bind_addr())
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
