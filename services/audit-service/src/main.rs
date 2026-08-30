use audit_service::{build_router, state::AppState, validate_internal_service_auth};
use shared_config::runtime_guard::{self, ServiceKind};
use shared_tracing::init_tracing;

fn main() {
    // SAFETY: this is the first startup action, before tracing, Tokio, or any
    // application worker thread is initialized.
    let prepared = match unsafe { runtime_guard::prepare_process_environment(ServiceKind::Audit) } {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("audit-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };

    init_tracing();
    let runtime = match runtime_guard::build_multi_thread_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("audit-service async runtime initialization failed: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    runtime.block_on(run(prepared));
}

async fn run(prepared: runtime_guard::PreparedStartup) {
    let startup = match runtime_guard::enforce_prepared(prepared).await {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("audit-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    if let Err(error) = validate_internal_service_auth(startup.profile.is_production_like()) {
        eprintln!("audit-service startup rejected: {error}");
        std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
    }
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
