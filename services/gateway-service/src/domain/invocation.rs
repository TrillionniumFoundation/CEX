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
    pub reserve_amount: Option<f64>,
}

impl CreateInvocationBody {
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
    pub reserve_amount: Option<f64>,
}

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
