use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    AdminPrincipal,
};
use shared_types::{AuditEventCreateRequest, ExecutionDispatchMode, ExecutionStatus};
use sqlx::Row;
use std::collections::HashSet;
use uuid::Uuid;

use crate::{
    dispatch_policy::dispatch_mode_for_execution,
    providers::{build_provider_target, dispatch_via_provider, ProviderDispatchInput},
    state::{AppState, ExecutionRecord, ExecutionRuntimeMetricsSnapshot},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateExecutionRequest {
    pub invocation_id: Uuid,
    pub trace_id: Uuid,
    pub org_id: Option<String>,
    pub actor_id: Option<String>,
    pub capability_id: Option<String>,
    pub capability_provider: Option<String>,
    pub capability_provider_ref: Option<String>,
    pub prompt: String,
    pub reserve_amount: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveExecutionRequest {
    pub approved_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectExecutionRequest {
    pub rejected_by: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchExecutionRequest {
    pub dispatched_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartExecutionRequest {
    pub started_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessExecutionRequest {
    pub processed_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimNextExecutionRequest {
    pub claimed_by: String,
    pub note: Option<String>,
    pub lease_expired_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimExecutionBatchRequest {
    pub claimed_by: String,
    pub note: Option<String>,
    pub limit: Option<usize>,
    pub lease_expired_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaimExecutionBatchResponse {
    pub items: Vec<ExecutionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReclaimExpiredExecutionsRequest {
    pub reclaimed_by: String,
    pub note: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReclaimExpiredExecutionsResponse {
    pub items: Vec<ExecutionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutExpiredExecutionsRequest {
    pub timed_out_by: String,
    pub reason: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimeoutExpiredExecutionResult {
    pub record: ExecutionRecord,
    pub refund_applied: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimeoutExpiredExecutionsResponse {
    pub items: Vec<TimeoutExpiredExecutionResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenewExecutionLeaseRequest {
    pub worker_id: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequeueExecutionRequest {
    pub worker_id: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryExecutionRequest {
    pub retried_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelExecutionRequest {
    pub cancelled_by: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutExecutionRequest {
    pub timed_out_by: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SucceedExecutionRequest {
    pub settled_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailExecutionRequest {
    pub failed_by: String,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StoredInvocationRequest {
    pub account_id: Option<Uuid>,
    pub reserve_amount: Option<f64>,
    pub prompt: String,
}

#[derive(Debug, Clone)]
struct RefundingOutcome {
    pub record: ExecutionRecord,
    pub refund_applied: bool,
}

#[derive(Serialize)]
pub struct ExecutionPolicyInfo {
    pub approval_reserve_threshold: f64,
    pub hard_reject_reserve_threshold: Option<f64>,
    pub approval_sensitive_keywords: Vec<String>,
    pub block_keywords: Vec<String>,
    pub approval_capability_prefixes: Vec<String>,
    pub block_capability_prefixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionAlertSignal {
    pub value: usize,
    pub threshold: usize,
    pub alert: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionOperatorSignals {
    pub approval_backlog: ExecutionAlertSignal,
    pub queued_worker_lease_expired: ExecutionAlertSignal,
    pub queued_worker_retry_budget_exhausted: ExecutionAlertSignal,
    pub provider_failures: ExecutionAlertSignal,
    pub provider_billing_failures: ExecutionAlertSignal,
    pub provider_timeout_failures: ExecutionAlertSignal,
    pub audit_failures: ExecutionAlertSignal,
    pub refund_failures: ExecutionAlertSignal,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProviderFailureSummary {
    pub total: usize,
    pub billing: usize,
    pub timeout: usize,
    pub auth: usize,
    pub rate_limited: usize,
    pub unavailable: usize,
    pub unknown: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionRuntimeOverview {
    pub total: usize,
    pub created: usize,
    pub policy_check_pending: usize,
    pub awaiting_approval: usize,
    pub approved: usize,
    pub queued: usize,
    pub dispatching: usize,
    pub running: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub timed_out: usize,
    pub refunded: usize,
    pub queued_worker: WorkerQueueSummary,
    pub provider_failures: ProviderFailureSummary,
}

#[derive(Serialize)]
pub struct ExecutionInfo {
    pub service: &'static str,
    pub state_machine: &'static str,
    pub policy: ExecutionPolicyInfo,
    pub metrics: ExecutionRuntimeMetricsSnapshot,
    pub runtime: Option<ExecutionRuntimeOverview>,
    pub operator_signals: Option<ExecutionOperatorSignals>,
    pub runtime_error: Option<String>,
}

const DEFAULT_CLAIM_BATCH_LIMIT: usize = 10;
const MAX_CLAIM_BATCH_LIMIT: usize = 100;
const DEFAULT_WORKER_QUEUE_LIMIT: usize = 50;
const MAX_WORKER_QUEUE_LIMIT: usize = 200;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct WorkerQueueQuery {
    pub limit: Option<usize>,
    pub claimable_only: Option<bool>,
    pub lease_expired_only: Option<bool>,
    pub worker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkerQueueExecutionView {
    pub execution_id: Uuid,
    pub invocation_id: Uuid,
    pub org_id: Option<String>,
    pub status: ExecutionStatus,
    pub dispatch_mode: ExecutionDispatchMode,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub attempts_remaining: i32,
    pub retry_budget_exhausted: bool,
    pub worker_id: Option<String>,
    pub lease_expires_at: Option<chrono::DateTime<Utc>>,
    pub provider_target: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
    pub claimable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkerQueueSummary {
    pub total: usize,
    pub queued: usize,
    pub dispatching: usize,
    pub claimable: usize,
    pub lease_expired: usize,
    pub claimed_active: usize,
    pub retryable: usize,
    pub retry_budget_exhausted: usize,
    pub active_workers: usize,
}

pub async fn health() -> &'static str {
    "execution-service ok"
}

pub async fn execution_info(State(state): State<AppState>) -> Json<ExecutionInfo> {
    let runtime_summary = load_execution_runtime_overview(&state).await;
    let (runtime, operator_signals, runtime_error) = match runtime_summary {
        Ok(summary) => {
            let signals = build_execution_operator_signals(&state, &summary);
            (Some(summary), Some(signals), None)
        }
        Err(err) => (None, None, Some(err)),
    };

    Json(ExecutionInfo {
        service: "execution-service",
        state_machine: "created -> policy_check_pending -> awaiting_approval|queued -> dispatching -> running -> succeeded|failed|timed_out|cancelled with durable ledger settlement/refund",
        policy: ExecutionPolicyInfo {
            approval_reserve_threshold: state.approval_reserve_threshold,
            hard_reject_reserve_threshold: state.hard_reject_reserve_threshold,
            approval_sensitive_keywords: state.approval_sensitive_keywords.as_ref().clone(),
            block_keywords: state.block_keywords.as_ref().clone(),
            approval_capability_prefixes: state.approval_capability_prefixes.as_ref().clone(),
            block_capability_prefixes: state.block_capability_prefixes.as_ref().clone(),
        },
        metrics: state.metrics.snapshot(),
        runtime,
        operator_signals,
        runtime_error,
    })
}

pub async fn create_execution(
    State(state): State<AppState>,
    Json(req): Json<CreateExecutionRequest>,
) -> impl IntoResponse {
    state
        .metrics
        .create_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let now = Utc::now();
    let execution_id = Uuid::new_v4();
    let mut record = ExecutionRecord {
        execution_id,
        invocation_id: req.invocation_id,
        trace_id: req.trace_id,
        org_id: req.org_id.clone(),
        status: ExecutionStatus::PolicyCheckPending,
        provider_target: req
            .capability_provider
            .as_deref()
            .zip(req.capability_provider_ref.as_deref())
            .map(|(provider, provider_ref)| build_provider_target(provider, provider_ref)),
        dispatch_mode: ExecutionDispatchMode::Manual,
        attempt_count: 0,
        max_attempts: state.execution_default_max_attempts,
        worker_id: None,
        lease_expires_at: None,
        started_at: None,
        ended_at: None,
        result_payload: None,
        approval_required: false,
        policy_reason: None,
        approved_by: None,
        created_at: now,
        updated_at: now,
    };

    best_effort_audit(
        &state,
        AuditEventCreateRequest {
            trace_id: req.trace_id,
            org_id: req.org_id.clone(),
            actor_type: "execution-service".to_string(),
            actor_id: None,
            event_type: "execution.created".to_string(),
            payload: json!({
                "execution_id": execution_id,
                "invocation_id": req.invocation_id,
            }),
        },
    )
    .await;

    best_effort_audit(
        &state,
        AuditEventCreateRequest {
            trace_id: req.trace_id,
            org_id: req.org_id.clone(),
            actor_type: "execution-service".to_string(),
            actor_id: None,
            event_type: "execution.policy_check_started".to_string(),
            payload: json!({
                "execution_id": execution_id,
                "reserve_amount": req.reserve_amount,
                "capability_id": req.capability_id,
            }),
        },
    )
    .await;

    let policy = evaluate_policy(&req, &state);
    if policy.blocked {
        state
            .metrics
            .create_blocked_policy
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        best_effort_audit(
            &state,
            AuditEventCreateRequest {
                trace_id: req.trace_id,
                org_id: req.org_id.clone(),
                actor_type: "policy-engine".to_string(),
                actor_id: None,
                event_type: "execution.blocked_by_policy".to_string(),
                payload: json!({
                    "invocation_id": req.invocation_id,
                    "reason": policy.reason,
                    "capability_id": req.capability_id,
                    "reserve_amount": req.reserve_amount,
                }),
            },
        )
        .await;

        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "execution blocked by policy",
                "reason": policy.reason,
            })),
        )
            .into_response();
    }

    record.approval_required = policy.requires_approval;
    record.policy_reason = policy.reason.clone();
    record.updated_at = Utc::now();

    state.provider_inputs.write().await.insert(
        execution_id,
        ProviderDispatchInput {
            prompt: req.prompt.clone(),
        },
    );

    if policy.requires_approval {
        state
            .metrics
            .create_awaiting_approval
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        record.status = ExecutionStatus::AwaitingApproval;
        best_effort_audit(
            &state,
            AuditEventCreateRequest {
                trace_id: req.trace_id,
                org_id: req.org_id.clone(),
                actor_type: "policy-engine".to_string(),
                actor_id: None,
                event_type: "execution.awaiting_approval".to_string(),
                payload: json!({
                    "execution_id": execution_id,
                    "reason": policy.reason.clone(),
                }),
            },
        )
        .await;
    } else {
        state
            .metrics
            .create_auto_approved
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        record.status = ExecutionStatus::Queued;
        best_effort_audit(
            &state,
            AuditEventCreateRequest {
                trace_id: req.trace_id,
                org_id: req.org_id.clone(),
                actor_type: "policy-engine".to_string(),
                actor_id: None,
                event_type: "execution.auto_approved".to_string(),
                payload: json!({
                    "execution_id": execution_id,
                    "reason": policy.reason.clone(),
                }),
            },
        )
        .await;
    }

    record.dispatch_mode = dispatch_mode_for_execution(&record);
    record.max_attempts = default_max_attempts_for_record(&state, &record);

    match store_execution(&state, &record).await {
        Ok(()) => (StatusCode::CREATED, Json(record)).into_response(),
        Err(err) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": err })),
        )
            .into_response(),
    }
}

pub async fn get_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    match require_execution_access(
        &state,
        &headers,
        id,
        &["executions:read", "executions:manage"],
    )
    .await
    {
        Ok(record) => (StatusCode::OK, Json(record)).into_response(),
        Err(response) => response.into_response(),
    }
}

pub async fn approve_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ApproveExecutionRequest>,
) -> impl IntoResponse {
    state
        .metrics
        .approve_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Err(response) =
        require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        return response.into_response();
    }
    match approve_execution_inner(&state, id, &req).await {
        Ok(record) => {
            state
                .metrics
                .approve_successes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            best_effort_audit(
                &state,
                AuditEventCreateRequest {
                    trace_id: record.trace_id,
                    org_id: record.org_id.clone(),
                    actor_type: "approver".to_string(),
                    actor_id: Some(req.approved_by.clone()),
                    event_type: "execution.approved".to_string(),
                    payload: json!({
                        "execution_id": record.execution_id,
                        "note": req.note.clone(),
                    }),
                },
            )
            .await;
            (StatusCode::OK, Json(record)).into_response()
        }
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn reject_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RejectExecutionRequest>,
) -> impl IntoResponse {
    state
        .metrics
        .reject_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Err(response) =
        require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        return response.into_response();
    }
    match reject_execution_inner(&state, id, &req).await {
        Ok(outcome) => {
            state
                .metrics
                .reject_successes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            best_effort_audit(
                &state,
                AuditEventCreateRequest {
                    trace_id: outcome.record.trace_id,
                    org_id: outcome.record.org_id.clone(),
                    actor_type: "approver".to_string(),
                    actor_id: Some(req.rejected_by.clone()),
                    event_type: "execution.rejected".to_string(),
                    payload: json!({
                        "execution_id": outcome.record.execution_id,
                        "reason": req.reason.clone(),
                    }),
                },
            )
            .await;
            emit_refundish_invocation_audit(
                &state,
                outcome.record.trace_id,
                outcome.record.org_id.clone(),
                outcome.record.invocation_id,
                outcome.record.execution_id,
                Some(req.rejected_by.clone()),
                if outcome.refund_applied {
                    "invocation.refunded"
                } else {
                    "invocation.cancelled"
                },
                req.reason.clone(),
                outcome.refund_applied,
            )
            .await;
            (StatusCode::OK, Json(outcome.record)).into_response()
        }
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn dispatch_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DispatchExecutionRequest>,
) -> impl IntoResponse {
    if let Err(response) =
        require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        return response.into_response();
    }
    match dispatch_execution_inner(&state, id).await {
        Ok(record) => {
            emit_simple_transition_audits(
                &state,
                &record,
                Some(req.dispatched_by.clone()),
                "execution.dispatching",
                "invocation.dispatching",
                req.note.clone(),
            )
            .await;
            (StatusCode::OK, Json(record)).into_response()
        }
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn start_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<StartExecutionRequest>,
) -> impl IntoResponse {
    if let Err(response) =
        require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        return response.into_response();
    }
    match start_execution_inner(&state, id).await {
        Ok(record) => {
            let (execution_event, invocation_event) =
                execution_transition_event_types(&record.status);
            emit_simple_transition_audits(
                &state,
                &record,
                Some(req.started_by.clone()),
                execution_event,
                invocation_event,
                req.note.clone(),
            )
            .await;
            (StatusCode::OK, Json(record)).into_response()
        }
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn worker_queue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WorkerQueueQuery>,
) -> impl IntoResponse {
    let admin = match authorize_execution_admin(
        &state,
        &headers,
        &["executions:read", "executions:manage"],
    ) {
        Ok(admin) => admin,
        Err(response) => return response.into_response(),
    };

    match load_worker_queue(&state, &admin, &query).await {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
        Err(message) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn worker_queue_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let admin = match authorize_execution_admin(
        &state,
        &headers,
        &["executions:read", "executions:manage"],
    ) {
        Ok(admin) => admin,
        Err(response) => return response.into_response(),
    };

    match load_worker_queue_summary(&state, &admin).await {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(message) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn claim_next_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ClaimNextExecutionRequest>,
) -> impl IntoResponse {
    state
        .metrics
        .claim_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let admin = match authorize_execution_admin(&state, &headers, &["executions:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response.into_response(),
    };

    let lease_expired_only = req.lease_expired_only.unwrap_or(false);
    match claim_queued_worker_executions_inner(
        &state,
        &admin,
        &req.claimed_by,
        1,
        lease_expired_only,
    )
    .await
    {
        Ok(mut records) => {
            let Some(record) = records.pop() else {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "no queued worker execution available" })),
                )
                    .into_response();
            };
            state
                .metrics
                .claim_successes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            emit_simple_transition_audits(
                &state,
                &record,
                Some(req.claimed_by.clone()),
                "execution.dispatching",
                "invocation.dispatching",
                req.note.clone(),
            )
            .await;
            (StatusCode::OK, Json(record)).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn claim_execution_batch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ClaimExecutionBatchRequest>,
) -> impl IntoResponse {
    let admin = match authorize_execution_admin(&state, &headers, &["executions:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response.into_response(),
    };

    let limit = normalize_claim_batch_limit(req.limit);
    let lease_expired_only = req.lease_expired_only.unwrap_or(false);
    match claim_queued_worker_executions_inner(
        &state,
        &admin,
        &req.claimed_by,
        limit,
        lease_expired_only,
    )
    .await
    {
        Ok(records) => {
            for record in &records {
                emit_simple_transition_audits(
                    &state,
                    record,
                    Some(req.claimed_by.clone()),
                    "execution.dispatching",
                    "invocation.dispatching",
                    req.note.clone(),
                )
                .await;
            }
            (
                StatusCode::OK,
                Json(ClaimExecutionBatchResponse { items: records }),
            )
                .into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn reclaim_expired_executions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ReclaimExpiredExecutionsRequest>,
) -> impl IntoResponse {
    let admin = match authorize_execution_admin(&state, &headers, &["executions:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response.into_response(),
    };

    let limit = normalize_claim_batch_limit(req.limit);
    match reclaim_expired_executions_inner(&state, &admin, limit).await {
        Ok(records) => {
            for record in &records {
                emit_simple_transition_audits(
                    &state,
                    record,
                    Some(req.reclaimed_by.clone()),
                    "execution.reclaimed",
                    "invocation.queued",
                    req.note.clone(),
                )
                .await;
            }
            (
                StatusCode::OK,
                Json(ReclaimExpiredExecutionsResponse { items: records }),
            )
                .into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn timeout_expired_executions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<TimeoutExpiredExecutionsRequest>,
) -> impl IntoResponse {
    let admin = match authorize_execution_admin(&state, &headers, &["executions:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response.into_response(),
    };

    let limit = normalize_claim_batch_limit(req.limit);
    match timeout_expired_executions_inner(&state, &admin, &req.reason, limit).await {
        Ok(items) => {
            for item in &items {
                best_effort_audit(
                    &state,
                    AuditEventCreateRequest {
                        trace_id: item.record.trace_id,
                        org_id: item.record.org_id.clone(),
                        actor_type: "execution-service".to_string(),
                        actor_id: Some(req.timed_out_by.clone()),
                        event_type: "execution.timed_out".to_string(),
                        payload: json!({
                            "execution_id": item.record.execution_id,
                            "reason": req.reason.clone(),
                        }),
                    },
                )
                .await;
                emit_refundish_invocation_audit(
                    &state,
                    item.record.trace_id,
                    item.record.org_id.clone(),
                    item.record.invocation_id,
                    item.record.execution_id,
                    Some(req.timed_out_by.clone()),
                    if item.refund_applied {
                        "invocation.refunded"
                    } else {
                        "invocation.timed_out"
                    },
                    req.reason.clone(),
                    item.refund_applied,
                )
                .await;
            }
            (
                StatusCode::OK,
                Json(TimeoutExpiredExecutionsResponse { items }),
            )
                .into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn process_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ProcessExecutionRequest>,
) -> impl IntoResponse {
    let record = match require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        Ok(record) => record,
        Err(response) => return response.into_response(),
    };

    if !matches!(record.dispatch_mode, ExecutionDispatchMode::QueuedWorker) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "execution dispatch mode does not require worker processing",
                "dispatch_mode": record.dispatch_mode,
                "status": record.status,
            })),
        )
            .into_response();
    }

    if let Err(response) = require_worker_lease_holder(&record, &req.processed_by) {
        return response.into_response();
    }

    match start_execution_inner(&state, id).await {
        Ok(record) => {
            let (execution_event, invocation_event) =
                execution_transition_event_types(&record.status);
            emit_simple_transition_audits(
                &state,
                &record,
                Some(req.processed_by.clone()),
                execution_event,
                invocation_event,
                req.note.clone(),
            )
            .await;
            (StatusCode::OK, Json(record)).into_response()
        }
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn renew_execution_lease(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RenewExecutionLeaseRequest>,
) -> impl IntoResponse {
    let record = match require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        Ok(record) => record,
        Err(response) => return response.into_response(),
    };

    if !matches!(record.dispatch_mode, ExecutionDispatchMode::QueuedWorker) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "execution dispatch mode does not require worker lease renewal",
                "dispatch_mode": record.dispatch_mode,
                "status": record.status,
            })),
        )
            .into_response();
    }

    if !matches!(record.status, ExecutionStatus::Dispatching) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "execution is not currently claimed",
                "dispatch_mode": record.dispatch_mode,
                "status": record.status,
            })),
        )
            .into_response();
    }

    if let Err(response) = require_worker_lease_holder(&record, &req.worker_id) {
        return response.into_response();
    }

    match renew_execution_lease_inner(&state, id, &req.worker_id).await {
        Ok(record) => (StatusCode::OK, Json(record)).into_response(),
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn requeue_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RequeueExecutionRequest>,
) -> impl IntoResponse {
    if let Err(response) =
        require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        return response.into_response();
    }

    match requeue_execution_inner(&state, id, &req).await {
        Ok(record) => {
            emit_simple_transition_audits(
                &state,
                &record,
                Some(req.worker_id.clone()),
                "execution.requeued",
                "invocation.queued",
                req.note.clone(),
            )
            .await;
            (StatusCode::OK, Json(record)).into_response()
        }
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn retry_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RetryExecutionRequest>,
) -> impl IntoResponse {
    state
        .metrics
        .retry_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Err(response) =
        require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        return response.into_response();
    }

    match retry_execution_inner(&state, id, &req).await {
        Ok(record) => {
            emit_simple_transition_audits(
                &state,
                &record,
                Some(req.retried_by.clone()),
                "execution.retried",
                "invocation.queued",
                req.note.clone(),
            )
            .await;
            (StatusCode::OK, Json(record)).into_response()
        }
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn cancel_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CancelExecutionRequest>,
) -> impl IntoResponse {
    state
        .metrics
        .cancel_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Err(response) =
        require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        return response.into_response();
    }
    match cancel_execution_inner(&state, id, &req).await {
        Ok(outcome) => {
            best_effort_audit(
                &state,
                AuditEventCreateRequest {
                    trace_id: outcome.record.trace_id,
                    org_id: outcome.record.org_id.clone(),
                    actor_type: "execution-service".to_string(),
                    actor_id: Some(req.cancelled_by.clone()),
                    event_type: "execution.cancelled".to_string(),
                    payload: json!({
                        "execution_id": outcome.record.execution_id,
                        "reason": req.reason.clone(),
                    }),
                },
            )
            .await;
            emit_refundish_invocation_audit(
                &state,
                outcome.record.trace_id,
                outcome.record.org_id.clone(),
                outcome.record.invocation_id,
                outcome.record.execution_id,
                Some(req.cancelled_by.clone()),
                if outcome.refund_applied {
                    "invocation.refunded"
                } else {
                    "invocation.cancelled"
                },
                req.reason.clone(),
                outcome.refund_applied,
            )
            .await;
            (StatusCode::OK, Json(outcome.record)).into_response()
        }
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn timeout_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<TimeoutExecutionRequest>,
) -> impl IntoResponse {
    if let Err(response) =
        require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        return response.into_response();
    }
    match timeout_execution_inner(&state, id, &req).await {
        Ok(outcome) => {
            best_effort_audit(
                &state,
                AuditEventCreateRequest {
                    trace_id: outcome.record.trace_id,
                    org_id: outcome.record.org_id.clone(),
                    actor_type: "execution-service".to_string(),
                    actor_id: Some(req.timed_out_by.clone()),
                    event_type: "execution.timed_out".to_string(),
                    payload: json!({
                        "execution_id": outcome.record.execution_id,
                        "reason": req.reason.clone(),
                    }),
                },
            )
            .await;
            emit_refundish_invocation_audit(
                &state,
                outcome.record.trace_id,
                outcome.record.org_id.clone(),
                outcome.record.invocation_id,
                outcome.record.execution_id,
                Some(req.timed_out_by.clone()),
                if outcome.refund_applied {
                    "invocation.refunded"
                } else {
                    "invocation.timed_out"
                },
                req.reason.clone(),
                outcome.refund_applied,
            )
            .await;
            (StatusCode::OK, Json(outcome.record)).into_response()
        }
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn succeed_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SucceedExecutionRequest>,
) -> impl IntoResponse {
    if let Err(response) =
        require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        return response.into_response();
    }
    match succeed_execution_inner(&state, id, &req).await {
        Ok(record) => {
            best_effort_audit(
                &state,
                AuditEventCreateRequest {
                    trace_id: record.trace_id,
                    org_id: record.org_id.clone(),
                    actor_type: "execution-service".to_string(),
                    actor_id: Some(req.settled_by.clone()),
                    event_type: "execution.succeeded".to_string(),
                    payload: json!({
                        "execution_id": record.execution_id,
                        "note": req.note.clone(),
                    }),
                },
            )
            .await;
            best_effort_audit(
                &state,
                AuditEventCreateRequest {
                    trace_id: record.trace_id,
                    org_id: record.org_id.clone(),
                    actor_type: "execution-service".to_string(),
                    actor_id: Some(req.settled_by.clone()),
                    event_type: "invocation.settled".to_string(),
                    payload: json!({
                        "invocation_id": record.invocation_id,
                        "execution_id": record.execution_id,
                        "status": "Succeeded",
                    }),
                },
            )
            .await;
            (StatusCode::OK, Json(record)).into_response()
        }
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

pub async fn fail_execution(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<FailExecutionRequest>,
) -> impl IntoResponse {
    if let Err(response) =
        require_execution_access(&state, &headers, id, &["executions:manage"]).await
    {
        return response.into_response();
    }
    match fail_execution_inner(&state, id, &req).await {
        Ok(outcome) => {
            best_effort_audit(
                &state,
                AuditEventCreateRequest {
                    trace_id: outcome.record.trace_id,
                    org_id: outcome.record.org_id.clone(),
                    actor_type: "execution-service".to_string(),
                    actor_id: Some(req.failed_by.clone()),
                    event_type: "execution.failed".to_string(),
                    payload: json!({
                        "execution_id": outcome.record.execution_id,
                        "reason": req.reason.clone(),
                    }),
                },
            )
            .await;
            emit_refundish_invocation_audit(
                &state,
                outcome.record.trace_id,
                outcome.record.org_id.clone(),
                outcome.record.invocation_id,
                outcome.record.execution_id,
                Some(req.failed_by.clone()),
                if outcome.refund_applied {
                    "invocation.refunded"
                } else {
                    "invocation.failed"
                },
                req.reason.clone(),
                outcome.refund_applied,
            )
            .await;
            (StatusCode::OK, Json(outcome.record)).into_response()
        }
        Err(ApiError::NotFound(message)) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
        }
        Err(ApiError::Conflict(message, status)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        )
            .into_response(),
        Err(ApiError::Unavailable(message)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

fn normalize_claim_batch_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_CLAIM_BATCH_LIMIT)
        .clamp(1, MAX_CLAIM_BATCH_LIMIT)
}

async fn renew_execution_lease_inner(
    state: &AppState,
    id: Uuid,
    worker_id: &str,
) -> Result<ExecutionRecord, ApiError> {
    if let Some(pool) = &state.pool {
        return renew_execution_lease_in_db(
            pool,
            id,
            worker_id,
            state.execution_claim_lease_seconds,
        )
        .await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let mut map = state.executions.write().await;
    let Some(record) = map.get_mut(&id) else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };

    if !matches!(record.dispatch_mode, ExecutionDispatchMode::QueuedWorker) {
        return Err(conflict_error(
            "execution dispatch mode does not require worker lease renewal",
            &record.status,
        ));
    }

    if !matches!(record.status, ExecutionStatus::Dispatching) {
        return Err(conflict_error(
            "execution is not currently claimed",
            &record.status,
        ));
    }

    validate_worker_lease_holder(record, worker_id)?;

    renew_worker_lease(record, state.execution_claim_lease_seconds);
    Ok(record.clone())
}

async fn claim_queued_worker_executions_inner(
    state: &AppState,
    admin: &AdminPrincipal,
    claimed_by: &str,
    limit: usize,
    lease_expired_only: bool,
) -> Result<Vec<ExecutionRecord>, ApiError> {
    if let Some(pool) = &state.pool {
        return claim_queued_worker_executions_in_db(
            pool,
            admin,
            claimed_by,
            state.execution_claim_lease_seconds,
            limit,
            lease_expired_only,
        )
        .await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let now = Utc::now();
    let candidate_ids = {
        let map = state.executions.read().await;
        let mut candidates = map
            .values()
            .filter(|record| worker_queue_visible_to_admin(admin, record))
            .filter(|record| should_include_claim_candidate(record, now, lease_expired_only))
            .map(|record| (record.execution_id, record.created_at))
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(_, created_at)| *created_at);
        candidates
            .into_iter()
            .take(limit)
            .map(|(execution_id, _)| execution_id)
            .collect::<Vec<_>>()
    };

    if candidate_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut claimed = Vec::new();
    let mut map = state.executions.write().await;
    for execution_id in candidate_ids {
        let now = Utc::now();
        let Some(record) = map.get_mut(&execution_id) else {
            continue;
        };
        if !worker_queue_visible_to_admin(admin, record)
            || !should_include_claim_candidate(record, now, lease_expired_only)
        {
            continue;
        }
        match ensure_transition_allowed(
            ExecutionTransitionKind::Dispatch,
            &record.status,
            "execution cannot transition from current status",
        )? {
            TransitionDecision::Replay | TransitionDecision::Advance => {
                apply_worker_claim(record, claimed_by, state.execution_claim_lease_seconds);
                claimed.push(record.clone());
            }
        }
    }

    Ok(claimed)
}

async fn store_execution(state: &AppState, record: &ExecutionRecord) -> Result<(), String> {
    if let Some(pool) = &state.pool {
        return upsert_execution(pool, record).await;
    }

    if state.fail_fast {
        return Err("execution postgres pool not initialized".to_string());
    }

    state
        .executions
        .write()
        .await
        .insert(record.execution_id, record.clone());
    Ok(())
}

async fn require_execution_access(
    state: &AppState,
    headers: &HeaderMap,
    id: Uuid,
    required_scopes: &[&str],
) -> Result<ExecutionRecord, (StatusCode, Json<serde_json::Value>)> {
    let admin = authorize_execution_admin(state, headers, required_scopes)?;

    match load_execution(state, id).await {
        Ok(Some(record)) => {
            enforce_execution_org_boundary(&admin, &record)?;
            Ok(record)
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "execution not found" })),
        )),
        Err(err) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": err })),
        )),
    }
}

fn authorize_execution_admin(
    state: &AppState,
    headers: &HeaderMap,
    required_scopes: &[&str],
) -> Result<AdminPrincipal, (StatusCode, Json<serde_json::Value>)> {
    authorize_scoped_admin_from_map(
        headers,
        &state.admin_tokens,
        required_scopes,
        |admin, scope| admin_principal_has_scope(admin, scope),
        "execution admin token not configured",
        Some(
            "set EXECUTION_ADMIN_TOKENS_JSON, EXECUTION_ADMIN_TOKEN, or the shared identity admin token env to enable execution admin access",
        ),
    )
    .cloned()
    .map_err(|err: shared_config::AdminAuthorizationFailure| err.into_response())
}

fn enforce_execution_org_boundary(
    admin: &AdminPrincipal,
    record: &ExecutionRecord,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if admin.org_ids.is_empty() {
        return Ok(());
    }

    let Some(org_id) = record.org_id.as_deref() else {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "execution missing org boundary metadata" })),
        ));
    };

    if admin_principal_allows_org(admin, org_id) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "admin token not authorized for org",
                "message": org_id,
            })),
        ))
    }
}

fn normalize_worker_queue_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_WORKER_QUEUE_LIMIT)
        .clamp(1, MAX_WORKER_QUEUE_LIMIT)
}

async fn load_visible_worker_queue_records(
    state: &AppState,
    admin: &AdminPrincipal,
) -> Result<Vec<ExecutionRecord>, String> {
    let mut records = if let Some(pool) = &state.pool {
        load_worker_queue_from_db(pool).await?
    } else {
        if state.fail_fast {
            return Err("execution postgres pool not initialized".to_string());
        }

        let mut records = state
            .executions
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by_key(|record| record.created_at);
        records
    };

    records.retain(|record| worker_queue_visible_to_admin(admin, record));
    Ok(records)
}

async fn load_all_execution_records(state: &AppState) -> Result<Vec<ExecutionRecord>, String> {
    if let Some(pool) = &state.pool {
        load_all_executions_from_db(pool).await
    } else {
        if state.fail_fast {
            return Err("execution postgres pool not initialized".to_string());
        }

        let mut records = state
            .executions
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by_key(|record| record.created_at);
        Ok(records)
    }
}

async fn load_execution_runtime_overview(
    state: &AppState,
) -> Result<ExecutionRuntimeOverview, String> {
    let records = load_all_execution_records(state).await?;
    Ok(build_execution_runtime_overview(records))
}

fn build_execution_alert_signal(value: usize, threshold: usize) -> ExecutionAlertSignal {
    ExecutionAlertSignal {
        value,
        threshold,
        alert: value >= threshold,
    }
}

fn build_execution_operator_signals(
    state: &AppState,
    overview: &ExecutionRuntimeOverview,
) -> ExecutionOperatorSignals {
    let metrics = state.metrics.snapshot();
    ExecutionOperatorSignals {
        approval_backlog: build_execution_alert_signal(
            overview.awaiting_approval,
            state.alert_approval_backlog_threshold,
        ),
        queued_worker_lease_expired: build_execution_alert_signal(
            overview.queued_worker.lease_expired,
            state.alert_lease_expired_threshold,
        ),
        queued_worker_retry_budget_exhausted: build_execution_alert_signal(
            overview.queued_worker.retry_budget_exhausted,
            state.alert_retry_budget_exhausted_threshold,
        ),
        provider_failures: build_execution_alert_signal(
            overview.provider_failures.total,
            state.alert_provider_failure_threshold,
        ),
        provider_billing_failures: build_execution_alert_signal(
            overview.provider_failures.billing,
            state.alert_provider_billing_failure_threshold,
        ),
        provider_timeout_failures: build_execution_alert_signal(
            overview.provider_failures.timeout,
            state.alert_provider_timeout_failure_threshold,
        ),
        audit_failures: build_execution_alert_signal(
            metrics.audit_failures as usize,
            state.alert_audit_failure_threshold,
        ),
        refund_failures: build_execution_alert_signal(
            metrics.refund_failures as usize,
            state.alert_refund_failure_threshold,
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderFailureKind {
    Billing,
    Timeout,
    Auth,
    RateLimited,
    Unavailable,
    Unknown,
}

fn classify_provider_failure(record: &ExecutionRecord) -> Option<ProviderFailureKind> {
    if record.provider_target.is_none()
        || !matches!(
            record.status,
            ExecutionStatus::Failed | ExecutionStatus::Refunded | ExecutionStatus::TimedOut
        )
    {
        return None;
    }

    let error = record
        .result_payload
        .as_ref()
        .and_then(|payload| payload.get("error"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    Some(classify_provider_error_text(error))
}

fn classify_provider_error_text(error: &str) -> ProviderFailureKind {
    let lowered = error.to_ascii_lowercase();
    if lowered.contains("insufficient balance")
        || lowered.contains("billing error")
        || lowered.contains("run out of credits")
        || lowered.contains("quota")
    {
        ProviderFailureKind::Billing
    } else if lowered.contains("timed out") || lowered.contains("timeout") {
        ProviderFailureKind::Timeout
    } else if lowered.contains("unauthorized")
        || lowered.contains("forbidden")
        || lowered.contains("invalid api key")
        || lowered.contains("auth error")
        || lowered.contains("authentication")
    {
        ProviderFailureKind::Auth
    } else if lowered.contains("rate limit")
        || lowered.contains("too many requests")
        || lowered.contains("429")
    {
        ProviderFailureKind::RateLimited
    } else if lowered.contains("unavailable")
        || lowered.contains("connection refused")
        || lowered.contains("connection reset")
        || lowered.contains("503")
        || lowered.contains("502")
        || lowered.contains("500")
    {
        ProviderFailureKind::Unavailable
    } else {
        ProviderFailureKind::Unknown
    }
}

fn build_execution_runtime_overview(records: Vec<ExecutionRecord>) -> ExecutionRuntimeOverview {
    let now = Utc::now();
    let mut queued_worker = WorkerQueueSummary {
        total: 0,
        queued: 0,
        dispatching: 0,
        claimable: 0,
        lease_expired: 0,
        claimed_active: 0,
        retryable: 0,
        retry_budget_exhausted: 0,
        active_workers: 0,
    };
    let mut active_workers = HashSet::new();
    let mut overview = ExecutionRuntimeOverview {
        total: 0,
        created: 0,
        policy_check_pending: 0,
        awaiting_approval: 0,
        approved: 0,
        queued: 0,
        dispatching: 0,
        running: 0,
        succeeded: 0,
        failed: 0,
        cancelled: 0,
        timed_out: 0,
        refunded: 0,
        queued_worker: WorkerQueueSummary {
            total: 0,
            queued: 0,
            dispatching: 0,
            claimable: 0,
            lease_expired: 0,
            claimed_active: 0,
            retryable: 0,
            retry_budget_exhausted: 0,
            active_workers: 0,
        },
        provider_failures: ProviderFailureSummary::default(),
    };

    for record in records {
        overview.total += 1;
        match record.status {
            ExecutionStatus::Created => overview.created += 1,
            ExecutionStatus::PolicyCheckPending => overview.policy_check_pending += 1,
            ExecutionStatus::AwaitingApproval => overview.awaiting_approval += 1,
            ExecutionStatus::Approved => overview.approved += 1,
            ExecutionStatus::Queued => overview.queued += 1,
            ExecutionStatus::Dispatching => overview.dispatching += 1,
            ExecutionStatus::Running => overview.running += 1,
            ExecutionStatus::Succeeded => overview.succeeded += 1,
            ExecutionStatus::Failed => overview.failed += 1,
            ExecutionStatus::Cancelled => overview.cancelled += 1,
            ExecutionStatus::TimedOut => overview.timed_out += 1,
            ExecutionStatus::Refunded => overview.refunded += 1,
        }

        if matches!(record.dispatch_mode, ExecutionDispatchMode::QueuedWorker) {
            queued_worker.total += 1;
            let claimable = is_claimable_queued_worker(&record, now);
            let lease_expired = is_expired_worker_lease(&record, now);
            let retry_budget_exhausted = !has_worker_attempt_budget_remaining(&record);
            if claimable {
                queued_worker.claimable += 1;
            }
            if lease_expired {
                queued_worker.lease_expired += 1;
            }
            if retry_budget_exhausted {
                queued_worker.retry_budget_exhausted += 1;
            } else {
                queued_worker.retryable += 1;
            }
            match record.status {
                ExecutionStatus::Queued => queued_worker.queued += 1,
                ExecutionStatus::Dispatching => {
                    queued_worker.dispatching += 1;
                    if !lease_expired {
                        queued_worker.claimed_active += 1;
                        if let Some(worker_id) = record.worker_id.as_deref() {
                            if !worker_id.trim().is_empty() {
                                active_workers.insert(worker_id.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(kind) = classify_provider_failure(&record) {
            overview.provider_failures.total += 1;
            match kind {
                ProviderFailureKind::Billing => overview.provider_failures.billing += 1,
                ProviderFailureKind::Timeout => overview.provider_failures.timeout += 1,
                ProviderFailureKind::Auth => overview.provider_failures.auth += 1,
                ProviderFailureKind::RateLimited => overview.provider_failures.rate_limited += 1,
                ProviderFailureKind::Unavailable => overview.provider_failures.unavailable += 1,
                ProviderFailureKind::Unknown => overview.provider_failures.unknown += 1,
            }
        }
    }

    queued_worker.active_workers = active_workers.len();
    overview.queued_worker = queued_worker;
    overview
}

async fn load_worker_queue_summary(
    state: &AppState,
    admin: &AdminPrincipal,
) -> Result<WorkerQueueSummary, String> {
    let now = Utc::now();
    let records = load_visible_worker_queue_records(state, admin).await?;
    let mut summary = WorkerQueueSummary {
        total: 0,
        queued: 0,
        dispatching: 0,
        claimable: 0,
        lease_expired: 0,
        claimed_active: 0,
        retryable: 0,
        retry_budget_exhausted: 0,
        active_workers: 0,
    };
    let mut active_workers = HashSet::new();

    for record in records
        .into_iter()
        .filter(|record| matches!(record.dispatch_mode, ExecutionDispatchMode::QueuedWorker))
    {
        summary.total += 1;
        let claimable = is_claimable_queued_worker(&record, now);
        let lease_expired = is_expired_worker_lease(&record, now);
        let retry_budget_exhausted = !has_worker_attempt_budget_remaining(&record);
        if claimable {
            summary.claimable += 1;
        }
        if lease_expired {
            summary.lease_expired += 1;
        }
        if retry_budget_exhausted {
            summary.retry_budget_exhausted += 1;
        } else {
            summary.retryable += 1;
        }
        match record.status {
            ExecutionStatus::Queued => summary.queued += 1,
            ExecutionStatus::Dispatching => {
                summary.dispatching += 1;
                if !lease_expired {
                    summary.claimed_active += 1;
                    if let Some(worker_id) = record.worker_id.as_deref() {
                        if !worker_id.trim().is_empty() {
                            active_workers.insert(worker_id.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    summary.active_workers = active_workers.len();
    Ok(summary)
}

async fn load_worker_queue(
    state: &AppState,
    admin: &AdminPrincipal,
    query: &WorkerQueueQuery,
) -> Result<Vec<WorkerQueueExecutionView>, String> {
    let now = Utc::now();
    let limit = normalize_worker_queue_limit(query.limit);
    let claimable_only = query.claimable_only.unwrap_or(false);
    let lease_expired_only = query.lease_expired_only.unwrap_or(false);
    let worker_id_filter = query
        .worker_id
        .as_deref()
        .map(str::trim)
        .filter(|worker_id| !worker_id.is_empty());

    let records = load_visible_worker_queue_records(state, admin).await?;
    Ok(records
        .into_iter()
        .filter(|record| matches!(record.dispatch_mode, ExecutionDispatchMode::QueuedWorker))
        .filter_map(|record| {
            let claimable = is_claimable_queued_worker(&record, now);
            let lease_expired = is_expired_worker_lease(&record, now);
            if claimable_only && !claimable {
                return None;
            }
            if lease_expired_only && !lease_expired {
                return None;
            }
            if let Some(worker_id) = worker_id_filter {
                if record.worker_id.as_deref() != Some(worker_id) {
                    return None;
                }
            }
            let attempts_remaining = remaining_attempts(&record);
            Some(WorkerQueueExecutionView {
                execution_id: record.execution_id,
                invocation_id: record.invocation_id,
                org_id: record.org_id,
                status: record.status,
                dispatch_mode: record.dispatch_mode,
                attempt_count: record.attempt_count,
                max_attempts: record.max_attempts,
                attempts_remaining,
                retry_budget_exhausted: attempts_remaining == 0,
                worker_id: record.worker_id,
                lease_expires_at: record.lease_expires_at,
                provider_target: record.provider_target,
                created_at: record.created_at,
                updated_at: record.updated_at,
                claimable,
            })
        })
        .take(limit)
        .collect())
}

async fn load_execution(state: &AppState, id: Uuid) -> Result<Option<ExecutionRecord>, String> {
    if let Some(pool) = &state.pool {
        return load_execution_from_db(pool, id).await;
    }

    if state.fail_fast {
        return Err("execution postgres pool not initialized".to_string());
    }

    Ok(state.executions.read().await.get(&id).cloned())
}

async fn upsert_execution(pool: &sqlx::PgPool, record: &ExecutionRecord) -> Result<(), String> {
    let result_payload = record
        .result_payload
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| format!("serialize execution result payload failed: {e}"))?;

    sqlx::query(
        "insert into executions (execution_id, invocation_id, status, provider_target, attempt_count, max_attempts, worker_id, lease_expires_at, started_at, ended_at, result_payload, trace_id, org_id, created_at, approval_required, policy_reason, approved_by, updated_at) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb, $12, $13::uuid, $14, $15, $16, $17, $18) on conflict (execution_id) do update set invocation_id = excluded.invocation_id, status = excluded.status, provider_target = excluded.provider_target, attempt_count = excluded.attempt_count, max_attempts = excluded.max_attempts, worker_id = excluded.worker_id, lease_expires_at = excluded.lease_expires_at, started_at = excluded.started_at, ended_at = excluded.ended_at, result_payload = excluded.result_payload, trace_id = excluded.trace_id, org_id = excluded.org_id, approval_required = excluded.approval_required, policy_reason = excluded.policy_reason, approved_by = excluded.approved_by, updated_at = excluded.updated_at"
    )
    .bind(record.execution_id)
    .bind(record.invocation_id)
    .bind(status_to_db(&record.status))
    .bind(&record.provider_target)
    .bind(record.attempt_count)
    .bind(record.max_attempts)
    .bind(&record.worker_id)
    .bind(record.lease_expires_at)
    .bind(record.started_at)
    .bind(record.ended_at)
    .bind(result_payload)
    .bind(record.trace_id)
    .bind(&record.org_id)
    .bind(record.created_at)
    .bind(record.approval_required)
    .bind(&record.policy_reason)
    .bind(&record.approved_by)
    .bind(record.updated_at)
    .execute(pool)
    .await
    .map_err(|e| format!("upsert execution failed: {e}"))?;

    if matches!(record.status, ExecutionStatus::AwaitingApproval) {
        let payload = serde_json::to_string(&json!({ "policy_reason": record.policy_reason }))
            .map_err(|e| format!("serialize approval payload failed: {e}"))?;
        sqlx::query(
            "insert into approvals (approval_id, execution_id, status, requested_to, requested_at, resolved_at, resolution, resolver_id, resolution_payload, updated_at) values (gen_random_uuid(), $1, 'pending', null, $2, null, null, null, $3::jsonb, $2) on conflict (execution_id) do update set status = 'pending', requested_at = excluded.requested_at, resolved_at = null, resolution = null, resolver_id = null, resolution_payload = excluded.resolution_payload, updated_at = excluded.updated_at"
        )
        .bind(record.execution_id)
        .bind(record.updated_at)
        .bind(payload)
        .execute(pool)
        .await
        .map_err(|e| format!("upsert approval request failed: {e}"))?;
    }

    Ok(())
}

async fn claim_queued_worker_executions_in_db(
    pool: &sqlx::PgPool,
    admin: &AdminPrincipal,
    claimed_by: &str,
    lease_seconds: i64,
    limit: usize,
    lease_expired_only: bool,
) -> Result<Vec<ExecutionRecord>, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin claim tx failed: {e}")))?;

    let now = Utc::now();
    let rows = sqlx::query(
        "select execution_id, invocation_id, trace_id, org_id::text as org_id, status, provider_target, attempt_count, max_attempts, worker_id, lease_expires_at, started_at, ended_at, result_payload, approval_required, policy_reason, approved_by, created_at, updated_at from executions where status in ('Queued', 'Dispatching') order by created_at asc for update skip locked"
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("load queued executions for claim failed: {e}")))?;

    let mut claimed = Vec::new();
    for row in rows {
        let record = execution_from_row(&row)?;
        if !worker_queue_visible_to_admin(admin, &record)
            || !should_include_claim_candidate(&record, now, lease_expired_only)
        {
            continue;
        }

        let mut claimed_record = record;
        apply_worker_claim(&mut claimed_record, claimed_by, lease_seconds);

        sqlx::query("update executions set status = $2, attempt_count = $3, worker_id = $4, lease_expires_at = $5, updated_at = $6 where execution_id = $1")
            .bind(claimed_record.execution_id)
            .bind(status_to_db(&claimed_record.status))
            .bind(claimed_record.attempt_count)
            .bind(&claimed_record.worker_id)
            .bind(claimed_record.lease_expires_at)
            .bind(claimed_record.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Unavailable(format!("update claimed execution failed: {e}")))?;

        sqlx::query("update invocations set status = 'Dispatching', updated_at = $2, execution_id = $3 where invocation_id = $1")
            .bind(claimed_record.invocation_id)
            .bind(claimed_record.updated_at)
            .bind(claimed_record.execution_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Unavailable(format!("update invocation after claim failed: {e}")))?;

        claimed.push(claimed_record);
        if claimed.len() >= limit {
            break;
        }
    }

    if claimed.is_empty() {
        tx.rollback()
            .await
            .map_err(|e| ApiError::Unavailable(format!("rollback empty claim tx failed: {e}")))?;
        return Ok(Vec::new());
    }

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit claim tx failed: {e}")))?;

    Ok(claimed)
}

async fn timeout_expired_executions_inner(
    state: &AppState,
    admin: &AdminPrincipal,
    reason: &str,
    limit: usize,
) -> Result<Vec<TimeoutExpiredExecutionResult>, ApiError> {
    let now = Utc::now();
    let candidate_ids = load_visible_worker_queue_records(state, admin)
        .await
        .map_err(ApiError::Unavailable)?
        .into_iter()
        .filter(|record| is_expired_worker_lease(record, now))
        .map(|record| record.execution_id)
        .collect::<Vec<_>>();

    let mut results = Vec::new();
    for execution_id in candidate_ids {
        if results.len() >= limit {
            break;
        }
        if let Some(outcome) =
            timeout_expired_execution_if_eligible(state, execution_id, reason).await?
        {
            results.push(TimeoutExpiredExecutionResult {
                record: outcome.record,
                refund_applied: outcome.refund_applied,
            });
        }
    }

    Ok(results)
}

async fn timeout_expired_execution_if_eligible(
    state: &AppState,
    id: Uuid,
    reason: &str,
) -> Result<Option<RefundingOutcome>, ApiError> {
    if let Some(pool) = &state.pool {
        return timeout_expired_execution_if_eligible_in_db(state, pool, id, reason).await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let mut map = state.executions.write().await;
    let Some(record) = map.get_mut(&id) else {
        return Ok(None);
    };

    if !is_expired_worker_lease(record, Utc::now()) {
        return Ok(None);
    }

    apply_transition(record, ExecutionStatus::TimedOut);
    let _ = reason;
    Ok(Some(RefundingOutcome {
        record: record.clone(),
        refund_applied: false,
    }))
}

async fn timeout_expired_execution_if_eligible_in_db(
    state: &AppState,
    pool: &sqlx::PgPool,
    id: Uuid,
    reason: &str,
) -> Result<Option<RefundingOutcome>, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin timeout expired tx failed: {e}")))?;

    let row = lock_execution_row(&mut tx, id, "timeout-expired").await?;
    let mut record = execution_from_row(&row)?;
    if !is_expired_worker_lease(&record, Utc::now()) {
        tx.rollback().await.map_err(|e| {
            ApiError::Unavailable(format!("rollback skipped timeout expired tx failed: {e}"))
        })?;
        return Ok(None);
    }

    let invocation_state = load_locked_invocation_state(&mut tx, record.invocation_id).await?;
    let refund_applied = if invocation_state.ledger_reserved && !invocation_state.ledger_refunded {
        release_reserved_credits(
            state,
            &invocation_state.request,
            record.execution_id,
            record.invocation_id,
            "timeout-expired-refund",
        )
        .await?
    } else {
        false
    };

    apply_transition(&mut record, ExecutionStatus::TimedOut);

    sqlx::query("update executions set status = 'TimedOut', worker_id = $2, lease_expires_at = $3, updated_at = $4 where execution_id = $1")
        .bind(record.execution_id)
        .bind(&record.worker_id)
        .bind(record.lease_expires_at)
        .bind(record.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update execution timeout expired failed: {e}")))?;

    update_invocation_after_refunding_action(
        &mut tx,
        record.invocation_id,
        record.execution_id,
        record.updated_at,
        if refund_applied {
            "Refunded"
        } else {
            "TimedOut"
        },
        false,
        refund_applied,
        &Some(reason.to_string()),
    )
    .await?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit timeout expired tx failed: {e}")))?;

    Ok(Some(RefundingOutcome {
        record,
        refund_applied,
    }))
}

async fn reclaim_expired_executions_inner(
    state: &AppState,
    admin: &AdminPrincipal,
    limit: usize,
) -> Result<Vec<ExecutionRecord>, ApiError> {
    if let Some(pool) = &state.pool {
        return reclaim_expired_executions_in_db(pool, admin, limit).await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let now = Utc::now();
    let candidate_ids = {
        let map = state.executions.read().await;
        let mut candidates = map
            .values()
            .filter(|record| worker_queue_visible_to_admin(admin, record))
            .filter(|record| is_expired_worker_lease(record, now))
            .filter(|record| has_worker_attempt_budget_remaining(record))
            .map(|record| (record.execution_id, record.created_at))
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(_, created_at)| *created_at);
        candidates
            .into_iter()
            .take(limit)
            .map(|(execution_id, _)| execution_id)
            .collect::<Vec<_>>()
    };

    if candidate_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut reclaimed = Vec::new();
    let mut map = state.executions.write().await;
    for execution_id in candidate_ids {
        let now = Utc::now();
        let Some(record) = map.get_mut(&execution_id) else {
            continue;
        };
        if !worker_queue_visible_to_admin(admin, record)
            || !is_expired_worker_lease(record, now)
            || !has_worker_attempt_budget_remaining(record)
        {
            continue;
        }
        apply_transition(record, ExecutionStatus::Queued);
        reclaimed.push(record.clone());
    }

    Ok(reclaimed)
}

async fn reclaim_expired_executions_in_db(
    pool: &sqlx::PgPool,
    admin: &AdminPrincipal,
    limit: usize,
) -> Result<Vec<ExecutionRecord>, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin reclaim expired tx failed: {e}")))?;

    let now = Utc::now();
    let rows = sqlx::query(
        "select execution_id, invocation_id, trace_id, org_id::text as org_id, status, provider_target, attempt_count, max_attempts, worker_id, lease_expires_at, started_at, ended_at, result_payload, approval_required, policy_reason, approved_by, created_at, updated_at from executions where status = 'Dispatching' order by created_at asc for update skip locked"
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("load expired executions for reclaim failed: {e}")))?;

    let mut reclaimed = Vec::new();
    for row in rows {
        let mut record = execution_from_row(&row)?;
        if !worker_queue_visible_to_admin(admin, &record)
            || !is_expired_worker_lease(&record, now)
            || !has_worker_attempt_budget_remaining(&record)
        {
            continue;
        }

        apply_transition(&mut record, ExecutionStatus::Queued);

        sqlx::query("update executions set status = $2, worker_id = $3, lease_expires_at = $4, updated_at = $5 where execution_id = $1")
            .bind(record.execution_id)
            .bind(status_to_db(&record.status))
            .bind(&record.worker_id)
            .bind(record.lease_expires_at)
            .bind(record.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Unavailable(format!("update reclaimed execution failed: {e}")))?;

        sqlx::query("update invocations set status = 'Queued', updated_at = $2, execution_id = $3, failure_reason = null where invocation_id = $1")
            .bind(record.invocation_id)
            .bind(record.updated_at)
            .bind(record.execution_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Unavailable(format!("update invocation after reclaim failed: {e}")))?;

        reclaimed.push(record);
        if reclaimed.len() >= limit {
            break;
        }
    }

    if reclaimed.is_empty() {
        tx.rollback()
            .await
            .map_err(|e| ApiError::Unavailable(format!("rollback empty reclaim tx failed: {e}")))?;
        return Ok(Vec::new());
    }

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit reclaim tx failed: {e}")))?;

    Ok(reclaimed)
}

async fn renew_execution_lease_in_db(
    pool: &sqlx::PgPool,
    id: Uuid,
    worker_id: &str,
    lease_seconds: i64,
) -> Result<ExecutionRecord, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin renew lease tx failed: {e}")))?;

    let row = lock_execution_row(&mut tx, id, "renew-lease").await?;
    let mut record = execution_from_row(&row)?;

    if !matches!(record.dispatch_mode, ExecutionDispatchMode::QueuedWorker) {
        return Err(conflict_error(
            "execution dispatch mode does not require worker lease renewal",
            &record.status,
        ));
    }

    if !matches!(record.status, ExecutionStatus::Dispatching) {
        return Err(conflict_error(
            "execution is not currently claimed",
            &record.status,
        ));
    }

    validate_worker_lease_holder(&record, worker_id)?;
    renew_worker_lease(&mut record, lease_seconds);
    record.dispatch_mode = dispatch_mode_for_execution(&record);

    sqlx::query("update executions set worker_id = $2, lease_expires_at = $3, updated_at = $4 where execution_id = $1")
        .bind(record.execution_id)
        .bind(&record.worker_id)
        .bind(record.lease_expires_at)
        .bind(record.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update execution lease failed: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit renew lease tx failed: {e}")))?;

    Ok(record)
}

async fn load_worker_queue_from_db(pool: &sqlx::PgPool) -> Result<Vec<ExecutionRecord>, String> {
    let rows = sqlx::query(
        "select execution_id, invocation_id, trace_id, org_id::text as org_id, status, provider_target, attempt_count, max_attempts, worker_id, lease_expires_at, started_at, ended_at, result_payload, approval_required, policy_reason, approved_by, created_at, updated_at from executions where status in ('Queued', 'Dispatching') order by created_at asc"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("load worker queue failed: {e}"))?;

    rows.into_iter()
        .map(|row| {
            execution_from_row(&row)
                .map_err(|err| format!("decode worker queue row failed: {err:?}"))
        })
        .collect()
}

async fn load_all_executions_from_db(pool: &sqlx::PgPool) -> Result<Vec<ExecutionRecord>, String> {
    let rows = sqlx::query(
        "select execution_id, invocation_id, trace_id, org_id::text as org_id, status, provider_target, attempt_count, max_attempts, worker_id, lease_expires_at, started_at, ended_at, result_payload, approval_required, policy_reason, approved_by, created_at, updated_at from executions order by created_at asc"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("load executions failed: {e}"))?;

    rows.into_iter()
        .map(|row| {
            execution_from_row(&row).map_err(|err| format!("decode execution row failed: {err:?}"))
        })
        .collect()
}

async fn load_execution_from_db(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<Option<ExecutionRecord>, String> {
    let row = sqlx::query(
        "select execution_id, invocation_id, trace_id, org_id::text as org_id, status, provider_target, attempt_count, max_attempts, worker_id, lease_expires_at, started_at, ended_at, result_payload, approval_required, policy_reason, approved_by, created_at, updated_at from executions where execution_id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("load execution failed: {e}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let status_raw: String = row
        .try_get("status")
        .map_err(|e| format!("read execution status failed: {e}"))?;

    let result_payload_raw: Option<Value> = row
        .try_get("result_payload")
        .map_err(|e| format!("read result_payload failed: {e}"))?;

    let mut record = ExecutionRecord {
        execution_id: row
            .try_get("execution_id")
            .map_err(|e| format!("read execution_id failed: {e}"))?,
        invocation_id: row
            .try_get("invocation_id")
            .map_err(|e| format!("read invocation_id failed: {e}"))?,
        trace_id: row
            .try_get("trace_id")
            .map_err(|e| format!("read trace_id failed: {e}"))?,
        org_id: row
            .try_get("org_id")
            .map_err(|e| format!("read org_id failed: {e}"))?,
        status: status_from_db(&status_raw)?,
        provider_target: row
            .try_get("provider_target")
            .map_err(|e| format!("read provider_target failed: {e}"))?,
        dispatch_mode: ExecutionDispatchMode::Manual,
        attempt_count: row
            .try_get("attempt_count")
            .map_err(|e| format!("read attempt_count failed: {e}"))?,
        max_attempts: row
            .try_get("max_attempts")
            .map_err(|e| format!("read max_attempts failed: {e}"))?,
        worker_id: row
            .try_get("worker_id")
            .map_err(|e| format!("read worker_id failed: {e}"))?,
        lease_expires_at: row
            .try_get("lease_expires_at")
            .map_err(|e| format!("read lease_expires_at failed: {e}"))?,
        started_at: row
            .try_get("started_at")
            .map_err(|e| format!("read started_at failed: {e}"))?,
        ended_at: row
            .try_get("ended_at")
            .map_err(|e| format!("read ended_at failed: {e}"))?,
        result_payload: result_payload_raw,
        approval_required: row
            .try_get("approval_required")
            .map_err(|e| format!("read approval_required failed: {e}"))?,
        policy_reason: row
            .try_get("policy_reason")
            .map_err(|e| format!("read policy_reason failed: {e}"))?,
        approved_by: row
            .try_get("approved_by")
            .map_err(|e| format!("read approved_by failed: {e}"))?,
        created_at: row
            .try_get("created_at")
            .map_err(|e| format!("read created_at failed: {e}"))?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|e| format!("read updated_at failed: {e}"))?,
    };
    record.dispatch_mode = dispatch_mode_for_execution(&record);
    Ok(Some(record))
}

async fn load_provider_dispatch_input_from_db(
    pool: &sqlx::PgPool,
    record: &ExecutionRecord,
) -> Result<ProviderDispatchInput, String> {
    let row = sqlx::query("select request_payload from invocations where invocation_id = $1")
        .bind(record.invocation_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("load invocation request payload failed: {e}"))?;

    let Some(row) = row else {
        return Err("invocation not found for execution provider dispatch".to_string());
    };

    let payload: Value = row
        .try_get("request_payload")
        .map_err(|e| format!("read invocation request payload failed: {e}"))?;
    let stored: StoredInvocationRequest = serde_json::from_value(payload)
        .map_err(|e| format!("decode invocation request payload failed: {e}"))?;

    Ok(ProviderDispatchInput {
        prompt: stored.prompt,
    })
}

async fn approve_execution_inner(
    state: &AppState,
    id: Uuid,
    req: &ApproveExecutionRequest,
) -> Result<ExecutionRecord, ApiError> {
    if let Some(pool) = &state.pool {
        return approve_execution_in_db(pool, id, req).await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let mut map = state.executions.write().await;
    let Some(record) = map.get_mut(&id) else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };
    if !matches!(record.status, ExecutionStatus::AwaitingApproval) {
        return Err(ApiError::Conflict(
            "execution is not awaiting approval".to_string(),
            record.status.clone(),
        ));
    }
    record.status = ExecutionStatus::Queued;
    record.approved_by = Some(req.approved_by.clone());
    record.updated_at = Utc::now();
    record.dispatch_mode = dispatch_mode_for_execution(record);
    Ok(record.clone())
}

async fn reject_execution_inner(
    state: &AppState,
    id: Uuid,
    req: &RejectExecutionRequest,
) -> Result<RefundingOutcome, ApiError> {
    if let Some(pool) = &state.pool {
        return reject_execution_in_db(state, pool, id, req).await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let mut map = state.executions.write().await;
    let Some(record) = map.get_mut(&id) else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };
    if !matches!(record.status, ExecutionStatus::AwaitingApproval) {
        return Err(ApiError::Conflict(
            "execution is not awaiting approval".to_string(),
            record.status.clone(),
        ));
    }
    record.status = ExecutionStatus::Cancelled;
    record.policy_reason = Some(req.reason.clone());
    record.updated_at = Utc::now();
    record.dispatch_mode = dispatch_mode_for_execution(record);
    Ok(RefundingOutcome {
        record: record.clone(),
        refund_applied: false,
    })
}

async fn dispatch_execution_inner(state: &AppState, id: Uuid) -> Result<ExecutionRecord, ApiError> {
    if let Some(pool) = &state.pool {
        return transition_execution_in_db(pool, id, ExecutionStatus::Dispatching, "Dispatching")
            .await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let mut map = state.executions.write().await;
    let Some(record) = map.get_mut(&id) else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };
    match ensure_transition_allowed(
        ExecutionTransitionKind::Dispatch,
        &record.status,
        "execution cannot be dispatched from current status",
    )? {
        TransitionDecision::Replay => Ok(record.clone()),
        TransitionDecision::Advance => {
            apply_transition(record, ExecutionStatus::Dispatching);
            Ok(record.clone())
        }
    }
}

async fn start_execution_inner(state: &AppState, id: Uuid) -> Result<ExecutionRecord, ApiError> {
    if let Some(pool) = &state.pool {
        let Some(record) = load_execution_from_db(pool, id)
            .await
            .map_err(ApiError::Unavailable)?
        else {
            return Err(ApiError::NotFound("execution not found".to_string()));
        };

        match ensure_transition_allowed(
            ExecutionTransitionKind::Start,
            &record.status,
            "execution cannot be started from current status",
        )? {
            TransitionDecision::Replay => Ok(record),
            TransitionDecision::Advance => start_execution_in_db(state, pool, id).await,
        }
    } else {
        if state.fail_fast {
            return Err(ApiError::Unavailable(
                "execution postgres pool not initialized".to_string(),
            ));
        }

        let provider_target = {
            let map = state.executions.read().await;
            let Some(record) = map.get(&id) else {
                return Err(ApiError::NotFound("execution not found".to_string()));
            };
            match ensure_transition_allowed(
                ExecutionTransitionKind::Start,
                &record.status,
                "execution cannot be started from current status",
            )? {
                TransitionDecision::Replay => return Ok(record.clone()),
                TransitionDecision::Advance => record.provider_target.clone(),
            }
        };

        if let Some(provider_target) = provider_target {
            let input = state
                .provider_inputs
                .read()
                .await
                .get(&id)
                .cloned()
                .ok_or_else(|| {
                    ApiError::Unavailable("provider dispatch input not found".to_string())
                })?;
            let started_at = Utc::now();
            let dispatch_result = dispatch_via_provider(
                &state.http,
                &state.ollama_base_url,
                &state.openclaw_cli_bin,
                &crate::providers::OpenClawCliEnvScope {
                    config_path: state.openclaw_config_path.clone(),
                    state_dir: state.openclaw_state_dir.clone(),
                    agent_dir: state.openclaw_agent_dir.clone(),
                },
                state.execution_provider_dispatch_timeout_seconds,
                &provider_target,
                &input,
            )
            .await;

            let mut map = state.executions.write().await;
            let record = map
                .get_mut(&id)
                .ok_or_else(|| ApiError::NotFound("execution not found".to_string()))?;
            record.started_at = Some(started_at);
            record.ended_at = Some(Utc::now());
            record.updated_at = Utc::now();

            match dispatch_result {
                Ok(output) => {
                    clear_worker_claim(record);
                    record.status = ExecutionStatus::Succeeded;
                    record.provider_target = Some(output.provider_target);
                    record.result_payload = Some(output.result_payload);
                    record.dispatch_mode = dispatch_mode_for_execution(record);
                }
                Err(err) => {
                    clear_worker_claim(record);
                    record.status = ExecutionStatus::Failed;
                    record.result_payload = Some(json!({
                        "provider_target": provider_target,
                        "error": err.message,
                    }));
                    record.dispatch_mode = dispatch_mode_for_execution(record);
                }
            }

            state.provider_inputs.write().await.remove(&id);
            return Ok(record.clone());
        }

        let mut map = state.executions.write().await;
        let Some(record) = map.get_mut(&id) else {
            return Err(ApiError::NotFound("execution not found".to_string()));
        };
        match ensure_transition_allowed(
            ExecutionTransitionKind::Start,
            &record.status,
            "execution cannot be started from current status",
        )? {
            TransitionDecision::Replay => Ok(record.clone()),
            TransitionDecision::Advance => {
                apply_transition(record, ExecutionStatus::Running);
                Ok(record.clone())
            }
        }
    }
}

async fn retry_execution_inner(
    state: &AppState,
    id: Uuid,
    req: &RetryExecutionRequest,
) -> Result<ExecutionRecord, ApiError> {
    if let Some(pool) = &state.pool {
        return retry_execution_in_db(pool, id, req).await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let mut map = state.executions.write().await;
    let Some(record) = map.get_mut(&id) else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };

    ensure_retryable_execution(record)?;
    prepare_execution_for_retry(record);
    let _ = req;
    Ok(record.clone())
}

async fn requeue_execution_inner(
    state: &AppState,
    id: Uuid,
    req: &RequeueExecutionRequest,
) -> Result<ExecutionRecord, ApiError> {
    if let Some(pool) = &state.pool {
        return requeue_execution_in_db(pool, id, req).await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let mut map = state.executions.write().await;
    let Some(record) = map.get_mut(&id) else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };

    if !matches!(record.dispatch_mode, ExecutionDispatchMode::QueuedWorker) {
        return Err(conflict_error(
            "execution dispatch mode does not require worker requeue",
            &record.status,
        ));
    }

    if !matches!(record.status, ExecutionStatus::Dispatching) {
        return Err(conflict_error(
            "execution is not currently claimed",
            &record.status,
        ));
    }

    validate_worker_lease_holder(record, &req.worker_id)?;
    if !has_worker_attempt_budget_remaining(record) {
        return Err(conflict_error(
            "execution retry budget exhausted",
            &record.status,
        ));
    }
    apply_transition(record, ExecutionStatus::Queued);
    Ok(record.clone())
}

async fn cancel_execution_inner(
    state: &AppState,
    id: Uuid,
    req: &CancelExecutionRequest,
) -> Result<RefundingOutcome, ApiError> {
    if let Some(pool) = &state.pool {
        return cancel_execution_in_db(state, pool, id, req).await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let mut map = state.executions.write().await;
    let Some(record) = map.get_mut(&id) else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };
    match ensure_transition_allowed(
        ExecutionTransitionKind::Cancel,
        &record.status,
        "execution cannot be cancelled from current status",
    )? {
        TransitionDecision::Replay => Ok(RefundingOutcome {
            record: record.clone(),
            refund_applied: false,
        }),
        TransitionDecision::Advance => {
            apply_transition(record, ExecutionStatus::Cancelled);
            Ok(RefundingOutcome {
                record: record.clone(),
                refund_applied: false,
            })
        }
    }
}

async fn timeout_execution_inner(
    state: &AppState,
    id: Uuid,
    req: &TimeoutExecutionRequest,
) -> Result<RefundingOutcome, ApiError> {
    if let Some(pool) = &state.pool {
        return timeout_execution_in_db(state, pool, id, req).await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let mut map = state.executions.write().await;
    let Some(record) = map.get_mut(&id) else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };
    match ensure_transition_allowed(
        ExecutionTransitionKind::Timeout,
        &record.status,
        "execution cannot be timed out from current status",
    )? {
        TransitionDecision::Replay => Ok(RefundingOutcome {
            record: record.clone(),
            refund_applied: false,
        }),
        TransitionDecision::Advance => {
            apply_transition(record, ExecutionStatus::TimedOut);
            Ok(RefundingOutcome {
                record: record.clone(),
                refund_applied: false,
            })
        }
    }
}

async fn succeed_execution_inner(
    state: &AppState,
    id: Uuid,
    _req: &SucceedExecutionRequest,
) -> Result<ExecutionRecord, ApiError> {
    if let Some(pool) = &state.pool {
        return succeed_execution_in_db(state, pool, id).await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let mut map = state.executions.write().await;
    let Some(record) = map.get_mut(&id) else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };
    match ensure_transition_allowed(
        ExecutionTransitionKind::Succeed,
        &record.status,
        "execution cannot be marked succeeded from current status",
    )? {
        TransitionDecision::Replay => Ok(record.clone()),
        TransitionDecision::Advance => {
            apply_transition(record, ExecutionStatus::Succeeded);
            Ok(record.clone())
        }
    }
}

async fn fail_execution_inner(
    state: &AppState,
    id: Uuid,
    req: &FailExecutionRequest,
) -> Result<RefundingOutcome, ApiError> {
    if let Some(pool) = &state.pool {
        return fail_execution_in_db(state, pool, id, req).await;
    }

    if state.fail_fast {
        return Err(ApiError::Unavailable(
            "execution postgres pool not initialized".to_string(),
        ));
    }

    let mut map = state.executions.write().await;
    let Some(record) = map.get_mut(&id) else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };
    match ensure_transition_allowed(
        ExecutionTransitionKind::Fail,
        &record.status,
        "execution cannot be marked failed from current status",
    )? {
        TransitionDecision::Replay => Ok(RefundingOutcome {
            record: record.clone(),
            refund_applied: false,
        }),
        TransitionDecision::Advance => {
            apply_transition(record, ExecutionStatus::Failed);
            Ok(RefundingOutcome {
                record: record.clone(),
                refund_applied: false,
            })
        }
    }
}

async fn approve_execution_in_db(
    pool: &sqlx::PgPool,
    id: Uuid,
    req: &ApproveExecutionRequest,
) -> Result<ExecutionRecord, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin approval tx failed: {e}")))?;

    let row = sqlx::query(
        "select execution_id, invocation_id, trace_id, org_id::text as org_id, status, provider_target, attempt_count, max_attempts, worker_id, lease_expires_at, started_at, ended_at, result_payload, approval_required, policy_reason, approved_by, created_at, updated_at from executions where execution_id = $1 for update"
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("load execution for approval failed: {e}")))?;

    let Some(row) = row else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };

    let mut record = execution_from_row(&row)?;
    if !matches!(record.status, ExecutionStatus::AwaitingApproval) {
        return Err(ApiError::Conflict(
            "execution is not awaiting approval".to_string(),
            record.status.clone(),
        ));
    }

    record.status = ExecutionStatus::Queued;
    record.approved_by = Some(req.approved_by.clone());
    record.updated_at = Utc::now();
    record.dispatch_mode = dispatch_mode_for_execution(&record);

    sqlx::query("update executions set status = $2, approved_by = $3, worker_id = $4, lease_expires_at = $5, updated_at = $6 where execution_id = $1")
        .bind(record.execution_id)
        .bind(status_to_db(&record.status))
        .bind(&record.approved_by)
        .bind(&record.worker_id)
        .bind(record.lease_expires_at)
        .bind(record.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update execution approval failed: {e}")))?;

    let resolution_payload =
        serde_json::to_string(&json!({ "note": req.note.clone() })).map_err(|e| {
            ApiError::Unavailable(format!("serialize approval resolution payload failed: {e}"))
        })?;
    sqlx::query(
        "insert into approvals (approval_id, execution_id, status, requested_to, requested_at, resolved_at, resolution, resolver_id, resolution_payload, updated_at) values (gen_random_uuid(), $1, 'approved', null, $2, $2, $3, $4, $5::jsonb, $2) on conflict (execution_id) do update set status = 'approved', resolved_at = excluded.resolved_at, resolution = excluded.resolution, resolver_id = excluded.resolver_id, resolution_payload = excluded.resolution_payload, updated_at = excluded.updated_at"
    )
    .bind(record.execution_id)
    .bind(record.updated_at)
    .bind(req.note.clone().unwrap_or_else(|| "approved".to_string()))
    .bind(&req.approved_by)
    .bind(resolution_payload)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("update approval row failed: {e}")))?;

    sqlx::query("update invocations set status = $2, updated_at = $3, approval_required = true, execution_id = $4 where invocation_id = $1")
        .bind(record.invocation_id)
        .bind(status_to_db(&record.status))
        .bind(record.updated_at)
        .bind(record.execution_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update invocation after approval failed: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit approval tx failed: {e}")))?;

    Ok(record)
}

async fn transition_execution_in_db(
    pool: &sqlx::PgPool,
    id: Uuid,
    to_status: ExecutionStatus,
    invocation_status: &str,
) -> Result<ExecutionRecord, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin transition tx failed: {e}")))?;

    let row = sqlx::query(
        "select execution_id, invocation_id, trace_id, org_id::text as org_id, status, provider_target, attempt_count, max_attempts, worker_id, lease_expires_at, started_at, ended_at, result_payload, approval_required, policy_reason, approved_by, created_at, updated_at from executions where execution_id = $1 for update"
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("load execution for transition failed: {e}")))?;

    let Some(row) = row else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };

    let mut record = execution_from_row(&row)?;
    match ensure_transition_allowed(
        ExecutionTransitionKind::Dispatch,
        &record.status,
        "execution cannot transition from current status",
    )? {
        TransitionDecision::Replay => return Ok(record),
        TransitionDecision::Advance => {
            apply_transition(&mut record, to_status);
        }
    }

    sqlx::query("update executions set status = $2, worker_id = $3, lease_expires_at = $4, updated_at = $5 where execution_id = $1")
        .bind(record.execution_id)
        .bind(status_to_db(&record.status))
        .bind(&record.worker_id)
        .bind(record.lease_expires_at)
        .bind(record.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update execution transition failed: {e}")))?;

    sqlx::query("update invocations set status = $2, updated_at = $3, execution_id = $4 where invocation_id = $1")
        .bind(record.invocation_id)
        .bind(invocation_status)
        .bind(record.updated_at)
        .bind(record.execution_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update invocation transition failed: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit transition tx failed: {e}")))?;

    Ok(record)
}

async fn start_execution_in_db(
    state: &AppState,
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<ExecutionRecord, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin start tx failed: {e}")))?;

    let row = sqlx::query(
        "select execution_id, invocation_id, trace_id, org_id::text as org_id, status, provider_target, attempt_count, max_attempts, worker_id, lease_expires_at, started_at, ended_at, result_payload, approval_required, policy_reason, approved_by, created_at, updated_at from executions where execution_id = $1 for update"
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("load execution for start failed: {e}")))?;

    let Some(row) = row else {
        return Err(ApiError::NotFound("execution not found".to_string()));
    };

    let mut record = execution_from_row(&row)?;
    match ensure_transition_allowed(
        ExecutionTransitionKind::Start,
        &record.status,
        "execution cannot be started from current status",
    )? {
        TransitionDecision::Replay => return Ok(record),
        TransitionDecision::Advance => {}
    }

    if let Some(provider_target) = record.provider_target.clone() {
        let invocation_state = load_locked_invocation_state(&mut tx, record.invocation_id).await?;
        let input = load_provider_dispatch_input_from_db(pool, &record)
            .await
            .map_err(ApiError::Unavailable)?;
        let started_at = Utc::now();
        let dispatch_result = dispatch_via_provider(
            &state.http,
            &state.ollama_base_url,
            &state.openclaw_cli_bin,
            &crate::providers::OpenClawCliEnvScope {
                config_path: state.openclaw_config_path.clone(),
                state_dir: state.openclaw_state_dir.clone(),
                agent_dir: state.openclaw_agent_dir.clone(),
            },
            state.execution_provider_dispatch_timeout_seconds,
            &provider_target,
            &input,
        )
        .await;

        match dispatch_result {
            Ok(output) => {
                if invocation_state.ledger_reserved && !invocation_state.ledger_refunded {
                    consume_reserved_credits(
                        state,
                        &invocation_state.request,
                        record.execution_id,
                        record.invocation_id,
                    )
                    .await?;
                }
                clear_worker_claim(&mut record);
                record.status = ExecutionStatus::Succeeded;
                record.provider_target = Some(output.provider_target);
                record.started_at = Some(started_at);
                record.ended_at = Some(Utc::now());
                record.result_payload = Some(output.result_payload);
                record.updated_at = Utc::now();
                record.dispatch_mode = dispatch_mode_for_execution(&record);

                sqlx::query("update executions set status = $2, provider_target = $3, worker_id = $4, lease_expires_at = $5, started_at = $6, ended_at = $7, result_payload = $8::jsonb, updated_at = $9 where execution_id = $1")
                    .bind(record.execution_id)
                    .bind(status_to_db(&record.status))
                    .bind(&record.provider_target)
                    .bind(&record.worker_id)
                    .bind(record.lease_expires_at)
                    .bind(record.started_at)
                    .bind(record.ended_at)
                    .bind(serde_json::to_string(record.result_payload.as_ref().expect("provider result payload")).map_err(|e| ApiError::Unavailable(format!("serialize provider result payload failed: {e}")))?)
                    .bind(record.updated_at)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| ApiError::Unavailable(format!("update provider execution success failed: {e}")))?;

                sqlx::query("update invocations set status = 'Succeeded', updated_at = $2, execution_id = $3, ledger_reserved = false, ledger_refunded = false, failure_reason = null where invocation_id = $1")
                    .bind(record.invocation_id)
                    .bind(record.updated_at)
                    .bind(record.execution_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| ApiError::Unavailable(format!("update invocation after provider success failed: {e}")))?;
            }
            Err(err) => {
                let refunded =
                    if invocation_state.ledger_reserved && !invocation_state.ledger_refunded {
                        release_reserved_credits(
                            state,
                            &invocation_state.request,
                            record.execution_id,
                            record.invocation_id,
                            "provider-start-refund",
                        )
                        .await?
                    } else {
                        false
                    };

                clear_worker_claim(&mut record);
                record.status = if refunded {
                    ExecutionStatus::Refunded
                } else {
                    ExecutionStatus::Failed
                };
                record.started_at = Some(started_at);
                record.ended_at = Some(Utc::now());
                record.result_payload = Some(json!({
                    "provider_target": provider_target,
                    "error": err.message,
                }));
                record.updated_at = Utc::now();
                record.dispatch_mode = dispatch_mode_for_execution(&record);

                sqlx::query("update executions set status = $2, worker_id = $3, lease_expires_at = $4, started_at = $5, ended_at = $6, result_payload = $7::jsonb, updated_at = $8 where execution_id = $1")
                    .bind(record.execution_id)
                    .bind(status_to_db(&record.status))
                    .bind(&record.worker_id)
                    .bind(record.lease_expires_at)
                    .bind(record.started_at)
                    .bind(record.ended_at)
                    .bind(serde_json::to_string(record.result_payload.as_ref().expect("provider error payload")).map_err(|e| ApiError::Unavailable(format!("serialize provider error payload failed: {e}")))?)
                    .bind(record.updated_at)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| ApiError::Unavailable(format!("update provider execution failure failed: {e}")))?;

                update_invocation_after_refunding_action(
                    &mut tx,
                    record.invocation_id,
                    record.execution_id,
                    record.updated_at,
                    if refunded { "Refunded" } else { "Failed" },
                    false,
                    refunded,
                    &Some(err.message),
                )
                .await?;
            }
        }

        tx.commit()
            .await
            .map_err(|e| ApiError::Unavailable(format!("commit provider start tx failed: {e}")))?;

        return Ok(record);
    }

    apply_transition(&mut record, ExecutionStatus::Running);

    sqlx::query("update executions set status = $2, worker_id = $3, lease_expires_at = $4, updated_at = $5 where execution_id = $1")
        .bind(record.execution_id)
        .bind(status_to_db(&record.status))
        .bind(&record.worker_id)
        .bind(record.lease_expires_at)
        .bind(record.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update execution start failed: {e}")))?;

    sqlx::query("update invocations set status = 'Running', updated_at = $2, execution_id = $3 where invocation_id = $1")
        .bind(record.invocation_id)
        .bind(record.updated_at)
        .bind(record.execution_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update invocation start failed: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit start tx failed: {e}")))?;

    Ok(record)
}

async fn reject_execution_in_db(
    state: &AppState,
    pool: &sqlx::PgPool,
    id: Uuid,
    req: &RejectExecutionRequest,
) -> Result<RefundingOutcome, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin reject tx failed: {e}")))?;

    let row = lock_execution_row(&mut tx, id, "reject").await?;
    let mut record = execution_from_row(&row)?;
    if !matches!(record.status, ExecutionStatus::AwaitingApproval) {
        return Err(ApiError::Conflict(
            "execution is not awaiting approval".to_string(),
            record.status.clone(),
        ));
    }

    let invocation_state = load_locked_invocation_state(&mut tx, record.invocation_id).await?;
    let refund_applied = if invocation_state.ledger_reserved && !invocation_state.ledger_refunded {
        release_reserved_credits(
            state,
            &invocation_state.request,
            record.execution_id,
            record.invocation_id,
            "reject-refund",
        )
        .await?
    } else {
        false
    };

    record.status = ExecutionStatus::Cancelled;
    record.policy_reason = Some(req.reason.clone());
    record.updated_at = Utc::now();
    record.dispatch_mode = dispatch_mode_for_execution(&record);

    sqlx::query("update executions set status = $2, policy_reason = $3, worker_id = $4, lease_expires_at = $5, updated_at = $6 where execution_id = $1")
        .bind(record.execution_id)
        .bind(status_to_db(&record.status))
        .bind(&record.policy_reason)
        .bind(&record.worker_id)
        .bind(record.lease_expires_at)
        .bind(record.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update execution reject failed: {e}")))?;

    let resolution_payload = serde_json::to_string(&json!({ "reason": req.reason.clone() }))
        .map_err(|e| ApiError::Unavailable(format!("serialize rejection payload failed: {e}")))?;
    sqlx::query(
        "insert into approvals (approval_id, execution_id, status, requested_to, requested_at, resolved_at, resolution, resolver_id, resolution_payload, updated_at) values (gen_random_uuid(), $1, 'rejected', null, $2, $2, $3, $4, $5::jsonb, $2) on conflict (execution_id) do update set status = 'rejected', resolved_at = excluded.resolved_at, resolution = excluded.resolution, resolver_id = excluded.resolver_id, resolution_payload = excluded.resolution_payload, updated_at = excluded.updated_at"
    )
    .bind(record.execution_id)
    .bind(record.updated_at)
    .bind(&req.reason)
    .bind(&req.rejected_by)
    .bind(resolution_payload)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("update rejection row failed: {e}")))?;

    update_invocation_after_refunding_action(
        &mut tx,
        record.invocation_id,
        record.execution_id,
        record.updated_at,
        if refund_applied {
            "Refunded"
        } else {
            "Cancelled"
        },
        false,
        refund_applied,
        &record.policy_reason,
    )
    .await?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit reject tx failed: {e}")))?;

    Ok(RefundingOutcome {
        record,
        refund_applied,
    })
}

async fn retry_execution_in_db(
    pool: &sqlx::PgPool,
    id: Uuid,
    req: &RetryExecutionRequest,
) -> Result<ExecutionRecord, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin retry tx failed: {e}")))?;

    let row = lock_execution_row(&mut tx, id, "retry").await?;
    let mut record = execution_from_row(&row)?;
    ensure_retryable_execution(&record)?;

    let invocation_state = load_locked_invocation_state(&mut tx, record.invocation_id).await?;
    if invocation_state.ledger_refunded {
        return Err(conflict_error(
            "execution cannot be retried after refund",
            &record.status,
        ));
    }

    prepare_execution_for_retry(&mut record);

    sqlx::query("update executions set status = $2, worker_id = $3, lease_expires_at = $4, started_at = $5, ended_at = $6, result_payload = $7::jsonb, updated_at = $8 where execution_id = $1")
        .bind(record.execution_id)
        .bind(status_to_db(&record.status))
        .bind(&record.worker_id)
        .bind(record.lease_expires_at)
        .bind(record.started_at)
        .bind(record.ended_at)
        .bind(Option::<Value>::None)
        .bind(record.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update execution retry failed: {e}")))?;

    sqlx::query("update invocations set status = 'Queued', updated_at = $2, execution_id = $3, ledger_reserved = false, ledger_refunded = false, failure_reason = null where invocation_id = $1")
        .bind(record.invocation_id)
        .bind(record.updated_at)
        .bind(record.execution_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update invocation after retry failed: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit retry tx failed: {e}")))?;

    let _ = req;
    Ok(record)
}

async fn requeue_execution_in_db(
    pool: &sqlx::PgPool,
    id: Uuid,
    req: &RequeueExecutionRequest,
) -> Result<ExecutionRecord, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin requeue tx failed: {e}")))?;

    let row = lock_execution_row(&mut tx, id, "requeue").await?;
    let mut record = execution_from_row(&row)?;

    if !matches!(record.dispatch_mode, ExecutionDispatchMode::QueuedWorker) {
        return Err(conflict_error(
            "execution dispatch mode does not require worker requeue",
            &record.status,
        ));
    }

    if !matches!(record.status, ExecutionStatus::Dispatching) {
        return Err(conflict_error(
            "execution is not currently claimed",
            &record.status,
        ));
    }

    validate_worker_lease_holder(&record, &req.worker_id)?;
    if !has_worker_attempt_budget_remaining(&record) {
        return Err(conflict_error(
            "execution retry budget exhausted",
            &record.status,
        ));
    }
    apply_transition(&mut record, ExecutionStatus::Queued);

    sqlx::query("update executions set status = $2, worker_id = $3, lease_expires_at = $4, updated_at = $5 where execution_id = $1")
        .bind(record.execution_id)
        .bind(status_to_db(&record.status))
        .bind(&record.worker_id)
        .bind(record.lease_expires_at)
        .bind(record.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update execution requeue failed: {e}")))?;

    sqlx::query("update invocations set status = 'Queued', updated_at = $2, execution_id = $3, failure_reason = null where invocation_id = $1")
        .bind(record.invocation_id)
        .bind(record.updated_at)
        .bind(record.execution_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update invocation after requeue failed: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit requeue tx failed: {e}")))?;

    Ok(record)
}

async fn cancel_execution_in_db(
    state: &AppState,
    pool: &sqlx::PgPool,
    id: Uuid,
    req: &CancelExecutionRequest,
) -> Result<RefundingOutcome, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin cancel tx failed: {e}")))?;

    let row = lock_execution_row(&mut tx, id, "cancel").await?;
    let mut record = execution_from_row(&row)?;
    let invocation_state = load_locked_invocation_state(&mut tx, record.invocation_id).await?;
    match ensure_transition_allowed(
        ExecutionTransitionKind::Cancel,
        &record.status,
        "execution cannot be cancelled from current status",
    )? {
        TransitionDecision::Replay => {
            return Ok(RefundingOutcome {
                record,
                refund_applied: invocation_state.ledger_refunded,
            })
        }
        TransitionDecision::Advance => {}
    }

    let refund_applied = if invocation_state.ledger_reserved && !invocation_state.ledger_refunded {
        release_reserved_credits(
            state,
            &invocation_state.request,
            record.execution_id,
            record.invocation_id,
            "cancel-refund",
        )
        .await?
    } else {
        false
    };

    apply_transition(&mut record, ExecutionStatus::Cancelled);

    sqlx::query(
        "update executions set status = 'Cancelled', updated_at = $2 where execution_id = $1",
    )
    .bind(record.execution_id)
    .bind(record.updated_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("update execution cancel failed: {e}")))?;

    update_invocation_after_refunding_action(
        &mut tx,
        record.invocation_id,
        record.execution_id,
        record.updated_at,
        if refund_applied {
            "Refunded"
        } else {
            "Cancelled"
        },
        false,
        refund_applied,
        &Some(req.reason.clone()),
    )
    .await?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit cancel tx failed: {e}")))?;

    Ok(RefundingOutcome {
        record,
        refund_applied,
    })
}

async fn timeout_execution_in_db(
    state: &AppState,
    pool: &sqlx::PgPool,
    id: Uuid,
    req: &TimeoutExecutionRequest,
) -> Result<RefundingOutcome, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin timeout tx failed: {e}")))?;

    let row = lock_execution_row(&mut tx, id, "timeout").await?;
    let mut record = execution_from_row(&row)?;
    let invocation_state = load_locked_invocation_state(&mut tx, record.invocation_id).await?;
    match ensure_transition_allowed(
        ExecutionTransitionKind::Timeout,
        &record.status,
        "execution cannot be timed out from current status",
    )? {
        TransitionDecision::Replay => {
            return Ok(RefundingOutcome {
                record,
                refund_applied: invocation_state.ledger_refunded,
            })
        }
        TransitionDecision::Advance => {}
    }

    let refund_applied = if invocation_state.ledger_reserved && !invocation_state.ledger_refunded {
        release_reserved_credits(
            state,
            &invocation_state.request,
            record.execution_id,
            record.invocation_id,
            "timeout-refund",
        )
        .await?
    } else {
        false
    };

    apply_transition(&mut record, ExecutionStatus::TimedOut);

    sqlx::query(
        "update executions set status = 'TimedOut', updated_at = $2 where execution_id = $1",
    )
    .bind(record.execution_id)
    .bind(record.updated_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("update execution timeout failed: {e}")))?;

    update_invocation_after_refunding_action(
        &mut tx,
        record.invocation_id,
        record.execution_id,
        record.updated_at,
        if refund_applied {
            "Refunded"
        } else {
            "TimedOut"
        },
        false,
        refund_applied,
        &Some(req.reason.clone()),
    )
    .await?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit timeout tx failed: {e}")))?;

    Ok(RefundingOutcome {
        record,
        refund_applied,
    })
}

async fn succeed_execution_in_db(
    state: &AppState,
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<ExecutionRecord, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin succeed tx failed: {e}")))?;

    let row = lock_execution_row(&mut tx, id, "succeed").await?;
    let mut record = execution_from_row(&row)?;
    match ensure_transition_allowed(
        ExecutionTransitionKind::Succeed,
        &record.status,
        "execution cannot be marked succeeded from current status",
    )? {
        TransitionDecision::Replay => return Ok(record),
        TransitionDecision::Advance => {}
    }

    let invocation_state = load_locked_invocation_state(&mut tx, record.invocation_id).await?;
    if invocation_state.ledger_reserved && !invocation_state.ledger_refunded {
        consume_reserved_credits(
            state,
            &invocation_state.request,
            record.execution_id,
            record.invocation_id,
        )
        .await?;
    }

    apply_transition(&mut record, ExecutionStatus::Succeeded);

    sqlx::query(
        "update executions set status = 'Succeeded', updated_at = $2 where execution_id = $1",
    )
    .bind(record.execution_id)
    .bind(record.updated_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("update execution succeed failed: {e}")))?;

    sqlx::query("update invocations set status = 'Succeeded', updated_at = $2, execution_id = $3, ledger_reserved = false, ledger_refunded = false, failure_reason = null where invocation_id = $1")
        .bind(record.invocation_id)
        .bind(record.updated_at)
        .bind(record.execution_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update invocation after succeed failed: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit succeed tx failed: {e}")))?;

    Ok(record)
}

async fn fail_execution_in_db(
    state: &AppState,
    pool: &sqlx::PgPool,
    id: Uuid,
    req: &FailExecutionRequest,
) -> Result<RefundingOutcome, ApiError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::Unavailable(format!("begin fail tx failed: {e}")))?;

    let row = lock_execution_row(&mut tx, id, "fail").await?;
    let mut record = execution_from_row(&row)?;
    let invocation_state = load_locked_invocation_state(&mut tx, record.invocation_id).await?;
    match ensure_transition_allowed(
        ExecutionTransitionKind::Fail,
        &record.status,
        "execution cannot be marked failed from current status",
    )? {
        TransitionDecision::Replay => {
            return Ok(RefundingOutcome {
                record,
                refund_applied: invocation_state.ledger_refunded,
            })
        }
        TransitionDecision::Advance => {}
    }

    let refund_applied = if invocation_state.ledger_reserved && !invocation_state.ledger_refunded {
        release_reserved_credits(
            state,
            &invocation_state.request,
            record.execution_id,
            record.invocation_id,
            "execution-fail-refund",
        )
        .await?
    } else {
        false
    };

    apply_transition(&mut record, ExecutionStatus::Failed);

    sqlx::query("update executions set status = 'Failed', updated_at = $2 where execution_id = $1")
        .bind(record.execution_id)
        .bind(record.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Unavailable(format!("update execution fail failed: {e}")))?;

    update_invocation_after_refunding_action(
        &mut tx,
        record.invocation_id,
        record.execution_id,
        record.updated_at,
        if refund_applied { "Refunded" } else { "Failed" },
        false,
        refund_applied,
        &Some(req.reason.clone()),
    )
    .await?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Unavailable(format!("commit fail tx failed: {e}")))?;

    Ok(RefundingOutcome {
        record,
        refund_applied,
    })
}

struct InvocationState {
    request: StoredInvocationRequest,
    ledger_reserved: bool,
    ledger_refunded: bool,
}

async fn lock_execution_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    label: &str,
) -> Result<sqlx::postgres::PgRow, ApiError> {
    let row = sqlx::query(
        "select execution_id, invocation_id, trace_id, org_id::text as org_id, status, provider_target, attempt_count, max_attempts, worker_id, lease_expires_at, started_at, ended_at, result_payload, approval_required, policy_reason, approved_by, created_at, updated_at from executions where execution_id = $1 for update"
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("load execution for {label} failed: {e}")))?;

    row.ok_or_else(|| ApiError::NotFound("execution not found".to_string()))
}

async fn load_locked_invocation_state(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invocation_id: Uuid,
) -> Result<InvocationState, ApiError> {
    let row = sqlx::query(
        "select request_payload::text as request_payload_text, ledger_reserved, ledger_refunded from invocations where invocation_id = $1 for update"
    )
    .bind(invocation_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("load invocation state failed: {e}")))?;

    let Some(row) = row else {
        return Err(ApiError::Unavailable(
            "invocation not found for execution transition".to_string(),
        ));
    };

    let request_payload_text: String = row.try_get("request_payload_text").map_err(|e| {
        ApiError::Unavailable(format!("read invocation request payload failed: {e}"))
    })?;
    let request: StoredInvocationRequest =
        serde_json::from_str(&request_payload_text).map_err(|e| {
            ApiError::Unavailable(format!(
                "decode invocation request failed: {e}; payload={request_payload_text}"
            ))
        })?;

    Ok(InvocationState {
        request,
        ledger_reserved: row
            .try_get("ledger_reserved")
            .map_err(|e| ApiError::Unavailable(format!("read ledger_reserved failed: {e}")))?,
        ledger_refunded: row
            .try_get("ledger_refunded")
            .map_err(|e| ApiError::Unavailable(format!("read ledger_refunded failed: {e}")))?,
    })
}

async fn update_invocation_after_refunding_action(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invocation_id: Uuid,
    execution_id: Uuid,
    updated_at: chrono::DateTime<chrono::Utc>,
    status: &str,
    ledger_reserved: bool,
    ledger_refunded: bool,
    failure_reason: &Option<String>,
) -> Result<(), ApiError> {
    sqlx::query(
        "update invocations set status = $2, updated_at = $3, execution_id = $4, ledger_reserved = $5, ledger_refunded = $6, failure_reason = $7 where invocation_id = $1"
    )
    .bind(invocation_id)
    .bind(status)
    .bind(updated_at)
    .bind(execution_id)
    .bind(ledger_reserved)
    .bind(ledger_refunded)
    .bind(failure_reason)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::Unavailable(format!("update invocation refunding action failed: {e}")))?;
    Ok(())
}

async fn consume_reserved_credits(
    state: &AppState,
    request: &StoredInvocationRequest,
    execution_id: Uuid,
    invocation_id: Uuid,
) -> Result<(), ApiError> {
    let Some(account_id) = request.account_id else {
        return Ok(());
    };
    let Some(amount) = request.reserve_amount else {
        return Ok(());
    };
    if amount <= 0.0 {
        return Ok(());
    }

    call_ledger_action(
        state,
        "consume",
        account_id,
        amount,
        invocation_id,
        format!("consume:{execution_id}"),
    )
    .await
}

async fn release_reserved_credits(
    state: &AppState,
    request: &StoredInvocationRequest,
    execution_id: Uuid,
    invocation_id: Uuid,
    idempotency_prefix: &str,
) -> Result<bool, ApiError> {
    let Some(account_id) = request.account_id else {
        return Ok(false);
    };
    let Some(amount) = request.reserve_amount else {
        return Ok(false);
    };
    if amount <= 0.0 {
        return Ok(false);
    }

    if let Err(err) = call_ledger_action(
        state,
        "refund",
        account_id,
        amount,
        invocation_id,
        format!("{idempotency_prefix}:{execution_id}"),
    )
    .await
    {
        state
            .metrics
            .refund_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return Err(err);
    }
    Ok(true)
}

async fn call_ledger_action(
    state: &AppState,
    action: &str,
    account_id: Uuid,
    amount: f64,
    invocation_id: Uuid,
    idempotency_key: String,
) -> Result<(), ApiError> {
    let response = state
        .http
        .post(format!("{}/v1/ledger/{action}", state.ledger_base_url))
        .header("x-admin-token", &state.ledger_manage_token)
        .json(&json!({
            "account_id": account_id,
            "amount": amount,
            "reference_id": invocation_id,
            "idempotency_key": idempotency_key,
        }))
        .send()
        .await
        .map_err(|e| ApiError::Unavailable(format!("ledger {action} request failed: {e}")))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| ApiError::Unavailable(format!("ledger {action} read body failed: {e}")))?;

    if status.is_success() {
        return Ok(());
    }

    if status.as_u16() == 409
        && body
            .to_ascii_lowercase()
            .contains("duplicate idempotency key")
    {
        return Ok(());
    }

    Err(ApiError::Unavailable(format!(
        "ledger {action} returned status {}: {body}",
        status.as_u16()
    )))
}

fn execution_from_row(row: &sqlx::postgres::PgRow) -> Result<ExecutionRecord, ApiError> {
    let status_raw: String = row
        .try_get("status")
        .map_err(|e| ApiError::Unavailable(format!("read execution status failed: {e}")))?;

    let mut record = ExecutionRecord {
        execution_id: row
            .try_get("execution_id")
            .map_err(|e| ApiError::Unavailable(format!("read execution_id failed: {e}")))?,
        invocation_id: row
            .try_get("invocation_id")
            .map_err(|e| ApiError::Unavailable(format!("read invocation_id failed: {e}")))?,
        trace_id: row
            .try_get("trace_id")
            .map_err(|e| ApiError::Unavailable(format!("read trace_id failed: {e}")))?,
        org_id: row
            .try_get("org_id")
            .map_err(|e| ApiError::Unavailable(format!("read org_id failed: {e}")))?,
        status: status_from_db(&status_raw).map_err(ApiError::Unavailable)?,
        provider_target: row
            .try_get("provider_target")
            .map_err(|e| ApiError::Unavailable(format!("read provider_target failed: {e}")))?,
        dispatch_mode: ExecutionDispatchMode::Manual,
        attempt_count: row
            .try_get("attempt_count")
            .map_err(|e| ApiError::Unavailable(format!("read attempt_count failed: {e}")))?,
        max_attempts: row
            .try_get("max_attempts")
            .map_err(|e| ApiError::Unavailable(format!("read max_attempts failed: {e}")))?,
        worker_id: row
            .try_get("worker_id")
            .map_err(|e| ApiError::Unavailable(format!("read worker_id failed: {e}")))?,
        lease_expires_at: row
            .try_get("lease_expires_at")
            .map_err(|e| ApiError::Unavailable(format!("read lease_expires_at failed: {e}")))?,
        started_at: row
            .try_get("started_at")
            .map_err(|e| ApiError::Unavailable(format!("read started_at failed: {e}")))?,
        ended_at: row
            .try_get("ended_at")
            .map_err(|e| ApiError::Unavailable(format!("read ended_at failed: {e}")))?,
        result_payload: row
            .try_get("result_payload")
            .map_err(|e| ApiError::Unavailable(format!("read result_payload failed: {e}")))?,
        approval_required: row
            .try_get("approval_required")
            .map_err(|e| ApiError::Unavailable(format!("read approval_required failed: {e}")))?,
        policy_reason: row
            .try_get("policy_reason")
            .map_err(|e| ApiError::Unavailable(format!("read policy_reason failed: {e}")))?,
        approved_by: row
            .try_get("approved_by")
            .map_err(|e| ApiError::Unavailable(format!("read approved_by failed: {e}")))?,
        created_at: row
            .try_get("created_at")
            .map_err(|e| ApiError::Unavailable(format!("read created_at failed: {e}")))?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|e| ApiError::Unavailable(format!("read updated_at failed: {e}")))?,
    };
    record.dispatch_mode = dispatch_mode_for_execution(&record);
    Ok(record)
}

fn execution_transition_event_types(status: &ExecutionStatus) -> (&'static str, &'static str) {
    match status {
        ExecutionStatus::Succeeded => ("execution.succeeded", "invocation.succeeded"),
        ExecutionStatus::Failed => ("execution.failed", "invocation.failed"),
        ExecutionStatus::Refunded => ("execution.refunded", "invocation.refunded"),
        _ => ("execution.running", "invocation.running"),
    }
}

async fn emit_simple_transition_audits(
    state: &AppState,
    record: &ExecutionRecord,
    actor_id: Option<String>,
    execution_event: &str,
    invocation_event: &str,
    note: Option<String>,
) {
    best_effort_audit(
        state,
        AuditEventCreateRequest {
            trace_id: record.trace_id,
            org_id: record.org_id.clone(),
            actor_type: "execution-service".to_string(),
            actor_id: actor_id.clone(),
            event_type: execution_event.to_string(),
            payload: json!({
                "execution_id": record.execution_id,
                "note": note,
            }),
        },
    )
    .await;
    best_effort_audit(
        state,
        AuditEventCreateRequest {
            trace_id: record.trace_id,
            org_id: record.org_id.clone(),
            actor_type: "execution-service".to_string(),
            actor_id,
            event_type: invocation_event.to_string(),
            payload: json!({
                "invocation_id": record.invocation_id,
                "execution_id": record.execution_id,
            }),
        },
    )
    .await;
}

async fn emit_refundish_invocation_audit(
    state: &AppState,
    trace_id: Uuid,
    org_id: Option<String>,
    invocation_id: Uuid,
    execution_id: Uuid,
    actor_id: Option<String>,
    event_type: &str,
    reason: String,
    refund_applied: bool,
) {
    best_effort_audit(
        state,
        AuditEventCreateRequest {
            trace_id,
            org_id,
            actor_type: "execution-service".to_string(),
            actor_id,
            event_type: event_type.to_string(),
            payload: json!({
                "invocation_id": invocation_id,
                "execution_id": execution_id,
                "reason": reason,
                "refund_applied": refund_applied,
            }),
        },
    )
    .await;
}

#[derive(Debug, Clone, Copy)]
enum TransitionDecision {
    Replay,
    Advance,
}

#[derive(Debug, Clone, Copy)]
enum ExecutionTransitionKind {
    Dispatch,
    Start,
    Cancel,
    Timeout,
    Succeed,
    Fail,
}

#[derive(Debug)]
enum ApiError {
    NotFound(String),
    Conflict(String, ExecutionStatus),
    Unavailable(String),
}

fn ensure_transition_allowed(
    kind: ExecutionTransitionKind,
    current: &ExecutionStatus,
    conflict_message: &str,
) -> Result<TransitionDecision, ApiError> {
    match kind {
        ExecutionTransitionKind::Dispatch => match current {
            ExecutionStatus::Dispatching => Ok(TransitionDecision::Replay),
            ExecutionStatus::Queued => Ok(TransitionDecision::Advance),
            _ => Err(conflict_error(conflict_message, current)),
        },
        ExecutionTransitionKind::Start => match current {
            ExecutionStatus::Running => Ok(TransitionDecision::Replay),
            ExecutionStatus::Queued | ExecutionStatus::Dispatching => {
                Ok(TransitionDecision::Advance)
            }
            _ => Err(conflict_error(conflict_message, current)),
        },
        ExecutionTransitionKind::Cancel => match current {
            ExecutionStatus::Cancelled => Ok(TransitionDecision::Replay),
            ExecutionStatus::Queued | ExecutionStatus::Dispatching | ExecutionStatus::Running => {
                Ok(TransitionDecision::Advance)
            }
            _ => Err(conflict_error(conflict_message, current)),
        },
        ExecutionTransitionKind::Timeout => match current {
            ExecutionStatus::TimedOut => Ok(TransitionDecision::Replay),
            ExecutionStatus::Queued | ExecutionStatus::Dispatching | ExecutionStatus::Running => {
                Ok(TransitionDecision::Advance)
            }
            _ => Err(conflict_error(conflict_message, current)),
        },
        ExecutionTransitionKind::Succeed => match current {
            ExecutionStatus::Succeeded => Ok(TransitionDecision::Replay),
            ExecutionStatus::Queued | ExecutionStatus::Dispatching | ExecutionStatus::Running => {
                Ok(TransitionDecision::Advance)
            }
            _ => Err(conflict_error(conflict_message, current)),
        },
        ExecutionTransitionKind::Fail => match current {
            ExecutionStatus::Failed => Ok(TransitionDecision::Replay),
            ExecutionStatus::Queued | ExecutionStatus::Dispatching | ExecutionStatus::Running => {
                Ok(TransitionDecision::Advance)
            }
            _ => Err(conflict_error(conflict_message, current)),
        },
    }
}

fn conflict_error(message: &str, current: &ExecutionStatus) -> ApiError {
    ApiError::Conflict(message.to_string(), current.clone())
}

fn worker_queue_visible_to_admin(admin: &AdminPrincipal, record: &ExecutionRecord) -> bool {
    if admin.org_ids.is_empty() {
        return true;
    }

    record
        .org_id
        .as_deref()
        .is_some_and(|org_id| admin_principal_allows_org(admin, org_id))
}

fn is_expired_worker_lease(record: &ExecutionRecord, now: chrono::DateTime<Utc>) -> bool {
    matches!(record.dispatch_mode, ExecutionDispatchMode::QueuedWorker)
        && matches!(record.status, ExecutionStatus::Dispatching)
        && record
            .lease_expires_at
            .is_some_and(|lease_expires_at| lease_expires_at <= now)
}

fn default_max_attempts_for_record(state: &AppState, record: &ExecutionRecord) -> i32 {
    match record.dispatch_mode {
        ExecutionDispatchMode::QueuedWorker => state.execution_queued_worker_max_attempts,
        _ => state.execution_default_max_attempts,
    }
}

fn remaining_attempts(record: &ExecutionRecord) -> i32 {
    (record.max_attempts - record.attempt_count).max(0)
}

fn has_worker_attempt_budget_remaining(record: &ExecutionRecord) -> bool {
    remaining_attempts(record) > 0
}

fn should_include_claim_candidate(
    record: &ExecutionRecord,
    now: chrono::DateTime<Utc>,
    lease_expired_only: bool,
) -> bool {
    if lease_expired_only {
        is_expired_worker_lease(record, now) && has_worker_attempt_budget_remaining(record)
    } else {
        is_claimable_queued_worker(record, now)
    }
}

fn is_claimable_queued_worker(record: &ExecutionRecord, now: chrono::DateTime<Utc>) -> bool {
    if !matches!(record.dispatch_mode, ExecutionDispatchMode::QueuedWorker)
        || !has_worker_attempt_budget_remaining(record)
    {
        return false;
    }

    match record.status {
        ExecutionStatus::Queued => true,
        ExecutionStatus::Dispatching => is_expired_worker_lease(record, now),
        _ => false,
    }
}

fn apply_worker_claim(record: &mut ExecutionRecord, claimed_by: &str, lease_seconds: i64) {
    record.status = ExecutionStatus::Dispatching;
    record.attempt_count += 1;
    record.worker_id = Some(claimed_by.to_string());
    renew_worker_lease(record, lease_seconds);
    record.dispatch_mode = dispatch_mode_for_execution(record);
}

fn renew_worker_lease(record: &mut ExecutionRecord, lease_seconds: i64) {
    record.lease_expires_at = Some(Utc::now() + Duration::seconds(lease_seconds));
    record.updated_at = Utc::now();
}

fn clear_worker_claim(record: &mut ExecutionRecord) {
    record.worker_id = None;
    record.lease_expires_at = None;
}

fn ensure_retryable_execution(record: &ExecutionRecord) -> Result<(), ApiError> {
    if record.max_attempts <= 1 {
        return Err(conflict_error(
            "execution dispatch mode does not support retry",
            &record.status,
        ));
    }

    if !matches!(
        record.status,
        ExecutionStatus::Failed | ExecutionStatus::TimedOut
    ) {
        return Err(conflict_error(
            "execution cannot be retried from current status",
            &record.status,
        ));
    }

    if !has_worker_attempt_budget_remaining(record) {
        return Err(conflict_error(
            "execution retry budget exhausted",
            &record.status,
        ));
    }

    Ok(())
}

fn prepare_execution_for_retry(record: &mut ExecutionRecord) {
    record.status = ExecutionStatus::Queued;
    clear_worker_claim(record);
    record.started_at = None;
    record.ended_at = None;
    record.result_payload = None;
    record.updated_at = Utc::now();
    record.dispatch_mode = dispatch_mode_for_execution(record);
}

fn validate_worker_lease_holder(record: &ExecutionRecord, worker_id: &str) -> Result<(), ApiError> {
    if !matches!(record.status, ExecutionStatus::Dispatching) {
        return Err(conflict_error(
            "execution is not currently claimed",
            &record.status,
        ));
    }

    let Some(current_worker_id) = record.worker_id.as_deref() else {
        return Err(conflict_error(
            "execution is not currently claimed",
            &record.status,
        ));
    };

    if current_worker_id != worker_id {
        return Err(conflict_error(
            "execution claimed by another worker",
            &record.status,
        ));
    }

    if let Some(lease_expires_at) = record.lease_expires_at {
        if lease_expires_at < Utc::now() {
            return Err(conflict_error("worker lease expired", &record.status));
        }
    }

    Ok(())
}

fn require_worker_lease_holder(
    record: &ExecutionRecord,
    worker_id: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    validate_worker_lease_holder(record, worker_id).map_err(|err| match err {
        ApiError::Conflict(message, _) if message == "execution claimed by another worker" => (
            StatusCode::CONFLICT,
            Json(json!({
                "error": message,
                "worker_id": record.worker_id,
                "status": record.status,
            })),
        ),
        ApiError::Conflict(message, _) if message == "worker lease expired" => (
            StatusCode::CONFLICT,
            Json(json!({
                "error": message,
                "lease_expires_at": record.lease_expires_at,
                "status": record.status,
            })),
        ),
        ApiError::Conflict(message, status) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "status": status })),
        ),
        ApiError::NotFound(message) => (StatusCode::NOT_FOUND, Json(json!({ "error": message }))),
        ApiError::Unavailable(message) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": message })),
        ),
    })
}

fn apply_transition(record: &mut ExecutionRecord, status: ExecutionStatus) {
    let keep_worker_claim = matches!(status, ExecutionStatus::Dispatching);
    record.status = status;
    record.updated_at = Utc::now();
    if !keep_worker_claim {
        clear_worker_claim(record);
    }
    record.dispatch_mode = dispatch_mode_for_execution(record);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PolicyDecision {
    requires_approval: bool,
    blocked: bool,
    reason: Option<String>,
}

fn evaluate_policy(req: &CreateExecutionRequest, state: &AppState) -> PolicyDecision {
    let mut approval_reasons = Vec::new();
    let mut block_reasons = Vec::new();

    if let Some(amount) = req.reserve_amount {
        if amount >= state.approval_reserve_threshold {
            approval_reasons.push(format!(
                "reserve_amount {:.3} >= approval threshold {:.3}",
                amount, state.approval_reserve_threshold
            ));
        }

        if let Some(hard_threshold) = state.hard_reject_reserve_threshold {
            if amount > hard_threshold {
                block_reasons.push(format!(
                    "reserve_amount {:.3} > hard reject threshold {:.3}",
                    amount, hard_threshold
                ));
            }
        }
    }

    let lowered_prompt = req.prompt.to_ascii_lowercase();
    for keyword in state.approval_sensitive_keywords.iter() {
        if lowered_prompt.contains(keyword) {
            approval_reasons.push(format!("prompt matched sensitive keyword '{}'", keyword));
        }
    }
    for keyword in state.block_keywords.iter() {
        if lowered_prompt.contains(keyword) {
            block_reasons.push(format!("prompt matched blocked keyword '{}'", keyword));
        }
    }

    if let Some(capability_id) = req.capability_id.as_deref() {
        let normalized = capability_id.to_ascii_lowercase();
        for prefix in state.approval_capability_prefixes.iter() {
            if normalized.starts_with(prefix) {
                approval_reasons.push(format!(
                    "capability_id '{}' matched approval prefix '{}'",
                    capability_id, prefix
                ));
            }
        }
        for prefix in state.block_capability_prefixes.iter() {
            if normalized.starts_with(prefix) {
                block_reasons.push(format!(
                    "capability_id '{}' matched blocked prefix '{}'",
                    capability_id, prefix
                ));
            }
        }
    }

    if !block_reasons.is_empty() {
        return PolicyDecision {
            requires_approval: false,
            blocked: true,
            reason: Some(block_reasons.join("; ")),
        };
    }

    if approval_reasons.is_empty() {
        PolicyDecision {
            requires_approval: false,
            blocked: false,
            reason: Some("auto-approved by configured policy".to_string()),
        }
    } else {
        PolicyDecision {
            requires_approval: true,
            blocked: false,
            reason: Some(approval_reasons.join("; ")),
        }
    }
}

fn status_to_db(status: &ExecutionStatus) -> &'static str {
    match status {
        ExecutionStatus::Created => "Created",
        ExecutionStatus::Queued => "Queued",
        ExecutionStatus::PolicyCheckPending => "PolicyCheckPending",
        ExecutionStatus::AwaitingApproval => "AwaitingApproval",
        ExecutionStatus::Approved => "Approved",
        ExecutionStatus::Dispatching => "Dispatching",
        ExecutionStatus::Running => "Running",
        ExecutionStatus::Succeeded => "Succeeded",
        ExecutionStatus::Failed => "Failed",
        ExecutionStatus::Cancelled => "Cancelled",
        ExecutionStatus::TimedOut => "TimedOut",
        ExecutionStatus::Refunded => "Refunded",
    }
}

fn status_from_db(raw: &str) -> Result<ExecutionStatus, String> {
    match raw {
        "Created" | "created" => Ok(ExecutionStatus::Created),
        "Queued" | "queued" => Ok(ExecutionStatus::Queued),
        "PolicyCheckPending" | "policy_check_pending" => Ok(ExecutionStatus::PolicyCheckPending),
        "AwaitingApproval" | "awaiting_approval" => Ok(ExecutionStatus::AwaitingApproval),
        "Approved" | "approved" => Ok(ExecutionStatus::Approved),
        "Dispatching" | "dispatching" => Ok(ExecutionStatus::Dispatching),
        "Running" | "running" => Ok(ExecutionStatus::Running),
        "Succeeded" | "succeeded" => Ok(ExecutionStatus::Succeeded),
        "Failed" | "failed" => Ok(ExecutionStatus::Failed),
        "Cancelled" | "cancelled" => Ok(ExecutionStatus::Cancelled),
        "TimedOut" | "timed_out" => Ok(ExecutionStatus::TimedOut),
        "Refunded" | "refunded" => Ok(ExecutionStatus::Refunded),
        other => Err(format!("unknown execution status: {other}")),
    }
}

async fn best_effort_audit(state: &AppState, req: AuditEventCreateRequest) {
    match state
        .http
        .post(format!("{}/v1/audit/events", state.audit_base_url))
        .json(&req)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {}
        Ok(_) | Err(_) => {
            state
                .metrics
                .audit_failures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DEFAULT_EXECUTION_DEFAULT_MAX_ATTEMPTS;
    use chrono::{Duration, Utc};

    fn sample_record(status: ExecutionStatus) -> ExecutionRecord {
        let now = Utc::now();
        ExecutionRecord {
            execution_id: Uuid::nil(),
            invocation_id: Uuid::nil(),
            trace_id: Uuid::nil(),
            org_id: None,
            status,
            provider_target: None,
            dispatch_mode: ExecutionDispatchMode::Manual,
            attempt_count: 0,
            max_attempts: DEFAULT_EXECUTION_DEFAULT_MAX_ATTEMPTS,
            worker_id: None,
            lease_expires_at: None,
            started_at: None,
            ended_at: None,
            result_payload: None,
            approval_required: false,
            policy_reason: None,
            approved_by: None,
            created_at: now,
            updated_at: now - Duration::seconds(5),
        }
    }

    #[test]
    fn dispatch_rules_allow_queued_replay_dispatching_and_block_succeeded() {
        let advance = ensure_transition_allowed(
            ExecutionTransitionKind::Dispatch,
            &ExecutionStatus::Queued,
            "dispatch conflict",
        )
        .unwrap();
        assert!(matches!(advance, TransitionDecision::Advance));

        let replay = ensure_transition_allowed(
            ExecutionTransitionKind::Dispatch,
            &ExecutionStatus::Dispatching,
            "dispatch conflict",
        )
        .unwrap();
        assert!(matches!(replay, TransitionDecision::Replay));

        let err = ensure_transition_allowed(
            ExecutionTransitionKind::Dispatch,
            &ExecutionStatus::Succeeded,
            "dispatch conflict",
        )
        .unwrap_err();
        match err {
            ApiError::Conflict(message, status) => {
                assert_eq!(message, "dispatch conflict");
                assert!(matches!(status, ExecutionStatus::Succeeded));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn start_rules_allow_queued_or_dispatching_replay_running_and_block_cancelled() {
        let from_queued = ensure_transition_allowed(
            ExecutionTransitionKind::Start,
            &ExecutionStatus::Queued,
            "start conflict",
        )
        .unwrap();
        assert!(matches!(from_queued, TransitionDecision::Advance));

        let from_dispatching = ensure_transition_allowed(
            ExecutionTransitionKind::Start,
            &ExecutionStatus::Dispatching,
            "start conflict",
        )
        .unwrap();
        assert!(matches!(from_dispatching, TransitionDecision::Advance));

        let replay = ensure_transition_allowed(
            ExecutionTransitionKind::Start,
            &ExecutionStatus::Running,
            "start conflict",
        )
        .unwrap();
        assert!(matches!(replay, TransitionDecision::Replay));

        let err = ensure_transition_allowed(
            ExecutionTransitionKind::Start,
            &ExecutionStatus::Cancelled,
            "start conflict",
        )
        .unwrap_err();
        match err {
            ApiError::Conflict(message, status) => {
                assert_eq!(message, "start conflict");
                assert!(matches!(status, ExecutionStatus::Cancelled));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn terminal_and_refund_transition_rules_behave_as_expected() {
        let cancel_replay = ensure_transition_allowed(
            ExecutionTransitionKind::Cancel,
            &ExecutionStatus::Cancelled,
            "cancel conflict",
        )
        .unwrap();
        assert!(matches!(cancel_replay, TransitionDecision::Replay));

        let timeout_replay = ensure_transition_allowed(
            ExecutionTransitionKind::Timeout,
            &ExecutionStatus::TimedOut,
            "timeout conflict",
        )
        .unwrap();
        assert!(matches!(timeout_replay, TransitionDecision::Replay));

        let succeed_replay = ensure_transition_allowed(
            ExecutionTransitionKind::Succeed,
            &ExecutionStatus::Succeeded,
            "succeed conflict",
        )
        .unwrap();
        assert!(matches!(succeed_replay, TransitionDecision::Replay));

        let fail_replay = ensure_transition_allowed(
            ExecutionTransitionKind::Fail,
            &ExecutionStatus::Failed,
            "fail conflict",
        )
        .unwrap();
        assert!(matches!(fail_replay, TransitionDecision::Replay));

        let cancel_advance = ensure_transition_allowed(
            ExecutionTransitionKind::Cancel,
            &ExecutionStatus::Running,
            "cancel conflict",
        )
        .unwrap();
        assert!(matches!(cancel_advance, TransitionDecision::Advance));

        let timeout_advance = ensure_transition_allowed(
            ExecutionTransitionKind::Timeout,
            &ExecutionStatus::Dispatching,
            "timeout conflict",
        )
        .unwrap();
        assert!(matches!(timeout_advance, TransitionDecision::Advance));

        let succeed_advance = ensure_transition_allowed(
            ExecutionTransitionKind::Succeed,
            &ExecutionStatus::Running,
            "succeed conflict",
        )
        .unwrap();
        assert!(matches!(succeed_advance, TransitionDecision::Advance));

        let fail_advance = ensure_transition_allowed(
            ExecutionTransitionKind::Fail,
            &ExecutionStatus::Queued,
            "fail conflict",
        )
        .unwrap();
        assert!(matches!(fail_advance, TransitionDecision::Advance));

        let err = ensure_transition_allowed(
            ExecutionTransitionKind::Fail,
            &ExecutionStatus::Succeeded,
            "fail conflict",
        )
        .unwrap_err();
        match err {
            ApiError::Conflict(message, status) => {
                assert_eq!(message, "fail conflict");
                assert!(matches!(status, ExecutionStatus::Succeeded));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn apply_transition_updates_status_and_timestamp() {
        let mut record = sample_record(ExecutionStatus::Queued);
        let old_updated_at = record.updated_at;
        apply_transition(&mut record, ExecutionStatus::Running);

        assert!(matches!(record.status, ExecutionStatus::Running));
        assert!(record.updated_at > old_updated_at);
    }

    #[test]
    fn evaluate_policy_flags_sensitive_requests_and_auto_approves_safe_requests() {
        let safe = CreateExecutionRequest {
            invocation_id: Uuid::nil(),
            trace_id: Uuid::nil(),
            org_id: Some("00000000-0000-0000-0000-00000000ce01".to_string()),
            actor_id: None,
            capability_id: None,
            capability_provider: None,
            capability_provider_ref: None,
            prompt: "summarize logs".to_string(),
            reserve_amount: Some(5.0),
        };
        let risky = CreateExecutionRequest {
            invocation_id: Uuid::nil(),
            trace_id: Uuid::nil(),
            org_id: Some("00000000-0000-0000-0000-00000000ce01".to_string()),
            actor_id: None,
            capability_id: None,
            capability_provider: None,
            capability_provider_ref: None,
            prompt: "publish deploy package".to_string(),
            reserve_amount: Some(25.0),
        };

        let state = AppState::new_for_tests(false, None, vec![], vec![]);

        let safe_policy = evaluate_policy(&safe, &state);
        assert!(!safe_policy.requires_approval);
        assert!(!safe_policy.blocked);
        assert_eq!(
            safe_policy.reason.as_deref(),
            Some("auto-approved by configured policy")
        );

        let risky_policy = evaluate_policy(&risky, &state);
        assert!(risky_policy.requires_approval);
        assert!(!risky_policy.blocked);
        let risky_reason = risky_policy.reason.expect("expected risky reason");
        assert!(risky_reason.contains("reserve_amount 25.000 >= approval threshold 10.000"));
        assert!(risky_reason.contains("prompt matched sensitive keyword 'publish'"));
        assert!(risky_reason.contains("prompt matched sensitive keyword 'deploy'"));
    }

    #[test]
    fn evaluate_policy_blocks_requests_that_match_hard_reject_rules() {
        let mut state = AppState::new_for_tests(false, None, vec![], vec![]);
        state.hard_reject_reserve_threshold = Some(50.0);
        state.block_keywords = std::sync::Arc::new(vec!["rm -rf".to_string()]);
        state.block_capability_prefixes = std::sync::Arc::new(vec!["cap.blocked".to_string()]);

        let blocked = CreateExecutionRequest {
            invocation_id: Uuid::nil(),
            trace_id: Uuid::nil(),
            org_id: Some("00000000-0000-0000-0000-00000000ce01".to_string()),
            actor_id: None,
            capability_id: Some("cap.blocked.delete".to_string()),
            capability_provider: None,
            capability_provider_ref: None,
            prompt: "please rm -rf the temp workspace".to_string(),
            reserve_amount: Some(75.0),
        };

        let decision = evaluate_policy(&blocked, &state);
        assert!(decision.blocked);
        assert!(!decision.requires_approval);
        let reason = decision.reason.expect("blocked reason");
        assert!(reason.contains("hard reject threshold"));
        assert!(reason.contains("blocked keyword 'rm -rf'"));
        assert!(reason.contains("matched blocked prefix 'cap.blocked'"));
    }

    #[test]
    fn evaluate_policy_can_require_approval_for_capability_prefixes() {
        let mut state = AppState::new_for_tests(false, None, vec![], vec![]);
        state.approval_capability_prefixes = std::sync::Arc::new(vec!["cap.finance".to_string()]);

        let request = CreateExecutionRequest {
            invocation_id: Uuid::nil(),
            trace_id: Uuid::nil(),
            org_id: Some("00000000-0000-0000-0000-00000000ce01".to_string()),
            actor_id: None,
            capability_id: Some("cap.finance.wire".to_string()),
            capability_provider: None,
            capability_provider_ref: None,
            prompt: "summarize the invoice".to_string(),
            reserve_amount: Some(1.0),
        };

        let decision = evaluate_policy(&request, &state);
        assert!(decision.requires_approval);
        assert!(!decision.blocked);
        assert!(decision
            .reason
            .expect("approval reason")
            .contains("matched approval prefix 'cap.finance'"));
    }

    #[test]
    fn build_execution_runtime_overview_summarizes_backlog_and_queue() {
        let mut awaiting = sample_record(ExecutionStatus::AwaitingApproval);
        awaiting.execution_id = Uuid::new_v4();

        let mut queued_worker = sample_record(ExecutionStatus::Queued);
        queued_worker.execution_id = Uuid::new_v4();
        queued_worker.dispatch_mode = ExecutionDispatchMode::QueuedWorker;
        queued_worker.max_attempts = 3;

        let mut dispatching_worker = sample_record(ExecutionStatus::Dispatching);
        dispatching_worker.execution_id = Uuid::new_v4();
        dispatching_worker.dispatch_mode = ExecutionDispatchMode::QueuedWorker;
        dispatching_worker.max_attempts = 3;
        dispatching_worker.worker_id = Some("worker-a".to_string());
        dispatching_worker.lease_expires_at = Some(Utc::now() + Duration::seconds(60));

        let mut running = sample_record(ExecutionStatus::Running);
        running.execution_id = Uuid::new_v4();

        let mut provider_failure = sample_record(ExecutionStatus::Refunded);
        provider_failure.execution_id = Uuid::new_v4();
        provider_failure.provider_target = Some("minimax://MiniMax-M2.5".to_string());
        provider_failure.result_payload = Some(json!({
            "error": "⚠️ minimax returned a billing error — insufficient balance (1008)"
        }));

        let overview = build_execution_runtime_overview(vec![
            awaiting,
            queued_worker,
            dispatching_worker,
            running,
            provider_failure,
        ]);

        assert_eq!(overview.total, 5);
        assert_eq!(overview.awaiting_approval, 1);
        assert_eq!(overview.queued, 1);
        assert_eq!(overview.dispatching, 1);
        assert_eq!(overview.running, 1);
        assert_eq!(overview.refunded, 1);
        assert_eq!(overview.queued_worker.total, 2);
        assert_eq!(overview.queued_worker.queued, 1);
        assert_eq!(overview.queued_worker.dispatching, 1);
        assert_eq!(overview.queued_worker.claimed_active, 1);
        assert_eq!(overview.queued_worker.active_workers, 1);
        assert_eq!(overview.provider_failures.total, 1);
        assert_eq!(overview.provider_failures.billing, 1);
    }

    #[test]
    fn classify_provider_error_text_covers_operator_categories() {
        assert_eq!(
            classify_provider_error_text("insufficient balance (1008)"),
            ProviderFailureKind::Billing
        );
        assert_eq!(
            classify_provider_error_text("provider dispatch timed out after 60s"),
            ProviderFailureKind::Timeout
        );
        assert_eq!(
            classify_provider_error_text("invalid api key"),
            ProviderFailureKind::Auth
        );
        assert_eq!(
            classify_provider_error_text("rate limit exceeded 429"),
            ProviderFailureKind::RateLimited
        );
        assert_eq!(
            classify_provider_error_text("provider upstream returned status 503"),
            ProviderFailureKind::Unavailable
        );
        assert_eq!(
            classify_provider_error_text("unexpected provider shape"),
            ProviderFailureKind::Unknown
        );
    }

    #[test]
    fn build_execution_operator_signals_marks_threshold_breaches() {
        let mut state = AppState::new_for_tests(false, None, vec![], vec![]);
        state.alert_approval_backlog_threshold = 2;
        state.alert_lease_expired_threshold = 1;
        state.alert_retry_budget_exhausted_threshold = 1;
        state.alert_provider_failure_threshold = 1;
        state.alert_provider_billing_failure_threshold = 1;
        state.alert_provider_timeout_failure_threshold = 1;
        state.alert_audit_failure_threshold = 1;
        state.alert_refund_failure_threshold = 1;
        state
            .metrics
            .audit_failures
            .store(1, std::sync::atomic::Ordering::Relaxed);
        state
            .metrics
            .refund_failures
            .store(1, std::sync::atomic::Ordering::Relaxed);

        let overview = ExecutionRuntimeOverview {
            total: 5,
            created: 0,
            policy_check_pending: 0,
            awaiting_approval: 2,
            approved: 0,
            queued: 1,
            dispatching: 1,
            running: 0,
            succeeded: 0,
            failed: 0,
            cancelled: 0,
            timed_out: 0,
            refunded: 0,
            queued_worker: WorkerQueueSummary {
                total: 2,
                queued: 1,
                dispatching: 1,
                claimable: 1,
                lease_expired: 1,
                claimed_active: 0,
                retryable: 1,
                retry_budget_exhausted: 1,
                active_workers: 0,
            },
            provider_failures: ProviderFailureSummary {
                total: 2,
                billing: 1,
                timeout: 1,
                auth: 0,
                rate_limited: 0,
                unavailable: 0,
                unknown: 0,
            },
        };

        let signals = build_execution_operator_signals(&state, &overview);
        assert!(signals.approval_backlog.alert);
        assert!(signals.queued_worker_lease_expired.alert);
        assert!(signals.queued_worker_retry_budget_exhausted.alert);
        assert!(signals.provider_failures.alert);
        assert!(signals.provider_billing_failures.alert);
        assert!(signals.provider_timeout_failures.alert);
        assert!(signals.audit_failures.alert);
        assert!(signals.refund_failures.alert);
        assert_eq!(signals.approval_backlog.value, 2);
        assert_eq!(signals.approval_backlog.threshold, 2);
    }

    #[test]
    fn status_round_trip_conversion_covers_known_values() {
        let statuses = [
            ExecutionStatus::Created,
            ExecutionStatus::Queued,
            ExecutionStatus::PolicyCheckPending,
            ExecutionStatus::AwaitingApproval,
            ExecutionStatus::Approved,
            ExecutionStatus::Dispatching,
            ExecutionStatus::Running,
            ExecutionStatus::Succeeded,
            ExecutionStatus::Failed,
            ExecutionStatus::Cancelled,
            ExecutionStatus::TimedOut,
            ExecutionStatus::Refunded,
        ];

        for status in statuses {
            let raw = status_to_db(&status);
            let decoded = status_from_db(raw).expect("round trip decode failed");
            assert_eq!(raw, status_to_db(&decoded));
        }

        let err = status_from_db("definitely-not-a-status").unwrap_err();
        assert!(err.contains("unknown execution status"));
    }
}
