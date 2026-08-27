#[path = "../../../crates/shared-config/src/runtime_guard.rs"]
mod runtime_guard;

use identity_service::{build_router, AppState};
use runtime_guard::ServiceKind;
use shared_tracing::init_tracing;

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
    eprintln!(
        "identity-service startup guard accepted profile={} db_preflight={} static_fallback_disabled={}",
        startup.profile,
        startup.database_preflight,
        startup.identity_static_fallback_disabled
    );

    let state = AppState::from_env().await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:7001")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
