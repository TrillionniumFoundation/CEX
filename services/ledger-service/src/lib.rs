pub mod api;
pub mod repository;
pub mod state;

use axum::{
    routing::{get, post},
    Router,
};
use state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(api::health))
        .route("/v1/accounts", post(api::create_account))
        .route("/v1/accounts/:id", get(api::get_account))
        .route("/v1/ledger/reserve", post(api::reserve_credits))
        .route("/v1/ledger/consume", post(api::consume_credits))
        .route("/v1/ledger/refund", post(api::refund_credits))
        .with_state(state)
}
