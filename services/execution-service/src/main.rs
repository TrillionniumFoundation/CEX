#[path = "../../../crates/shared-config/src/runtime_guard.rs"]
mod runtime_guard;
#[path = "../../../crates/shared-config/src/service_client.rs"]
mod service_client;

use execution_service::{build_router, state::AppState, validate_internal_service_auth};
use runtime_guard::ServiceKind;
use shared_tracing::init_tracing;

#[tokio::main]
async fn main() {
    init_tracing();

    let startup = match runtime_guard::enforce(ServiceKind::Execution).await {
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
