#[path = "../../../crates/shared-config/src/service_client.rs"]
mod service_client;

use execution_service::{build_router, state::AppState, validate_internal_service_auth};
use shared_config::{
    load_ledger_scoped_admin_tokens,
    runtime_guard::{self, ServiceKind},
    select_ledger_manage_token,
};
use shared_tracing::init_tracing;

fn main() {
    // SAFETY: this is the first startup action, before tracing, Tokio, or any
    // application worker thread is initialized.
    let prepared =
        match unsafe { runtime_guard::prepare_process_environment(ServiceKind::Execution) } {
            Ok(prepared) => prepared,
            Err(error) => {
                eprintln!("execution-service startup rejected: {error}");
                std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
            }
        };

    init_tracing();
    let runtime = match runtime_guard::build_multi_thread_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("execution-service async runtime initialization failed: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    runtime.block_on(run(prepared));
}

async fn run(prepared: runtime_guard::PreparedStartup) {
    let startup = match runtime_guard::enforce_prepared(prepared).await {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("execution-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    if let Err(error) = validate_internal_service_auth(startup.profile.is_production_like()) {
        eprintln!("execution-service startup rejected: {error}");
        std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
    }
    if startup.profile.is_production_like() {
        let ledger_tokens = load_ledger_scoped_admin_tokens();
        if select_ledger_manage_token(&ledger_tokens).is_none() {
            eprintln!(
                "execution-service startup rejected: production-like execution requires an explicit ledger:manage downstream principal"
            );
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    }
    let internal_http = match service_client::build_internal_http_client(
        "execution-service",
        startup.profile.is_production_like(),
    ) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("execution-service startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    eprintln!(
        "execution-service startup guard accepted profile={} db_preflight={}",
        startup.profile, startup.database_preflight
    );

    let mut state = AppState::from_env().await;
    state.http = internal_http;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:7003")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
