use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared_config::{
    build_admin_principal_map, load_execution_scoped_admin_tokens, load_ledger_scoped_admin_tokens,
    select_ledger_manage_token, AdminPrincipal,
};
use shared_types::{ExecutionDispatchMode, ExecutionStatus};

use crate::providers::ProviderDispatchInput;
use sqlx::PgPool;
use std::{
    collections::HashMap,
    env, fs,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::sync::RwLock;
use uuid::Uuid;

pub const DEFAULT_EXECUTION_DEFAULT_MAX_ATTEMPTS: i32 = 1;
pub const DEFAULT_EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS: i32 = 3;
pub const DEFAULT_EXECUTION_PROVIDER_DISPATCH_TIMEOUT_SECONDS: u64 = 60;
pub const DEFAULT_EXECUTION_RETRY_BACKOFF_SECONDS: i64 = 5;
pub const DEFAULT_EXECUTION_RETRY_BACKOFF_MAX_SECONDS: i64 = 60;
pub const DEFAULT_ALERT_APPROVAL_BACKLOG_THRESHOLD: usize = 10;
pub const DEFAULT_ALERT_LEASE_EXPIRED_THRESHOLD: usize = 3;
pub const DEFAULT_ALERT_RETRY_BUDGET_EXHAUSTED_THRESHOLD: usize = 3;
pub const DEFAULT_ALERT_PROVIDER_FAILURE_THRESHOLD: usize = 1;
pub const DEFAULT_ALERT_PROVIDER_BILLING_FAILURE_THRESHOLD: usize = 1;
pub const DEFAULT_ALERT_PROVIDER_TIMEOUT_FAILURE_THRESHOLD: usize = 3;
pub const DEFAULT_ALERT_PROVIDER_DEAD_LETTER_THRESHOLD: usize = 1;
pub const DEFAULT_ALERT_PROVIDER_RETRY_BUDGET_EXHAUSTED_THRESHOLD: usize = 1;
pub const DEFAULT_ALERT_AUDIT_FAILURE_THRESHOLD: usize = 1;
pub const DEFAULT_ALERT_REFUND_FAILURE_THRESHOLD: usize = 1;
const DEFAULT_APPROVAL_SENSITIVE_KEYWORDS: &[&str] =
    &["delete", "wire", "transfer", "publish", "deploy", "email"];

#[derive(Debug, Clone, Default, Deserialize)]
struct ExecutionPolicyBundle {
    approval_reserve_threshold: Option<f64>,
    hard_reject_reserve_threshold: Option<f64>,
    approval_sensitive_keywords: Option<Vec<String>>,
    block_keywords: Option<Vec<String>>,
    approval_capability_prefixes: Option<Vec<String>>,
    block_capability_prefixes: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
struct LoadedExecutionPolicyBundle {
    path: Option<String>,
    load_status: String,
    load_error: Option<String>,
    bundle: ExecutionPolicyBundle,
}

/// Compatibility storage for the retired in-process provider route.
///
/// The default external-Agent-only build deliberately discards every value so
/// normal execution admission cannot retain raw Prompt material for an
/// unreachable local-provider path. Historical local compatibility builds may
/// opt into storage through the explicitly non-default feature.
#[derive(Clone, Default)]
pub struct ProviderInputStore {
    #[cfg(feature = "legacy-local-provider-dispatch")]
    inner: Arc<RwLock<HashMap<Uuid, ProviderDispatchInput>>>,
}

impl ProviderInputStore {
    pub async fn read(&self) -> ProviderInputReadGuard<'_> {
        #[cfg(feature = "legacy-local-provider-dispatch")]
        {
            ProviderInputReadGuard {
                inner: self.inner.read().await,
            }
        }
        #[cfg(not(feature = "legacy-local-provider-dispatch"))]
        {
            ProviderInputReadGuard {
                _marker: std::marker::PhantomData,
            }
        }
    }

    pub async fn write(&self) -> ProviderInputWriteGuard<'_> {
        #[cfg(feature = "legacy-local-provider-dispatch")]
        {
            ProviderInputWriteGuard {
                inner: self.inner.write().await,
            }
        }
        #[cfg(not(feature = "legacy-local-provider-dispatch"))]
        {
            ProviderInputWriteGuard {
                _marker: std::marker::PhantomData,
            }
        }
    }
}

pub struct ProviderInputReadGuard<'a> {
    #[cfg(feature = "legacy-local-provider-dispatch")]
    inner: tokio::sync::RwLockReadGuard<'a, HashMap<Uuid, ProviderDispatchInput>>,
    #[cfg(not(feature = "legacy-local-provider-dispatch"))]
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> ProviderInputReadGuard<'a> {
    pub fn get(&self, execution_id: &Uuid) -> Option<&ProviderDispatchInput> {
        #[cfg(feature = "legacy-local-provider-dispatch")]
        {
            self.inner.get(execution_id)
        }
        #[cfg(not(feature = "legacy-local-provider-dispatch"))]
        {
            let _ = execution_id;
            None
        }
    }
}

pub struct ProviderInputWriteGuard<'a> {
    #[cfg(feature = "legacy-local-provider-dispatch")]
    inner: tokio::sync::RwLockWriteGuard<'a, HashMap<Uuid, ProviderDispatchInput>>,
    #[cfg(not(feature = "legacy-local-provider-dispatch"))]
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> ProviderInputWriteGuard<'a> {
    pub fn insert(
        &mut self,
        execution_id: Uuid,
        input: ProviderDispatchInput,
    ) -> Option<ProviderDispatchInput> {
        #[cfg(feature = "legacy-local-provider-dispatch")]
        {
            self.inner.insert(execution_id, input)
        }
        #[cfg(not(feature = "legacy-local-provider-dispatch"))]
        {
            let _ = (execution_id, input);
            None
        }
    }

    pub fn remove(&mut self, execution_id: &Uuid) -> Option<ProviderDispatchInput> {
        #[cfg(feature = "legacy-local-provider-dispatch")]
        {
            self.inner.remove(execution_id)
        }
        #[cfg(not(feature = "legacy-local-provider-dispatch"))]
        {
            let _ = execution_id;
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub execution_id: Uuid,
    pub invocation_id: Uuid,
    pub trace_id: Uuid,
    pub org_id: Option<String>,
    pub status: ExecutionStatus,
    pub provider_target: Option<String>,
    pub dispatch_mode: ExecutionDispatchMode,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub worker_id: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub result_payload: Option<Value>,
    pub approval_required: bool,
    pub policy_reason: Option<String>,
    pub approved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct ExecutionRuntimeMetrics {
    pub create_requests: AtomicU64,
    pub create_blocked_policy: AtomicU64,
    pub create_awaiting_approval: AtomicU64,
    pub create_auto_approved: AtomicU64,
    pub approve_requests: AtomicU64,
    pub approve_successes: AtomicU64,
    pub reject_requests: AtomicU64,
    pub reject_successes: AtomicU64,
    pub retry_requests: AtomicU64,
    pub cancel_requests: AtomicU64,
    pub claim_requests: AtomicU64,
    pub claim_successes: AtomicU64,
    pub audit_failures: AtomicU64,
    pub refund_failures: AtomicU64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionRuntimeMetricsSnapshot {
    pub create_requests: u64,
    pub create_blocked_policy: u64,
    pub create_awaiting_approval: u64,
    pub create_auto_approved: u64,
    pub approve_requests: u64,
    pub approve_successes: u64,
    pub reject_requests: u64,
    pub reject_successes: u64,
    pub retry_requests: u64,
    pub cancel_requests: u64,
    pub claim_requests: u64,
    pub claim_successes: u64,
    pub audit_failures: u64,
    pub refund_failures: u64,
}

impl ExecutionRuntimeMetrics {
    pub fn snapshot(&self) -> ExecutionRuntimeMetricsSnapshot {
        ExecutionRuntimeMetricsSnapshot {
            create_requests: self.create_requests.load(Ordering::Relaxed),
            create_blocked_policy: self.create_blocked_policy.load(Ordering::Relaxed),
            create_awaiting_approval: self.create_awaiting_approval.load(Ordering::Relaxed),
            create_auto_approved: self.create_auto_approved.load(Ordering::Relaxed),
            approve_requests: self.approve_requests.load(Ordering::Relaxed),
            approve_successes: self.approve_successes.load(Ordering::Relaxed),
            reject_requests: self.reject_requests.load(Ordering::Relaxed),
            reject_successes: self.reject_successes.load(Ordering::Relaxed),
            retry_requests: self.retry_requests.load(Ordering::Relaxed),
            cancel_requests: self.cancel_requests.load(Ordering::Relaxed),
            claim_requests: self.claim_requests.load(Ordering::Relaxed),
            claim_successes: self.claim_successes.load(Ordering::Relaxed),
            audit_failures: self.audit_failures.load(Ordering::Relaxed),
            refund_failures: self.refund_failures.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub executions: Arc<RwLock<HashMap<Uuid, ExecutionRecord>>>,
    pub provider_inputs: ProviderInputStore,
    pub audit_base_url: String,
    pub ledger_base_url: String,
    pub ledger_manage_token: String,
    // These fields remain solely so feature-gated historical source compiles.
    // Default builds assign inert values and never read local-provider env vars.
    pub ollama_base_url: String,
    pub openclaw_cli_bin: String,
    pub openclaw_config_path: Option<String>,
    pub openclaw_state_dir: Option<String>,
    pub openclaw_agent_dir: Option<String>,
    pub http: Client,
    pub approval_reserve_threshold: f64,
    pub hard_reject_reserve_threshold: Option<f64>,
    pub approval_sensitive_keywords: Arc<Vec<String>>,
    pub block_keywords: Arc<Vec<String>>,
    pub approval_capability_prefixes: Arc<Vec<String>>,
    pub block_capability_prefixes: Arc<Vec<String>>,
    pub policy_bundle_path: Option<String>,
    pub policy_bundle_load_status: String,
    pub policy_bundle_load_error: Option<String>,
    pub execution_claim_lease_seconds: i64,
    pub execution_default_max_attempts: i32,
    pub execution_queued_worker_max_attempts: i32,
    pub execution_provider_dispatch_timeout_seconds: u64,
    pub execution_retry_backoff_seconds: i64,
    pub execution_retry_backoff_max_seconds: i64,
    pub alert_approval_backlog_threshold: usize,
    pub alert_lease_expired_threshold: usize,
    pub alert_retry_budget_exhausted_threshold: usize,
    pub alert_provider_failure_threshold: usize,
    pub alert_provider_billing_failure_threshold: usize,
    pub alert_provider_timeout_failure_threshold: usize,
    pub alert_provider_dead_letter_threshold: usize,
    pub alert_provider_retry_budget_exhausted_threshold: usize,
    pub alert_audit_failure_threshold: usize,
    pub alert_refund_failure_threshold: usize,
    pub pool: Option<PgPool>,
    pub fail_fast: bool,
    pub admin_tokens: Arc<HashMap<String, AdminPrincipal>>,
    pub metrics: Arc<ExecutionRuntimeMetrics>,
}

impl AppState {
    pub async fn from_env() -> Self {
        let audit_base_url =
            env::var("AUDIT_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:7004".to_string());
        let ledger_base_url =
            env::var("LEDGER_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:7002".to_string());
        let policy_bundle = load_execution_policy_bundle();
        let approval_reserve_threshold = env::var("APPROVAL_RESERVE_THRESHOLD")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .or(policy_bundle.bundle.approval_reserve_threshold)
            .unwrap_or(10.0);
        let hard_reject_reserve_threshold =
            optional_positive_f64_env("POLICY_HARD_REJECT_RESERVE_THRESHOLD")
                .or(policy_bundle.bundle.hard_reject_reserve_threshold);
        let default_approval_sensitive_keywords = policy_bundle
            .bundle
            .approval_sensitive_keywords
            .clone()
            .unwrap_or_else(|| {
                DEFAULT_APPROVAL_SENSITIVE_KEYWORDS
                    .iter()
                    .map(|value| value.to_string())
                    .collect()
            });
        let approval_sensitive_keywords = Arc::new(csv_env_or_vec_default(
            "POLICY_APPROVAL_SENSITIVE_KEYWORDS",
            default_approval_sensitive_keywords,
        ));
        let block_keywords = Arc::new(csv_env_or_vec_default(
            "POLICY_BLOCK_KEYWORDS",
            policy_bundle
                .bundle
                .block_keywords
                .clone()
                .unwrap_or_default(),
        ));
        let approval_capability_prefixes = Arc::new(csv_env_or_vec_default(
            "POLICY_APPROVAL_CAPABILITY_PREFIXES",
            policy_bundle
                .bundle
                .approval_capability_prefixes
                .clone()
                .unwrap_or_default(),
        ));
        let block_capability_prefixes = Arc::new(csv_env_or_vec_default(
            "POLICY_BLOCK_CAPABILITY_PREFIXES",
            policy_bundle
                .bundle
                .block_capability_prefixes
                .clone()
                .unwrap_or_default(),
        ));
        let execution_claim_lease_seconds = env::var("EXECUTION_CLAIM_LEASE_SECONDS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(300);

        // Legacy local-provider configuration is compiled into meaningful
        // values only when the explicit compatibility feature is selected.
        #[cfg(feature = "legacy-local-provider-dispatch")]
        let ollama_base_url =
            env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
        #[cfg(not(feature = "legacy-local-provider-dispatch"))]
        let ollama_base_url = String::new();
        #[cfg(feature = "legacy-local-provider-dispatch")]
        let openclaw_cli_bin =
            env::var("OPENCLAW_CLI_BIN").unwrap_or_else(|_| "openclaw".to_string());
        #[cfg(not(feature = "legacy-local-provider-dispatch"))]
        let openclaw_cli_bin = String::new();
        #[cfg(feature = "legacy-local-provider-dispatch")]
        let openclaw_config_path = optional_non_empty_env("OPENCLAW_CONFIG_PATH");
        #[cfg(not(feature = "legacy-local-provider-dispatch"))]
        let openclaw_config_path: Option<String> = None;
        #[cfg(feature = "legacy-local-provider-dispatch")]
        let openclaw_state_dir = optional_non_empty_env("OPENCLAW_STATE_DIR");
        #[cfg(not(feature = "legacy-local-provider-dispatch"))]
        let openclaw_state_dir: Option<String> = None;
        #[cfg(feature = "legacy-local-provider-dispatch")]
        let openclaw_agent_dir = optional_non_empty_env("OPENCLAW_AGENT_DIR");
        #[cfg(not(feature = "legacy-local-provider-dispatch"))]
        let openclaw_agent_dir: Option<String> = None;

        let execution_default_max_attempts = positive_i32_env(
            "EXECUTION_DEFAULT_MAX_ATTEMPTS",
            DEFAULT_EXECUTION_DEFAULT_MAX_ATTEMPTS,
        );
        let execution_queued_worker_max_attempts = positive_i32_env(
            "EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS",
            DEFAULT_EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS,
        );
        #[cfg(feature = "legacy-local-provider-dispatch")]
        let execution_provider_dispatch_timeout_seconds = positive_u64_env(
            "EXECUTION_PROVIDER_DISPATCH_TIMEOUT_SECONDS",
            DEFAULT_EXECUTION_PROVIDER_DISPATCH_TIMEOUT_SECONDS,
        );
        #[cfg(not(feature = "legacy-local-provider-dispatch"))]
        let execution_provider_dispatch_timeout_seconds =
            DEFAULT_EXECUTION_PROVIDER_DISPATCH_TIMEOUT_SECONDS;
        let execution_retry_backoff_seconds = positive_i64_env(
            "EXECUTION_RETRY_BACKOFF_SECONDS",
            DEFAULT_EXECUTION_RETRY_BACKOFF_SECONDS,
        );
        let execution_retry_backoff_max_seconds = positive_i64_env(
            "EXECUTION_RETRY_BACKOFF_MAX_SECONDS",
            DEFAULT_EXECUTION_RETRY_BACKOFF_MAX_SECONDS,
        );
        let alert_approval_backlog_threshold = positive_usize_env(
            "ALERT_APPROVAL_BACKLOG_THRESHOLD",
            DEFAULT_ALERT_APPROVAL_BACKLOG_THRESHOLD,
        );
        let alert_lease_expired_threshold = positive_usize_env(
            "ALERT_LEASE_EXPIRED_THRESHOLD",
            DEFAULT_ALERT_LEASE_EXPIRED_THRESHOLD,
        );
        let alert_retry_budget_exhausted_threshold = positive_usize_env(
            "ALERT_RETRY_BUDGET_EXHAUSTED_THRESHOLD",
            DEFAULT_ALERT_RETRY_BUDGET_EXHAUSTED_THRESHOLD,
        );
        let alert_provider_failure_threshold = positive_usize_env(
            "ALERT_PROVIDER_FAILURE_THRESHOLD",
            DEFAULT_ALERT_PROVIDER_FAILURE_THRESHOLD,
        );
        let alert_provider_billing_failure_threshold = positive_usize_env(
            "ALERT_PROVIDER_BILLING_FAILURE_THRESHOLD",
            DEFAULT_ALERT_PROVIDER_BILLING_FAILURE_THRESHOLD,
        );
        let alert_provider_timeout_failure_threshold = positive_usize_env(
            "ALERT_PROVIDER_TIMEOUT_FAILURE_THRESHOLD",
            DEFAULT_ALERT_PROVIDER_TIMEOUT_FAILURE_THRESHOLD,
        );
        let alert_provider_dead_letter_threshold = positive_usize_env(
            "ALERT_PROVIDER_DEAD_LETTER_THRESHOLD",
            DEFAULT_ALERT_PROVIDER_DEAD_LETTER_THRESHOLD,
        );
        let alert_provider_retry_budget_exhausted_threshold = positive_usize_env(
            "ALERT_PROVIDER_RETRY_BUDGET_EXHAUSTED_THRESHOLD",
            DEFAULT_ALERT_PROVIDER_RETRY_BUDGET_EXHAUSTED_THRESHOLD,
        );
        let alert_audit_failure_threshold = positive_usize_env(
            "ALERT_AUDIT_FAILURE_THRESHOLD",
            DEFAULT_ALERT_AUDIT_FAILURE_THRESHOLD,
        );
        let alert_refund_failure_threshold = positive_usize_env(
            "ALERT_REFUND_FAILURE_THRESHOLD",
            DEFAULT_ALERT_REFUND_FAILURE_THRESHOLD,
        );
        let fail_fast = env_flag("EXECUTION_FAIL_FAST", true);
        let pool = match env::var("DATABASE_URL") {
            Ok(database_url) => match PgPool::connect(&database_url).await {
                Ok(pool) => Some(pool),
                Err(err) => {
                    if fail_fast {
                        panic!("execution-service failed to connect postgres: {err}");
                    }
                    eprintln!("execution-service: postgres unavailable, falling back to in-memory state: {err}");
                    None
                }
            },
            Err(_) => {
                if fail_fast {
                    panic!("execution-service requires DATABASE_URL when EXECUTION_FAIL_FAST=true");
                }
                eprintln!(
                    "execution-service: DATABASE_URL is not set, falling back to in-memory state"
                );
                None
            }
        };

        let ledger_manage_token = load_ledger_scoped_admin_tokens()
            .into_iter()
            .find(|token| select_ledger_manage_token(std::slice::from_ref(token)).is_some())
            .map(|token| token.token)
            .unwrap_or_else(|| "local-dev-admin-token".to_string());

        Self {
            executions: Arc::new(RwLock::new(HashMap::new())),
            provider_inputs: ProviderInputStore::default(),
            audit_base_url,
            ledger_base_url,
            ledger_manage_token,
            ollama_base_url,
            openclaw_cli_bin,
            openclaw_config_path,
            openclaw_state_dir,
            openclaw_agent_dir,
            http: Client::new(),
            approval_reserve_threshold,
            hard_reject_reserve_threshold,
            approval_sensitive_keywords,
            block_keywords,
            approval_capability_prefixes,
            block_capability_prefixes,
            policy_bundle_path: policy_bundle.path,
            policy_bundle_load_status: policy_bundle.load_status,
            policy_bundle_load_error: policy_bundle.load_error,
            execution_claim_lease_seconds,
            execution_default_max_attempts,
            execution_queued_worker_max_attempts,
            execution_provider_dispatch_timeout_seconds,
            execution_retry_backoff_seconds,
            execution_retry_backoff_max_seconds,
            alert_approval_backlog_threshold,
            alert_lease_expired_threshold,
            alert_retry_budget_exhausted_threshold,
            alert_provider_failure_threshold,
            alert_provider_billing_failure_threshold,
            alert_provider_timeout_failure_threshold,
            alert_provider_dead_letter_threshold,
            alert_provider_retry_budget_exhausted_threshold,
            alert_audit_failure_threshold,
            alert_refund_failure_threshold,
            pool,
            fail_fast,
            admin_tokens: Arc::new(load_admin_tokens()),
            metrics: Arc::new(ExecutionRuntimeMetrics::default()),
        }
    }

    pub fn new_for_tests(
        fail_fast: bool,
        admin_token: Option<String>,
        scopes: Vec<String>,
        org_ids: Vec<String>,
    ) -> Self {
        Self::new_for_tests_with_attempt_limits(
            fail_fast,
            admin_token,
            scopes,
            org_ids,
            DEFAULT_EXECUTION_DEFAULT_MAX_ATTEMPTS,
            DEFAULT_EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS,
        )
    }

    pub fn new_for_tests_with_attempt_limits(
        fail_fast: bool,
        admin_token: Option<String>,
        scopes: Vec<String>,
        org_ids: Vec<String>,
        execution_default_max_attempts: i32,
        execution_queued_worker_max_attempts: i32,
    ) -> Self {
        let admin_tokens = admin_token
            .into_iter()
            .map(|token| {
                (
                    token,
                    AdminPrincipal {
                        actor_id: "test-execution-admin".to_string(),
                        actor_label: Some("Test Execution Admin".to_string()),
                        scopes: scopes.clone(),
                        org_ids: org_ids.clone(),
                    },
                )
            })
            .collect();

        Self {
            executions: Arc::new(RwLock::new(HashMap::new())),
            provider_inputs: ProviderInputStore::default(),
            audit_base_url: "http://127.0.0.1:9".to_string(),
            ledger_base_url: "http://127.0.0.1:9".to_string(),
            ledger_manage_token: "local-dev-admin-token".to_string(),
            ollama_base_url: String::new(),
            openclaw_cli_bin: String::new(),
            openclaw_config_path: None,
            openclaw_state_dir: None,
            openclaw_agent_dir: None,
            http: Client::new(),
            approval_reserve_threshold: 10.0,
            hard_reject_reserve_threshold: None,
            approval_sensitive_keywords: Arc::new(
                DEFAULT_APPROVAL_SENSITIVE_KEYWORDS
                    .iter()
                    .map(|value| value.to_string())
                    .collect(),
            ),
            block_keywords: Arc::new(Vec::new()),
            approval_capability_prefixes: Arc::new(Vec::new()),
            block_capability_prefixes: Arc::new(Vec::new()),
            policy_bundle_path: None,
            policy_bundle_load_status: "disabled".to_string(),
            policy_bundle_load_error: None,
            execution_claim_lease_seconds: 300,
            execution_default_max_attempts: execution_default_max_attempts.max(1),
            execution_queued_worker_max_attempts: execution_queued_worker_max_attempts.max(1),
            execution_provider_dispatch_timeout_seconds:
                DEFAULT_EXECUTION_PROVIDER_DISPATCH_TIMEOUT_SECONDS,
            execution_retry_backoff_seconds: DEFAULT_EXECUTION_RETRY_BACKOFF_SECONDS,
            execution_retry_backoff_max_seconds: DEFAULT_EXECUTION_RETRY_BACKOFF_MAX_SECONDS,
            alert_approval_backlog_threshold: DEFAULT_ALERT_APPROVAL_BACKLOG_THRESHOLD,
            alert_lease_expired_threshold: DEFAULT_ALERT_LEASE_EXPIRED_THRESHOLD,
            alert_retry_budget_exhausted_threshold: DEFAULT_ALERT_RETRY_BUDGET_EXHAUSTED_THRESHOLD,
            alert_provider_failure_threshold: DEFAULT_ALERT_PROVIDER_FAILURE_THRESHOLD,
            alert_provider_billing_failure_threshold:
                DEFAULT_ALERT_PROVIDER_BILLING_FAILURE_THRESHOLD,
            alert_provider_timeout_failure_threshold:
                DEFAULT_ALERT_PROVIDER_TIMEOUT_FAILURE_THRESHOLD,
            alert_provider_dead_letter_threshold: DEFAULT_ALERT_PROVIDER_DEAD_LETTER_THRESHOLD,
            alert_provider_retry_budget_exhausted_threshold:
                DEFAULT_ALERT_PROVIDER_RETRY_BUDGET_EXHAUSTED_THRESHOLD,
            alert_audit_failure_threshold: DEFAULT_ALERT_AUDIT_FAILURE_THRESHOLD,
            alert_refund_failure_threshold: DEFAULT_ALERT_REFUND_FAILURE_THRESHOLD,
            pool: None,
            fail_fast,
            admin_tokens: Arc::new(admin_tokens),
            metrics: Arc::new(ExecutionRuntimeMetrics::default()),
        }
    }
}

fn load_admin_tokens() -> HashMap<String, AdminPrincipal> {
    build_admin_principal_map(load_execution_scoped_admin_tokens())
}

fn env_flag(name: &str, default_value: bool) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default_value,
    }
}

fn positive_i32_env(name: &str, default_value: i32) -> i32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_value)
}

fn positive_usize_env(name: &str, default_value: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_value)
}

fn positive_i64_env(name: &str, default_value: i64) -> i64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_value)
}

#[cfg(feature = "legacy-local-provider-dispatch")]
fn positive_u64_env(name: &str, default_value: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_value)
}

fn optional_positive_f64_env(name: &str) -> Option<f64> {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| *value > 0.0)
}

fn csv_env_or_vec_default(name: &str, default_values: Vec<String>) -> Vec<String> {
    match env::var(name) {
        Ok(raw) => parse_csv_list(&raw),
        Err(_) => normalize_string_list(default_values),
    }
}

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn load_execution_policy_bundle() -> LoadedExecutionPolicyBundle {
    let Some(path) = optional_non_empty_env("EXECUTION_POLICY_BUNDLE_PATH") else {
        return LoadedExecutionPolicyBundle {
            path: None,
            load_status: "disabled".to_string(),
            load_error: None,
            bundle: ExecutionPolicyBundle::default(),
        };
    };

    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) => {
            return LoadedExecutionPolicyBundle {
                path: Some(path),
                load_status: "read_error".to_string(),
                load_error: Some(err.to_string()),
                bundle: ExecutionPolicyBundle::default(),
            }
        }
    };

    match serde_json::from_str::<ExecutionPolicyBundle>(&raw) {
        Ok(bundle) => LoadedExecutionPolicyBundle {
            path: Some(path),
            load_status: "loaded".to_string(),
            load_error: None,
            bundle,
        },
        Err(err) => LoadedExecutionPolicyBundle {
            path: Some(path),
            load_status: "parse_error".to_string(),
            load_error: Some(err.to_string()),
            bundle: ExecutionPolicyBundle::default(),
        },
    }
}

fn optional_non_empty_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_csv_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        parse_csv_list, positive_i32_env, positive_usize_env,
        DEFAULT_EXECUTION_DEFAULT_MAX_ATTEMPTS, DEFAULT_EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS,
    };
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn positive_i32_env_uses_defaults_when_missing_or_invalid() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::remove_var("EXECUTION_DEFAULT_MAX_ATTEMPTS");
        std::env::remove_var("EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS");

        assert_eq!(
            positive_i32_env(
                "EXECUTION_DEFAULT_MAX_ATTEMPTS",
                DEFAULT_EXECUTION_DEFAULT_MAX_ATTEMPTS,
            ),
            DEFAULT_EXECUTION_DEFAULT_MAX_ATTEMPTS,
        );
        assert_eq!(
            positive_i32_env(
                "EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS",
                DEFAULT_EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS,
            ),
            DEFAULT_EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS,
        );

        std::env::set_var("EXECUTION_DEFAULT_MAX_ATTEMPTS", "0");
        std::env::set_var("EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS", "not-a-number");

        assert_eq!(
            positive_i32_env(
                "EXECUTION_DEFAULT_MAX_ATTEMPTS",
                DEFAULT_EXECUTION_DEFAULT_MAX_ATTEMPTS,
            ),
            DEFAULT_EXECUTION_DEFAULT_MAX_ATTEMPTS,
        );
        assert_eq!(
            positive_i32_env(
                "EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS",
                DEFAULT_EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS,
            ),
            DEFAULT_EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS,
        );

        std::env::remove_var("EXECUTION_DEFAULT_MAX_ATTEMPTS");
        std::env::remove_var("EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS");
    }

    #[test]
    fn positive_i32_env_accepts_positive_overrides() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::set_var("EXECUTION_DEFAULT_MAX_ATTEMPTS", "2");
        std::env::set_var("EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS", "7");

        assert_eq!(
            positive_i32_env(
                "EXECUTION_DEFAULT_MAX_ATTEMPTS",
                DEFAULT_EXECUTION_DEFAULT_MAX_ATTEMPTS,
            ),
            2,
        );
        assert_eq!(
            positive_i32_env(
                "EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS",
                DEFAULT_EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS,
            ),
            7,
        );

        std::env::remove_var("EXECUTION_DEFAULT_MAX_ATTEMPTS");
        std::env::remove_var("EXECUTION_QUEUED_WORKER_MAX_ATTEMPTS");
    }

    #[test]
    fn parse_csv_list_trims_filters_and_normalizes_case() {
        assert_eq!(
            parse_csv_list(" publish, Deploy , ,EMAIL "),
            vec!["publish", "deploy", "email"]
        );
    }

    #[test]
    fn positive_usize_env_uses_default_when_invalid() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::set_var("ALERT_APPROVAL_BACKLOG_THRESHOLD", "0");
        assert_eq!(positive_usize_env("ALERT_APPROVAL_BACKLOG_THRESHOLD", 9), 9);
        std::env::set_var("ALERT_APPROVAL_BACKLOG_THRESHOLD", "4");
        assert_eq!(positive_usize_env("ALERT_APPROVAL_BACKLOG_THRESHOLD", 9), 4);
        std::env::remove_var("ALERT_APPROVAL_BACKLOG_THRESHOLD");
    }
}
