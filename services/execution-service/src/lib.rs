#[path = "../../../crates/shared-config/src/service_auth.rs"]
mod service_auth;

pub mod api;
pub mod dispatch_policy;
pub mod ledger_settlement;
pub mod provider_dispatch;
pub mod providers;
pub mod settlement_worker;
pub mod state;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use service_auth::ServiceAuthConfig;
use state::AppState;
use std::sync::Arc;

pub fn validate_internal_service_auth(require_enforce: bool) -> Result<(), String> {
    ServiceAuthConfig::execution_create_from_env(require_enforce).map(|_| ())
}

pub fn build_router(state: AppState) -> Router {
    let execution_create_auth = Arc::new(
        ServiceAuthConfig::execution_create_from_env(false).unwrap_or_else(|error| {
            panic!("execution-service internal auth configuration rejected: {error}")
        }),
    );
    let protected_execution_create = Router::new()
        .route("/v1/executions", post(api::create_execution))
        .route_layer(middleware::from_fn_with_state(
            execution_create_auth,
            service_auth::require_service_auth,
        ));

    Router::new()
        .route("/health", get(api::health))
        .route("/metrics", get(api::metrics))
        .merge(protected_execution_create)
        .route("/v1/executions/worker-queue", get(api::worker_queue))
        .route(
            "/v1/executions/worker-queue/summary",
            get(api::worker_queue_summary),
        )
        .route(
            "/v1/executions/provider-dead-letters",
            get(api::provider_dead_letters),
        )
        .route(
            "/v1/executions/provider-failures",
            get(api::provider_failures),
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
        .route(
            "/v1/executions/:id/provider-dead-letter/ack",
            post(api::acknowledge_provider_dead_letter),
        )
        .route(
            "/v1/executions/:id/provider-failure/ack",
            post(api::acknowledge_provider_failure),
        )
        .route("/v1/executions/:id/approve", post(api::approve_execution))
        .route("/v1/executions/:id/reject", post(api::reject_execution))
        .route("/v1/executions/:id/dispatch", post(api::dispatch_execution))
        .route(
            "/v1/executions/:id/start",
            post(provider_dispatch::start_execution),
        )
        .route(
            "/v1/executions/:id/process",
            post(provider_dispatch::process_execution),
        )
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
