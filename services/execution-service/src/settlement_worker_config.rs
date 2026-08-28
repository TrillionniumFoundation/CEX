use crate::ledger_settlement::{
    ExecutionLedgerMode, ExecutionLedgerSettlementAdapter, SettlementAction, SettlementOutcome,
};
use chrono::{DateTime, Utc};
use reqwest::{redirect::Policy, Client};
use shared_config::{load_ledger_scoped_admin_tokens, select_ledger_manage_token};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::{env, error::Error, fmt, time::Duration};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

const DATABASE_URL_ENV: &str = "DATABASE_URL";
const LEDGER_BASE_URL_ENV: &str = "LEDGER_BASE_URL";
const WORKER_ID_ENV: &str = "CEX_EXECUTION_SETTLEMENT_WORKER_ID";
const BATCH_SIZE_ENV: &str = "CEX_EXECUTION_SETTLEMENT_BATCH_SIZE";
const LEASE_SECONDS_ENV: &str = "CEX_EXECUTION_SETTLEMENT_LEASE_SECONDS";
const POLL_SECONDS_ENV: &str = "CEX_EXECUTION_SETTLEMENT_POLL_SECONDS";
const REQUEST_TIMEOUT_SECONDS_ENV: &str = "CEX_EXECUTION_SETTLEMENT_REQUEST_TIMEOUT_SECONDS";
const DATABASE_MAX_CONNECTIONS_ENV: &str = "CEX_EXECUTION_SETTLEMENT_DATABASE_MAX_CONNECTIONS";
const RUN_ONCE_ENV: &str = "CEX_EXECUTION_SETTLEMENT_RUN_ONCE";

const DEFAULT_LEDGER_BASE_URL: &str = "http://127.0.0.1:7002";
const DEFAULT_WORKER_ID: &str = "execution-settlement-worker-local";
const DEFAULT_BATCH_SIZE: i64 = 2;
const DEFAULT_LEASE_SECONDS: i64 = 90;
const DEFAULT_POLL_SECONDS: u64 = 2;
const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 20;
const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 4;
const DATABASE_OPERATION_TIMEOUT_SECONDS: u64 = 5;
const LEASE_SAFETY_MARGIN_SECONDS: u64 = 5;
const MAX_BATCH_SIZE: i64 = 25;
const MAX_LEASE_SECONDS: i64 = 3_600;
const MAX_POLL_SECONDS: u64 = 60;
const MAX_REQUEST_TIMEOUT_SECONDS: u64 = 600;
const MAX_DATABASE_CONNECTIONS: u32 = 32;

#[derive(Debug, Clone)]
pub struct SettlementWorkerConfig {
    pub database_url: String,
    pub ledger_base_url: String,
    pub ledger_manage_token: String,
    pub worker_id: String,
    pub batch_size: i64,
    pub lease_seconds: i64,
    pub poll_seconds: u64,
    pub request_timeout_seconds: u64,
    pub database_max_connections: u32,
    pub run_once: bool,
    pub mode: ExecutionLedgerMode,
}

impl SettlementWorkerConfig {
    pub fn from_env() -> Result<Self, WorkerError> {
        let database_url = required_env(DATABASE_URL_ENV)?;
        let ledger_base_url = env::var(LEDGER_BASE_URL_ENV)
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_LEDGER_BASE_URL.to_string());
        let worker_id = env::var(WORKER_ID_ENV)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_WORKER_ID.to_string());
        validate_worker_id(&worker_id)?;

        let batch_size = bounded_i64_env(BATCH_SIZE_ENV, DEFAULT_BATCH_SIZE, 1, MAX_BATCH_SIZE)?;
        let lease_seconds = bounded_i64_env(
            LEASE_SECONDS_ENV,
            DEFAULT_LEASE_SECONDS,
            5,
            MAX_LEASE_SECONDS,
        )?;
        let poll_seconds = bounded_u64_env(
            POLL_SECONDS_ENV,
            DEFAULT_POLL_SECONDS,
            1,
            MAX_POLL_SECONDS,
        )?;
        let request_timeout_seconds = bounded_u64_env(
            REQUEST_TIMEOUT_SECONDS_ENV,
            DEFAULT_REQUEST_TIMEOUT_SECONDS,
            1,
            MAX_REQUEST_TIMEOUT_SECONDS,
        )?;
        let database_max_connections = bounded_u32_env(
            DATABASE_MAX_CONNECTIONS_ENV,
            DEFAULT_DATABASE_MAX_CONNECTIONS,
            1,
            MAX_DATABASE_CONNECTIONS,
        )?;
        let run_once = bool_env(RUN_ONCE_ENV, false)?;
        let mode = ExecutionLedgerMode::from_env().map_err(WorkerError::Config)?;
        if matches!(mode, ExecutionLedgerMode::LegacyV1) {
            return Err(WorkerError::Config(
                "execution settlement worker requires CEX_EXECUTION_LEDGER_MODE=dual or require_v2"
                    .to_string(),
            ));
        }

        validate_serial_lease_budget(batch_size, request_timeout_seconds, lease_seconds)?;

        let tokens = load_ledger_scoped_admin_tokens();
        let token = select_ledger_manage_token(&tokens)
            .ok_or_else(|| {
                WorkerError::Config(
                    "ledger:manage credential is required for execution settlement worker"
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

#[derive(Debug, Clone)]
struct ClaimedSettlementCommand {
    command_id: Uuid,
    invocation_id: Uuid,
    action: SettlementAction,
    attempt_count: i32,
    max_attempts: i32,
    lease_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct PersistedOutcome {
    outcome: &'static str,
    error_code: Option<String>,
    error_message: Option<String>,
    http_status: Option<u16>,
    receipt: Option<serde_json::Value>,
    replayed: Option<bool>,
    retry_after_seconds: Option<i32>,
}

impl PersistedOutcome {
    fn from_settlement(outcome: SettlementOutcome, attempt_count: i32) -> Self {
        match outcome {
            SettlementOutcome::Applied { replayed, receipt } => Self {
                outcome: "succeeded",
                error_code: None,
                error_message: None,
                http_status: None,
                receipt: Some(receipt),
                replayed: Some(replayed),
                retry_after_seconds: None,
            },
            SettlementOutcome::RetryableExactReplay { code, http_status } => Self {
                outcome: "retry_wait",
                error_code: Some(code),
                error_message: Some(
                    "Ledger rejected or failed before a verified terminal receipt; exact replay is scheduled"
                        .to_string(),
                ),
                http_status,
                receipt: None,
                replayed: None,
                retry_after_seconds: Some(exponential_retry_seconds(attempt_count)),
            },
            SettlementOutcome::ReconcileRequired { code, http_status } => Self {
                outcome: "reconcile_required",
                error_code: Some(code),
                error_message: Some(
                    "Ledger remote outcome is unknown or receipt verification failed; operator reconciliation is required"
                        .to_string(),
                ),
                http_status,
                receipt: None,
                replayed: None,
                retry_after_seconds: None,
            },
            SettlementOutcome::Rejected { code, http_status } => Self {
                outcome: "dead_letter",
                error_code: Some(code),
                error_message: Some(
                    "Ledger permanently rejected the immutable settlement command".to_string(),
                ),
                http_status,
                receipt: None,
                replayed: None,
                retry_after_seconds: None,
            },
            SettlementOutcome::UseLegacyV1 { reason } => Self {
                outcome: "reconcile_required",
                error_code: Some("exact_settlement_contract_missing".to_string()),
                error_message: Some(reason),
                http_status: None,
                receipt: None,
                replayed: None,
                retry_after_seconds: None,
            },
        }
    }
}

#[derive(Debug)]
pub enum WorkerError {
    Config(String),
    Database(String),
    Http(String),
}

impl fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(message) => write!(formatter, "configuration rejected: {message}"),
            Self::Database(message) => write!(formatter, "database operation failed: {message}"),
            Self::Http(message) => write!(formatter, "HTTP client setup failed: {message}"),
        }
    }
}

impl Error for WorkerError {}
