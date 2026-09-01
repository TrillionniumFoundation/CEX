use ledger_service::{
    build_router,
    repository::{postgres::PostgresLedgerRepository, LedgerRepositoryHandle},
    state::AppState,
};
use shared_config::runtime_guard::{self, ServiceKind};
use shared_tracing::init_tracing;
use std::sync::Arc;

const SHARED_RUNTIME_GUARD_SERVICE_KINDS: [ServiceKind; 5] = [
    ServiceKind::Gateway,
    ServiceKind::Identity,
    ServiceKind::Ledger,
    ServiceKind::Execution,
    ServiceKind::Audit,
];

fn main() {
    debug_assert!(SHARED_RUNTIME_GUARD_SERVICE_KINDS.contains(&ServiceKind::Ledger));

    // SAFETY: this is the first startup action, before tracing, Tokio, or any
    // application worker thread is initialized.
    let prepared = match unsafe { runtime_guard::prepare_process_environment(ServiceKind::Ledger) }
    {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("ledger-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };

    init_tracing();
    let runtime = match runtime_guard::build_multi_thread_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("ledger-service async runtime initialization failed: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    runtime.block_on(run(prepared));
}

async fn run(prepared: runtime_guard::PreparedStartup) {
    let startup = match runtime_guard::enforce_prepared(prepared).await {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("ledger-service startup rejected: {error}");
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

    let fail_fast = runtime_guard::env_flag("LEDGER_FAIL_FAST", false);

    let (repository, operation_pool): (LedgerRepositoryHandle, _) =
        match PostgresLedgerRepository::connect_from_env().await {
            Ok(repo) => {
                let operation_pool = repo.pool.clone();
                (Arc::new(repo), operation_pool)
            }
            Err(err) if fail_fast => {
                eprintln!("ledger repository connection failed with LEDGER_FAIL_FAST=true: {err}");
                std::process::exit(1);
            }
            Err(_) => (PostgresLedgerRepository::new_placeholder(), None),
        };

    let state = AppState::new_with_operation_pool(repository, operation_pool);
    let app = build_router(state);

    let bind_addr =
        std::env::var("LEDGER_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:7002".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
