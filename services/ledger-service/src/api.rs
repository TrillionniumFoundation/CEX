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

pub async fn health() -> &'static str {
    "ledger-service ok"
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

fn authorize_ledger_admin(
    state: &AppState,
    headers: &HeaderMap,
    required_scopes: &[&str],
) -> Result<AdminPrincipal, Response> {
    authorize_scoped_admin_from_map(
        headers,
        &state.admin_tokens,
        required_scopes,
        |admin, scope| admin_principal_has_scope(admin, scope),
        "ledger admin token not configured",
        Some(
            "set LEDGER_ADMIN_TOKENS_JSON, LEDGER_ADMIN_TOKEN, or the shared identity admin token env to enable ledger admin access",
        ),
    )
    .cloned()
    .map_err(|err: AdminAuthorizationFailure| err.into_response().into_response())
}

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

    matches!(
        err,
        LedgerActionError::RepositoryUnavailable(_) | LedgerActionError::Other(_)
    )
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

        let mut keys = state.idempotency_keys.write().await;
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
        keys.insert(key.clone());
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
