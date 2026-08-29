use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{error::Error, fmt};
use uuid::Uuid;

pub const AUDIT_EVENT_V2_SCHEMA: &str = "cex.audit.event.v2";
pub const AUDIT_WRITER_AUTH_SCHEME_V1: &str = "workload-token-v1";
const MAX_COMPONENT_LEN: usize = 128;
const MAX_ACTOR_ID_LEN: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEventCreateRequestV2 {
    pub event_id: Uuid,
    pub trace_id: Uuid,
    pub org_id: Option<Uuid>,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub event_type: String,
    pub schema_version: String,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
}

impl AuditEventCreateRequestV2 {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), AuditV2ContractError> {
        if self.event_id.is_nil() {
            return Err(AuditV2ContractError::NilIdentifier("event_id"));
        }
        if self.trace_id.is_nil() {
            return Err(AuditV2ContractError::NilIdentifier("trace_id"));
        }
        validate_component("actor_type", &self.actor_type)?;
        validate_component("event_type", &self.event_type)?;
        validate_component("schema_version", &self.schema_version)?;
        if let Some(actor_id) = self.actor_id.as_deref() {
            let actor_id = actor_id.trim();
            if actor_id.is_empty() || actor_id.chars().count() > MAX_ACTOR_ID_LEN {
                return Err(AuditV2ContractError::InvalidActorId);
            }
        }
        if !self.payload.is_object() {
            return Err(AuditV2ContractError::PayloadMustBeObject);
        }
        if self.payload.get("_cex_audit_writer").is_some() {
            return Err(AuditV2ContractError::ReservedPayloadKey(
                "_cex_audit_writer",
            ));
        }
        if self.occurred_at > now + Duration::minutes(5) {
            return Err(AuditV2ContractError::OccurredAtTooFarInFuture);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEventRecordV2 {
    pub event_id: Uuid,
    pub trace_id: Uuid,
    pub org_id: Option<Uuid>,
    pub chain_key: String,
    pub tenant_sequence: i64,
    pub previous_event_hash: Option<String>,
    pub event_hash: String,
    pub writer_service_id: String,
    pub writer_auth_scheme: String,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub event_type: String,
    pub schema_version: String,
    pub occurred_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditV2ContractError {
    NilIdentifier(&'static str),
    InvalidComponent { field: &'static str, value: String },
    InvalidActorId,
    PayloadMustBeObject,
    ReservedPayloadKey(&'static str),
    OccurredAtTooFarInFuture,
}

impl fmt::Display for AuditV2ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NilIdentifier(field) => write!(formatter, "{field} must not be the nil UUID"),
            Self::InvalidComponent { field, value } => {
                write!(formatter, "invalid audit {field} component '{value}'")
            }
            Self::InvalidActorId => write!(
                formatter,
                "actor_id must contain between 1 and {MAX_ACTOR_ID_LEN} characters"
            ),
            Self::PayloadMustBeObject => formatter.write_str("audit payload must be a JSON object"),
            Self::ReservedPayloadKey(key) => {
                write!(
                    formatter,
                    "audit payload contains server-reserved key '{key}'"
                )
            }
            Self::OccurredAtTooFarInFuture => {
                formatter.write_str("occurred_at is more than five minutes in the future")
            }
        }
    }
}

impl Error for AuditV2ContractError {}

fn validate_component(field: &'static str, raw: &str) -> Result<(), AuditV2ContractError> {
    let value = raw.trim();
    if value.is_empty()
        || value.len() > MAX_COMPONENT_LEN
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':')
        })
    {
        return Err(AuditV2ContractError::InvalidComponent {
            field,
            value: value.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> AuditEventCreateRequestV2 {
        AuditEventCreateRequestV2 {
            event_id: Uuid::new_v4(),
            trace_id: Uuid::new_v4(),
            org_id: Some(Uuid::new_v4()),
            actor_type: "policy-engine".to_string(),
            actor_id: Some("operator-1".to_string()),
            event_type: "execution.approved".to_string(),
            schema_version: AUDIT_EVENT_V2_SCHEMA.to_string(),
            occurred_at: Utc::now(),
            payload: json!({"execution_id": Uuid::new_v4()}),
        }
    }

    #[test]
    fn valid_event_contract_is_accepted() {
        assert!(request().validate(Utc::now()).is_ok());
    }

    #[test]
    fn reserved_writer_metadata_is_rejected() {
        let mut request = request();
        request.payload = json!({"_cex_audit_writer": {"service_id": "spoofed"}});
        assert!(matches!(
            request.validate(Utc::now()),
            Err(AuditV2ContractError::ReservedPayloadKey(_))
        ));
    }

    #[test]
    fn nil_ids_scalar_payload_and_future_time_are_rejected() {
        let mut request = request();
        request.event_id = Uuid::nil();
        assert!(request.validate(Utc::now()).is_err());
        request = self::request();
        request.payload = json!("scalar");
        assert!(request.validate(Utc::now()).is_err());
        request = self::request();
        request.occurred_at = Utc::now() + Duration::minutes(6);
        assert!(request.validate(Utc::now()).is_err());
    }
}
