#[path = "../../../crates/shared-config/src/runtime_guard.rs"]
mod runtime_guard;
#[path = "../../../crates/shared-config/src/service_client.rs"]
mod service_client;

use axum::Router;
use gateway_service::{
    build_router,
    infrastructure::{clients::ServiceClients, state::AppState},
};
use runtime_guard::ServiceKind;
use shared_config::GatewayConfig;
use shared_tracing::init_tracing;

#[tokio::main]
async fn main() {
    init_tracing();

    let startup = match runtime_guard::enforce(ServiceKind::Gateway).await {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("gateway-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    eprintln!(
        "gateway-service startup guard accepted profile={} db_preflight={}",
        startup.profile, startup.database_preflight
    );

    let config = GatewayConfig::from_env();
    let mut clients = ServiceClients::new(
        config.identity_base_url.clone(),
        config.ledger_base_url.clone(),
        config.execution_base_url.clone(),
        config.audit_base_url.clone(),
        config.capability_base_url.clone(),
    );
    clients.http = match service_client::build_internal_http_client(
        "gateway-service",
        startup.profile.is_production_like(),
    ) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("gateway-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };

    let state = AppState::from_env(clients).await;
    let app: Router = build_router(state);

    let listener = tokio::net::TcpListener::bind(config.bind_addr())
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
