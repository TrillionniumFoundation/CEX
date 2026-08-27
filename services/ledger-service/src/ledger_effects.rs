use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    AdminAuthorizationFailure, AdminPrincipal,
};
use sqlx::Row;
use uuid::Uuid;

use crate::state::AppState;

const LEDGER_EFFECT_SCHEMA_V1: &str = "cex.ledger.effect.v1";
const LEDGER_SOURCE_SERVICE: &str = "ledger-service";
const TRACE_RESULT_LIMIT: i64 = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEffectRequestV1 {
    pub account_id: Uuid,
    pub trace_id: Option<Uuid>,
    pub operation_id: Option<Uuid>,
    pub operation_kind: String,
    pub amount_minor: i64,
    pub currency_scale: i16,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub idempotency_scope: String,
    pub idempotency_key: String,
}

pub async fn apply_effect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LedgerEffectRequestV1>,
) -> Response {
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let pool = match state.operation_pool.as_ref() {
        Some(pool) => pool,
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "ledger_operation_persistence_unavailable",
                "ledger operation persistence is unavailable",
            )
        }
    };

    if let Err((code, message)) = validate_request(&request) {
        return error_response(StatusCode::BAD_REQUEST, code, &message);
    }
    if state.require_explicit_ledger_trace && request.trace_id.is_none() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "explicit_trace_required",
            "trace_id is required by the production ledger operation profile",
        );
    }

    let account_org_id = match sqlx::query_scalar::<_, String>(
        "select org_id::text from public.accounts where account_id = $1",
    )
    .bind(request.account_id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(org_id)) => org_id,
        Ok(None) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "ledger_account_not_found",
                "account not found",
            )
        }
        Err(error) => return database_error_response("load account tenancy", error),
    };

    if let Err(response) = enforce_org_boundary(&admin, &account_org_id) {
        return response;
    }

    let scope = request.idempotency_scope.trim();
    let key = request.idempotency_key.trim();
    let operation_id = request
        .operation_id
        .unwrap_or_else(|| deterministic_operation_id(scope, key));
    let (trace_id, provenance_mode) = match request.trace_id {
        Some(trace_id) => (trace_id, "explicit"),
        None => (operation_id, "operation_scoped_compatibility"),
    };
    let source_principal = admin.actor_id.trim();
    if source_principal.is_empty() || source_principal.chars().count() > 256 {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid_ledger_source_principal",
            "the authenticated principal cannot be represented in the ledger contract",
        );
    }

    let result_text = sqlx::query_scalar::<_, String>(
        "select public.cex_apply_ledger_effect_v1(
            $1, $2, $3, $4, $5, $6,
            $7, $8, $9, $10, $11, $12, $13
        )::text",
    )
    .bind(request.account_id)
    .bind(trace_id)
    .bind(operation_id)
    .bind(request.operation_kind.trim())
    .bind(request.amount_minor)
    .bind(request.currency_scale)
    .bind(request.reference_type.as_deref().map(str::trim))
    .bind(request.reference_id)
    .bind(scope)
    .bind(key)
    .bind(LEDGER_SOURCE_SERVICE)
    .bind(source_principal)
    .bind(provenance_mode)
    .fetch_one(pool)
    .await;

    let result_text = match result_text {
        Ok(result) => result,
        Err(error) => return database_error_response("apply ledger effect", error),
    };
    let result: Value = match serde_json::from_str(&result_text) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("ledger-service: decode ledger operation response failed: {error}");
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "ledger_operation_response_invalid",
                "ledger operation response could not be verified",
            );
        }
    };
    let replayed = result
        .get("replayed")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    (
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        Json(result),
    )
        .into_response()
}

pub async fn get_effect(
    Path(operation_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if operation_id.is_nil() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_operation_id",
            "operation_id must not be the nil UUID",
        );
    }
    let admin = match authorize_ledger_admin(
        &state,
        &headers,
        &["ledger:read", "ledger:manage"],
    ) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let pool = match state.operation_pool.as_ref() {
        Some(pool) => pool,
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "ledger_operation_persistence_unavailable",
                "ledger operation persistence is unavailable",
            )
        }
    };

    let row = sqlx::query(
        "select
            account.org_id::text as org_id,
            public.cex_ledger_effect_result_v1(entry.entry_id, false)::text as result_text
         from public.ledger_entries entry
         join public.accounts account on account.account_id = entry.account_id
         where entry.operation_id = $1",
    )
    .bind(operation_id)
    .fetch_optional(pool)
    .await;

    let row = match row {
        Ok(Some(row)) => row,
        Ok(None) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "ledger_operation_not_found",
                "ledger operation not found",
            )
        }
        Err(error) => return database_error_response("get ledger effect", error),
    };
    let org_id: String = match row.try_get("org_id") {
        Ok(value) => value,
        Err(error) => return database_error_response("decode ledger effect tenancy", error.into()),
    };
    if let Err(response) = enforce_org_boundary(&admin, &org_id) {
        return response;
    }
    let result_text: String = match row.try_get("result_text") {
        Ok(value) => value,
        Err(error) => return database_error_response("decode ledger effect", error.into()),
    };
    decode_json_response(result_text)
}

pub async fn list_trace(
    Path(trace_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if trace_id.is_nil() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_trace_id",
            "trace_id must not be the nil UUID",
        );
    }
    let admin = match authorize_ledger_admin(
        &state,
        &headers,
        &["ledger:read", "ledger:manage"],
    ) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let pool = match state.operation_pool.as_ref() {
        Some(pool) => pool,
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "ledger_operation_persistence_unavailable",
                "ledger operation persistence is unavailable",
            )
        }
    };

    let rows = match sqlx::query(
        "select
            account.org_id::text as org_id,
            public.cex_ledger_effect_result_v1(entry.entry_id, false)::text as result_text
         from public.ledger_entries entry
         join public.accounts account on account.account_id = entry.account_id
         where entry.trace_id = $1
         order by entry.created_at, entry.entry_id
         limit $2",
    )
    .bind(trace_id)
    .bind(TRACE_RESULT_LIMIT)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => return database_error_response("list ledger trace", error),
    };

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let org_id: String = match row.try_get("org_id") {
            Ok(value) => value,
            Err(error) => {
                return database_error_response("decode ledger trace tenancy", error.into())
            }
        };
        if let Err(response) = enforce_org_boundary(&admin, &org_id) {
            return response;
        }
        let result_text: String = match row.try_get("result_text") {
            Ok(value) => value,
            Err(error) => return database_error_response("decode ledger trace", error.into()),
        };
        match serde_json::from_str::<Value>(&result_text) {
            Ok(value) => results.push(value),
            Err(error) => {
                eprintln!("ledger-service: decode ledger trace response failed: {error}");
                return error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "ledger_operation_response_invalid",
                    "ledger trace response could not be verified",
                );
            }
        }
    }

    (StatusCode::OK, Json(json!({
        "trace_id": trace_id,
        "limit": TRACE_RESULT_LIMIT,
        "effects": results,
    })))
        .into_response()
}

fn validate_request(request: &LedgerEffectRequestV1) -> Result<(), (&'static str, String)> {
    if request.account_id.is_nil() {
        return Err((
            "invalid_account_id",
            "account_id must not be the nil UUID".to_string(),
        ));
    }
    if request.trace_id.is_some_and(|value| value.is_nil()) {
        return Err((
            "invalid_trace_id",
            "trace_id must not be the nil UUID".to_string(),
        ));
    }
    if request.operation_id.is_some_and(|value| value.is_nil()) {
        return Err((
            "invalid_operation_id",
            "operation_id must not be the nil UUID".to_string(),
        ));
    }
    if !matches!(
        request.operation_kind.trim(),
        "reserve" | "consume" | "refund" | "grant"
    ) {
        return Err((
            "unsupported_operation_kind",
            "operation_kind must be reserve, consume, refund, or grant".to_string(),
        ));
    }
    if request.amount_minor <= 0 {
        return Err((
            "invalid_amount_minor",
            "amount_minor must be positive".to_string(),
        ));
    }
    if !(0..=6).contains(&request.currency_scale) {
        return Err((
            "invalid_currency_scale",
            "currency_scale must be between 0 and 6".to_string(),
        ));
    }
    validate_component(
        "invalid_idempotency_scope",
        "idempotency_scope",
        &request.idempotency_scope,
        160,
    )?;
    let key = request.idempotency_key.trim();
    if key.is_empty() || key.chars().count() > 256 || key.chars().any(char::is_control) {
        return Err((
            "invalid_idempotency_key",
            "idempotency_key must contain 1..256 non-control characters".to_string(),
        ));
    }
    if request.reference_type.is_some() != request.reference_id.is_some() {
        return Err((
            "invalid_reference_binding",
            "reference_type and reference_id must be supplied together".to_string(),
        ));
    }
    if let Some(reference_type) = request.reference_type.as_deref() {
        validate_component(
            "invalid_reference_type",
            "reference_type",
            reference_type,
            128,
        )?;
    }
    Ok(())
}

fn validate_component(
    code: &'static str,
    field: &str,
    raw: &str,
    maximum: usize,
) -> Result<(), (&'static str, String)> {
    let value = raw.trim();
    let valid = !value.is_empty()
        && value.len() <= maximum
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':')
        });
    if valid {
        Ok(())
    } else {
        Err((
            code,
            format!("{field} must use 1..{maximum} characters from [A-Za-z0-9._:-]"),
        ))
    }
}

fn deterministic_operation_id(scope: &str, key: &str) -> Uuid {
    let digest = Sha256::digest(format!("cex:ledger-operation:{scope}:{key}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
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
        Some("configure a scoped ledger principal before using ledger operation APIs"),
    )
    .cloned()
    .map_err(|error: AdminAuthorizationFailure| error.into_response().into_response())
}

#[allow(clippy::result_large_err)]
fn enforce_org_boundary(admin: &AdminPrincipal, org_id: &str) -> Result<(), Response> {
    if admin.org_ids.is_empty() || admin_principal_allows_org(admin, org_id) {
        return Ok(());
    }
    Err(error_response(
        StatusCode::FORBIDDEN,
        "ledger_org_forbidden",
        "authenticated principal is not authorized for the account organization",
    ))
}

fn database_error_response(context: &str, error: sqlx::Error) -> Response {
    let mut status = StatusCode::SERVICE_UNAVAILABLE;
    let mut code = "ledger_operation_unavailable";
    if let Some(database_error) = error.as_database_error() {
        let message = database_error.message().to_ascii_lowercase();
        if database_error.code().as_deref() == Some("23505") || message.contains("collision") {
            status = StatusCode::CONFLICT;
            code = "ledger_operation_collision";
        } else if database_error.code().as_deref() == Some("P0002")
            || message.contains("not found")
        {
            status = StatusCode::NOT_FOUND;
            code = "ledger_account_not_found";
        } else if message.contains("insufficient") {
            status = StatusCode::UNPROCESSABLE_ENTITY;
            code = "ledger_insufficient_funds";
        } else if message.contains("invalid")
            || message.contains("unsupported")
            || message.contains("must")
            || message.contains("differs")
            || message.contains("not active")
        {
            status = StatusCode::BAD_REQUEST;
            code = "ledger_operation_invalid";
        }
    }
    eprintln!("ledger-service: {context} failed: {error}");
    error_response(status, code, "ledger operation could not be completed")
}

fn decode_json_response(result_text: String) -> Response {
    match serde_json::from_str::<Value>(&result_text) {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => {
            eprintln!("ledger-service: decode ledger effect response failed: {error}");
            error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "ledger_operation_response_invalid",
                "ledger operation response could not be verified",
            )
        }
    }
}

fn error_response(status: StatusCode, code: &'static str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": "ledger operation rejected",
            "code": code,
            "message": message,
            "schema_version": LEDGER_EFFECT_SCHEMA_V1,
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> LedgerEffectRequestV1 {
        LedgerEffectRequestV1 {
            account_id: Uuid::new_v4(),
            trace_id: Some(Uuid::new_v4()),
            operation_id: None,
            operation_kind: "reserve".to_string(),
            amount_minor: 1_000_000,
            currency_scale: 6,
            reference_type: Some("invocation".to_string()),
            reference_id: Some(Uuid::new_v4()),
            idempotency_scope: "org:test:reserve".to_string(),
            idempotency_key: "reserve:test".to_string(),
        }
    }

    #[test]
    fn deterministic_operation_identity_is_stable_and_scoped() {
        let first = deterministic_operation_id("scope-a", "same-key");
        let replay = deterministic_operation_id("scope-a", "same-key");
        let other_scope = deterministic_operation_id("scope-b", "same-key");
        assert_eq!(first, replay);
        assert_ne!(first, other_scope);
        assert!(!first.is_nil());
    }

    #[test]
    fn request_validation_rejects_ambiguous_or_binary_float_shapes() {
        assert!(validate_request(&request()).is_ok());

        let mut invalid = request();
        invalid.amount_minor = 0;
        assert!(validate_request(&invalid).is_err());

        let mut invalid = request();
        invalid.reference_id = None;
        assert!(validate_request(&invalid).is_err());

        let mut invalid = request();
        invalid.idempotency_scope = "bad scope".to_string();
        assert!(validate_request(&invalid).is_err());
    }
}
