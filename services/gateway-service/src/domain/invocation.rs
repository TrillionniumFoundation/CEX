use serde::{Deserialize, Serialize};
use shared_types::{
    AuthContext, CapabilityRecord, ExecutionDispatchMode, ExecutionStatus, TraceContext,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInvocationBody {
    pub capability_id: Option<String>,
    pub account_id: Option<Uuid>,
    pub prompt: String,
    /// Legacy major-unit input. New callers must use the exact reserve ingress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserve_amount: Option<f64>,
}

impl CreateInvocationBody {
    pub fn has_legacy_reserve(&self) -> bool {
        self.reserve_amount.is_some()
    }

    pub fn with_auth(
        self,
        auth: &AuthContext,
        capability: Option<&CapabilityRecord>,
    ) -> InvocationRequest {
        InvocationRequest {
            org_id: auth.org_id.clone(),
            actor_id: auth.actor_id.clone(),
            capability_id: self.capability_id,
            capability_provider: capability.map(|record| record.provider.clone()),
            capability_provider_ref: capability.map(|record| record.provider_ref.clone()),
            account_id: self.account_id,
            prompt: self.prompt,
            reserve_amount: self.reserve_amount,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationRequest {
    pub org_id: String,
    pub actor_id: Option<String>,
    pub capability_id: Option<String>,
    pub capability_provider: Option<String>,
    pub capability_provider_ref: Option<String>,
    pub account_id: Option<Uuid>,
    pub prompt: String,
    /// Legacy major-unit input retained only for an explicitly governed
    /// compatibility path. Exact reserve commands use the durable 0066
    /// Invocation Ledger contract instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserve_amount: Option<f64>,
}

impl InvocationRequest {
    pub fn has_legacy_reserve(&self) -> bool {
        self.reserve_amount.is_some()
    }
}

pub const LEGACY_RESERVE_REJECTION_CODE: &str = "legacy_reserve_requires_exact_ingress";
pub const LEGACY_RESERVE_REJECTION_MESSAGE: &str =
    "reserve_amount is a legacy floating-point field; omit it and register an exact reserve through POST /v2/invocations/:invocation_id/exact-reserve";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationExecutionState {
    pub dispatch_mode: ExecutionDispatchMode,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub attempts_remaining: i32,
    pub retry_budget_exhausted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationRecord {
    pub invocation_id: Uuid,
    pub trace: TraceContext,
    pub status: ExecutionStatus,
    pub request: InvocationRequest,
    pub ledger_reserved: bool,
    pub ledger_refunded: bool,
    pub approval_required: bool,
    pub execution_id: Option<Uuid>,
    pub execution: Option<InvocationExecutionState>,
    pub policy_reason: Option<String>,
    pub failure_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_monetary_invocation_payload_omits_legacy_reserve_key() {
        let request = InvocationRequest {
            org_id: "00000000-0000-0000-0000-00000000ce01".to_string(),
            actor_id: None,
            capability_id: None,
            capability_provider: None,
            capability_provider_ref: None,
            account_id: None,
            prompt: "skeleton".to_string(),
            reserve_amount: None,
        };

        let encoded = serde_json::to_value(request).expect("encode invocation request");
        assert!(encoded.get("reserve_amount").is_none());
    }

    #[test]
    fn legacy_reserve_presence_is_detected_even_for_zero() {
        let body = CreateInvocationBody {
            capability_id: None,
            account_id: None,
            prompt: "legacy".to_string(),
            reserve_amount: Some(0.0),
        };
        assert!(body.has_legacy_reserve());
    }
}
