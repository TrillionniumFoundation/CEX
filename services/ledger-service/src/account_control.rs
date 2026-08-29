use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    AdminAuthorizationFailure, AdminPrincipal,
};
use sqlx::Row;
use uuid::Uuid;

use crate::state::AppState;

const ACCOUNT_MONEY_SCHEMA_V2: &str = "cex.account.money.v2";

#[derive(Debug, Deserialize)]
pub struct OpenAccountV2Request {
    pub account_id: Uuid,
    pub org_id: Uuid,
    pub trace_id: Uuid,
    pub account_type: String,
    pub currency_unit: String,
    pub currency_scale: u8,
    /// Canonical base-10 integer encoded as a JSON string.
    pub opening_minor: String,
    pub idempotency_scope: String,
    pub idempotency_key: String,
}

#[derive(Debug, Deserialize)]
pub struct BuildInventoryRequest {
    pub org_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct SealInventoryRequest {
    pub signer_key_id: String,
    pub signer_public_key_sha256: String,
    pub signature_base64: String,
    pub verification_evidence: Value,
}

#[derive(Debug, Deserialize)]
pub struct CaptureProjectionRequest {
    pub org_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct SetReadPolicyRequest {
    pub org_id: Uuid,
    pub mode: String,
    /// Decimal ratio encoded as a JSON string to avoid binary floating-point policy drift.
    pub min_coverage_ratio: String,
    pub max_capture_age_seconds: i32,
}

#[derive(Debug, Deserialize)]
pub struct RepairProjectionRequest {
    pub run_id: Uuid,
    pub account_id: Uuid,
    pub reason: String,
}

pub async fn open_account_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<OpenAccountV2Request>,
) -> Response {
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    if let Err(response) = enforce_org_boundary(&admin, &request.org_id.to_string()) {
        return response;
    }
    if request.account_id.is_nil() || request.org_id.is_nil() || request.trace_id.is_nil() {
        return bad_request("account_id, org_id and trace_id must be non-nil UUIDs");
    }
    if request.currency_scale > 6 {
        return bad_request("currency_scale must be between 0 and 6");
    }
    let opening_minor = match parse_nonnegative_minor(&request.opening_minor) {
        Ok(value) => value,
        Err(message) => return bad_request(message),
    };
    let pool = match operation_pool(&state) {
        Ok(pool) => pool,
        Err(response) => return response,
    };

    let result = sqlx::query_scalar::<_, String>(
        "select public.cex_open_account_v2(\
            $1,$2,$3,$4,$5,$6,$7,$8,$9,$10\
        )::text",
    )
    .bind(request.account_id)
    .bind(request.org_id)
    .bind(request.trace_id)
    .bind(request.account_type.trim())
    .bind(request.currency_unit.trim())
    .bind(i16::from(request.currency_scale))
    .bind(opening_minor)
    .bind(request.idempotency_scope.trim())
    .bind(request.idempotency_key.trim())
    .bind(admin.actor_id.trim())
    .fetch_one(pool)
    .await;

    match result {
        Ok(body) => decode_json_response(StatusCode::CREATED, body),
        Err(error) => database_error_response("open exact account", error),
    }
}

pub async fn get_account_exact(
    Path(account_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if account_id.is_nil() {
        return bad_request("account_id must be a non-nil UUID");
    }
    let admin = match authorize_ledger_admin(
        &state,
        &headers,
        &["ledger:read", "ledger:manage"],
    ) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let pool = match operation_pool(&state) {
        Ok(pool) => pool,
        Err(response) => return response,
    };

    let row = sqlx::query(
        "select org_id::text as org_id, public.cex_read_account_money_v2(account_id)::text as result_text\
           from public.accounts where account_id=$1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await;

    let row = match row {
        Ok(Some(row)) => row,
        Ok(None) => return not_found("account not found"),
        Err(error) => return database_error_response("read exact account", error),
    };
    let org_id: String = match row.try_get("org_id") {
        Ok(value) => value,
        Err(error) => return database_error_response("decode account organization", error),
    };
    if let Err(response) = enforce_org_boundary(&admin, &org_id) {
        return response;
    }
    let result_text: String = match row.try_get("result_text") {
        Ok(value) => value,
        Err(error) => return database_error_response("decode exact account", error),
    };
    decode_json_response(StatusCode::OK, result_text)
}

pub async fn build_inventory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<BuildInventoryRequest>,
) -> Response {
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    if let Err(response) = enforce_org_boundary(&admin, &request.org_id.to_string()) {
        return response;
    }
    call_json_function(
        &state,
        "build account opening inventory",
        sqlx::query_scalar::<_, String>(
            "select to_jsonb(public.cex_build_account_opening_inventory_v1($1,$2))::text",
        )
        .bind(request.org_id)
        .bind(admin.actor_id.trim()),
        StatusCode::CREATED,
    )
    .await
}

pub async fn seal_inventory(
    Path(run_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SealInventoryRequest>,
) -> Response {
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let pool = match operation_pool(&state) {
        Ok(pool) => pool,
        Err(response) => return response,
    };
    let org_id = match load_org_for_run(
        pool,
        "public.cex_account_opening_inventory_runs_v1",
        run_id,
    )
    .await
    {
        Ok(org_id) => org_id,
        Err(response) => return response,
    };
    if let Err(response) = enforce_org_boundary(&admin, &org_id) {
        return response;
    }

    call_json_query(
        pool,
        "seal account opening inventory",
        sqlx::query_scalar::<_, String>(
            "select to_jsonb(public.cex_seal_account_opening_inventory_v1(\
                $1,$2,$3,$4,$5::jsonb\
            ))::text",
        )
        .bind(run_id)
        .bind(request.signer_key_id.trim())
        .bind(request.signer_public_key_sha256.trim())
        .bind(request.signature_base64.trim())
        .bind(request.verification_evidence.to_string()),
        StatusCode::OK,
    )
    .await
}

pub async fn capture_projection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CaptureProjectionRequest>,
) -> Response {
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    if let Err(response) = enforce_org_boundary(&admin, &request.org_id.to_string()) {
        return response;
    }
    call_json_function(
        &state,
        "capture account projection",
        sqlx::query_scalar::<_, String>(
            "select to_jsonb(public.cex_capture_account_projection_v1($1,$2))::text",
        )
        .bind(request.org_id)
        .bind(admin.actor_id.trim()),
        StatusCode::CREATED,
    )
    .await
}

pub async fn projection_status(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let admin = match authorize_ledger_admin(
        &state,
        &headers,
        &["ledger:read", "ledger:manage"],
    ) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    if let Err(response) = enforce_org_boundary(&admin, &org_id.to_string()) {
        return response;
    }
    let pool = match operation_pool(&state) {
        Ok(pool) => pool,
        Err(response) => return response,
    };
    let result = sqlx::query_scalar::<_, String>(
        "select to_jsonb(status)::text from public.cex_money_v2_org_status_v1 status\
          where org_id=$1",
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await;
    match result {
        Ok(Some(body)) => decode_json_response(StatusCode::OK, body),
        Ok(None) => not_found("organization projection status not found"),
        Err(error) => database_error_response("read projection status", error),
    }
}

pub async fn set_read_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SetReadPolicyRequest>,
) -> Response {
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    if let Err(response) = enforce_org_boundary(&admin, &request.org_id.to_string()) {
        return response;
    }
    if !matches!(request.mode.as_str(), "legacy_v1" | "shadow" | "require_v2") {
        return bad_request("mode must be legacy_v1, shadow or require_v2");
    }
    let ratio = request.min_coverage_ratio.trim();
    if ratio.is_empty() || ratio.len() > 32 || ratio.parse::<f64>().is_err() {
        return bad_request("min_coverage_ratio must be a decimal string");
    }
    let pool = match operation_pool(&state) {
        Ok(pool) => pool,
        Err(response) => return response,
    };
    call_json_query(
        pool,
        "set money read policy",
        sqlx::query_scalar::<_, String>(
            "select to_jsonb(public.cex_set_money_read_policy_v1(\
                $1,$2,$3::text::numeric,$4,$5\
            ))::text",
        )
        .bind(request.org_id)
        .bind(request.mode)
        .bind(ratio)
        .bind(request.max_capture_age_seconds)
        .bind(admin.actor_id.trim()),
        StatusCode::OK,
    )
    .await
}

pub async fn repair_projection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RepairProjectionRequest>,
) -> Response {
    let admin = match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let pool = match operation_pool(&state) {
        Ok(pool) => pool,
        Err(response) => return response,
    };
    let org_id = match sqlx::query_scalar::<_, String>(
        "select org_id::text from public.cex_account_projection_items_v1\
          where run_id=$1 and account_id=$2",
    )
    .bind(request.run_id)
    .bind(request.account_id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return not_found("projection item not found"),
        Err(error) => return database_error_response("load projection repair boundary", error),
    };
    if let Err(response) = enforce_org_boundary(&admin, &org_id) {
        return response;
    }
    call_json_query(
        pool,
        "repair account projection",
        sqlx::query_scalar::<_, String>(
            "select public.cex_repair_account_projection_v1($1,$2,$3,$4)::text",
        )
        .bind(request.run_id)
        .bind(request.account_id)
        .bind(admin.actor_id.trim())
        .bind(request.reason.trim()),
        StatusCode::OK,
    )
    .await
}

pub async fn legacy_create_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_request): Json<Value>,
) -> Response {
    match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(_) => gone_legacy_response(),
        Err(response) => response,
    }
}

pub async fn gone_legacy_value_write(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_request): Json<Value>,
) -> Response {
    match authorize_ledger_admin(&state, &headers, &["ledger:manage"]) {
        Ok(_) => gone_legacy_response(),
        Err(response) => response,
    }
}

fn operation_pool(state: &AppState) -> Result<&sqlx::PgPool, Response> {
    state.operation_pool.as_ref().ok_or_else(|| {
        error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "ledger_operation_persistence_unavailable",
            "exact account persistence is unavailable",
        )
    })
}

async fn call_json_function<'q>(
    state: &AppState,
    context: &str,
    query: sqlx::query::QueryScalar<'q, sqlx::Postgres, String, sqlx::postgres::PgArguments>,
    success: StatusCode,
) -> Response {
    let pool = match operation_pool(state) {
        Ok(pool) => pool,
        Err(response) => return response,
    };
    call_json_query(pool, context, query, success).await
}

async fn call_json_query<'q>(
    pool: &sqlx::PgPool,
    context: &str,
    query: sqlx::query::QueryScalar<'q, sqlx::Postgres, String, sqlx::postgres::PgArguments>,
    success: StatusCode,
) -> Response {
    match query.fetch_one(pool).await {
        Ok(body) => decode_json_response(success, body),
        Err(error) => database_error_response(context, error),
    }
}

async fn load_org_for_run(
    pool: &sqlx::PgPool,
    table: &str,
    run_id: Uuid,
) -> Result<String, Response> {
    // The table name is selected by the caller, never by an HTTP value.
    let statement = format!("select org_id::text from {table} where run_id=$1");
    match sqlx::query_scalar::<_, String>(&statement)
        .bind(run_id)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(org_id)) => Ok(org_id),
        Ok(None) => Err(not_found("inventory run not found")),
        Err(error) => Err(database_error_response("load inventory organization", error)),
    }
}

fn parse_nonnegative_minor(raw: &str) -> Result<i64, &'static str> {
    let value = raw.trim();
    if value.is_empty()
        || value.starts_with('+')
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("opening_minor must be a canonical non-negative integer string");
    }
    value
        .parse::<i64>()
        .map_err(|_| "opening_minor is outside the signed 64-bit range")
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
        Some("configure a scoped ledger principal before using exact account APIs"),
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
        "authenticated principal is not authorized for this organization",
    ))
}

fn database_error_response(context: &str, error: sqlx::Error) -> Response {
    let mut status = StatusCode::SERVICE_UNAVAILABLE;
    let mut code = "ledger_account_control_unavailable";
    if let Some(database_error) = error.as_database_error() {
        let message = database_error.message().to_ascii_lowercase();
        if database_error.code().as_deref() == Some("23505") || message.contains("collision") {
            status = StatusCode::CONFLICT;
            code = "ledger_account_control_collision";
        } else if database_error.code().as_deref() == Some("P0002")
            || message.contains("not found")
        {
            status = StatusCode::NOT_FOUND;
            code = "ledger_account_control_not_found";
        } else if message.contains("blocked") || message.contains("requires") {
            status = StatusCode::CONFLICT;
            code = "ledger_account_control_blocked";
        } else if message.contains("invalid")
            || message.contains("must")
            || message.contains("cannot")
            || message.contains("forbidden")
            || message.contains("unsupported")
        {
            status = StatusCode::BAD_REQUEST;
            code = "ledger_account_control_invalid";
        }
    }
    eprintln!("ledger-service: {context} failed: {error}");
    error_response(status, code, "exact account operation could not be completed")
}

fn decode_json_response(status: StatusCode, body: String) -> Response {
    match serde_json::from_str::<Value>(&body) {
        Ok(value) => (status, Json(value)).into_response(),
        Err(error) => {
            eprintln!("ledger-service: decode account control response failed: {error}");
            error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "ledger_account_control_response_invalid",
                "exact account response could not be verified",
            )
        }
    }
}

fn bad_request(message: &'static str) -> Response {
    error_response(StatusCode::BAD_REQUEST, "invalid_exact_account_request", message)
}

fn not_found(message: &'static str) -> Response {
    error_response(StatusCode::NOT_FOUND, "exact_account_not_found", message)
}

fn gone_legacy_response() -> Response {
    (
        StatusCode::GONE,
        Json(json!({
            "error": "legacy value write retired",
            "code": "ledger_v1_value_write_gone",
            "message": "use /v2/accounts and /v2/ledger/effects with exact minor units",
            "schema_version": ACCOUNT_MONEY_SCHEMA_V2,
        })),
    )
        .into_response()
}

fn error_response(status: StatusCode, code: &'static str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": "exact account operation rejected",
            "code": code,
            "message": message,
            "schema_version": ACCOUNT_MONEY_SCHEMA_V2,
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minor_units_require_canonical_integer_text() {
        assert_eq!(parse_nonnegative_minor("0").unwrap(), 0);
        assert_eq!(parse_nonnegative_minor("42").unwrap(), 42);
        assert!(parse_nonnegative_minor("01").is_err());
        assert!(parse_nonnegative_minor("+1").is_err());
        assert!(parse_nonnegative_minor("1.0").is_err());
        assert!(parse_nonnegative_minor("-1").is_err());
    }
}
