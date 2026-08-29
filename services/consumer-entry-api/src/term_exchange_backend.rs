use super::*;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use term_exchange_protocol::{
    ActorRef, AssetRef, EconomicIntent, EconomicIntentKind, EconomicReceipt, IdempotencyKey,
    ReceiptStatus, SettlementBackendKind, CEX_SETTLEMENT_BACKEND_ID,
    TERM_EXCHANGE_PROTOCOL_VERSION,
};
use uuid::Uuid;

pub(super) const TERM_EXCHANGE_BACKEND_ADAPTER_CONTRACT_VERSION: &str =
    "trillionnium_term_exchange_backend_adapter_v2";
const TERM_EXCHANGE_EXACT_CURRENCY_SCALE: u8 = 6;
const LEDGER_EFFECT_SCHEMA_V1: &str = "cex.ledger.effect.v1";

#[derive(Debug, Clone)]
pub(super) struct TermExchangeLedgerActionRequest {
    pub(super) term_id: String,
    pub(super) term_version: String,
    pub(super) domain: String,
    pub(super) intent_id: String,
    pub(super) intent_kind: EconomicIntentKind,
    pub(super) room_id: Option<String>,
    pub(super) matrix_user_id: String,
    pub(super) account_id_override: Option<String>,
    pub(super) message: String,
    pub(super) failure_context: String,
    pub(super) ledger_action: String,
    pub(super) success_status: String,
    pub(super) idempotency_key: String,
    pub(super) idempotency_scope: String,
    pub(super) reference_id: Option<String>,
    /// Exact whole-credit settlement authority. A value of zero is used with
    /// `amount_validation_error` when a legacy display amount cannot be resolved exactly; the
    /// adapter defers that failure until room/account preconditions have been evaluated.
    pub(super) amount_credits: i64,
    pub(super) amount_validation_error: Option<String>,
    pub(super) currency: String,
    pub(super) metadata: Value,
    /// Compatibility metadata retained as evidence; never forwarded as value authority.
    pub(super) extra_ledger_body: Map<String, Value>,
}

impl TermExchangeLedgerActionRequest {
    pub(super) fn economic_intent(&self, account_id: Option<String>) -> EconomicIntent {
        EconomicIntent {
            protocol_version: TERM_EXCHANGE_PROTOCOL_VERSION.to_string(),
            intent_id: self.intent_id.clone(),
            term_id: self.term_id.clone(),
            term_version: self.term_version.clone(),
            domain: self.domain.clone(),
            kind: self.intent_kind.clone(),
            idempotency_key: IdempotencyKey {
                scope: self.idempotency_scope.clone(),
                key: self.idempotency_key.clone(),
            },
            actors: vec![ActorRef {
                actor_id: self.matrix_user_id.clone(),
                actor_kind: "matrix_user".to_string(),
                account_id,
            }],
            assets: vec![AssetRef {
                asset_id: self
                    .reference_id
                    .clone()
                    .unwrap_or_else(|| self.intent_id.clone()),
                asset_kind: self.term_id.clone(),
                quantity: self.amount_credits,
                unit: self.currency.clone(),
            }],
            amount_credits: Some(self.amount_credits),
            currency: Some(self.currency.clone()),
            metadata: json!({
                "message": self.message,
                "ledger_action": self.ledger_action,
                "success_status": self.success_status,
                "reference_id": self.reference_id,
                "request_metadata": self.metadata,
            }),
            created_at_epoch: Utc::now().timestamp(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TermExchangeBackendReceipt {
    pub(super) receipt: EconomicReceipt,
    pub(super) raw_status: String,
    pub(super) account_id: Option<String>,
    pub(super) entry_id: Option<String>,
    pub(super) balance_after: Option<f64>,
    pub(super) error: Option<String>,
}

impl TermExchangeBackendReceipt {
    pub(super) fn into_legacy_settlement(self) -> LeagueLedgerSettlement {
        let _receipt_progression_allowed = self.receipt.allows_progression();
        LeagueLedgerSettlement {
            status: self.raw_status,
            account_id: self.account_id,
            entry_id: self.entry_id,
            balance_after: self.balance_after,
            error: self.error,
            term_exchange_receipt: Some(TermExchangeReceiptState::from(&self.receipt)),
        }
    }
}

#[async_trait]
pub(super) trait TermExchangeBackend {
    async fn execute_ledger_action(
        &self,
        state: &AppState,
        request: TermExchangeLedgerActionRequest,
    ) -> TermExchangeBackendReceipt;
}

#[derive(Debug, Clone, Default)]
pub(super) struct CexTermExchangeBackend;

#[async_trait]
impl TermExchangeBackend for CexTermExchangeBackend {
    async fn execute_ledger_action(
        &self,
        state: &AppState,
        request: TermExchangeLedgerActionRequest,
    ) -> TermExchangeBackendReceipt {
        execute_cex_ledger_action(state, request).await
    }
}

async fn execute_cex_ledger_action(
    state: &AppState,
    request: TermExchangeLedgerActionRequest,
) -> TermExchangeBackendReceipt {
    // Preserve the adapter's established precondition precedence: a missing room/account is
    // reported before a rejected legacy display amount. The request still carries no f64 value;
    // callers put any failed compatibility resolution in `amount_validation_error`.
    if request.amount_validation_error.is_none() && request.amount_credits <= 0 {
        let raw_status = if request.term_id.contains("purchase") {
            "skipped_zero_price"
        } else {
            "skipped_zero_reward"
        };
        return backend_receipt(request, raw_status, None, None, None, None, None, json!({}));
    }
    let Some(room_id) = request
        .room_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
    else {
        return backend_receipt(
            request,
            "skipped_missing_room",
            None,
            None,
            None,
            None,
            None,
            json!({}),
        );
    };

    let account_id = if let Some(account_id) = request.account_id_override.clone() {
        account_id
    } else {
        let matrix_payload = MatrixMessageRequest {
            matrix_user_id: request.matrix_user_id.clone(),
            room_id,
            session_id: None,
            org_id: None,
            message: request.message.clone(),
            capability_id: None,
            account_id: None,
            event_id: None,
            idempotency_key: None,
            metadata: None,
        };
        let resolved_identity = match resolve_matrix_identity(state, &matrix_payload).await {
            Ok(identity) => identity,
            Err(_) => {
                let error = request.failure_context.clone();
                return backend_receipt(
                    request,
                    "failed_identity",
                    None,
                    None,
                    None,
                    Some(error),
                    None,
                    json!({}),
                );
            }
        };
        let Some(account_id) = resolved_identity.scope.account_id.clone() else {
            return backend_receipt(
                request,
                "skipped_missing_account",
                None,
                None,
                None,
                Some("matrix identity did not resolve a ledger account_id".to_string()),
                None,
                json!({}),
            );
        };
        account_id
    };
    let account_uuid = match Uuid::parse_str(&account_id) {
        Ok(account_id) if !account_id.is_nil() => account_id,
        _ => {
            return backend_receipt(
                request,
                "failed_ledger",
                Some(account_id),
                None,
                None,
                Some("resolved ledger account_id is not a non-nil UUID".to_string()),
                None,
                json!({}),
            )
        }
    };
    if let Some(error) = request.amount_validation_error.clone() {
        return backend_receipt(
            request,
            "failed_ledger",
            Some(account_uuid.to_string()),
            None,
            None,
            Some(error),
            None,
            json!({
                "amount_authority": "amount_credits",
                "amount_conversion": "rejected",
            }),
        );
    }
    let amount_minor = match whole_credits_to_minor_units(
        request.amount_credits,
        TERM_EXCHANGE_EXACT_CURRENCY_SCALE,
    ) {
        Ok(value) => value,
        Err(error) => {
            return backend_receipt(
                request,
                "failed_ledger",
                Some(account_uuid.to_string()),
                None,
                None,
                Some(error),
                None,
                json!({
                    "amount_authority": "amount_credits",
                    "amount_conversion": "rejected",
                }),
            )
        }
    };
    let Some(ledger_admin_token) = state.config().ledger_admin_token.clone() else {
        return backend_receipt(
            request,
            "skipped_missing_ledger_token",
            Some(account_uuid.to_string()),
            None,
            None,
            Some("consumer-entry ledger admin token is not configured".to_string()),
            None,
            json!({}),
        );
    };
    let operation_kind = match request.ledger_action.as_str() {
        "reserve" | "consume" | "refund" | "grant" => request.ledger_action.as_str(),
        _ => {
            return backend_receipt(
                request,
                "failed_ledger",
                Some(account_uuid.to_string()),
                None,
                None,
                Some("unsupported exact ledger operation".to_string()),
                None,
                json!({}),
            )
        }
    };
    let scope = request.idempotency_scope.trim().to_string();
    let key = request.idempotency_key.trim().to_string();
    let operation_id = deterministic_uuid(&format!("cex:term-exchange-effect:{scope}:{key}"));
    let trace_id = deterministic_uuid(&format!("cex:term-exchange-trace:{}", request.intent_id));
    let reference_source = request
        .reference_id
        .as_deref()
        .unwrap_or(request.intent_id.as_str());
    let reference_id =
        deterministic_uuid(&format!("cex:term-exchange-reference:{reference_source}"));
    let currency = request.currency.trim().to_ascii_lowercase();
    let body_value = json!({
        "account_id": account_uuid,
        "trace_id": trace_id,
        "operation_id": operation_id,
        "operation_kind": operation_kind,
        "currency_unit": currency,
        "currency_scale": TERM_EXCHANGE_EXACT_CURRENCY_SCALE,
        "amount_minor": amount_minor.to_string(),
        "reference_type": "term_exchange",
        "reference_id": reference_id,
        "idempotency_scope": scope,
        "idempotency_key": key,
    });
    let base_url = state.config().ledger_base_url.trim_end_matches('/');
    let url = format!("{base_url}/v2/ledger/effects");

    let response = state
        .inner
        .http
        .post(&url)
        .header("x-admin-token", &ledger_admin_token)
        .json(&body_value)
        .send()
        .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            return match lookup_exact_effect(state, base_url, &ledger_admin_token, operation_id)
                .await
            {
                Ok(Some(value)) => match exact_success_receipt(
                    request.clone(),
                    value.clone(),
                    body_value.clone(),
                    true,
                ) {
                    Ok(receipt) => receipt,
                    Err(validation_error) => malformed_exact_response_receipt(
                        request,
                        account_uuid,
                        value,
                        body_value,
                        validation_error,
                        true,
                    ),
                },
                Ok(None) => backend_receipt(
                    request,
                    "failed_network",
                    Some(account_uuid.to_string()),
                    None,
                    None,
                    Some(format!(
                        "exact ledger request failed and lookup found no effect: {error}"
                    )),
                    None,
                    json!({
                        "ledger_request": body_value,
                        "operation_id": operation_id,
                        "lookup_result": "not_found",
                    }),
                ),
                Err(lookup_error) => backend_receipt(
                    request,
                    "failed_network",
                    Some(account_uuid.to_string()),
                    None,
                    None,
                    Some(format!(
                        "exact ledger outcome is unknown: request={error}; lookup={lookup_error}"
                    )),
                    None,
                    json!({
                        "ledger_request": body_value,
                        "operation_id": operation_id,
                        "lookup_result": "unknown",
                    }),
                ),
            };
        }
    };

    let status = response.status();
    let text = match response.text().await {
        Ok(text) => text,
        Err(error) => {
            return recover_after_bad_response(
                state,
                request,
                base_url,
                &ledger_admin_token,
                operation_id,
                account_uuid,
                body_value,
                format!("read exact ledger response failed: {error}"),
            )
            .await;
        }
    };
    let value = match serde_json::from_str::<Value>(&text) {
        Ok(value) => value,
        Err(error) => {
            return recover_after_bad_response(
                state,
                request,
                base_url,
                &ledger_admin_token,
                operation_id,
                account_uuid,
                body_value,
                format!("exact ledger returned non-json response: {error}"),
            )
            .await;
        }
    };

    if !status.is_success() {
        let code = value
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("ledger_effect_rejected");
        let message = value
            .get("message")
            .or_else(|| value.get("error"))
            .and_then(Value::as_str)
            .unwrap_or("exact ledger action failed");
        return backend_receipt(
            request,
            "failed_ledger",
            Some(account_uuid.to_string()),
            None,
            None,
            Some(format!("{} {code}: {message}", status.as_u16())),
            Some(value),
            json!({
                "ledger_request": body_value,
                "operation_id": operation_id,
                "collision": status == StatusCode::CONFLICT,
            }),
        );
    }

    match exact_success_receipt(request.clone(), value.clone(), body_value.clone(), false) {
        Ok(receipt) => receipt,
        Err(validation_error) => {
            recover_after_bad_response(
                state,
                request,
                base_url,
                &ledger_admin_token,
                operation_id,
                account_uuid,
                body_value,
                format!("exact ledger response failed contract validation: {validation_error}"),
            )
            .await
        }
    }
}

async fn recover_after_bad_response(
    state: &AppState,
    request: TermExchangeLedgerActionRequest,
    base_url: &str,
    ledger_admin_token: &str,
    operation_id: Uuid,
    account_id: Uuid,
    body_value: Value,
    response_error: String,
) -> TermExchangeBackendReceipt {
    match lookup_exact_effect(state, base_url, ledger_admin_token, operation_id).await {
        Ok(Some(value)) => {
            match exact_success_receipt(request.clone(), value.clone(), body_value.clone(), true) {
                Ok(receipt) => receipt,
                Err(validation_error) => malformed_exact_response_receipt(
                    request,
                    account_id,
                    value,
                    body_value,
                    validation_error,
                    true,
                ),
            }
        }
        Ok(None) => backend_receipt(
            request,
            "failed_bad_response",
            Some(account_id.to_string()),
            None,
            None,
            Some(format!("{response_error}; exact lookup returned not found")),
            None,
            json!({
                "ledger_request": body_value,
                "operation_id": operation_id,
                "lookup_result": "not_found",
            }),
        ),
        Err(lookup_error) => backend_receipt(
            request,
            "failed_bad_response",
            Some(account_id.to_string()),
            None,
            None,
            Some(format!(
                "{response_error}; exact lookup failed: {lookup_error}"
            )),
            None,
            json!({
                "ledger_request": body_value,
                "operation_id": operation_id,
                "lookup_result": "unknown",
            }),
        ),
    }
}

async fn lookup_exact_effect(
    state: &AppState,
    base_url: &str,
    ledger_admin_token: &str,
    operation_id: Uuid,
) -> Result<Option<Value>, String> {
    let response = state
        .inner
        .http
        .get(format!("{base_url}/v2/ledger/effects/{operation_id}"))
        .header("x-admin-token", ledger_admin_token)
        .send()
        .await
        .map_err(|error| format!("effect lookup request failed: {error}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("read effect lookup response failed: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "effect lookup returned {}: {text}",
            status.as_u16()
        ));
    }
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| format!("decode effect lookup response failed: {error}"))
}

#[derive(Debug, Clone)]
struct ExactLedgerResponseFields {
    replayed: bool,
    account_id: Uuid,
    entry_id: Uuid,
    balance_minor: i64,
    currency_scale: u8,
}

/// Decode and authenticate the exact Ledger v2 response against the request we sent.
///
/// The consumer still stores a legacy floating-point display field for compatibility, but that
/// field is derived only after every exact response invariant has been checked. A malformed or
/// cross-account response must never be treated as a successful settlement.
fn validate_exact_ledger_response(
    value: &Value,
    ledger_request: &Value,
) -> Result<ExactLedgerResponseFields, String> {
    let replayed = value
        .get("replayed")
        .and_then(Value::as_bool)
        .ok_or_else(|| "exact ledger response replayed must be a boolean".to_string())?;
    let expected_account_id = required_uuid(ledger_request, "account_id")?;
    let expected_trace_id = required_uuid(ledger_request, "trace_id")?;
    let expected_operation_id = required_uuid(ledger_request, "operation_id")?;
    let expected_reference_id = required_uuid(ledger_request, "reference_id")?;
    let expected_operation_kind = required_text(ledger_request, "operation_kind")?;
    let expected_currency_unit = required_text(ledger_request, "currency_unit")?;
    let expected_currency_scale = required_u8(ledger_request, "currency_scale")?;
    if expected_currency_scale > TERM_EXCHANGE_EXACT_CURRENCY_SCALE {
        return Err(format!(
            "request currency_scale {} exceeds exact contract maximum {}",
            expected_currency_scale, TERM_EXCHANGE_EXACT_CURRENCY_SCALE
        ));
    }
    let expected_amount_minor = required_i64(ledger_request, "amount_minor")?;
    let expected_scope = required_text(ledger_request, "idempotency_scope")?;
    let expected_key = required_text(ledger_request, "idempotency_key")?;
    let expected_reference_type = required_text(ledger_request, "reference_type")?;

    let account = value
        .get("account")
        .and_then(Value::as_object)
        .ok_or_else(|| "exact ledger response account object is missing".to_string())?;
    let account_id = required_uuid_from_object(account, "account_id")?;
    if account_id != expected_account_id {
        return Err(format!(
            "exact ledger response account_id {} does not match request {}",
            account_id, expected_account_id
        ));
    }
    let currency_unit = required_text_from_object(account, "currency_unit")?;
    if currency_unit != expected_currency_unit {
        return Err(format!(
            "exact ledger response currency_unit {currency_unit:?} does not match request {expected_currency_unit:?}"
        ));
    }
    let currency_scale = required_u8_from_object(account, "currency_scale")?;
    if currency_scale > TERM_EXCHANGE_EXACT_CURRENCY_SCALE
        || currency_scale != expected_currency_scale
    {
        return Err(format!(
            "exact ledger response currency_scale {currency_scale} does not match request {expected_currency_scale}"
        ));
    }
    let balance_minor = required_i64_from_object(account, "balance_minor")?;
    let reserved_minor = required_i64_from_object(account, "reserved_minor")?;
    if balance_minor < 0 || reserved_minor < 0 || reserved_minor > balance_minor {
        return Err(
            "exact ledger response account balances violate non-negative invariant".to_string(),
        );
    }

    let effect = value
        .get("effect")
        .and_then(Value::as_object)
        .ok_or_else(|| "exact ledger response effect object is missing".to_string())?;
    let entry_id = required_uuid_from_object(effect, "entry_id")?;
    let effect_account_id = required_uuid_from_object(effect, "account_id")?;
    if effect_account_id != expected_account_id {
        return Err(format!(
            "exact ledger response effect account_id {} does not match request {}",
            effect_account_id, expected_account_id
        ));
    }
    let effect_trace_id = required_uuid_from_object(effect, "trace_id")?;
    if effect_trace_id != expected_trace_id {
        return Err("exact ledger response effect trace_id does not match request".to_string());
    }
    let effect_operation_id = required_uuid_from_object(effect, "operation_id")?;
    if effect_operation_id != expected_operation_id {
        return Err("exact ledger response effect operation_id does not match request".to_string());
    }
    let effect_operation_kind = required_text_from_object(effect, "operation_kind")?;
    if effect_operation_kind != expected_operation_kind {
        return Err(
            "exact ledger response effect operation_kind does not match request".to_string(),
        );
    }
    let effect_scope = required_text_from_object(effect, "idempotency_scope")?;
    if effect_scope != expected_scope {
        return Err(
            "exact ledger response effect idempotency_scope does not match request".to_string(),
        );
    }
    let effect_key = required_text_from_object(effect, "idempotency_key")?;
    if effect_key != expected_key {
        return Err(
            "exact ledger response effect idempotency_key does not match request".to_string(),
        );
    }
    let effect_amount_minor = required_i64_from_object(effect, "amount_minor")?;
    if effect_amount_minor != expected_amount_minor {
        return Err("exact ledger response effect amount_minor does not match request".to_string());
    }
    let effect_scale = required_u8_from_object(effect, "currency_scale")?;
    if effect_scale != expected_currency_scale {
        return Err(
            "exact ledger response effect currency_scale does not match request".to_string(),
        );
    }
    let effect_reference_type = required_text_from_object(effect, "reference_type")?;
    if effect_reference_type != expected_reference_type {
        return Err(
            "exact ledger response effect reference_type does not match request".to_string(),
        );
    }
    let effect_reference_id = required_uuid_from_object(effect, "reference_id")?;
    if effect_reference_id != expected_reference_id {
        return Err("exact ledger response effect reference_id does not match request".to_string());
    }
    if required_text_from_object(effect, "source_service")? != "ledger-service"
        || required_text_from_object(effect, "schema_version")? != LEDGER_EFFECT_SCHEMA_V1
        || required_text_from_object(effect, "provenance_mode")? != "explicit"
    {
        return Err("exact ledger response effect provenance is not canonical".to_string());
    }
    let expected_direction = match expected_operation_kind.as_str() {
        "reserve" | "consume" => "debit",
        "refund" | "grant" => "credit",
        other => return Err(format!("unsupported exact operation_kind {other:?}")),
    };
    if required_text_from_object(effect, "direction")? != expected_direction {
        return Err("exact ledger response effect direction does not match operation".to_string());
    }

    Ok(ExactLedgerResponseFields {
        replayed,
        account_id,
        entry_id,
        balance_minor,
        currency_scale,
    })
}

fn exact_success_receipt(
    request: TermExchangeLedgerActionRequest,
    value: Value,
    ledger_request: Value,
    recovered_by_lookup: bool,
) -> Result<TermExchangeBackendReceipt, String> {
    let fields = validate_exact_ledger_response(&value, &ledger_request)?;
    let compatibility_metadata = request.extra_ledger_body.clone();
    Ok(backend_receipt(
        request,
        if fields.replayed {
            "duplicate"
        } else {
            "__success__"
        },
        Some(fields.account_id.to_string()),
        Some(fields.entry_id.to_string()),
        Some(minor_to_f64(fields.balance_minor, fields.currency_scale)),
        None,
        Some(value),
        json!({
            "ledger_request": ledger_request,
            "recovered_by_operation_lookup": recovered_by_lookup,
            "legacy_display_metadata": compatibility_metadata,
        }),
    ))
}

fn malformed_exact_response_receipt(
    request: TermExchangeLedgerActionRequest,
    account_id: Uuid,
    value: Value,
    ledger_request: Value,
    validation_error: String,
    recovered_by_lookup: bool,
) -> TermExchangeBackendReceipt {
    backend_receipt(
        request,
        "failed_bad_response",
        Some(account_id.to_string()),
        None,
        None,
        Some(format!(
            "exact ledger response failed contract validation: {validation_error}"
        )),
        Some(value),
        json!({
            "ledger_request": ledger_request,
            "recovered_by_operation_lookup": recovered_by_lookup,
            "response_validation": "failed",
        }),
    )
}

fn required_uuid(value: &Value, field: &str) -> Result<Uuid, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "exact ledger request must be an object".to_string())?;
    required_uuid_from_object(object, field)
}

fn required_uuid_from_object(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Uuid, String> {
    let raw = object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("exact ledger response field {field} must be a UUID string"))?;
    let parsed = Uuid::parse_str(raw)
        .map_err(|_| format!("exact ledger response field {field} is not a UUID"))?;
    if parsed.is_nil() {
        return Err(format!(
            "exact ledger response field {field} must be non-nil"
        ));
    }
    Ok(parsed)
}

fn required_text(value: &Value, field: &str) -> Result<String, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "exact ledger request must be an object".to_string())?;
    required_text_from_object(object, field)
}

fn required_text_from_object(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<String, String> {
    let raw = object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("exact ledger response field {field} must be a string"))?;
    if raw.is_empty() {
        return Err(format!(
            "exact ledger response field {field} must not be empty"
        ));
    }
    Ok(raw.to_string())
}

fn required_u8(value: &Value, field: &str) -> Result<u8, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "exact ledger request must be an object".to_string())?;
    required_u8_from_object(object, field)
}

fn required_u8_from_object(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<u8, String> {
    let raw = object.get(field).and_then(Value::as_u64).ok_or_else(|| {
        format!("exact ledger response field {field} must be an unsigned integer")
    })?;
    u8::try_from(raw).map_err(|_| format!("exact ledger response field {field} is out of range"))
}

fn required_i64(value: &Value, field: &str) -> Result<i64, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "exact ledger request must be an object".to_string())?;
    required_i64_from_object(object, field)
}

fn required_i64_from_object(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<i64, String> {
    object
        .get(field)
        .and_then(value_as_i64)
        .ok_or_else(|| format!("exact ledger response field {field} must be an integer"))
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|raw| raw.parse::<i64>().ok()))
}

fn minor_to_f64(value: i64, scale: u8) -> f64 {
    value as f64 / 10_f64.powi(i32::from(scale))
}

/// Resolve a legacy display amount to an exact whole-credit compatibility value.
///
/// This helper is deliberately a narrow ingress shim, not a Ledger amount conversion. The v2
/// term protocol represents value as whole credits, so a fractional or non-finite legacy value is
/// rejected instead of rounded/truncated. Parsing the canonical decimal text avoids a lossy
/// float-to-integer cast; the returned integer is then the sole authority carried by the request.
pub(super) fn whole_credits_from_compatibility_amount(value: f64) -> Result<i64, String> {
    if !value.is_finite() || value < 0.0 {
        return Err("compatibility settlement amount must be finite and non-negative".to_string());
    }
    if value.fract() != 0.0 {
        return Err(
            "fractional compatibility settlement amount requires an exact minor-unit contract"
                .to_string(),
        );
    }
    value
        .to_string()
        .parse::<i64>()
        .map_err(|_| "compatibility settlement amount is outside whole-credit range".to_string())
}

/// Convert an exact whole-credit intent into the account's exact minor-unit scale.
///
/// This is the sole value conversion used by the term-exchange Ledger v2 caller. It performs
/// checked integer arithmetic only; overflow and non-positive intents fail closed.
fn whole_credits_to_minor_units(amount_credits: i64, scale: u8) -> Result<i64, String> {
    if amount_credits < 0 {
        return Err("settlement amount_credits must be non-negative".to_string());
    }
    let factor = 10_i64
        .checked_pow(u32::from(scale))
        .ok_or_else(|| "settlement currency scale is unsupported".to_string())?;
    amount_credits
        .checked_mul(factor)
        .ok_or_else(|| "settlement amount_credits exceeds signed minor-unit range".to_string())
}

fn deterministic_uuid(namespace: &str) -> Uuid {
    let digest = Sha256::digest(namespace.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn backend_receipt(
    request: TermExchangeLedgerActionRequest,
    raw_status: &str,
    account_id: Option<String>,
    entry_id: Option<String>,
    balance_after: Option<f64>,
    error: Option<String>,
    ledger_response: Option<Value>,
    mut evidence: Value,
) -> TermExchangeBackendReceipt {
    let raw_status = if raw_status == "__success__" {
        request.success_status.clone()
    } else {
        raw_status.to_string()
    };
    let intent = request.economic_intent(account_id.clone());
    let typed_status = receipt_status_from_raw(&raw_status);
    let progression_class = typed_status.progression_class();
    if let Some(map) = evidence.as_object_mut() {
        map.insert(
            "adapter_contract_version".to_string(),
            json!(TERM_EXCHANGE_BACKEND_ADAPTER_CONTRACT_VERSION),
        );
        map.insert("raw_status".to_string(), json!(raw_status.clone()));
        map.insert("intent".to_string(), json!(intent.clone()));
        map.insert(
            "ledger_response".to_string(),
            ledger_response.unwrap_or(Value::Null),
        );
        map.insert("progression_class".to_string(), json!(progression_class));
    }
    let mut receipt = EconomicReceipt::from_intent(
        format!("receipt:{}", request.intent_id),
        &intent,
        CEX_SETTLEMENT_BACKEND_ID,
        SettlementBackendKind::Cex,
        typed_status,
        Utc::now().timestamp(),
    );
    receipt.progression_class = progression_class;
    receipt.settlement_reference = request.reference_id;
    receipt.ledger_entry_id = entry_id.clone();
    receipt.reason = error.clone();
    receipt.evidence = evidence;
    TermExchangeBackendReceipt {
        receipt,
        raw_status,
        account_id,
        entry_id,
        balance_after,
        error,
    }
}

pub(super) fn receipt_status_from_raw(raw_status: &str) -> ReceiptStatus {
    match raw_status {
        "reserved" => ReceiptStatus::Reserved,
        "settled" | "reopened_settled" => ReceiptStatus::Settled,
        "consumed" => ReceiptStatus::Consumed,
        "refunded" => ReceiptStatus::Refunded,
        "seller_chargeback_reserved" => ReceiptStatus::SellerChargebackReserved,
        "seller_chargeback_consumed" => ReceiptStatus::SellerChargebackConsumed,
        "approved_release" => ReceiptStatus::ApprovedRelease,
        "duplicate" => ReceiptStatus::Duplicate,
        "held_review" => ReceiptStatus::HeldReview,
        "skipped_zero_price" => ReceiptStatus::SkippedZeroPrice,
        "skipped_zero_reward" => ReceiptStatus::SkippedZeroReward,
        "skipped_zero_seller_net" => ReceiptStatus::SkippedZeroSellerNet,
        "skipped_missing_room" => ReceiptStatus::SkippedMissingRoom,
        "skipped_missing_account" => ReceiptStatus::SkippedMissingAccount,
        "skipped_missing_ledger_token" => ReceiptStatus::SkippedMissingLedgerToken,
        "failed_network" => ReceiptStatus::FailedNetwork,
        "failed_identity" => ReceiptStatus::FailedIdentity,
        "failed_ledger" => ReceiptStatus::FailedLedger,
        "failed_bad_response" => ReceiptStatus::FailedBadResponse,
        "seller_chargeback_reserve_failed" => ReceiptStatus::SellerChargebackReserveFailed,
        "seller_chargeback_failed" => ReceiptStatus::SellerChargebackFailed,
        "rejected_refund_failed" => ReceiptStatus::RejectedRefundFailed,
        "cancelled_refund_failed" => ReceiptStatus::CancelledRefundFailed,
        _ => ReceiptStatus::FailedLedger,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use term_exchange_protocol::ReceiptProgressionClass;

    #[test]
    fn raw_legacy_statuses_map_to_typed_receipt_statuses() {
        assert_eq!(receipt_status_from_raw("settled"), ReceiptStatus::Settled);
        assert_eq!(
            receipt_status_from_raw("duplicate"),
            ReceiptStatus::Duplicate
        );
        assert_eq!(
            receipt_status_from_raw("seller_chargeback_reserved"),
            ReceiptStatus::SellerChargebackReserved
        );
        assert_eq!(
            receipt_status_from_raw("skipped_missing_ledger_token"),
            ReceiptStatus::SkippedMissingLedgerToken
        );
        assert_eq!(
            receipt_status_from_raw("seller_chargeback_failed").progression_class(),
            ReceiptProgressionClass::RecoverableHold
        );
    }

    #[test]
    fn ledger_action_request_projects_economic_intent() {
        let request = TermExchangeLedgerActionRequest {
            term_id: "league_reward_settlement".to_string(),
            term_version: "v1".to_string(),
            domain: "trillionnium_league".to_string(),
            intent_id: "intent:test".to_string(),
            intent_kind: EconomicIntentKind::ReleaseReward,
            room_id: Some("!room:local.dev".to_string()),
            matrix_user_id: "@alice:local.dev".to_string(),
            account_id_override: None,
            message: "test".to_string(),
            failure_context: "test failure".to_string(),
            ledger_action: "grant".to_string(),
            success_status: "settled".to_string(),
            idempotency_key: "league_reward:reward-1".to_string(),
            idempotency_scope: "league_reward".to_string(),
            reference_id: Some("task-1".to_string()),
            amount_credits: 42,
            amount_validation_error: None,
            currency: "credits".to_string(),
            metadata: json!({"source": "test"}),
            extra_ledger_body: Map::new(),
        };
        let intent = request.economic_intent(Some("acct-1".to_string()));
        assert_eq!(intent.protocol_version, TERM_EXCHANGE_PROTOCOL_VERSION);
        assert_eq!(intent.term_id, "league_reward_settlement");
        assert_eq!(intent.kind, EconomicIntentKind::ReleaseReward);
        assert_eq!(intent.idempotency_key.key, "league_reward:reward-1");
        assert_eq!(intent.actors[0].account_id.as_deref(), Some("acct-1"));
        assert_eq!(intent.assets[0].quantity, 42);
    }

    #[test]
    fn exact_whole_credit_authority_uses_checked_scale_multiplication() {
        assert_eq!(whole_credits_to_minor_units(42, 6).unwrap(), 42_000_000);
        assert_eq!(whole_credits_to_minor_units(0, 6).unwrap(), 0);
        assert!(whole_credits_to_minor_units(i64::MAX, 6).is_err());
    }

    #[test]
    fn fractional_compatibility_amount_fails_closed() {
        assert_eq!(whole_credits_from_compatibility_amount(42.0).unwrap(), 42);
        assert!(whole_credits_from_compatibility_amount(4.24).is_err());
        assert!(whole_credits_from_compatibility_amount(0.000_001).is_err());
        assert!(whole_credits_from_compatibility_amount(f64::NAN).is_err());
    }

    #[test]
    fn exact_operation_and_reference_ids_are_stable() {
        assert_eq!(deterministic_uuid("same"), deterministic_uuid("same"));
        assert_ne!(deterministic_uuid("same"), deterministic_uuid("other"));
    }

    fn exact_response_fixture() -> (TermExchangeLedgerActionRequest, Value, Value) {
        let account_id = Uuid::new_v4();
        let trace_id = Uuid::new_v4();
        let operation_id = Uuid::new_v4();
        let reference_id = Uuid::new_v4();
        let entry_id = Uuid::new_v4();
        let request = TermExchangeLedgerActionRequest {
            term_id: "league_reward_settlement".to_string(),
            term_version: "v1".to_string(),
            domain: "trillionnium_league".to_string(),
            intent_id: "intent:exact-response".to_string(),
            intent_kind: EconomicIntentKind::ReleaseReward,
            room_id: Some("!room:local.dev".to_string()),
            matrix_user_id: "@alice:local.dev".to_string(),
            account_id_override: Some(account_id.to_string()),
            message: "test".to_string(),
            failure_context: "test failure".to_string(),
            ledger_action: "grant".to_string(),
            success_status: "settled".to_string(),
            idempotency_key: "league_reward:exact-response".to_string(),
            idempotency_scope: "league_reward".to_string(),
            reference_id: Some("task-exact-response".to_string()),
            amount_credits: 42,
            amount_validation_error: None,
            currency: "credits".to_string(),
            metadata: json!({}),
            extra_ledger_body: Map::new(),
        };
        let ledger_request = json!({
            "account_id": account_id,
            "trace_id": trace_id,
            "operation_id": operation_id,
            "operation_kind": "grant",
            "currency_unit": "credits",
            "currency_scale": 6,
            "amount_minor": "42000000",
            "reference_type": "term_exchange",
            "reference_id": reference_id,
            "idempotency_scope": "league_reward",
            "idempotency_key": "league_reward:exact-response"
        });
        let response = json!({
            "replayed": false,
            "account": {
                "account_id": account_id,
                "org_id": Uuid::new_v4(),
                "currency_unit": "credits",
                "currency_scale": 6,
                "balance_minor": "42000000",
                "reserved_minor": "0"
            },
            "effect": {
                "entry_id": entry_id,
                "account_id": account_id,
                "trace_id": trace_id,
                "operation_id": operation_id,
                "operation_kind": "grant",
                "idempotency_scope": "league_reward",
                "idempotency_key": "league_reward:exact-response",
                "direction": "credit",
                "amount_minor": 42000000,
                "currency_scale": 6,
                "reference_type": "term_exchange",
                "reference_id": reference_id,
                "source_service": "ledger-service",
                "schema_version": LEDGER_EFFECT_SCHEMA_V1,
                "provenance_mode": "explicit"
            }
        });
        (request, ledger_request, response)
    }

    #[test]
    fn exact_response_is_bound_to_request_identity_and_money_contract() {
        let (request, ledger_request, response) = exact_response_fixture();
        let fields = validate_exact_ledger_response(&response, &ledger_request).unwrap();
        assert!(!fields.replayed);
        assert_eq!(fields.currency_scale, 6);
        assert_eq!(fields.balance_minor, 42_000_000);
        let receipt = exact_success_receipt(request, response, ledger_request, false).unwrap();
        assert_eq!(receipt.raw_status, "settled");
        assert!(receipt.entry_id.is_some());
        assert_eq!(receipt.balance_after, Some(42.0));
    }

    #[test]
    fn malformed_exact_response_fails_closed_before_display_conversion() {
        let (request, ledger_request, mut response) = exact_response_fixture();
        response["account"]["currency_scale"] = json!(255);
        let error = exact_success_receipt(request, response, ledger_request, false)
            .expect_err("unsupported response scale must not be accepted");
        assert!(error.contains("currency_scale"));
    }

    #[test]
    fn exact_response_rejects_cross_account_effects() {
        let (request, ledger_request, mut response) = exact_response_fixture();
        response["effect"]["account_id"] = json!(Uuid::new_v4());
        let error = exact_success_receipt(request, response, ledger_request, false)
            .expect_err("cross-account response must not be accepted");
        assert!(error.contains("effect account_id"));
    }
}
