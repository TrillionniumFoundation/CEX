use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub mod money;
pub mod saga;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    pub trace_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyResolveRequest {
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub org_id: String,
    pub actor_id: Option<String>,
    pub actor_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Created,
    Queued,
    PolicyCheckPending,
    AwaitingApproval,
    Approved,
    Dispatching,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    Refunded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionDispatchMode {
    #[default]
    Manual,
    Immediate,
    QueuedWorker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventCreateRequest {
    pub trace_id: Uuid,
    pub org_id: Option<String>,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub event_type: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventRecord {
    pub event_id: Uuid,
    pub trace_id: Uuid,
    pub org_id: Option<String>,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub event_type: String,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityRecord {
    pub capability_id: String,
    pub kind: String,
    pub provider: String,
    pub provider_ref: String,
    pub display_name: String,
    pub version: String,
    pub description: Option<String>,
    pub enabled: bool,
}
