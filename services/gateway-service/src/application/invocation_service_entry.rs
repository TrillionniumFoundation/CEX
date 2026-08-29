use crate::{
    domain::invocation::{InvocationRecord, InvocationRequest},
    infrastructure::state::AppState,
};
use serde_json::json;
use shared_types::saga::{operation_key, SagaCommandKind};
use std::env;
use uuid::Uuid;

mod legacy {
    include!("invocation_service.rs");
}

#[derive(Debug)]
struct ShadowCommand {
    command_id: Uuid,
    operation_key: String,
    command_kind: SagaCommandKind,
    max_attempts: i32,
    payload: serde_json::Value,
}

pub async fn create_invocation(state: AppState, req: InvocationRequest) -> InvocationRecord {
    let record = legacy::create_invocation(state.clone(), req).await;

    if saga_shadow_write_enabled() {
        state
            .metrics
            .saga_shadow_write_attempts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match persist_shadow_commands(&state, &record).await {
            Ok(()) => {
                state
                    .metrics
                    .saga_shadow_write_successes
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Err(error) => {
                state
                    .metrics
                    .saga_shadow_write_failures
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                // Shadow writes are observational in v1 and must never change the
                // authoritative synchronous response.
                eprintln!(
                    "gateway-service: saga shadow write failed invocation_id={}: {error}",
                    record.invocation_id
                );
            }
        }
    }

    record
}

pub async fn get_invocation(
    state: AppState,
    id: Uuid,
) -> Result<Option<InvocationRecord>, String> {
    legacy::get_invocation(state, id).await
}

fn saga_shadow_write_enabled() -> bool {
    env::var("CEX_SAGA_SHADOW_WRITE")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn shadow_commands_for_record(record: &InvocationRecord) -> Result<Vec<ShadowCommand>, String> {
    let workflow_kind = "invocation";
    let observed = json!({
        "status": record.status.clone(),
        "ledger_reserved": record.ledger_reserved,
        "ledger_refunded": record.ledger_refunded,
        "execution_id": record.execution_id,
        "approval_required": record.approval_required,
        "failure_reason": record.failure_reason.clone(),
    });
    let mut commands = Vec::new();

    if let (Some(account_id), Some(amount)) =
        (record.request.account_id, record.request.reserve_amount)
    {
        if amount > 0.0 {
            commands.push(ShadowCommand {
                command_id: Uuid::new_v4(),
                operation_key: operation_key(
                    workflow_kind,
                    record.invocation_id,
                    SagaCommandKind::LedgerReserve,
                    "initial",
                )
                .map_err(|error| error.to_string())?,
                command_kind: SagaCommandKind::LedgerReserve,
                max_attempts: 3,
                payload: json!({
                    "schema": "cex.saga.shadow.ledger-reserve.v1",
                    "shadow_only": true,
                    "invocation_id": record.invocation_id,
                    "trace_id": record.trace.trace_id,
                    "account_id": account_id,
                    "amount_decimal_legacy": format!("{amount:.6}"),
                    "legacy_idempotency_key": format!("reserve:{}", record.invocation_id),
                    "observed": observed.clone(),
                }),
            });
        }
    }

    commands.push(ShadowCommand {
        command_id: Uuid::new_v4(),
        operation_key: operation_key(
            workflow_kind,
            record.invocation_id,
            SagaCommandKind::ExecutionCreate,
            "initial",
        )
        .map_err(|error| error.to_string())?,
        command_kind: SagaCommandKind::ExecutionCreate,
        max_attempts: 3,
        payload: json!({
            "schema": "cex.saga.shadow.execution-create.v1",
            "shadow_only": true,
            "invocation_id": record.invocation_id,
            "trace_id": record.trace.trace_id,
            "capability_id": record.request.capability_id.clone(),
            "capability_provider": record.request.capability_provider.clone(),
            "capability_provider_ref": record.request.capability_provider_ref.clone(),
            "prompt_bytes": record.request.prompt.len(),
            "observed": observed,
        }),
    });

    Ok(commands)
}

async fn persist_shadow_commands(
    state: &AppState,
    record: &InvocationRecord,
) -> Result<(), String> {
    let Some(pool) = &state.pool else {
        if state.fail_fast {
            return Err("gateway postgres pool not initialized".to_string());
        }
        return Ok(());
    };

    let commands = shadow_commands_for_record(record)?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin saga shadow transaction: {error}"))?;

    for command in commands {
        let payload = serde_json::to_string(&command.payload)
            .map_err(|error| format!("encode saga shadow payload: {error}"))?;
        sqlx::query(
            "insert into cex_saga_commands_v1 (
                command_id,
                workflow_kind,
                workflow_id,
                org_id,
                operation_key,
                command_kind,
                execution_mode,
                status,
                max_attempts,
                payload
             ) values ($1, 'invocation', $2, $3::uuid, $4, $5, 'shadow', 'pending', $6, $7::jsonb)
             on conflict (operation_key) do nothing",
        )
        .bind(command.command_id)
        .bind(record.invocation_id)
        .bind(&record.request.org_id)
        .bind(&command.operation_key)
        .bind(command.command_kind.as_str())
        .bind(command.max_attempts)
        .bind(payload)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("insert saga shadow command: {error}"))?;
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit saga shadow transaction: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::invocation::{InvocationExecutionState, InvocationRequest};
    use shared_types::{ExecutionDispatchMode, ExecutionStatus, TraceContext};

    fn record(reserve_amount: Option<f64>) -> InvocationRecord {
        InvocationRecord {
            invocation_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            trace: TraceContext {
                trace_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
                created_at: chrono::Utc::now(),
            },
            status: ExecutionStatus::Queued,
            request: InvocationRequest {
                org_id: "00000000-0000-0000-0000-00000000ce01".to_string(),
                actor_id: Some("actor-1".to_string()),
                capability_id: Some("capability-1".to_string()),
                capability_provider: Some("ollama".to_string()),
                capability_provider_ref: Some("model-1".to_string()),
                account_id: reserve_amount.map(|_| {
                    Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap()
                }),
                prompt: "do not duplicate prompt content into shadow payload".to_string(),
                reserve_amount,
            },
            ledger_reserved: reserve_amount.is_some(),
            ledger_refunded: false,
            approval_required: false,
            execution_id: Some(
                Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap(),
            ),
            execution: Some(InvocationExecutionState {
                dispatch_mode: ExecutionDispatchMode::QueuedWorker,
                attempt_count: 0,
                max_attempts: 3,
                attempts_remaining: 3,
                retry_budget_exhausted: false,
            }),
            policy_reason: None,
            failure_reason: None,
        }
    }

    #[test]
    fn reserve_invocation_produces_two_deterministic_shadow_operations() {
        let commands = shadow_commands_for_record(&record(Some(1.25))).unwrap();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].command_kind, SagaCommandKind::LedgerReserve);
        assert_eq!(
            commands[0].operation_key,
            "invocation:11111111-1111-4111-8111-111111111111:ledger_reserve:initial"
        );
        assert_eq!(commands[0].payload["amount_decimal_legacy"], "1.250000");
        assert_eq!(commands[1].command_kind, SagaCommandKind::ExecutionCreate);
        assert!(commands[1].payload.get("prompt").is_none());
    }

    #[test]
    fn invocation_without_reserve_only_models_execution_create() {
        let commands = shadow_commands_for_record(&record(None)).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_kind, SagaCommandKind::ExecutionCreate);
    }
}
