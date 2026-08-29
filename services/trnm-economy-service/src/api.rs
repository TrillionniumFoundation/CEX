use crate::{
    config::{AuthorityIdentity, AuthorityRegistry, AuthorizationFailure, IssuerKeyRegistry},
    contract::{
        canonical_sha256, serialized_intent_hash, EconomicIntent, EconomicIntentKind,
        EntitlementIssuerKeyStatusRequest, EntitlementIssuerKeyStatusResponse,
        ServerSignedValueEntitlementV2, SettlementErrorBody, SettlementReadiness,
        SettlementReceiptLookupResponse, SubmitIntentRequest, ValueEntitlementSource,
        BATTLE_WALLET_REWARD_PER_EVENT_CAP, ENTITLEMENT_SIGNER_ISSUER, GAME_AUTHORITY_HEADER,
        INTENT_HASH_HEADER, SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY,
        SERVER_SIGNED_VALUE_ENTITLEMENT_V2_CONTRACT, SETTLEMENT_CONTRACT_VERSION,
        SETTLEMENT_ERROR_CONTRACT, SETTLEMENT_RECEIPT_LOOKUP_CONTRACT,
        TERM_EXCHANGE_PROTOCOL_VERSION,
    },
    repository::{RepositoryError, SettlementPlan, SettlementRepository},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Datelike, Utc};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub repository: SettlementRepository,
    pub authorities: AuthorityRegistry,
    pub issuer_keys: IssuerKeyRegistry,
}

impl AppState {
    pub fn new(
        repository: SettlementRepository,
        authorities: AuthorityRegistry,
        issuer_keys: IssuerKeyRegistry,
    ) -> Self {
        Self {
            repository,
            authorities,
            issuer_keys,
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/trnm/economy/readiness", get(readiness))
        .route(
            "/v1/trnm/economy/issuer-keys/status",
            post(issuer_key_status),
        )
        .route("/v1/trnm/economy/intents", post(submit_intent))
        .route("/v1/trnm/economy/receipts/by-intent", get(lookup_receipt))
        .with_state(state)
}

async fn health() -> &'static str {
    "trnm-economy-service ok"
}

async fn readiness(State(state): State<AppState>) -> Response {
    let postgres = state.repository.postgres_ready().await;
    let receipt_lookup = state.repository.schema_ready().await;
    let authority_principals = state.authorities.active_expected_audience_count();
    let active_issuer_keys = state.issuer_keys.active_count();
    let ready = postgres && receipt_lookup && authority_principals > 0 && active_issuer_keys > 0;

    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(SettlementReadiness {
            status: if ready { "ok" } else { "blocked" }.to_string(),
            contract_version: SETTLEMENT_CONTRACT_VERSION.to_string(),
            postgres,
            receipt_lookup,
            authority_principals,
            active_issuer_keys,
            public_player_market_enabled: false,
        }),
    )
        .into_response()
}

async fn issuer_key_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<EntitlementIssuerKeyStatusRequest>,
) -> Result<Json<EntitlementIssuerKeyStatusResponse>, ApiError> {
    authorize(&state, &headers)?;
    let key = state.issuer_keys.get(&request.key_id).ok_or_else(|| {
        ApiError::not_found("issuer_key_not_found", "issuer key is not registered")
    })?;
    Ok(Json(EntitlementIssuerKeyStatusResponse {
        key_id: key.key_id.clone(),
        issuer: key.issuer.clone(),
        status: key.status.clone(),
        signature_algorithm: key.signature_algorithm.clone(),
        public_key_sha256: key.public_key_sha256.clone(),
    }))
}

async fn submit_intent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SubmitIntentRequest>,
) -> Result<Json<crate::contract::EconomicReceipt>, ApiError> {
    let authority = authorize(&state, &headers)?;
    let supplied_hash = require_intent_hash(&headers)?;
    let computed_hash = serialized_intent_hash(&request.intent)
        .map_err(|message| ApiError::bad_request("intent_encoding_failed", message))?;
    if supplied_hash != computed_hash {
        return Err(ApiError::conflict(
            "intent_hash_mismatch",
            "x-trnm-intent-sha256 does not bind the exact EconomicIntent bytes",
        ));
    }

    let plan = validate_intent(&request.intent, &state.issuer_keys)?;
    let receipt = state
        .repository
        .submit(
            &request.intent,
            &computed_hash,
            &authority.authority_id,
            plan,
        )
        .await
        .map_err(map_repository_error)?;
    Ok(Json(receipt))
}

#[derive(Debug, Deserialize)]
struct ReceiptLookupQuery {
    intent_id: String,
}

async fn lookup_receipt(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReceiptLookupQuery>,
) -> Result<Json<SettlementReceiptLookupResponse>, ApiError> {
    authorize(&state, &headers)?;
    validate_identifier(&query.intent_id, "intent_id")?;
    let supplied_hash = require_intent_hash(&headers)?;
    let Some((stored_hash, receipt)) = state
        .repository
        .lookup(&query.intent_id)
        .await
        .map_err(map_repository_error)?
    else {
        return Err(ApiError::not_found(
            "intent_not_found",
            "no durable intent or receipt exists under this identity",
        ));
    };

    if stored_hash != supplied_hash {
        return Err(ApiError::conflict(
            "intent_hash_conflict",
            "intent_id exists under different immutable bytes",
        ));
    }

    Ok(Json(SettlementReceiptLookupResponse {
        contract_version: SETTLEMENT_RECEIPT_LOOKUP_CONTRACT.to_string(),
        intent_id: query.intent_id,
        intent_hash: stored_hash,
        receipt,
    }))
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<AuthorityIdentity, ApiError> {
    let supplied = headers
        .get(GAME_AUTHORITY_HEADER)
        .and_then(|value| value.to_str().ok());
    state
        .authorities
        .authorize(supplied)
        .map_err(|failure| match failure {
            AuthorizationFailure::Missing => ApiError::unauthorized(
                "missing_game_authority",
                "x-trnm-game-authority is required",
            ),
            AuthorizationFailure::Invalid => ApiError::unauthorized(
                "invalid_game_authority",
                "game authority credential is invalid or inactive",
            ),
            AuthorizationFailure::WrongAudience => ApiError::forbidden(
                "wrong_game_authority_audience",
                "game authority credential is not scoped to trnm-cex-settlement-v1",
            ),
        })
}

fn require_intent_hash(headers: &HeaderMap) -> Result<String, ApiError> {
    let value = headers
        .get(INTENT_HASH_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            ApiError::bad_request("missing_intent_hash", "x-trnm-intent-sha256 is required")
        })?;
    if !canonical_sha256(value) {
        return Err(ApiError::bad_request(
            "invalid_intent_hash",
            "intent hash must contain 64 lowercase hexadecimal characters",
        ));
    }
    Ok(value.to_string())
}

fn validate_intent(
    intent: &EconomicIntent,
    issuer_keys: &IssuerKeyRegistry,
) -> Result<SettlementPlan, ApiError> {
    if intent.protocol_version != TERM_EXCHANGE_PROTOCOL_VERSION
        || intent.domain != "trnm_game"
        || intent.term_version.trim().is_empty()
        || intent.idempotency_key.scope.trim().is_empty()
        || intent.idempotency_key.key.trim().is_empty()
        || intent.actors.is_empty()
    {
        return Err(ApiError::unprocessable(
            "invalid_economic_intent",
            "economic intent violates the TRNM protocol boundary",
        ));
    }
    validate_identifier(&intent.intent_id, "intent_id")?;
    validate_identifier(&intent.term_id, "term_id")?;
    if intent
        .actors
        .iter()
        .any(|actor| actor.actor_id.trim().is_empty())
    {
        return Err(ApiError::unprocessable(
            "invalid_actor_binding",
            "all economic actors require an actor_id",
        ));
    }

    match intent.kind {
        EconomicIntentKind::ReleaseReward => validate_release_reward(intent, issuer_keys),
        EconomicIntentKind::CompleteContract => {
            if intent.amount_credits.unwrap_or_default() != 0 {
                return Err(ApiError::unprocessable(
                    "complete_contract_has_value",
                    "complete_contract cannot mutate wallet value",
                ));
            }
            Ok(SettlementPlan::CompleteContract)
        }
        _ => Err(ApiError::unprocessable(
            "unsupported_economic_intent_kind",
            "this settlement boundary accepts only release_reward and complete_contract",
        )),
    }
}

fn valid_ledger_idempotency_scope(value: &str) -> bool {
    let bytes = value.trim().as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 160
        && bytes[0].is_ascii_alphanumeric()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn valid_ledger_idempotency_key(value: &str) -> bool {
    let trimmed = value.trim();
    (1..=256).contains(&trimmed.len()) && trimmed.chars().all(|character| !character.is_control())
}

fn validate_release_reward(
    intent: &EconomicIntent,
    issuer_keys: &IssuerKeyRegistry,
) -> Result<SettlementPlan, ApiError> {
    if !valid_ledger_idempotency_scope(&intent.idempotency_key.scope)
        || !valid_ledger_idempotency_key(&intent.idempotency_key.key)
    {
        return Err(ApiError::unprocessable(
            "invalid_reward_idempotency",
            "release_reward idempotency identity violates the exact Ledger v2 contract",
        ));
    }
    let amount = intent.amount_credits.ok_or_else(|| {
        ApiError::unprocessable("missing_reward_amount", "reward amount is required")
    })?;
    if !(1..=BATTLE_WALLET_REWARD_PER_EVENT_CAP).contains(&amount)
        || intent.currency.as_deref() != Some("wallet_credits")
    {
        return Err(ApiError::unprocessable(
            "reward_policy_violation",
            "wallet reward must be 1..=100 wallet_credits",
        ));
    }

    if intent.actors.len() != 1 {
        return Err(ApiError::unprocessable(
            "ambiguous_reward_actor",
            "release_reward requires exactly one economic actor",
        ));
    }
    let actor = &intent.actors[0];
    let account_id = actor
        .account_id
        .as_deref()
        .ok_or_else(|| {
            ApiError::unprocessable(
                "missing_reward_account",
                "release_reward actor requires an account_id",
            )
        })
        .and_then(|value| {
            Uuid::parse_str(value).map_err(|_| {
                ApiError::unprocessable(
                    "invalid_reward_account",
                    "release_reward account_id must be a CEX UUID",
                )
            })
        })?;

    if intent.assets.len() != 1 {
        return Err(ApiError::unprocessable(
            "invalid_reward_asset",
            "release_reward requires exactly one wallet-credit asset",
        ));
    }
    let asset = &intent.assets[0];
    if asset.asset_id != "cex-wallet-credit"
        || !matches!(asset.asset_kind.as_str(), "walletcredit" | "wallet_credit")
        || asset.quantity != amount
        || asset.unit != "credits"
    {
        return Err(ApiError::unprocessable(
            "invalid_reward_asset",
            "wallet asset binding does not match the reward amount",
        ));
    }

    let entitlement_value = intent
        .metadata
        .get(SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY)
        .cloned()
        .ok_or_else(|| {
            ApiError::unprocessable(
                "missing_value_entitlement",
                "release_reward requires a server-signed value entitlement",
            )
        })?;
    let entitlement: ServerSignedValueEntitlementV2 = serde_json::from_value(entitlement_value)
        .map_err(|error| {
            ApiError::unprocessable(
                "invalid_value_entitlement",
                format!("decode server-signed entitlement: {error}"),
            )
        })?;
    validate_entitlement(intent, actor, account_id, amount, &entitlement, issuer_keys)?;

    Ok(SettlementPlan::ReleaseReward {
        account_id,
        amount_credits: amount,
        budget_day: i32::try_from(entitlement.budget_day).map_err(|_| {
            ApiError::unprocessable(
                "invalid_entitlement_budget_day",
                "entitlement budget_day is outside PostgreSQL integer range",
            )
        })?,
    })
}

fn validate_entitlement(
    intent: &EconomicIntent,
    actor: &crate::contract::ActorRef,
    account_id: Uuid,
    amount: i64,
    entitlement: &ServerSignedValueEntitlementV2,
    issuer_keys: &IssuerKeyRegistry,
) -> Result<(), ApiError> {
    let now = Utc::now().timestamp();
    let issued_at =
        DateTime::<Utc>::from_timestamp(entitlement.issued_at_epoch, 0).ok_or_else(|| {
            ApiError::unprocessable(
                "invalid_entitlement_time",
                "entitlement issued_at is outside the supported range",
            )
        })?;
    let expected_budget_day =
        (issued_at.year() as u32) * 10_000 + issued_at.month() * 100 + issued_at.day();

    if entitlement.contract_version != SERVER_SIGNED_VALUE_ENTITLEMENT_V2_CONTRACT
        || entitlement.issuer != ENTITLEMENT_SIGNER_ISSUER
        || entitlement.signature_algorithm != "ed25519"
        || entitlement.actor_id != actor.actor_id
        || entitlement.account_id != account_id.to_string()
        || entitlement.intent_id != intent.intent_id
        || entitlement.amount_credits != amount
        || entitlement.currency != "wallet_credits"
        || entitlement.source != ValueEntitlementSource::Battle
        || entitlement.budget_day != expected_budget_day
        || entitlement.issued_at_epoch > now.saturating_add(30)
        || entitlement.expires_at_epoch <= now
        || entitlement.expires_at_epoch <= entitlement.issued_at_epoch
        || entitlement.expires_at_epoch > entitlement.issued_at_epoch.saturating_add(600)
        || entitlement.match_id.trim().is_empty()
        || entitlement.rules_version.trim().is_empty()
        || entitlement.build_id.trim().is_empty()
        || !canonical_sha256(&entitlement.result_hash)
        || !canonical_sha256(&entitlement.participants_hash)
        || entitlement.nonce.trim().is_empty()
        || entitlement.signature.trim().is_empty()
    {
        return Err(ApiError::unprocessable(
            "invalid_value_entitlement",
            "server-signed entitlement fails authoritative binding or timing policy",
        ));
    }

    issuer_keys
        .verify_entitlement(entitlement)
        .map_err(|message| ApiError::unprocessable("invalid_value_entitlement_signature", message))
}

fn validate_identifier(value: &str, field: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_' | b'.'))
    {
        return Err(ApiError::bad_request(
            "invalid_identifier",
            format!("{field} contains unsupported characters or length"),
        ));
    }
    Ok(())
}

fn map_repository_error(error: RepositoryError) -> ApiError {
    match error {
        RepositoryError::Conflict => ApiError::conflict(
            "intent_hash_conflict",
            "intent_id exists under different immutable bytes",
        ),
        RepositoryError::AccountNotFound => ApiError::unprocessable(
            "wallet_account_not_found",
            "CEX wallet account does not exist",
        ),
        RepositoryError::AccountInactive => ApiError::forbidden(
            "wallet_account_inactive",
            "CEX wallet account is not active",
        ),
        RepositoryError::AccountCurrencyMismatch => ApiError::unprocessable(
            "wallet_currency_mismatch",
            "CEX wallet account is not denominated in wallet_credits",
        ),
        RepositoryError::DailyRewardLimit => ApiError::unprocessable(
            "daily_reward_limit_exceeded",
            "battle wallet reward daily cap would be exceeded",
        ),
        RepositoryError::StoredReceiptCorrupt(message) => {
            ApiError::internal("stored_receipt_corrupt", message)
        }
        RepositoryError::Database(message) => ApiError::service_unavailable(
            "settlement_database_unavailable",
            format!("settlement database operation failed: {message}"),
        ),
    }
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    retryable: bool,
}

impl ApiError {
    fn new(
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            retryable,
        }
    }

    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message, false)
    }

    fn unauthorized(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, code, message, false)
    }

    fn forbidden(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message, false)
    }

    fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message, false)
    }

    fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message, false)
    }

    fn unprocessable(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, message, false)
    }

    fn service_unavailable(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, code, message, true)
    }

    fn internal(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, code, message, false)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(SettlementErrorBody {
                contract_version: SETTLEMENT_ERROR_CONTRACT.to_string(),
                code: self.code.to_string(),
                message: self.message,
                retryable: self.retryable,
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        valid_ledger_idempotency_key, valid_ledger_idempotency_scope, validate_identifier,
    };

    #[test]
    fn identifiers_are_bounded_ascii() {
        assert!(validate_identifier("intent:abc-1_test.value", "intent_id").is_ok());
        assert!(validate_identifier("", "intent_id").is_err());
        assert!(validate_identifier("contains space", "intent_id").is_err());
        assert!(validate_identifier(&"a".repeat(257), "intent_id").is_err());
    }

    #[test]
    fn ledger_idempotency_scope_matches_exact_v2_contract() {
        assert!(valid_ledger_idempotency_scope("campaign:account-1"));
        assert!(valid_ledger_idempotency_scope("a"));
        assert!(!valid_ledger_idempotency_scope(":leading-punctuation"));
        assert!(!valid_ledger_idempotency_scope(&"a".repeat(161)));
        assert!(!valid_ledger_idempotency_scope("scope with spaces"));
        assert!(valid_ledger_idempotency_key("reward:one"));
        assert!(!valid_ledger_idempotency_key("reward\nline"));
        assert!(!valid_ledger_idempotency_key(&"a".repeat(257)));
    }
}
