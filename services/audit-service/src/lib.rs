pub(crate) use shared_config::service_auth;

pub mod api;
pub mod outbox_dispatcher;
pub mod state;
pub mod v2;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use service_auth::ServiceAuthConfig;
use state::AppState;
use std::sync::Arc;

pub fn validate_internal_service_auth(require_enforce: bool) -> Result<(), String> {
    ServiceAuthConfig::audit_write_from_env(require_enforce).map(|_| ())
}

pub fn build_router(state: AppState) -> Router {
    let audit_write_auth = Arc::new(
        ServiceAuthConfig::audit_write_from_env(false).unwrap_or_else(|error| {
            panic!("audit-service internal auth configuration rejected: {error}")
        }),
    );
    let protected_audit_write = Router::new()
        .route("/v1/audit/events", post(api::create_event))
        .route("/v1/audit/events/v2", post(v2::create_event_v2))
        .route("/v2/audit/events", post(v2::create_event_v2))
        .route_layer(middleware::from_fn_with_state(
            audit_write_auth,
            service_auth::require_service_auth,
        ));

    Router::new()
        .route("/health", get(api::health))
        .route("/metrics", get(api::metrics))
        .route("/metrics/audit-v2", get(v2::metrics_v2))
        .route("/v2/audit/metrics", get(v2::metrics_v2))
        .merge(protected_audit_write)
        .route(
            "/v1/audit/events/trace/:trace_id",
            get(api::list_events_by_trace),
        )
        .route(
            "/v1/audit/events/v2/trace/:trace_id",
            get(v2::list_events_by_trace_v2),
        )
        .route(
            "/v2/audit/events/trace/:trace_id",
            get(v2::list_events_by_trace_v2),
        )
        .with_state(state)
}
