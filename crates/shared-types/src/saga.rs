use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{error::Error, fmt};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SagaCommandKind {
    LedgerReserve,
    ExecutionCreate,
    ProviderDispatch,
    LedgerConsume,
    LedgerRefund,
    AuditDeliver,
    Reconcile,
}

impl SagaCommandKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LedgerReserve => "ledger_reserve",
            Self::ExecutionCreate => "execution_create",
            Self::ProviderDispatch => "provider_dispatch",
            Self::LedgerConsume => "ledger_consume",
            Self::LedgerRefund => "ledger_refund",
            Self::AuditDeliver => "audit_deliver",
            Self::Reconcile => "reconcile",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SagaCommandStatus {
    Pending,
    Claimed,
    RetryWait,
    Succeeded,
    Failed,
    DeadLetter,
    Cancelled,
}

impl SagaCommandStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::DeadLetter | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaCommandEnvelope {
    pub command_id: Uuid,
    pub workflow_kind: String,
    pub workflow_id: Uuid,
    pub org_id: Option<Uuid>,
    pub operation_key: String,
    pub command_kind: SagaCommandKind,
    pub status: SagaCommandStatus,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub available_at: DateTime<Utc>,
    pub claimed_by: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SagaCommandEnvelope {
    pub fn pending(
        workflow_kind: impl Into<String>,
        workflow_id: Uuid,
        org_id: Option<Uuid>,
        command_kind: SagaCommandKind,
        step: impl AsRef<str>,
        max_attempts: i32,
        payload: Value,
    ) -> Result<Self, SagaContractError> {
        let workflow_kind = normalize_component("workflow_kind", workflow_kind.into())?;
        let step = normalize_component("step", step.as_ref().to_string())?;
        if !(1..=100).contains(&max_attempts) {
            return Err(SagaContractError::InvalidAttemptBudget(max_attempts));
        }
        if !payload.is_object() {
            return Err(SagaContractError::PayloadMustBeObject);
        }

        let now = Utc::now();
        Ok(Self {
            command_id: Uuid::new_v4(),
            operation_key: operation_key(&workflow_kind, workflow_id, command_kind, &step)?,
            workflow_kind,
            workflow_id,
            org_id,
            command_kind,
            status: SagaCommandStatus::Pending,
            attempt_count: 0,
            max_attempts,
            available_at: now,
            claimed_by: None,
            lease_expires_at: None,
            payload,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn attempts_remaining(&self) -> i32 {
        (self.max_attempts - self.attempt_count).max(0)
    }

    pub fn claimable_at(&self, now: DateTime<Utc>) -> bool {
        self.attempts_remaining() > 0
            && self.available_at <= now
            && match self.status {
                SagaCommandStatus::Pending | SagaCommandStatus::RetryWait => true,
                SagaCommandStatus::Claimed => self
                    .lease_expires_at
                    .is_some_and(|lease_expires_at| lease_expires_at <= now),
                _ => false,
            }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaReceiptEnvelope {
    pub receipt_id: Uuid,
    pub command_id: Uuid,
    pub source_system: String,
    pub receipt_key: String,
    pub outcome: String,
    pub payload: Value,
    pub observed_at: DateTime<Utc>,
}

impl SagaReceiptEnvelope {
    pub fn new(
        command_id: Uuid,
        source_system: impl Into<String>,
        receipt_key: impl Into<String>,
        outcome: impl Into<String>,
        payload: Value,
    ) -> Result<Self, SagaContractError> {
        if !payload.is_object() {
            return Err(SagaContractError::PayloadMustBeObject);
        }
        Ok(Self {
            receipt_id: Uuid::new_v4(),
            command_id,
            source_system: normalize_component("source_system", source_system.into())?,
            receipt_key: normalize_component("receipt_key", receipt_key.into())?,
            outcome: normalize_component("outcome", outcome.into())?,
            payload,
            observed_at: Utc::now(),
        })
    }
}

pub fn operation_key(
    workflow_kind: &str,
    workflow_id: Uuid,
    command_kind: SagaCommandKind,
    step: &str,
) -> Result<String, SagaContractError> {
    let workflow_kind = normalize_component("workflow_kind", workflow_kind.to_string())?;
    let step = normalize_component("step", step.to_string())?;
    let key = format!(
        "{workflow_kind}:{workflow_id}:{}:{step}",
        command_kind.as_str()
    );
    if key.len() > 256 {
        return Err(SagaContractError::OperationKeyTooLong);
    }
    Ok(key)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SagaContractError {
    InvalidComponent { field: &'static str, value: String },
    InvalidAttemptBudget(i32),
    PayloadMustBeObject,
    OperationKeyTooLong,
}

impl fmt::Display for SagaContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidComponent { field, value } => {
                write!(formatter, "invalid saga {field} component '{value}'")
            }
            Self::InvalidAttemptBudget(value) => {
                write!(formatter, "saga max_attempts must be between 1 and 100, got {value}")
            }
            Self::PayloadMustBeObject => formatter.write_str("saga payload must be a JSON object"),
            Self::OperationKeyTooLong => formatter.write_str("saga operation key exceeds 256 bytes"),
        }
    }
}

impl Error for SagaContractError {}

fn normalize_component(
    field: &'static str,
    value: String,
) -> Result<String, SagaContractError> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(SagaContractError::InvalidComponent { field, value });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    #[test]
    fn operation_key_is_stable_and_step_scoped() {
        let workflow_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let key = operation_key(
            "invocation",
            workflow_id,
            SagaCommandKind::LedgerReserve,
            "initial",
        )
        .unwrap();
        assert_eq!(
            key,
            "invocation:11111111-1111-4111-8111-111111111111:ledger_reserve:initial"
        );
    }

    #[test]
    fn pending_command_has_bounded_attempt_budget() {
        let command = SagaCommandEnvelope::pending(
            "invocation",
            Uuid::new_v4(),
            None,
            SagaCommandKind::ProviderDispatch,
            "provider-1",
            3,
            json!({"provider": "ollama"}),
        )
        .unwrap();
        assert_eq!(command.attempts_remaining(), 3);
        assert!(command.claimable_at(Utc::now() + Duration::seconds(1)));
    }

    #[test]
    fn expired_claim_is_claimable_but_terminal_state_is_not() {
        let mut command = SagaCommandEnvelope::pending(
            "invocation",
            Uuid::new_v4(),
            None,
            SagaCommandKind::LedgerRefund,
            "provider-failure",
            2,
            json!({}),
        )
        .unwrap();
        command.status = SagaCommandStatus::Claimed;
        command.attempt_count = 1;
        command.lease_expires_at = Some(Utc::now() - Duration::seconds(1));
        assert!(command.claimable_at(Utc::now()));
        command.status = SagaCommandStatus::Succeeded;
        assert!(!command.claimable_at(Utc::now()));
    }

    #[test]
    fn payload_and_component_validation_fail_closed() {
        assert!(SagaCommandEnvelope::pending(
            "invocation",
            Uuid::new_v4(),
            None,
            SagaCommandKind::AuditDeliver,
            "event",
            0,
            json!({}),
        )
        .is_err());
        assert!(SagaReceiptEnvelope::new(
            Uuid::new_v4(),
            "audit service",
            "receipt",
            "ok",
            json!({})
        )
        .is_err());
        assert!(SagaReceiptEnvelope::new(
            Uuid::new_v4(),
            "audit-service",
            "receipt",
            "ok",
            json!("not-an-object")
        )
        .is_err());
    }
}
