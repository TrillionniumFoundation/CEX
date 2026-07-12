use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    AdminAuthorizationFailure, AdminPrincipal,
};
use term_exchange_protocol::{
    EconomicIntent, EconomicIntentKind, EconomicReceipt, ReceiptStatus, SettlementBackendKind,
    WalletSnapshot, CEX_SETTLEMENT_BACKEND_ID,
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

pub async fn health() -> &'static str {
    "ledger-service ok"
}

pub async fn trnm_economy_readiness(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "status": if state.fail_fast && state.repository.persistence_ready() { "ok" } else { "blocked" },
        "profile": "trnm-economy-production",
        "fail_fast": state.fail_fast,
        "postgres_persistent": state.repository.persistence_ready(),
        "atomic_intent_receipts": true,
        "escrow": true,
    }))
}

pub async fn post_trnm_economic_intent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TrnmEconomicIntentRequest>,
) -> Response {
    if let Err(response) = authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
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
    if let Err(response) =
        authorize_ledger_admin(&state, &headers, &["ledger:read", "ledger:manage"])
    {
        return response;
    }
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
