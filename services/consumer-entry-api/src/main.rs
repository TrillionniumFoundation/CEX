use axum::{middleware, Router};
use consumer_entry_api::{build_router, AppState};
use shared_tracing::init_tracing;

mod matrix_result_lookup;
mod matrix_result_response_binding;

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
    let result_lookup = matrix_result_lookup::router(state.config().clone()).layer(
        middleware::from_fn(matrix_result_response_binding::enforce_matrix_result_response_binding),
    );
    let app: Router = build_router(state.clone()).merge(result_lookup);

    let listener = tokio::net::TcpListener::bind(&state.config().bind_addr)
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
