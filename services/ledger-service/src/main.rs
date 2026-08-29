#[path = "../../../crates/shared-config/src/runtime_guard.rs"]
mod runtime_guard;

use ledger_service::{
    build_router, repository::postgres::PostgresLedgerRepository, state::AppState,
};
use runtime_guard::ServiceKind;
use shared_tracing::init_tracing;

fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
        .unwrap_or(default)
}

#[tokio::main]
async fn main() {
    init_tracing();

    let startup = match runtime_guard::enforce(ServiceKind::Ledger).await {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("ledger-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    eprintln!(
        "ledger-service startup guard accepted profile={} db_preflight={}",
        startup.profile, startup.database_preflight
    );

    let fail_fast = env_flag("LEDGER_FAIL_FAST", false);

    let repository = match PostgresLedgerRepository::connect_from_env().await {
        Ok(repo) => std::sync::Arc::new(repo),
        Err(err) if fail_fast => {
            eprintln!("ledger repository connection failed with LEDGER_FAIL_FAST=true: {err}");
            std::process::exit(1);
        }
        Err(_) => PostgresLedgerRepository::new_placeholder(),
    };

    let state = AppState::new(repository);
    let app = build_router(state);

    let bind_addr =
        std::env::var("LEDGER_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:7002".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
