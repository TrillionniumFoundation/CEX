use super::*;
use async_trait::async_trait;
use term_exchange_protocol::{
    ActorRef, AssetRef, EconomicIntent, EconomicIntentKind, EconomicReceipt, IdempotencyKey,
    ReceiptStatus, SettlementBackendKind, CEX_SETTLEMENT_BACKEND_ID,
    TERM_EXCHANGE_PROTOCOL_VERSION,
};

pub(super) const TERM_EXCHANGE_BACKEND_ADAPTER_CONTRACT_VERSION: &str =
    "trillionnium_term_exchange_backend_adapter_v1";

#[derive(Debug, Clone)]
pub(super) struct TermExchangeLedgerActionRequest {
    pub(super) term_id: String,
    pub(super) term_version: String,
    pub(super) domain: String,
    pub(super) intent_id: String,
    pub(super) intent_kind: EconomicIntentKind,
    pub(super) room_id: Option<String>,
    pub(super) matrix_user_id: String,
    pub(super) message: String,
    pub(super) failure_context: String,
    pub(super) ledger_action: String,
    pub(super) success_status: String,
    pub(super) idempotency_key: String,
    pub(super) idempotency_scope: String,
    pub(super) reference_id: Option<String>,
    pub(super) amount: f64,
    pub(super) amount_credits: i64,
    pub(super) currency: String,
    pub(super) metadata: Value,
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
        let _receipt_progression_allowed = self.receipt.allows_world_progression();
        LeagueLedgerSettlement {
            status: self.raw_status,
            account_id: self.account_id,
            entry_id: self.entry_id,
            balance_after: self.balance_after,
            error: self.error,
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
    if request.amount_credits <= 0 || request.amount <= 0.0 {
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
    let Some(ledger_admin_token) = state.config().ledger_admin_token.clone() else {
        return backend_receipt(
            request,
            "skipped_missing_ledger_token",
            Some(account_id),
            None,
            None,
            Some("consumer-entry ledger admin token is not configured".to_string()),
            None,
            json!({}),
        );
    };

    let url = format!(
        "{}/v1/ledger/{}",
        state.config().ledger_base_url.trim_end_matches('/'),
        request.ledger_action
    );
    let mut body = Map::new();
    body.insert("account_id".to_string(), json!(account_id));
    body.insert("amount".to_string(), json!(request.amount));
    body.insert(
        "idempotency_key".to_string(),
        json!(request.idempotency_key),
    );
    if let Some(reference_id) = request.reference_id.as_deref() {
        body.insert("reference_id".to_string(), json!(reference_id));
    }
    for (key, value) in request.extra_ledger_body.clone() {
        body.insert(key, value);
    }
    let body_value = Value::Object(body.clone());

    let response = match state
        .inner
        .http
        .post(url)
        .header("x-admin-token", ledger_admin_token)
        .json(&body_value)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return backend_receipt(
                request,
                "failed_network",
                body.get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                None,
                None,
                Some(format!("failed to reach ledger-service: {err}")),
                None,
                json!({ "ledger_request": body_value }),
            );
        }
    };

    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return backend_receipt(
                request,
                "failed_bad_response",
                body.get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                None,
                None,
                Some(format!("ledger-service returned non-json response: {err}")),
                None,
                json!({ "ledger_request": body_value }),
            );
        }
    };

    if !status.is_success() {
        let error = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("ledger action failed");
        return backend_receipt(
            request,
            if status.as_u16() == 409 {
                "duplicate"
            } else {
                "failed_ledger"
            },
            body.get("account_id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            None,
            None,
            Some(format!("{}: {error}", status.as_u16())),
            Some(value),
            json!({ "ledger_request": body_value }),
        );
    }

    backend_receipt(
        request,
        "__success__",
        value
            .get("account")
            .and_then(|account| account.get("account_id"))
            .and_then(Value::as_str)
            .or_else(|| body.get("account_id").and_then(Value::as_str))
            .map(ToString::to_string),
        value
            .get("entry")
            .and_then(|entry| entry.get("entry_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        value
            .get("account")
            .and_then(|account| account.get("balance"))
            .and_then(Value::as_f64),
        None,
        Some(value),
        json!({ "ledger_request": body_value }),
    )
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
        map.insert("intent".to_string(), json!(intent));
        map.insert(
            "ledger_response".to_string(),
            ledger_response.unwrap_or(Value::Null),
        );
        map.insert("progression_class".to_string(), json!(progression_class));
    }
    let mut receipt = EconomicReceipt::new(
        format!("receipt:{}", request.intent_id),
        request.intent_id.clone(),
        request.term_id.clone(),
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
}
