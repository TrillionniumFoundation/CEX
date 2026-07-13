use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use chrono::{Datelike, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    AdminAuthorizationFailure, AdminPrincipal,
};
use term_exchange_protocol::{
    EconomicIntent, EconomicIntentKind, EconomicReceipt, ReceiptStatus,
    ServerSignedValueEntitlementV1, ServerSignedValueEntitlementV2, SettlementBackendKind,
    ValueEntitlementSource, WalletSnapshot, BATTLE_WALLET_REWARD_PER_EVENT_CAP,
    CEX_SETTLEMENT_BACKEND_ID, SERVER_SIGNED_VALUE_ENTITLEMENT_CONTRACT,
    SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY, SERVER_SIGNED_VALUE_ENTITLEMENT_V2_CONTRACT,
};
use uuid::Uuid;

use crate::{
    repository::LedgerActionError,
    state::{AccountRecord, AppState, LedgerEntryRecord},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAccountRequest {
    pub org_id: String,
    pub account_type: String,
    pub currency_unit: Option<String>,
    pub initial_balance: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerActionRequest {
    pub account_id: Uuid,
    pub amount: f64,
    pub reference_id: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmEconomicIntentRequest {
    pub intent: EconomicIntent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmWalletRequest {
    pub actor_id: String,
    pub account_id: String,
    #[serde(default)]
    pub reconciliation_cursor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmIdentityRegisterRequest {
    pub player_id: String,
    pub account_id: String,
    pub recovery_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmIdentityRecoverRequest {
    pub player_id: String,
    pub recovery_key: String,
    pub new_recovery_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmIdentityStatusRequest {
    pub player_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmProductRegisterRequest {
    pub player_id: String,
    pub credential: String,
    pub invite_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmProductInviteIssueRequest {
    #[serde(default = "default_product_invite_lifetime_seconds")]
    pub lifetime_seconds: i64,
    #[serde(default = "default_product_invite_max_uses")]
    pub max_uses: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmProductLoginRequest {
    pub player_id: String,
    pub credential: String,
    pub device_id: String,
    #[serde(default = "default_session_lifetime_seconds")]
    pub lifetime_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmProductCredentialRotateRequest {
    pub player_id: String,
    pub credential: String,
    pub new_credential: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmIdentityAppealRequest {
    pub player_id: String,
    pub credential: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmIdentityAppealResolveRequest {
    pub appeal_id: String,
    pub decision: String,
    pub resolution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmPlayerSessionIssueRequest {
    pub player_id: String,
    pub recovery_key: String,
    pub device_id: String,
    #[serde(default = "default_session_lifetime_seconds")]
    pub lifetime_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmPlayerSessionRevokeRequest {
    pub session_id: String,
    #[serde(default = "default_session_revoke_reason")]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmPlayerSessionVerifyRequest {
    pub player_id: String,
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmPlayerSessionVerifyResponse {
    pub verified: bool,
    pub session_id: String,
    pub player_id: String,
    pub account_id: String,
    pub device_id: String,
    pub recovery_generation: i64,
    pub expires_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmPlayerSessionResponse {
    pub contract_version: String,
    pub session_token: String,
    pub session_id: String,
    pub player_id: String,
    pub account_id: String,
    pub device_id: String,
    pub recovery_generation: i64,
    pub issued_at_epoch: i64,
    pub expires_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrnmPlayerSessionClaimsV1 {
    contract_version: String,
    session_id: String,
    player_id: String,
    account_id: String,
    device_id: String,
    recovery_generation: i64,
    issued_at_epoch: i64,
    expires_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmValueEntitlementIssueRequest {
    pub actor_id: String,
    pub account_id: String,
    pub source: ValueEntitlementSource,
    pub source_id: String,
    pub intent_id: String,
    pub amount_credits: i64,
    #[serde(default = "default_entitlement_lifetime_seconds")]
    pub lifetime_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmEntitlementIssuerKeyStatusRequest {
    pub key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnmEntitlementIssuerKeyStatusResponse {
    pub key_id: String,
    pub issuer: String,
    pub status: String,
    pub signature_algorithm: String,
    pub public_key_sha256: String,
}

const PLAYER_SESSION_CONTRACT_VERSION: &str = "trnm_player_session_v1";
const PLAYER_SESSION_HEADER: &str = "x-trnm-player-session";
const SYSTEM_OPERATION_HEADER: &str = "x-trnm-system-operation";
const GAME_AUTHORITY_HEADER: &str = "x-trnm-game-authority";

fn default_session_lifetime_seconds() -> i64 {
    3_600
}

fn default_entitlement_lifetime_seconds() -> i64 {
    600
}

fn default_product_invite_lifetime_seconds() -> i64 {
    86_400
}

fn default_product_invite_max_uses() -> i32 {
    1
}

fn default_session_revoke_reason() -> String {
    "player_requested".to_string()
}

pub async fn health() -> &'static str {
    "ledger-service ok"
}

pub async fn trnm_economy_readiness(State(state): State<AppState>) -> Response {
    let persistence_healthy = state.repository.persistence_healthy().await;
    let ready = state.fail_fast && state.repository.persistence_ready() && persistence_healthy;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({
            "status": if ready { "ok" } else { "blocked" },
            "profile": "trnm-economy-production",
            "fail_fast": state.fail_fast,
            "postgres_persistent": state.repository.persistence_ready(),
            "postgres_healthy": persistence_healthy,
            "atomic_intent_receipts": true,
            "escrow": true,
            "seller_payout_hold": true,
            "player_identity_recovery": true,
            "server_signed_value_entitlement": true,
            "online_entitlement_signature_algorithm": "ed25519",
            "online_entitlement_active_issuer_keys": state.entitlement_issuer_keys.values().filter(|key| key.status == "active").count(),
            "online_entitlement_revoked_issuer_keys": state.entitlement_issuer_keys.values().filter(|key| key.status == "revoked").count(),
            "player_session_account_ownership": true,
            "closed_alpha_single_use_registration_invites": true,
            "product_credentials": "argon2id",
            "credential_rotation_revokes_sessions": true,
            "durable_login_rate_limit": true,
            "suspension_appeal_workflow": true,
            "wallet_reward_per_event_cap": BATTLE_WALLET_REWARD_PER_EVENT_CAP,
            "complete_contract_zero_value_only": true,
            "seller_hold_sweeper": true,
            "receipt_projection_rebuild_source": "postgresql",
        })),
    )
        .into_response()
}

pub async fn post_trnm_identity_register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmIdentityRegisterRequest>,
) -> Response {
    if let Err(response) = authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        return response;
    }
    let account_id = match Uuid::parse_str(&request.account_id) {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "account_id must be a UUID".to_string(),
                    message: None,
                }),
            )
                .into_response()
        }
    };
    match state
        .repository
        .register_trnm_player_identity(&request.player_id, account_id, &request.recovery_key)
        .await
    {
        Ok(identity) => (StatusCode::CREATED, Json(identity)).into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_identity_recover(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmIdentityRecoverRequest>,
) -> Response {
    if let Err(response) = authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        return response;
    }
    match state
        .repository
        .recover_trnm_player_identity(
            &request.player_id,
            &request.recovery_key,
            &request.new_recovery_key,
        )
        .await
    {
        Ok(identity) => (StatusCode::OK, Json(identity)).into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_identity_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmIdentityStatusRequest>,
) -> Response {
    if let Err(response) = authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        return response;
    }
    match state
        .repository
        .set_trnm_player_identity_status(&request.player_id, &request.status)
        .await
    {
        Ok(identity) => (StatusCode::OK, Json(identity)).into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_product_register(
    State(state): State<AppState>,
    Json(request): Json<TrnmProductRegisterRequest>,
) -> Response {
    match state
        .repository
        .register_trnm_product_player(
            &request.player_id,
            &request.credential,
            state.product_org_id,
            &request.invite_code,
        )
        .await
    {
        Ok(identity) => (StatusCode::CREATED, Json(identity)).into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_product_invite_issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmProductInviteIssueRequest>,
) -> Response {
    if let Err(response) = authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        return response;
    }
    match state
        .repository
        .issue_trnm_product_registration_invite(request.lifetime_seconds, request.max_uses)
        .await
    {
        Ok(invite) => (StatusCode::CREATED, Json(invite)).into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_product_login(
    State(state): State<AppState>,
    Json(request): Json<TrnmProductLoginRequest>,
) -> Response {
    post_trnm_player_session_issue(
        State(state),
        Json(TrnmPlayerSessionIssueRequest {
            player_id: request.player_id,
            recovery_key: request.credential,
            device_id: request.device_id,
            lifetime_seconds: request.lifetime_seconds,
        }),
    )
    .await
}

pub async fn post_trnm_product_credential_rotate(
    State(state): State<AppState>,
    Json(request): Json<TrnmProductCredentialRotateRequest>,
) -> Response {
    if let Err(error) = state
        .repository
        .authenticate_trnm_player_identity(&request.player_id, &request.credential)
        .await
    {
        return repository_error_response(error).into_response();
    }
    match state
        .repository
        .recover_trnm_player_identity(
            &request.player_id,
            &request.credential,
            &request.new_credential,
        )
        .await
    {
        Ok(identity) => (StatusCode::OK, Json(identity)).into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_identity_appeal(
    State(state): State<AppState>,
    Json(request): Json<TrnmIdentityAppealRequest>,
) -> Response {
    match state
        .repository
        .submit_trnm_identity_appeal(&request.player_id, &request.credential, &request.message)
        .await
    {
        Ok(appeal) => (StatusCode::CREATED, Json(appeal)).into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_identity_appeal_resolve(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmIdentityAppealResolveRequest>,
) -> Response {
    if let Err(response) = authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        return response;
    }
    let appeal_id = match Uuid::parse_str(&request.appeal_id) {
        Ok(value) => value,
        Err(_) => return bad_request_response("appeal_id must be a UUID"),
    };
    match state
        .repository
        .resolve_trnm_identity_appeal(appeal_id, &request.decision, &request.resolution)
        .await
    {
        Ok(appeal) => (StatusCode::OK, Json(appeal)).into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_player_session_issue(
    State(state): State<AppState>,
    Json(request): Json<TrnmPlayerSessionIssueRequest>,
) -> Response {
    let identity = match state
        .repository
        .authenticate_trnm_player_identity(&request.player_id, &request.recovery_key)
        .await
    {
        Ok(identity) => identity,
        Err(error) => return repository_error_response(error).into_response(),
    };
    let now = Utc::now().timestamp();
    let lifetime = request.lifetime_seconds.clamp(60, 86_400);
    let session_id = Uuid::new_v4();
    let claims = TrnmPlayerSessionClaimsV1 {
        contract_version: PLAYER_SESSION_CONTRACT_VERSION.to_string(),
        session_id: session_id.to_string(),
        player_id: identity.player_id.clone(),
        account_id: identity.account_id.to_string(),
        device_id: request.device_id.clone(),
        recovery_generation: identity.recovery_generation,
        issued_at_epoch: now,
        expires_at_epoch: now.saturating_add(lifetime),
    };
    let session_token = match sign_player_session(&state, &claims) {
        Ok(token) => token,
        Err(error) => return internal_error_response(error),
    };
    let token_hash = sha256_hex(session_token.as_bytes());
    let record = match state
        .repository
        .create_trnm_player_session(
            &request.player_id,
            &request.recovery_key,
            &request.device_id,
            session_id,
            &token_hash,
            claims.issued_at_epoch,
            claims.expires_at_epoch,
        )
        .await
    {
        Ok(record) => record,
        Err(error) => return repository_error_response(error).into_response(),
    };
    (
        StatusCode::CREATED,
        Json(TrnmPlayerSessionResponse {
            contract_version: PLAYER_SESSION_CONTRACT_VERSION.to_string(),
            session_token,
            session_id: record.session_id.to_string(),
            player_id: record.player_id,
            account_id: record.account_id.to_string(),
            device_id: record.device_id,
            recovery_generation: record.recovery_generation,
            issued_at_epoch: record.issued_at_epoch,
            expires_at_epoch: record.expires_at_epoch,
        }),
    )
        .into_response()
}

pub async fn post_trnm_player_session_revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmPlayerSessionRevokeRequest>,
) -> Response {
    let session_id = match Uuid::parse_str(&request.session_id) {
        Ok(value) => value,
        Err(_) => return bad_request_response("session_id must be a UUID"),
    };
    let claims = match player_session_claims_from_headers(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if claims.session_id != request.session_id {
        return unauthorized_response("a player session may only revoke itself");
    }
    match state
        .repository
        .revoke_trnm_player_session(session_id, &request.reason)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({"revoked": true}))).into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_player_session_verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmPlayerSessionVerifyRequest>,
) -> Response {
    let claims = match player_session_claims_from_headers(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if claims.player_id != request.player_id || claims.account_id != request.account_id {
        return unauthorized_response("player session does not own the requested actor/account");
    }
    let session_id = match Uuid::parse_str(&claims.session_id) {
        Ok(value) => value,
        Err(_) => return unauthorized_response("invalid player session id"),
    };
    let account_id = match Uuid::parse_str(&claims.account_id) {
        Ok(value) => value,
        Err(_) => return unauthorized_response("invalid player session account id"),
    };
    let token_hash = sha256_hex(
        headers
            .get(PLAYER_SESSION_HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .as_bytes(),
    );
    match state
        .repository
        .verify_trnm_player_session(
            session_id,
            &token_hash,
            &claims.player_id,
            account_id,
            claims.recovery_generation,
        )
        .await
    {
        Ok(record) => (
            StatusCode::OK,
            Json(TrnmPlayerSessionVerifyResponse {
                verified: true,
                session_id: record.session_id.to_string(),
                player_id: record.player_id,
                account_id: record.account_id.to_string(),
                device_id: record.device_id,
                recovery_generation: record.recovery_generation,
                expires_at_epoch: record.expires_at_epoch,
            }),
        )
            .into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_value_entitlement_issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmValueEntitlementIssueRequest>,
) -> Response {
    if let Err(error) = authorize_game_authority(&state, &headers) {
        return unauthorized_response(error.0);
    }
    if request.amount_credits <= 0
        || request.amount_credits > BATTLE_WALLET_REWARD_PER_EVENT_CAP
        || !matches!(request.source, ValueEntitlementSource::Battle)
    {
        return bad_request_response(
            "only positive battle entitlements within the per-event cap may mint wallet value",
        );
    }
    if Uuid::parse_str(&request.account_id).is_err()
        || request.actor_id.trim().is_empty()
        || request.source_id.trim().is_empty()
        || request.intent_id.trim().is_empty()
    {
        return bad_request_response("entitlement actor/account/source/intent is invalid");
    }
    let now = Utc::now();
    let lifetime = request.lifetime_seconds.clamp(60, 3_600);
    let budget_day = (now.year() as u32) * 10_000 + now.month() * 100 + now.day();
    let mut entitlement = ServerSignedValueEntitlementV1 {
        contract_version: SERVER_SIGNED_VALUE_ENTITLEMENT_CONTRACT.to_string(),
        entitlement_id: format!("trnm-entitlement:{}", Uuid::new_v4()),
        issuer: "cex-trusted-game-authority".to_string(),
        key_id: state.entitlement_key_id.as_ref().clone(),
        actor_id: request.actor_id,
        account_id: request.account_id,
        source: request.source,
        source_id: request.source_id,
        intent_id: request.intent_id,
        amount_credits: request.amount_credits,
        currency: "wallet_credits".to_string(),
        budget_day,
        issued_at_epoch: now.timestamp(),
        expires_at_epoch: now.timestamp().saturating_add(lifetime),
        signature: String::new(),
    };
    entitlement.signature = match sign_entitlement(&state, &entitlement) {
        Ok(signature) => signature,
        Err(error) => return internal_error_response(error),
    };
    (StatusCode::CREATED, Json(entitlement)).into_response()
}

pub async fn post_trnm_entitlement_issuer_key_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmEntitlementIssuerKeyStatusRequest>,
) -> Response {
    if let Err(error) = authorize_game_authority(&state, &headers) {
        return unauthorized_response(error.0);
    }
    let Some(key) = state.entitlement_issuer_keys.get(&request.key_id) else {
        return unauthorized_response("entitlement issuer key is not registered");
    };
    let public_key = match STANDARD.decode(&key.public_key_base64) {
        Ok(value) if value.len() == 32 => value,
        _ => {
            return internal_error_response(
                "registered entitlement public key is malformed".to_string(),
            )
        }
    };
    (
        StatusCode::OK,
        Json(TrnmEntitlementIssuerKeyStatusResponse {
            key_id: request.key_id,
            issuer: key.issuer.clone(),
            status: key.status.clone(),
            signature_algorithm: "ed25519".to_string(),
            public_key_sha256: sha256_hex(&public_key),
        }),
    )
        .into_response()
}

struct GameAuthorityError(&'static str);

fn authorize_game_authority(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), GameAuthorityError> {
    let supplied = headers
        .get(GAME_AUTHORITY_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if supplied.is_empty()
        || sha256_hex(supplied.as_bytes()) != sha256_hex(state.game_authority_token.as_bytes())
    {
        return Err(GameAuthorityError(
            "x-trnm-game-authority is required for value issuance",
        ));
    }
    Ok(())
}

pub async fn get_trnm_economic_receipts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) =
        authorize_ledger_admin(&state, &headers, &["ledger:read", "ledger:manage"])
    {
        return response;
    }
    match state.repository.list_trnm_economic_receipts().await {
        Ok(receipts) => (StatusCode::OK, Json(receipts)).into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_economy_maintenance(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        return response;
    }
    match state.repository.maintain_trnm_native_economy().await {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(error) => repository_error_response(error).into_response(),
    }
}

pub async fn post_trnm_economic_intent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmEconomicIntentRequest>,
) -> Response {
    if let Err(response) = authorize_trnm_economy_intent(&state, &headers, &request.intent).await {
        return response;
    }
    if let Err(error) = request.intent.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error,
                message: None,
            }),
        )
            .into_response();
    }
    if let Err(response) = validate_value_authorization(&state, &request.intent) {
        return response;
    }
    match state
        .repository
        .execute_trnm_economic_intent(&request.intent)
        .await
    {
        Ok(receipt) => (StatusCode::OK, Json(receipt)).into_response(),
        Err(LedgerActionError::RepositoryUnavailable(_)) if !state.fail_fast => {
            match execute_trnm_intent_in_memory(&state, &request.intent).await {
                Ok(receipt) => (StatusCode::OK, Json(receipt)).into_response(),
                Err(response) => response,
            }
        }
        Err(error) => repository_error_response(error).into_response(),
    }
}

async fn execute_trnm_intent_in_memory(
    state: &AppState,
    intent: &EconomicIntent,
) -> Result<EconomicReceipt, Response> {
    let account_id = intent
        .actors
        .first()
        .and_then(|actor| actor.account_id.as_deref())
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| {
            repository_error_response(LedgerActionError::AccountNotFound).into_response()
        })?;
    if state
        .idempotency_keys
        .read()
        .await
        .contains(&intent.idempotency_key.key)
    {
        let mut receipt = EconomicReceipt::from_intent(
            format!("cex-local-receipt:{}", intent.intent_id),
            intent,
            CEX_SETTLEMENT_BACKEND_ID,
            SettlementBackendKind::LocalTest,
            ReceiptStatus::Duplicate,
            0,
        );
        receipt.evidence = json!({"persistent": false, "local_dev_fallback": true});
        return Ok(receipt);
    }
    let amount = intent.amount_credits.unwrap_or_default().max(0) as f64;
    let mut accounts = state.accounts.write().await;
    let account = accounts.get_mut(&account_id).ok_or_else(|| {
        repository_error_response(LedgerActionError::AccountNotFound).into_response()
    })?;
    let status = match intent.kind {
        EconomicIntentKind::ReleaseReward | EconomicIntentKind::CompleteContract
            if amount > 0.0 =>
        {
            account.balance += amount;
            ReceiptStatus::ApprovedRelease
        }
        EconomicIntentKind::Reserve if account.balance - account.reserved >= amount => {
            account.reserved += amount;
            ReceiptStatus::Reserved
        }
        EconomicIntentKind::Refund if account.reserved >= amount => {
            account.reserved -= amount;
            ReceiptStatus::Refunded
        }
        EconomicIntentKind::Settle => {
            account.balance += amount;
            ReceiptStatus::Settled
        }
        EconomicIntentKind::Consume | EconomicIntentKind::Chargeback
            if account.reserved >= amount =>
        {
            account.reserved -= amount;
            account.balance -= amount;
            if matches!(intent.kind, EconomicIntentKind::Chargeback) {
                ReceiptStatus::SellerChargebackConsumed
            } else {
                ReceiptStatus::Consumed
            }
        }
        EconomicIntentKind::ReleaseReward | EconomicIntentKind::CompleteContract => {
            ReceiptStatus::SkippedZeroReward
        }
        _ => ReceiptStatus::FailedLedger,
    };
    drop(accounts);
    state
        .idempotency_keys
        .write()
        .await
        .insert(intent.idempotency_key.key.clone());
    let mut receipt = EconomicReceipt::from_intent(
        format!("cex-local-receipt:{}", intent.intent_id),
        intent,
        CEX_SETTLEMENT_BACKEND_ID,
        SettlementBackendKind::LocalTest,
        status,
        0,
    );
    receipt.evidence = json!({"persistent": false, "local_dev_fallback": true});
    Ok(receipt)
}

pub async fn post_trnm_wallet_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmWalletRequest>,
) -> Response {
    if request.actor_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "actor_id is required".to_string(),
                message: None,
            }),
        )
            .into_response();
    }
    let account_id = match Uuid::parse_str(&request.account_id) {
        Ok(account_id) => account_id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "account_id must be a UUID".to_string(),
                    message: None,
                }),
            )
                .into_response()
        }
    };
    if let Err(response) =
        authorize_trnm_account(&state, &headers, &request.actor_id, account_id).await
    {
        return response;
    }
    match state
        .repository
        .reconcile_trnm_wallet(&request.actor_id, account_id, request.reconciliation_cursor)
        .await
    {
        Ok(snapshot) => (StatusCode::OK, Json::<WalletSnapshot>(snapshot)).into_response(),
        Err(LedgerActionError::RepositoryUnavailable(_)) if !state.fail_fast => {
            let accounts = state.accounts.read().await;
            match accounts.get(&account_id) {
                Some(account) => (
                    StatusCode::OK,
                    Json(WalletSnapshot {
                        account_id: account_id.to_string(),
                        available_credits: (account.balance - account.reserved).round() as i64,
                        reserved_credits: account.reserved.round() as i64,
                        observed_at_cursor: request.reconciliation_cursor,
                    }),
                )
                    .into_response(),
                None => {
                    repository_error_response(LedgerActionError::AccountNotFound).into_response()
                }
            }
        }
        Err(error) => repository_error_response(error).into_response(),
    }
}

async fn authorize_trnm_economy_intent(
    state: &AppState,
    headers: &HeaderMap,
    intent: &EconomicIntent,
) -> Result<(), Response> {
    let actor = intent
        .actors
        .first()
        .ok_or_else(|| bad_request_response("TRNM economic intent requires a primary actor"))?;
    let account_id = actor
        .account_id
        .as_deref()
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| bad_request_response("TRNM actor account_id must be a UUID"))?;
    authorize_trnm_account(state, headers, &actor.actor_id, account_id).await
}

async fn authorize_trnm_account(
    state: &AppState,
    headers: &HeaderMap,
    actor_id: &str,
    account_id: Uuid,
) -> Result<(), Response> {
    if headers.contains_key(GAME_AUTHORITY_HEADER) {
        authorize_game_authority(state, headers).map_err(|error| unauthorized_response(error.0))?;
        return Ok(());
    }
    if headers
        .get(SYSTEM_OPERATION_HEADER)
        .and_then(|value| value.to_str().ok())
        == Some("true")
    {
        if !state.allow_system_economy_operations {
            return Err(unauthorized_response(
                "trusted system economy operations are disabled",
            ));
        }
        authorize_ledger_admin(state, headers, &["ledger:manage"])?;
        return Ok(());
    }

    if headers.contains_key(PLAYER_SESSION_HEADER) {
        let claims = player_session_claims_from_headers(state, headers)?;
        if claims.player_id != actor_id || claims.account_id != account_id.to_string() {
            return Err(unauthorized_response(
                "player session does not own the requested actor/account",
            ));
        }
        state
            .repository
            .verify_trnm_player_session(
                Uuid::parse_str(&claims.session_id)
                    .map_err(|_| unauthorized_response("invalid player session id"))?,
                &sha256_hex(
                    headers
                        .get(PLAYER_SESSION_HEADER)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .as_bytes(),
                ),
                actor_id,
                account_id,
                claims.recovery_generation,
            )
            .await
            .map_err(|error| repository_error_response(error).into_response())?;
        return Ok(());
    }

    if !state.require_player_session {
        authorize_ledger_admin(state, headers, &["ledger:manage"])?;
        return Ok(());
    }
    Err(unauthorized_response(
        "x-trnm-player-session is required for player economy requests",
    ))
}

// Axum responses intentionally carry the complete fail-closed HTTP error at this boundary.
#[allow(clippy::result_large_err)]
fn validate_value_authorization(state: &AppState, intent: &EconomicIntent) -> Result<(), Response> {
    let amount = intent.amount_credits.unwrap_or_default();
    if matches!(intent.kind, EconomicIntentKind::CompleteContract) {
        if amount != 0 {
            return Err(bad_request_response(
                "CompleteContract is audit-only and must have amount_credits=0",
            ));
        }
        return Ok(());
    }
    if !matches!(intent.kind, EconomicIntentKind::ReleaseReward) || amount <= 0 {
        return Ok(());
    }
    if amount > BATTLE_WALLET_REWARD_PER_EVENT_CAP {
        return Err(bad_request_response(
            "ReleaseReward exceeds the server-enforced per-event cap",
        ));
    }
    let value = intent
        .metadata
        .get(SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY)
        .ok_or_else(|| unauthorized_response("server-signed value entitlement is required"))?;
    if value
        .get("contract_version")
        .and_then(|value| value.as_str())
        == Some(SERVER_SIGNED_VALUE_ENTITLEMENT_V2_CONTRACT)
    {
        return validate_v2_value_authorization(state, intent, value);
    }
    let entitlement: ServerSignedValueEntitlementV1 = serde_json::from_value(value.clone())
        .map_err(|_| unauthorized_response("value entitlement cannot be decoded"))?;
    entitlement
        .validate_shape()
        .map_err(|error| unauthorized_response(&error))?;
    let actor = intent
        .actors
        .first()
        .ok_or_else(|| bad_request_response("TRNM economic intent requires a primary actor"))?;
    if entitlement.key_id != *state.entitlement_key_id
        || entitlement.actor_id != actor.actor_id
        || entitlement.account_id != actor.account_id.clone().unwrap_or_default()
        || entitlement.intent_id != intent.intent_id
        || entitlement.amount_credits != amount
        || !matches!(entitlement.source, ValueEntitlementSource::Battle)
    {
        return Err(unauthorized_response(
            "value entitlement does not bind this battle reward intent",
        ));
    }
    let now = Utc::now();
    let today = (now.year() as u32) * 10_000 + now.month() * 100 + now.day();
    if entitlement.budget_day != today
        || entitlement.issued_at_epoch > now.timestamp().saturating_add(30)
        || entitlement.expires_at_epoch < now.timestamp()
    {
        return Err(unauthorized_response(
            "value entitlement is expired or outside today's wallet budget",
        ));
    }
    let payload = entitlement
        .signing_payload()
        .map_err(|error| unauthorized_response(&error))?;
    verify_hmac_base64(
        state.entitlement_signing_secret.as_bytes(),
        &payload,
        &entitlement.signature,
    )
    .map_err(|error| unauthorized_response(&error))
}

#[allow(clippy::result_large_err)]
fn validate_v2_value_authorization(
    state: &AppState,
    intent: &EconomicIntent,
    value: &serde_json::Value,
) -> Result<(), Response> {
    let entitlement: ServerSignedValueEntitlementV2 = serde_json::from_value(value.clone())
        .map_err(|_| unauthorized_response("v2 value entitlement cannot be decoded"))?;
    entitlement
        .validate_shape()
        .map_err(|error| unauthorized_response(&error))?;
    let actor = intent
        .actors
        .first()
        .ok_or_else(|| bad_request_response("TRNM economic intent requires a primary actor"))?;
    if entitlement.actor_id != actor.actor_id
        || entitlement.account_id != actor.account_id.clone().unwrap_or_default()
        || entitlement.intent_id != intent.intent_id
        || entitlement.amount_credits != intent.amount_credits.unwrap_or_default()
        || entitlement.source_id
            != intent
                .metadata
                .get("value_event_id")
                .and_then(|value| value.as_str())
                .unwrap_or(&intent.intent_id)
        || !matches!(entitlement.source, ValueEntitlementSource::Battle)
    {
        return Err(unauthorized_response(
            "v2 value entitlement does not bind this authoritative match reward",
        ));
    }
    for (entitlement_value, metadata_key) in [
        (&entitlement.match_id, "online_match_id"),
        (&entitlement.rules_version, "online_rules_version"),
        (&entitlement.build_id, "online_build_id"),
        (&entitlement.result_hash, "online_result_hash"),
        (&entitlement.participants_hash, "online_participants_hash"),
    ] {
        if intent
            .metadata
            .get(metadata_key)
            .and_then(|value| value.as_str())
            != Some(entitlement_value.as_str())
        {
            return Err(unauthorized_response(
                "v2 value entitlement does not bind authoritative match metadata",
            ));
        }
    }
    let now = Utc::now();
    let today = (now.year() as u32) * 10_000 + now.month() * 100 + now.day();
    if entitlement.budget_day != today
        || entitlement.issued_at_epoch > now.timestamp().saturating_add(30)
        || entitlement.expires_at_epoch < now.timestamp()
    {
        return Err(unauthorized_response(
            "v2 value entitlement is expired or outside today's wallet budget",
        ));
    }
    let issuer_key = state
        .entitlement_issuer_keys
        .get(&entitlement.key_id)
        .ok_or_else(|| unauthorized_response("v2 entitlement issuer key is not registered"))?;
    if issuer_key.status != "active" || issuer_key.issuer != entitlement.issuer {
        return Err(unauthorized_response(
            "v2 entitlement issuer key is revoked or does not own this issuer",
        ));
    }
    let public_key = STANDARD
        .decode(&issuer_key.public_key_base64)
        .map_err(|_| unauthorized_response("v2 entitlement issuer public key is malformed"))?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| unauthorized_response("v2 entitlement issuer public key is malformed"))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| unauthorized_response("v2 entitlement issuer public key is invalid"))?;
    let signature = STANDARD
        .decode(&entitlement.signature)
        .map_err(|_| unauthorized_response("v2 entitlement signature is malformed"))?;
    let signature = Signature::from_slice(&signature)
        .map_err(|_| unauthorized_response("v2 entitlement signature is malformed"))?;
    let payload = entitlement
        .signing_payload()
        .map_err(|error| unauthorized_response(&error))?;
    verifying_key
        .verify(&payload, &signature)
        .map_err(|_| unauthorized_response("v2 entitlement signature is invalid"))
}

fn sign_entitlement(
    state: &AppState,
    entitlement: &ServerSignedValueEntitlementV1,
) -> Result<String, String> {
    sign_hmac_base64(
        state.entitlement_signing_secret.as_bytes(),
        &entitlement.signing_payload()?,
    )
}

fn sign_player_session(
    state: &AppState,
    claims: &TrnmPlayerSessionClaimsV1,
) -> Result<String, String> {
    let payload = serde_json::to_vec(claims)
        .map_err(|error| format!("encode player session failed: {error}"))?;
    let signature = sign_hmac_base64(state.player_session_signing_secret.as_bytes(), &payload)?;
    Ok(format!("{}.{}", URL_SAFE_NO_PAD.encode(payload), signature))
}

// Returning the complete Axum response keeps authentication failures uniform at every route.
#[allow(clippy::result_large_err)]
fn player_session_claims_from_headers(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<TrnmPlayerSessionClaimsV1, Response> {
    let token = headers
        .get(PLAYER_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| unauthorized_response("x-trnm-player-session is required"))?;
    let (payload_encoded, signature) = token
        .split_once('.')
        .ok_or_else(|| unauthorized_response("player session token is malformed"))?;
    let payload = URL_SAFE_NO_PAD
        .decode(payload_encoded)
        .map_err(|_| unauthorized_response("player session payload is malformed"))?;
    verify_hmac_base64(
        state.player_session_signing_secret.as_bytes(),
        &payload,
        signature,
    )
    .map_err(|error| unauthorized_response(&error))?;
    let claims: TrnmPlayerSessionClaimsV1 = serde_json::from_slice(&payload)
        .map_err(|_| unauthorized_response("player session claims are malformed"))?;
    if claims.contract_version != PLAYER_SESSION_CONTRACT_VERSION
        || claims.expires_at_epoch < Utc::now().timestamp()
        || claims.issued_at_epoch > Utc::now().timestamp().saturating_add(30)
    {
        return Err(unauthorized_response(
            "player session is expired or invalid",
        ));
    }
    Ok(claims)
}

fn sign_hmac_base64(secret: &[u8], payload: &[u8]) -> Result<String, String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret)
        .map_err(|_| "invalid HMAC signing secret".to_string())?;
    mac.update(payload);
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

fn verify_hmac_base64(secret: &[u8], payload: &[u8], signature: &str) -> Result<(), String> {
    let decoded = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| "invalid HMAC signature encoding".to_string())?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret)
        .map_err(|_| "invalid HMAC verification secret".to_string())?;
    mac.update(payload);
    mac.verify_slice(&decoded)
        .map_err(|_| "HMAC signature verification failed".to_string())
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn bad_request_response(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: message.to_string(),
            message: None,
        }),
    )
        .into_response()
}

fn unauthorized_response(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: message.to_string(),
            message: None,
        }),
    )
        .into_response()
}

fn internal_error_response(message: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: message,
            message: None,
        }),
    )
        .into_response()
}

pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let accounts = state.accounts.read().await.len();
    let entries = state.entries.read().await.len();
    let idempotency_keys = state.idempotency_keys.read().await.len();
    let body = format!(
        concat!(
            "# HELP cex_ledger_service_up Whether ledger-service metrics are being served.\n",
            "# TYPE cex_ledger_service_up gauge\n",
            "cex_ledger_service_up 1\n",
            "# HELP cex_ledger_memory_accounts_total In-memory ledger accounts.\n",
            "# TYPE cex_ledger_memory_accounts_total gauge\n",
            "cex_ledger_memory_accounts_total {accounts}\n",
            "# HELP cex_ledger_memory_entries_total In-memory ledger entries.\n",
            "# TYPE cex_ledger_memory_entries_total gauge\n",
            "cex_ledger_memory_entries_total {entries}\n",
            "# HELP cex_ledger_memory_idempotency_keys_total In-memory ledger idempotency keys.\n",
            "# TYPE cex_ledger_memory_idempotency_keys_total gauge\n",
            "cex_ledger_memory_idempotency_keys_total {idempotency_keys}\n",
            "# HELP cex_ledger_admin_tokens_total Ledger admin tokens currently loaded.\n",
            "# TYPE cex_ledger_admin_tokens_total gauge\n",
            "cex_ledger_admin_tokens_total {admin_tokens}\n",
        ),
        accounts = accounts,
        entries = entries,
        idempotency_keys = idempotency_keys,
        admin_tokens = state.admin_tokens.len(),
    );
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}

pub async fn create_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateAccountRequest>,
) -> Response {
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };

    if let Err(response) = enforce_org_boundary(&admin, &req.org_id) {
        return response;
    }

    let record = AccountRecord {
        account_id: Uuid::new_v4(),
        org_id: req.org_id,
        account_type: req.account_type,
        currency_unit: req.currency_unit.unwrap_or_else(|| "credit".to_string()),
        balance: req.initial_balance.unwrap_or(0.0),
        reserved: 0.0,
    };

    if let Err(err) = state.repository.create_account(&record).await {
        if state.fail_fast {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("create_account repository failure: {err}"),
                    message: None,
                }),
            )
                .into_response();
        }
    }

    state
        .accounts
        .write()
        .await
        .insert(record.account_id, record.clone());
    (StatusCode::CREATED, Json(record)).into_response()
}

pub async fn get_account(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    match require_account_access(&state, &headers, id, &["ledger:read", "ledger:manage"]).await {
        Ok(account) => (StatusCode::OK, Json(account)).into_response(),
        Err(response) => response,
    }
}

pub async fn reserve_credits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LedgerActionRequest>,
) -> impl IntoResponse {
    if let Err(response) =
        require_account_access(&state, &headers, req.account_id, &["ledger:manage"]).await
    {
        return response;
    }
    apply_action(state, req, "reserve").await
}

pub async fn consume_credits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LedgerActionRequest>,
) -> impl IntoResponse {
    if let Err(response) =
        require_account_access(&state, &headers, req.account_id, &["ledger:manage"]).await
    {
        return response;
    }
    apply_action(state, req, "consume").await
}

pub async fn refund_credits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LedgerActionRequest>,
) -> impl IntoResponse {
    if let Err(response) =
        require_account_access(&state, &headers, req.account_id, &["ledger:manage"]).await
    {
        return response;
    }
    apply_action(state, req, "refund").await
}

pub async fn grant_credits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LedgerActionRequest>,
) -> impl IntoResponse {
    if let Err(response) =
        require_account_access(&state, &headers, req.account_id, &["ledger:manage"]).await
    {
        return response;
    }
    apply_action(state, req, "grant").await
}

async fn apply_action(state: AppState, req: LedgerActionRequest, action: &str) -> Response {
    if req.amount <= 0.0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "amount must be positive".to_string(),
                message: None,
            }),
        )
            .into_response();
    }

    let entry = LedgerEntryRecord {
        entry_id: Uuid::new_v4(),
        account_id: req.account_id,
        action: action.to_string(),
        amount: req.amount,
        reference_id: req.reference_id.clone(),
        idempotency_key: req.idempotency_key.clone(),
    };

    match apply_action_via_repository(&state, &entry).await {
        Ok(account) => return success_response(state.fail_fast, account, entry).into_response(),
        Err(err) if should_fallback_to_memory(&err, state.fail_fast) => {}
        Err(err) => return repository_error_response(err).into_response(),
    }

    apply_action_in_memory(state, entry).await
}

#[allow(clippy::result_large_err)]
fn authorize_ledger_admin(
    state: &AppState,
    headers: &HeaderMap,
    required_scopes: &[&str],
) -> Result<AdminPrincipal, Response> {
    authorize_scoped_admin_from_map(
        headers,
        &state.admin_tokens,
        required_scopes,
        admin_principal_has_scope,
        "ledger admin token not configured",
        Some(
            "set LEDGER_ADMIN_TOKENS_JSON, LEDGER_ADMIN_TOKEN, or the shared identity admin token env to enable ledger admin access",
        ),
    )
    .cloned()
    .map_err(|err: AdminAuthorizationFailure| err.into_response().into_response())
}

#[allow(clippy::result_large_err)]
fn enforce_org_boundary(admin: &AdminPrincipal, org_id: &str) -> Result<(), Response> {
    if admin.org_ids.is_empty() || admin_principal_allows_org(admin, org_id) {
        return Ok(());
    }

    Err((
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            error: "admin token not authorized for org".to_string(),
            message: Some(org_id.to_string()),
        }),
    )
        .into_response())
}

async fn load_account_record(
    state: &AppState,
    id: Uuid,
) -> Result<Option<AccountRecord>, Response> {
    match state.repository.get_account(id).await {
        Ok(Some(account)) => return Ok(Some(account)),
        Ok(None) => {}
        Err(err) if state.fail_fast => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("get_account repository failure: {err}"),
                    message: None,
                }),
            )
                .into_response())
        }
        Err(_) => {}
    }

    let map = state.accounts.read().await;
    Ok(map.get(&id).cloned())
}

async fn require_account_access(
    state: &AppState,
    headers: &HeaderMap,
    account_id: Uuid,
    required_scopes: &[&str],
) -> Result<AccountRecord, Response> {
    let admin = authorize_ledger_admin(state, headers, required_scopes)?;
    let Some(account) = load_account_record(state, account_id).await? else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "account not found".to_string(),
                message: None,
            }),
        )
            .into_response());
    };

    enforce_org_boundary(&admin, &account.org_id)?;
    Ok(account)
}

async fn apply_action_via_repository(
    state: &AppState,
    entry: &LedgerEntryRecord,
) -> Result<AccountRecord, LedgerActionError> {
    let account = match entry.action.as_str() {
        "reserve" => state.repository.reserve_credits(entry).await?,
        "consume" => state.repository.consume_credits(entry).await?,
        "refund" => state.repository.refund_credits(entry).await?,
        "grant" => state.repository.grant_credits(entry).await?,
        _ => {
            return Err(LedgerActionError::Other(
                "unsupported ledger action".to_string(),
            ))
        }
    };

    state
        .accounts
        .write()
        .await
        .insert(account.account_id, account.clone());
    if let Some(key) = &entry.idempotency_key {
        state.idempotency_keys.write().await.insert(key.clone());
    }
    state.entries.write().await.push(entry.clone());

    Ok(account)
}

fn should_fallback_to_memory(err: &LedgerActionError, fail_fast: bool) -> bool {
    if fail_fast {
        return false;
    }

    matches!(err, LedgerActionError::RepositoryUnavailable(_))
}

fn repository_error_response(err: LedgerActionError) -> (StatusCode, Json<ErrorResponse>) {
    match err {
        LedgerActionError::RepositoryUnavailable(message) | LedgerActionError::Other(message) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: message,
                message: None,
            }),
        ),
        LedgerActionError::AccountNotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "account not found".to_string(),
                message: None,
            }),
        ),
        LedgerActionError::DuplicateIdempotencyKey => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "duplicate idempotency key".to_string(),
                message: None,
            }),
        ),
        LedgerActionError::IdentityRejected(message) => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: message,
                message: None,
            }),
        ),
        LedgerActionError::InsufficientAvailable { available, requested } => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "insufficient available balance for reserve: available={available:.6}, requested={requested:.6}"
                ),
                message: None,
            }),
        ),
        LedgerActionError::InsufficientReserved { reserved, requested } => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "insufficient reserved balance: reserved={reserved:.6}, requested={requested:.6}"
                ),
                message: None,
            }),
        ),
    }
}

async fn apply_action_in_memory(state: AppState, entry: LedgerEntryRecord) -> Response {
    if let Some(key) = &entry.idempotency_key {
        if let Ok(Some(_)) = state.repository.find_by_idempotency_key(key).await {
            return (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "duplicate idempotency key".to_string(),
                    message: None,
                }),
            )
                .into_response();
        }

        let keys = state.idempotency_keys.read().await;
        if keys.contains(key) {
            return (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "duplicate idempotency key".to_string(),
                    message: None,
                }),
            )
                .into_response();
        }
    }

    let updated_account = {
        let mut accounts = state.accounts.write().await;
        let account = match accounts.get_mut(&entry.account_id) {
            Some(account) => account,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "account not found".to_string(),
                        message: None,
                    }),
                )
                    .into_response();
            }
        };

        match entry.action.as_str() {
            "reserve" => {
                let available = account.balance - account.reserved;
                if available < entry.amount {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "insufficient available balance for reserve".to_string(),
                            message: None,
                        }),
                    )
                        .into_response();
                }
                account.reserved += entry.amount;
            }
            "consume" => {
                if account.reserved < entry.amount {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "insufficient reserved balance for consume".to_string(),
                            message: None,
                        }),
                    )
                        .into_response();
                }
                account.reserved -= entry.amount;
                account.balance -= entry.amount;
            }
            "refund" => {
                if account.reserved < entry.amount {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "insufficient reserved balance for refund".to_string(),
                            message: None,
                        }),
                    )
                        .into_response();
                }
                account.reserved -= entry.amount;
            }
            "grant" => {
                account.balance += entry.amount;
            }
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "unsupported ledger action".to_string(),
                        message: None,
                    }),
                )
                    .into_response();
            }
        }

        account.clone()
    };

    if let Err(err) = state.repository.append_entry(&entry).await {
        if state.fail_fast {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("append_entry repository failure: {err}"),
                    message: None,
                }),
            )
                .into_response();
        }
    }

    state.entries.write().await.push(entry.clone());
    if let Some(key) = &entry.idempotency_key {
        state.idempotency_keys.write().await.insert(key.clone());
    }
    success_response(state.fail_fast, updated_account, entry).into_response()
}

fn success_response(
    fail_fast: bool,
    account: AccountRecord,
    entry: LedgerEntryRecord,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "account": account,
            "entry": entry,
            "fail_fast": fail_fast
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::postgres::PostgresLedgerRepository;
    use ed25519_dalek::{Signer, SigningKey};
    use std::sync::Arc;
    use term_exchange_protocol::{ActorRef, IdempotencyKey, TERM_EXCHANGE_PROTOCOL_VERSION};

    fn signed_v2_intent() -> (AppState, EconomicIntent) {
        let state = AppState::new_for_tests(
            PostgresLedgerRepository::new_placeholder(),
            false,
            None,
            Vec::new(),
            Vec::new(),
        );
        let now = Utc::now();
        let account_id = "00000000-0000-0000-0000-000000000001";
        let mut entitlement = ServerSignedValueEntitlementV2 {
            contract_version: SERVER_SIGNED_VALUE_ENTITLEMENT_V2_CONTRACT.to_string(),
            entitlement_id: "entitlement-test-v2".to_string(),
            issuer: "trnm-online-game-server".to_string(),
            key_id: "test-online-ed25519-v1".to_string(),
            signature_algorithm: "ed25519".to_string(),
            actor_id: "player-a".to_string(),
            account_id: account_id.to_string(),
            source: ValueEntitlementSource::Battle,
            source_id: "intent-v2".to_string(),
            intent_id: "intent-v2".to_string(),
            amount_credits: 25,
            currency: "wallet_credits".to_string(),
            budget_day: (now.year() as u32) * 10_000 + now.month() * 100 + now.day(),
            issued_at_epoch: now.timestamp(),
            expires_at_epoch: now.timestamp() + 600,
            match_id: "00000000-0000-0000-0000-000000000002".to_string(),
            rules_version: "rules-v2".to_string(),
            build_id: "build-v2".to_string(),
            result_hash: "1".repeat(64),
            participants_hash: "2".repeat(64),
            nonce: "nonce-v2".to_string(),
            signature: String::new(),
        };
        entitlement.signature = STANDARD.encode(
            SigningKey::from_bytes(&[7_u8; 32])
                .sign(&entitlement.signing_payload().expect("signing payload"))
                .to_bytes(),
        );
        let intent = EconomicIntent {
            protocol_version: TERM_EXCHANGE_PROTOCOL_VERSION.to_string(),
            intent_id: "intent-v2".to_string(),
            term_id: "battle-reward".to_string(),
            term_version: "1".to_string(),
            domain: "trnm".to_string(),
            kind: EconomicIntentKind::ReleaseReward,
            idempotency_key: IdempotencyKey {
                scope: "test".to_string(),
                key: "intent-v2".to_string(),
            },
            actors: vec![ActorRef {
                actor_id: "player-a".to_string(),
                actor_kind: "player".to_string(),
                account_id: Some(account_id.to_string()),
            }],
            assets: Vec::new(),
            amount_credits: Some(25),
            currency: Some("wallet_credits".to_string()),
            metadata: json!({
                SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY: entitlement,
                "online_match_id": "00000000-0000-0000-0000-000000000002",
                "online_rules_version": "rules-v2",
                "online_build_id": "build-v2",
                "online_result_hash": "1".repeat(64),
                "online_participants_hash": "2".repeat(64),
            }),
            created_at_epoch: now.timestamp(),
        };
        (state, intent)
    }

    #[test]
    fn v2_ed25519_entitlement_accepts_only_active_bound_issuer_signatures() {
        let (state, intent) = signed_v2_intent();
        assert!(validate_value_authorization(&state, &intent).is_ok());

        let mut tampered = intent.clone();
        tampered.metadata[SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY]["signature"] =
            json!(STANDARD.encode([0_u8; 64]));
        assert!(validate_value_authorization(&state, &tampered).is_err());

        let mut unknown = intent.clone();
        unknown.metadata[SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY]["key_id"] =
            json!("unknown-key");
        assert!(validate_value_authorization(&state, &unknown).is_err());

        let mut revoked_state = state.clone();
        let mut keys = revoked_state.entitlement_issuer_keys.as_ref().clone();
        keys.get_mut("test-online-ed25519-v1")
            .expect("test issuer")
            .status = "revoked".to_string();
        revoked_state.entitlement_issuer_keys = Arc::new(keys);
        assert!(validate_value_authorization(&revoked_state, &intent).is_err());
    }
}
