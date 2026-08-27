#[path = "../../../crates/shared-config/src/runtime_guard.rs"]
mod runtime_guard;

use audit_service::{build_router, state::AppState};
use runtime_guard::ServiceKind;
use shared_tracing::init_tracing;

#[tokio::main]
async fn main() {
    init_tracing();

    let startup = match runtime_guard::enforce(ServiceKind::Audit).await {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("audit-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    eprintln!(
        "audit-service startup guard accepted profile={} db_preflight={}",
        startup.profile, startup.database_preflight
    );

    let state = AppState::from_env().await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:7004")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
