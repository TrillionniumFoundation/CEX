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
// This evidence key is written only by migration 0086 while converting a
// legacy 0027 receipt into the native event stream.  Native/live writers must
// never be able to use a positive amount when the immutable intent omitted the
// amount key, so the provenance is checked in addition to the numeric value.
const LEGACY_AMOUNT_FALLBACK_EVIDENCE_KEY: &str = "_cex_legacy_amount_fallback";

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

/// The database row that supplies a native receipt is carried as one paired
/// object.  Keeping the immutable columns together with the JSON prevents a
/// list/read path from accidentally combining a latest event with a different
/// receipt or provenance row.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredNativeReceiptEvent {
    event_id: i64,
    intent_id: String,
    event_sequence: i64,
    intent_hash: String,
    receipt_id: String,
    protocol_version: String,
    idempotency_scope: String,
    idempotency_key: String,
    amount_credits: i64,
    receipt_hash: String,
    receipt_hash_actual: Option<String>,
    event_kind: String,
    finalized_at_epoch: i64,
    receipt_json: Value,
    legacy_fallback_provenance: Option<StoredLegacyFallbackProvenance>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredLegacyFallbackProvenance {
    event_id: i64,
    legacy_receipt_id: String,
    legacy_receipt_json_sha256: String,
    // PostgreSQL computes the source hash from jsonb::text.  The query returns
    // that independently recomputed value so Rust can compare it without
    // attempting to reproduce PostgreSQL's jsonb textual canonicalization.
    legacy_receipt_json_sha256_actual: Option<String>,
    legacy_receipt_json: Value,
    intent_hash: String,
    amount_credits: i64,
}

struct LegacyFallbackSourceContext<'a> {
    legacy_receipt_id: &'a str,
    intent_id: &'a str,
    protocol_version: &'a str,
    intent: &'a EconomicIntent,
    receipt: &'a EconomicReceipt,
    amount_credits: i64,
    payload_hash: &'a str,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredReceiptBinding {
    intent_id: Option<String>,
    payload_hash: Option<String>,
    intent_json: Option<Value>,
    native_event: Option<StoredNativeReceiptEvent>,
    // These scalar fields remain as a compatibility seam for pure unit
    // fixtures. Database-backed reads always populate `native_event` and
    // therefore take the full paired validation path below.
    native_event_present: bool,
    native_event_sequence: Option<i64>,
    native_event_kind: Option<String>,
    native_event_legacy_fallback_authorized: bool,
    receipt_id: Option<String>,
    receipt_json: Option<Value>,
}

impl StoredReceiptBinding {
    /// Decode the paired row returned by either the by-intent query or the
    /// administrative list query.  A malformed JSON envelope is a durable
    /// corruption, not an empty/absent receipt.
    pub(crate) fn from_database_parts(
        intent_id: Option<String>,
        payload_hash: Option<String>,
        intent_json: Option<Value>,
        native_event_json: Option<Value>,
        receipt_id: Option<String>,
        receipt_json: Option<Value>,
    ) -> Result<Self, String> {
        let native_event: Option<StoredNativeReceiptEvent> = native_event_json
            .map(|value| {
                serde_json::from_value(value)
                    .map_err(|error| format!("decode native receipt event binding failed: {error}"))
            })
            .transpose()?;
        let native_event_present = native_event.is_some();
        let native_event_sequence = native_event.as_ref().map(|event| event.event_sequence);
        let native_event_kind = native_event.as_ref().map(|event| event.event_kind.clone());
        let native_event_legacy_fallback_authorized = native_event
            .as_ref()
            .and_then(|event| event.legacy_fallback_provenance.as_ref())
            .is_some();
        Ok(Self {
            intent_id,
            payload_hash,
            intent_json,
            native_event,
            native_event_present,
            native_event_sequence,
            native_event_kind,
            native_event_legacy_fallback_authorized,
            receipt_id,
            receipt_json,
        })
    }
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
            Option<String>,
            Option<Value>,
            Option<Value>,
            Option<String>,
            Option<Value>,
        ),
    >(
        "select
             (select intent_id from public.trnm_economic_intents where intent_id = $1),
             (select payload_hash from public.trnm_economic_intents where intent_id = $1),
             (select intent_json from public.trnm_economic_intents where intent_id = $1),
             case
                 when native_event.present is true then jsonb_build_object(
                     'event_id', native_event.event_id,
                     'intent_id', native_event.intent_id,
                     'event_sequence', native_event.event_sequence,
                     'intent_hash', native_event.intent_hash,
                     'receipt_id', native_event.receipt_id,
                     'protocol_version', native_event.protocol_version,
                     'idempotency_scope', native_event.idempotency_scope,
                     'idempotency_key', native_event.idempotency_key,
                     'amount_credits', native_event.amount_credits,
                     'receipt_hash', native_event.receipt_hash,
                     'receipt_hash_actual', native_event.receipt_hash_actual,
                     'event_kind', native_event.event_kind,
                     'finalized_at_epoch', native_event.finalized_at_epoch,
                     'receipt_json', native_event.receipt_json,
                     'legacy_fallback_provenance', native_event.legacy_fallback_provenance
                 )
                 else null::jsonb
             end as native_event_json,
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
              select true as present,
                     e.event_id,
                     e.intent_id,
                     e.event_sequence,
                     e.intent_hash,
                     e.receipt_id,
                     e.protocol_version,
                     e.idempotency_scope,
                     e.idempotency_key,
                     e.amount_credits,
                     e.receipt_hash,
                     encode(digest(e.receipt_json::text, 'sha256'), 'hex') as receipt_hash_actual,
                     e.event_kind,
                     extract(epoch from e.finalized_at)::bigint as finalized_at_epoch,
                     e.receipt_json,
                     (
                         select jsonb_build_object(
                             'event_id', p.event_id,
                             'legacy_receipt_id', p.legacy_receipt_id,
                             'legacy_receipt_json_sha256', p.legacy_receipt_json_sha256,
                             'legacy_receipt_json_sha256_actual',
                                 case when r.receipt_json is null then null
                                      else encode(digest(r.receipt_json::text, 'sha256'), 'hex')
                                 end,
                             'legacy_receipt_json', r.receipt_json,
                             'intent_hash', p.intent_hash,
                             'amount_credits', p.amount_credits
                         )
                           from public.trnm_economic_receipt_legacy_fallback_provenance_v1 p
                           left join public.trnm_economic_receipts r
                             on r.receipt_id = p.legacy_receipt_id
                          where p.event_id = e.event_id
                     ) as legacy_fallback_provenance
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

    let (stored_intent_id, payload_hash, intent_json, native_event_json, receipt_id, receipt_json) =
        match row {
            Ok(row) => row,
            Err(_) => return LookupFailure::DatabaseUnavailable.into_response(),
        };

    let binding = match StoredReceiptBinding::from_database_parts(
        stored_intent_id,
        payload_hash,
        intent_json,
        native_event_json,
        receipt_id,
        receipt_json,
    ) {
        Ok(binding) => binding,
        Err(_) => return LookupFailure::CorruptBinding.into_response(),
    };

    match resolve_binding(&query.intent_id, requested_hash, binding) {
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
        intent_id: stored_intent_id,
        payload_hash,
        intent_json,
        native_event,
        native_event_present,
        native_event_sequence,
        native_event_kind,
        native_event_legacy_fallback_authorized,
        receipt_id,
        receipt_json,
    } = binding;

    if let Some(stored_intent_id) = stored_intent_id.as_deref() {
        if stored_intent_id != intent_id {
            return Err(LookupFailure::CorruptBinding);
        }
    }
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
    if native_event_present
        && !matches!(
            native_event_kind.as_deref(),
            Some("initial" | "recoverable_hold_retry" | "progression")
        )
    {
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
        serde_json::from_value(intent_json.clone()).map_err(|_| LookupFailure::CorruptBinding)?;
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

    // Native reads must validate the complete event/provenance envelope.  The
    // old scalar fields are retained only for synthetic unit fixtures; rows
    // decoded from PostgreSQL always carry `native_event` and cannot fall
    // back to a legacy projection when any paired field is damaged.
    let native_event_legacy_fallback_authorized = if let Some(event) = native_event {
        validate_native_event(
            &event,
            intent_id,
            &stored_hash,
            &intent,
            &intent_json,
            &receipt_id,
            &receipt,
        )?
    } else {
        native_event_legacy_fallback_authorized
    };

    let receipt_amount = receipt
        .evidence
        .get("amount_credits")
        .and_then(Value::as_i64);
    let expected_amount = expected_receipt_amount(
        &intent_json,
        &intent,
        &receipt,
        native_event_present,
        native_event_sequence,
        native_event_kind.as_deref(),
        native_event_legacy_fallback_authorized,
    )?;
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

/// Resolve one row for the administrative/native list using exactly the same
/// integrity contract as the by-intent endpoint.  Missing immutable columns
/// are corruption in a list result (not a reason to silently omit the row).
pub(crate) fn resolve_stored_receipt_for_listing(
    binding: StoredReceiptBinding,
) -> Result<EconomicReceipt, &'static str> {
    let intent_id = binding.intent_id.clone().ok_or("receipt_binding_corrupt")?;
    let requested_hash = binding
        .payload_hash
        .clone()
        .ok_or("receipt_binding_corrupt")?;
    resolve_binding(&intent_id, &requested_hash, binding)
        .map(|response| response.receipt)
        .map_err(LookupFailure::code)
}

/// Validate the immutable event row and its optional legacy-fallback
/// authorization as one object.  This is deliberately shared by the
/// by-intent and administrative list paths so a list cannot expose a receipt
/// that the exact lookup endpoint would reject.
fn validate_native_event(
    event: &StoredNativeReceiptEvent,
    intent_id: &str,
    payload_hash: &str,
    intent: &EconomicIntent,
    intent_json: &Value,
    receipt_id: &str,
    receipt: &EconomicReceipt,
) -> Result<bool, LookupFailure> {
    if event.event_id <= 0
        || event.intent_id != intent_id
        || event.event_sequence <= 0
        || !matches!(
            event.event_kind.as_str(),
            "initial" | "recoverable_hold_retry" | "progression"
        )
        || event.intent_hash != payload_hash
        || event.receipt_id != receipt_id
        || event.protocol_version != intent.protocol_version
        || event.idempotency_scope != intent.idempotency_key.scope
        || event.idempotency_key != intent.idempotency_key.key
        || event.finalized_at_epoch != receipt.finalized_at_epoch
        || event.amount_credits < 0
        || receipt
            .evidence
            .get("amount_credits")
            .and_then(Value::as_i64)
            != Some(event.amount_credits)
        || event.receipt_json
            != serde_json::to_value(receipt).map_err(|_| LookupFailure::CorruptBinding)?
        || !valid_sha256_hex(&event.receipt_hash)
        || event.receipt_hash_actual.as_deref() != Some(event.receipt_hash.as_str())
    {
        return Err(LookupFailure::CorruptBinding);
    }

    let event_marker = event
        .receipt_json
        .get("evidence")
        .and_then(Value::as_object)
        .and_then(|evidence| evidence.get(LEGACY_AMOUNT_FALLBACK_EVIDENCE_KEY));
    let Some(provenance) = event.legacy_fallback_provenance.as_ref() else {
        if event_marker.is_some() {
            return Err(LookupFailure::CorruptBinding);
        }
        return Ok(false);
    };
    if intent_json.get("amount_credits").is_some()
        || event_marker != Some(&Value::Bool(true))
        || provenance.event_id != event.event_id
        || provenance.legacy_receipt_id != event.receipt_id
        || provenance.intent_hash != event.intent_hash
        || provenance.amount_credits != event.amount_credits
        || provenance.amount_credits < 0
        || !valid_sha256_hex(&provenance.legacy_receipt_json_sha256)
        || provenance.legacy_receipt_json_sha256_actual.as_deref()
            != Some(provenance.legacy_receipt_json_sha256.as_str())
    {
        return Err(LookupFailure::CorruptBinding);
    }
    validate_legacy_fallback_source(
        &provenance.legacy_receipt_json,
        &LegacyFallbackSourceContext {
            legacy_receipt_id: &provenance.legacy_receipt_id,
            intent_id,
            protocol_version: &event.protocol_version,
            intent,
            receipt,
            amount_credits: provenance.amount_credits,
            payload_hash,
        },
    )?;
    Ok(true)
}

/// Validate the original 0027 receipt envelope without requiring it to have
/// already been expanded to the full native EconomicReceipt shape.  Migration
/// 0086 deliberately adds absent identity/backend/finalization fields to the
/// native event, so byte-for-byte equality with the event would reject valid
/// legacy rows.  Fields that were present in the source remain immutable and
/// are checked for the correct JSON type and value; only additive fields may be
/// absent.
fn validate_legacy_fallback_source(
    source: &Value,
    context: &LegacyFallbackSourceContext<'_>,
) -> Result<(), LookupFailure> {
    let source_object = source.as_object().ok_or(LookupFailure::CorruptBinding)?;
    let required_string = |name: &str, expected: &str| -> Result<(), LookupFailure> {
        match source_object.get(name) {
            Some(Value::String(value)) if value == expected && !value.trim().is_empty() => Ok(()),
            _ => Err(LookupFailure::CorruptBinding),
        }
    };
    required_string("receipt_id", context.legacy_receipt_id)?;
    required_string("intent_id", context.intent_id)?;

    let optional_string = |name: &str, expected: &str| -> Result<(), LookupFailure> {
        match source_object.get(name) {
            None => Ok(()),
            Some(Value::String(value)) if value == expected && !value.trim().is_empty() => Ok(()),
            _ => Err(LookupFailure::CorruptBinding),
        }
    };
    optional_string("protocol_version", context.protocol_version)?;
    optional_string("term_id", &context.intent.term_id)?;
    optional_string("backend_id", CEX_SETTLEMENT_BACKEND_ID)?;
    optional_string("backend_kind", "cex")?;

    let status = serde_json::to_value(context.receipt.status.clone())
        .map_err(|_| LookupFailure::CorruptBinding)?;
    optional_string(
        "status",
        status.as_str().ok_or(LookupFailure::CorruptBinding)?,
    )?;
    let progression_class = serde_json::to_value(context.receipt.progression_class)
        .map_err(|_| LookupFailure::CorruptBinding)?;
    optional_string(
        "progression_class",
        progression_class
            .as_str()
            .ok_or(LookupFailure::CorruptBinding)?,
    )?;

    if let Some(finalized) = source_object.get("finalized_at_epoch") {
        if finalized.as_i64() != Some(context.receipt.finalized_at_epoch) {
            return Err(LookupFailure::CorruptBinding);
        }
    }

    let evidence = source_object
        .get("evidence")
        .and_then(Value::as_object)
        .ok_or(LookupFailure::CorruptBinding)?;
    if evidence.contains_key(LEGACY_AMOUNT_FALLBACK_EVIDENCE_KEY)
        || evidence.get("amount_credits").and_then(Value::as_i64) != Some(context.amount_credits)
    {
        return Err(LookupFailure::CorruptBinding);
    }
    if let Some(source_hash) = evidence.get("payload_hash") {
        if source_hash.as_str() != Some(context.payload_hash) {
            return Err(LookupFailure::CorruptBinding);
        }
    }
    Ok(())
}

/// Mirror the native writer and migration fail-closed evidence rule.
///
/// A present `amount_credits` key is immutable intent authority.  This includes
/// an explicit JSON `null`, which deserializes to `None` and therefore remains
/// fail-closed at zero rather than authorizing a positive compatibility value.
///
/// The only compatibility exception is an omitted key: 0086 may have recovered
/// a non-negative amount from legacy receipt evidence for audit-only intents.
/// Accept that fallback only when the evidence is an actual non-negative JSON
/// integer; malformed, negative, fractional, or missing evidence remains a
/// corrupt binding instead of becoming value authority.
fn expected_receipt_amount(
    intent_json: &Value,
    intent: &EconomicIntent,
    receipt: &EconomicReceipt,
    native_event_present: bool,
    native_event_sequence: Option<i64>,
    native_event_kind: Option<&str>,
    native_event_legacy_fallback_authorized: bool,
) -> Result<i64, LookupFailure> {
    let legacy_fallback_marker = match receipt.evidence.get(LEGACY_AMOUNT_FALLBACK_EVIDENCE_KEY) {
        None => false,
        Some(Value::Bool(value)) => *value,
        // A reserved provenance marker with any other shape is itself a
        // corrupt binding; do not silently ignore a forged marker.
        Some(_) => return Err(LookupFailure::CorruptBinding),
    };
    let legacy_fallback_provenance_authorized = native_event_present
        && legacy_fallback_marker
        && native_event_legacy_fallback_authorized
        && native_event_kind == Some("initial")
        && native_event_sequence == Some(1);
    // The marker is immutable provenance, not a value-bearing hint.  Even a
    // zero amount (or a legacy projection with no native event) must not be
    // able to carry a forged marker; only migration 0086's sequence-one
    // initial event, joined to its provenance row, may claim it.
    if legacy_fallback_marker && !legacy_fallback_provenance_authorized {
        return Err(LookupFailure::CorruptBinding);
    }
    if intent_json.get("amount_credits").is_some() {
        if legacy_fallback_marker {
            return Err(LookupFailure::CorruptBinding);
        }
        return Ok(intent.amount_credits.unwrap_or_default().max(0));
    }

    let amount = receipt
        .evidence
        .get("amount_credits")
        .and_then(Value::as_i64);

    // A legacy projection has no native event, so its non-negative evidence is
    // the only amount available.  Migration 0086 marks the equivalent native
    // compatibility backfill explicitly; `event_kind` must be the sequence-one
    // seed kind so a later live/retry event cannot inherit that authority.
    let compatibility_fallback = !native_event_present || legacy_fallback_provenance_authorized;
    if compatibility_fallback {
        let Some(amount) = amount.filter(|amount| *amount >= 0) else {
            return Err(LookupFailure::CorruptBinding);
        };
        return Ok(amount);
    };

    // Native/live writers fail closed to zero for an omitted amount.  Accept
    // that non-value-bearing snapshot, but never authorize a positive amount
    // without the migration provenance above.
    if amount == Some(0) {
        return Ok(0);
    }
    Err(LookupFailure::CorruptBinding)
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
                intent_id: Some(intent_id.to_string()),
                payload_hash: Some(payload_hash),
                intent_json: Some(intent_json),
                native_event: None,
                native_event_present: false,
                native_event_sequence: None,
                native_event_kind: None,
                native_event_legacy_fallback_authorized: false,
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
    fn omitted_intent_amount_uses_nonnegative_legacy_evidence_fallback() {
        let intent_id = "world:omitted-legacy-amount";
        let (_, mut binding) = stored_binding(intent_id);
        binding
            .intent_json
            .as_mut()
            .expect("intent")
            .as_object_mut()
            .expect("intent object")
            .remove("amount_credits");
        let payload_hash =
            sha256_json(binding.intent_json.as_ref().expect("intent")).expect("hash fixture");
        binding.payload_hash = Some(payload_hash.clone());
        let receipt_json = binding.receipt_json.as_mut().expect("receipt");
        receipt_json["evidence"]["payload_hash"] = json!(payload_hash.clone());
        receipt_json["evidence"]["amount_credits"] = json!(9);

        let result = resolve_binding(intent_id, &payload_hash, binding)
            .expect("omitted amount may use valid legacy evidence");
        assert_eq!(result.receipt.evidence["amount_credits"].as_i64(), Some(9));
    }

    #[test]
    fn explicit_null_intent_amount_remains_fail_closed() {
        let intent_id = "world:explicit-null-amount";
        let (_, mut binding) = stored_binding(intent_id);
        binding
            .intent_json
            .as_mut()
            .expect("intent")
            .as_object_mut()
            .expect("intent object")
            .insert("amount_credits".to_string(), Value::Null);
        let payload_hash =
            sha256_json(binding.intent_json.as_ref().expect("intent")).expect("hash fixture");
        binding.payload_hash = Some(payload_hash.clone());
        let receipt_json = binding.receipt_json.as_mut().expect("receipt");
        receipt_json["evidence"]["payload_hash"] = json!(payload_hash.clone());
        receipt_json["evidence"]["amount_credits"] = json!(9);

        assert_eq!(
            resolve_binding(intent_id, &payload_hash, binding).unwrap_err(),
            LookupFailure::CorruptBinding
        );
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
            intent_id: None,
            payload_hash: None,
            intent_json: None,
            native_event: None,
            native_event_present: false,
            native_event_sequence: None,
            native_event_kind: None,
            native_event_legacy_fallback_authorized: false,
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

    #[test]
    fn native_omitted_amount_positive_evidence_is_not_value_authority() {
        let intent_id = "world:native-omitted-positive";
        let (_, mut binding) = stored_binding(intent_id);
        binding.native_event_present = true;
        binding.native_event_sequence = Some(1);
        binding.native_event_kind = Some("initial".to_string());
        binding.native_event_legacy_fallback_authorized = false;
        binding
            .intent_json
            .as_mut()
            .expect("intent")
            .as_object_mut()
            .expect("intent object")
            .remove("amount_credits");
        let payload_hash =
            sha256_json(binding.intent_json.as_ref().expect("intent")).expect("hash fixture");
        binding.payload_hash = Some(payload_hash.clone());
        let receipt_json = binding.receipt_json.as_mut().expect("receipt");
        receipt_json["evidence"]["payload_hash"] = json!(payload_hash.clone());
        receipt_json["evidence"]["amount_credits"] = json!(9);

        assert_eq!(
            resolve_binding(intent_id, &payload_hash, binding).unwrap_err(),
            LookupFailure::CorruptBinding
        );
    }

    #[test]
    fn marked_native_backfill_may_use_omitted_amount_evidence() {
        let intent_id = "world:native-backfill-omitted-positive";
        let (_, mut binding) = stored_binding(intent_id);
        binding.native_event_present = true;
        binding.native_event_sequence = Some(1);
        binding.native_event_kind = Some("initial".to_string());
        binding.native_event_legacy_fallback_authorized = true;
        binding
            .intent_json
            .as_mut()
            .expect("intent")
            .as_object_mut()
            .expect("intent object")
            .remove("amount_credits");
        let payload_hash =
            sha256_json(binding.intent_json.as_ref().expect("intent")).expect("hash fixture");
        binding.payload_hash = Some(payload_hash.clone());
        let receipt_json = binding.receipt_json.as_mut().expect("receipt");
        receipt_json["evidence"]["payload_hash"] = json!(payload_hash.clone());
        receipt_json["evidence"]["amount_credits"] = json!(9);
        receipt_json["evidence"][LEGACY_AMOUNT_FALLBACK_EVIDENCE_KEY] = json!(true);

        let result = resolve_binding(intent_id, &payload_hash, binding)
            .expect("marked compatibility backfill should remain readable");
        assert_eq!(result.receipt.evidence["amount_credits"].as_i64(), Some(9));
    }

    #[test]
    fn positive_legacy_evidence_requires_the_marker_in_addition_to_provenance_flag() {
        let intent_id = "world:native-provenance-without-marker";
        let (_, mut binding) = stored_binding(intent_id);
        binding.native_event_present = true;
        binding.native_event_sequence = Some(1);
        binding.native_event_kind = Some("initial".to_string());
        // This is the old scalar representation used by pre-refactor callers;
        // it intentionally claims provenance while omitting the immutable
        // migration marker.  The amount must not become value authority.
        binding.native_event_legacy_fallback_authorized = true;
        binding
            .intent_json
            .as_mut()
            .expect("intent")
            .as_object_mut()
            .expect("intent object")
            .remove("amount_credits");
        let payload_hash =
            sha256_json(binding.intent_json.as_ref().expect("intent")).expect("hash fixture");
        binding.payload_hash = Some(payload_hash.clone());
        let receipt_json = binding.receipt_json.as_mut().expect("receipt");
        receipt_json["evidence"]["payload_hash"] = json!(payload_hash.clone());
        receipt_json["evidence"]["amount_credits"] = json!(9);

        assert_eq!(
            resolve_binding(intent_id, &payload_hash, binding).unwrap_err(),
            LookupFailure::CorruptBinding
        );
    }

    #[test]
    fn marked_native_non_initial_sequence_cannot_authorize_legacy_amount() {
        let intent_id = "world:native-retry-marked-positive";
        let (_, mut binding) = stored_binding(intent_id);
        binding.native_event_present = true;
        binding.native_event_sequence = Some(2);
        binding.native_event_kind = Some("initial".to_string());
        binding.native_event_legacy_fallback_authorized = true;
        binding
            .intent_json
            .as_mut()
            .expect("intent")
            .as_object_mut()
            .expect("intent object")
            .remove("amount_credits");
        let payload_hash =
            sha256_json(binding.intent_json.as_ref().expect("intent")).expect("hash fixture");
        binding.payload_hash = Some(payload_hash.clone());
        let receipt_json = binding.receipt_json.as_mut().expect("receipt");
        receipt_json["evidence"]["payload_hash"] = json!(payload_hash.clone());
        receipt_json["evidence"]["amount_credits"] = json!(9);
        receipt_json["evidence"][LEGACY_AMOUNT_FALLBACK_EVIDENCE_KEY] = json!(true);

        assert_eq!(
            resolve_binding(intent_id, &payload_hash, binding).unwrap_err(),
            LookupFailure::CorruptBinding
        );
    }

    #[test]
    fn legacy_fallback_marker_cannot_authorize_zero_without_native_sequence_one() {
        for (
            label,
            native_event_present,
            native_event_sequence,
            native_event_kind,
            native_event_legacy_fallback_authorized,
        ) in [
            (
                "native-non-initial-zero",
                true,
                Some(2),
                Some("initial"),
                true,
            ),
            ("legacy-projection-zero", false, None, None, false),
        ] {
            let intent_id = format!("world:{label}");
            let (_, mut binding) = stored_binding(&intent_id);
            binding.native_event_present = native_event_present;
            binding.native_event_sequence = native_event_sequence;
            binding.native_event_kind = native_event_kind.map(str::to_string);
            binding.native_event_legacy_fallback_authorized =
                native_event_legacy_fallback_authorized;
            binding
                .intent_json
                .as_mut()
                .expect("intent")
                .as_object_mut()
                .expect("intent object")
                .remove("amount_credits");
            let payload_hash =
                sha256_json(binding.intent_json.as_ref().expect("intent")).expect("hash fixture");
            binding.payload_hash = Some(payload_hash.clone());
            let receipt_json = binding.receipt_json.as_mut().expect("receipt");
            receipt_json["evidence"]["payload_hash"] = json!(payload_hash.clone());
            receipt_json["evidence"]["amount_credits"] = json!(0);
            receipt_json["evidence"][LEGACY_AMOUNT_FALLBACK_EVIDENCE_KEY] = json!(true);

            assert_eq!(
                resolve_binding(&intent_id, &payload_hash, binding).unwrap_err(),
                LookupFailure::CorruptBinding,
                "forged fallback marker was accepted for {label}"
            );
        }
    }

    #[test]
    fn native_omitted_amount_zero_remains_fail_closed_and_readable() {
        let intent_id = "world:native-omitted-zero";
        let (_, mut binding) = stored_binding(intent_id);
        binding.native_event_present = true;
        binding.native_event_sequence = Some(1);
        binding.native_event_kind = Some("initial".to_string());
        binding.native_event_legacy_fallback_authorized = false;
        binding
            .intent_json
            .as_mut()
            .expect("intent")
            .as_object_mut()
            .expect("intent object")
            .remove("amount_credits");
        let payload_hash =
            sha256_json(binding.intent_json.as_ref().expect("intent")).expect("hash fixture");
        binding.payload_hash = Some(payload_hash.clone());
        let receipt_json = binding.receipt_json.as_mut().expect("receipt");
        receipt_json["evidence"]["payload_hash"] = json!(payload_hash.clone());
        receipt_json["evidence"]["amount_credits"] = json!(0);

        let result = resolve_binding(intent_id, &payload_hash, binding)
            .expect("native fail-closed zero should remain readable");
        assert_eq!(result.receipt.evidence["amount_credits"].as_i64(), Some(0));
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
