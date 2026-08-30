use axum::{
    extract::{rejection::QueryRejection, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use term_exchange_protocol::{
    EconomicIntent, EconomicReceipt, SettlementBackendKind, CEX_SETTLEMENT_BACKEND_ID,
};

use crate::state::AppState;

pub const RECEIPT_LOOKUP_CONTRACT_VERSION: &str = "trnm_cex_settlement_receipt_lookup_v1";
const GAME_AUTHORITY_HEADER: &str = "x-trnm-game-authority";
const INTENT_HASH_HEADER: &str = "x-trnm-intent-sha256";
const MAX_INTENT_ID_BYTES: usize = 512;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptLookupQuery {
    intent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptLookupResponse {
    pub contract_version: String,
    pub intent_id: String,
    pub intent_hash: String,
    pub receipt: EconomicReceipt,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ReceiptLookupErrorResponse {
    contract_version: &'static str,
    error: ReceiptLookupErrorDetail,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ReceiptLookupErrorDetail {
    code: &'static str,
    message: &'static str,
}

#[derive(Debug, Clone)]
struct StoredReceiptBinding {
    payload_hash: Option<String>,
    intent_json: Option<Value>,
    native_event_present: bool,
    receipt_id: Option<String>,
    receipt_json: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LookupFailure {
    InvalidQuery,
    InvalidIntentId,
    MissingIntentHash,
    InvalidIntentHash,
    Unauthorized,
    NotFound,
    ImmutableConflict,
    ReceiptNotFinalized,
    CorruptBinding,
    DatabaseUnavailable,
}

impl LookupFailure {
    fn status(self) -> StatusCode {
        match self {
            Self::InvalidQuery
            | Self::InvalidIntentId
            | Self::MissingIntentHash
            | Self::InvalidIntentHash => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::ImmutableConflict => StatusCode::CONFLICT,
            Self::ReceiptNotFinalized | Self::CorruptBinding | Self::DatabaseUnavailable => {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::InvalidQuery => "invalid_lookup_query",
            Self::InvalidIntentId => "invalid_intent_id",
            Self::MissingIntentHash => "intent_hash_required",
            Self::InvalidIntentHash => "invalid_intent_hash",
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "intent_receipt_not_found",
            Self::ImmutableConflict => "immutable_intent_hash_conflict",
            Self::ReceiptNotFinalized => "receipt_not_finalized",
            Self::CorruptBinding => "receipt_binding_corrupt",
            Self::DatabaseUnavailable => "receipt_lookup_unavailable",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::InvalidQuery => "exactly one intent_id query parameter is required",
            Self::InvalidIntentId => {
                "intent_id must be a non-empty canonical identifier of at most 512 bytes"
            }
            Self::MissingIntentHash => "x-trnm-intent-sha256 is required",
            Self::InvalidIntentHash => {
                "x-trnm-intent-sha256 must contain exactly 64 lowercase hexadecimal characters"
            }
            Self::Unauthorized => "a valid x-trnm-game-authority credential is required",
            Self::NotFound => "no durable intent or receipt exists under this intent_id",
            Self::ImmutableConflict => {
                "intent_id is durably bound to a different immutable payload hash"
            }
            Self::ReceiptNotFinalized => {
                "the intent exists, but its durable settlement receipt is not finalized"
            }
            Self::CorruptBinding => {
                "the durable intent/receipt binding failed integrity validation"
            }
            Self::DatabaseUnavailable => "the authoritative receipt store is unavailable",
        }
    }

    fn into_response(self) -> Response {
        (
            self.status(),
            Json(ReceiptLookupErrorResponse {
                contract_version: RECEIPT_LOOKUP_CONTRACT_VERSION,
                error: ReceiptLookupErrorDetail {
                    code: self.code(),
                    message: self.message(),
                },
            }),
        )
            .into_response()
    }
}

pub async fn get_trnm_economic_receipt_by_intent(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<ReceiptLookupQuery>, QueryRejection>,
) -> Response {
    if !authorized_game_authority(&headers, state.game_authority_token.as_bytes()) {
        return LookupFailure::Unauthorized.into_response();
    }

    let Query(query) = match query {
        Ok(query) => query,
        Err(_) => return LookupFailure::InvalidQuery.into_response(),
    };
    if !valid_intent_id(&query.intent_id) {
        return LookupFailure::InvalidIntentId.into_response();
    }

    let requested_hash = match single_header(&headers, INTENT_HASH_HEADER) {
        Some(value) if valid_sha256_hex(value) => value,
        Some(_) => return LookupFailure::InvalidIntentHash.into_response(),
        None => return LookupFailure::MissingIntentHash.into_response(),
    };

    let Some(pool) = state.operation_pool.as_ref() else {
        return LookupFailure::DatabaseUnavailable.into_response();
    };

    let row = sqlx::query_as::<
        _,
        (
            Option<String>,
            Option<Value>,
            bool,
            Option<String>,
            Option<Value>,
        ),
    >(
        "select
             (select payload_hash from public.trnm_economic_intents where intent_id = $1),
             (select intent_json from public.trnm_economic_intents where intent_id = $1),
             coalesce(native_event.present, false) as native_event_present,
             case
                 when native_event.present is true then native_event.receipt_id
                 else legacy_receipt.receipt_id
             end as receipt_id,
             case
                 when native_event.present is true then native_event.receipt_json
                 else legacy_receipt.receipt_json
             end as receipt_json
          from (select 1) as lookup_anchor
          left join lateral (
              select true as present, e.receipt_id, e.receipt_json
                from public.trnm_economic_receipt_events_v1 e
               where e.intent_id = $1
               order by e.event_sequence desc, e.event_id desc
               limit 1
          ) as native_event on true
          left join lateral (
              select receipt_id, receipt_json
                from public.trnm_economic_receipts
               where intent_id = $1
               limit 1
          ) as legacy_receipt on native_event.present is not true",
    )
    .bind(&query.intent_id)
    .fetch_one(pool)
    .await;

    let (payload_hash, intent_json, native_event_present, receipt_id, receipt_json) = match row {
        Ok(row) => row,
        Err(_) => return LookupFailure::DatabaseUnavailable.into_response(),
    };

    match resolve_binding(
        &query.intent_id,
        requested_hash,
        StoredReceiptBinding {
            payload_hash,
            intent_json,
            native_event_present,
            receipt_id,
            receipt_json,
        },
    ) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(failure) => failure.into_response(),
    }
}

fn resolve_binding(
    intent_id: &str,
    requested_hash: &str,
    binding: StoredReceiptBinding,
) -> Result<ReceiptLookupResponse, LookupFailure> {
    let StoredReceiptBinding {
        payload_hash,
        intent_json,
        native_event_present,
        receipt_id,
        receipt_json,
    } = binding;

    if !native_event_present
        && payload_hash.is_none()
        && intent_json.is_none()
        && receipt_id.is_none()
        && receipt_json.is_none()
    {
        return Err(LookupFailure::NotFound);
    }
    // A native event is one immutable row authority.  Never combine a
    // partially-corrupt native row with a compatibility projection column;
    // doing so could make an old receipt appear valid under a new event id.
    if native_event_present && receipt_id.is_none() && receipt_json.is_none() {
        return Err(LookupFailure::CorruptBinding);
    }

    let stored_hash = payload_hash.ok_or(LookupFailure::CorruptBinding)?;
    let intent_json = intent_json.ok_or(LookupFailure::CorruptBinding)?;
    if sha256_json(&intent_json)? != stored_hash {
        return Err(LookupFailure::CorruptBinding);
    }
    if stored_hash != requested_hash {
        return Err(LookupFailure::ImmutableConflict);
    }

    let intent: EconomicIntent =
        serde_json::from_value(intent_json).map_err(|_| LookupFailure::CorruptBinding)?;
    if intent.validate().is_err() || intent.intent_id != intent_id {
        return Err(LookupFailure::CorruptBinding);
    }

    match (&receipt_id, &receipt_json) {
        (None, None) => return Err(LookupFailure::ReceiptNotFinalized),
        (Some(_), Some(_)) => {}
        _ => return Err(LookupFailure::CorruptBinding),
    }

    let receipt_id = receipt_id.expect("receipt_id checked above");
    let receipt: EconomicReceipt =
        serde_json::from_value(receipt_json.expect("receipt_json checked above"))
            .map_err(|_| LookupFailure::CorruptBinding)?;
    if receipt.validate_for(&intent).is_err() {
        return Err(LookupFailure::CorruptBinding);
    }

    let expected_amount = fail_closed_receipt_amount(&intent);
    let receipt_amount = receipt
        .evidence
        .get("amount_credits")
        .and_then(Value::as_i64);
    if receipt.receipt_id != receipt_id
        || receipt.backend_id != CEX_SETTLEMENT_BACKEND_ID
        || receipt.backend_kind != SettlementBackendKind::Cex
        || receipt.evidence.get("payload_hash").and_then(Value::as_str)
            != Some(stored_hash.as_str())
        || receipt_amount != Some(expected_amount)
    {
        return Err(LookupFailure::CorruptBinding);
    }

    Ok(ReceiptLookupResponse {
        contract_version: RECEIPT_LOOKUP_CONTRACT_VERSION.to_string(),
        intent_id: intent_id.to_string(),
        intent_hash: stored_hash,
        receipt,
    })
}

/// Mirror the native writer and migration fail-closed evidence rule.
///
/// Invalid negative compatibility amounts never become value authority. The
/// immutable intent bytes and hash remain unchanged, while receipt evidence
/// records zero so lookup/recovery agrees with both live writes and 0086
/// backfill rows.
fn fail_closed_receipt_amount(intent: &EconomicIntent) -> i64 {
    intent.amount_credits.unwrap_or_default().max(0)
}

fn sha256_json(value: &Value) -> Result<String, LookupFailure> {
    let bytes = serde_json::to_vec(value).map_err(|_| LookupFailure::CorruptBinding)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn valid_intent_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_INTENT_ID_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    Some(value)
}

fn authorized_game_authority(headers: &HeaderMap, expected: &[u8]) -> bool {
    let Some(supplied) = single_header(headers, GAME_AUTHORITY_HEADER) else {
        return false;
    };
    !supplied.is_empty() && constant_time_eq(supplied.as_bytes(), expected)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let maximum = left.len().max(right.len());
    for index in 0..maximum {
        let left_byte = left.get(index).copied().unwrap_or_default();
        let right_byte = right.get(index).copied().unwrap_or_default();
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_router, repository::postgres::PostgresLedgerRepository};
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::json;
    use term_exchange_protocol::{ReceiptStatus, TERM_EXCHANGE_PROTOCOL_VERSION};
    use tower::ServiceExt;

    fn stored_binding_with_amount(
        intent_id: &str,
        intent_amount: i64,
        evidence_amount: i64,
        status: ReceiptStatus,
    ) -> (String, StoredReceiptBinding) {
        let intent_json = json!({
            "actors": [{"actor_id": "player-a", "actor_kind": "player", "account_id": "00000000-0000-0000-0000-000000000001"}],
            "amount_credits": intent_amount,
            "assets": [],
            "created_at_epoch": 1,
            "currency": "credit",
            "domain": "trnm_game",
            "idempotency_key": {"key": intent_id, "scope": "test"},
            "intent_id": intent_id,
            "kind": "release_reward",
            "metadata": {},
            "protocol_version": TERM_EXCHANGE_PROTOCOL_VERSION,
            "term_id": "term-a",
            "term_version": "1"
        });
        let payload_hash = sha256_json(&intent_json).expect("hash fixture");
        let mut receipt = EconomicReceipt::new(
            format!("receipt:{intent_id}"),
            intent_id,
            "term-a",
            CEX_SETTLEMENT_BACKEND_ID,
            SettlementBackendKind::Cex,
            status,
            1,
        );
        receipt.evidence = json!({
            "payload_hash": payload_hash.clone(),
            "amount_credits": evidence_amount
        });
        (
            payload_hash.clone(),
            StoredReceiptBinding {
                payload_hash: Some(payload_hash),
                intent_json: Some(intent_json),
                native_event_present: false,
                receipt_id: Some(receipt.receipt_id.clone()),
                receipt_json: Some(serde_json::to_value(receipt).expect("receipt fixture")),
            },
        )
    }

    fn stored_binding(intent_id: &str) -> (String, StoredReceiptBinding) {
        stored_binding_with_amount(intent_id, 25, 25, ReceiptStatus::ApprovedRelease)
    }

    async fn error_code(response: Response) -> String {
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        serde_json::from_slice::<Value>(&body).expect("JSON response")["error"]["code"]
            .as_str()
            .expect("error code")
            .to_string()
    }

    #[test]
    fn hash_and_intent_contracts_are_canonical() {
        assert!(valid_sha256_hex(&"a".repeat(64)));
        assert!(!valid_sha256_hex(&"A".repeat(64)));
        assert!(!valid_sha256_hex(&"a".repeat(63)));
        assert!(valid_intent_id("world:settlement-1"));
        assert!(!valid_intent_id(" world:settlement-1"));
        assert!(!valid_intent_id(""));
    }

    #[test]
    fn exact_binding_returns_the_stored_receipt() {
        let intent_id = "world:settlement-1";
        let (payload_hash, binding) = stored_binding(intent_id);
        let result = resolve_binding(intent_id, &payload_hash, binding).expect("exact binding");
        assert_eq!(result.contract_version, RECEIPT_LOOKUP_CONTRACT_VERSION);
        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.intent_hash, payload_hash);
        assert_eq!(result.receipt.receipt_id, format!("receipt:{intent_id}"));
    }

    #[test]
    fn negative_legacy_amount_uses_same_fail_closed_evidence_amount_as_writer() {
        let intent_id = "world:negative-legacy-amount";
        let (payload_hash, binding) =
            stored_binding_with_amount(intent_id, -25, 0, ReceiptStatus::SkippedZeroReward);
        let result = resolve_binding(intent_id, &payload_hash, binding)
            .expect("negative legacy amount evidence must remain readable");
        assert_eq!(result.receipt.evidence["amount_credits"].as_i64(), Some(0));
    }

    #[test]
    fn typed_intent_receipt_and_backend_binding_fail_closed() {
        let (payload_hash, mut progression_mismatch) = stored_binding("progression-mismatch");
        progression_mismatch.receipt_json.as_mut().expect("receipt")["progression_class"] =
            json!("recoverable_hold");
        assert_eq!(
            resolve_binding("progression-mismatch", &payload_hash, progression_mismatch)
                .unwrap_err(),
            LookupFailure::CorruptBinding
        );

        let (payload_hash, mut wrong_backend) = stored_binding("wrong-backend");
        wrong_backend.receipt_json.as_mut().expect("receipt")["backend_id"] = json!("not-cex");
        assert_eq!(
            resolve_binding("wrong-backend", &payload_hash, wrong_backend).unwrap_err(),
            LookupFailure::CorruptBinding
        );

        let (_, mut malformed_intent) = stored_binding("malformed-intent");
        malformed_intent.intent_json.as_mut().expect("intent")["domain"] = json!("not-trnm-game");
        let payload_hash =
            sha256_json(malformed_intent.intent_json.as_ref().expect("intent")).expect("hash");
        malformed_intent.payload_hash = Some(payload_hash.clone());
        malformed_intent.receipt_json.as_mut().expect("receipt")["evidence"]["payload_hash"] =
            json!(payload_hash.clone());
        assert_eq!(
            resolve_binding("malformed-intent", &payload_hash, malformed_intent).unwrap_err(),
            LookupFailure::CorruptBinding
        );
    }

    #[test]
    fn missing_conflicting_and_incomplete_bindings_are_distinct() {
        let empty = StoredReceiptBinding {
            payload_hash: None,
            intent_json: None,
            native_event_present: false,
            receipt_id: None,
            receipt_json: None,
        };
        assert_eq!(
            resolve_binding("missing", &"a".repeat(64), empty).unwrap_err(),
            LookupFailure::NotFound
        );

        let (payload_hash, binding) = stored_binding("collision");
        assert_ne!(payload_hash, "b".repeat(64));
        assert_eq!(
            resolve_binding("collision", &"b".repeat(64), binding).unwrap_err(),
            LookupFailure::ImmutableConflict
        );

        let (payload_hash, mut pending) = stored_binding("pending");
        pending.receipt_id = None;
        pending.receipt_json = None;
        assert_eq!(
            resolve_binding("pending", &payload_hash, pending).unwrap_err(),
            LookupFailure::ReceiptNotFinalized
        );
    }

    #[test]
    fn corrupted_hash_or_receipt_binding_never_degrades_to_not_found() {
        let (payload_hash, mut binding) = stored_binding("corrupt");
        binding.payload_hash = Some("0".repeat(64));
        assert_eq!(
            resolve_binding("corrupt", &payload_hash, binding).unwrap_err(),
            LookupFailure::CorruptBinding
        );

        let (payload_hash, mut binding) = stored_binding("wrong-receipt");
        binding.receipt_json.as_mut().expect("receipt")["intent_id"] = json!("other");
        assert_eq!(
            resolve_binding("wrong-receipt", &payload_hash, binding).unwrap_err(),
            LookupFailure::CorruptBinding
        );
    }

    #[test]
    fn native_event_pair_never_falls_back_to_legacy_columns() {
        let (payload_hash, mut binding) = stored_binding("native-pair-corrupt");
        binding.native_event_present = true;
        binding.receipt_id = None;
        binding.receipt_json = None;
        assert_eq!(
            resolve_binding("native-pair-corrupt", &payload_hash, binding).unwrap_err(),
            LookupFailure::CorruptBinding
        );
    }

    #[tokio::test]
    async fn route_fails_closed_and_database_outage_is_not_a_404() {
        let state = AppState::new_for_tests(
            PostgresLedgerRepository::new_placeholder(),
            false,
            None,
            Vec::new(),
            Vec::new(),
        );
        let app = build_router(state);
        let hash = "a".repeat(64);

        let missing_credential = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/trnm/economy/receipts/by-intent?intent_id=test")
                    .header(INTENT_HASH_HEADER, hash.as_str())
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(missing_credential.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(error_code(missing_credential).await, "unauthorized");

        let malformed_hash = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/trnm/economy/receipts/by-intent?intent_id=test")
                    .header(GAME_AUTHORITY_HEADER, "test-game-authority-token")
                    .header(INTENT_HASH_HEADER, "ABC")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(malformed_hash.status(), StatusCode::BAD_REQUEST);
        assert_eq!(error_code(malformed_hash).await, "invalid_intent_hash");

        let database_unavailable = app
            .oneshot(
                Request::builder()
                    .uri("/v1/trnm/economy/receipts/by-intent?intent_id=test")
                    .header(GAME_AUTHORITY_HEADER, "test-game-authority-token")
                    .header(INTENT_HASH_HEADER, hash.as_str())
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(
            database_unavailable.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_ne!(database_unavailable.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            error_code(database_unavailable).await,
            "receipt_lookup_unavailable"
        );
    }
}
