use serde_json::Value;
use shared_types::{
    ledger_v2::{LedgerEffectRequestV1, LedgerOperationKind},
    money::MoneyAmount,
};
use std::{env, fmt};
use uuid::Uuid;

use super::clients::ServiceClients;

const MODE_ENV: &str = "CEX_GATEWAY_LEDGER_MODE";
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayLedgerMode {
    LegacyV1,
    Dual,
    RequireV2,
}

impl GatewayLedgerMode {
    pub fn from_env() -> Result<Self, String> {
        parse_mode(env::var(MODE_ENV).ok().as_deref())
    }

    pub const fn exact_effects_enabled(self) -> bool {
        !matches!(self, Self::LegacyV1)
    }

    pub const fn legacy_writes_allowed(self) -> bool {
        !matches!(self, Self::RequireV2)
    }
}

#[derive(Debug, Clone)]
pub struct LedgerV2ClientError {
    pub status: Option<u16>,
    pub code: String,
    pub message: String,
}

impl fmt::Display for LedgerV2ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for LedgerV2ClientError {}

impl ServiceClients {
    pub async fn apply_ledger_effect_v2(
        &self,
        request: &LedgerEffectRequestV1,
    ) -> Result<Value, LedgerV2ClientError> {
        request
            .validate(false)
            .map_err(|error| LedgerV2ClientError {
                status: None,
                code: error.code().to_string(),
                message: error.to_string(),
            })?;

        let response = self
            .http
            .post(format!("{}/v2/ledger/effects", self.ledger_base_url))
            .header("x-admin-token", &self.ledger_manage_token)
            .json(request)
            .send()
            .await
            .map_err(|error| LedgerV2ClientError {
                status: None,
                code: "ledger_v2_transport_failed".to_string(),
                message: format!("request failed: {error}"),
            })?;

        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err(LedgerV2ClientError {
                status: Some(status.as_u16()),
                code: "ledger_v2_response_too_large".to_string(),
                message: "ledger response exceeds the 64 KiB contract".to_string(),
            });
        }
        let body = response.text().await.map_err(|error| LedgerV2ClientError {
            status: Some(status.as_u16()),
            code: "ledger_v2_response_read_failed".to_string(),
            message: format!("read response failed: {error}"),
        })?;
        if body.len() > MAX_RESPONSE_BYTES as usize {
            return Err(LedgerV2ClientError {
                status: Some(status.as_u16()),
                code: "ledger_v2_response_too_large".to_string(),
                message: "ledger response exceeds the 64 KiB contract".to_string(),
            });
        }

        let document: Value = serde_json::from_str(&body).map_err(|error| LedgerV2ClientError {
            status: Some(status.as_u16()),
            code: "ledger_v2_response_invalid".to_string(),
            message: format!("decode response failed: {error}"),
        })?;
        if !status.is_success() {
            return Err(LedgerV2ClientError {
                status: Some(status.as_u16()),
                code: document
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("ledger_v2_upstream_rejected")
                    .to_string(),
                message: document
                    .get("message")
                    .or_else(|| document.get("error"))
                    .and_then(Value::as_str)
                    .unwrap_or("ledger v2 operation rejected")
                    .to_string(),
            });
        }
        Ok(document)
    }
}

pub fn invocation_ledger_effect(
    org_id: &str,
    account_id: Uuid,
    invocation_id: Uuid,
    trace_id: Uuid,
    operation_kind: LedgerOperationKind,
    amount: MoneyAmount,
) -> Result<LedgerEffectRequestV1, String> {
    let request = LedgerEffectRequestV1 {
        account_id,
        trace_id: Some(trace_id),
        operation_id: None,
        operation_kind,
        currency_unit: amount.currency,
        currency_scale: amount.scale,
        amount_minor: amount.minor_units,
        reference_type: Some("invocation".to_string()),
        reference_id: Some(invocation_id),
        idempotency_scope: format!("org:{org_id}:invocation:{invocation_id}"),
        idempotency_key: operation_kind.as_str().to_string(),
    };
    request
        .validate(true)
        .map_err(|error| format!("invalid invocation ledger effect: {error}"))?;
    Ok(request)
}

fn parse_mode(raw: Option<&str>) -> Result<GatewayLedgerMode, String> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(GatewayLedgerMode::LegacyV1),
        Some(value) => match value.to_ascii_lowercase().as_str() {
            "legacy_v1" | "legacy" => Ok(GatewayLedgerMode::LegacyV1),
            "dual" | "prefer_v2" => Ok(GatewayLedgerMode::Dual),
            "require_v2" | "v2_only" => Ok(GatewayLedgerMode::RequireV2),
            _ => Err(format!("{MODE_ENV} must be legacy_v1, dual, or require_v2")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_defaults_to_legacy_and_exposes_cutover_semantics() {
        assert_eq!(parse_mode(None).unwrap(), GatewayLedgerMode::LegacyV1);
        assert!(parse_mode(Some("dual")).unwrap().exact_effects_enabled());
        assert!(!parse_mode(Some("require_v2"))
            .unwrap()
            .legacy_writes_allowed());
        assert!(parse_mode(Some("maybe")).is_err());
    }

    #[test]
    fn invocation_builder_preserves_exact_minor_units_and_trace() {
        let account_id = Uuid::new_v4();
        let invocation_id = Uuid::new_v4();
        let trace_id = Uuid::new_v4();
        let request = invocation_ledger_effect(
            "00000000-0000-4000-8000-00000000ce01",
            account_id,
            invocation_id,
            trace_id,
            LedgerOperationKind::Reserve,
            MoneyAmount::credits(1_250_000),
        )
        .unwrap();

        assert_eq!(request.amount_minor, 1_250_000);
        assert_eq!(request.trace_id, Some(trace_id));
        assert_eq!(request.reference_id, Some(invocation_id));
        assert_eq!(request.idempotency_key, "reserve");
    }
}
