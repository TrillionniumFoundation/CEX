use capability_service::{build_router, AppState};
use shared_config::runtime_guard::CONFIG_ERROR_EXIT_CODE;
use shared_tracing::init_tracing;
use std::env;

#[tokio::main]
async fn main() {
    init_tracing();

    let state = match AppState::from_env().await {
        Ok(state) => state,
        Err(error) => {
            eprintln!("capability-service startup rejected: {error}");
            std::process::exit(CONFIG_ERROR_EXIT_CODE);
        }
    };
    let app = build_router(state);
    let bind_addr = env::var("CAPABILITY_BIND_ADDR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "127.0.0.1:7005".to_string());

    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("capability-service bind rejected for {bind_addr}: {error}");
            std::process::exit(CONFIG_ERROR_EXIT_CODE);
        }
    };
    if let Err(error) = axum::serve(listener, app).await {
        eprintln!("capability-service terminated: {error}");
        std::process::exit(1);
    }
}
