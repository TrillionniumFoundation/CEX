use crate::{domain::invocation::InvocationRecord, infrastructure::clients::ServiceClients};
use serde::Serialize;
use sqlx::PgPool;
use std::{
    collections::HashMap,
    env,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::sync::RwLock;
use uuid::Uuid;

pub const DEFAULT_ALERT_GATEWAY_UPSTREAM_FAILURE_THRESHOLD: usize = 1;
const LEGACY_RESERVE_BREAK_GLASS_ENV: &str = "CEX_GATEWAY_LEGACY_RESERVE_BREAK_GLASS";

#[derive(Debug, Default)]
pub struct GatewayRuntimeMetrics {
    pub invocation_create_requests: AtomicU64,
    pub invocation_create_auth_failures: AtomicU64,
    pub invocation_create_capability_failures: AtomicU64,
    pub invocation_create_upstream_failures: AtomicU64,
    pub invocation_get_requests: AtomicU64,
    pub invocation_get_auth_failures: AtomicU64,
    pub execution_approve_requests: AtomicU64,
    pub execution_retry_requests: AtomicU64,
    pub execution_cancel_requests: AtomicU64,
    pub saga_shadow_write_attempts: AtomicU64,
    pub saga_shadow_write_successes: AtomicU64,
    pub saga_shadow_write_failures: AtomicU64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewayRuntimeMetricsSnapshot {
    pub invocation_create_requests: u64,
    pub invocation_create_auth_failures: u64,
    pub invocation_create_capability_failures: u64,
    pub invocation_create_upstream_failures: u64,
    pub invocation_get_requests: u64,
    pub invocation_get_auth_failures: u64,
    pub execution_approve_requests: u64,
    pub execution_retry_requests: u64,
    pub execution_cancel_requests: u64,
    pub saga_shadow_write_attempts: u64,
    pub saga_shadow_write_successes: u64,
    pub saga_shadow_write_failures: u64,
}

impl GatewayRuntimeMetrics {
    pub fn snapshot(&self) -> GatewayRuntimeMetricsSnapshot {
        GatewayRuntimeMetricsSnapshot {
            invocation_create_requests: self.invocation_create_requests.load(Ordering::Relaxed),
            invocation_create_auth_failures: self
                .invocation_create_auth_failures
                .load(Ordering::Relaxed),
            invocation_create_capability_failures: self
                .invocation_create_capability_failures
                .load(Ordering::Relaxed),
            invocation_create_upstream_failures: self
                .invocation_create_upstream_failures
                .load(Ordering::Relaxed),
            invocation_get_requests: self.invocation_get_requests.load(Ordering::Relaxed),
            invocation_get_auth_failures: self.invocation_get_auth_failures.load(Ordering::Relaxed),
            execution_approve_requests: self.execution_approve_requests.load(Ordering::Relaxed),
            execution_retry_requests: self.execution_retry_requests.load(Ordering::Relaxed),
            execution_cancel_requests: self.execution_cancel_requests.load(Ordering::Relaxed),
            saga_shadow_write_attempts: self.saga_shadow_write_attempts.load(Ordering::Relaxed),
            saga_shadow_write_successes: self.saga_shadow_write_successes.load(Ordering::Relaxed),
            saga_shadow_write_failures: self.saga_shadow_write_failures.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewayAlertSignal {
    pub value: usize,
    pub threshold: usize,
    pub alert: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewayOperatorSignals {
    pub invocation_create_upstream_failures: GatewayAlertSignal,
}

#[derive(Clone)]
pub struct AppState {
    pub invocations: Arc<RwLock<HashMap<Uuid, InvocationRecord>>>,
    pub clients: Arc<ServiceClients>,
    pub pool: Option<PgPool>,
    pub fail_fast: bool,
    /// Explicitly governed compatibility escape hatch for the legacy
    /// `reserve_amount: f64` path. It is disabled for production-like
    /// profiles and defaults to false everywhere else.
    pub legacy_reserve_break_glass: bool,
    pub metrics: Arc<GatewayRuntimeMetrics>,
    pub alert_gateway_upstream_failure_threshold: usize,
}

impl AppState {
    pub async fn from_env(clients: ServiceClients) -> Self {
        let fail_fast = env_flag("GATEWAY_FAIL_FAST", true);
        let alert_gateway_upstream_failure_threshold = positive_usize_env(
            "ALERT_GATEWAY_UPSTREAM_FAILURE_THRESHOLD",
            DEFAULT_ALERT_GATEWAY_UPSTREAM_FAILURE_THRESHOLD,
        );
        let legacy_reserve_break_glass = legacy_reserve_break_glass_from_env();
        let pool = match env::var("DATABASE_URL") {
            Ok(database_url) => match PgPool::connect(&database_url).await {
                Ok(pool) => Some(pool),
                Err(err) => {
                    if fail_fast {
                        panic!("gateway-service failed to connect postgres: {err}");
                    }
                    eprintln!("gateway-service: postgres unavailable, falling back to in-memory state: {err}");
                    None
                }
            },
            Err(_) => {
                if fail_fast {
                    panic!("gateway-service requires DATABASE_URL when GATEWAY_FAIL_FAST=true");
                }
                eprintln!(
                    "gateway-service: DATABASE_URL is not set, falling back to in-memory state"
                );
                None
            }
        };

        Self {
            invocations: Arc::new(RwLock::new(HashMap::new())),
            clients: Arc::new(clients),
            pool,
            fail_fast,
            legacy_reserve_break_glass,
            metrics: Arc::new(GatewayRuntimeMetrics::default()),
            alert_gateway_upstream_failure_threshold,
        }
    }
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

fn positive_usize_env(name: &str, default_value: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_value)
}

fn legacy_reserve_break_glass_from_env() -> bool {
    if !env_flag(LEGACY_RESERVE_BREAK_GLASS_ENV, false) {
        return false;
    }

    let production_like_profile = ["CEX_RUNTIME_PROFILE", "APP_ENV"]
        .into_iter()
        .filter_map(|name| env::var(name).ok())
        .map(|value| value.trim().to_ascii_lowercase())
        .any(|profile| {
            matches!(
                profile.as_str(),
                "beta" | "staging" | "stage" | "production" | "prod"
            )
        });
    if production_like_profile {
        eprintln!(
            "gateway-service: {LEGACY_RESERVE_BREAK_GLASS_ENV}=true is ignored in production-like profile; legacy reserve remains fail-closed"
        );
        return false;
    }

    eprintln!(
        "gateway-service: legacy reserve break-glass enabled for a non-production profile; exact reserve ingress remains the required migration path"
    );
    true
}
