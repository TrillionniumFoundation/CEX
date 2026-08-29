use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    AdminAuthorizationFailure, AdminPrincipal,
};
use shared_types::ledger_v2::{LedgerEffectRequestV1, LEDGER_EFFECT_SCHEMA_V1};
use sqlx::Row;
use uuid::Uuid;

use crate::state::AppState;

const LEDGER_SOURCE_SERVICE: &str = "ledger-service";
const TRACE_RESULT_LIMIT: i64 = 200;

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

    if let Err(error) = request.validate(state.require_explicit_ledger_trace) {
        return error_response(StatusCode::BAD_REQUEST, error.code(), &error.to_string());
    }
    let money = match request.money() {
        Ok(money) => money,
        Err(error) => {
            return error_response(StatusCode::BAD_REQUEST, error.code(), &error.to_string())
        }
    };

    let account_row = match sqlx::query(
        "select org_id::text as org_id, currency_unit, currency_scale
           from public.accounts
          where account_id = $1",
    )
    .bind(request.account_id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "ledger_account_not_found",
                "account not found",
            )
        }
        Err(error) => return database_error_response("load account contract", error),
    };

    let account_org_id: String = match account_row.try_get("org_id") {
        Ok(value) => value,
        Err(error) => return database_error_response("decode account tenancy", error),
    };
    let account_currency_unit: String = match account_row.try_get("currency_unit") {
        Ok(value) => value,
        Err(error) => return database_error_response("decode account currency", error),
    };
    let account_currency_scale: i16 = match account_row.try_get("currency_scale") {
        Ok(value) => value,
        Err(error) => return database_error_response("decode account scale", error),
    };

    if let Err(response) = enforce_org_boundary(&admin, &account_org_id) {
        return response;
    }
    if money.currency != account_currency_unit || i16::from(money.scale) != account_currency_scale {
        return error_response(
            StatusCode::BAD_REQUEST,
            "ledger_currency_mismatch",
            "request currency/scale does not match the account contract",
        );
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
    .bind(request.operation_kind.as_str())
    .bind(money.minor_units)
    .bind(i16::from(money.scale))
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
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:read", "ledger:manage"]) {
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
        Err(error) => return database_error_response("decode ledger effect tenancy", error),
    };
    if let Err(response) = enforce_org_boundary(&admin, &org_id) {
        return response;
    }
    let result_text: String = match row.try_get("result_text") {
        Ok(value) => value,
        Err(error) => return database_error_response("decode ledger effect", error),
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
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:read", "ledger:manage"]) {
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
            Err(error) => return database_error_response("decode ledger trace tenancy", error),
        };
        if let Err(response) = enforce_org_boundary(&admin, &org_id) {
            return response;
        }
        let result_text: String = match row.try_get("result_text") {
            Ok(value) => value,
            Err(error) => return database_error_response("decode ledger trace", error),
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

    (
        StatusCode::OK,
        Json(json!({
            "trace_id": trace_id,
            "limit": TRACE_RESULT_LIMIT,
            "effects": results,
        })),
    )
        .into_response()
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
        } else if database_error.code().as_deref() == Some("P0002") || message.contains("not found")
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
    use shared_types::ledger_v2::LedgerOperationKind;

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
    fn shared_request_contract_rejects_missing_trace_and_nonpositive_amount() {
        let mut request = LedgerEffectRequestV1 {
            account_id: Uuid::new_v4(),
            trace_id: Some(Uuid::new_v4()),
            operation_id: None,
            operation_kind: LedgerOperationKind::Reserve,
            currency_unit: "credit".to_string(),
            currency_scale: 6,
            amount_minor: 1_000_000,
            reference_type: Some("invocation".to_string()),
            reference_id: Some(Uuid::new_v4()),
            idempotency_scope: "org:test:reserve".to_string(),
            idempotency_key: "reserve:test".to_string(),
        };
        assert!(request.validate(true).is_ok());

        request.trace_id = None;
        assert_eq!(
            request.validate(true).unwrap_err().code(),
            "explicit_trace_required"
        );
        request.trace_id = Some(Uuid::new_v4());
        request.amount_minor = 0;
        assert_eq!(
            request.validate(true).unwrap_err().code(),
            "invalid_amount_minor"
        );
    }
}
