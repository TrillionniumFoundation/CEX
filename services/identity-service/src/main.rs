#[path = "../../../crates/shared-config/src/runtime_guard.rs"]
mod runtime_guard;
#[path = "../../../crates/shared-config/src/service_auth.rs"]
mod service_auth;
#[path = "../../../crates/shared-config/src/service_client.rs"]
mod service_client;

use axum::middleware;
use identity_service::{build_router, harden_runtime_state, AppState};
use runtime_guard::ServiceKind;
use service_auth::ServiceAuthConfig;
use shared_tracing::init_tracing;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    init_tracing();

    let startup = match runtime_guard::enforce(ServiceKind::Identity).await {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("identity-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    let internal_http = match service_client::build_internal_http_client(
        "identity-service",
        startup.profile.is_production_like(),
    ) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("identity-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    let resolve_auth =
        match ServiceAuthConfig::identity_resolve_from_env(startup.profile.is_production_like()) {
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
    harden_runtime_state(
        &mut state,
        internal_http,
        startup.profile.is_production_like(),
    );
    let app = build_router(state).layer(middleware::from_fn_with_state(
        resolve_auth,
        service_auth::require_identity_resolve_auth,
    ));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:7001")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
