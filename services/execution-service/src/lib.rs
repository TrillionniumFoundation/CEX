pub mod api;
pub mod dispatch_policy;
pub mod providers;
pub mod state;

use axum::{
    routing::{get, post},
    Router,
};
use state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(api::health))
        .route("/v1/executions", post(api::create_execution))
        .route("/v1/executions/worker-queue", get(api::worker_queue))
        .route(
            "/v1/executions/worker-queue/summary",
            get(api::worker_queue_summary),
        )
        .route(
            "/v1/executions/provider-dead-letters",
            get(api::provider_dead_letters),
        )
        .route("/v1/executions/claim-next", post(api::claim_next_execution))
        .route(
            "/v1/executions/claim-batch",
            post(api::claim_execution_batch),
        )
        .route(
            "/v1/executions/reclaim-expired",
            post(api::reclaim_expired_executions),
        )
        .route(
            "/v1/executions/timeout-expired",
            post(api::timeout_expired_executions),
        )
        .route("/v1/executions/:id", get(api::get_execution))
        .route("/v1/executions/:id/approve", post(api::approve_execution))
        .route("/v1/executions/:id/reject", post(api::reject_execution))
        .route("/v1/executions/:id/dispatch", post(api::dispatch_execution))
        .route("/v1/executions/:id/start", post(api::start_execution))
        .route("/v1/executions/:id/process", post(api::process_execution))
        .route(
            "/v1/executions/:id/renew-lease",
            post(api::renew_execution_lease),
        )
        .route("/v1/executions/:id/requeue", post(api::requeue_execution))
        .route("/v1/executions/:id/retry", post(api::retry_execution))
        .route("/v1/executions/:id/cancel", post(api::cancel_execution))
        .route("/v1/executions/:id/timeout", post(api::timeout_execution))
        .route("/v1/executions/:id/succeed", post(api::succeed_execution))
        .route("/v1/executions/:id/fail", post(api::fail_execution))
        .route("/v1/executions/info", get(api::execution_info))
        .route("/v1/info", get(api::execution_info))
        .with_state(state)
}
