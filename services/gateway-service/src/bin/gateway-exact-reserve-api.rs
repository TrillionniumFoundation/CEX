use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::{env, net::SocketAddr, sync::Arc, time::Duration};
use tokio::net::TcpListener;
use uuid::Uuid;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:7010";
const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 8;
const MAX_DATABASE_CONNECTIONS: u32 = 32;
const MAX_REQUEST_BYTES: usize = 16 * 1024;
const MIN_PRODUCTION_TOKEN_BYTES: usize = 32;
const WEAK_TOKEN_MARKERS: &[&str] = &[
    "local-dev",
    "change-me",
    "changeme",
    "replace-me",
    "replace_",
    "insecure-default",
];

#[derive(Clone)]
struct ApiState {
    pool: PgPool,
    ingress_token: Arc<str>,
    allow_active: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactReserveRequest {
    account_id: Uuid,
    org_id: Uuid,
    trace_id: Uuid,
    currency_unit: String,
    currency_scale: i16,
    amount_minor: String,
    #[serde(default = "default_execution_mode")]
    execution_mode: String,
    #[serde(default = "default_max_attempts")]
    max_attempts: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthenticationError {
    Unauthorized,
    InvalidServiceId,
}

impl AuthenticationError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthorized => error_response(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "a valid exact-ingress bearer token is required",
            ),
            Self::InvalidServiceId => error_response(
                StatusCode::BAD_REQUEST,
                "invalid_service_id",
                "x-cex-service-id must use 1..128 characters from [A-Za-z0-9._:-]",
            ),
        }
    }
}

fn default_execution_mode() -> String {
    "shadow".to_string()
}

fn default_max_attempts() -> i32 {
    5
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("gateway-exact-reserve-api terminated: {error}");
        std::process::exit(78);
    }
}

async fn run() -> Result<(), String> {
    let profile = resolve_profile()?;
    let database_url = required_env("DATABASE_URL")?;
    let ingress_token = required_env("CEX_GATEWAY_EXACT_INGRESS_TOKEN")?;
    validate_ingress_token(&ingress_token, profile.is_production_like())?;
    let allow_active = bool_env("CEX_GATEWAY_EXACT_RESERVE_ALLOW_ACTIVE", false)?;
    let database_max_connections = bounded_u32_env(
        "CEX_GATEWAY_EXACT_INGRESS_DATABASE_MAX_CONNECTIONS",
        DEFAULT_DATABASE_MAX_CONNECTIONS,
        1,
        MAX_DATABASE_CONNECTIONS,
    )?;
    let bind_addr: SocketAddr = env::var("CEX_GATEWAY_EXACT_INGRESS_BIND_ADDR")
        .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string())
        .parse()
        .map_err(|error| format!("invalid CEX_GATEWAY_EXACT_INGRESS_BIND_ADDR: {error}"))?;

    let pool = PgPoolOptions::new()
        .max_connections(database_max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await
        .map_err(|error| format!("connect PostgreSQL: {error}"))?;

    let state = ApiState {
        pool,
        ingress_token: Arc::from(ingress_token),
        allow_active,
    };
    let router = Router::new()
        .route("/health", get(health))
        .route(
            "/v2/invocations/:invocation_id/exact-reserve",
            post(prepare_exact_reserve),
        )
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .with_state(state);

    let listener = TcpListener::bind(bind_addr)
        .await
        .map_err(|error| format!("bind {bind_addr}: {error}"))?;
    eprintln!(
        "gateway exact reserve API listening on {bind_addr}; active_commands_allowed={allow_active}"
    );
    axum::serve(listener, router)
        .await
        .map_err(|error| format!("serve Gateway exact reserve API: {error}"))
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

async fn prepare_exact_reserve(
    State(state): State<ApiState>,
    Path(invocation_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<ExactReserveRequest>,
) -> impl IntoResponse {
    let principal = match authenticate(&headers, &state.ingress_token) {
        Ok(principal) => principal,
        Err(error) => return error.into_response(),
    };
    if invocation_id.is_nil()
        || request.account_id.is_nil()
        || request.org_id.is_nil()
        || request.trace_id.is_nil()
    {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_identifier",
            "invocation_id, account_id, org_id and trace_id must be non-nil UUIDs",
        );
    }

    let amount_minor = match parse_positive_minor_units(&request.amount_minor) {
        Ok(value) => value,
        Err(message) => {
            return error_response(StatusCode::BAD_REQUEST, "invalid_amount_minor", message)
        }
    };
    if let Err(message) = validate_currency(&request.currency_unit, request.currency_scale) {
        return error_response(StatusCode::BAD_REQUEST, "invalid_currency", message);
    }
    if request.execution_mode != "shadow" && request.execution_mode != "active" {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_execution_mode",
            "execution_mode must be shadow or active",
        );
    }
    if request.execution_mode == "active" && !state.allow_active {
        return error_response(
            StatusCode::CONFLICT,
            "active_reserve_disabled",
            "active exact reserve commands require an explicit rollout switch",
        );
    }
    if !(1..=100).contains(&request.max_attempts) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_max_attempts",
            "max_attempts must be between 1 and 100",
        );
    }

    let query = sqlx::query(
        "select public.cex_prepare_gateway_exact_reserve_v1(\
         $1, $2, $3, $4, $5, $6, $7, $8, $9, $10) as result",
    )
    .bind(invocation_id)
    .bind(request.account_id)
    .bind(request.org_id)
    .bind(request.trace_id)
    .bind(&request.currency_unit)
    .bind(request.currency_scale)
    .bind(amount_minor)
    .bind(&principal)
    .bind(&request.execution_mode)
    .bind(request.max_attempts)
    .fetch_one(&state.pool)
    .await;

    match query {
        Ok(row) => match row.try_get::<Value, _>("result") {
            Ok(result) => {
                let replayed = result
                    .get("replayed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let status = if replayed {
                    StatusCode::OK
                } else {
                    StatusCode::CREATED
                };
                (status, Json(result)).into_response()
            }
            Err(error) => error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid_database_result",
                &format!("decode exact reserve result: {error}"),
            ),
        },
        Err(error) => map_database_error(error),
    }
}

fn authenticate(
    headers: &HeaderMap,
    expected_token: &str,
) -> Result<String, AuthenticationError> {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let supplied = authorization.strip_prefix("Bearer ").unwrap_or_default();
    if !constant_time_eq(supplied.as_bytes(), expected_token.as_bytes()) {
        return Err(AuthenticationError::Unauthorized);
    }

    let principal = headers
        .get("x-cex-service-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("gateway-exact-ingress")
        .to_string();
    if principal.len() > 128
        || !principal.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':')
        })
    {
        return Err(AuthenticationError::InvalidServiceId);
    }
    Ok(principal)
}

fn map_database_error(error: sqlx::Error) -> axum::response::Response {
    let code = error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|value| value.into_owned());
    let message = error
        .as_database_error()
        .map(|database_error| database_error.message().to_string())
        .unwrap_or_else(|| error.to_string());

    match code.as_deref() {
        Some("P0002") => error_response(StatusCode::NOT_FOUND, "not_found", &message),
        Some("23505") => error_response(StatusCode::CONFLICT, "immutable_collision", &message),
        Some("23503") => error_response(StatusCode::CONFLICT, "missing_binding", &message),
        Some("23514") | Some("22023") => {
            error_response(StatusCode::BAD_REQUEST, "contract_rejected", &message)
        }
        _ => error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "database_unavailable",
            "exact reserve registration did not commit",
        ),
    }
}

fn error_response(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({"error": {"code": code, "message": message}})),
    )
        .into_response()
}

fn parse_positive_minor_units(raw: &str) -> Result<i64, &'static str> {
    if raw.is_empty()
        || !raw.bytes().all(|byte| byte.is_ascii_digit())
        || (raw.len() > 1 && raw.starts_with('0'))
    {
        return Err("amount_minor must be a canonical positive base-10 integer string");
    }
    let value = raw
        .parse::<i64>()
        .map_err(|_| "amount_minor exceeds signed 64-bit range")?;
    if value <= 0 {
        return Err("amount_minor must be positive");
    }
    Ok(value)
}

fn validate_currency(unit: &str, scale: i16) -> Result<(), &'static str> {
    if !(0..=6).contains(&scale) {
        return Err("currency_scale must be between 0 and 6");
    }
    if unit.is_empty()
        || unit.len() > 32
        || unit != unit.to_ascii_lowercase()
        || !unit.bytes().enumerate().all(|(index, byte)| {
            if index == 0 {
                byte.is_ascii_lowercase()
            } else {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            }
        })
    {
        return Err("currency_unit must match ^[a-z][a-z0-9._-]{0,31}$");
    }
    Ok(())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeProfile {
    Test,
    Local,
    Dev,
    Beta,
    Staging,
    Production,
}

impl RuntimeProfile {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "test" => Ok(Self::Test),
            "local" => Ok(Self::Local),
            "dev" | "development" => Ok(Self::Dev),
            "beta" => Ok(Self::Beta),
            "staging" | "stage" => Ok(Self::Staging),
            "production" | "prod" => Ok(Self::Production),
            other => Err(format!("unsupported runtime profile '{other}'")),
        }
    }

    fn is_production_like(self) -> bool {
        matches!(self, Self::Beta | Self::Staging | Self::Production)
    }
}

fn resolve_profile() -> Result<RuntimeProfile, String> {
    let primary = env::var("CEX_RUNTIME_PROFILE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| RuntimeProfile::parse(&value))
        .transpose()?;
    let compatibility = env::var("APP_ENV")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| RuntimeProfile::parse(&value))
        .transpose()?;
    match (primary, compatibility) {
        (Some(left), Some(right)) if left != right => {
            Err("CEX_RUNTIME_PROFILE and APP_ENV resolve to different profiles".to_string())
        }
        (Some(profile), _) | (_, Some(profile)) => Ok(profile),
        (None, None) => Err("CEX_RUNTIME_PROFILE or APP_ENV must be set explicitly".to_string()),
    }
}

fn validate_ingress_token(token: &str, production_like: bool) -> Result<(), String> {
    if token.contains('\n') || token.contains('\r') {
        return Err("exact ingress token cannot contain line breaks".to_string());
    }
    if production_like {
        if token.len() < MIN_PRODUCTION_TOKEN_BYTES {
            return Err(format!(
                "production-like exact ingress token must be at least {MIN_PRODUCTION_TOKEN_BYTES} bytes"
            ));
        }
        let lowered = token.to_ascii_lowercase();
        if let Some(marker) = WEAK_TOKEN_MARKERS
            .iter()
            .copied()
            .find(|marker| lowered.contains(marker))
        {
            return Err(format!(
                "production-like exact ingress token contains forbidden marker '{marker}'"
            ));
        }
    }
    Ok(())
}

fn required_env(name: &str) -> Result<String, String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn bounded_u32_env(
    name: &str,
    default_value: u32,
    minimum: u32,
    maximum: u32,
) -> Result<u32, String> {
    let value = match env::var(name) {
        Ok(raw) => raw
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("{name} must be an integer"))?,
        Err(_) => default_value,
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("{name} must be between {minimum} and {maximum}"));
    }
    Ok(value)
}

fn bool_env(name: &str, default_value: bool) -> Result<bool, String> {
    match env::var(name) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(format!("{name} must be a boolean")),
        },
        Err(_) => Ok(default_value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_minor_units_reject_float_sign_and_noncanonical_zero_prefix() {
        assert_eq!(parse_positive_minor_units("1").unwrap(), 1);
        assert_eq!(
            parse_positive_minor_units("9007199254740991").unwrap(),
            9_007_199_254_740_991
        );
        for invalid in ["", "0", "01", "+1", "-1", "1.0", " 1", "1 "] {
            assert!(parse_positive_minor_units(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn currency_contract_is_canonical() {
        assert!(validate_currency("credit", 6).is_ok());
        assert!(validate_currency("usd.test", 2).is_ok());
        assert!(validate_currency("USD", 2).is_err());
        assert!(validate_currency("1usd", 2).is_err());
        assert!(validate_currency("usd", 7).is_err());
    }

    #[test]
    fn token_comparison_handles_different_lengths() {
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"same-longer"));
        assert!(!constant_time_eq(b"same", b"diff"));
    }
}
