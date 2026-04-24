use ledger_service::{
    build_router, repository::postgres::PostgresLedgerRepository, state::AppState,
};
use shared_tracing::init_tracing;

#[tokio::main]
async fn main() {
    init_tracing();

    let repository = match PostgresLedgerRepository::connect_from_env().await {
        Ok(repo) => std::sync::Arc::new(repo),
        Err(_) => PostgresLedgerRepository::new_placeholder(),
    };

    let state = AppState::new(repository);
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:7002")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
