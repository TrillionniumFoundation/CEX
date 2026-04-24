pub mod api;
pub mod state;

use axum::{
    routing::{get, post},
    Router,
};
use state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(api::health))
        .route("/v1/audit/events", post(api::create_event))
        .route(
            "/v1/audit/events/trace/:trace_id",
            get(api::list_events_by_trace),
        )
        .with_state(state)
}
