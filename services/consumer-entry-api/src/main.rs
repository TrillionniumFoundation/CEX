use axum::Router;
use consumer_entry_api::{build_router, AppState};
use shared_tracing::init_tracing;

mod matrix_result_lookup;

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
    let app: Router = build_router(state.clone())
        .merge(matrix_result_lookup::router(state.config().clone()));

    let listener = tokio::net::TcpListener::bind(&state.config().bind_addr)
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
