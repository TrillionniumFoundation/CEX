use axum::Router;
use matrix_entry_adapter::{build_router, AppState};
use shared_tracing::init_tracing;

#[tokio::main]
async fn main() {
    init_tracing();

    let state = match AppState::from_env().await {
        Ok(state) => state,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };
    let app: Router = build_router(state.clone());

    let listener = tokio::net::TcpListener::bind(&state.config().bind_addr)
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
