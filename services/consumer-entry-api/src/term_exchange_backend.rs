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
    /// Compatibility/display value only. Exact settlement authority is `amount_credits`.
    pub(super) amount: f64,
    pub(super) amount_credits: i64,
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
    let amount_minor = match exact_minor_from_compatibility_amount(
        request.amount,
        TERM_EXCHANGE_EXACT_CURRENCY_SCALE,
    ) {
        Ok(value) => value,
        Err(error) => {
            return backend_receipt(
                request,
                "failed_ledger",
                None,
                None,
                None,
                Some(error),
                None,
                json!({"amount_conversion": "rejected"}),
            )
        }
    };
    if amount_minor <= 0 {
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
                Ok(Some(value)) => exact_success_receipt(request, value, body_value, true),
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

    exact_success_receipt(request, value, body_value, false)
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
        Ok(Some(value)) => exact_success_receipt(request, value, body_value, true),
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

fn exact_success_receipt(
    request: TermExchangeLedgerActionRequest,
    value: Value,
    ledger_request: Value,
    recovered_by_lookup: bool,
) -> TermExchangeBackendReceipt {
    let replayed = value
        .get("replayed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let account = value.get("account");
    let effect = value.get("effect");
    let scale = account
        .and_then(|account| account.get("currency_scale"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u8;
    let balance_after = account
        .and_then(|account| account.get("balance_minor"))
        .and_then(value_as_i64)
        .map(|minor| minor_to_f64(minor, scale));
    let compatibility_metadata = request.extra_ledger_body.clone();
    backend_receipt(
        request,
        if replayed { "duplicate" } else { "__success__" },
        account
            .and_then(|account| account.get("account_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        effect
            .and_then(|effect| effect.get("entry_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        balance_after,
        None,
        Some(value),
        json!({
            "ledger_request": ledger_request,
            "recovered_by_operation_lookup": recovered_by_lookup,
            "legacy_display_metadata": compatibility_metadata,
        }),
    )
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|raw| raw.parse::<i64>().ok()))
}

fn minor_to_f64(value: i64, scale: u8) -> f64 {
    value as f64 / 10_i64.pow(u32::from(scale)) as f64
}

fn exact_minor_from_compatibility_amount(value: f64, scale: u8) -> Result<i64, String> {
    if !value.is_finite() || value < 0.0 {
        return Err("settlement amount must be a finite non-negative value".to_string());
    }
    let factor = 10_i64
        .checked_pow(u32::from(scale))
        .ok_or_else(|| "settlement currency scale is unsupported".to_string())?;
    let scaled = value * factor as f64;
    let rounded = scaled.round();
    let tolerance = f64::EPSILON * scaled.abs().max(1.0) * 16.0;
    if (scaled - rounded).abs() > tolerance {
        return Err(format!(
            "settlement amount {value} exceeds configured scale {scale}"
        ));
    }
    if rounded > i64::MAX as f64 {
        return Err("settlement amount exceeds signed minor-unit range".to_string());
    }
    Ok(rounded as i64)
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
            amount: 42.0,
            amount_credits: 42,
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
    fn compatibility_amount_is_converted_to_exact_minor_units() {
        assert_eq!(
            exact_minor_from_compatibility_amount(4.24, 6).unwrap(),
            4_240_000
        );
        assert_eq!(
            exact_minor_from_compatibility_amount(0.000_001, 6).unwrap(),
            1
        );
        assert!(exact_minor_from_compatibility_amount(0.000_000_1, 6).is_err());
        assert!(exact_minor_from_compatibility_amount(f64::NAN, 6).is_err());
    }

    #[test]
    fn exact_operation_and_reference_ids_are_stable() {
        assert_eq!(deterministic_uuid("same"), deterministic_uuid("same"));
        assert_ne!(deterministic_uuid("same"), deterministic_uuid("other"));
    }
}
