#[path = "../../../crates/shared-config/src/service_client.rs"]
mod service_client;

use axum::Router;
use gateway_service::{
    build_router,
    infrastructure::{clients::ServiceClients, state::AppState},
};
use shared_config::{
    runtime_guard::{self, ServiceKind},
    GatewayConfig,
};
use shared_tracing::init_tracing;

const SHARED_RUNTIME_GUARD_SERVICE_KINDS: [ServiceKind; 5] = [
    ServiceKind::Gateway,
    ServiceKind::Identity,
    ServiceKind::Ledger,
    ServiceKind::Execution,
    ServiceKind::Audit,
];

fn main() {
    debug_assert!(SHARED_RUNTIME_GUARD_SERVICE_KINDS.contains(&ServiceKind::Gateway));

    // SAFETY: this is the first startup action, before tracing, Tokio, or any
    // application worker thread is initialized.
    let prepared = match unsafe { runtime_guard::prepare_process_environment(ServiceKind::Gateway) }
    {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("gateway-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };

    init_tracing();
    let runtime = match runtime_guard::build_multi_thread_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("gateway-service async runtime initialization failed: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    runtime.block_on(run(prepared));
}

async fn run(prepared: runtime_guard::PreparedStartup) {
    let startup = match runtime_guard::enforce_prepared(prepared).await {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("gateway-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    eprintln!(
        "{} startup guard accepted profile={} db_preflight={} identity_static_fallback_disabled={}",
        startup.service,
        startup.profile,
        startup.database_preflight,
        startup.identity_static_fallback_disabled
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
