use audit_service::{build_router, state::AppState};
use shared_tracing::init_tracing;

#[tokio::main]
async fn main() {
    init_tracing();

    let state = AppState::from_env().await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:7004")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
