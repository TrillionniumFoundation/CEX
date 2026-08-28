use reqwest::{Client, Response, StatusCode};
use serde_json::Value;
use shared_types::ledger_v2::{LedgerEffectRequestV1, LedgerOperationKind};
use sqlx::PgPool;
use std::{env, fmt};
use uuid::Uuid;

const MODE_ENV: &str = "CEX_EXECUTION_LEDGER_MODE";
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionLedgerMode {
    LegacyV1,
    Dual,
    RequireV2,
}

impl ExecutionLedgerMode {
    pub fn from_env() -> Result<Self, String> {
        parse_mode(env::var(MODE_ENV).ok().as_deref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementAction {
    Consume,
    Refund,
}

impl SettlementAction {
    pub const fn operation_kind(self) -> LedgerOperationKind {
        match self {
            Self::Consume => LedgerOperationKind::Consume,
            Self::Refund => LedgerOperationKind::Refund,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SettlementOutcome {
    UseLegacyV1 {
        reason: String,
    },
    Applied {
        replayed: bool,
        receipt: Value,
    },
    RetryableExactReplay {
        code: String,
        http_status: Option<u16>,
    },
    ReconcileRequired {
        code: String,
        http_status: Option<u16>,
    },
    Rejected {
        code: String,
        http_status: Option<u16>,
    },
}

impl SettlementOutcome {
    pub const fn is_terminal_success(&self) -> bool {
        matches!(self, Self::Applied { .. })
    }
}

#[derive(Clone)]
pub struct ExecutionLedgerSettlementAdapter {
    pool: PgPool,
    http: Client,
    ledger_base_url: String,
    ledger_manage_token: String,
    mode: ExecutionLedgerMode,
}

impl fmt::Debug for ExecutionLedgerSettlementAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionLedgerSettlementAdapter")
            .field("ledger_base_url", &self.ledger_base_url)
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

impl ExecutionLedgerSettlementAdapter {
    pub fn new(
        pool: PgPool,
        http: Client,
        ledger_base_url: String,
        ledger_manage_token: String,
        mode: ExecutionLedgerMode,
    ) -> Result<Self, String> {
        let ledger_base_url = ledger_base_url.trim().trim_end_matches('/').to_string();
        if ledger_base_url.is_empty() {
            return Err("ledger_base_url must not be empty".to_string());
        }
        if ledger_manage_token.trim().is_empty() {
            return Err("ledger_manage_token must not be empty".to_string());
        }
        Ok(Self {
            pool,
            http,
            ledger_base_url,
            ledger_manage_token,
            mode,
        })
    }

    pub const fn mode(&self) -> ExecutionLedgerMode {
        self.mode
    }

    pub async fn settle_invocation(
        &self,
        invocation_id: Uuid,
        action: SettlementAction,
    ) -> SettlementOutcome {
        if invocation_id.is_nil() {
            return SettlementOutcome::Rejected {
                code: "invalid_invocation_id".to_string(),
                http_status: None,
            };
        }
        if matches!(self.mode, ExecutionLedgerMode::LegacyV1) {
            return SettlementOutcome::UseLegacyV1 {
                reason: "execution ledger mode is legacy_v1".to_string(),
            };
        }

        let request = match load_contract_request(&self.pool, invocation_id, action).await {
            Ok(request) => request,
            Err(ContractLoadError::Missing) if matches!(self.mode, ExecutionLedgerMode::Dual) => {
                return SettlementOutcome::UseLegacyV1 {
                    reason: "Invocation has no durable exact Ledger contract".to_string(),
                };
            }
            Err(ContractLoadError::Missing) => {
                return SettlementOutcome::Rejected {
                    code: "exact_invocation_ledger_contract_required".to_string(),
                    http_status: None,
                };
            }
            Err(ContractLoadError::InvalidState) => {
                return SettlementOutcome::Rejected {
                    code: "invalid_invocation_ledger_contract_state".to_string(),
                    http_status: None,
                };
            }
            Err(ContractLoadError::Unavailable) => {
                return SettlementOutcome::RetryableExactReplay {
                    code: "invocation_ledger_contract_unavailable".to_string(),
                    http_status: None,
                };
            }
            Err(ContractLoadError::InvalidPayload) => {
                return SettlementOutcome::ReconcileRequired {
                    code: "invocation_ledger_contract_payload_invalid".to_string(),
                    http_status: None,
                };
            }
        };

        if request.operation_kind != action.operation_kind() {
            return SettlementOutcome::ReconcileRequired {
                code: "invocation_ledger_contract_operation_mismatch".to_string(),
                http_status: None,
            };
        }
        if let Err(error) = request.validate(true) {
            return SettlementOutcome::ReconcileRequired {
                code: format!("invocation_ledger_contract_{}", error.code()),
                http_status: None,
            };
        }

        let response = self
            .http
            .post(format!("{}/v2/ledger/effects", self.ledger_base_url))
            .header("x-admin-token", &self.ledger_manage_token)
            .json(&request)
            .send()
            .await;

        let response = match response {
            Ok(response) => response,
            Err(error) if error.is_connect() => {
                return SettlementOutcome::RetryableExactReplay {
                    code: "ledger_v2_connect_failed".to_string(),
                    http_status: None,
                };
            }
            Err(error) if error.is_timeout() => {
                return SettlementOutcome::ReconcileRequired {
                    code: "ledger_v2_timeout_unknown_outcome".to_string(),
                    http_status: None,
                };
            }
            Err(_) => {
                return SettlementOutcome::ReconcileRequired {
                    code: "ledger_v2_transport_unknown_outcome".to_string(),
                    http_status: None,
                };
            }
        };

        evaluate_response(response, &request).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContractLoadError {
    Missing,
    InvalidState,
    Unavailable,
    InvalidPayload,
}

async fn load_contract_request(
    pool: &PgPool,
    invocation_id: Uuid,
    action: SettlementAction,
) -> Result<LedgerEffectRequestV1, ContractLoadError> {
    let raw = sqlx::query_scalar::<_, String>(
        "select public.cex_invocation_ledger_effect_request_v1($1, $2)::text",
    )
    .bind(invocation_id)
    .bind(action.operation_kind().as_str())
    .fetch_one(pool)
    .await
    .map_err(classify_contract_query_error)?;

    serde_json::from_str::<LedgerEffectRequestV1>(&raw)
        .map_err(|_| ContractLoadError::InvalidPayload)
}

fn classify_contract_query_error(error: sqlx::Error) -> ContractLoadError {
    let Some(database_error) = error.as_database_error() else {
        return ContractLoadError::Unavailable;
    };
    let message = database_error.message().to_ascii_lowercase();
    if database_error.code().as_deref() == Some("P0002") || message.contains("not found") {
        ContractLoadError::Missing
    } else if message.contains("requires a reserved contract")
        || message.contains("invalid after terminal settlement")
        || message.contains("unsupported invocation ledger operation")
    {
        ContractLoadError::InvalidState
    } else {
        ContractLoadError::Unavailable
    }
}

async fn evaluate_response(
    response: Response,
    request: &LedgerEffectRequestV1,
) -> SettlementOutcome {
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return SettlementOutcome::ReconcileRequired {
            code: "ledger_v2_response_too_large".to_string(),
            http_status: Some(status.as_u16()),
        };
    }

    let body = match read_bounded_body(response).await {
        Ok(body) => body,
        Err(code) => {
            return SettlementOutcome::ReconcileRequired {
                code: code.to_string(),
                http_status: Some(status.as_u16()),
            };
        }
    };
    let document = match serde_json::from_slice::<Value>(&body) {
        Ok(document) => document,
        Err(_) => {
            return if status.is_success() {
                SettlementOutcome::ReconcileRequired {
                    code: "ledger_v2_success_body_invalid".to_string(),
                    http_status: Some(status.as_u16()),
                }
            } else {
                classify_http_failure(status, None)
            };
        }
    };

    if status.is_success() {
        match validate_success_receipt(request, &document) {
            Ok(replayed) => SettlementOutcome::Applied {
                replayed,
                receipt: document,
            },
            Err(_) => SettlementOutcome::ReconcileRequired {
                code: "ledger_v2_invalid_success_receipt".to_string(),
                http_status: Some(status.as_u16()),
            },
        }
    } else {
        classify_http_failure(status, Some(&document))
    }
}

async fn read_bounded_body(mut response: Response) -> Result<Vec<u8>, &'static str> {
    let mut body = Vec::new();
    loop {
        let chunk = response
            .chunk()
            .await
            .map_err(|_| "ledger_v2_response_read_failed")?;
        let Some(chunk) = chunk else {
            break;
        };
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err("ledger_v2_response_too_large");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn classify_http_failure(status: StatusCode, document: Option<&Value>) -> SettlementOutcome {
    let code = document
        .and_then(|value| value.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("ledger_v2_upstream_rejected")
        .to_string();
    let http_status = Some(status.as_u16());

    if matches!(status.as_u16(), 408 | 425 | 429) || status.is_server_error() {
        SettlementOutcome::RetryableExactReplay { code, http_status }
    } else if matches!(status.as_u16(), 400 | 401 | 403 | 404 | 409 | 422) {
        SettlementOutcome::Rejected { code, http_status }
    } else {
        SettlementOutcome::ReconcileRequired { code, http_status }
    }
}

fn validate_success_receipt(
    request: &LedgerEffectRequestV1,
    document: &Value,
) -> Result<bool, &'static str> {
    let replayed = document
        .get("replayed")
        .and_then(Value::as_bool)
        .ok_or("missing replayed")?;

    require_uuid(document, "/effect/account_id", request.account_id)?;
    require_uuid(
        document,
        "/effect/trace_id",
        request.trace_id.ok_or("request trace missing")?,
    )?;
    require_uuid(
        document,
        "/effect/operation_id",
        request.operation_id.ok_or("request operation missing")?,
    )?;
    require_text(
        document,
        "/effect/operation_kind",
        request.operation_kind.as_str(),
    )?;
    require_text(
        document,
        "/effect/idempotency_scope",
        request.idempotency_scope.trim(),
    )?;
    require_text(
        document,
        "/effect/idempotency_key",
        request.idempotency_key.trim(),
    )?;
    require_i64(document, "/effect/amount_minor", request.amount_minor)?;
    require_i64(
        document,
        "/effect/currency_scale",
        i64::from(request.currency_scale),
    )?;
    require_text(
        document,
        "/account/currency_unit",
        request.currency_unit.trim(),
    )?;
    require_i64(
        document,
        "/account/currency_scale",
        i64::from(request.currency_scale),
    )?;
    Ok(replayed)
}

fn require_uuid(document: &Value, pointer: &str, expected: Uuid) -> Result<(), &'static str> {
    let raw = document
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or("UUID field missing")?;
    let parsed = Uuid::parse_str(raw).map_err(|_| "UUID field invalid")?;
    if parsed == expected {
        Ok(())
    } else {
        Err("UUID field mismatch")
    }
}

fn require_text(document: &Value, pointer: &str, expected: &str) -> Result<(), &'static str> {
    let actual = document
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or("text field missing")?;
    if actual == expected {
        Ok(())
    } else {
        Err("text field mismatch")
    }
}

fn require_i64(document: &Value, pointer: &str, expected: i64) -> Result<(), &'static str> {
    let value = document.pointer(pointer).ok_or("integer field missing")?;
    let actual = value
        .as_i64()
        .or_else(|| value.as_str().and_then(|raw| raw.parse::<i64>().ok()))
        .ok_or("integer field invalid")?;
    if actual == expected {
        Ok(())
    } else {
        Err("integer field mismatch")
    }
}

fn parse_mode(raw: Option<&str>) -> Result<ExecutionLedgerMode, String> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(ExecutionLedgerMode::LegacyV1),
        Some(value) => match value.to_ascii_lowercase().as_str() {
            "legacy_v1" | "legacy" => Ok(ExecutionLedgerMode::LegacyV1),
            "dual" | "prefer_v2" => Ok(ExecutionLedgerMode::Dual),
            "require_v2" | "v2_only" => Ok(ExecutionLedgerMode::RequireV2),
            _ => Err(format!(
                "{MODE_ENV} must be legacy_v1, dual, or require_v2"
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> LedgerEffectRequestV1 {
        LedgerEffectRequestV1 {
            account_id: Uuid::new_v4(),
            trace_id: Some(Uuid::new_v4()),
            operation_id: Some(Uuid::new_v4()),
            operation_kind: LedgerOperationKind::Consume,
            currency_unit: "credit".to_string(),
            currency_scale: 6,
            amount_minor: 1_250_000,
            reference_type: Some("invocation".to_string()),
            reference_id: Some(Uuid::new_v4()),
            idempotency_scope: "org:test:invocation:test".to_string(),
            idempotency_key: "consume".to_string(),
        }
    }

    fn receipt(request: &LedgerEffectRequestV1) -> Value {
        json!({
            "replayed": true,
            "account": {
                "account_id": request.account_id,
                "currency_unit": request.currency_unit,
                "currency_scale": request.currency_scale,
                "balance_minor": 10_000_000,
                "reserved_minor": 0
            },
            "effect": {
                "entry_id": Uuid::new_v4(),
                "account_id": request.account_id,
                "trace_id": request.trace_id,
                "operation_id": request.operation_id,
                "operation_kind": request.operation_kind.as_str(),
                "idempotency_scope": request.idempotency_scope,
                "idempotency_key": request.idempotency_key,
                "amount_minor": request.amount_minor,
                "currency_scale": request.currency_scale
            }
        })
    }

    #[test]
    fn mode_defaults_to_legacy_and_supports_staged_cutover() {
        assert_eq!(parse_mode(None).unwrap(), ExecutionLedgerMode::LegacyV1);
        assert_eq!(
            parse_mode(Some("dual")).unwrap(),
            ExecutionLedgerMode::Dual
        );
        assert_eq!(
            parse_mode(Some("v2_only")).unwrap(),
            ExecutionLedgerMode::RequireV2
        );
        assert!(parse_mode(Some("maybe")).is_err());
    }

    #[test]
    fn verified_receipt_accepts_exact_replay_and_rejects_operation_drift() {
        let request = request();
        assert_eq!(
            validate_success_receipt(&request, &receipt(&request)).unwrap(),
            true
        );

        let mut drifted = receipt(&request);
        drifted["effect"]["operation_id"] = Value::String(Uuid::new_v4().to_string());
        assert!(validate_success_receipt(&request, &drifted).is_err());
    }

    #[test]
    fn status_classification_preserves_exact_replay_and_reconciliation_boundaries() {
        assert!(matches!(
            classify_http_failure(StatusCode::SERVICE_UNAVAILABLE, None),
            SettlementOutcome::RetryableExactReplay { .. }
        ));
        assert!(matches!(
            classify_http_failure(StatusCode::CONFLICT, None),
            SettlementOutcome::Rejected { .. }
        ));
        assert!(matches!(
            classify_http_failure(StatusCode::IM_A_TEAPOT, None),
            SettlementOutcome::ReconcileRequired { .. }
        ));
    }
}
