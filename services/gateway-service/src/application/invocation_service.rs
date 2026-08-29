use crate::{
    domain::invocation::{InvocationExecutionState, InvocationRecord, InvocationRequest},
    infrastructure::{
        clients::{
            AccountLookupResult, ExecutionCreateRequest, ExecutionCreateResult, LedgerActionRequest,
        },
        state::AppState,
    },
};
use serde_json::json;
use shared_types::{ExecutionDispatchMode, ExecutionStatus, TraceContext};
use sqlx::Row;
use uuid::Uuid;

pub async fn create_invocation(state: AppState, req: InvocationRequest) -> InvocationRecord {
    let invocation_id = Uuid::new_v4();
    let trace = TraceContext {
        trace_id: Uuid::new_v4(),
        created_at: chrono::Utc::now(),
    };

    let mut record = InvocationRecord {
        invocation_id,
        trace: trace.clone(),
        status: ExecutionStatus::Created,
        request: req,
        ledger_reserved: false,
        ledger_refunded: false,
        approval_required: false,
        execution_id: None,
        execution: None,
        policy_reason: None,
        failure_reason: None,
    };

    if !persist_invocation_record(&state, &mut record, "invocation persistence failed").await {
        return record;
    }

    audit_invocation_created(&state, &record).await;

    if let Some(account_id) = record.request.account_id {
        match ensure_account_belongs_to_org(&state, &record.request.org_id, account_id).await {
            Ok(account) => {
                audit_invocation_account_verified(&state, &record, &account).await;
            }
            Err(reason) => {
                best_effort_store_invocation_update(&state, &mut record, |record| {
                    record.status = ExecutionStatus::Failed;
                    record.failure_reason = Some(reason);
                })
                .await;
                audit_invocation_failed(&state, &record, record.request.actor_id.clone()).await;
                return record;
            }
        }
    }

    if let (Some(account_id), Some(amount)) =
        (record.request.account_id, record.request.reserve_amount)
    {
        let reserve_req = LedgerActionRequest {
            account_id,
            amount,
            reference_id: Some(invocation_id.to_string()),
            idempotency_key: Some(format!("reserve:{invocation_id}")),
        };

        match state.clients.reserve_credits_legacy_v1(&reserve_req).await {
            Ok(()) => {
                if !persisted_invocation_update(
                    &state,
                    &mut record,
                    "invocation persistence failed after reserve",
                    |record| {
                        record.ledger_reserved = true;
                    },
                )
                .await
                {
                    return record;
                }
                audit_invocation_ledger_reserved(&state, &record, account_id, amount).await;
            }
            Err(err) => {
                best_effort_store_invocation_update(&state, &mut record, |record| {
                    record.status = ExecutionStatus::Failed;
                    record.failure_reason = Some(format!("ledger reserve failed: {}", err.message));
                })
                .await;
                audit_invocation_failed(&state, &record, None).await;
                return record;
            }
        }
    }

    match state
        .clients
        .create_execution(&ExecutionCreateRequest {
            invocation_id,
            trace_id: trace.trace_id,
            org_id: Some(record.request.org_id.clone()),
            actor_id: record.request.actor_id.clone(),
            capability_id: record.request.capability_id.clone(),
            capability_provider: record.request.capability_provider.clone(),
            capability_provider_ref: record.request.capability_provider_ref.clone(),
            prompt: record.request.prompt.clone(),
            reserve_amount: record.request.reserve_amount,
        })
        .await
    {
        Ok(execution) => {
            let dispatch_mode = execution.dispatch_mode.clone();
            apply_execution_result(&mut record, execution);

            if matches!(dispatch_mode, ExecutionDispatchMode::Immediate) {
                if let Some(execution_id) = record.execution_id {
                    match state.clients.start_execution(execution_id).await {
                        Ok(started) => {
                            apply_execution_result(&mut record, started);
                            match record.status {
                                ExecutionStatus::Succeeded => {
                                    record.ledger_reserved = false;
                                    record.ledger_refunded = false;
                                    record.failure_reason = None;
                                }
                                ExecutionStatus::Refunded => {
                                    record.ledger_reserved = false;
                                    record.ledger_refunded = true;
                                    record.failure_reason = Some(
                                        "provider execution failed and reserve was refunded"
                                            .to_string(),
                                    );
                                }
                                ExecutionStatus::Failed => {
                                    record.ledger_reserved = false;
                                    record.ledger_refunded = false;
                                    record.failure_reason =
                                        Some("provider execution failed".to_string());
                                }
                                _ => {}
                            }
                        }
                        Err(err) => {
                            record.failure_reason =
                                Some(format!("execution auto-start deferred: {}", err.message));
                        }
                    }
                }
            }

            let _ = persist_invocation_record(
                &state,
                &mut record,
                "invocation persistence failed after execution create",
            )
            .await;
            audit_invocation_lifecycle(&state, &record, record.request.actor_id.clone()).await;
            record
        }
        Err(err) => {
            let base_error = format!("execution create failed: {}", err.message);
            if let (true, Some(account_id), Some(amount)) = (
                record.ledger_reserved,
                record.request.account_id,
                record.request.reserve_amount,
            ) {
                let refund_req = LedgerActionRequest {
                    account_id,
                    amount,
                    reference_id: Some(invocation_id.to_string()),
                    idempotency_key: Some(format!("refund:{invocation_id}")),
                };

                match state.clients.refund_credits_legacy_v1(&refund_req).await {
                    Ok(()) => {
                        record.ledger_refunded = true;
                        record.status = ExecutionStatus::Refunded;
                        record.failure_reason = Some(format!("{base_error}; reserve refunded"));
                        audit_invocation_refunded(&state, &record, None).await;
                    }
                    Err(refund_err) => {
                        record.status = ExecutionStatus::Failed;
                        record.failure_reason = Some(format!(
                            "{base_error}; refund failed: {}",
                            refund_err.message
                        ));
                    }
                }
            } else {
                record.status = ExecutionStatus::Failed;
                record.failure_reason = Some(base_error);
            }

            best_effort_store_invocation_record(&state, &mut record).await;
            audit_invocation_failed(&state, &record, None).await;

            record
        }
    }
}

pub async fn get_invocation(state: AppState, id: Uuid) -> Result<Option<InvocationRecord>, String> {
    let Some(mut record) = load_invocation(&state, id).await? else {
        return Ok(None);
    };

    refresh_execution_snapshot_cache(&state, &mut record).await;

    Ok(Some(record))
}

fn apply_execution_result(record: &mut InvocationRecord, execution: ExecutionCreateResult) {
    apply_execution_snapshot(record, &execution);
    record.status = execution.status;
    record.approval_required = execution.approval_required;
    record.policy_reason = execution.policy_reason;
}

fn apply_execution_snapshot(record: &mut InvocationRecord, execution: &ExecutionCreateResult) {
    record.execution_id = Some(execution.execution_id);
    record.execution = Some(execution_snapshot(execution));
}

async fn refresh_execution_snapshot_cache(state: &AppState, record: &mut InvocationRecord) {
    let Some(execution_id) = record.execution_id else {
        return;
    };

    if let Ok(execution) = state.clients.get_execution(execution_id).await {
        best_effort_store_invocation_update(state, record, |record| {
            apply_execution_snapshot(record, &execution);
        })
        .await;
    }
}

fn execution_snapshot(execution: &ExecutionCreateResult) -> InvocationExecutionState {
    InvocationExecutionState {
        dispatch_mode: execution.dispatch_mode.clone(),
        attempt_count: execution.attempt_count,
        max_attempts: execution.max_attempts,
        attempts_remaining: execution.attempts_remaining(),
        retry_budget_exhausted: execution.retry_budget_exhausted(),
    }
}

async fn persist_invocation_record(
    state: &AppState,
    record: &mut InvocationRecord,
    failure_context: &str,
) -> bool {
    persisted_invocation_update(state, record, failure_context, |_| {}).await
}

async fn best_effort_store_invocation_record(state: &AppState, record: &mut InvocationRecord) {
    best_effort_store_invocation_update(state, record, |_| {}).await;
}

async fn persisted_invocation_update<F>(
    state: &AppState,
    record: &mut InvocationRecord,
    failure_context: &str,
    apply: F,
) -> bool
where
    F: FnOnce(&mut InvocationRecord),
{
    apply(record);

    match store_invocation(state, record).await {
        Ok(()) => true,
        Err(err) => {
            record.status = ExecutionStatus::Failed;
            record.failure_reason = Some(format!("{failure_context}: {err}"));
            false
        }
    }
}

async fn best_effort_store_invocation_update<F>(
    state: &AppState,
    record: &mut InvocationRecord,
    apply: F,
) where
    F: FnOnce(&mut InvocationRecord),
{
    apply(record);
    let _ = store_invocation(state, record).await;
}

async fn store_invocation(state: &AppState, record: &InvocationRecord) -> Result<(), String> {
    if let Some(pool) = &state.pool {
        return upsert_invocation(pool, record).await;
    }

    if state.fail_fast {
        return Err("gateway postgres pool not initialized".to_string());
    }

    state
        .invocations
        .write()
        .await
        .insert(record.invocation_id, record.clone());
    Ok(())
}

async fn load_invocation(state: &AppState, id: Uuid) -> Result<Option<InvocationRecord>, String> {
    if let Some(pool) = &state.pool {
        return load_invocation_from_db(pool, id).await;
    }

    if state.fail_fast {
        return Err("gateway postgres pool not initialized".to_string());
    }

    Ok(state.invocations.read().await.get(&id).cloned())
}

fn encode_execution_snapshot(
    snapshot: &Option<InvocationExecutionState>,
) -> Result<Option<serde_json::Value>, String> {
    snapshot
        .as_ref()
        .map(|snapshot| {
            serde_json::to_value(snapshot)
                .map_err(|e| format!("serialize invocation execution snapshot failed: {e}"))
        })
        .transpose()
}

fn decode_execution_snapshot(
    raw: Option<String>,
) -> Result<Option<InvocationExecutionState>, String> {
    raw.map(|raw| {
        serde_json::from_str(&raw)
            .map_err(|e| format!("decode invocation execution snapshot failed: {e}; payload={raw}"))
    })
    .transpose()
}

async fn upsert_invocation(pool: &sqlx::PgPool, record: &InvocationRecord) -> Result<(), String> {
    let request_payload = serde_json::to_string(&record.request)
        .map_err(|e| format!("serialize invocation request failed: {e}"))?;
    let execution_snapshot = encode_execution_snapshot(&record.execution)?;
    let actor_id = record
        .request
        .actor_id
        .as_deref()
        .and_then(|raw| Uuid::parse_str(raw).ok());
    let capability_id = record
        .request
        .capability_id
        .as_deref()
        .and_then(|raw| Uuid::parse_str(raw).ok());

    sqlx::query(
        "insert into invocations (invocation_id, org_id, actor_id, capability_id, status, request_payload, route_hint, trace_id, created_at, updated_at, execution_id, execution_snapshot, ledger_reserved, ledger_refunded, approval_required, policy_reason, failure_reason) values ($1, $2::uuid, $3, $4, $5, $6::jsonb, null, $7, $8, $9, $10, $11::jsonb, $12, $13, $14, $15, $16) on conflict (invocation_id) do update set org_id = excluded.org_id, actor_id = excluded.actor_id, capability_id = excluded.capability_id, status = excluded.status, request_payload = excluded.request_payload, trace_id = excluded.trace_id, updated_at = excluded.updated_at, execution_id = excluded.execution_id, execution_snapshot = excluded.execution_snapshot, ledger_reserved = excluded.ledger_reserved, ledger_refunded = excluded.ledger_refunded, approval_required = excluded.approval_required, policy_reason = excluded.policy_reason, failure_reason = excluded.failure_reason"
    )
    .bind(record.invocation_id)
    .bind(&record.request.org_id)
    .bind(actor_id)
    .bind(capability_id)
    .bind(status_to_db(&record.status))
    .bind(request_payload)
    .bind(record.trace.trace_id)
    .bind(record.trace.created_at)
    .bind(chrono::Utc::now())
    .bind(record.execution_id)
    .bind(execution_snapshot)
    .bind(record.ledger_reserved)
    .bind(record.ledger_refunded)
    .bind(record.approval_required)
    .bind(&record.policy_reason)
    .bind(&record.failure_reason)
    .execute(pool)
    .await
    .map_err(|e| format!("upsert invocation failed: {e}"))?;

    Ok(())
}

async fn load_invocation_from_db(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<Option<InvocationRecord>, String> {
    let row = sqlx::query(
        "select invocation_id, trace_id, created_at, status, request_payload::text as request_payload_text, execution_id, execution_snapshot::text as execution_snapshot_text, ledger_reserved, ledger_refunded, approval_required, policy_reason, failure_reason from invocations where invocation_id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("load invocation failed: {e}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let request_payload_text: String = row
        .try_get("request_payload_text")
        .map_err(|e| format!("read invocation request payload failed: {e}"))?;
    let request: InvocationRequest = serde_json::from_str(&request_payload_text).map_err(|e| {
        format!("decode invocation request failed: {e}; payload={request_payload_text}")
    })?;
    let status_raw: String = row
        .try_get("status")
        .map_err(|e| format!("read invocation status failed: {e}"))?;
    let execution_snapshot_text: Option<String> = row
        .try_get("execution_snapshot_text")
        .map_err(|e| format!("read invocation execution snapshot failed: {e}"))?;
    let execution = decode_execution_snapshot(execution_snapshot_text)?;

    Ok(Some(InvocationRecord {
        invocation_id: row
            .try_get("invocation_id")
            .map_err(|e| format!("read invocation_id failed: {e}"))?,
        trace: TraceContext {
            trace_id: row
                .try_get("trace_id")
                .map_err(|e| format!("read trace_id failed: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("read created_at failed: {e}"))?,
        },
        status: status_from_db(&status_raw)?,
        request,
        ledger_reserved: row
            .try_get("ledger_reserved")
            .map_err(|e| format!("read ledger_reserved failed: {e}"))?,
        ledger_refunded: row
            .try_get("ledger_refunded")
            .map_err(|e| format!("read ledger_refunded failed: {e}"))?,
        approval_required: row
            .try_get("approval_required")
            .map_err(|e| format!("read approval_required failed: {e}"))?,
        execution_id: row
            .try_get("execution_id")
            .map_err(|e| format!("read execution_id failed: {e}"))?,
        execution,
        policy_reason: row
            .try_get("policy_reason")
            .map_err(|e| format!("read policy_reason failed: {e}"))?,
        failure_reason: row
            .try_get("failure_reason")
            .map_err(|e| format!("read failure_reason failed: {e}"))?,
    }))
}

async fn ensure_account_belongs_to_org(
    state: &AppState,
    expected_org_id: &str,
    account_id: Uuid,
) -> Result<AccountLookupResult, String> {
    let account = state
        .clients
        .get_account(account_id)
        .await
        .map_err(|err| format!("account lookup failed: {}", err.message))?;

    if account.org_id != expected_org_id {
        return Err(format!(
            "account org mismatch: account {} belongs to org {}, not {}",
            account.account_id, account.org_id, expected_org_id
        ));
    }

    Ok(account)
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

async fn audit_invocation_created(state: &AppState, record: &InvocationRecord) {
    best_effort_audit(
        state,
        record.trace.trace_id,
        Some(record.request.org_id.clone()),
        "gateway-service",
        None,
        "invocation.created",
        json!({
            "invocation_id": record.invocation_id,
            "org_id": record.request.org_id.clone(),
            "capability_id": record.request.capability_id.clone(),
        }),
    )
    .await;
}

async fn audit_invocation_account_verified(
    state: &AppState,
    record: &InvocationRecord,
    account: &AccountLookupResult,
) {
    best_effort_audit(
        state,
        record.trace.trace_id,
        Some(record.request.org_id.clone()),
        "gateway-service",
        record.request.actor_id.clone(),
        "invocation.account_verified",
        json!({
            "invocation_id": record.invocation_id,
            "account_id": account.account_id,
            "account_org_id": account.org_id,
        }),
    )
    .await;
}

async fn audit_invocation_ledger_reserved(
    state: &AppState,
    record: &InvocationRecord,
    account_id: Uuid,
    amount: f64,
) {
    best_effort_audit(
        state,
        record.trace.trace_id,
        Some(record.request.org_id.clone()),
        "gateway-service",
        None,
        "invocation.ledger_reserved",
        json!({
            "invocation_id": record.invocation_id,
            "account_id": account_id,
            "amount": amount,
        }),
    )
    .await;
}

fn invocation_lifecycle_event_type(status: &ExecutionStatus) -> &'static str {
    match status {
        ExecutionStatus::AwaitingApproval => "invocation.awaiting_approval",
        ExecutionStatus::Succeeded => "invocation.succeeded",
        ExecutionStatus::Refunded => "invocation.refunded",
        ExecutionStatus::Failed => "invocation.failed",
        _ => "invocation.accepted",
    }
}

fn invocation_lifecycle_payload(record: &InvocationRecord) -> serde_json::Value {
    json!({
        "invocation_id": record.invocation_id,
        "execution_id": record.execution_id,
        "status": record.status,
        "approval_required": record.approval_required,
        "policy_reason": record.policy_reason.clone(),
        "failure_reason": record.failure_reason.clone(),
    })
}

async fn audit_invocation_lifecycle(
    state: &AppState,
    record: &InvocationRecord,
    actor_id: Option<String>,
) {
    best_effort_audit(
        state,
        record.trace.trace_id,
        Some(record.request.org_id.clone()),
        "gateway-service",
        actor_id,
        invocation_lifecycle_event_type(&record.status),
        invocation_lifecycle_payload(record),
    )
    .await;
}

async fn audit_invocation_refunded(
    state: &AppState,
    record: &InvocationRecord,
    actor_id: Option<String>,
) {
    best_effort_audit(
        state,
        record.trace.trace_id,
        Some(record.request.org_id.clone()),
        "gateway-service",
        actor_id,
        "invocation.refunded",
        json!({
            "invocation_id": record.invocation_id,
            "reason": record.failure_reason.clone(),
        }),
    )
    .await;
}

async fn audit_invocation_failed(
    state: &AppState,
    record: &InvocationRecord,
    actor_id: Option<String>,
) {
    best_effort_audit(
        state,
        record.trace.trace_id,
        Some(record.request.org_id.clone()),
        "gateway-service",
        actor_id,
        "invocation.failed",
        json!({
            "invocation_id": record.invocation_id,
            "reason": record.failure_reason.clone(),
        }),
    )
    .await;
}

async fn best_effort_audit(
    state: &AppState,
    trace_id: Uuid,
    org_id: Option<String>,
    actor_type: &str,
    actor_id: Option<String>,
    event_type: &str,
    payload: serde_json::Value,
) {
    let _ = state
        .clients
        .emit_audit_event(trace_id, org_id, actor_type, actor_id, event_type, payload)
        .await;
}
