#[path = "../../../crates/shared-config/src/service_client.rs"]
mod service_client;

use axum::middleware;
use identity_service::{build_router, harden_runtime_state, validate_runtime_backend, AppState};
use shared_config::{
    runtime_guard::{self, ServiceKind},
    service_auth::{self, ServiceAuthConfig},
};
use shared_tracing::init_tracing;
use std::sync::Arc;

fn main() {
    // SAFETY: this is the first startup action, before tracing, Tokio, or any
    // application worker thread is initialized.
    let prepared =
        match unsafe { runtime_guard::prepare_process_environment(ServiceKind::Identity) } {
            Ok(prepared) => prepared,
            Err(error) => {
                eprintln!("identity-service startup rejected: {error}");
                std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
            }
        };

    init_tracing();
    let runtime = match runtime_guard::build_multi_thread_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("identity-service async runtime initialization failed: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    runtime.block_on(run(prepared));
}

async fn run(prepared: runtime_guard::PreparedStartup) {
    let startup = match runtime_guard::enforce_prepared(prepared).await {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("identity-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    let production_like = startup.profile.is_production_like();
    let internal_http =
        match service_client::build_internal_http_client("identity-service", production_like) {
            Ok(client) => client,
            Err(error) => {
                eprintln!("identity-service startup rejected: {error}");
                std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
            }
        };
    let resolve_auth = match ServiceAuthConfig::identity_resolve_from_env(production_like) {
        Ok(config) => Arc::new(config),
        Err(error) => {
            eprintln!("identity-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    eprintln!(
        "identity-service startup guard accepted profile={} db_preflight={} static_fallback_disabled={}",
        startup.profile,
        startup.database_preflight,
        startup.identity_static_fallback_disabled
    );

    let mut state = AppState::from_env().await;
    if let Err(error) = validate_runtime_backend(&state, production_like) {
        eprintln!("identity-service startup rejected: {error}");
        std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
    }
    harden_runtime_state(&mut state, internal_http, production_like);
    let app = build_router(state).layer(middleware::from_fn_with_state(
        resolve_auth,
        service_auth::require_identity_resolve_auth,
    ));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:7001")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
