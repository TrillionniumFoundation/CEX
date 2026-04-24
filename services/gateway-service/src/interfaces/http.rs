use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use shared_types::{ApiKeyResolveRequest, ExecutionStatus};
use uuid::Uuid;

use crate::{
    application::invocation_service,
    domain::invocation::CreateInvocationBody,
    infrastructure::{
        clients::ServiceCallError,
        state::{
            AppState, GatewayAlertSignal, GatewayOperatorSignals, GatewayRuntimeMetricsSnapshot,
        },
    },
};

#[derive(serde::Serialize)]
struct GatewayInfo {
    service: &'static str,
    metrics: GatewayRuntimeMetricsSnapshot,
    operator_signals: GatewayOperatorSignals,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/info", get(info))
        .route("/v1/invocations", post(create_invocation))
        .route("/v1/invocations/:id", get(get_invocation))
        .route("/v1/executions/:id/approve", post(approve_execution))
        .route("/v1/executions/:id/retry", post(retry_execution))
        .route("/v1/executions/:id/cancel", post(cancel_execution))
        .with_state(state)
}

async fn health() -> &'static str {
    "gateway-service ok"
}

fn build_gateway_operator_signals(state: &AppState) -> GatewayOperatorSignals {
    let metrics = state.metrics.snapshot();
    GatewayOperatorSignals {
        invocation_create_upstream_failures: GatewayAlertSignal {
            value: metrics.invocation_create_upstream_failures as usize,
            threshold: state.alert_gateway_upstream_failure_threshold,
            alert: (metrics.invocation_create_upstream_failures as usize)
                >= state.alert_gateway_upstream_failure_threshold,
        },
    }
}

async fn info(State(state): State<AppState>) -> Json<GatewayInfo> {
    Json(GatewayInfo {
        service: "gateway-service",
        metrics: state.metrics.snapshot(),
        operator_signals: build_gateway_operator_signals(&state),
    })
}

#[derive(Debug, Deserialize)]
struct ApproveExecutionRequest {
    pub approved_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RetryExecutionRequest {
    pub retried_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CancelExecutionRequest {
    pub cancelled_by: String,
    pub reason: String,
}

async fn create_invocation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateInvocationBody>,
) -> impl IntoResponse {
    state
        .metrics
        .invocation_create_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let Some(api_key) = extract_api_key(&headers) else {
        state
            .metrics
            .invocation_create_auth_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "missing api key" })),
        )
            .into_response();
    };

    let auth = match state
        .clients
        .resolve_api_key(&ApiKeyResolveRequest { api_key })
        .await
    {
        Ok(auth) => auth,
        Err(err) => {
            state
                .metrics
                .invocation_create_auth_failures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return map_auth_error(err);
        }
    };

    let mut capability = None;
    if let Some(capability_id) = body.capability_id.as_deref() {
        match state.clients.get_capability(capability_id).await {
            Ok(found) => {
                if !found.enabled {
                    state
                        .metrics
                        .invocation_create_capability_failures
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": "capability disabled",
                            "message": capability_id,
                        })),
                    )
                        .into_response();
                }
                capability = Some(found);
            }
            Err(err) => {
                state
                    .metrics
                    .invocation_create_capability_failures
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return match err.status {
                    Some(404) => (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": "invalid capability id",
                            "message": capability_id,
                        })),
                    )
                        .into_response(),
                    _ => (
                        StatusCode::BAD_GATEWAY,
                        Json(serde_json::json!({
                            "error": "capability lookup failed",
                            "message": err.message,
                        })),
                    )
                        .into_response(),
                };
            }
        }
    }

    let record = invocation_service::create_invocation(
        state.clone(),
        body.with_auth(&auth, capability.as_ref()),
    )
    .await;
    let status = match record.status {
        ExecutionStatus::Created | ExecutionStatus::Queued | ExecutionStatus::Succeeded => {
            StatusCode::CREATED
        }
        ExecutionStatus::PolicyCheckPending
        | ExecutionStatus::AwaitingApproval
        | ExecutionStatus::Approved
        | ExecutionStatus::Dispatching
        | ExecutionStatus::Running => StatusCode::ACCEPTED,
        ExecutionStatus::Refunded
        | ExecutionStatus::Failed
        | ExecutionStatus::Cancelled
        | ExecutionStatus::TimedOut => StatusCode::BAD_GATEWAY,
    };
    if status == StatusCode::BAD_GATEWAY {
        state
            .metrics
            .invocation_create_upstream_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    (status, Json(record)).into_response()
}

async fn get_invocation(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    state
        .metrics
        .invocation_get_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let Some(api_key) = extract_api_key(&headers) else {
        state
            .metrics
            .invocation_get_auth_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "missing api key" })),
        )
            .into_response();
    };

    let auth = match state
        .clients
        .resolve_api_key(&ApiKeyResolveRequest { api_key })
        .await
    {
        Ok(auth) => auth,
        Err(err) => {
            state
                .metrics
                .invocation_get_auth_failures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return map_auth_error(err);
        }
    };

    match invocation_service::get_invocation(state, id).await {
        Ok(Some(record)) => {
            if record.request.org_id != auth.org_id {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "api key not authorized for org",
                        "message": record.request.org_id,
                    })),
                )
                    .into_response();
            }
            (StatusCode::OK, Json(record)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "invocation not found" })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": "invocation lookup failed",
                "message": err,
            })),
        )
            .into_response(),
    }
}

async fn approve_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ApproveExecutionRequest>,
) -> impl IntoResponse {
    state
        .metrics
        .execution_approve_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let Some(admin_token) = extract_admin_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "missing admin token" })),
        )
            .into_response();
    };

    match state
        .clients
        .approve_execution(id, &req.approved_by, req.note.as_deref(), &admin_token)
        .await
    {
        Ok(record) => (StatusCode::OK, Json(record)).into_response(),
        Err(err) => map_exec_client_error(err),
    }
}

async fn retry_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RetryExecutionRequest>,
) -> impl IntoResponse {
    state
        .metrics
        .execution_retry_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let Some(admin_token) = extract_admin_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "missing admin token" })),
        )
            .into_response();
    };

    match state
        .clients
        .retry_execution(id, &req.retried_by, req.note.as_deref(), &admin_token)
        .await
    {
        Ok(record) => (StatusCode::OK, Json(record)).into_response(),
        Err(err) => map_exec_client_error(err),
    }
}

async fn cancel_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CancelExecutionRequest>,
) -> impl IntoResponse {
    state
        .metrics
        .execution_cancel_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let Some(admin_token) = extract_admin_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "missing admin token" })),
        )
            .into_response();
    };

    match state
        .clients
        .cancel_execution(id, &req.cancelled_by, &req.reason, &admin_token)
        .await
    {
        Ok(record) => (StatusCode::OK, Json(record)).into_response(),
        Err(err) => map_exec_client_error(err),
    }
}

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Some(value) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        let trimmed = value.trim();
        if let Some(rest) = trimmed.strip_prefix("Bearer ") {
            let token = rest.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }

    None
}

fn extract_admin_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("x-admin-token").and_then(|v| v.to_str().ok()) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Some(value) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        let trimmed = value.trim();
        if let Some(rest) = trimmed.strip_prefix("Bearer ") {
            let token = rest.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }

    None
}

fn map_auth_error(err: ServiceCallError) -> axum::response::Response {
    let status = match err.status {
        Some(401) | Some(403) => StatusCode::UNAUTHORIZED,
        Some(400) => StatusCode::BAD_REQUEST,
        _ => StatusCode::BAD_GATEWAY,
    };

    (
        status,
        Json(serde_json::json!({
            "error": "auth resolution failed",
            "message": err.message,
        })),
    )
        .into_response()
}

fn map_exec_client_error(err: ServiceCallError) -> axum::response::Response {
    let status = match err.status {
        Some(status) => StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
        None => StatusCode::BAD_GATEWAY,
    };

    (
        status,
        Json(serde_json::json!({
            "error": "execution control failed",
            "message": err.message,
        })),
    )
        .into_response()
}
