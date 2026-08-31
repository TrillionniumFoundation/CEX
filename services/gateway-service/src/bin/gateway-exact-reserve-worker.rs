use reqwest::{header::RETRY_AFTER, redirect::Policy, Client, StatusCode};
use serde_json::Value;
use shared_config::{
    load_ledger_scoped_admin_tokens, runtime_guard, select_ledger_manage_token,
};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::{env, error::Error, fmt, time::Duration};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

const DATABASE_OPERATION_TIMEOUT_SECONDS: u64 = 5;
const LEASE_SAFETY_MARGIN_SECONDS: u64 = 5;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const DEFAULT_LEDGER_BASE_URL: &str = "http://127.0.0.1:7002";
const DEFAULT_WORKER_ID: &str = "gateway-exact-reserve-worker-local";
const DEFAULT_BATCH_SIZE: i64 = 2;
const DEFAULT_LEASE_SECONDS: i64 = 90;
const DEFAULT_POLL_SECONDS: u64 = 2;
const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 20;
const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 4;
const MAX_BATCH_SIZE: i64 = 25;
const MAX_LEASE_SECONDS: i64 = 3_600;
const MAX_POLL_SECONDS: u64 = 60;
const MAX_REQUEST_TIMEOUT_SECONDS: u64 = 600;
const MAX_DATABASE_CONNECTIONS: u32 = 32;
const WEAK_TOKEN_MARKERS: &[&str] = &[
    "local-dev",
    "change-me",
    "changeme",
    "replace-me",
    "replace_",
    "insecure-default",
];

#[derive(Debug, Clone)]
struct WorkerConfig {
    database_url: String,
    ledger_base_url: String,
    ledger_manage_token: String,
    worker_id: String,
    batch_size: i64,
    lease_seconds: i64,
    poll_seconds: u64,
    request_timeout_seconds: u64,
    database_max_connections: u32,
    run_once: bool,
    mode: GatewayLedgerMode,
}

impl WorkerConfig {
    fn from_env() -> Result<Self, WorkerError> {
        let database_url = required_env("DATABASE_URL")?;
        let ledger_base_url = env::var("LEDGER_BASE_URL")
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_LEDGER_BASE_URL.to_string());
        if !ledger_base_url.starts_with("http://") && !ledger_base_url.starts_with("https://") {
            return Err(WorkerError::Config(
                "LEDGER_BASE_URL must use http:// or https://".to_string(),
            ));
        }

        let worker_id = env::var("CEX_GATEWAY_EXACT_RESERVE_WORKER_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_WORKER_ID.to_string());
        validate_worker_id(&worker_id)?;

        let batch_size = bounded_i64_env(
            "CEX_GATEWAY_EXACT_RESERVE_BATCH_SIZE",
            DEFAULT_BATCH_SIZE,
            1,
            MAX_BATCH_SIZE,
        )?;
        let lease_seconds = bounded_i64_env(
            "CEX_GATEWAY_EXACT_RESERVE_LEASE_SECONDS",
            DEFAULT_LEASE_SECONDS,
            5,
            MAX_LEASE_SECONDS,
        )?;
        let poll_seconds = bounded_u64_env(
            "CEX_GATEWAY_EXACT_RESERVE_POLL_SECONDS",
            DEFAULT_POLL_SECONDS,
            1,
            MAX_POLL_SECONDS,
        )?;
        let request_timeout_seconds = bounded_u64_env(
            "CEX_GATEWAY_EXACT_RESERVE_REQUEST_TIMEOUT_SECONDS",
            DEFAULT_REQUEST_TIMEOUT_SECONDS,
            1,
            MAX_REQUEST_TIMEOUT_SECONDS,
        )?;
        let database_max_connections = bounded_u32_env(
            "CEX_GATEWAY_EXACT_RESERVE_DATABASE_MAX_CONNECTIONS",
            DEFAULT_DATABASE_MAX_CONNECTIONS,
            1,
            MAX_DATABASE_CONNECTIONS,
        )?;
        let run_once = bool_env("CEX_GATEWAY_EXACT_RESERVE_RUN_ONCE", false)?;
        let mode = GatewayLedgerMode::from_env()?;
        if matches!(mode, GatewayLedgerMode::LegacyV1) {
            return Err(WorkerError::Config(
                "Gateway exact reserve worker requires CEX_GATEWAY_LEDGER_MODE=dual or require_v2"
                    .to_string(),
            ));
        }
        validate_serial_lease_budget(batch_size, request_timeout_seconds, lease_seconds)?;

        let tokens = load_ledger_scoped_admin_tokens();
        let token = select_ledger_manage_token(&tokens)
            .ok_or_else(|| {
                WorkerError::Config(
                    "ledger:manage credential is required for Gateway exact reserve worker"
                        .to_string(),
                )
            })?
            .token
            .trim()
            .to_string();
        validate_runtime_posture(&token, mode)?;

        Ok(Self {
            database_url,
            ledger_base_url,
            ledger_manage_token: token,
            worker_id,
            batch_size,
            lease_seconds,
            poll_seconds,
            request_timeout_seconds,
            database_max_connections,
            run_once,
            mode,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GatewayLedgerMode {
    LegacyV1,
    Dual,
    RequireV2,
}

impl GatewayLedgerMode {
    fn from_env() -> Result<Self, WorkerError> {
        let raw = env::var("CEX_GATEWAY_LEDGER_MODE")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                WorkerError::Config("CEX_GATEWAY_LEDGER_MODE must be set explicitly".to_string())
            })?;
        match raw.as_str() {
            "legacy_v1" | "legacy" | "v1" => Ok(Self::LegacyV1),
            "dual" => Ok(Self::Dual),
            "require_v2" | "v2" => Ok(Self::RequireV2),
            other => Err(WorkerError::Config(format!(
                "unsupported CEX_GATEWAY_LEDGER_MODE '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
struct ClaimedCommand {
    command_id: Uuid,
    request_payload: Value,
    attempt_count: i32,
    max_attempts: i32,
}

#[derive(Debug, Clone)]
struct PersistedOutcome {
    outcome: &'static str,
    error_code: Option<String>,
    error_message: Option<String>,
    http_status: Option<u16>,
    receipt: Option<Value>,
    replayed: Option<bool>,
    retry_after_seconds: Option<i32>,
}

impl PersistedOutcome {
    fn succeeded(receipt: Value, replayed: bool, http_status: u16) -> Self {
        Self {
            outcome: "succeeded",
            error_code: None,
            error_message: None,
            http_status: Some(http_status),
            receipt: Some(receipt),
            replayed: Some(replayed),
            retry_after_seconds: None,
        }
    }

    fn retryable(code: impl Into<String>, message: impl Into<String>, status: Option<u16>) -> Self {
        Self {
            outcome: "retry_wait",
            error_code: Some(code.into()),
            error_message: Some(message.into()),
            http_status: status,
            receipt: None,
            replayed: None,
            retry_after_seconds: None,
        }
    }

    fn reconcile(code: impl Into<String>, message: impl Into<String>, status: Option<u16>) -> Self {
        Self {
            outcome: "reconcile_required",
            error_code: Some(code.into()),
            error_message: Some(message.into()),
            http_status: status,
            receipt: None,
            replayed: None,
            retry_after_seconds: None,
        }
    }

    fn rejected(code: impl Into<String>, message: impl Into<String>, status: Option<u16>) -> Self {
        Self {
            outcome: "dead_letter",
            error_code: Some(code.into()),
            error_message: Some(message.into()),
            http_status: status,
            receipt: None,
            replayed: None,
            retry_after_seconds: None,
        }
    }
}

#[derive(Debug)]
enum WorkerError {
    Config(String),
    Database(String),
    Http(String),
}

impl fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(message) => write!(formatter, "configuration rejected: {message}"),
            Self::Database(message) => write!(formatter, "database operation failed: {message}"),
            Self::Http(message) => write!(formatter, "HTTP operation failed: {message}"),
        }
    }
}

impl Error for WorkerError {}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("gateway-exact-reserve-worker terminated: {error}");
        std::process::exit(78);
    }
}

async fn run() -> Result<(), WorkerError> {
    let config = WorkerConfig::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .acquire_timeout(Duration::from_secs(DATABASE_OPERATION_TIMEOUT_SECONDS))
        .connect(&config.database_url)
        .await
        .map_err(|error| WorkerError::Database(format!("connect PostgreSQL: {error}")))?;
    let http = Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(config.request_timeout_seconds))
        .user_agent("cex-gateway-exact-reserve-worker/1")
        .build()
        .map_err(|error| WorkerError::Http(format!("build client: {error}")))?;

    eprintln!(
        "Gateway exact reserve worker started worker_id={} batch={} lease={}s timeout={}s mode={:?}",
        config.worker_id,
        config.batch_size,
        config.lease_seconds,
        config.request_timeout_seconds,
        config.mode
    );

    loop {
        let commands = claim_commands(&pool, &config).await?;
        let claimed_count = commands.len();
        let mut first_error = None;
        for command in commands {
            if let Err(error) = process_command(&pool, &http, &config, command).await {
                eprintln!("Gateway exact reserve command processing failed: {error}");
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }

        if config.run_once {
            if let Some(error) = first_error {
                return Err(error);
            }
            return Ok(());
        }
        if claimed_count == 0 {
            sleep(Duration::from_secs(config.poll_seconds)).await;
        }
    }
}

async fn claim_commands(
    pool: &PgPool,
    config: &WorkerConfig,
) -> Result<Vec<ClaimedCommand>, WorkerError> {
    let claim = sqlx::query(
        "select command_id, request_payload, attempt_count, max_attempts \
         from public.cex_claim_gateway_exact_reserves_v1($1, $2, $3)",
    )
    .bind(&config.worker_id)
    .bind(i32::try_from(config.batch_size).map_err(|_| {
        WorkerError::Config("Gateway exact reserve batch size cannot fit i32".to_string())
    })?)
    .bind(i32::try_from(config.lease_seconds).map_err(|_| {
        WorkerError::Config("Gateway exact reserve lease seconds cannot fit i32".to_string())
    })?)
    .fetch_all(pool);
    let rows = timeout(
        Duration::from_secs(DATABASE_OPERATION_TIMEOUT_SECONDS),
        claim,
    )
    .await
    .map_err(|_| WorkerError::Database("claim Gateway reserve commands timed out".to_string()))?
    .map_err(|error| WorkerError::Database(format!("claim Gateway reserve commands: {error}")))?;

    rows.into_iter()
        .map(|row| {
            Ok(ClaimedCommand {
                command_id: row.try_get("command_id").map_err(|error| {
                    WorkerError::Database(format!("decode command_id: {error}"))
                })?,
                request_payload: row.try_get("request_payload").map_err(|error| {
                    WorkerError::Database(format!("decode request_payload: {error}"))
                })?,
                attempt_count: row.try_get("attempt_count").map_err(|error| {
                    WorkerError::Database(format!("decode attempt_count: {error}"))
                })?,
                max_attempts: row.try_get("max_attempts").map_err(|error| {
                    WorkerError::Database(format!("decode max_attempts: {error}"))
                })?,
            })
        })
        .collect()
}

async fn process_command(
    pool: &PgPool,
    http: &Client,
    config: &WorkerConfig,
    command: ClaimedCommand,
) -> Result<(), WorkerError> {
    let url = format!("{}/v2/ledger/effects", config.ledger_base_url);
    let request = http
        .post(url)
        .bearer_auth(&config.ledger_manage_token)
        .header("x-cex-service-id", "gateway-service")
        .json(&command.request_payload)
        .send()
        .await;

    let mut outcome = match request {
        Ok(response) => classify_response(response).await,
        Err(error) if error.is_connect() => PersistedOutcome::retryable(
            "ledger_connect_failed",
            format!("Ledger connection failed before a response: {error}"),
            None,
        ),
        Err(error) => PersistedOutcome::reconcile(
            "ledger_transport_unknown_outcome",
            format!(
                "Ledger transport failed after request dispatch; durable contract inspection is required: {error}"
            ),
            None,
        ),
    };
    outcome = enforce_attempt_boundary(outcome, command.attempt_count, command.max_attempts);
    persist_outcome(pool, config, command.command_id, outcome).await
}

async fn classify_response(response: reqwest::Response) -> PersistedOutcome {
    let status = response.status();
    let retry_after_seconds = parse_retry_after(response.headers().get(RETRY_AFTER));
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            return PersistedOutcome::reconcile(
                "ledger_response_read_unknown_outcome",
                format!("cannot read Ledger response body: {error}"),
                Some(status.as_u16()),
            )
        }
    };
    if bytes.len() > MAX_RESPONSE_BYTES {
        return PersistedOutcome::reconcile(
            "ledger_response_too_large_unknown_outcome",
            format!("Ledger response exceeded bounded body size of {MAX_RESPONSE_BYTES} bytes"),
            Some(status.as_u16()),
        );
    }

    let body: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(error) if status.is_success() => {
            return PersistedOutcome::reconcile(
                "ledger_success_receipt_decode_failed",
                format!("Ledger success body is not a verifiable JSON receipt: {error}"),
                Some(status.as_u16()),
            )
        }
        Err(_) => Value::Null,
    };

    if status.is_success() {
        let replayed = body
            .get("replayed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return PersistedOutcome::succeeded(body, replayed, status.as_u16());
    }

    let code =
        extract_error_code(&body).unwrap_or_else(|| format!("ledger_http_{}", status.as_u16()));
    let message = extract_error_message(&body).unwrap_or_else(|| {
        format!(
            "Ledger rejected exact reserve with HTTP {}",
            status.as_u16()
        )
    });

    if is_retryable_status(status) {
        let mut outcome = PersistedOutcome::retryable(code, message, Some(status.as_u16()));
        outcome.retry_after_seconds = retry_after_seconds;
        outcome
    } else if status.is_redirection() {
        PersistedOutcome::reconcile(
            "ledger_redirect_rejected_unknown_outcome",
            "Ledger returned a redirect while redirects are disabled",
            Some(status.as_u16()),
        )
    } else {
        PersistedOutcome::rejected(code, message, Some(status.as_u16()))
    }
}

fn is_retryable_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::REQUEST_TIMEOUT
            | StatusCode::TOO_EARLY
            | StatusCode::TOO_MANY_REQUESTS
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    ) || status.is_server_error()
}

fn extract_error_code(body: &Value) -> Option<String> {
    body.pointer("/error/code")
        .or_else(|| body.get("code"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(128).collect())
}

fn extract_error_message(body: &Value) -> Option<String> {
    body.pointer("/error/message")
        .or_else(|| body.get("message"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(2_000).collect())
}

fn parse_retry_after(value: Option<&reqwest::header::HeaderValue>) -> Option<i32> {
    value
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<i32>().ok())
        .filter(|value| (1..=3_600).contains(value))
}

fn enforce_attempt_boundary(
    mut outcome: PersistedOutcome,
    attempt_count: i32,
    max_attempts: i32,
) -> PersistedOutcome {
    if outcome.outcome == "retry_wait" && attempt_count >= max_attempts {
        outcome.outcome = "reconcile_required";
        outcome.error_code = Some("retry_budget_exhausted_unknown_outcome".to_string());
        outcome.error_message = Some(
            "automatic exact reserve attempts are exhausted; inspect the 0066 contract before replay"
                .to_string(),
        );
        outcome.retry_after_seconds = None;
    }
    outcome
}

async fn persist_outcome(
    pool: &PgPool,
    config: &WorkerConfig,
    command_id: Uuid,
    outcome: PersistedOutcome,
) -> Result<(), WorkerError> {
    let receipt_json = outcome
        .receipt
        .map(|receipt| serde_json::to_string(&receipt))
        .transpose()
        .map_err(|error| {
            WorkerError::Database(format!(
                "serialize Gateway reserve command {command_id} receipt: {error}"
            ))
        })?;
    let persist = sqlx::query(
        "select command_id from public.cex_finish_gateway_exact_reserve_v1(\
         $1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9)",
    )
    .bind(command_id)
    .bind(&config.worker_id)
    .bind(outcome.outcome)
    .bind(outcome.error_code)
    .bind(outcome.error_message)
    .bind(outcome.http_status.map(i32::from))
    .bind(receipt_json)
    .bind(outcome.replayed)
    .bind(outcome.retry_after_seconds)
    .fetch_one(pool);
    timeout(Duration::from_secs(DATABASE_OPERATION_TIMEOUT_SECONDS), persist)
        .await
        .map_err(|_| {
            WorkerError::Database(format!(
                "persist Gateway reserve command {command_id} outcome timed out; claim is intentionally left for lease recovery"
            ))
        })?
        .map_err(|error| {
            WorkerError::Database(format!(
                "persist Gateway reserve command {command_id} outcome; claim is intentionally left for lease recovery: {error}"
            ))
        })?;
    Ok(())
}

fn validate_serial_lease_budget(
    batch_size: i64,
    request_timeout_seconds: u64,
    lease_seconds: i64,
) -> Result<(), WorkerError> {
    let batch = u64::try_from(batch_size)
        .map_err(|_| WorkerError::Config("batch size must be positive".to_string()))?;
    let lease = u64::try_from(lease_seconds)
        .map_err(|_| WorkerError::Config("lease seconds must be positive".to_string()))?;
    let required = DATABASE_OPERATION_TIMEOUT_SECONDS
        .saturating_add(
            batch.saturating_mul(
                request_timeout_seconds
                    .saturating_add(DATABASE_OPERATION_TIMEOUT_SECONDS.saturating_mul(2)),
            ),
        )
        .saturating_add(LEASE_SAFETY_MARGIN_SECONDS);
    if required >= lease {
        return Err(WorkerError::Config(format!(
            "serial Gateway reserve lease budget is unsafe: batch_size={batch_size}, request_timeout={request_timeout_seconds}s requires lease greater than {required}s, configured {lease_seconds}s"
        )));
    }
    Ok(())
}

fn validate_worker_id(worker_id: &str) -> Result<(), WorkerError> {
    if worker_id.is_empty()
        || worker_id.len() > 128
        || !worker_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':')
        })
    {
        return Err(WorkerError::Config(
            "Gateway reserve worker id must use 1..128 characters from [A-Za-z0-9._:-]".to_string(),
        ));
    }
    Ok(())
}

fn validate_runtime_posture(
    ledger_manage_token: &str,
    mode: GatewayLedgerMode,
) -> Result<(), WorkerError> {
    let profile = runtime_guard::resolve_runtime_profile()
        .map_err(|error| WorkerError::Config(error.to_string()))?;
    if profile.is_production_like() {
        if !matches!(mode, GatewayLedgerMode::Dual | GatewayLedgerMode::RequireV2) {
            return Err(WorkerError::Config(
                "production-like Gateway reserve worker requires dual or require_v2 mode"
                    .to_string(),
            ));
        }
        if ledger_manage_token.len() < 32 {
            return Err(WorkerError::Config(
                "production-like Ledger credential must be at least 32 bytes".to_string(),
            ));
        }
        let lowered = ledger_manage_token.to_ascii_lowercase();
        if let Some(marker) = WEAK_TOKEN_MARKERS
            .iter()
            .copied()
            .find(|marker| lowered.contains(marker))
        {
            return Err(WorkerError::Config(format!(
                "production-like Ledger credential contains forbidden marker '{marker}'"
            )));
        }
    }
    Ok(())
}

fn required_env(name: &str) -> Result<String, WorkerError> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| WorkerError::Config(format!("{name} is required")))
}

fn bounded_i64_env(
    name: &str,
    default_value: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, WorkerError> {
    let value = match env::var(name) {
        Ok(raw) => raw
            .trim()
            .parse::<i64>()
            .map_err(|_| WorkerError::Config(format!("{name} must be an integer")))?,
        Err(_) => default_value,
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(WorkerError::Config(format!(
            "{name} must be between {minimum} and {maximum}"
        )));
    }
    Ok(value)
}

fn bounded_u64_env(
    name: &str,
    default_value: u64,
    minimum: u64,
    maximum: u64,
) -> Result<u64, WorkerError> {
    let value = match env::var(name) {
        Ok(raw) => raw
            .trim()
            .parse::<u64>()
            .map_err(|_| WorkerError::Config(format!("{name} must be an integer")))?,
        Err(_) => default_value,
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(WorkerError::Config(format!(
            "{name} must be between {minimum} and {maximum}"
        )));
    }
    Ok(value)
}

fn bounded_u32_env(
    name: &str,
    default_value: u32,
    minimum: u32,
    maximum: u32,
) -> Result<u32, WorkerError> {
    let value = match env::var(name) {
        Ok(raw) => raw
            .trim()
            .parse::<u32>()
            .map_err(|_| WorkerError::Config(format!("{name} must be an integer")))?,
        Err(_) => default_value,
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(WorkerError::Config(format!(
            "{name} must be between {minimum} and {maximum}"
        )));
    }
    Ok(value)
}

fn bool_env(name: &str, default_value: bool) -> Result<bool, WorkerError> {
    match env::var(name) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(WorkerError::Config(format!("{name} must be a boolean"))),
        },
        Err(_) => Ok(default_value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_statuses_are_explicit() {
        assert!(is_retryable_status(StatusCode::REQUEST_TIMEOUT));
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::CONFLICT));
    }

    #[test]
    fn final_retryable_attempt_becomes_reconciliation() {
        let retry = PersistedOutcome::retryable("temporary", "temporary", Some(503));
        let final_outcome = enforce_attempt_boundary(retry, 3, 3);
        assert_eq!(final_outcome.outcome, "reconcile_required");
        assert_eq!(
            final_outcome.error_code.as_deref(),
            Some("retry_budget_exhausted_unknown_outcome")
        );
        assert!(final_outcome.retry_after_seconds.is_none());
    }

    #[test]
    fn serial_batch_requires_safe_lease_budget() {
        assert!(validate_serial_lease_budget(2, 20, 90).is_ok());
        assert!(validate_serial_lease_budget(2, 20, 60).is_err());
        assert!(validate_serial_lease_budget(3, 20, 90).is_err());
    }

    #[test]
    fn worker_id_rejects_spaces_and_oversize() {
        assert!(validate_worker_id("gateway-reserve-1:local").is_ok());
        assert!(validate_worker_id("worker with spaces").is_err());
        assert!(validate_worker_id(&"x".repeat(129)).is_err());
    }

    #[test]
    fn retry_after_accepts_only_bounded_delta_seconds() {
        let valid = reqwest::header::HeaderValue::from_static("30");
        let invalid = reqwest::header::HeaderValue::from_static("99999");
        assert_eq!(parse_retry_after(Some(&valid)), Some(30));
        assert_eq!(parse_retry_after(Some(&invalid)), None);
    }
}
