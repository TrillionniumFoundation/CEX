#![recursion_limit = "256"]

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

const DEFAULT_MAX_TEXT_CHARS: usize = 4_000;
const DEFAULT_RATE_LIMIT_WINDOW_SECS: u64 = 60;
const DEFAULT_RATE_LIMIT_MAX_REQUESTS: usize = 20;
const DEFAULT_RECENT_EVENT_WINDOW_SECS: u64 = 600;
const DEFAULT_RECENT_EVENT_CACHE_SIZE: usize = 2048;
const DEFAULT_CONSUMER_ENTRY_SESSION_AUTH_TTL_SECS: u64 = 300;
const DEFAULT_CONSUMER_ENTRY_SESSION_AUTH_AUDIENCE: &str = "consumer-entry-api";
const USER_SESSION_ASSERTION_HEADER: &str = "x-cex-user-session";
const USER_SESSION_SIGNATURE_HEADER: &str = "x-cex-user-session-signature";
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    env,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock as StdRwLock,
    },
    time::UNIX_EPOCH,
};
use tokio::sync::Mutex;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    http: Client,
    config: MatrixAdapterConfig,
    consumer_entry_session_auth_issuer_registry_state:
        StdRwLock<SessionAuthIssuerRegistryRuntimeState>,
    rate_limits: Mutex<RateLimitCache>,
    recent_event_cache: Mutex<RecentEventCache>,
    metrics: MatrixEntryMetrics,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionAuthIssuerRegistryRuntimeState {
    pub metadata: SessionAuthIssuerRegistryMetadata,
    pub registry: HashMap<String, SessionAuthIssuerRegistryIssuer>,
}

#[derive(Debug, Default)]
struct MatrixEntryMetrics {
    event_requests: AtomicU64,
    projection_requests: AtomicU64,
    duplicate_events: AtomicU64,
    rate_limited_requests: AtomicU64,
    ingress_auth_failures: AtomicU64,
    ignored_self_events: AtomicU64,
}

impl MatrixEntryMetrics {
    fn inc_event_requests(&self) {
        self.event_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_projection_requests(&self) {
        self.projection_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_duplicate_events(&self) {
        self.duplicate_events.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_requests(&self) {
        self.rate_limited_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_ingress_auth_failures(&self) {
        self.ingress_auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_ignored_self_events(&self) {
        self.ignored_self_events.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> Value {
        json!({
            "event_requests": self.event_requests.load(Ordering::Relaxed),
            "projection_requests": self.projection_requests.load(Ordering::Relaxed),
            "duplicate_events": self.duplicate_events.load(Ordering::Relaxed),
            "rate_limited_requests": self.rate_limited_requests.load(Ordering::Relaxed),
            "ingress_auth_failures": self.ingress_auth_failures.load(Ordering::Relaxed),
            "ignored_self_events": self.ignored_self_events.load(Ordering::Relaxed),
        })
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentEventCache {
    seen: HashMap<String, i64>,
    order: VecDeque<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RateLimitCache {
    seen: HashMap<String, VecDeque<i64>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProfile {
    LocalDev,
    Beta,
    Production,
}

impl RuntimeProfile {
    fn as_str(self) -> &'static str {
        match self {
            Self::LocalDev => "local_dev",
            Self::Beta => "beta",
            Self::Production => "production",
        }
    }

    fn from_env() -> Self {
        match first_present_env(&["MATRIX_ENTRY_RUNTIME_PROFILE", "CEX_RUNTIME_PROFILE"])
            .as_deref()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("beta") => Self::Beta,
            Some("production") | Some("prod") => Self::Production,
            _ => Self::LocalDev,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatrixAdapterConfig {
    pub runtime_profile: RuntimeProfile,
    pub bind_addr: String,
    pub consumer_entry_base_url: String,
    pub consumer_entry_api_key: Option<String>,
    pub consumer_entry_ingress_token: Option<String>,
    pub consumer_entry_session_auth_explicit_secret: Option<String>,
    pub consumer_entry_session_auth_explicit_key_id: Option<String>,
    pub consumer_entry_session_auth_secret: Option<String>,
    pub consumer_entry_session_auth_key_id: Option<String>,
    pub consumer_entry_session_auth_issuer_registry_path: Option<String>,
    pub consumer_entry_session_auth_issuer_registry:
        HashMap<String, SessionAuthIssuerRegistryIssuer>,
    pub consumer_entry_session_auth_issuer_registry_load_error: Option<String>,
    pub consumer_entry_session_auth_issuer_registry_metadata: SessionAuthIssuerRegistryMetadata,
    pub consumer_entry_session_auth_issuer_registry_approved_revisions_path: Option<String>,
    pub consumer_entry_session_auth_issuer_registry_require_approved_revision: bool,
    pub consumer_entry_session_auth_selection: SessionAuthIssuerRegistrySelection,
    pub consumer_entry_session_auth_issuer: String,
    pub consumer_entry_session_auth_audience: String,
    pub consumer_entry_session_auth_ttl_secs: u64,
    pub ingress_token: Option<String>,
    pub bot_user_id: String,
    pub max_text_chars: usize,
    pub rate_limit_window_secs: u64,
    pub rate_limit_max_requests: usize,
    pub rate_limit_store_path: Option<String>,
    pub recent_event_window_secs: u64,
    pub recent_event_cache_size: usize,
    pub recent_event_store_path: Option<String>,
}

impl MatrixAdapterConfig {
    pub fn from_env() -> Self {
        let consumer_entry_session_auth_issuer =
            env::var("MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "matrix-entry-adapter".to_string());
        let consumer_entry_session_auth_issuer_registry_path = first_present_env(&[
            "MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_PATH",
            "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH",
            "CEX_SESSION_AUTH_ISSUER_REGISTRY_PATH",
        ]);
        let (
            consumer_entry_session_auth_issuer_registry_metadata,
            consumer_entry_session_auth_issuer_registry,
        ) = load_session_auth_issuer_registry(
            consumer_entry_session_auth_issuer_registry_path.as_deref(),
        );
        let consumer_entry_session_auth_issuer_registry_load_error =
            consumer_entry_session_auth_issuer_registry_metadata
                .load_error
                .clone();
        let consumer_entry_session_auth_issuer_registry_approved_revisions_path =
            first_present_env(&[
                "MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH",
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH",
                "CEX_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH",
            ]);
        let explicit_consumer_entry_session_auth_secret = first_present_env(&[
            "MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET",
            "CONSUMER_ENTRY_SESSION_AUTH_SECRET",
        ]);
        let explicit_consumer_entry_session_auth_key_id =
            env::var("MATRIX_ENTRY_CONSUMER_SESSION_AUTH_KEY_ID")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
        let (
            consumer_entry_session_auth_key_id,
            consumer_entry_session_auth_secret,
            consumer_entry_session_auth_selection,
        ) = if explicit_consumer_entry_session_auth_secret.is_some() {
            (
                explicit_consumer_entry_session_auth_key_id.clone(),
                explicit_consumer_entry_session_auth_secret.clone(),
                SessionAuthIssuerRegistrySelection {
                    source: "explicit_secret".to_string(),
                    status: "ok".to_string(),
                    issuer: consumer_entry_session_auth_issuer.clone(),
                    key_id: explicit_consumer_entry_session_auth_key_id.clone(),
                    registry_path: consumer_entry_session_auth_issuer_registry_path.clone(),
                    detail: Some(
                        "using MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET / CONSUMER_ENTRY_SESSION_AUTH_SECRET"
                            .to_string(),
                    ),
                },
            )
        } else if let Some(registry_entry) = consumer_entry_session_auth_issuer_registry
            .get(consumer_entry_session_auth_issuer.as_str())
        {
            let selected_key_id = explicit_consumer_entry_session_auth_key_id
                .clone()
                .or_else(|| registry_entry.active_key_id.clone());
            let selected_secret = selected_key_id
                .as_deref()
                .and_then(|key_id| registry_entry.keys.get(key_id).cloned());
            let selection = if let Some(key_id) = selected_key_id.clone() {
                if selected_secret.is_some() {
                    SessionAuthIssuerRegistrySelection {
                        source: "issuer_registry".to_string(),
                        status: "ok".to_string(),
                        issuer: consumer_entry_session_auth_issuer.clone(),
                        key_id: Some(key_id),
                        registry_path: consumer_entry_session_auth_issuer_registry_path.clone(),
                        detail: Some(
                            "selected signing secret from shared issuer registry".to_string(),
                        ),
                    }
                } else {
                    SessionAuthIssuerRegistrySelection {
                        source: "issuer_registry".to_string(),
                        status: "unknown_key_id".to_string(),
                        issuer: consumer_entry_session_auth_issuer.clone(),
                        key_id: Some(key_id.clone()),
                        registry_path: consumer_entry_session_auth_issuer_registry_path.clone(),
                        detail: Some(format!(
                            "shared issuer registry entry for issuer {} does not contain key_id {}",
                            consumer_entry_session_auth_issuer, key_id
                        )),
                    }
                }
            } else {
                SessionAuthIssuerRegistrySelection {
                    source: "issuer_registry".to_string(),
                    status: "missing_key_id".to_string(),
                    issuer: consumer_entry_session_auth_issuer.clone(),
                    key_id: None,
                    registry_path: consumer_entry_session_auth_issuer_registry_path.clone(),
                    detail: Some(format!(
                        "shared issuer registry entry for issuer {} has no active key and MATRIX_ENTRY_CONSUMER_SESSION_AUTH_KEY_ID was not set",
                        consumer_entry_session_auth_issuer
                    )),
                }
            };
            (selected_key_id, selected_secret, selection)
        } else {
            let selection = if consumer_entry_session_auth_issuer_registry_path.is_some() {
                SessionAuthIssuerRegistrySelection {
                    source: "issuer_registry".to_string(),
                    status: "issuer_missing".to_string(),
                    issuer: consumer_entry_session_auth_issuer.clone(),
                    key_id: explicit_consumer_entry_session_auth_key_id.clone(),
                    registry_path: consumer_entry_session_auth_issuer_registry_path.clone(),
                    detail: Some(format!(
                        "shared issuer registry does not contain issuer {}",
                        consumer_entry_session_auth_issuer
                    )),
                }
            } else {
                SessionAuthIssuerRegistrySelection {
                    source: "none".to_string(),
                    status: "unconfigured".to_string(),
                    issuer: consumer_entry_session_auth_issuer.clone(),
                    key_id: explicit_consumer_entry_session_auth_key_id.clone(),
                    registry_path: None,
                    detail: Some(
                        "no explicit secret or shared issuer registry configured".to_string(),
                    ),
                }
            };
            (
                explicit_consumer_entry_session_auth_key_id.clone(),
                explicit_consumer_entry_session_auth_secret.clone(),
                selection,
            )
        };

        Self {
            runtime_profile: RuntimeProfile::from_env(),
            bind_addr: env::var("MATRIX_ENTRY_ADAPTER_BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8091".to_string()),
            consumer_entry_base_url: env::var("CONSUMER_ENTRY_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string()),
            consumer_entry_api_key: env::var("CONSUMER_ENTRY_API_KEY")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            consumer_entry_ingress_token: env::var("CONSUMER_ENTRY_INGRESS_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            consumer_entry_session_auth_explicit_secret:
                explicit_consumer_entry_session_auth_secret,
            consumer_entry_session_auth_explicit_key_id:
                explicit_consumer_entry_session_auth_key_id,
            consumer_entry_session_auth_secret,
            consumer_entry_session_auth_key_id,
            consumer_entry_session_auth_issuer_registry_path,
            consumer_entry_session_auth_issuer_registry,
            consumer_entry_session_auth_issuer_registry_load_error,
            consumer_entry_session_auth_issuer_registry_metadata,
            consumer_entry_session_auth_issuer_registry_approved_revisions_path,
            consumer_entry_session_auth_issuer_registry_require_approved_revision: boolean_env(
                &[
                    "MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION",
                    "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION",
                ],
                false,
            ),
            consumer_entry_session_auth_selection,
            consumer_entry_session_auth_issuer,
            consumer_entry_session_auth_audience: env::var(
                "MATRIX_ENTRY_CONSUMER_SESSION_AUTH_AUDIENCE",
            )
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_CONSUMER_ENTRY_SESSION_AUTH_AUDIENCE.to_string()),
            consumer_entry_session_auth_ttl_secs: positive_u64_env(
                "MATRIX_ENTRY_CONSUMER_SESSION_AUTH_TTL_SECS",
                DEFAULT_CONSUMER_ENTRY_SESSION_AUTH_TTL_SECS,
            ),
            ingress_token: env::var("MATRIX_ENTRY_INGRESS_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            rate_limit_store_path: env::var("MATRIX_ENTRY_RATE_LIMIT_STORE_PATH")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            recent_event_store_path: env::var("MATRIX_ENTRY_RECENT_EVENT_STORE_PATH")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            bot_user_id: env::var("MATRIX_BOT_USER_ID")
                .unwrap_or_else(|_| "@cex-bot:local.dev".to_string()),
            max_text_chars: positive_usize_env(
                "MATRIX_ENTRY_MAX_TEXT_CHARS",
                DEFAULT_MAX_TEXT_CHARS,
            ),
            rate_limit_window_secs: positive_u64_env(
                "MATRIX_ENTRY_RATE_LIMIT_WINDOW_SECS",
                DEFAULT_RATE_LIMIT_WINDOW_SECS,
            ),
            rate_limit_max_requests: positive_usize_env(
                "MATRIX_ENTRY_RATE_LIMIT_MAX_REQUESTS",
                DEFAULT_RATE_LIMIT_MAX_REQUESTS,
            ),
            recent_event_window_secs: positive_u64_env(
                "MATRIX_ENTRY_RECENT_EVENT_WINDOW_SECS",
                DEFAULT_RECENT_EVENT_WINDOW_SECS,
            ),
            recent_event_cache_size: positive_usize_env(
                "MATRIX_ENTRY_RECENT_EVENT_CACHE_SIZE",
                DEFAULT_RECENT_EVENT_CACHE_SIZE,
            ),
        }
    }

    fn profile_validation_errors(&self) -> Vec<String> {
        let runtime_state = SessionAuthIssuerRegistryRuntimeState {
            metadata: self
                .consumer_entry_session_auth_issuer_registry_metadata
                .clone(),
            registry: self.consumer_entry_session_auth_issuer_registry.clone(),
        };
        matrix_profile_validation_errors(self, &runtime_state)
    }

    fn validate_runtime_profile(&self) -> Result<(), Vec<String>> {
        let errors = self.profile_validation_errors();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn first_present_env(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        env::var(name)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn boolean_env(names: &[&str], default: bool) -> bool {
    first_present_env(names)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn positive_usize_env(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn positive_u64_env(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn default_session_auth_issuer_registry_version() -> u32 {
    1
}

fn consumer_entry_session_auth_issuer_registry_runtime_state(
    state: &AppState,
) -> SessionAuthIssuerRegistryRuntimeState {
    state
        .inner
        .consumer_entry_session_auth_issuer_registry_state
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn resolve_consumer_entry_session_auth_runtime_selection(
    config: &MatrixAdapterConfig,
    runtime_state: &SessionAuthIssuerRegistryRuntimeState,
) -> (
    Option<String>,
    Option<String>,
    SessionAuthIssuerRegistrySelection,
) {
    if let Some(secret) = config
        .consumer_entry_session_auth_explicit_secret
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return (
            Some(secret.to_string()),
            config.consumer_entry_session_auth_explicit_key_id.clone(),
            SessionAuthIssuerRegistrySelection {
                source: "explicit_secret".to_string(),
                status: "ok".to_string(),
                issuer: config.consumer_entry_session_auth_issuer.clone(),
                key_id: config.consumer_entry_session_auth_explicit_key_id.clone(),
                registry_path: config.consumer_entry_session_auth_issuer_registry_path.clone(),
                detail: Some(
                    "using MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET / CONSUMER_ENTRY_SESSION_AUTH_SECRET"
                        .to_string(),
                ),
            },
        );
    }

    if let Some(registry_entry) = runtime_state
        .registry
        .get(config.consumer_entry_session_auth_issuer.as_str())
    {
        let selected_key_id = config
            .consumer_entry_session_auth_explicit_key_id
            .clone()
            .or_else(|| registry_entry.active_key_id.clone());
        let selected_secret = selected_key_id
            .as_deref()
            .and_then(|key_id| registry_entry.keys.get(key_id).cloned());
        let selection = if let Some(key_id) = selected_key_id.clone() {
            if selected_secret.is_some() {
                SessionAuthIssuerRegistrySelection {
                    source: "issuer_registry".to_string(),
                    status: "ok".to_string(),
                    issuer: config.consumer_entry_session_auth_issuer.clone(),
                    key_id: Some(key_id),
                    registry_path: config
                        .consumer_entry_session_auth_issuer_registry_path
                        .clone(),
                    detail: Some("selected signing secret from shared issuer registry".to_string()),
                }
            } else {
                SessionAuthIssuerRegistrySelection {
                    source: "issuer_registry".to_string(),
                    status: "unknown_key_id".to_string(),
                    issuer: config.consumer_entry_session_auth_issuer.clone(),
                    key_id: Some(key_id.clone()),
                    registry_path: config
                        .consumer_entry_session_auth_issuer_registry_path
                        .clone(),
                    detail: Some(format!(
                        "shared issuer registry entry for issuer {} does not contain key_id {}",
                        config.consumer_entry_session_auth_issuer, key_id
                    )),
                }
            }
        } else {
            SessionAuthIssuerRegistrySelection {
                source: "issuer_registry".to_string(),
                status: "missing_key_id".to_string(),
                issuer: config.consumer_entry_session_auth_issuer.clone(),
                key_id: None,
                registry_path: config.consumer_entry_session_auth_issuer_registry_path.clone(),
                detail: Some(format!(
                    "shared issuer registry entry for issuer {} has no active key and MATRIX_ENTRY_CONSUMER_SESSION_AUTH_KEY_ID was not set",
                    config.consumer_entry_session_auth_issuer
                )),
            }
        };
        return (selected_secret, selected_key_id, selection);
    }

    let selection = if config
        .consumer_entry_session_auth_issuer_registry_path
        .is_some()
    {
        SessionAuthIssuerRegistrySelection {
            source: "issuer_registry".to_string(),
            status: "issuer_missing".to_string(),
            issuer: config.consumer_entry_session_auth_issuer.clone(),
            key_id: config.consumer_entry_session_auth_explicit_key_id.clone(),
            registry_path: config
                .consumer_entry_session_auth_issuer_registry_path
                .clone(),
            detail: Some(format!(
                "shared issuer registry does not contain issuer {}",
                config.consumer_entry_session_auth_issuer
            )),
        }
    } else {
        SessionAuthIssuerRegistrySelection {
            source: "none".to_string(),
            status: "unconfigured".to_string(),
            issuer: config.consumer_entry_session_auth_issuer.clone(),
            key_id: config.consumer_entry_session_auth_explicit_key_id.clone(),
            registry_path: None,
            detail: Some("no explicit secret or shared issuer registry configured".to_string()),
        }
    };

    (
        None,
        config.consumer_entry_session_auth_explicit_key_id.clone(),
        selection,
    )
}

fn matrix_profile_validation_errors(
    config: &MatrixAdapterConfig,
    runtime_state: &SessionAuthIssuerRegistryRuntimeState,
) -> Vec<String> {
    let mut errors = Vec::new();

    if matches!(
        config.runtime_profile,
        RuntimeProfile::Beta | RuntimeProfile::Production
    ) {
        if config.ingress_token.is_none() {
            errors.push("beta/production profile requires MATRIX_ENTRY_INGRESS_TOKEN".to_string());
        }
        if config.consumer_entry_ingress_token.is_none() {
            errors
                .push("beta/production profile requires CONSUMER_ENTRY_INGRESS_TOKEN".to_string());
        }
        if let Some(error) = runtime_state.metadata.load_error.as_deref() {
            errors.push(format!(
                "failed to load MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_PATH: {error}"
            ));
        } else if config
            .consumer_entry_session_auth_issuer_registry_path
            .is_some()
            && runtime_state.registry.is_empty()
        {
            errors.push(
                "MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_PATH is configured but contains no issuers"
                    .to_string(),
            );
        }
        let (selected_secret, _, selection) =
            resolve_consumer_entry_session_auth_runtime_selection(config, runtime_state);
        if selected_secret.is_none() {
            let detail = selection.detail.clone().unwrap_or_else(|| {
                "beta/production profile requires MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET or CONSUMER_ENTRY_SESSION_AUTH_SECRET or MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_PATH"
                    .to_string()
            });
            errors.push(detail);
        }
        if config.consumer_entry_session_auth_issuer_registry_require_approved_revision {
            let approval_state =
                load_consumer_entry_session_auth_issuer_registry_revision_approval_state(config);
            let approval_checks = consumer_entry_session_auth_issuer_registry_approval_checks_json(
                config,
                &runtime_state.metadata,
                &approval_state,
            );
            let approval_status = approval_checks
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("approval_state_not_loaded");
            if config
                .consumer_entry_session_auth_issuer_registry_approved_revisions_path
                .is_none()
            {
                errors.push(
                    "MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION=true requires MATRIX_ENTRY_CONSUMER_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH"
                        .to_string(),
                );
            } else if approval_status != "ok" {
                errors.push(format!(
                    "matrix consumer-entry session auth issuer registry approval invalid: {approval_status}"
                ));
            }
        }
        if config.recent_event_store_path.is_none() {
            errors.push(
                "beta/production profile requires MATRIX_ENTRY_RECENT_EVENT_STORE_PATH".to_string(),
            );
        }
        if config.rate_limit_store_path.is_none() {
            errors.push(
                "beta/production profile requires MATRIX_ENTRY_RATE_LIMIT_STORE_PATH".to_string(),
            );
        }
        if config.bot_user_id.trim() == "@cex-bot:local.dev" {
            errors.push(
                "beta/production profile requires non-default MATRIX_BOT_USER_ID".to_string(),
            );
        }
    }

    errors
}

fn consumer_entry_session_auth_governance_overview_json(
    config: &MatrixAdapterConfig,
    metadata: &SessionAuthIssuerRegistryMetadata,
    selection: &SessionAuthIssuerRegistrySelection,
    approval_state: &SessionAuthIssuerRegistryRevisionApprovalState,
    approval_limit: usize,
) -> Value {
    let registry_configured = config
        .consumer_entry_session_auth_issuer_registry_path
        .is_some();
    let approval_source_configured = config
        .consumer_entry_session_auth_issuer_registry_approved_revisions_path
        .is_some();
    let selection_status = if selection.status.trim().is_empty() {
        if config.consumer_entry_session_auth_explicit_secret.is_some() {
            "ok".to_string()
        } else {
            "unconfigured".to_string()
        }
    } else {
        selection.status.clone()
    };
    let selection_ok = selection_status == "ok";
    let session_auth_expected = matches!(
        config.runtime_profile,
        RuntimeProfile::Beta | RuntimeProfile::Production
    ) || config.consumer_entry_session_auth_explicit_secret.is_some()
        || registry_configured;
    let registry_loaded = !registry_configured || metadata.load_status == "loaded";
    let revision_present = !registry_configured || metadata.revision.is_some();
    let approval_checks = consumer_entry_session_auth_issuer_registry_approval_checks_json(
        config,
        metadata,
        approval_state,
    );
    let approval_source = consumer_entry_session_auth_issuer_registry_approval_source_json(
        config,
        metadata,
        approval_state,
        approval_limit,
    );
    let approval_source_valid = if !registry_configured {
        true
    } else if !approval_source_configured {
        !config.consumer_entry_session_auth_issuer_registry_require_approved_revision
    } else {
        approval_source
            .get("valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let approval_coverage_valid = if !registry_configured
        || !config.consumer_entry_session_auth_issuer_registry_require_approved_revision
    {
        true
    } else {
        approval_checks
            .get("valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let approval_source_status = approval_source
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let approval_coverage_status = approval_checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let status = if !session_auth_expected {
        "not_required".to_string()
    } else if !registry_loaded {
        "issuer_registry_not_loaded".to_string()
    } else if !revision_present {
        "issuer_registry_revision_missing".to_string()
    } else if !approval_source_valid {
        approval_source_status.to_string()
    } else if !approval_coverage_valid {
        approval_coverage_status.to_string()
    } else if !selection_ok {
        selection_status.clone()
    } else {
        "ok".to_string()
    };

    json!({
        "status": status,
        "valid": status == "ok" || status == "not_required",
        "expected": session_auth_expected,
        "registry_configured": registry_configured,
        "approval_required": config.consumer_entry_session_auth_issuer_registry_require_approved_revision,
        "selection": selection,
        "checks": {
            "selection_ok": selection_ok,
            "registry_loaded": registry_loaded,
            "revision_present": revision_present,
            "approval_source_valid": approval_source_valid,
            "approval_coverage_valid": approval_coverage_valid,
        },
        "issuer_registry_metadata": metadata,
        "issuer_registry_approval": approval_state,
        "issuer_registry_approval_checks": approval_checks,
        "issuer_registry_approval_source": approval_source,
    })
}

fn system_time_to_epoch(time: std::time::SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
}

fn load_session_auth_issuer_registry(
    path: Option<&str>,
) -> (
    SessionAuthIssuerRegistryMetadata,
    HashMap<String, SessionAuthIssuerRegistryIssuer>,
) {
    let Some(path) = path.map(str::trim).filter(|value| !value.is_empty()) else {
        return (SessionAuthIssuerRegistryMetadata::default(), HashMap::new());
    };

    let source_modified_epoch = std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_to_epoch);
    let loaded_at_epoch = Some(Utc::now().timestamp());
    let raw = match std::fs::read_to_string(path) {
        Ok(value) => value,
        Err(err) => {
            return (
                SessionAuthIssuerRegistryMetadata {
                    source_path: Some(path.to_string()),
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "read_error".to_string(),
                    load_error: Some(err.to_string()),
                    ..SessionAuthIssuerRegistryMetadata::default()
                },
                HashMap::new(),
            )
        }
    };
    let parsed = match serde_json::from_str::<SessionAuthIssuerRegistryDocument>(&raw) {
        Ok(value) => value,
        Err(err) => {
            return (
                SessionAuthIssuerRegistryMetadata {
                    source_path: Some(path.to_string()),
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                    ..SessionAuthIssuerRegistryMetadata::default()
                },
                HashMap::new(),
            )
        }
    };

    let mut normalized = HashMap::new();
    let mut key_count = 0usize;
    for (issuer, entry) in parsed.issuers {
        let issuer = issuer.trim();
        if issuer.is_empty() {
            continue;
        }
        let mut normalized_keys = HashMap::new();
        for (key_id, secret) in entry.keys {
            let key_id = key_id.trim();
            let secret = secret.trim();
            if key_id.is_empty() || secret.is_empty() {
                continue;
            }
            normalized_keys.insert(key_id.to_string(), secret.to_string());
        }
        if normalized_keys.is_empty() {
            continue;
        }
        let active_key_id = entry
            .active_key_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if let Some(active_key_id) = active_key_id.as_deref() {
            if !normalized_keys.contains_key(active_key_id) {
                return (
                    SessionAuthIssuerRegistryMetadata {
                        version: parsed.version,
                        revision: parsed.revision.clone(),
                        source_path: Some(path.to_string()),
                        source_modified_epoch,
                        loaded_at_epoch,
                        load_status: "invalid_active_key".to_string(),
                        load_error: Some(format!(
                            "issuer {issuer} declares active key {active_key_id} but no matching secret exists"
                        )),
                        issuer_count: normalized.len(),
                        key_count,
                    },
                    HashMap::new(),
                );
            }
        }
        key_count += normalized_keys.len();
        normalized.insert(
            issuer.to_string(),
            SessionAuthIssuerRegistryIssuer {
                active_key_id,
                keys: normalized_keys,
            },
        );
    }

    (
        SessionAuthIssuerRegistryMetadata {
            version: parsed.version,
            revision: parsed.revision,
            source_path: Some(path.to_string()),
            source_modified_epoch,
            loaded_at_epoch,
            load_status: "loaded".to_string(),
            load_error: None,
            issuer_count: normalized.len(),
            key_count,
        },
        normalized,
    )
}

fn normalize_revision_list(revisions: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for revision in revisions {
        let revision = revision.trim();
        if revision.is_empty() {
            continue;
        }
        if normalized.iter().any(|existing| existing == revision) {
            continue;
        }
        normalized.push(revision.to_string());
    }
    normalized
}

fn load_consumer_entry_session_auth_issuer_registry_revision_approval_state(
    config: &MatrixAdapterConfig,
) -> SessionAuthIssuerRegistryRevisionApprovalState {
    let Some(path) = config
        .consumer_entry_session_auth_issuer_registry_approved_revisions_path
        .as_deref()
    else {
        return SessionAuthIssuerRegistryRevisionApprovalState::default();
    };

    let loaded_at_epoch = Some(Utc::now().timestamp());
    let source_path = Some(path.to_string());
    let source_modified_epoch = std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_to_epoch);

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) => {
            return SessionAuthIssuerRegistryRevisionApprovalState {
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "read_error".to_string(),
                load_error: Some(err.to_string()),
                version: 0,
                revision: None,
                approved_revisions: Vec::new(),
            }
        }
    };

    let document =
        match serde_json::from_str::<SessionAuthIssuerRegistryRevisionApprovalDocument>(&raw) {
            Ok(document) => document,
            Err(err) => {
                return SessionAuthIssuerRegistryRevisionApprovalState {
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                    version: 0,
                    revision: None,
                    approved_revisions: Vec::new(),
                }
            }
        };

    SessionAuthIssuerRegistryRevisionApprovalState {
        source_path,
        source_modified_epoch,
        loaded_at_epoch,
        load_status: "loaded".to_string(),
        load_error: None,
        version: document.version,
        revision: document.revision,
        approved_revisions: normalize_revision_list(document.approved_revisions),
    }
}

fn consumer_entry_session_auth_issuer_registry_current_revision(
    metadata: &SessionAuthIssuerRegistryMetadata,
) -> Option<String> {
    metadata
        .revision
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn consumer_entry_session_auth_issuer_registry_approval_checks_json(
    config: &MatrixAdapterConfig,
    metadata: &SessionAuthIssuerRegistryMetadata,
    approval_state: &SessionAuthIssuerRegistryRevisionApprovalState,
) -> Value {
    let current_revision = consumer_entry_session_auth_issuer_registry_current_revision(metadata);
    let current_revision_index = current_revision.as_ref().and_then(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .position(|approved| approved == revision)
    });
    let latest_approved_revision = approval_state.approved_revisions.last().cloned();
    let latest_approved_revision_index = approval_state.approved_revisions.len().checked_sub(1);
    let current_revision_approved = current_revision.as_ref().map(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .any(|approved| approved == revision)
    });
    let current_matches_latest_approved = current_revision
        .as_ref()
        .zip(latest_approved_revision.as_ref())
        .map(|(current, latest)| current == latest);
    let status = if config
        .consumer_entry_session_auth_issuer_registry_approved_revisions_path
        .is_none()
    {
        "approval_not_configured"
    } else if approval_state.load_status != "loaded" {
        "approval_state_not_loaded"
    } else if current_revision.is_none() {
        "current_revision_missing"
    } else if current_revision_approved != Some(true) {
        "current_revision_not_approved"
    } else {
        "ok"
    };

    json!({
        "configured": config
            .consumer_entry_session_auth_issuer_registry_approved_revisions_path
            .is_some(),
        "required": config.consumer_entry_session_auth_issuer_registry_require_approved_revision,
        "status": status,
        "valid": status == "ok",
        "current_revision": current_revision,
        "current_revision_approved": current_revision_approved,
        "current_revision_index": current_revision_index,
        "latest_approved_revision": latest_approved_revision,
        "latest_approved_revision_index": latest_approved_revision_index,
        "current_matches_latest_approved": current_matches_latest_approved,
        "approved_revision_count": approval_state.approved_revisions.len(),
        "approval_state_loaded": approval_state.load_status == "loaded",
    })
}

fn consumer_entry_session_auth_issuer_registry_approval_source_json(
    config: &MatrixAdapterConfig,
    metadata: &SessionAuthIssuerRegistryMetadata,
    approval_state: &SessionAuthIssuerRegistryRevisionApprovalState,
    limit: usize,
) -> Value {
    let current_revision = consumer_entry_session_auth_issuer_registry_current_revision(metadata);
    let current_revision_index = current_revision.as_ref().and_then(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .position(|approved| approved == revision)
    });
    let latest_approved_revision = approval_state.approved_revisions.last().cloned();
    let latest_approved_revision_index = approval_state.approved_revisions.len().checked_sub(1);
    let revisions = approval_state
        .approved_revisions
        .iter()
        .enumerate()
        .rev()
        .take(limit)
        .map(|(index, revision)| {
            json!({
                "index": index,
                "revision": revision,
                "is_latest": Some(index) == latest_approved_revision_index,
                "is_current": current_revision
                    .as_ref()
                    .map(|current| current == revision)
                    .unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();
    let status = if config
        .consumer_entry_session_auth_issuer_registry_approved_revisions_path
        .is_none()
    {
        "approval_not_configured"
    } else if approval_state.load_status != "loaded" {
        "approval_state_not_loaded"
    } else if approval_state.approved_revisions.is_empty() {
        "approved_revision_set_empty"
    } else {
        "ok"
    };

    json!({
        "configured": config
            .consumer_entry_session_auth_issuer_registry_approved_revisions_path
            .is_some(),
        "required": config.consumer_entry_session_auth_issuer_registry_require_approved_revision,
        "status": status,
        "valid": status == "ok",
        "source_path": approval_state.source_path,
        "source_modified_epoch": approval_state.source_modified_epoch,
        "loaded_at_epoch": approval_state.loaded_at_epoch,
        "load_status": approval_state.load_status,
        "load_error": approval_state.load_error,
        "version": approval_state.version,
        "revision": approval_state.revision,
        "limit": limit,
        "returned_order": "latest_first",
        "approved_revision_count": approval_state.approved_revisions.len(),
        "returned_revision_count": revisions.len(),
        "latest_approved_revision": latest_approved_revision,
        "latest_approved_revision_index": latest_approved_revision_index,
        "current_revision": current_revision,
        "current_revision_index": current_revision_index,
        "current_revision_approved": current_revision_index.is_some(),
        "revisions": revisions,
    })
}

fn load_recent_event_cache(config: &MatrixAdapterConfig) -> RecentEventCache {
    let Some(path) = config.recent_event_store_path.as_deref() else {
        return RecentEventCache::default();
    };

    let Ok(raw) = std::fs::read_to_string(path) else {
        return RecentEventCache::default();
    };

    let Ok(mut cache) = serde_json::from_str::<RecentEventCache>(&raw) else {
        return RecentEventCache::default();
    };

    prune_recent_event_cache(
        &mut cache,
        Utc::now().timestamp(),
        config.recent_event_window_secs,
        config.recent_event_cache_size,
    );
    cache
}

fn persist_recent_event_cache(cache: &RecentEventCache, config: &MatrixAdapterConfig) {
    let Some(path) = config.recent_event_store_path.as_deref() else {
        return;
    };

    let parent = std::path::Path::new(path).parent().map(|p| p.to_path_buf());
    if let Some(parent) = parent {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(body) = serde_json::to_vec(cache) {
        let _ = std::fs::write(path, body);
    }
}

fn prune_rate_limit_entries(entries: &mut VecDeque<i64>, now_epoch: i64, window_secs: u64) -> bool {
    let mut modified = false;
    while entries
        .front()
        .copied()
        .map(|seen_at| now_epoch - seen_at >= window_secs as i64)
        .unwrap_or(false)
    {
        entries.pop_front();
        modified = true;
    }
    modified
}

fn prune_rate_limit_cache(cache: &mut RateLimitCache, now_epoch: i64, window_secs: u64) {
    cache.seen.retain(|_, entries| {
        prune_rate_limit_entries(entries, now_epoch, window_secs);
        !entries.is_empty()
    });
}

fn load_rate_limit_cache(config: &MatrixAdapterConfig) -> RateLimitCache {
    let Some(path) = config.rate_limit_store_path.as_deref() else {
        return RateLimitCache::default();
    };

    let Ok(raw) = std::fs::read_to_string(path) else {
        return RateLimitCache::default();
    };

    let Ok(mut cache) = serde_json::from_str::<RateLimitCache>(&raw) else {
        return RateLimitCache::default();
    };

    prune_rate_limit_cache(
        &mut cache,
        Utc::now().timestamp(),
        config.rate_limit_window_secs,
    );
    cache
}

fn persist_rate_limit_cache(cache: &RateLimitCache, config: &MatrixAdapterConfig) {
    let Some(path) = config.rate_limit_store_path.as_deref() else {
        return;
    };

    let parent = std::path::Path::new(path).parent().map(|p| p.to_path_buf());
    if let Some(parent) = parent {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(body) = serde_json::to_vec(cache) {
        let _ = std::fs::write(path, body);
    }
}

impl AppState {
    pub async fn from_env() -> Result<Self, String> {
        let config = MatrixAdapterConfig::from_env();
        if let Err(errors) = config.validate_runtime_profile() {
            return Err(format!(
                "invalid matrix-entry-adapter runtime profile ({}): {}",
                config.runtime_profile.as_str(),
                errors.join("; ")
            ));
        }
        Ok(Self::new(config))
    }

    pub fn new(config: MatrixAdapterConfig) -> Self {
        let recent_event_cache = load_recent_event_cache(&config);
        let rate_limit_cache = load_rate_limit_cache(&config);
        let consumer_entry_session_auth_issuer_registry_state =
            SessionAuthIssuerRegistryRuntimeState {
                metadata: config
                    .consumer_entry_session_auth_issuer_registry_metadata
                    .clone(),
                registry: config.consumer_entry_session_auth_issuer_registry.clone(),
            };
        Self {
            inner: Arc::new(AppStateInner {
                http: Client::new(),
                config,
                consumer_entry_session_auth_issuer_registry_state: StdRwLock::new(
                    consumer_entry_session_auth_issuer_registry_state,
                ),
                rate_limits: Mutex::new(rate_limit_cache),
                recent_event_cache: Mutex::new(recent_event_cache),
                metrics: MatrixEntryMetrics::default(),
            }),
        }
    }

    pub fn config(&self) -> &MatrixAdapterConfig {
        &self.inner.config
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route(
            "/v1/admin/consumer-entry-session-auth/status",
            get(get_consumer_entry_session_auth_status),
        )
        .route(
            "/v1/admin/consumer-entry-session-auth/validate",
            post(validate_consumer_entry_session_auth_runtime),
        )
        .route(
            "/v1/admin/consumer-entry-session-auth/reload",
            post(reload_consumer_entry_session_auth_runtime),
        )
        .route("/v1/matrix/events", post(handle_matrix_event))
        .route("/v1/matrix/tasks/:id/projection", get(get_task_projection))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    let consumer_entry_session_auth_runtime_state =
        consumer_entry_session_auth_issuer_registry_runtime_state(&state);
    let profile_errors = matrix_profile_validation_errors(
        state.config(),
        &consumer_entry_session_auth_runtime_state,
    );
    let (_, _, consumer_entry_session_auth_selection) =
        resolve_consumer_entry_session_auth_runtime_selection(
            state.config(),
            &consumer_entry_session_auth_runtime_state,
        );
    let rate_limits = state.inner.rate_limits.lock().await;
    let consumer_entry_session_auth_approval_state =
        load_consumer_entry_session_auth_issuer_registry_revision_approval_state(state.config());
    let consumer_entry_session_auth_approval_checks =
        consumer_entry_session_auth_issuer_registry_approval_checks_json(
            state.config(),
            &consumer_entry_session_auth_runtime_state.metadata,
            &consumer_entry_session_auth_approval_state,
        );
    let consumer_entry_session_auth_governance_overview =
        consumer_entry_session_auth_governance_overview_json(
            state.config(),
            &consumer_entry_session_auth_runtime_state.metadata,
            &consumer_entry_session_auth_selection,
            &consumer_entry_session_auth_approval_state,
            5,
        );
    let consumer_entry_session_auth_governance_valid =
        consumer_entry_session_auth_governance_overview
            .get("valid")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    Json(json!({
        "status": "ok",
        "service": "matrix-entry-adapter",
        "runtime_profile": state.config().runtime_profile.as_str(),
        "profile_validation": {
            "ok": profile_errors.is_empty(),
            "errors": profile_errors,
            "checks": {
                "ingress_token_present": state.config().ingress_token.is_some(),
                "consumer_entry_ingress_token_present": state.config().consumer_entry_ingress_token.is_some(),
                "consumer_entry_session_auth_secret_present": consumer_entry_session_auth_governance_overview.get("selection").and_then(|value| value.get("status")).and_then(Value::as_str) == Some("ok"),
                "consumer_entry_session_auth_key_id_configured": consumer_entry_session_auth_selection.key_id.is_some(),
                "consumer_entry_session_auth_selection_ok": consumer_entry_session_auth_selection.status == "ok",
                "consumer_entry_session_auth_selected_from_registry": consumer_entry_session_auth_selection.source == "issuer_registry" && consumer_entry_session_auth_selection.status == "ok",
                "consumer_entry_session_auth_issuer_registry_configured": state.config().consumer_entry_session_auth_issuer_registry_path.is_some(),
                "consumer_entry_session_auth_issuer_registry_loaded": consumer_entry_session_auth_runtime_state.metadata.load_status == "loaded",
                "consumer_entry_session_auth_issuer_registry_revision_present": consumer_entry_session_auth_runtime_state.metadata.revision.is_some(),
                "consumer_entry_session_auth_issuer_registry_approval_configured": state.config().consumer_entry_session_auth_issuer_registry_approved_revisions_path.is_some(),
                "consumer_entry_session_auth_issuer_registry_approval_required": state.config().consumer_entry_session_auth_issuer_registry_require_approved_revision,
                "consumer_entry_session_auth_issuer_registry_approval_loaded": consumer_entry_session_auth_approval_state.load_status == "loaded",
                "consumer_entry_session_auth_issuer_registry_revision_approved": consumer_entry_session_auth_approval_checks.get("current_revision_approved").and_then(Value::as_bool),
                "consumer_entry_session_auth_governance_valid": consumer_entry_session_auth_governance_valid,
                "consumer_entry_session_auth_issuer_registry_issuer_count": consumer_entry_session_auth_runtime_state.metadata.issuer_count,
                "consumer_entry_session_auth_issuer_registry_key_count": consumer_entry_session_auth_runtime_state.metadata.key_count,
                "recent_event_store_configured": state.config().recent_event_store_path.is_some(),
                "rate_limit_store_configured": state.config().rate_limit_store_path.is_some(),
                "bot_user_id_is_non_default": state.config().bot_user_id.trim() != "@cex-bot:local.dev",
            }
        },
        "consumer_entry_base_url": state.config().consumer_entry_base_url,
        "bot_user_id": state.config().bot_user_id,
        "ingress_protected": state.config().ingress_token.is_some(),
        "consumer_entry_protected": state.config().consumer_entry_ingress_token.is_some(),
        "consumer_entry_session_auth": {
            "secret_present": consumer_entry_session_auth_selection.status == "ok",
            "key_id": consumer_entry_session_auth_selection.key_id.clone(),
            "issuer_registry_path": state.config().consumer_entry_session_auth_issuer_registry_path.clone(),
            "issuer_registry_loaded": consumer_entry_session_auth_runtime_state.metadata.load_status == "loaded",
            "issuer_registry_load_error": consumer_entry_session_auth_runtime_state.metadata.load_error.clone(),
            "issuer_registry_issuer_count": consumer_entry_session_auth_runtime_state.metadata.issuer_count,
            "issuer_registry_key_count": consumer_entry_session_auth_runtime_state.metadata.key_count,
            "issuer_registry_issuers": consumer_entry_session_auth_runtime_state.registry.keys().cloned().collect::<Vec<_>>(),
            "issuer_registry_metadata": consumer_entry_session_auth_runtime_state.metadata.clone(),
            "issuer_registry_approved_revisions_path": state.config().consumer_entry_session_auth_issuer_registry_approved_revisions_path.clone(),
            "issuer_registry_approval_required": state.config().consumer_entry_session_auth_issuer_registry_require_approved_revision,
            "issuer_registry_approval_state": consumer_entry_session_auth_approval_state,
            "issuer_registry_approval_checks": consumer_entry_session_auth_approval_checks,
            "selection": consumer_entry_session_auth_selection,
            "issuer": state.config().consumer_entry_session_auth_issuer.clone(),
            "audience": state.config().consumer_entry_session_auth_audience.clone(),
            "ttl_secs": state.config().consumer_entry_session_auth_ttl_secs,
            "assertion_header": USER_SESSION_ASSERTION_HEADER,
            "signature_header": USER_SESSION_SIGNATURE_HEADER,
        },
        "consumer_entry_session_auth_governance_overview": consumer_entry_session_auth_governance_overview,
        "max_text_chars": state.config().max_text_chars,
        "rate_limit_window_secs": state.config().rate_limit_window_secs,
        "rate_limit_max_requests": state.config().rate_limit_max_requests,
        "rate_limit_store_path": state.config().rate_limit_store_path,
        "rate_limit_store_enabled": state.config().rate_limit_store_path.is_some(),
        "rate_limit_bucket_count": rate_limits.seen.len(),
        "recent_event_window_secs": state.config().recent_event_window_secs,
        "recent_event_cache_size": state.config().recent_event_cache_size,
        "recent_event_store_path": state.config().recent_event_store_path,
        "recent_event_store_enabled": state.config().recent_event_store_path.is_some(),
        "metrics": state.inner.metrics.snapshot(),
    }))
}

async fn metrics(State(state): State<AppState>) -> Response {
    let consumer_entry_session_auth_runtime_state =
        consumer_entry_session_auth_issuer_registry_runtime_state(&state);
    let (_, _, consumer_entry_session_auth_selection) =
        resolve_consumer_entry_session_auth_runtime_selection(
            state.config(),
            &consumer_entry_session_auth_runtime_state,
        );
    let profile_ok = if matrix_profile_validation_errors(
        state.config(),
        &consumer_entry_session_auth_runtime_state,
    )
    .is_empty()
    {
        1
    } else {
        0
    };
    let consumer_entry_session_auth_approval_state =
        load_consumer_entry_session_auth_issuer_registry_revision_approval_state(state.config());
    let consumer_entry_session_auth_approval_checks =
        consumer_entry_session_auth_issuer_registry_approval_checks_json(
            state.config(),
            &consumer_entry_session_auth_runtime_state.metadata,
            &consumer_entry_session_auth_approval_state,
        );
    let consumer_entry_session_auth_governance_overview =
        consumer_entry_session_auth_governance_overview_json(
            state.config(),
            &consumer_entry_session_auth_runtime_state.metadata,
            &consumer_entry_session_auth_selection,
            &consumer_entry_session_auth_approval_state,
            5,
        );
    let consumer_entry_session_auth_governance_valid =
        consumer_entry_session_auth_governance_overview
            .get("valid")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let rate_limit_bucket_count = {
        let rate_limits = state.inner.rate_limits.lock().await;
        rate_limits.seen.len()
    };
    let body = format!(
        concat!(
            "# TYPE cex_matrix_entry_event_requests_total counter\n",
            "cex_matrix_entry_event_requests_total {}\n",
            "# TYPE cex_matrix_entry_projection_requests_total counter\n",
            "cex_matrix_entry_projection_requests_total {}\n",
            "# TYPE cex_matrix_entry_duplicate_events_total counter\n",
            "cex_matrix_entry_duplicate_events_total {}\n",
            "# TYPE cex_matrix_entry_rate_limited_requests_total counter\n",
            "cex_matrix_entry_rate_limited_requests_total {}\n",
            "# TYPE cex_matrix_entry_ingress_auth_failures_total counter\n",
            "cex_matrix_entry_ingress_auth_failures_total {}\n",
            "# TYPE cex_matrix_entry_ignored_self_events_total counter\n",
            "cex_matrix_entry_ignored_self_events_total {}\n",
            "# TYPE cex_matrix_entry_profile_validation_ok gauge\n",
            "cex_matrix_entry_profile_validation_ok {}\n",
            "# TYPE cex_matrix_entry_ingress_protected gauge\n",
            "cex_matrix_entry_ingress_protected {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_protected gauge\n",
            "cex_matrix_entry_consumer_entry_protected {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_configured gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_configured {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_key_id_configured gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_key_id_configured {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_selection_ok gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_selection_ok {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_governance_valid gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_governance_valid {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_selected_from_registry gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_selected_from_registry {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_issuer_registry_configured gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_issuer_registry_configured {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_issuer_registry_loaded gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_issuer_registry_loaded {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_issuer_registry_revision_present gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_issuer_registry_revision_present {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_issuer_registry_approval_configured gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_issuer_registry_approval_configured {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_issuer_registry_approval_required gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_issuer_registry_approval_required {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_issuer_registry_approval_loaded gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_issuer_registry_approval_loaded {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_issuer_registry_revision_approved gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_issuer_registry_revision_approved {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_approval_source_valid gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_approval_source_valid {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_approval_coverage_valid gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_approval_coverage_valid {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_issuer_registry_issuers gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_issuer_registry_issuers {}\n",
            "# TYPE cex_matrix_entry_consumer_entry_session_auth_issuer_registry_keys gauge\n",
            "cex_matrix_entry_consumer_entry_session_auth_issuer_registry_keys {}\n",
            "# TYPE cex_matrix_entry_rate_limit_store_enabled gauge\n",
            "cex_matrix_entry_rate_limit_store_enabled {}\n",
            "# TYPE cex_matrix_entry_rate_limit_bucket_count gauge\n",
            "cex_matrix_entry_rate_limit_bucket_count {}\n"
        ),
        state.inner.metrics.event_requests.load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .projection_requests
            .load(Ordering::Relaxed),
        state.inner.metrics.duplicate_events.load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .ingress_auth_failures
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .ignored_self_events
            .load(Ordering::Relaxed),
        profile_ok,
        if state.config().ingress_token.is_some() {
            1
        } else {
            0
        },
        if state.config().consumer_entry_ingress_token.is_some() {
            1
        } else {
            0
        },
        if consumer_entry_session_auth_selection.status == "ok" {
            1
        } else {
            0
        },
        if consumer_entry_session_auth_selection.key_id.is_some() {
            1
        } else {
            0
        },
        if consumer_entry_session_auth_selection.status == "ok" {
            1
        } else {
            0
        },
        if consumer_entry_session_auth_governance_valid {
            1
        } else {
            0
        },
        if consumer_entry_session_auth_selection.source == "issuer_registry"
            && consumer_entry_session_auth_selection.status == "ok"
        {
            1
        } else {
            0
        },
        if state.config().consumer_entry_session_auth_issuer_registry_path.is_some() {
            1
        } else {
            0
        },
        if consumer_entry_session_auth_runtime_state.metadata.load_status == "loaded" {
            1
        } else {
            0
        },
        if consumer_entry_session_auth_runtime_state
            .metadata
            .revision
            .is_some()
        {
            1
        } else {
            0
        },
        if state
            .config()
            .consumer_entry_session_auth_issuer_registry_approved_revisions_path
            .is_some()
        {
            1
        } else {
            0
        },
        if state
            .config()
            .consumer_entry_session_auth_issuer_registry_require_approved_revision
        {
            1
        } else {
            0
        },
        if consumer_entry_session_auth_approval_state.load_status == "loaded" {
            1
        } else {
            0
        },
        if consumer_entry_session_auth_approval_checks
            .get("current_revision_approved")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if consumer_entry_session_auth_governance_overview
            .get("checks")
            .and_then(|value| value.get("approval_source_valid"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if consumer_entry_session_auth_governance_overview
            .get("checks")
            .and_then(|value| value.get("approval_coverage_valid"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        consumer_entry_session_auth_runtime_state.metadata.issuer_count,
        consumer_entry_session_auth_runtime_state.metadata.key_count,
        if state.config().rate_limit_store_path.is_some() {
            1
        } else {
            0
        },
        rate_limit_bucket_count,
    );
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}

async fn get_consumer_entry_session_auth_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        return response;
    }

    let runtime_state = consumer_entry_session_auth_issuer_registry_runtime_state(&state);
    let (_, _, selection) =
        resolve_consumer_entry_session_auth_runtime_selection(state.config(), &runtime_state);
    let approval_state =
        load_consumer_entry_session_auth_issuer_registry_revision_approval_state(state.config());
    let approval_checks = consumer_entry_session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &runtime_state.metadata,
        &approval_state,
    );
    let approval_source = consumer_entry_session_auth_issuer_registry_approval_source_json(
        state.config(),
        &runtime_state.metadata,
        &approval_state,
        20,
    );
    let governance = consumer_entry_session_auth_governance_overview_json(
        state.config(),
        &runtime_state.metadata,
        &selection,
        &approval_state,
        20,
    );

    Json(json!({
        "status": "ok",
        "consumer_entry_session_auth": {
            "issuer": state.config().consumer_entry_session_auth_issuer,
            "audience": state.config().consumer_entry_session_auth_audience,
            "selection": selection,
            "issuer_registry_path": state.config().consumer_entry_session_auth_issuer_registry_path,
            "issuer_registry_metadata": runtime_state.metadata.clone(),
            "issuer_registry_issuers": runtime_state.registry.keys().cloned().collect::<Vec<_>>(),
            "issuer_registry_approval": approval_checks,
            "issuer_registry_approval_source": approval_source,
        },
        "consumer_entry_session_auth_governance_overview": governance,
    }))
    .into_response()
}

async fn validate_consumer_entry_session_auth_runtime(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        return response;
    }

    let Some(path) = state
        .config()
        .consumer_entry_session_auth_issuer_registry_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "status": "issuer_registry_path_not_configured",
                "valid": false,
                "error": "consumer-entry session auth issuer registry path is not configured"
            })),
        )
            .into_response();
    };

    let current_state = consumer_entry_session_auth_issuer_registry_runtime_state(&state);
    let (_, _, current_selection) =
        resolve_consumer_entry_session_auth_runtime_selection(state.config(), &current_state);
    let (candidate_metadata, candidate_registry) = load_session_auth_issuer_registry(Some(path));
    let candidate_state = SessionAuthIssuerRegistryRuntimeState {
        metadata: candidate_metadata,
        registry: candidate_registry,
    };
    let (_, _, candidate_selection) =
        resolve_consumer_entry_session_auth_runtime_selection(state.config(), &candidate_state);
    let approval_state =
        load_consumer_entry_session_auth_issuer_registry_revision_approval_state(state.config());
    let approval_checks = consumer_entry_session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &candidate_state.metadata,
        &approval_state,
    );
    let approval_source = consumer_entry_session_auth_issuer_registry_approval_source_json(
        state.config(),
        &candidate_state.metadata,
        &approval_state,
        20,
    );
    let governance = consumer_entry_session_auth_governance_overview_json(
        state.config(),
        &candidate_state.metadata,
        &candidate_selection,
        &approval_state,
        20,
    );
    let governance_valid = governance
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let status = if candidate_state.metadata.load_status != "loaded" {
        candidate_state.metadata.load_status.clone()
    } else {
        governance
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("validation_failed")
            .to_string()
    };
    let valid = candidate_state.metadata.load_status == "loaded" && governance_valid;

    let response_status = if valid {
        StatusCode::OK
    } else {
        StatusCode::CONFLICT
    };

    (
        response_status,
        Json(json!({
            "status": status,
            "valid": valid,
            "matches_loaded_revision": current_state.metadata.revision == candidate_state.metadata.revision,
            "matches_loaded_selection": current_selection == candidate_selection,
            "matches_loaded_status": current_state.metadata.load_status == candidate_state.metadata.load_status,
            "consumer_entry_session_auth_runtime": {
                "metadata": candidate_state.metadata.clone(),
                "selection": candidate_selection,
                "approval": approval_checks,
                "approval_source": approval_source,
                "governance": governance,
            },
            "live_consumer_entry_session_auth_runtime": {
                "metadata": current_state.metadata.clone(),
                "selection": current_selection,
            }
        })),
    )
        .into_response()
}

async fn reload_consumer_entry_session_auth_runtime(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        return response;
    }

    let Some(path) = state
        .config()
        .consumer_entry_session_auth_issuer_registry_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "status": "issuer_registry_path_not_configured",
                "reloaded": false,
                "error": "consumer-entry session auth issuer registry path is not configured"
            })),
        )
            .into_response();
    };

    let current_state = consumer_entry_session_auth_issuer_registry_runtime_state(&state);
    let (_, _, current_selection) =
        resolve_consumer_entry_session_auth_runtime_selection(state.config(), &current_state);
    let (candidate_metadata, candidate_registry) = load_session_auth_issuer_registry(Some(path));
    let candidate_state = SessionAuthIssuerRegistryRuntimeState {
        metadata: candidate_metadata,
        registry: candidate_registry,
    };
    let (_, _, candidate_selection) =
        resolve_consumer_entry_session_auth_runtime_selection(state.config(), &candidate_state);
    let approval_state =
        load_consumer_entry_session_auth_issuer_registry_revision_approval_state(state.config());
    let approval_checks = consumer_entry_session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &candidate_state.metadata,
        &approval_state,
    );
    let approval_source = consumer_entry_session_auth_issuer_registry_approval_source_json(
        state.config(),
        &candidate_state.metadata,
        &approval_state,
        20,
    );
    let governance = consumer_entry_session_auth_governance_overview_json(
        state.config(),
        &candidate_state.metadata,
        &candidate_selection,
        &approval_state,
        20,
    );
    let governance_valid = governance
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let valid = candidate_state.metadata.load_status == "loaded" && governance_valid;

    if !valid {
        let status = if candidate_state.metadata.load_status != "loaded" {
            candidate_state.metadata.load_status.clone()
        } else {
            governance
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("reload_rejected")
                .to_string()
        };
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "status": status,
                "reloaded": false,
                "consumer_entry_session_auth_runtime": {
                    "metadata": candidate_state.metadata.clone(),
                    "selection": candidate_selection,
                    "approval": approval_checks,
                    "approval_source": approval_source,
                    "governance": governance,
                },
                "live_consumer_entry_session_auth_runtime": {
                    "metadata": current_state.metadata.clone(),
                    "selection": current_selection,
                }
            })),
        )
            .into_response();
    }

    {
        let mut guard = state
            .inner
            .consumer_entry_session_auth_issuer_registry_state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = candidate_state;
    }

    let live_state = consumer_entry_session_auth_issuer_registry_runtime_state(&state);
    let (_, _, live_selection) =
        resolve_consumer_entry_session_auth_runtime_selection(state.config(), &live_state);

    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "reloaded": true,
            "previous_revision": current_state.metadata.revision,
            "consumer_entry_session_auth_runtime": {
                "metadata": live_state.metadata,
                "selection": live_selection,
                "approval": approval_checks,
                "approval_source": approval_source,
                "governance": governance,
            }
        })),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct MatrixEventEnvelope {
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub room_id: String,
    pub sender: String,
    pub text: Option<String>,
    pub content: Option<Value>,
    pub timestamp_ms: Option<i64>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct MatrixAdapterResponse {
    pub accepted: bool,
    pub action: String,
    pub event_id: Option<String>,
    pub room_id: String,
    pub sender: String,
    pub forwarded: Option<Value>,
    pub projected_reply: Option<Value>,
}

#[derive(Debug, PartialEq)]
enum ParsedCommand {
    Raw {
        text: String,
    },
    Help,
    ClientApp,
    ClientFeed {
        filter: Option<String>,
    },
    ClientSocial,
    ClientDuel {
        opponent: Option<String>,
        body: String,
    },
    League,
    Arena,
    Quest,
    World,
    WorldMap,
    WorldMapMove {
        target: String,
    },
    WorldAction {
        body: String,
    },
    WorldAssets,
    WorldAssetUpgrade {
        asset_id: String,
        body: String,
    },
    WorldCompanies,
    WorldCompanyCreate {
        asset_id: String,
        body: String,
    },
    WorldShops,
    WorldListingCreate {
        company_id: String,
        body: String,
    },
    WorldListingBuy {
        listing_id: String,
        body: String,
    },
    WorldWork,
    WorldWorkDeliver {
        work_order_id: String,
        body: String,
    },
    WorldWorkAccept {
        work_order_id: String,
        body: String,
    },
    WorldWorkReject {
        work_order_id: String,
        body: String,
    },
    WorldWorkReopen {
        work_order_id: String,
        body: String,
    },
    WorldWorkCancel {
        work_order_id: String,
        body: String,
    },
    WorldFactions,
    CraftAction {
        body: String,
    },
    WorldContract {
        body: String,
    },
    WorldContractComplete {
        contract_id: String,
        body: String,
    },
    Season,
    Raid {
        match_id: Option<String>,
        body: Option<String>,
    },
    Team {
        match_id: Option<String>,
        role: Option<String>,
    },
    Rank,
    Loadout,
    Progression,
    Skills,
    Tools,
    Skins,
    Profile,
    Rewards,
    Inventory,
    History,
    Guild {
        guild_id: Option<String>,
    },
    Draft {
        heroes: Vec<String>,
    },
    Join {
        match_id: String,
    },
    Battle {
        match_id: String,
        text: String,
    },
    Submit {
        match_id: String,
        body: String,
    },
    Wallet,
    Plans,
    Status {
        task_id: String,
    },
    Task {
        text: String,
        capability_id: Option<String>,
        account_id: Option<String>,
    },
    Unsupported {
        text: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserSessionAuthClaims {
    version: u32,
    issuer: String,
    key_id: Option<String>,
    subject: String,
    source_kind: String,
    audience: Option<String>,
    request_fingerprint: Option<String>,
    room_id: Option<String>,
    session_id: Option<String>,
    org_id: Option<String>,
    account_id: Option<String>,
    issued_at_epoch: i64,
    expires_at_epoch: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SessionAuthIssuerRegistryDocument {
    #[serde(default = "default_session_auth_issuer_registry_version")]
    version: u32,
    revision: Option<String>,
    #[serde(default)]
    issuers: HashMap<String, SessionAuthIssuerRegistryIssuer>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SessionAuthIssuerRegistryRevisionApprovalDocument {
    #[serde(default = "default_session_auth_issuer_registry_version")]
    version: u32,
    revision: Option<String>,
    #[serde(default)]
    approved_revisions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionAuthIssuerRegistryMetadata {
    version: u32,
    revision: Option<String>,
    source_path: Option<String>,
    source_modified_epoch: Option<i64>,
    loaded_at_epoch: Option<i64>,
    load_status: String,
    load_error: Option<String>,
    issuer_count: usize,
    key_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionAuthIssuerRegistrySelection {
    source: String,
    status: String,
    issuer: String,
    key_id: Option<String>,
    registry_path: Option<String>,
    detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SessionAuthIssuerRegistryRevisionApprovalState {
    source_path: Option<String>,
    source_modified_epoch: Option<i64>,
    loaded_at_epoch: Option<i64>,
    load_status: String,
    load_error: Option<String>,
    version: u32,
    revision: Option<String>,
    approved_revisions: Vec<String>,
}

impl Default for SessionAuthIssuerRegistrySelection {
    fn default() -> Self {
        Self {
            source: "none".to_string(),
            status: "unconfigured".to_string(),
            issuer: "matrix-entry-adapter".to_string(),
            key_id: None,
            registry_path: None,
            detail: None,
        }
    }
}

impl Default for SessionAuthIssuerRegistryMetadata {
    fn default() -> Self {
        Self {
            version: 0,
            revision: None,
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: None,
            load_status: "disabled".to_string(),
            load_error: None,
            issuer_count: 0,
            key_count: 0,
        }
    }
}

impl Default for SessionAuthIssuerRegistryRevisionApprovalState {
    fn default() -> Self {
        Self {
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: None,
            load_status: "disabled".to_string(),
            load_error: None,
            version: 0,
            revision: None,
            approved_revisions: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionAuthIssuerRegistryIssuer {
    #[serde(default, alias = "activeKeyId")]
    active_key_id: Option<String>,
    #[serde(default)]
    keys: HashMap<String, String>,
}

async fn handle_matrix_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(event): Json<MatrixEventEnvelope>,
) -> Response {
    state.inner.metrics.inc_event_requests();

    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    if event.sender == state.config().bot_user_id {
        state.inner.metrics.inc_ignored_self_events();
        return (
            StatusCode::OK,
            Json(MatrixAdapterResponse {
                accepted: false,
                action: "ignored_self_event".to_string(),
                event_id: event.event_id,
                room_id: event.room_id,
                sender: event.sender,
                forwarded: None,
                projected_reply: None,
            }),
        )
            .into_response();
    }

    if let Some(response) = reject_duplicate_event(&state, &event).await {
        return response;
    }

    if let Err(response) = enforce_rate_limit(
        &state,
        build_matrix_event_rate_limit_key(&event),
        "matrix_entry_rate_limited",
    )
    .await
    {
        return response;
    }

    let text = event
        .text
        .clone()
        .or_else(|| extract_matrix_body(event.content.as_ref()))
        .unwrap_or_default();

    if text.trim().is_empty() {
        return (
            StatusCode::OK,
            Json(MatrixAdapterResponse {
                accepted: false,
                action: "ignored_non_text_event".to_string(),
                event_id: event.event_id,
                room_id: event.room_id,
                sender: event.sender,
                forwarded: None,
                projected_reply: None,
            }),
        )
            .into_response();
    }

    let text = match validate_text_payload(&text, state.config().max_text_chars) {
        Ok(text) => text,
        Err(response) => return response,
    };

    let command = parse_matrix_command(&text);
    match command {
        ParsedCommand::Raw { text } => {
            let request_body = build_task_request_body(
                &event.sender,
                &event.room_id,
                &event.event_id,
                &text,
                None,
                None,
                &event.event_type,
                event.timestamp_ms,
                &event.metadata,
                event.content,
            );
            forward_to_consumer_entry(state, request_body).await
        }
        ParsedCommand::Task {
            text,
            capability_id,
            account_id,
        } => {
            let request_body = build_task_request_body(
                &event.sender,
                &event.room_id,
                &event.event_id,
                &text,
                capability_id,
                account_id,
                &event.event_type,
                event.timestamp_ms,
                &event.metadata,
                event.content,
            );
            forward_to_consumer_entry(state, request_body).await
        }
        ParsedCommand::Status { task_id } => match fetch_task_projection(&state, &task_id).await {
            Ok(value) => {
                let projected_reply = value.get("projected_reply").cloned().or_else(|| {
                    value
                        .get("forwarded")
                        .map(build_projected_matrix_reply)
                        .or_else(|| Some(build_plain_matrix_reply("任务状态查询失败。")))
                });
                (
                    StatusCode::OK,
                    Json(MatrixAdapterResponse {
                        accepted: true,
                        action: "status_lookup".to_string(),
                        event_id: Some(task_id.clone()),
                        room_id: event.room_id,
                        sender: event.sender,
                        forwarded: Some(value),
                        projected_reply,
                    }),
                )
                    .into_response()
            }
            Err(response) => response,
        },
        ParsedCommand::Help => {
            let help_reply = build_help_matrix_reply();
            (
                StatusCode::OK,
                Json(MatrixAdapterResponse {
                    accepted: false,
                    action: "help".to_string(),
                    event_id: event.event_id,
                    room_id: event.room_id,
                    sender: event.sender,
                    forwarded: None,
                    projected_reply: Some(help_reply),
                }),
            )
                .into_response()
        }
        ParsedCommand::ClientApp => {
            let path = format!("/v1/client/app/{}", url_encode_component(&event.sender));
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "trillionnium_client_app",
                    event,
                    value.clone(),
                    build_trillionnium_client_app_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::ClientFeed { filter } => {
            let path = format!("/v1/client/feed/{}", url_encode_component(&event.sender));
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "trillionnium_client_feed",
                    event,
                    value.clone(),
                    build_trillionnium_client_feed_matrix_reply(&value, filter.as_deref()),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::ClientSocial => {
            let path = format!("/v1/client/app/{}", url_encode_component(&event.sender));
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "trillionnium_client_social",
                    event,
                    value.clone(),
                    build_trillionnium_client_social_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::ClientDuel { opponent, body } => {
            let opponent = opponent.unwrap_or_else(|| "nearby".to_string());
            let message = format!("face-to-face duel vs {opponent}: {body}");
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "message": message,
                "event_id": &event.event_id,
            });
            match fetch_consumer_entry_post(
                &state,
                "/v1/league/matches/face-duel-001/battle",
                request,
            )
            .await
            {
                Ok(value) => league_response(
                    "trillionnium_client_duel",
                    event,
                    value.clone(),
                    build_trillionnium_client_duel_matrix_reply(&value, &opponent),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::League => match fetch_consumer_entry_get(&state, "/v1/league/home").await {
            Ok(value) => league_response(
                "league_home",
                event,
                value.clone(),
                build_league_home_matrix_reply(Some(&value)),
            ),
            Err(response) => response,
        },
        ParsedCommand::Arena => {
            match fetch_consumer_entry_get(&state, "/v1/league/matches").await {
                Ok(value) => league_response(
                    "league_arena",
                    event,
                    value.clone(),
                    build_league_arena_matrix_reply(Some(&value)),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Quest => {
            match fetch_consumer_entry_get(&state, "/v1/league/matches").await {
                Ok(value) => league_response(
                    "league_quest",
                    event,
                    value.clone(),
                    build_league_quest_matrix_reply(Some(&value)),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::World => match fetch_consumer_entry_get(&state, "/v1/world/home").await {
            Ok(value) => league_response(
                "trillionnium_world",
                event,
                value.clone(),
                build_trillionnium_world_matrix_reply(&value),
            ),
            Err(response) => response,
        },
        ParsedCommand::WorldMap => {
            let path = format!("/v1/world/map/{}", url_encode_component(&event.sender));
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "trillionnium_world_map",
                    event,
                    value.clone(),
                    build_trillionnium_world_map_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldMapMove { target } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "target": target,
            });
            match fetch_consumer_entry_post(&state, "/v1/world/map/move", request).await {
                Ok(value) => league_response(
                    "trillionnium_world_map_move",
                    event,
                    value.clone(),
                    build_trillionnium_world_map_move_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldAction { body } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "message": &body,
                "body": &body,
            });
            match fetch_consumer_entry_post(&state, "/v1/world/action", request).await {
                Ok(value) => league_response(
                    "trillionnium_world_action",
                    event,
                    value.clone(),
                    build_trillionnium_world_action_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldAssets => {
            match fetch_consumer_entry_get(&state, "/v1/world/assets").await {
                Ok(value) => league_response(
                    "trillionnium_world_assets",
                    event,
                    value.clone(),
                    build_trillionnium_world_assets_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldAssetUpgrade { asset_id, body } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "body": body,
            });
            let path = format!("/v1/world/assets/{asset_id}/upgrade");
            match fetch_consumer_entry_post(&state, &path, request).await {
                Ok(value) => league_response(
                    "trillionnium_world_asset_upgrade",
                    event,
                    value.clone(),
                    build_trillionnium_world_asset_upgrade_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldCompanies => {
            match fetch_consumer_entry_get(&state, "/v1/world/companies").await {
                Ok(value) => league_response(
                    "trillionnium_world_companies",
                    event,
                    value.clone(),
                    build_trillionnium_world_companies_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldCompanyCreate { asset_id, body } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "asset_id": asset_id,
                "body": body,
            });
            match fetch_consumer_entry_post(&state, "/v1/world/companies", request).await {
                Ok(value) => league_response(
                    "trillionnium_world_company_created",
                    event,
                    value.clone(),
                    build_trillionnium_world_company_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldShops => {
            match fetch_consumer_entry_get(&state, "/v1/world/shops").await {
                Ok(value) => league_response(
                    "trillionnium_world_shops",
                    event,
                    value.clone(),
                    build_trillionnium_world_shops_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldListingCreate { company_id, body } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "company_id": company_id,
                "body": body,
            });
            match fetch_consumer_entry_post(&state, "/v1/world/listings", request).await {
                Ok(value) => league_response(
                    "trillionnium_world_listing_created",
                    event,
                    value.clone(),
                    build_trillionnium_world_listing_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldListingBuy { listing_id, body } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "body": body,
            });
            let path = format!("/v1/world/listings/{listing_id}/buy");
            match fetch_consumer_entry_post(&state, &path, request).await {
                Ok(value) => league_response(
                    "trillionnium_world_listing_purchase",
                    event,
                    value.clone(),
                    build_trillionnium_world_purchase_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldWork => {
            match fetch_consumer_entry_get(&state, "/v1/world/commerce").await {
                Ok(value) => league_response(
                    "trillionnium_world_commerce",
                    event,
                    value.clone(),
                    build_trillionnium_world_commerce_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldWorkDeliver {
            work_order_id,
            body,
        } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "body": body,
            });
            let path = format!("/v1/world/work-orders/{work_order_id}/deliver");
            match fetch_consumer_entry_post(&state, &path, request).await {
                Ok(value) => league_response(
                    "trillionnium_world_work_delivery",
                    event,
                    value.clone(),
                    build_trillionnium_world_work_delivery_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldWorkAccept {
            work_order_id,
            body,
        } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "body": body,
            });
            let path = format!("/v1/world/work-orders/{work_order_id}/accept");
            match fetch_consumer_entry_post(&state, &path, request).await {
                Ok(value) => league_response(
                    "trillionnium_world_work_acceptance",
                    event,
                    value.clone(),
                    build_trillionnium_world_work_acceptance_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldWorkReject {
            work_order_id,
            body,
        } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "body": body,
            });
            let path = format!("/v1/world/work-orders/{work_order_id}/reject");
            match fetch_consumer_entry_post(&state, &path, request).await {
                Ok(value) => league_response(
                    "trillionnium_world_work_rejection",
                    event,
                    value.clone(),
                    build_trillionnium_world_work_rejection_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldWorkReopen {
            work_order_id,
            body,
        } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "body": body,
            });
            let path = format!("/v1/world/work-orders/{work_order_id}/reopen");
            match fetch_consumer_entry_post(&state, &path, request).await {
                Ok(value) => league_response(
                    "trillionnium_world_work_reopen",
                    event,
                    value.clone(),
                    build_trillionnium_world_work_reopen_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldWorkCancel {
            work_order_id,
            body,
        } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "body": body,
            });
            let path = format!("/v1/world/work-orders/{work_order_id}/cancel");
            match fetch_consumer_entry_post(&state, &path, request).await {
                Ok(value) => league_response(
                    "trillionnium_world_work_cancellation",
                    event,
                    value.clone(),
                    build_trillionnium_world_work_cancellation_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldFactions => {
            match fetch_consumer_entry_get(&state, "/v1/world/factions").await {
                Ok(value) => league_response(
                    "trillionnium_world_factions",
                    event,
                    value.clone(),
                    build_trillionnium_world_factions_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::CraftAction { body } => {
            let message = format!("craft build {}", body);
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "event_id": &event.event_id,
                "location_id": "starter-studio",
                "message": &message,
                "body": &message,
            });
            match fetch_consumer_entry_post(&state, "/v1/world/action", request).await {
                Ok(value) => league_response(
                    "trillionnium_craft_action",
                    event,
                    value.clone(),
                    build_trillionnium_craft_action_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldContract { body } => {
            let message = format!("contract task {}", body);
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "event_id": &event.event_id,
                "location_id": "zbj-market-gate",
                "message": &message,
                "body": &message,
            });
            match fetch_consumer_entry_post(&state, "/v1/world/action", request).await {
                Ok(value) => league_response(
                    "trillionnium_world_contract",
                    event,
                    value.clone(),
                    build_trillionnium_world_contract_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::WorldContractComplete { contract_id, body } => {
            let request = json!({
                "matrix_user_id": &event.sender,
                "room_id": &event.room_id,
                "body": body,
            });
            let path = format!("/v1/world/contracts/{contract_id}/complete");
            match fetch_consumer_entry_post(&state, &path, request).await {
                Ok(value) => league_response(
                    "trillionnium_world_contract_completion",
                    event,
                    value.clone(),
                    build_trillionnium_world_contract_completion_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Season => {
            match fetch_consumer_entry_get(&state, "/v1/league/season").await {
                Ok(value) => league_response(
                    "league_season",
                    event,
                    value.clone(),
                    build_league_season_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Raid { match_id, body } => {
            if let Some(match_id) = match_id {
                let path = format!(
                    "/v1/league/raids/{}/contribute",
                    url_encode_component(&match_id)
                );
                let body = json!({
                    "matrix_user_id": event.sender,
                    "room_id": event.room_id,
                    "role": "raider",
                    "body": body.unwrap_or_else(|| "raid contribution: scout evidence, assign builders, define risk gate".to_string()),
                });
                match fetch_consumer_entry_post(&state, &path, body).await {
                    Ok(value) => league_response(
                        "league_raid_contribution",
                        event,
                        value.clone(),
                        build_league_raid_contribution_matrix_reply(&value),
                    ),
                    Err(response) => response,
                }
            } else {
                match fetch_consumer_entry_get(&state, "/v1/league/raids").await {
                    Ok(value) => league_response(
                        "league_raids",
                        event,
                        value.clone(),
                        build_league_raids_matrix_reply(&value),
                    ),
                    Err(response) => response,
                }
            }
        }
        ParsedCommand::Team { match_id, role } => {
            let match_id = match_id.unwrap_or_else(|| "guild-raid-001".to_string());
            let path = format!(
                "/v1/league/raids/{}/roster",
                url_encode_component(&match_id)
            );
            if let Some(role) = role {
                let body = json!({
                    "matrix_user_id": event.sender,
                    "room_id": event.room_id,
                    "role": role,
                    "hero_id": hero_for_raid_role(&role),
                });
                match fetch_consumer_entry_post(&state, &path, body).await {
                    Ok(value) => league_response(
                        "league_raid_roster_join",
                        event,
                        value.clone(),
                        build_league_raid_roster_matrix_reply(&value),
                    ),
                    Err(response) => response,
                }
            } else {
                match fetch_consumer_entry_get(&state, &path).await {
                    Ok(value) => league_response(
                        "league_raid_roster",
                        event,
                        value.clone(),
                        build_league_raid_roster_matrix_reply(&value),
                    ),
                    Err(response) => response,
                }
            }
        }
        ParsedCommand::Rank => {
            match fetch_consumer_entry_get(&state, "/v1/league/rankings").await {
                Ok(value) => league_response(
                    "league_rank",
                    event,
                    value.clone(),
                    build_league_rank_matrix_reply(Some(&value)),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Guild { guild_id } => {
            if let Some(guild_id) = guild_id {
                let path = format!("/v1/league/guilds/{}/join", url_encode_component(&guild_id));
                let body = json!({
                    "matrix_user_id": event.sender,
                    "room_id": event.room_id,
                    "display_name": event.sender,
                });
                match fetch_consumer_entry_post(&state, &path, body).await {
                    Ok(value) => league_response(
                        "league_guild_join",
                        event,
                        value.clone(),
                        build_league_guild_join_matrix_reply(&value),
                    ),
                    Err(response) => response,
                }
            } else {
                match fetch_consumer_entry_get(&state, "/v1/league/guilds").await {
                    Ok(value) => league_response(
                        "league_guilds",
                        event,
                        value.clone(),
                        build_league_guilds_matrix_reply(&value),
                    ),
                    Err(response) => response,
                }
            }
        }
        ParsedCommand::Draft { heroes } => {
            let path = format!(
                "/v1/league/players/{}/draft",
                url_encode_component(&event.sender)
            );
            let body = json!({
                "matrix_user_id": event.sender,
                "room_id": event.room_id,
                "heroes": heroes,
            });
            match fetch_consumer_entry_post(&state, &path, body).await {
                Ok(value) => league_response(
                    "league_draft",
                    event,
                    value.clone(),
                    build_league_draft_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Loadout => {
            let path = format!(
                "/v1/league/players/{}/loadout",
                url_encode_component(&event.sender)
            );
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "league_loadout",
                    event,
                    value.clone(),
                    build_league_loadout_matrix_reply(Some(&value)),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Progression => {
            let path = format!(
                "/v1/league/players/{}/progression",
                url_encode_component(&event.sender)
            );
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "league_progression",
                    event,
                    value.clone(),
                    build_league_progression_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Skills => {
            let path = format!(
                "/v1/league/players/{}/progression",
                url_encode_component(&event.sender)
            );
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "league_skills",
                    event,
                    value.clone(),
                    build_league_skills_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Tools => {
            let path = format!(
                "/v1/league/players/{}/progression",
                url_encode_component(&event.sender)
            );
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "league_tools",
                    event,
                    value.clone(),
                    build_league_tools_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Skins => {
            let path = format!(
                "/v1/league/players/{}/progression",
                url_encode_component(&event.sender)
            );
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "league_skins",
                    event,
                    value.clone(),
                    build_league_skins_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Profile => {
            let path = format!(
                "/v1/league/players/{}/profile",
                url_encode_component(&event.sender)
            );
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "league_profile",
                    event,
                    value.clone(),
                    build_league_profile_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Rewards => {
            let path = format!(
                "/v1/league/players/{}/rewards",
                url_encode_component(&event.sender)
            );
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "league_rewards",
                    event,
                    value.clone(),
                    build_league_rewards_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Inventory => {
            let path = format!(
                "/v1/league/players/{}/inventory",
                url_encode_component(&event.sender)
            );
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "league_inventory",
                    event,
                    value.clone(),
                    build_league_inventory_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::History => {
            let path = format!(
                "/v1/league/players/{}/history",
                url_encode_component(&event.sender)
            );
            match fetch_consumer_entry_get(&state, &path).await {
                Ok(value) => league_response(
                    "league_history",
                    event,
                    value.clone(),
                    build_league_history_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Join { match_id } => {
            let path = format!(
                "/v1/league/matches/{}/join",
                url_encode_component(&match_id)
            );
            let body = json!({
                "matrix_user_id": event.sender,
                "room_id": event.room_id,
                "display_name": event.sender,
            });
            match fetch_consumer_entry_post(&state, &path, body).await {
                Ok(value) => league_response(
                    "league_join",
                    event,
                    value.clone(),
                    build_league_join_matrix_reply_from(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Submit { match_id, body } => {
            let path = format!(
                "/v1/league/matches/{}/submit",
                url_encode_component(&match_id)
            );
            let body = json!({
                "matrix_user_id": event.sender,
                "room_id": event.room_id,
                "body": body,
            });
            match fetch_consumer_entry_post(&state, &path, body).await {
                Ok(value) => league_response(
                    "league_submit",
                    event,
                    value.clone(),
                    build_league_submission_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Battle { match_id, text } => {
            let path = format!(
                "/v1/league/matches/{}/battle",
                url_encode_component(&match_id)
            );
            let body = build_league_battle_request_body(
                &event.sender,
                &event.room_id,
                &event.event_id,
                &text,
                &event.event_type,
                event.timestamp_ms,
                &event.metadata,
                event.content.clone(),
            );
            match fetch_consumer_entry_post(&state, &path, body).await {
                Ok(value) => league_response(
                    "league_battle",
                    event,
                    value.clone(),
                    build_league_battle_matrix_reply(&value),
                ),
                Err(response) => response,
            }
        }
        ParsedCommand::Wallet => {
            match fetch_wallet_projection(&state, &event.sender, &event.room_id).await {
                Ok(value) => (
                    StatusCode::OK,
                    Json(MatrixAdapterResponse {
                        accepted: true,
                        action: "wallet_lookup".to_string(),
                        event_id: event.event_id,
                        room_id: event.room_id,
                        sender: event.sender,
                        forwarded: Some(value.clone()),
                        projected_reply: Some(build_wallet_matrix_reply(&value)),
                    }),
                )
                    .into_response(),
                Err(response) => response,
            }
        }
        ParsedCommand::Plans => {
            match fetch_wallet_projection(&state, &event.sender, &event.room_id).await {
                Ok(value) => (
                    StatusCode::OK,
                    Json(MatrixAdapterResponse {
                        accepted: true,
                        action: "plans_lookup".to_string(),
                        event_id: event.event_id,
                        room_id: event.room_id,
                        sender: event.sender,
                        forwarded: Some(value.clone()),
                        projected_reply: Some(build_plans_matrix_reply(&value)),
                    }),
                )
                    .into_response(),
                Err(response) => response,
            }
        }
        ParsedCommand::Unsupported { text } => (
            StatusCode::OK,
            Json(MatrixAdapterResponse {
                accepted: false,
                action: "unsupported_command".to_string(),
                event_id: event.event_id,
                room_id: event.room_id,
                sender: event.sender,
                forwarded: None,
                projected_reply: Some(build_plain_matrix_reply(&format!(
                    "未识别指令: {text}。输入 /help 查看可用命令。"
                ))),
            }),
        )
            .into_response(),
    }
}

#[allow(clippy::result_large_err)]
fn authorize_ingress(headers: &HeaderMap, config: &MatrixAdapterConfig) -> Result<(), Response> {
    let Some(expected) = config.ingress_token.as_deref() else {
        return Ok(());
    };

    let provided = headers
        .get("x-entry-token")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty());

    match provided {
        Some(token) if token == expected => Ok(()),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "missing or invalid entry token" })),
        )
            .into_response()),
    }
}

async fn reject_duplicate_event(state: &AppState, event: &MatrixEventEnvelope) -> Option<Response> {
    let event_id = event.event_id.as_deref()?.trim();
    if event_id.is_empty() {
        return None;
    }

    let now_epoch = Utc::now().timestamp();
    let ttl_secs = state.config().recent_event_window_secs;
    let max_size = state.config().recent_event_cache_size;
    let mut cache = state.inner.recent_event_cache.lock().await;
    prune_recent_event_cache(&mut cache, now_epoch, ttl_secs, max_size);

    if cache.seen.contains_key(event_id) {
        state.inner.metrics.inc_duplicate_events();
        return Some(
            (
                StatusCode::OK,
                Json(MatrixAdapterResponse {
                    accepted: false,
                    action: "duplicate_event".to_string(),
                    event_id: event.event_id.clone(),
                    room_id: event.room_id.clone(),
                    sender: event.sender.clone(),
                    forwarded: None,
                    projected_reply: Some(build_plain_matrix_reply(
                        "重复事件已忽略，未再次创建任务。",
                    )),
                }),
            )
                .into_response(),
        );
    }

    cache.seen.insert(event_id.to_string(), now_epoch);
    cache.order.push_back(event_id.to_string());
    prune_recent_event_cache(&mut cache, now_epoch, ttl_secs, max_size);
    persist_recent_event_cache(&cache, state.config());
    None
}

fn prune_recent_event_cache(
    cache: &mut RecentEventCache,
    now_epoch: i64,
    ttl_secs: u64,
    max_size: usize,
) {
    while let Some(front) = cache.order.front().cloned() {
        let should_drop = cache
            .seen
            .get(&front)
            .map(|seen_at| now_epoch.saturating_sub(*seen_at) >= ttl_secs as i64)
            .unwrap_or(true)
            || cache.seen.len() > max_size;

        if !should_drop {
            break;
        }

        cache.order.pop_front();
        cache.seen.remove(&front);
    }
}

async fn enforce_rate_limit(
    state: &AppState,
    key: String,
    error_code: &str,
) -> Result<(), Response> {
    let max_requests = state.config().rate_limit_max_requests;
    let now_epoch = Utc::now().timestamp();

    let mut rate_limits = state.inner.rate_limits.lock().await;
    let entries = rate_limits.seen.entry(key).or_default();
    let modified =
        prune_rate_limit_entries(entries, now_epoch, state.config().rate_limit_window_secs);

    if entries.len() >= max_requests {
        if modified {
            persist_rate_limit_cache(&rate_limits, state.config());
        }
        state.inner.metrics.inc_rate_limited_requests();
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "error": error_code,
                "rate_limit_window_secs": state.config().rate_limit_window_secs,
                "rate_limit_max_requests": max_requests,
            })),
        )
            .into_response());
    }

    entries.push_back(now_epoch);
    persist_rate_limit_cache(&rate_limits, state.config());
    Ok(())
}

fn build_matrix_event_rate_limit_key(event: &MatrixEventEnvelope) -> String {
    format!("matrix:{}:{}", event.sender, event.room_id)
}

#[allow(clippy::result_large_err)]
fn validate_text_payload(raw: &str, max_text_chars: usize) -> Result<String, Response> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix text payload must not be empty" })),
        )
            .into_response());
    }

    let length = trimmed.chars().count();
    if length > max_text_chars {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "matrix text payload too large",
                "max_text_chars": max_text_chars,
                "received_text_chars": length,
            })),
        )
            .into_response());
    }

    Ok(trimmed.to_string())
}

fn parse_matrix_command(text: &str) -> ParsedCommand {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return ParsedCommand::Raw {
            text: trimmed.to_string(),
        };
    }

    let mut parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return ParsedCommand::Unsupported {
            text: trimmed.to_string(),
        };
    }

    let command = parts.remove(0);
    match command {
        "/help" | "/h" => ParsedCommand::Help,
        "/app" | "/client" | "/客户端" => ParsedCommand::ClientApp,
        "/feed" | "/timeline" | "/动态" => parse_client_feed_command(parts),
        "/social" | "/contacts" | "/chat" | "/社交" => ParsedCommand::ClientSocial,
        "/duel" | "/battleface" | "/对战" => parse_client_duel_command(parts),
        "/league" | "/tl" | "/trillionnium" => ParsedCommand::League,
        "/arena" | "/matches" => ParsedCommand::Arena,
        "/quest" | "/quests" | "/daily" => ParsedCommand::Quest,
        "/world" => parse_world_command(parts),
        "/map" | "/look" | "/地图" => ParsedCommand::WorldMap,
        "/go" | "/move" | "/walk" | "/走" | "/移动" => parse_world_map_move_command(parts),
        "/assets" | "/asset" | "/worldassets" => ParsedCommand::WorldAssets,
        "/upgrade" | "/升级" => parse_world_asset_upgrade_command(parts),
        "/companies" | "/companys" => ParsedCommand::WorldCompanies,
        "/company" | "/公司" => parse_world_company_command(parts),
        "/shops" | "/shop" | "/店铺" => ParsedCommand::WorldShops,
        "/sell" | "/listing" | "/上架" => parse_world_listing_command(parts),
        "/buy" | "/hire" | "/购买" | "/雇佣" => parse_world_buy_command(parts),
        "/work" | "/orders" | "/工作" => parse_world_work_command(parts),
        "/factions" | "/rep" | "/reputation" | "/声望" => ParsedCommand::WorldFactions,
        "/craft" | "/build" | "/create" => parse_craft_command(parts),
        "/contract" | "/bounty" | "/委托" => parse_world_contract_command(parts),
        "/complete" | "/deliver" | "/交付" => parse_world_contract_complete_command(parts),
        "/season" => ParsedCommand::Season,
        "/raid" | "/raids" => parse_raid_command(parts),
        "/team" | "/party" | "/roster" => parse_team_command(parts),
        "/rank" | "/leaderboard" => ParsedCommand::Rank,
        "/loadout" | "/agent" | "/agents" => ParsedCommand::Loadout,
        "/progression" | "/level" | "/xp" | "/等级" | "/经验" => ParsedCommand::Progression,
        "/skills" | "/skill" | "/技能" => ParsedCommand::Skills,
        "/tools" | "/equipment" | "/gear" | "/装备" => ParsedCommand::Tools,
        "/skins" | "/skin" | "/皮肤" => ParsedCommand::Skins,
        "/profile" | "/me" => ParsedCommand::Profile,
        "/rewards" | "/earnings" => ParsedCommand::Rewards,
        "/inventory" | "/items" | "/bag" | "/背包" => ParsedCommand::Inventory,
        "/history" | "/replay" => ParsedCommand::History,
        "/guild" | "/guilds" => ParsedCommand::Guild {
            guild_id: parts.first().map(|value| (*value).to_string()),
        },
        "/draft" => parse_draft_command(parts),
        "/join" => match parts.first() {
            Some(match_id) if !match_id.trim().is_empty() => ParsedCommand::Join {
                match_id: (*match_id).to_string(),
            },
            _ => ParsedCommand::Unsupported {
                text: "/join 需要 match id，例如 /join daily-dungeon-001".to_string(),
            },
        },
        "/battle" => parse_battle_command(parts),
        "/submit" => parse_submit_command(parts),
        "/balance" | "/wallet" | "/pay" | "/余额" | "/钱包" | "/支付" => {
            ParsedCommand::Wallet
        }
        "/plans" | "/plan" | "/package" | "/套餐" => ParsedCommand::Plans,
        "/status" => match parts.first() {
            Some(task_id) if !task_id.trim().is_empty() => ParsedCommand::Status {
                task_id: (*task_id).to_string(),
            },
            _ => ParsedCommand::Unsupported {
                text: trimmed.to_string(),
            },
        },
        "/task" | "/ask" => parse_task_command(parts),
        _ => ParsedCommand::Unsupported {
            text: trimmed.to_string(),
        },
    }
}

fn parse_client_duel_command(parts: Vec<&str>) -> ParsedCommand {
    if parts.is_empty() {
        return ParsedCommand::ClientDuel {
            opponent: Some("nearby".to_string()),
            body: "nearby face-to-face Agent duel: choose loadout, make the first move, and record evidence.".to_string(),
        };
    }
    let opponent = parts.first().map(|value| (*value).to_string());
    let body = parts.iter().skip(1).copied().collect::<Vec<_>>().join(" ");
    ParsedCommand::ClientDuel {
        opponent,
        body: if body.trim().is_empty() {
            "nearby face-to-face Agent duel: choose loadout, make the first move, and record evidence.".to_string()
        } else {
            body
        },
    }
}

fn parse_client_feed_command(parts: Vec<&str>) -> ParsedCommand {
    if parts.is_empty() {
        return ParsedCommand::ClientFeed { filter: None };
    }
    let raw_filter = parts.join(" ");
    let trimmed = raw_filter.trim();
    if trimmed.is_empty() {
        return ParsedCommand::ClientFeed { filter: None };
    }
    let normalized = trimmed.to_ascii_lowercase();
    let filter = match normalized.as_str() {
        "all" | "recommended" => None,
        "event" | "events" | "live" | "signal" => Some("live_event".to_string()),
        "task" | "tasks" | "route" | "routes" => Some("route_task".to_string()),
        "contract" | "contracts" => Some("contract".to_string()),
        "completion" | "completions" | "complete" => Some("completion".to_string()),
        "commerce" | "deal" | "deals" | "purchase" | "purchases" | "work" => {
            Some("commerce".to_string())
        }
        "social" | "contact" | "contacts" => Some("social".to_string()),
        _ if matches!(trimmed, "全部" | "推荐") => None,
        _ if matches!(trimmed, "事件") => Some("live_event".to_string()),
        _ if matches!(trimmed, "任务") => Some("route_task".to_string()),
        _ if matches!(trimmed, "委托") => Some("contract".to_string()),
        _ if matches!(trimmed, "完成") => Some("completion".to_string()),
        _ if matches!(trimmed, "成交") => Some("commerce".to_string()),
        _ if matches!(trimmed, "社交") => Some("social".to_string()),
        _ => {
            return ParsedCommand::Unsupported {
                text: "/feed 仅支持 all/events/tasks/contracts/completion/commerce/social（或 全部/事件/任务/委托/完成/成交/社交）".to_string(),
            }
        }
    };
    ParsedCommand::ClientFeed { filter }
}

fn parse_world_command(parts: Vec<&str>) -> ParsedCommand {
    if parts.is_empty() {
        return ParsedCommand::World;
    }
    if parts
        .first()
        .is_some_and(|value| value.eq_ignore_ascii_case("action") || *value == "行动")
    {
        let body = parts.into_iter().skip(1).collect::<Vec<_>>().join(" ");
        if body.trim().is_empty() {
            return ParsedCommand::World;
        }
        return ParsedCommand::WorldAction { body };
    }
    if parts.first().is_some_and(|value| {
        value.eq_ignore_ascii_case("contract")
            || value.eq_ignore_ascii_case("bounty")
            || *value == "委托"
    }) {
        return parse_world_contract_command(parts.into_iter().skip(1).collect());
    }
    if parts.first().is_some_and(|value| {
        value.eq_ignore_ascii_case("map") || value.eq_ignore_ascii_case("look") || *value == "地图"
    }) {
        return ParsedCommand::WorldMap;
    }
    if parts.first().is_some_and(|value| {
        value.eq_ignore_ascii_case("go")
            || value.eq_ignore_ascii_case("move")
            || value.eq_ignore_ascii_case("walk")
            || *value == "走"
            || *value == "移动"
    }) {
        return parse_world_map_move_command(parts.into_iter().skip(1).collect());
    }
    ParsedCommand::WorldAction {
        body: parts.join(" "),
    }
}

fn parse_world_map_move_command(parts: Vec<&str>) -> ParsedCommand {
    let target = parts.join(" ");
    if target.trim().is_empty() {
        return ParsedCommand::WorldMap;
    }
    ParsedCommand::WorldMapMove {
        target: target.trim().to_string(),
    }
}

fn parse_world_contract_command(parts: Vec<&str>) -> ParsedCommand {
    let body = parts.join(" ");
    if body.trim().is_empty() {
        return ParsedCommand::World;
    }
    ParsedCommand::WorldContract { body }
}

fn parse_craft_command(parts: Vec<&str>) -> ParsedCommand {
    let body = parts.join(" ");
    if body.trim().is_empty() {
        return ParsedCommand::World;
    }
    ParsedCommand::CraftAction { body }
}

fn parse_world_asset_upgrade_command(parts: Vec<&str>) -> ParsedCommand {
    let Some(asset_id) = parts
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return ParsedCommand::WorldAssets;
    };
    let body = parts.into_iter().skip(1).collect::<Vec<_>>().join(" ");
    if body.trim().is_empty() {
        return ParsedCommand::WorldAssets;
    }
    ParsedCommand::WorldAssetUpgrade {
        asset_id: asset_id.to_string(),
        body,
    }
}

fn parse_world_company_command(parts: Vec<&str>) -> ParsedCommand {
    let Some(asset_id) = parts
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return ParsedCommand::WorldCompanies;
    };
    let body = parts.into_iter().skip(1).collect::<Vec<_>>().join(" ");
    if body.trim().is_empty() {
        return ParsedCommand::WorldCompanies;
    }
    ParsedCommand::WorldCompanyCreate {
        asset_id: asset_id.to_string(),
        body,
    }
}

fn parse_world_listing_command(parts: Vec<&str>) -> ParsedCommand {
    let Some(company_id) = parts
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return ParsedCommand::WorldShops;
    };
    let body = parts.into_iter().skip(1).collect::<Vec<_>>().join(" ");
    if body.trim().is_empty() {
        return ParsedCommand::WorldShops;
    }
    ParsedCommand::WorldListingCreate {
        company_id: company_id.to_string(),
        body,
    }
}

fn parse_world_buy_command(parts: Vec<&str>) -> ParsedCommand {
    let Some(listing_id) = parts
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return ParsedCommand::WorldShops;
    };
    let body = parts.into_iter().skip(1).collect::<Vec<_>>().join(" ");
    ParsedCommand::WorldListingBuy {
        listing_id: listing_id.to_string(),
        body: if body.trim().is_empty() {
            "Buy this listing and open a work order with deliverable, evidence, acceptance standard, and next action.".to_string()
        } else {
            body
        },
    }
}

fn parse_world_work_command(parts: Vec<&str>) -> ParsedCommand {
    if parts.is_empty() {
        return ParsedCommand::WorldWork;
    }
    let action = parts[0].trim();
    if matches!(action, "deliver" | "交付") {
        let Some(work_order_id) = parts
            .get(1)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            return ParsedCommand::WorldWork;
        };
        let body = parts.iter().skip(2).copied().collect::<Vec<_>>().join(" ");
        return ParsedCommand::WorldWorkDeliver {
            work_order_id: work_order_id.to_string(),
            body: if body.trim().is_empty() {
                "Work delivery package: deliverable, evidence package, acceptance checklist, risk review, next action, and self-review.".to_string()
            } else {
                body
            },
        };
    }
    if matches!(action, "accept" | "验收") {
        let Some(work_order_id) = parts
            .get(1)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            return ParsedCommand::WorldWork;
        };
        let body = parts.iter().skip(2).copied().collect::<Vec<_>>().join(" ");
        return ParsedCommand::WorldWorkAccept {
            work_order_id: work_order_id.to_string(),
            body: if body.trim().is_empty() {
                "Buyer acceptance: delivered work accepted with proof, quality note, next collaboration, and reputation confirmation.".to_string()
            } else {
                body
            },
        };
    }
    if matches!(action, "reject" | "refund" | "拒收" | "退款") {
        let Some(work_order_id) = parts
            .get(1)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            return ParsedCommand::WorldWork;
        };
        let body = parts.iter().skip(2).copied().collect::<Vec<_>>().join(" ");
        return ParsedCommand::WorldWorkReject {
            work_order_id: work_order_id.to_string(),
            body: if body.trim().is_empty() {
                "Buyer rejection: delivery is not accepted, refund reserved buyer funds, reopen with revision requirements, evidence gaps, and next action.".to_string()
            } else {
                body
            },
        };
    }
    if matches!(
        action,
        "reopen" | "revise" | "revision" | "返工" | "重开" | "重做"
    ) {
        let Some(work_order_id) = parts
            .get(1)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            return ParsedCommand::WorldWork;
        };
        let body = parts.iter().skip(2).copied().collect::<Vec<_>>().join(" ");
        return ParsedCommand::WorldWorkReopen {
            work_order_id: work_order_id.to_string(),
            body: if body.trim().is_empty() {
                "Buyer reopen: reserve funds again, list revision requirements, evidence gaps, acceptance standard, and next redelivery action.".to_string()
            } else {
                body
            },
        };
    }
    if matches!(action, "cancel" | "取消" | "撤销" | "关闭") {
        let Some(work_order_id) = parts
            .get(1)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            return ParsedCommand::WorldWork;
        };
        let body = parts.iter().skip(2).copied().collect::<Vec<_>>().join(" ");
        return ParsedCommand::WorldWorkCancel {
            work_order_id: work_order_id.to_string(),
            body: if body.trim().is_empty() {
                "Buyer cancel: cancel this open work before delivery, refund reserved buyer funds, record reason, and close the work order.".to_string()
            } else {
                body
            },
        };
    }
    ParsedCommand::WorldWork
}

fn parse_world_contract_complete_command(parts: Vec<&str>) -> ParsedCommand {
    let Some(contract_id) = parts
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return ParsedCommand::World;
    };
    let body = parts.into_iter().skip(1).collect::<Vec<_>>().join(" ");
    if body.trim().is_empty() {
        return ParsedCommand::World;
    }
    ParsedCommand::WorldContractComplete {
        contract_id: contract_id.to_string(),
        body,
    }
}

fn hero_for_raid_role(role: &str) -> &'static str {
    match role.trim().to_ascii_lowercase().as_str() {
        "builder" | "forge" => "forge_builder",
        "auditor" | "reviewer" => "mirror_auditor",
        "closer" | "courier" => "courier_closer",
        "warden" | "risk" => "ledger_warden",
        "designer" | "muse" => "muse_designer",
        _ => "oracle_scout",
    }
}

fn parse_team_command(parts: Vec<&str>) -> ParsedCommand {
    let match_id = parts
        .first()
        .copied()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let role = parts
        .get(1)
        .copied()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    ParsedCommand::Team { match_id, role }
}

fn parse_raid_command(parts: Vec<&str>) -> ParsedCommand {
    let Some(match_id) = parts
        .first()
        .copied()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return ParsedCommand::Raid {
            match_id: None,
            body: None,
        };
    };
    ParsedCommand::Raid {
        match_id: Some(match_id.to_string()),
        body: Some(parts.get(1..).unwrap_or_default().join(" "))
            .filter(|value| !value.trim().is_empty()),
    }
}

fn parse_draft_command(parts: Vec<&str>) -> ParsedCommand {
    let heroes: Vec<String> = parts
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect();
    if heroes.len() < 3 {
        return ParsedCommand::Unsupported {
            text: "/draft 至少需要 3 个英雄，例如 /draft oracle_scout forge_builder mirror_auditor"
                .to_string(),
        };
    }
    ParsedCommand::Draft { heroes }
}

fn parse_submit_command(parts: Vec<&str>) -> ParsedCommand {
    let Some(match_id) = parts
        .first()
        .copied()
        .filter(|value| !value.trim().is_empty())
    else {
        return ParsedCommand::Unsupported {
            text: "/submit 需要 match id 和提交内容，例如 /submit daily-dungeon-001 我的最终方案"
                .to_string(),
        };
    };
    if parts.len() < 2 {
        return ParsedCommand::Unsupported {
            text: "/submit 需要提交内容，例如 /submit daily-dungeon-001 我的最终方案".to_string(),
        };
    }

    ParsedCommand::Submit {
        match_id: match_id.to_string(),
        body: parts[1..].join(" "),
    }
}

fn parse_battle_command(parts: Vec<&str>) -> ParsedCommand {
    let Some(match_id) = parts
        .first()
        .copied()
        .filter(|value| !value.trim().is_empty())
    else {
        return ParsedCommand::Unsupported {
            text: "/battle 需要 match id 和行动文本，例如 /battle daily-dungeon-001 生成一版方案"
                .to_string(),
        };
    };
    if parts.len() < 2 {
        return ParsedCommand::Unsupported {
            text: "/battle 需要行动文本，例如 /battle daily-dungeon-001 生成一版方案".to_string(),
        };
    }

    ParsedCommand::Battle {
        match_id: match_id.to_string(),
        text: parts[1..].join(" "),
    }
}

fn parse_task_command(parts: Vec<&str>) -> ParsedCommand {
    let mut capability_id = None;
    let mut account_id = None;

    let mut idx = 0;
    while idx < parts.len() {
        let token = parts[idx];
        if let Some(rest) = token.strip_prefix("cap=") {
            if !rest.is_empty() {
                capability_id = Some(rest.to_string());
            }
            idx += 1;
            continue;
        }
        if let Some(rest) = token.strip_prefix("account=") {
            if !rest.is_empty() {
                account_id = Some(rest.to_string());
            }
            idx += 1;
            continue;
        }
        if token == "--cap" {
            if let Some(next) = parts.get(idx + 1) {
                if !next.is_empty() {
                    capability_id = Some((*next).to_string());
                }
                idx += 2;
                continue;
            }
            return ParsedCommand::Unsupported {
                text: "/task 参数缺失: --cap 后需提供 capability id".to_string(),
            };
        }
        if token == "--account" {
            if let Some(next) = parts.get(idx + 1) {
                if !next.is_empty() {
                    account_id = Some((*next).to_string());
                }
                idx += 2;
                continue;
            }
            return ParsedCommand::Unsupported {
                text: "/task 参数缺失: --account 后需提供 account id".to_string(),
            };
        }

        break;
    }

    let prompt_tokens = if idx >= parts.len() {
        vec![]
    } else {
        parts[idx..].to_vec()
    };

    if prompt_tokens.is_empty() {
        return ParsedCommand::Unsupported {
            text: "/task 需要跟提示文本，例如 /task summarize 这段内容".to_string(),
        };
    }

    ParsedCommand::Task {
        text: prompt_tokens.join(" "),
        capability_id,
        account_id,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_task_request_body(
    sender: &str,
    room_id: &str,
    event_id: &Option<String>,
    message: &str,
    capability_id: Option<String>,
    account_id: Option<String>,
    event_type: &Option<String>,
    timestamp_ms: Option<i64>,
    metadata: &Option<Value>,
    content: Option<Value>,
) -> Value {
    let mut body = json!({
        "matrix_user_id": sender,
        "room_id": room_id,
        "message": message,
        "event_id": event_id,
        "metadata": merge_metadata(event_type.clone(), timestamp_ms, metadata.clone(), content),
    });

    if let Some(capability_id) = capability_id {
        body["capability_id"] = json!(capability_id);
    }

    if let Some(account_id) = account_id {
        body["account_id"] = json!(account_id);
    }

    body
}

#[allow(clippy::too_many_arguments)]
fn build_league_battle_request_body(
    sender: &str,
    room_id: &str,
    event_id: &Option<String>,
    message: &str,
    event_type: &Option<String>,
    timestamp_ms: Option<i64>,
    metadata: &Option<Value>,
    content: Option<Value>,
) -> Value {
    json!({
        "matrix_user_id": sender,
        "room_id": room_id,
        "message": message,
        "event_id": event_id,
        "metadata": merge_metadata(event_type.clone(), timestamp_ms, metadata.clone(), content),
    })
}

fn normalize_request_fingerprint_value(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_default()
}

fn normalize_request_fingerprint_text(value: Option<&str>) -> String {
    value.map(str::trim).unwrap_or_default().to_string()
}

fn build_request_fingerprint(parts: &[String]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0x1f]);
    }
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn build_matrix_request_fingerprint_from_body(request_body: &Value) -> String {
    build_request_fingerprint(&[
        "matrix_message".to_string(),
        normalize_request_fingerprint_value(
            request_body.get("matrix_user_id").and_then(Value::as_str),
        ),
        normalize_request_fingerprint_value(request_body.get("room_id").and_then(Value::as_str)),
        normalize_request_fingerprint_value(request_body.get("session_id").and_then(Value::as_str)),
        normalize_request_fingerprint_value(request_body.get("org_id").and_then(Value::as_str)),
        normalize_request_fingerprint_value(request_body.get("account_id").and_then(Value::as_str)),
        normalize_request_fingerprint_value(
            request_body.get("capability_id").and_then(Value::as_str),
        ),
        normalize_request_fingerprint_value(request_body.get("event_id").and_then(Value::as_str)),
        normalize_request_fingerprint_value(
            request_body.get("idempotency_key").and_then(Value::as_str),
        ),
        normalize_request_fingerprint_text(request_body.get("message").and_then(Value::as_str)),
    ])
}

#[allow(clippy::result_large_err)]
fn sign_user_session_assertion(assertion_b64: &str, secret: &str) -> Result<String, Response> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "invalid CONSUMER_ENTRY_SESSION_AUTH_SECRET configuration"
            })),
        )
            .into_response()
    })?;
    mac.update(assertion_b64.as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

#[allow(clippy::result_large_err)]
fn build_consumer_entry_session_auth_headers(
    state: &AppState,
    request_body: &Value,
) -> Result<Option<(String, String)>, Response> {
    let runtime_state = consumer_entry_session_auth_issuer_registry_runtime_state(state);
    let (selected_secret, key_id, selection) =
        resolve_consumer_entry_session_auth_runtime_selection(state.config(), &runtime_state);
    let Some(secret) = selected_secret else {
        return Ok(None);
    };
    if selection.source == "issuer_registry" && selection.status != "ok" {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "failed to select a downstream session auth key from issuer registry",
                "issuer": state.config().consumer_entry_session_auth_issuer,
                "key_id": key_id,
                "selection": selection,
            })),
        )
            .into_response());
    }

    let subject = request_body
        .get("matrix_user_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": "matrix request body is missing matrix_user_id for downstream session auth"
                })),
            )
                .into_response()
        })?
        .to_string();
    let room_id = request_body
        .get("room_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": "matrix request body is missing room_id for downstream session auth"
                })),
            )
                .into_response()
        })?
        .to_string();
    let now_epoch = Utc::now().timestamp();
    let claims = UserSessionAuthClaims {
        version: 1,
        issuer: state.config().consumer_entry_session_auth_issuer.clone(),
        key_id,
        subject,
        source_kind: "matrix_message".to_string(),
        audience: Some(state.config().consumer_entry_session_auth_audience.clone()),
        request_fingerprint: Some(build_matrix_request_fingerprint_from_body(request_body)),
        room_id: Some(room_id),
        session_id: request_body
            .get("session_id")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        org_id: request_body
            .get("org_id")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        account_id: request_body
            .get("account_id")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        issued_at_epoch: now_epoch,
        expires_at_epoch: now_epoch + state.config().consumer_entry_session_auth_ttl_secs as i64,
    };
    let assertion_json = serde_json::to_vec(&claims).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("failed to serialize downstream session assertion: {err}")
            })),
        )
            .into_response()
    })?;
    let assertion = URL_SAFE_NO_PAD.encode(assertion_json);
    let signature = sign_user_session_assertion(&assertion, &secret)?;
    Ok(Some((assertion, signature)))
}

async fn fetch_wallet_projection(
    state: &AppState,
    matrix_user_id: &str,
    room_id: &str,
) -> Result<Value, Response> {
    let url = format!(
        "{}/v1/matrix/users/{}/wallet?room_id={}",
        state.config().consumer_entry_base_url.trim_end_matches('/'),
        url_encode_component(matrix_user_id),
        url_encode_component(room_id),
    );

    let mut req = state.inner.http.get(url);
    if let Some(api_key) = &state.config().consumer_entry_api_key {
        req = req.header("x-api-key", api_key);
    }
    if let Some(token) = &state.config().consumer_entry_ingress_token {
        req = req.header("x-entry-token", token);
    }

    let response = match req.send().await {
        Ok(response) => response,
        Err(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("failed to reach consumer-entry-api: {err}") })),
            )
                .into_response())
        }
    };

    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!(
                    "consumer-entry-api returned non-json wallet response: {err}"
                )})),
            )
                .into_response())
        }
    };

    if !status.is_success() {
        return Err((status, Json(value)).into_response());
    }

    Ok(value)
}

async fn fetch_consumer_entry_get(state: &AppState, path: &str) -> Result<Value, Response> {
    let url = format!(
        "{}{}",
        state.config().consumer_entry_base_url.trim_end_matches('/'),
        path
    );
    let mut req = state.inner.http.get(url);
    if let Some(api_key) = &state.config().consumer_entry_api_key {
        req = req.header("x-api-key", api_key);
    }
    if let Some(token) = &state.config().consumer_entry_ingress_token {
        req = req.header("x-entry-token", token);
    }

    let response = match req.send().await {
        Ok(response) => response,
        Err(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("failed to reach consumer-entry-api: {err}") })),
            )
                .into_response())
        }
    };
    parse_consumer_entry_json_response(response, "consumer-entry-api returned non-json response")
        .await
}

async fn fetch_consumer_entry_post(
    state: &AppState,
    path: &str,
    body: Value,
) -> Result<Value, Response> {
    let url = format!(
        "{}{}",
        state.config().consumer_entry_base_url.trim_end_matches('/'),
        path
    );
    let mut req = state.inner.http.post(url).json(&body);
    if let Some(api_key) = &state.config().consumer_entry_api_key {
        req = req.header("x-api-key", api_key);
    }
    if let Some(token) = &state.config().consumer_entry_ingress_token {
        req = req.header("x-entry-token", token);
    }
    if let Some((assertion, signature)) =
        match build_consumer_entry_session_auth_headers(state, &body) {
            Ok(value) => value,
            Err(response) => return Err(response),
        }
    {
        req = req
            .header(USER_SESSION_ASSERTION_HEADER, assertion)
            .header(USER_SESSION_SIGNATURE_HEADER, signature);
    }

    let response = match req.send().await {
        Ok(response) => response,
        Err(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("failed to reach consumer-entry-api: {err}") })),
            )
                .into_response())
        }
    };
    parse_consumer_entry_json_response(response, "consumer-entry-api returned non-json response")
        .await
}

async fn parse_consumer_entry_json_response(
    response: reqwest::Response,
    error_prefix: &str,
) -> Result<Value, Response> {
    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("{error_prefix}: {err}") })),
            )
                .into_response())
        }
    };

    if !status.is_success() {
        return Err((status, Json(value)).into_response());
    }

    Ok(value)
}

async fn fetch_task_projection(state: &AppState, task_id: &str) -> Result<Value, Response> {
    let url = format!(
        "{}/v1/chat/tasks/{}",
        state.config().consumer_entry_base_url.trim_end_matches('/'),
        task_id
    );

    let mut req = state.inner.http.get(url);
    if let Some(api_key) = &state.config().consumer_entry_api_key {
        req = req.header("x-api-key", api_key);
    }
    if let Some(token) = &state.config().consumer_entry_ingress_token {
        req = req.header("x-entry-token", token);
    }

    let response = match req.send().await {
        Ok(response) => response,
        Err(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("failed to reach consumer-entry-api: {err}") })),
            )
                .into_response())
        }
    };

    let status = response.status();
    let forwarded = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!(
                    "consumer-entry-api returned non-json response: {err}"
                )})),
            )
                .into_response())
        }
    };

    if !status.is_success() {
        return Err((status, Json(forwarded)).into_response());
    }

    Ok(json!({
        "task_id": task_id,
        "forwarded": forwarded,
        "projected_reply": build_projected_matrix_reply(&forwarded),
        "generated_at": Utc::now().to_rfc3339(),
    }))
}

async fn forward_to_consumer_entry(state: AppState, request_body: Value) -> Response {
    let url = format!(
        "{}/v1/matrix/messages",
        state.config().consumer_entry_base_url.trim_end_matches('/')
    );

    let mut req = state.inner.http.post(url).json(&request_body);
    if let Some(api_key) = &state.config().consumer_entry_api_key {
        req = req.header("x-api-key", api_key);
    }
    if let Some(token) = &state.config().consumer_entry_ingress_token {
        req = req.header("x-entry-token", token);
    }
    if let Some((assertion, signature)) =
        match build_consumer_entry_session_auth_headers(&state, &request_body) {
            Ok(value) => value,
            Err(response) => return response,
        }
    {
        req = req
            .header(USER_SESSION_ASSERTION_HEADER, assertion)
            .header(USER_SESSION_SIGNATURE_HEADER, signature);
    }

    let response = match req.send().await {
        Ok(response) => response,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("failed to reach consumer-entry-api: {err}") })),
            )
                .into_response();
        }
    };

    let status = response.status();
    let forwarded = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!(
                    "consumer-entry-api returned non-json response: {err}"
                )})),
            )
                .into_response();
        }
    };

    if !status.is_success() {
        return (status, Json(forwarded)).into_response();
    }

    let projected_reply = build_projected_matrix_reply(&forwarded);

    (
        StatusCode::ACCEPTED,
        Json(MatrixAdapterResponse {
            accepted: true,
            action: "forwarded_to_consumer_entry".to_string(),
            event_id: forwarded
                .get("source")
                .and_then(|s| s.get("event_id"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            room_id: request_body
                .get("room_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            sender: request_body
                .get("matrix_user_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            forwarded: Some(forwarded),
            projected_reply: Some(projected_reply),
        }),
    )
        .into_response()
}

async fn get_task_projection(
    Path(id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    state.inner.metrics.inc_projection_requests();

    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    match fetch_task_projection(&state, &id).await {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(response) => response,
    }
}

fn extract_matrix_body(content: Option<&Value>) -> Option<String> {
    content
        .and_then(|content| content.get("body"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn merge_metadata(
    event_type: Option<String>,
    timestamp_ms: Option<i64>,
    metadata: Option<Value>,
    content: Option<Value>,
) -> Value {
    json!({
        "event_type": event_type,
        "timestamp_ms": timestamp_ms,
        "metadata": metadata,
        "content": content,
    })
}

fn url_encode_component(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        let is_unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if is_unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn status_label(consumer_status: &str) -> &'static str {
    match consumer_status {
        "queued" => "任务已创建，正在排队中",
        "waiting_for_confirmation" => "这个任务需要确认后才能继续",
        "processing" => "任务已经开始处理",
        "done" => "任务已经完成",
        "failed" => "任务执行失败",
        "refunded" => "任务已退款",
        _ => "任务已收到",
    }
}

fn league_response(
    action: &str,
    event: MatrixEventEnvelope,
    forwarded: Value,
    projected_reply: Value,
) -> Response {
    (
        StatusCode::OK,
        Json(MatrixAdapterResponse {
            accepted: true,
            action: action.to_string(),
            event_id: event.event_id,
            room_id: event.room_id,
            sender: event.sender,
            forwarded: Some(forwarded),
            projected_reply: Some(projected_reply),
        }),
    )
        .into_response()
}

fn build_projected_matrix_reply(forwarded: &Value) -> Value {
    let task_id = forwarded
        .get("task_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown-task");
    let consumer_status = forwarded
        .get("consumer_status")
        .and_then(Value::as_str)
        .unwrap_or("received");
    let invocation_status = forwarded
        .get("invocation_status")
        .and_then(Value::as_str)
        .unwrap_or("Created");
    let execution = forwarded.get("execution").cloned().unwrap_or(Value::Null);
    let dispatch_mode = execution
        .get("dispatch_mode")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let attempts_remaining = execution
        .get("attempts_remaining")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let account_id = forwarded
        .get("request")
        .and_then(|value| value.get("account_id"))
        .and_then(Value::as_str)
        .or_else(|| {
            forwarded
                .get("source")
                .and_then(|value| value.get("identity_scope"))
                .and_then(|value| value.get("account_id"))
                .and_then(Value::as_str)
        })
        .unwrap_or("未绑定");
    let prompt = forwarded
        .get("request")
        .and_then(|value| value.get("prompt"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let label = status_label(consumer_status);
    let body = format!(
        "🧾 CEX 任务卡\n状态：{label}\nTask: {task_id}\n执行：{invocation_status} / {dispatch_mode}\n账户：{account_id}\n查看：/status {task_id}\n余额：/balance"
    );
    let prompt_html = if prompt.is_empty() {
        String::new()
    } else {
        format!("<p><strong>Prompt</strong>: {}</p>", escape_html(prompt))
    };

    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🧾 CEX 任务卡</h3><p><strong>状态</strong>: {}</p><p><strong>Task</strong>: <code>{}</code></p><p><strong>执行</strong>: {} / {}，剩余尝试 {}</p><p><strong>账户</strong>: <code>{}</code></p>{}<p><code>/status {}</code> · <code>/balance</code></p></blockquote>",
            escape_html(label),
            escape_html(task_id),
            escape_html(invocation_status),
            escape_html(dispatch_mode),
            attempts_remaining,
            escape_html(account_id),
            prompt_html,
            escape_html(task_id),
        ),
        "cex_task_id": task_id,
        "consumer_status": consumer_status,
        "invocation_status": invocation_status,
        "cex_card": {
            "type": "task_status",
            "version": 1,
            "task_id": task_id,
            "consumer_status": consumer_status,
            "invocation_status": invocation_status,
            "dispatch_mode": dispatch_mode,
            "attempts_remaining": attempts_remaining,
            "account_id": account_id,
        }
    })
}

fn build_wallet_matrix_reply(wallet: &Value) -> Value {
    let account = wallet.get("account").unwrap_or(wallet);
    let account_id = account
        .get("account_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown-account");
    let currency_unit = account
        .get("currency_unit")
        .and_then(Value::as_str)
        .unwrap_or("credit");
    let balance = account
        .get("balance")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let reserved = account
        .get("reserved")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let available = account
        .get("available")
        .and_then(Value::as_f64)
        .unwrap_or(balance - reserved);
    let package_name = wallet
        .get("package")
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("Local Production Credits");
    let body = format!(
        "💰 CEX 钱包\n可用：{available:.2} {currency_unit}\n已预留：{reserved:.2}\n总额：{balance:.2}\n套餐：{package_name}\n账户：{account_id}"
    );

    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>💰 CEX 钱包</h3><p><strong>可用</strong>: {:.2} {}</p><p><strong>已预留</strong>: {:.2}</p><p><strong>总额</strong>: {:.2}</p><p><strong>套餐</strong>: {}</p><p><strong>账户</strong>: <code>{}</code></p></blockquote>",
            available,
            escape_html(currency_unit),
            reserved,
            balance,
            escape_html(package_name),
            escape_html(account_id),
        ),
        "cex_card": {
            "type": "wallet_summary",
            "version": 1,
            "account_id": account_id,
            "currency_unit": currency_unit,
            "balance": format!("{balance:.2}"),
            "reserved": format!("{reserved:.2}"),
            "available": format!("{available:.2}"),
            "package_name": package_name,
        }
    })
}

fn build_plans_matrix_reply(wallet: &Value) -> Value {
    let package = wallet.get("package").unwrap_or(wallet);
    let package_name = package
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Local Production Credits");
    let billing_model = package
        .get("billing_model")
        .and_then(Value::as_str)
        .unwrap_or("credit_wallet");
    let body = format!(
        "📦 CEX 套餐\n当前套餐：{package_name}\n计费方式：{billing_model}\n功能：chat_tasks / matrix_entry / provider_dispatch\n查看余额：/balance"
    );

    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>📦 CEX 套餐</h3><p><strong>当前套餐</strong>: {}</p><p><strong>计费方式</strong>: {}</p><p><strong>功能</strong>: chat_tasks / matrix_entry / provider_dispatch</p><p><code>/balance</code></p></blockquote>",
            escape_html(package_name),
            escape_html(billing_model),
        ),
        "cex_card": {
            "type": "package_summary",
            "version": 1,
            "package_name": package_name,
            "billing_model": billing_model,
        }
    })
}

fn build_league_home_matrix_reply(home: Option<&Value>) -> Value {
    let player_count = home
        .and_then(|value| value.get("player_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let match_count = home
        .and_then(|value| value.get("match_count"))
        .and_then(Value::as_u64)
        .unwrap_or(3);
    let body = format!("🏆 Trillionnium League\nAI 任务电竞联赛已上线预备服。\n\n玩法：带着 Agent 阵容进入任务赛场，完成真实挑战，打榜、组队、赚取奖励。\n\n当前：{match_count} 个赛场 / {player_count} 名玩家\n\n可用入口：\n/arena - 查看当前赛场\n/quest - 今日副本\n/loadout - 我的 Agent 阵容\n/rank - 排行榜\n/wallet - 钱包");
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": "<blockquote><h3>🏆 Trillionnium League</h3><p><strong>AI 任务电竞联赛已上线预备服。</strong></p><p>带着 Agent 阵容进入任务赛场，完成真实挑战，打榜、组队、赚取奖励。</p><ul><li><code>/arena</code> 查看当前赛场</li><li><code>/quest</code> 今日副本</li><li><code>/loadout</code> 我的 Agent 阵容</li><li><code>/rank</code> 排行榜</li><li><code>/wallet</code> 钱包</li></ul></blockquote>",
        "cex_card": {
            "type": "league_home",
            "version": 1,
            "league": "trillionnium_league",
            "status": "preseason",
            "commands": ["/arena", "/quest", "/loadout", "/rank", "/wallet"]
        }
    })
}

fn build_league_arena_matrix_reply(_matches: Option<&Value>) -> Value {
    let body = "⚔️ Trillionnium Arena\n当前赛场：\n1. daily-dungeon-001｜每日副本｜Solo｜奖励：XP + credits\n2. bounty-arena-001｜赏金赛｜PvP/多人｜奖励：Prize Pool\n3. guild-raid-001｜公会团本｜Co-op｜奖励：贡献分成\n\n加入：/join daily-dungeon-001\n出招：/battle daily-dungeon-001 <你的行动>";
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": "<blockquote><h3>⚔️ Trillionnium Arena</h3><p><strong>当前赛场</strong></p><ol><li><code>daily-dungeon-001</code>｜每日副本｜Solo｜奖励：XP + credits</li><li><code>bounty-arena-001</code>｜赏金赛｜PvP/多人｜奖励：Prize Pool</li><li><code>guild-raid-001</code>｜公会团本｜Co-op｜奖励：贡献分成</li></ol><p><code>/join daily-dungeon-001</code></p><p><code>/battle daily-dungeon-001 &lt;你的行动&gt;</code></p></blockquote>",
        "cex_card": {
            "type": "league_arena",
            "version": 1,
            "league": "trillionnium_league",
            "matches": [
                {"match_id": "daily-dungeon-001", "mode": "daily_dungeon", "status": "open"},
                {"match_id": "bounty-arena-001", "mode": "bounty_arena", "status": "preview"},
                {"match_id": "guild-raid-001", "mode": "guild_raid", "status": "preview"}
            ]
        }
    })
}

fn build_league_quest_matrix_reply(_matches: Option<&Value>) -> Value {
    let body = "🗺️ 今日副本：Prompt Forge 入门战\n目标：用最少成本生成一个可交付方案，并给出自评/风险。\n计分：质量 45% / 速度 15% / 成本 15% / 证据 10% / 客户适配 10% / 体育精神 5%\n\n开始：/join daily-dungeon-001";
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": "<blockquote><h3>🗺️ 今日副本：Prompt Forge 入门战</h3><p><strong>目标</strong>: 用最少成本生成一个可交付方案，并给出自评/风险。</p><p><strong>计分</strong>: 质量 45% / 速度 15% / 成本 15% / 证据 10% / 客户适配 10% / 体育精神 5%</p><p><code>/join daily-dungeon-001</code></p></blockquote>",
        "cex_card": {
            "type": "league_quest",
            "version": 1,
            "league": "trillionnium_league",
            "match_id": "daily-dungeon-001",
            "scoring": {"quality": "45", "speed": "15", "cost_efficiency": "15", "evidence": "10", "customer_fit": "10", "sportsmanship": "5"}
        }
    })
}

fn matrix_route_contains_cjk(value: &str) -> bool {
    value
        .chars()
        .any(|ch| ('\u{3400}'..='\u{9fff}').contains(&ch))
}

fn matrix_route_has_delivery_anchor(value: &str, lower: &str) -> bool {
    lower.contains("deliver")
        || lower.contains("customer")
        || value.contains("客户")
        || value.contains("交付")
        || value.contains("方案")
}

fn matrix_route_has_evidence_anchor(value: &str, lower: &str) -> bool {
    lower.contains("evidence")
        || lower.contains("source")
        || lower.contains("data")
        || value.contains("证据")
        || value.contains("依据")
}

fn matrix_route_has_risk_anchor(value: &str, lower: &str) -> bool {
    lower.contains("risk") || value.contains("风险")
}

fn matrix_route_has_next_anchor(value: &str, lower: &str) -> bool {
    lower.contains("next") || value.contains("下一步") || value.contains("计划")
}

fn matrix_route_has_review_anchor(value: &str, lower: &str) -> bool {
    lower.contains("review")
        || lower.contains("self-check")
        || lower.contains("self check")
        || value.contains("自评")
        || value.contains("自检")
        || value.contains("复盘")
}

fn matrix_route_playability_anchor_body(body: String) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lower = trimmed.to_ascii_lowercase();
    let has_cjk = matrix_route_contains_cjk(trimmed);
    let mut missing_en = Vec::new();
    let mut missing_zh = Vec::new();
    if !matrix_route_has_delivery_anchor(trimmed, &lower) {
        missing_en.push("customer deliverable");
        missing_zh.push("客户交付方案");
    }
    if !matrix_route_has_evidence_anchor(trimmed, &lower) {
        missing_en.push("evidence package");
        missing_zh.push("证据包");
    }
    if !matrix_route_has_risk_anchor(trimmed, &lower) {
        missing_en.push("risk controls");
        missing_zh.push("风险控制");
    }
    if !matrix_route_has_next_anchor(trimmed, &lower) {
        missing_en.push("next action");
        missing_zh.push("下一步行动");
    }
    if !matrix_route_has_review_anchor(trimmed, &lower) {
        missing_en.push("self-review");
        missing_zh.push("自检复盘");
    }
    if missing_en.is_empty() {
        return trimmed.to_string();
    }

    let ends_sentence = trimmed
        .chars()
        .last()
        .map(|ch| matches!(ch, '.' | '。' | '!' | '！' | '?' | '？'))
        .unwrap_or(false);
    let separator = if ends_sentence {
        " "
    } else if has_cjk {
        "；"
    } else {
        "; "
    };
    if has_cjk {
        format!("{}{}补齐{}。", trimmed, separator, missing_zh.join("、"))
    } else {
        format!("{}{}add {}.", trimmed, separator, missing_en.join(", "))
    }
}

fn matrix_route_playability_anchor_command(command: String) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    for prefix in [
        "/upgrade latest",
        "/company latest",
        "/sell latest",
        "/buy latest",
        "/work deliver latest",
        "/work accept latest",
        "/work reject latest",
        "/work reopen latest",
        "/work cancel latest",
        "/world action",
        "/contract",
    ] {
        if let Some(body) = trimmed.strip_prefix(prefix) {
            let body = matrix_route_playability_anchor_body(body.trim().to_string());
            return if body.is_empty() {
                trimmed.to_string()
            } else {
                format!("{prefix} {body}")
            };
        }
    }
    if let Some(rest) = trimmed.strip_prefix("/complete ") {
        let rest = rest.trim();
        let (contract_id, body) = rest
            .split_once(' ')
            .map(|(contract_id, body)| (contract_id.trim(), body.trim()))
            .unwrap_or((rest, ""));
        let body = matrix_route_playability_anchor_body(body.to_string());
        return if body.is_empty() {
            trimmed.to_string()
        } else {
            format!("/complete {contract_id} {body}")
        };
    }
    trimmed.to_string()
}

fn extract_route_next_hint(
    route_task_graph: Option<&Value>,
) -> (
    u64,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
) {
    let route_task_graph_count = route_task_graph
        .and_then(|graph| graph.get("task_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let route_next_task = route_task_graph
        .and_then(|graph| graph.get("tasks"))
        .and_then(Value::as_array)
        .and_then(|tasks| tasks.first())
        .cloned()
        .unwrap_or(Value::Null);
    let route_next_task_id = route_next_task
        .get("task_id")
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_string();
    let route_next_action_label = route_next_task
        .get("suggested_action_label")
        .and_then(Value::as_str)
        .unwrap_or("Draft task follow-up")
        .to_string();
    let route_next_panel_id = route_next_task
        .get("suggested_panel_id")
        .and_then(Value::as_str)
        .unwrap_or("world-action-console")
        .to_string();
    let route_next_command_hint = route_next_task
        .get("suggested_matrix_command")
        .and_then(Value::as_str)
        .unwrap_or("/world action 跟进当前任务并记录证据、阻塞和下一步。")
        .to_string();
    let route_next_command_hint = matrix_route_playability_anchor_command(route_next_command_hint);
    let route_next_location_id = route_next_task
        .get("latest_location_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let route_next_node_id = route_next_task
        .get("suggested_node_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let route_next_opportunity_node_id = route_next_task
        .get("next_opportunity_node_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let route_next_stage_summary = route_next_task
        .get("route_stage_summary")
        .and_then(Value::as_str)
        .unwrap_or("0 events → 0 contracts · latest event/pending")
        .to_string();
    (
        route_task_graph_count,
        route_next_task_id,
        route_next_action_label,
        route_next_panel_id,
        route_next_command_hint,
        route_next_location_id,
        route_next_node_id,
        route_next_opportunity_node_id,
        route_next_stage_summary,
    )
}

fn extract_route_story_slots(
    route_task_graph: Option<&Value>,
) -> (String, String, String, String, String, String) {
    let route_next_task = route_task_graph
        .and_then(|graph| graph.get("tasks"))
        .and_then(Value::as_array)
        .and_then(|tasks| tasks.first())
        .cloned()
        .unwrap_or(Value::Null);
    let route_next_opportunity_kind = route_next_task
        .get("next_opportunity_kind")
        .and_then(Value::as_str)
        .unwrap_or("contract_capture")
        .to_string();
    let route_next_outcome_summary = route_next_task
        .get("outcome_summary")
        .and_then(Value::as_str)
        .unwrap_or("No route outcome yet.")
        .to_string();
    let route_next_feedback_focus = route_next_task
        .get("feedback_focus")
        .and_then(Value::as_str)
        .unwrap_or("Capture evidence, feedback, and blockers for the next follow-up.")
        .to_string();
    let route_next_opportunity_hint = route_next_task
        .get("next_opportunity_hint")
        .and_then(Value::as_str)
        .unwrap_or("Convert the current route into the next contract, listing, or world-action opportunity.")
        .to_string();
    let route_next_opportunity_playbook = route_next_task
        .get("next_opportunity_playbook")
        .and_then(Value::as_str)
        .unwrap_or("Qualify the route, capture proof, and turn it into a concrete repeat-order, upsell, reopen, or referral play.")
        .to_string();
    let route_next_opportunity_command = route_next_task
        .get("next_opportunity_command")
        .and_then(Value::as_str)
        .unwrap_or("/contract 围绕当前机会整理目标、证据、风险、验收标准和下一步。")
        .to_string();
    let route_next_opportunity_command =
        matrix_route_playability_anchor_command(route_next_opportunity_command);
    (
        route_next_opportunity_kind,
        route_next_outcome_summary,
        route_next_feedback_focus,
        route_next_opportunity_hint,
        route_next_opportunity_playbook,
        route_next_opportunity_command,
    )
}

struct RouteOpportunityTarget {
    action_label: String,
    panel_id: String,
    input_id: String,
    input_value: String,
    textarea_id: String,
    body: String,
    node_id: String,
}

fn route_opportunity_target_from_command(
    command: &str,
    opportunity_node_id: &str,
) -> RouteOpportunityTarget {
    let trimmed = command.trim();
    let mut target = RouteOpportunityTarget {
        action_label: "Open world action lane".to_string(),
        panel_id: "world-action-console".to_string(),
        input_id: String::new(),
        input_value: String::new(),
        textarea_id: "world-action-body".to_string(),
        body: trimmed.to_string(),
        node_id: opportunity_node_id.trim().to_string(),
    };

    let strip_body = |prefix: &str| {
        trimmed
            .strip_prefix(prefix)
            .map(|body| body.trim().to_string())
    };

    if let Some(body) = strip_body("/upgrade latest") {
        target.action_label = "Open asset upgrade lane".to_string();
        target.panel_id = "world-assets-panel".to_string();
        target.input_id = "world-asset-id".to_string();
        target.input_value = "latest".to_string();
        target.textarea_id = "world-asset-body".to_string();
        target.body = body;
    } else if let Some(body) = strip_body("/company latest") {
        target.action_label = "Open company lane".to_string();
        target.panel_id = "world-companies-panel".to_string();
        target.input_id = "world-company-asset-id".to_string();
        target.input_value = "latest".to_string();
        target.textarea_id = "world-company-body".to_string();
        target.body = body;
    } else if let Some(body) = strip_body("/sell latest") {
        target.action_label = "Open listing lane".to_string();
        target.panel_id = "world-listings-panel".to_string();
        target.input_id = "world-listing-company-id".to_string();
        target.input_value = "latest".to_string();
        target.textarea_id = "world-listing-body".to_string();
        target.body = body;
    } else if let Some(body) = strip_body("/buy latest") {
        target.action_label = "Open purchase lane".to_string();
        target.panel_id = "world-commerce-panel".to_string();
        target.input_id = "world-buy-listing-id".to_string();
        target.input_value = "latest".to_string();
        target.textarea_id = "world-buy-body".to_string();
        target.body = body;
    } else if let Some(body) = strip_body("/work deliver latest") {
        target.action_label = "Open delivery lane".to_string();
        target.panel_id = "world-commerce-panel".to_string();
        target.input_id = "world-work-deliver-id".to_string();
        target.input_value = "latest".to_string();
        target.textarea_id = "world-work-deliver-body".to_string();
        target.body = body;
    } else if let Some(body) = strip_body("/work accept latest") {
        target.action_label = "Open acceptance lane".to_string();
        target.panel_id = "world-commerce-panel".to_string();
        target.input_id = "world-work-accept-id".to_string();
        target.input_value = "latest".to_string();
        target.textarea_id = "world-work-accept-body".to_string();
        target.body = body;
    } else if let Some(body) = strip_body("/work reject latest") {
        target.action_label = "Open rejection lane".to_string();
        target.panel_id = "world-commerce-panel".to_string();
        target.input_id = "world-work-reject-id".to_string();
        target.input_value = "latest".to_string();
        target.textarea_id = "world-work-reject-body".to_string();
        target.body = body;
    } else if let Some(body) = strip_body("/work reopen latest") {
        target.action_label = "Open reopen lane".to_string();
        target.panel_id = "world-commerce-panel".to_string();
        target.input_id = "world-work-reopen-id".to_string();
        target.input_value = "latest".to_string();
        target.textarea_id = "world-work-reopen-body".to_string();
        target.body = body;
    } else if let Some(body) = strip_body("/work cancel latest") {
        target.action_label = "Open cancellation lane".to_string();
        target.panel_id = "world-commerce-panel".to_string();
        target.input_id = "world-work-cancel-id".to_string();
        target.input_value = "latest".to_string();
        target.textarea_id = "world-work-cancel-body".to_string();
        target.body = body;
    } else if let Some(body) = strip_body("/world action") {
        target.action_label = "Open world action lane".to_string();
        target.body = body;
    } else if let Some(body) = strip_body("/contract") {
        target.action_label = "Open contract capture lane".to_string();
        target.body = body;
    } else if let Some(rest) = trimmed.strip_prefix("/complete ") {
        let rest = rest.trim();
        let (contract_id, body) = rest
            .split_once(' ')
            .map(|(contract_id, body)| (contract_id.trim(), body.trim().to_string()))
            .unwrap_or((rest, String::new()));
        target.action_label = "Open contract completion lane".to_string();
        target.panel_id = "world-contracts-panel".to_string();
        target.input_id = "world-contract-completion-id".to_string();
        target.input_value = contract_id.to_string();
        target.textarea_id = "world-contract-completion-body".to_string();
        target.body = body;
    }

    if target.body.trim().is_empty() {
        target.body = trimmed.to_string();
    }
    target.body = matrix_route_playability_anchor_body(target.body);
    if target.node_id.trim().is_empty() {
        target.node_id = route_focus_panel_default_node_id(&target.panel_id).to_string();
    }

    target
}

fn route_opportunity_target_from_story_value(
    target_value: Option<&Value>,
    fallback_command: &str,
    fallback_node_id: &str,
) -> RouteOpportunityTarget {
    let fallback = route_opportunity_target_from_command(fallback_command, fallback_node_id);
    let Some(target_value) = target_value else {
        return fallback;
    };

    let mut target = RouteOpportunityTarget {
        action_label: target_value
            .get("action_label")
            .and_then(Value::as_str)
            .unwrap_or(&fallback.action_label)
            .to_string(),
        panel_id: target_value
            .get("panel_id")
            .and_then(Value::as_str)
            .unwrap_or(&fallback.panel_id)
            .to_string(),
        input_id: target_value
            .get("input_id")
            .and_then(Value::as_str)
            .unwrap_or(&fallback.input_id)
            .to_string(),
        input_value: target_value
            .get("input_value")
            .and_then(Value::as_str)
            .unwrap_or(&fallback.input_value)
            .to_string(),
        textarea_id: target_value
            .get("textarea_id")
            .and_then(Value::as_str)
            .unwrap_or(&fallback.textarea_id)
            .to_string(),
        body: target_value
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or(&fallback.body)
            .to_string(),
        node_id: target_value
            .get("node_id")
            .and_then(Value::as_str)
            .unwrap_or(&fallback.node_id)
            .to_string(),
    };
    target.body = matrix_route_playability_anchor_body(target.body);
    target
}

fn route_focus_panel_default_node_id(route_next_panel_id: &str) -> &'static str {
    match route_next_panel_id {
        "world-assets-panel" => "asset-yard",
        "world-companies-panel" => "starter-studio",
        "world-listings-panel" => "client-board",
        "world-commerce-panel" => "delivery-dock",
        "world-contracts-panel" => "ledger-office",
        _ => "",
    }
}

struct RouteRunnerHandoffCardContext {
    avatar_task_route_count: u64,
    runner_count: u64,
    reward_claim_action_count: u64,
    next_route_action_count: u64,
    reward_claim_ready_count: u64,
    next_route_ready_count: u64,
    first_runner_id: String,
    first_task_id: String,
    first_to_node_id: String,
    first_latest_location_id: String,
    lifecycle_contract_version: String,
    route_mastery_contract_version: String,
    route_mastery_runner_count: u64,
    first_route_mastery_xp: u64,
    first_route_mastery_tier: String,
    first_route_mastery_tier_label: String,
    first_route_mastery_streak: u64,
    first_route_mastery_next_goal: String,
    first_route_mastery_summary: String,
    first_lifecycle_source: String,
    first_lifecycle_stage: String,
    first_lifecycle_status: String,
    first_progress_label: String,
    first_telemetry_summary: String,
    first_reward_claim_label: String,
    first_reward_claim_status: String,
    first_reward_claim_action_body: String,
    first_next_route_label: String,
    first_next_route_status: String,
    first_next_route_action_body: String,
    first_next_route_sequence_summary: String,
    summary: String,
    handoff_prompt: String,
}

impl RouteRunnerHandoffCardContext {
    fn from_map_hub(map_hub: Option<&Value>) -> Self {
        let handoff = map_hub.and_then(|hub| hub.get("route_runner_handoff"));
        let viewport = map_hub.and_then(|hub| hub.get("viewport")).or(map_hub);
        let runner_items: &[Value] = viewport
            .and_then(|viewport| viewport.get("avatar_route_runners"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let first_runner = runner_items.first();
        let handoff_u64 = |field: &str, fallback: u64| {
            handoff
                .and_then(|handoff| handoff.get(field))
                .and_then(Value::as_u64)
                .unwrap_or(fallback)
        };
        let map_hub_u64 = |field: &str, fallback: u64| {
            map_hub
                .and_then(|hub| hub.get(field))
                .and_then(Value::as_u64)
                .unwrap_or(fallback)
        };
        let handoff_or_runner_str = |handoff_field: &str, runner_field: &str, fallback: &str| {
            handoff
                .and_then(|handoff| handoff.get(handoff_field))
                .and_then(Value::as_str)
                .or_else(|| {
                    first_runner
                        .and_then(|runner| runner.get(runner_field))
                        .and_then(Value::as_str)
                })
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(fallback)
                .to_string()
        };
        let handoff_or_runner_u64 = |handoff_field: &str, runner_field: &str, fallback: u64| {
            handoff
                .and_then(|handoff| handoff.get(handoff_field))
                .and_then(Value::as_u64)
                .or_else(|| {
                    first_runner
                        .and_then(|runner| runner.get(runner_field))
                        .and_then(Value::as_u64)
                })
                .unwrap_or(fallback)
        };
        let runner_count = handoff_u64(
            "runner_count",
            map_hub_u64("avatar_route_runner_count", runner_items.len() as u64),
        );
        let avatar_task_route_count = handoff_u64(
            "avatar_task_route_count",
            map_hub_u64("avatar_task_route_count", 0),
        );
        let reward_claim_action_fallback = runner_items
            .iter()
            .filter(|runner| {
                runner
                    .get("reward_checkpoint")
                    .and_then(|checkpoint| checkpoint.get("reward_claim_action"))
                    .is_some()
                    || runner.get("reward_claim_action_body").is_some()
            })
            .count() as u64;
        let next_route_action_fallback = runner_items
            .iter()
            .filter(|runner| {
                runner
                    .get("reward_checkpoint")
                    .and_then(|checkpoint| checkpoint.get("next_route_action"))
                    .is_some()
                    || runner.get("next_route_action_body").is_some()
            })
            .count() as u64;
        let reward_claim_ready_fallback = runner_items
            .iter()
            .filter(|runner| {
                runner.get("reward_claim_status").and_then(Value::as_str)
                    == Some("claimable_after_evidence")
            })
            .count() as u64;
        let next_route_ready_fallback = runner_items
            .iter()
            .filter(|runner| {
                runner.get("next_route_status").and_then(Value::as_str)
                    == Some("next_route_ready_after_reward_claim")
            })
            .count() as u64;
        let route_mastery_runner_fallback = runner_items
            .iter()
            .filter(|runner| {
                runner
                    .get("route_mastery_contract_version")
                    .and_then(Value::as_str)
                    == Some("trillionnium_route_mastery_v1")
                    || runner
                        .get("route_mastery")
                        .and_then(|mastery| mastery.get("contract_version"))
                        .and_then(Value::as_str)
                        == Some("trillionnium_route_mastery_v1")
            })
            .count() as u64;
        let first_reward_claim_label = handoff_or_runner_str(
            "first_reward_claim_label",
            "reward_claim_label",
            "Prepare reward claim / 准备领奖",
        );
        let first_next_route_label = handoff_or_runner_str(
            "first_next_route_label",
            "next_route_label",
            "Preview next route / 预览下一路线",
        );
        let summary = handoff
            .and_then(|handoff| handoff.get("summary"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                if runner_count > 0 {
                    format!(
                        "Route runner handoff: {runner_count} runners · {reward_claim_action_fallback} reward claims · {next_route_action_fallback} next-route actions · next {first_reward_claim_label} / {first_next_route_label}"
                    )
                } else {
                    "Route runner handoff: waiting for avatar task routes to unlock reward and next-route actions.".to_string()
                }
            });
        Self {
            avatar_task_route_count,
            runner_count,
            reward_claim_action_count: handoff_u64(
                "reward_claim_action_count",
                reward_claim_action_fallback,
            ),
            next_route_action_count: handoff_u64("next_route_action_count", next_route_action_fallback),
            reward_claim_ready_count: handoff_u64(
                "reward_claim_ready_count",
                reward_claim_ready_fallback,
            ),
            next_route_ready_count: handoff_u64("next_route_ready_count", next_route_ready_fallback),
            first_runner_id: handoff_or_runner_str("first_runner_id", "runner_id", "none"),
            first_task_id: handoff_or_runner_str("first_task_id", "task_id", "none"),
            first_to_node_id: handoff_or_runner_str("first_to_node_id", "to_node_id", "target-node"),
            first_latest_location_id: handoff_or_runner_str(
                "first_latest_location_id",
                "latest_location_id",
                "",
            ),
            lifecycle_contract_version: handoff
                .and_then(|handoff| handoff.get("lifecycle_contract_version"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("trillionnium_route_runner_lifecycle_v1")
                .to_string(),
            route_mastery_contract_version: handoff
                .and_then(|handoff| handoff.get("route_mastery_contract_version"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("trillionnium_route_mastery_v1")
                .to_string(),
            route_mastery_runner_count: handoff_u64(
                "route_mastery_runner_count",
                route_mastery_runner_fallback,
            ),
            first_route_mastery_xp: handoff_or_runner_u64("first_route_mastery_xp", "route_mastery_xp", 0),
            first_route_mastery_tier: handoff_or_runner_str(
                "first_route_mastery_tier",
                "route_mastery_tier",
                "route_novice",
            ),
            first_route_mastery_tier_label: handoff_or_runner_str(
                "first_route_mastery_tier_label",
                "route_mastery_tier_label",
                "Route Novice / 路线新手",
            ),
            first_route_mastery_streak: handoff_or_runner_u64(
                "first_route_mastery_streak",
                "route_mastery_streak",
                1,
            ),
            first_route_mastery_next_goal: handoff_or_runner_str(
                "first_route_mastery_next_goal",
                "route_mastery_next_goal",
                "Reach the evidence checkpoint, submit proof, and unlock the rating/reward claim.",
            ),
            first_route_mastery_summary: handoff_or_runner_str(
                "first_route_mastery_summary",
                "route_mastery_summary",
                "Route mastery: Route Novice / 路线新手",
            ),
            first_lifecycle_source: handoff_or_runner_str(
                "first_lifecycle_source",
                "lifecycle_source",
                "projection_preview_fallback",
            ),
            first_lifecycle_stage: handoff_or_runner_str(
                "first_lifecycle_stage",
                "lifecycle_stage",
                "preview_seeded_route",
            ),
            first_lifecycle_status: handoff_or_runner_str(
                "first_lifecycle_status",
                "lifecycle_status",
                "active_preview",
            ),
            first_progress_label: handoff_or_runner_str(
                "first_progress_label",
                "progress_label",
                "0% route progress / 0% 路线进度",
            ),
            first_telemetry_summary: handoff_or_runner_str(
                "first_telemetry_summary",
                "telemetry_summary",
                "route runner telemetry pending",
            ),
            first_reward_claim_label,
            first_reward_claim_status: handoff_or_runner_str(
                "first_reward_claim_status",
                "reward_claim_status",
                "locked_until_evidence_checkpoint",
            ),
            first_reward_claim_action_body: handoff_or_runner_str(
                "first_reward_claim_action_body",
                "reward_claim_action_body",
                "Prepare deliverable, evidence package, risk controls, next action, and self-review before claiming rating/reward.",
            ),
            first_next_route_label,
            first_next_route_status: handoff_or_runner_str(
                "first_next_route_status",
                "next_route_status",
                "next_route_preview_locked_until_reward_claim",
            ),
            first_next_route_action_body: handoff_or_runner_str(
                "first_next_route_action_body",
                "next_route_action_body",
                "Preview the next route with deliverable, evidence package, risk controls, next action, and self-review anchors.",
            ),
            first_next_route_sequence_summary: handoff_or_runner_str(
                "first_next_route_sequence_summary",
                "next_route_sequence_summary",
                "After reward claim, open the next Trillionnium World Map route with the same deliverable → evidence → risk controls → next action → self-review anchors.",
            ),
            summary,
            handoff_prompt: handoff
                .and_then(|handoff| handoff.get("handoff_prompt"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("Claim rating/reward, then open the next route with deliverable → evidence → risk controls → next action → self-review anchors.")
                .to_string(),
        }
    }

    fn to_value(&self) -> Value {
        json!({
            "contract_version": "trillionnium_route_runner_handoff_v1",
            "runner_count": self.runner_count,
            "avatar_task_route_count": self.avatar_task_route_count,
            "reward_claim_action_count": self.reward_claim_action_count,
            "next_route_action_count": self.next_route_action_count,
            "reward_claim_ready_count": self.reward_claim_ready_count,
            "next_route_ready_count": self.next_route_ready_count,
            "supports_checkpoint_reward_history": true,
            "supports_route_runner_lifecycle": true,
            "supports_route_runner_reward_claim_actions": true,
            "supports_route_runner_next_route_actions": true,
            "lifecycle_contract_version": &self.lifecycle_contract_version,
            "supports_route_mastery_progression": true,
            "route_mastery_contract_version": &self.route_mastery_contract_version,
            "route_mastery_runner_count": self.route_mastery_runner_count,
            "first_runner_id": &self.first_runner_id,
            "first_task_id": &self.first_task_id,
            "first_to_node_id": &self.first_to_node_id,
            "first_latest_location_id": &self.first_latest_location_id,
            "first_route_mastery_xp": self.first_route_mastery_xp,
            "first_route_mastery_tier": &self.first_route_mastery_tier,
            "first_route_mastery_tier_label": &self.first_route_mastery_tier_label,
            "first_route_mastery_streak": self.first_route_mastery_streak,
            "first_route_mastery_next_goal": &self.first_route_mastery_next_goal,
            "first_route_mastery_summary": &self.first_route_mastery_summary,
            "first_lifecycle_source": &self.first_lifecycle_source,
            "first_lifecycle_stage": &self.first_lifecycle_stage,
            "first_lifecycle_status": &self.first_lifecycle_status,
            "first_progress_label": &self.first_progress_label,
            "first_telemetry_summary": &self.first_telemetry_summary,
            "first_reward_claim_label": &self.first_reward_claim_label,
            "first_reward_claim_status": &self.first_reward_claim_status,
            "first_reward_claim_action_body": &self.first_reward_claim_action_body,
            "first_next_route_label": &self.first_next_route_label,
            "first_next_route_status": &self.first_next_route_status,
            "first_next_route_action_body": &self.first_next_route_action_body,
            "first_next_route_sequence_summary": &self.first_next_route_sequence_summary,
            "summary": &self.summary,
            "handoff_prompt": &self.handoff_prompt,
        })
    }

    fn text_block(&self) -> String {
        format!(
            "Runner Handoff: {}\nRunner: {} · {} · {}\nMastery: {} · {} XP · streak {}\nLifecycle: {} [{}]\nReward: {} [{}]\nNext Route: {} [{}]\nNext Body: {}",
            self.summary,
            self.first_task_id,
            self.first_progress_label,
            self.first_telemetry_summary,
            self.first_route_mastery_tier_label,
            self.first_route_mastery_xp,
            self.first_route_mastery_streak,
            self.first_lifecycle_stage,
            self.first_lifecycle_status,
            self.first_reward_claim_label,
            self.first_reward_claim_status,
            self.first_next_route_label,
            self.first_next_route_status,
            self.first_next_route_action_body,
        )
    }

    fn html_block(&self) -> String {
        format!(
            "<p><strong>Runner Handoff</strong>: {}</p><p><strong>Runner</strong>: <code>{}</code> · {} · {}</p><p><strong>Mastery</strong>: {} · {} XP · streak {}</p><p><strong>Lifecycle</strong>: {} · <code>{}</code></p><p><strong>Reward</strong>: {} · <code>{}</code></p><p><strong>Next Route</strong>: {} · <code>{}</code></p><p><strong>Next Body</strong>: {}</p>",
            escape_html(&self.summary),
            escape_html(&self.first_task_id),
            escape_html(&self.first_progress_label),
            escape_html(&self.first_telemetry_summary),
            escape_html(&self.first_route_mastery_tier_label),
            self.first_route_mastery_xp,
            self.first_route_mastery_streak,
            escape_html(&self.first_lifecycle_stage),
            escape_html(&self.first_lifecycle_status),
            escape_html(&self.first_reward_claim_label),
            escape_html(&self.first_reward_claim_status),
            escape_html(&self.first_next_route_label),
            escape_html(&self.first_next_route_status),
            escape_html(&self.first_next_route_action_body),
        )
    }

    fn card_json(&self, mut card: Value) -> Value {
        if let Some(card_object) = card.as_object_mut() {
            card_object.insert("route_runner_handoff".to_string(), self.to_value());
            card_object.insert(
                "avatar_task_route_count".to_string(),
                json!(self.avatar_task_route_count),
            );
            card_object.insert(
                "avatar_route_runner_count".to_string(),
                json!(self.runner_count),
            );
            card_object.insert(
                "route_runner_reward_claim_action_count".to_string(),
                json!(self.reward_claim_action_count),
            );
            card_object.insert(
                "route_runner_next_route_action_count".to_string(),
                json!(self.next_route_action_count),
            );
            card_object.insert(
                "route_runner_next_route_ready_count".to_string(),
                json!(self.next_route_ready_count),
            );
            card_object.insert(
                "route_runner_first_task_id".to_string(),
                json!(&self.first_task_id),
            );
            card_object.insert(
                "route_runner_lifecycle_contract_version".to_string(),
                json!(&self.lifecycle_contract_version),
            );
            card_object.insert(
                "route_runner_mastery_contract_version".to_string(),
                json!(&self.route_mastery_contract_version),
            );
            card_object.insert(
                "route_runner_mastery_runner_count".to_string(),
                json!(self.route_mastery_runner_count),
            );
            card_object.insert(
                "route_runner_first_mastery_xp".to_string(),
                json!(self.first_route_mastery_xp),
            );
            card_object.insert(
                "route_runner_first_mastery_tier".to_string(),
                json!(&self.first_route_mastery_tier),
            );
            card_object.insert(
                "route_runner_first_mastery_next_goal".to_string(),
                json!(&self.first_route_mastery_next_goal),
            );
            card_object.insert(
                "route_runner_first_lifecycle_source".to_string(),
                json!(&self.first_lifecycle_source),
            );
            card_object.insert(
                "route_runner_first_lifecycle_stage".to_string(),
                json!(&self.first_lifecycle_stage),
            );
            card_object.insert(
                "route_runner_first_lifecycle_status".to_string(),
                json!(&self.first_lifecycle_status),
            );
            card_object.insert(
                "route_runner_first_progress_label".to_string(),
                json!(&self.first_progress_label),
            );
            card_object.insert(
                "route_runner_reward_claim_status".to_string(),
                json!(&self.first_reward_claim_status),
            );
            card_object.insert(
                "route_runner_next_route_status".to_string(),
                json!(&self.first_next_route_status),
            );
            card_object.insert(
                "route_runner_next_route_action_body".to_string(),
                json!(&self.first_next_route_action_body),
            );
            card_object.insert(
                "route_runner_next_route_sequence_summary".to_string(),
                json!(&self.first_next_route_sequence_summary),
            );
        }
        card
    }
}

struct RouteStoryCardContext {
    route_preview_item_count: u64,
    route_task_linked_count: u64,
    route_task_graph_count: u64,
    route_next_task_id: String,
    route_next_action_label: String,
    route_next_panel_id: String,
    route_next_command_hint: String,
    route_next_location_id: String,
    route_next_node_id: String,
    route_next_opportunity_node_id: String,
    route_next_stage_summary: String,
    route_next_opportunity_kind: String,
    route_next_outcome_summary: String,
    route_next_feedback_focus: String,
    route_next_opportunity_hint: String,
    route_next_opportunity_playbook: String,
    route_next_opportunity_command: String,
    route_next_opportunity_target: RouteOpportunityTarget,
}

impl RouteStoryCardContext {
    fn from_value(value: &Value, fallback_node_id: &str) -> Self {
        let route_preview = value.get("route_preview");
        let route_story = value.get("route_story");
        let route_preview_item_count = route_story
            .and_then(|story| story.get("preview_item_count"))
            .and_then(Value::as_u64)
            .or_else(|| {
                route_preview
                    .and_then(|preview| preview.get("item_count"))
                    .and_then(Value::as_u64)
            })
            .unwrap_or(0);
        let route_task_linked_count = route_story
            .and_then(|story| story.get("task_linked_count"))
            .and_then(Value::as_u64)
            .or_else(|| {
                route_preview
                    .and_then(|preview| preview.get("task_linked_count"))
                    .and_then(Value::as_u64)
            })
            .unwrap_or(0);
        let (
            fallback_route_task_graph_count,
            fallback_route_next_task_id,
            fallback_route_next_action_label,
            fallback_route_next_panel_id,
            fallback_route_next_command_hint,
            fallback_route_next_location_id,
            fallback_route_next_node_id,
            fallback_route_next_opportunity_node_id,
            fallback_route_next_stage_summary,
        ) = extract_route_next_hint(value.get("route_task_graph"));
        let (
            fallback_route_next_opportunity_kind,
            fallback_route_next_outcome_summary,
            fallback_route_next_feedback_focus,
            fallback_route_next_opportunity_hint,
            fallback_route_next_opportunity_playbook,
            fallback_route_next_opportunity_command,
        ) = extract_route_story_slots(value.get("route_task_graph"));
        let route_task_graph_count = route_story
            .and_then(|story| story.get("task_graph_count"))
            .and_then(Value::as_u64)
            .unwrap_or(fallback_route_task_graph_count);
        let route_next_task_id = route_story
            .and_then(|story| story.get("next_task_id"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_task_id)
            .to_string();
        let route_next_action_label = route_story
            .and_then(|story| story.get("next_action_label"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_action_label)
            .to_string();
        let route_next_panel_id = route_story
            .and_then(|story| story.get("next_panel_id"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_panel_id)
            .to_string();
        let route_next_command_hint = matrix_route_playability_anchor_command(
            route_story
                .and_then(|story| story.get("next_command_hint"))
                .and_then(Value::as_str)
                .unwrap_or(&fallback_route_next_command_hint)
                .to_string(),
        );
        let route_next_location_id = route_story
            .and_then(|story| story.get("next_location_id"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_location_id)
            .to_string();
        let route_next_node_id = route_story
            .and_then(|story| story.get("next_node_id"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_node_id)
            .to_string();
        let route_next_opportunity_node_id = route_story
            .and_then(|story| story.get("next_opportunity_node_id"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_opportunity_node_id)
            .to_string();
        let route_next_stage_summary = route_story
            .and_then(|story| story.get("next_stage_summary"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_stage_summary)
            .to_string();
        let route_next_opportunity_kind = route_story
            .and_then(|story| story.get("next_opportunity_kind"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_opportunity_kind)
            .to_string();
        let route_next_outcome_summary = route_story
            .and_then(|story| story.get("next_outcome_summary"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_outcome_summary)
            .to_string();
        let route_next_feedback_focus = route_story
            .and_then(|story| story.get("next_feedback_focus"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_feedback_focus)
            .to_string();
        let route_next_opportunity_hint = route_story
            .and_then(|story| story.get("next_opportunity_hint"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_opportunity_hint)
            .to_string();
        let route_next_opportunity_playbook = route_story
            .and_then(|story| story.get("next_opportunity_playbook"))
            .and_then(Value::as_str)
            .unwrap_or(&fallback_route_next_opportunity_playbook)
            .to_string();
        let route_next_opportunity_command = matrix_route_playability_anchor_command(
            route_story
                .and_then(|story| story.get("next_opportunity_command"))
                .and_then(Value::as_str)
                .unwrap_or(&fallback_route_next_opportunity_command)
                .to_string(),
        );
        let (route_next_node_id, route_next_opportunity_node_id) = resolve_route_focus_nodes(
            &route_next_panel_id,
            &route_next_node_id,
            &route_next_opportunity_node_id,
            fallback_node_id,
        );
        let route_next_opportunity_target = route_opportunity_target_from_story_value(
            route_story.and_then(|story| story.get("next_opportunity_target")),
            &route_next_opportunity_command,
            &route_next_opportunity_node_id,
        );
        Self {
            route_preview_item_count,
            route_task_linked_count,
            route_task_graph_count,
            route_next_task_id,
            route_next_action_label,
            route_next_panel_id,
            route_next_command_hint,
            route_next_location_id,
            route_next_node_id,
            route_next_opportunity_node_id,
            route_next_stage_summary,
            route_next_opportunity_kind,
            route_next_outcome_summary,
            route_next_feedback_focus,
            route_next_opportunity_hint,
            route_next_opportunity_playbook,
            route_next_opportunity_command,
            route_next_opportunity_target,
        }
    }

    fn to_value(&self) -> Value {
        json!({
            "preview_item_count": self.route_preview_item_count,
            "task_linked_count": self.route_task_linked_count,
            "task_graph_count": self.route_task_graph_count,
            "next_task_id": &self.route_next_task_id,
            "next_action_label": &self.route_next_action_label,
            "next_panel_id": &self.route_next_panel_id,
            "next_command_hint": &self.route_next_command_hint,
            "next_location_id": &self.route_next_location_id,
            "next_node_id": &self.route_next_node_id,
            "next_opportunity_node_id": &self.route_next_opportunity_node_id,
            "next_stage_summary": &self.route_next_stage_summary,
            "next_opportunity_kind": &self.route_next_opportunity_kind,
            "next_outcome_summary": &self.route_next_outcome_summary,
            "next_feedback_focus": &self.route_next_feedback_focus,
            "next_opportunity_hint": &self.route_next_opportunity_hint,
            "next_opportunity_playbook": &self.route_next_opportunity_playbook,
            "next_opportunity_command": &self.route_next_opportunity_command,
            "next_opportunity_target": {
                "action_label": &self.route_next_opportunity_target.action_label,
                "panel_id": &self.route_next_opportunity_target.panel_id,
                "input_id": &self.route_next_opportunity_target.input_id,
                "input_value": &self.route_next_opportunity_target.input_value,
                "textarea_id": &self.route_next_opportunity_target.textarea_id,
                "body": &self.route_next_opportunity_target.body,
                "node_id": &self.route_next_opportunity_target.node_id,
            }
        })
    }

    fn text_block(&self, headline: &str, include_task_linked: bool) -> String {
        let linked = if include_task_linked {
            format!(" · {} task-linked", self.route_task_linked_count)
        } else {
            String::new()
        };
        format!(
            "{headline}: {} tasks · {} route items{} · next {} → {}\nNext Route: {} @ {} [{}]\nOutcome: {}\nFeedback: {}\nNext Opportunity: {}\nPlaybook: {}\nOpportunity Command: {}\nCommand: {}",
            self.route_task_graph_count,
            self.route_preview_item_count,
            linked,
            &self.route_next_task_id,
            &self.route_next_action_label,
            &self.route_next_stage_summary,
            &self.route_next_panel_id,
            &self.route_next_location_id,
            &self.route_next_outcome_summary,
            &self.route_next_feedback_focus,
            &self.route_next_opportunity_hint,
            &self.route_next_opportunity_playbook,
            &self.route_next_opportunity_command,
            &self.route_next_command_hint,
        )
    }

    fn html_block(&self, headline: &str, include_task_linked: bool) -> String {
        let linked = if include_task_linked {
            format!(" · {} task-linked", self.route_task_linked_count)
        } else {
            String::new()
        };
        format!(
            "<p><strong>{}</strong>: {} tasks · {} route items{} · next <code>{}</code> → {}</p><p><strong>Next Route</strong>: {} @ <code>{}</code> · <code>{}</code></p><p><strong>Outcome</strong>: {}</p><p><strong>Feedback</strong>: {}</p><p><strong>Next Opportunity</strong>: {}</p><p><strong>Playbook</strong>: {}</p><p><strong>Opportunity Command</strong>: <code>{}</code></p><p><strong>Command</strong>: <code>{}</code></p>",
            escape_html(headline),
            self.route_task_graph_count,
            self.route_preview_item_count,
            linked,
            escape_html(&self.route_next_task_id),
            escape_html(&self.route_next_action_label),
            escape_html(&self.route_next_stage_summary),
            escape_html(&self.route_next_panel_id),
            escape_html(&self.route_next_location_id),
            escape_html(&self.route_next_outcome_summary),
            escape_html(&self.route_next_feedback_focus),
            escape_html(&self.route_next_opportunity_hint),
            escape_html(&self.route_next_opportunity_playbook),
            escape_html(&self.route_next_opportunity_command),
            escape_html(&self.route_next_command_hint),
        )
    }

    fn specialize_opportunity(mut self, surface: &str) -> Self {
        (
            self.route_next_opportunity_hint,
            self.route_next_opportunity_playbook,
            self.route_next_opportunity_command,
        ) = specialize_route_opportunity(
            surface,
            &self.route_next_opportunity_kind,
            &self.route_next_outcome_summary,
            &self.route_next_feedback_focus,
            &self.route_next_opportunity_hint,
            &self.route_next_opportunity_playbook,
            &self.route_next_opportunity_command,
            &self.route_next_task_id,
            &self.route_next_location_id,
            &self.route_next_stage_summary,
        );
        self.route_next_opportunity_target = route_opportunity_target_from_command(
            &self.route_next_opportunity_command,
            &self.route_next_opportunity_node_id,
        );
        self
    }

    #[allow(clippy::too_many_arguments)]
    fn with_custom_follow_up(
        mut self,
        fallback_task_id: &str,
        action_label: &str,
        panel_id: &str,
        command_hint: String,
        fallback_location_id: &str,
        node_id: &str,
        stage_summary: String,
        opportunity_kind: &str,
        outcome_summary: String,
        feedback_focus: &str,
        opportunity_hint: String,
        opportunity_playbook: &str,
    ) -> Self {
        self.route_next_task_id =
            route_task_id_or_fallback(&self.route_next_task_id, fallback_task_id);
        self.route_next_action_label = action_label.to_string();
        self.route_next_panel_id = panel_id.to_string();
        self.route_next_command_hint = matrix_route_playability_anchor_command(command_hint);
        self.route_next_location_id =
            route_location_or_fallback(&self.route_next_location_id, fallback_location_id);
        self.route_next_node_id = node_id.to_string();
        self.route_next_opportunity_node_id = node_id.to_string();
        self.route_next_stage_summary = stage_summary;
        self.route_next_opportunity_kind = opportunity_kind.to_string();
        self.route_next_outcome_summary = outcome_summary;
        self.route_next_feedback_focus = feedback_focus.to_string();
        self.route_next_opportunity_hint = opportunity_hint;
        self.route_next_opportunity_playbook = opportunity_playbook.to_string();
        self.route_next_opportunity_command = self.route_next_command_hint.clone();
        self.route_next_opportunity_target = route_opportunity_target_from_command(
            &self.route_next_opportunity_command,
            &self.route_next_opportunity_node_id,
        );
        self
    }
}

fn route_task_id_or_fallback(route_next_task_id: &str, fallback_task_id: &str) -> String {
    if route_next_task_id.is_empty() || route_next_task_id == "none" {
        fallback_task_id.to_string()
    } else {
        route_next_task_id.to_string()
    }
}

fn route_story_card_json(
    mut card: Value,
    route: &RouteStoryCardContext,
    include_task_linked_count: bool,
) -> Value {
    let Some(card_object) = card.as_object_mut() else {
        return card;
    };

    card_object.insert("route_story".to_string(), route.to_value());
    card_object.insert(
        "route_preview_item_count".to_string(),
        json!(route.route_preview_item_count),
    );
    if include_task_linked_count {
        card_object.insert(
            "route_task_linked_count".to_string(),
            json!(route.route_task_linked_count),
        );
    }
    card_object.insert(
        "route_task_graph_count".to_string(),
        json!(route.route_task_graph_count),
    );
    card_object.insert(
        "route_next_task_id".to_string(),
        json!(&route.route_next_task_id),
    );
    card_object.insert(
        "route_next_action_label".to_string(),
        json!(&route.route_next_action_label),
    );
    card_object.insert(
        "route_next_panel_id".to_string(),
        json!(&route.route_next_panel_id),
    );
    card_object.insert(
        "route_next_command_hint".to_string(),
        json!(&route.route_next_command_hint),
    );
    card_object.insert(
        "route_next_location_id".to_string(),
        json!(&route.route_next_location_id),
    );
    card_object.insert(
        "route_next_node_id".to_string(),
        json!(&route.route_next_node_id),
    );
    card_object.insert(
        "route_next_opportunity_node_id".to_string(),
        json!(&route.route_next_opportunity_node_id),
    );
    card_object.insert(
        "route_next_stage_summary".to_string(),
        json!(&route.route_next_stage_summary),
    );
    card_object.insert(
        "route_next_outcome_summary".to_string(),
        json!(&route.route_next_outcome_summary),
    );
    card_object.insert(
        "route_next_feedback_focus".to_string(),
        json!(&route.route_next_feedback_focus),
    );
    card_object.insert(
        "route_next_opportunity_hint".to_string(),
        json!(&route.route_next_opportunity_hint),
    );
    card_object.insert(
        "route_next_opportunity_playbook".to_string(),
        json!(&route.route_next_opportunity_playbook),
    );
    card_object.insert(
        "route_next_opportunity_kind".to_string(),
        json!(&route.route_next_opportunity_kind),
    );
    card_object.insert(
        "route_next_opportunity_command".to_string(),
        json!(&route.route_next_opportunity_command),
    );
    card_object.insert(
        "route_next_opportunity_action_label".to_string(),
        json!(&route.route_next_opportunity_target.action_label),
    );
    card_object.insert(
        "route_next_opportunity_panel_id".to_string(),
        json!(&route.route_next_opportunity_target.panel_id),
    );
    card_object.insert(
        "route_next_opportunity_input_id".to_string(),
        json!(&route.route_next_opportunity_target.input_id),
    );
    card_object.insert(
        "route_next_opportunity_input_value".to_string(),
        json!(&route.route_next_opportunity_target.input_value),
    );
    card_object.insert(
        "route_next_opportunity_textarea_id".to_string(),
        json!(&route.route_next_opportunity_target.textarea_id),
    );
    card_object.insert(
        "route_next_opportunity_body".to_string(),
        json!(&route.route_next_opportunity_target.body),
    );
    card_object.insert(
        "route_next_opportunity_target_node_id".to_string(),
        json!(&route.route_next_opportunity_target.node_id),
    );

    card
}

fn route_location_or_fallback(route_next_location_id: &str, fallback_location_id: &str) -> String {
    if route_next_location_id.is_empty() {
        fallback_location_id.to_string()
    } else {
        route_next_location_id.to_string()
    }
}

fn world_context_node_id(value: &Value) -> String {
    value
        .get("current_node_id")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("current_node")
                .and_then(|node| node.get("node_id"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            value
                .get("map")
                .and_then(|map| map.get("current_node_id"))
                .and_then(Value::as_str)
        })
        .unwrap_or("mirror-city-square")
        .to_string()
}

fn resolve_route_focus_nodes(
    route_next_panel_id: &str,
    route_next_node_id: &str,
    route_next_opportunity_node_id: &str,
    fallback_node_id: &str,
) -> (String, String) {
    let panel_default = route_focus_panel_default_node_id(route_next_panel_id);
    let fallback = if !fallback_node_id.trim().is_empty() {
        fallback_node_id.trim()
    } else if !panel_default.is_empty() {
        panel_default
    } else {
        "mirror-city-square"
    };
    let route_next_node_id = if route_next_node_id.trim().is_empty() {
        fallback.to_string()
    } else {
        route_next_node_id.to_string()
    };
    let route_next_opportunity_node_id = if route_next_opportunity_node_id.trim().is_empty() {
        if !panel_default.is_empty() {
            panel_default.to_string()
        } else {
            route_next_node_id.clone()
        }
    } else {
        route_next_opportunity_node_id.to_string()
    };
    (route_next_node_id, route_next_opportunity_node_id)
}

#[allow(clippy::too_many_arguments)]
fn specialize_route_opportunity(
    surface: &str,
    route_next_opportunity_kind: &str,
    route_next_outcome_summary: &str,
    route_next_feedback_focus: &str,
    route_next_opportunity_hint: &str,
    route_next_opportunity_playbook: &str,
    route_next_opportunity_command: &str,
    route_next_task_id: &str,
    route_next_location_id: &str,
    route_next_stage_summary: &str,
) -> (String, String, String) {
    let route_anchor = if route_next_location_id.is_empty() {
        "the active route"
    } else {
        route_next_location_id
    };
    let task_ref = if route_next_task_id.is_empty() || route_next_task_id == "none" {
        "current-route"
    } else {
        route_next_task_id
    };
    let is_growth_kind = matches!(
        route_next_opportunity_kind,
        "repeat_order_upsell_referral" | "acceptance_upsell"
    );
    let is_recovery_kind = matches!(
        route_next_opportunity_kind,
        "revision_recovery"
            | "revision_reopen"
            | "reopen_recovery"
            | "rejection_refund_recovery"
            | "rejection_chargeback_recovery"
            | "cancellation_settlement_recovery"
            | "smaller_scope_requalification"
    );

    let (hint, playbook, command) = match surface {
        "assets" => {
            let hint = if is_growth_kind {
                format!(
                    "Assetize {} into the next premium upgrade pack for {}.",
                    route_next_outcome_summary, route_anchor
                )
            } else if is_recovery_kind {
                format!(
                    "Turn {} into a safer asset kit with tighter QA and starter scope for {}.",
                    route_next_outcome_summary, route_anchor
                )
            } else {
                format!(
                    "Turn {} into reusable templates, proof kits, and delivery assets for {}.",
                    route_next_outcome_summary, route_anchor
                )
            };
            let playbook = format!(
                "{} Then assetize it: extract templates, evidence snippets, QA checklists, pricing notes, and a reusable upgrade pack. {}",
                route_next_opportunity_playbook, route_next_feedback_focus
            );
            let command = if is_growth_kind {
                format!(
                    "/upgrade latest 资产化升级方案：围绕 {} 沉淀模板、evidence kit、QA checklist、pricing proof、复购钩子和 next action。",
                    route_next_outcome_summary
                )
            } else if is_recovery_kind {
                format!(
                    "/upgrade latest 风险收敛资产包：围绕 {} 补齐 QA gate、evidence gap、starter scope、acceptance checklist 和 next action。",
                    route_next_outcome_summary
                )
            } else {
                format!(
                    "/upgrade latest 交付资产包：围绕 {} 提炼模板、proof、handoff checklist、pricing note 和下一步。",
                    route_next_outcome_summary
                )
            };
            (hint, playbook, command)
        }
        "companies" => {
            let hint = if is_growth_kind {
                format!(
                    "Turn {} into the next company growth lane for {}.",
                    route_next_outcome_summary, route_anchor
                )
            } else if is_recovery_kind {
                format!(
                    "Turn {} into a requalification and repricing plan for {}.",
                    route_next_outcome_summary, route_anchor
                )
            } else {
                format!(
                    "Turn {} into the next operating plan, offer ladder, or service motion for {}.",
                    route_next_outcome_summary, route_anchor
                )
            };
            let playbook = format!(
                "{} Then route it through the company layer: define ICP, offer ladder, delivery cadence, owner, and referral or retention loop. {}",
                route_next_opportunity_playbook, route_next_feedback_focus
            );
            let command = if is_growth_kind {
                format!(
                    "/company latest 公司增长方案：基于 {} 设计 offer、ICP、price ladder、proof、delivery cadence、referral path 和 next step。",
                    route_next_outcome_summary
                )
            } else if is_recovery_kind {
                format!(
                    "/company latest 重定价与资格筛选方案：围绕 {} 重做 scope、risk gate、acceptance bar、refund policy 和 next step。",
                    route_next_outcome_summary
                )
            } else {
                format!(
                    "/company latest 服务交付经营方案：围绕 {} 设计 owner、offer、ops cadence、proof loop 和 next step。",
                    route_next_outcome_summary
                )
            };
            (hint, playbook, command)
        }
        "shops" | "listing" => {
            let hint = if is_growth_kind {
                format!(
                    "Turn {} into the next market-ready upsell or repeat-order listing for {}.",
                    route_next_outcome_summary, route_anchor
                )
            } else if is_recovery_kind {
                format!(
                    "Turn {} into a smaller starter offer or reprice path for {}.",
                    route_next_outcome_summary, route_anchor
                )
            } else {
                format!(
                    "Turn {} into the next market-facing listing or add-on package for {}.",
                    route_next_outcome_summary, route_anchor
                )
            };
            let playbook = format!(
                "{} Then package it for the market: define scope, evidence, price, acceptance bar, upsell ladder, and the next CTA. Stage: {}.",
                route_next_opportunity_playbook, route_next_stage_summary
            );
            let command = if is_growth_kind {
                format!(
                    "/sell latest 复购/升级方案：基于 {} 提供下一阶段 deliverable、evidence、price、acceptance standard、timeline 和推荐理由。",
                    route_next_outcome_summary
                )
            } else if is_recovery_kind {
                format!(
                    "/sell latest 小范围试单/重报价方案：围绕 {} 缩 scope、补 evidence、重设 acceptance standard、price 和 next step。",
                    route_next_outcome_summary
                )
            } else {
                format!(
                    "/sell latest 商机转化方案：基于 {} 输出 deliverable、evidence、price、acceptance standard、timeline、upsell/repeat angle。",
                    route_next_outcome_summary
                )
            };
            (hint, playbook, command)
        }
        "app" | "world" | "map" | "purchase" | "work" => {
            let hint = if is_growth_kind {
                format!(
                    "Route {} into the next growth lane via World Action for {}.",
                    route_next_outcome_summary, route_anchor
                )
            } else if is_recovery_kind {
                format!(
                    "Route {} into a recovery lane via World Action for {}.",
                    route_next_outcome_summary, route_anchor
                )
            } else {
                format!(
                    "Route {} into the next execution lane via World Action for {}.",
                    route_next_outcome_summary, route_anchor
                )
            };
            let playbook = format!(
                "{} Then push it through World Action: choose asset/company/listing/follow-up lane, assign owner, risk gate, proof pack, and next command. Stage: {}.",
                route_next_opportunity_playbook, route_next_stage_summary
            );
            let command = if is_growth_kind {
                format!(
                    "/world action 增长机会推进任务 {}：围绕 {} 选择 repeat order、upsell、referral lane，整理 owner、proof、offer、risk 和 next command。",
                    task_ref, route_next_outcome_summary
                )
            } else if is_recovery_kind {
                format!(
                    "/world action 挽回机会推进任务 {}：围绕 {} 重做 qualification、scope、proof、risk gate 和 next command。",
                    task_ref, route_next_outcome_summary
                )
            } else {
                format!(
                    "/world action 执行机会推进任务 {}：围绕 {} 选择 asset/company/listing/follow-up lane，整理 owner、proof、offer、risk 和 next command。",
                    task_ref, route_next_outcome_summary
                )
            };
            (hint, playbook, command)
        }
        _ => (
            route_next_opportunity_hint.to_string(),
            route_next_opportunity_playbook.to_string(),
            route_next_opportunity_command.to_string(),
        ),
    };
    (
        hint,
        playbook,
        matrix_route_playability_anchor_command(command),
    )
}

#[derive(Debug, Clone)]
struct MapRendererAdapterCardContext {
    adapter_id: String,
    adapter_contract_version: u64,
    runtime_handle_name: String,
    future_engine_candidate: String,
    supports_future_engine_swap: bool,
    planned_upgrade_engine_id: String,
    planned_upgrade_gating_contract: String,
}

impl MapRendererAdapterCardContext {
    fn from_engine(map_engine: Option<&Value>) -> Self {
        let adapter = map_engine.and_then(|engine| engine.get("renderer_adapter"));
        let adapter_contract = adapter.and_then(|adapter| adapter.get("adapter_contract"));
        let planned_upgrade = map_engine.and_then(|engine| engine.get("planned_upgrade_engine"));
        Self {
            adapter_id: adapter
                .and_then(|adapter| adapter.get("adapter_id"))
                .and_then(Value::as_str)
                .unwrap_or("leaflet_renderer_adapter_v1")
                .to_string(),
            adapter_contract_version: adapter
                .and_then(|adapter| adapter.get("adapter_contract_version"))
                .and_then(Value::as_u64)
                .unwrap_or(1),
            runtime_handle_name: adapter
                .and_then(|adapter| adapter.get("runtime_handle_name"))
                .and_then(Value::as_str)
                .unwrap_or("mapRuntime")
                .to_string(),
            future_engine_candidate: adapter
                .and_then(|adapter| adapter.get("future_engine_candidate"))
                .and_then(Value::as_str)
                .unwrap_or("maplibre_gl_v1")
                .to_string(),
            supports_future_engine_swap: adapter_contract
                .and_then(|contract| contract.get("supports_future_engine_swap"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            planned_upgrade_engine_id: planned_upgrade
                .and_then(|planned| planned.get("engine_id"))
                .and_then(Value::as_str)
                .unwrap_or("maplibre_gl_v1")
                .to_string(),
            planned_upgrade_gating_contract: planned_upgrade
                .and_then(|planned| planned.get("gating_contract"))
                .and_then(Value::as_str)
                .unwrap_or("renderer_adapter.adapter_contract_version >= 1")
                .to_string(),
        }
    }
}

fn build_trillionnium_world_matrix_reply(value: &Value) -> Value {
    let counts = value.get("counts").unwrap_or(value);
    let zone_count = counts.get("zones").and_then(Value::as_u64).unwrap_or(4);
    let location_count = counts.get("locations").and_then(Value::as_u64).unwrap_or(4);
    let asset_count = counts.get("assets").and_then(Value::as_u64).unwrap_or(0);
    let company_count = counts.get("companies").and_then(Value::as_u64).unwrap_or(0);
    let shop_count = counts.get("shops").and_then(Value::as_u64).unwrap_or(0);
    let listing_count = counts.get("listings").and_then(Value::as_u64).unwrap_or(0);
    let purchase_count = counts.get("purchases").and_then(Value::as_u64).unwrap_or(0);
    let work_order_count = counts
        .get("work_orders")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let faction_count = counts.get("factions").and_then(Value::as_u64).unwrap_or(0);
    let event_count = counts.get("events").and_then(Value::as_u64).unwrap_or(0);
    let world_context_node_id = world_context_node_id(value);
    let map_engine = value.get("real_world_map_engine");
    let renderer_adapter = MapRendererAdapterCardContext::from_engine(map_engine);
    let route = RouteStoryCardContext::from_value(value, &world_context_node_id)
        .specialize_opportunity("world");
    let route_text_block = route.text_block("Route Graph", true);
    let route_html_block = route.html_block("Route Graph", true);
    let route_runner = RouteRunnerHandoffCardContext::from_map_hub(Some(value));
    let route_runner_text_block = route_runner.text_block();
    let route_runner_html_block = route_runner.html_block();
    let body = format!(
        "🌍 Trillionnium World\n开放世界总层：现实镜像城市 + Craft 工坊 + Market + League。\nZones: {zone_count} · Locations: {location_count} · Assets: {asset_count} · Companies: {company_count} · Shops: {shop_count} · Listings: {listing_count} · Purchases: {purchase_count} · Work: {work_order_count} · Factions: {faction_count} · Events: {event_count}\nRenderer Adapter: {adapter_id} v{adapter_version} · handle {runtime_handle} · future {future_engine}\n{route_text_block}\n{route_runner_text_block}\n自由行动：/world action 我要开一家 AI 设计公司",
        adapter_id = &renderer_adapter.adapter_id,
        adapter_version = renderer_adapter.adapter_contract_version,
        runtime_handle = &renderer_adapter.runtime_handle_name,
        future_engine = &renderer_adapter.future_engine_candidate,
        route_text_block = route_text_block,
        route_runner_text_block = route_runner_text_block,
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🌍 Trillionnium World</h3><p>现实镜像城市 + Craft 工坊 + Market + League。</p><p><strong>Zones</strong>: {} · <strong>Locations</strong>: {} · <strong>Assets</strong>: {} · <strong>Companies</strong>: {} · <strong>Shops</strong>: {} · <strong>Listings</strong>: {} · <strong>Purchases</strong>: {} · <strong>Work</strong>: {} · <strong>Factions</strong>: {} · <strong>Events</strong>: {}</p><p><strong>Renderer Adapter</strong>: <code>{}</code> v{} · <code>{}</code> · future <code>{}</code></p>{}{}<p><code>/world action 我要开一家 AI 设计公司</code></p></blockquote>",
            zone_count, location_count, asset_count, company_count, shop_count, listing_count, purchase_count, work_order_count, faction_count, event_count, escape_html(&renderer_adapter.adapter_id), renderer_adapter.adapter_contract_version, escape_html(&renderer_adapter.runtime_handle_name), escape_html(&renderer_adapter.future_engine_candidate), route_html_block, route_runner_html_block,
        ),
        "cex_card": route_runner.card_json(route_story_card_json(json!({"type": "trillionnium_world", "version": 1, "world": "trillionnium_world", "zone_count": zone_count, "location_count": location_count, "asset_count": asset_count, "company_count": company_count, "shop_count": shop_count, "listing_count": listing_count, "purchase_count": purchase_count, "work_order_count": work_order_count, "faction_count": faction_count, "event_count": event_count, "map_renderer_adapter_id": renderer_adapter.adapter_id, "map_renderer_adapter_version": renderer_adapter.adapter_contract_version, "map_runtime_handle_name": renderer_adapter.runtime_handle_name, "map_renderer_future_engine_candidate": renderer_adapter.future_engine_candidate, "map_renderer_supports_future_engine_swap": renderer_adapter.supports_future_engine_swap, "map_planned_upgrade_engine_id": renderer_adapter.planned_upgrade_engine_id, "map_planned_upgrade_gating_contract": renderer_adapter.planned_upgrade_gating_contract}), &route, true))
    })
}

fn build_trillionnium_world_map_matrix_reply(value: &Value) -> Value {
    let counts = value.get("counts").unwrap_or(value);
    let node_count = counts.get("map_nodes").and_then(Value::as_u64).unwrap_or(0);
    let current_node = value.get("current_node").unwrap_or(value);
    let current_node_id = value
        .get("current_node_id")
        .and_then(Value::as_str)
        .or_else(|| current_node.get("node_id").and_then(Value::as_str))
        .unwrap_or("mirror-city-square");
    let current_name = current_node
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("镜像城市广场");
    let description = current_node
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("World map is booting.");
    let x = current_node.get("x").and_then(Value::as_i64).unwrap_or(0);
    let y = current_node.get("y").and_then(Value::as_i64).unwrap_or(0);
    let map_engine = value.get("real_world_map_engine");
    let map_engine_id = map_engine
        .and_then(|engine| engine.get("engine_id"))
        .and_then(Value::as_str)
        .unwrap_or("leaflet_openstreetmap_v1");
    let tile_provider = map_engine
        .and_then(|engine| engine.get("tile_provider"))
        .and_then(Value::as_str)
        .unwrap_or("OpenStreetMap");
    let mirror_scope = map_engine
        .and_then(|engine| engine.get("mirror_scope"))
        .and_then(Value::as_str)
        .unwrap_or("global_real_world_tiles");
    let active_region_id = map_engine
        .and_then(|engine| engine.get("active_region_id"))
        .and_then(Value::as_str)
        .unwrap_or("cn-shanghai-core");
    let renderer_adapter = MapRendererAdapterCardContext::from_engine(map_engine);
    let route =
        RouteStoryCardContext::from_value(value, current_node_id).specialize_opportunity("map");
    let route_text_block = route.text_block("Route Graph", false);
    let route_html_block = route.html_block("Route Graph", false);
    let route_runner = RouteRunnerHandoffCardContext::from_map_hub(Some(value));
    let route_runner_text_block = route_runner.text_block();
    let route_runner_html_block = route_runner.html_block();
    let combat_log = value
        .get("tactics_board")
        .and_then(|board| board.get("combat_log"));
    let combat_log_contract = combat_log
        .and_then(|log| log.get("contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_combat_log_v1");
    let combat_log_style = combat_log
        .and_then(|log| log.get("style"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_wuxia_log_v1");
    let combat_log_beats = combat_log
        .and_then(|log| log.get("beats"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let combat_log_beat_count = combat_log_beats.len();
    let tactics_board = value.get("tactics_board");
    let osm_objective_contract = tactics_board
        .and_then(|board| board.get("trillionnium_osm_objective_contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_osm_objective_v1");
    let osm_objective_count = tactics_board
        .and_then(|board| board.get("osm_objectives"))
        .and_then(Value::as_array)
        .map(|objectives| objectives.len())
        .unwrap_or(0);
    let npc_relationship_contract = tactics_board
        .and_then(|board| board.get("trillionnium_npc_relationship_contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_npc_relationship_v1");
    let tactics_combat_resolution_contract = tactics_board
        .and_then(|board| board.get("tactics_combat_resolution_contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_combat_resolution_v1");
    let tactics_game_session_contract = tactics_board
        .and_then(|board| board.get("tactics_game_session_contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_game_session_v1");
    let tactics_reward_settlement_contract = tactics_board
        .and_then(|board| board.get("tactics_reward_settlement_contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_reward_settlement_v1");
    let tactics_game_session = tactics_board.and_then(|board| board.get("game_session"));
    let tactics_session_id = tactics_board
        .and(tactics_game_session)
        .and_then(|session| session.get("session_id"))
        .and_then(Value::as_str)
        .unwrap_or("world-tactics-session:projected");
    let tactics_session_persistence_status = tactics_board
        .and(tactics_game_session)
        .and_then(|session| session.get("persistence_status"))
        .and_then(Value::as_str)
        .unwrap_or("projected_default_until_first_command");
    let tactics_objective_progress = tactics_game_session
        .and_then(|session| session.get("objective_progress"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let tactics_objective_goal = tactics_game_session
        .and_then(|session| session.get("objective_goal"))
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let tactics_victory_state = tactics_game_session
        .and_then(|session| session.get("victory_state"))
        .and_then(Value::as_str)
        .unwrap_or("active");
    let tactics_reward_status = tactics_game_session
        .and_then(|session| session.get("reward_status"))
        .and_then(Value::as_str)
        .unwrap_or("not_eligible");
    let tactics_simulation_tick_contract = tactics_board
        .and_then(|board| board.get("tactics_simulation_tick_contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_tactics_simulation_tick_v1");
    let tactics_simulation_tick_count = tactics_board
        .and_then(|board| board.get("simulation_ticks"))
        .and_then(Value::as_array)
        .map(|ticks| ticks.len())
        .unwrap_or(0);
    let map_overlay_identity_contract = tactics_board
        .and_then(|board| board.get("map_overlay_identity_contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_map_overlay_identity_v1");
    let map_overlay_identity_count = tactics_board
        .and_then(|board| board.get("map_overlay_identity_index"))
        .and_then(Value::as_array)
        .map(|identities| identities.len())
        .unwrap_or(0);
    let combat_log_preview = combat_log_beats
        .iter()
        .take(2)
        .filter_map(|beat| beat.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" / ");
    let combat_log_text_block = if combat_log_preview.is_empty() {
        String::new()
    } else {
        format!(
            "Trillionnium战报: {combat_log_style} · {combat_log_beat_count} beats · {combat_log_preview}\n"
        )
    };
    let combat_log_html_block = if combat_log_preview.is_empty() {
        String::new()
    } else {
        format!(
            "<p><strong>Trillionnium战报</strong>: <code>{}</code> · {} beats · {}</p>",
            escape_html(combat_log_style),
            combat_log_beat_count,
            escape_html(&combat_log_preview)
        )
    };
    let exits = value
        .get("exits")
        .and_then(Value::as_object)
        .map(|object| {
            let mut pairs: Vec<String> = object
                .iter()
                .map(|(direction, node_id)| {
                    format!("{}→{}", direction, node_id.as_str().unwrap_or("unknown"))
                })
                .collect();
            pairs.sort();
            pairs.join(" / ")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "none".to_string());
    let body = format!(
        "🗺️ Trillionnium World Map\n当前位置：{current_name} ({current_node_id})\n坐标：{x},{y}\n节点：{node_count}\nMap Engine: {map_engine_id} ({tile_provider})\nMirror: {mirror_scope} · Region: {active_region_id}\nRenderer Adapter: {adapter_id} v{adapter_version} · handle {runtime_handle} · future {future_engine}\n{route_text_block}\n{route_runner_text_block}{combat_log_text_block}出口：{exits}\n{description}\n移动：/go <direction|node-id>",
        adapter_id = &renderer_adapter.adapter_id,
        adapter_version = renderer_adapter.adapter_contract_version,
        runtime_handle = &renderer_adapter.runtime_handle_name,
        future_engine = &renderer_adapter.future_engine_candidate,
        route_text_block = route_text_block,
        route_runner_text_block = route_runner_text_block,
        combat_log_text_block = combat_log_text_block,
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🗺️ Trillionnium World Map</h3><p><strong>当前位置</strong>: {} (<code>{}</code>)</p><p><strong>坐标</strong>: {},{} · <strong>节点</strong>: {}</p><p><strong>Map Engine</strong>: <code>{}</code> · {} · <code>{}</code></p><p><strong>Renderer Adapter</strong>: <code>{}</code> v{} · <code>{}</code> · future <code>{}</code></p>{}{}{}<p><strong>出口</strong>: {}</p><p>{}</p><p><code>/go &lt;direction|node-id&gt;</code></p></blockquote>",
            escape_html(current_name), escape_html(current_node_id), x, y, node_count, escape_html(map_engine_id), escape_html(tile_provider), escape_html(active_region_id), escape_html(&renderer_adapter.adapter_id), renderer_adapter.adapter_contract_version, escape_html(&renderer_adapter.runtime_handle_name), escape_html(&renderer_adapter.future_engine_candidate), route_html_block, route_runner_html_block, combat_log_html_block, escape_html(&exits), escape_html(description),
        ),
        "cex_card": route_runner.card_json(route_story_card_json(json!({"type": "trillionnium_world_map", "version": 1, "world": "trillionnium_world", "current_node_id": current_node_id, "current_name": current_name, "node_count": node_count, "x": x, "y": y, "exits": exits, "has_real_world_map_engine": true, "map_engine_id": map_engine_id, "tile_provider": tile_provider, "mirror_scope": mirror_scope, "active_region_id": active_region_id, "map_renderer_adapter_id": renderer_adapter.adapter_id, "map_renderer_adapter_version": renderer_adapter.adapter_contract_version, "map_runtime_handle_name": renderer_adapter.runtime_handle_name, "map_renderer_future_engine_candidate": renderer_adapter.future_engine_candidate, "map_renderer_supports_future_engine_swap": renderer_adapter.supports_future_engine_swap, "map_planned_upgrade_engine_id": renderer_adapter.planned_upgrade_engine_id, "map_planned_upgrade_gating_contract": renderer_adapter.planned_upgrade_gating_contract, "has_trillionnium_combat_log": combat_log_beat_count > 0, "trillionnium_combat_log_contract": combat_log_contract, "trillionnium_combat_log_style": combat_log_style, "trillionnium_combat_log_beat_count": combat_log_beat_count, "trillionnium_combat_log_matrix_projection": true, "trillionnium_osm_objective_contract": osm_objective_contract, "trillionnium_osm_objective_count": osm_objective_count, "trillionnium_npc_relationship_contract": npc_relationship_contract, "tactics_combat_resolution_contract": tactics_combat_resolution_contract, "tactics_game_session_contract": tactics_game_session_contract, "tactics_session_id": tactics_session_id, "tactics_session_persistence_status": tactics_session_persistence_status, "tactics_objective_progress": tactics_objective_progress, "tactics_objective_goal": tactics_objective_goal, "tactics_victory_state": tactics_victory_state, "tactics_reward_status": tactics_reward_status, "tactics_reward_settlement_contract": tactics_reward_settlement_contract, "tactics_simulation_tick_contract": tactics_simulation_tick_contract, "tactics_simulation_tick_count": tactics_simulation_tick_count, "map_overlay_identity_contract": map_overlay_identity_contract, "map_overlay_identity_count": map_overlay_identity_count}), &route, false))
    })
}

fn build_trillionnium_world_map_move_matrix_reply(value: &Value) -> Value {
    let from_node = value.get("from_node").unwrap_or(value);
    let to_node = value.get("to_node").unwrap_or(value);
    let position = value.get("position").unwrap_or(value);
    let from_node_id = from_node
        .get("node_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let to_node_id = to_node
        .get("node_id")
        .and_then(Value::as_str)
        .unwrap_or("mirror-city-square");
    let to_name = to_node
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("镜像城市广场");
    let location_id = position
        .get("location_id")
        .and_then(Value::as_str)
        .or_else(|| to_node.get("location_id").and_then(Value::as_str))
        .unwrap_or("mirror-city-square");
    let x = to_node.get("x").and_then(Value::as_i64).unwrap_or(0);
    let y = to_node.get("y").and_then(Value::as_i64).unwrap_or(0);
    let body = format!(
        "🚶 World Move\nFrom: {from_node_id}\nTo: {to_name} ({to_node_id})\nLocation: {location_id}\n坐标：{x},{y}\n查看：/map"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🚶 World Move</h3><p><strong>From</strong>: <code>{}</code></p><p><strong>To</strong>: {} (<code>{}</code>)</p><p><strong>Location</strong>: <code>{}</code> · <strong>坐标</strong>: {},{}</p><p><code>/map</code></p></blockquote>",
            escape_html(from_node_id), escape_html(to_name), escape_html(to_node_id), escape_html(location_id), x, y,
        ),
        "cex_card": {"type": "trillionnium_world_map_move", "version": 1, "world": "trillionnium_world", "from_node_id": from_node_id, "to_node_id": to_node_id, "to_name": to_name, "location_id": location_id, "x": x, "y": y}
    })
}

fn build_trillionnium_client_app_matrix_reply(value: &Value) -> Value {
    let modules = value
        .get("modules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let module_count = value
        .get("module_count")
        .and_then(Value::as_u64)
        .unwrap_or(modules.len() as u64);
    let module_names: Vec<String> = modules
        .iter()
        .filter_map(|module| module.get("name").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect();
    let map_node = value
        .get("map")
        .and_then(|map| map.get("current_node_id"))
        .and_then(Value::as_str)
        .unwrap_or("mirror-city-square");
    let map_engine = value.get("real_world_map_engine").or_else(|| {
        value
            .get("map")
            .and_then(|map| map.get("real_world_map_engine"))
    });
    let map_engine_id = map_engine
        .and_then(|engine| engine.get("engine_id"))
        .and_then(Value::as_str)
        .unwrap_or("leaflet_openstreetmap_v1");
    let tile_provider = map_engine
        .and_then(|engine| engine.get("tile_provider"))
        .and_then(Value::as_str)
        .unwrap_or("OpenStreetMap");
    let renderer_adapter = MapRendererAdapterCardContext::from_engine(map_engine);
    let active_region_id = value
        .get("map_hub")
        .and_then(|hub| hub.get("active_region_id"))
        .and_then(Value::as_str)
        .unwrap_or("cn-shanghai-core");
    let nearby_poi_count = value
        .get("map_hub")
        .and_then(|hub| hub.get("nearby_poi_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let tile_shard_count = value
        .get("map_hub")
        .and_then(|hub| hub.get("tile_shard_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let prefetch_count = value
        .get("map_hub")
        .and_then(|hub| hub.get("prefetch_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let live_event_count = value
        .get("map_hub")
        .and_then(|hub| hub.get("live_event_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let player_density_mode = value
        .get("map_hub")
        .and_then(|hub| hub.get("player_density_mode"))
        .and_then(Value::as_str)
        .unwrap_or("dense");
    let map_hub = value.get("map_hub").cloned().unwrap_or(Value::Null);
    let route = RouteStoryCardContext::from_value(&map_hub, map_node).specialize_opportunity("app");
    let route_runner = RouteRunnerHandoffCardContext::from_map_hub(Some(&map_hub));
    let route_runner_text_block = route_runner.text_block();
    let route_runner_html_block = route_runner.html_block();
    let primary_entry_module_id = value
        .get("primary_entry_module_id")
        .and_then(Value::as_str)
        .unwrap_or("world_map");
    let progression = value.get("progression");
    let progression_level = progression
        .and_then(|progression| progression.get("level"))
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let progression_rank = progression
        .and_then(|progression| progression.get("rank_title"))
        .and_then(Value::as_str)
        .unwrap_or("Apprentice");
    let successful_task_count = progression
        .and_then(|progression| progression.get("successful_task_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unlocked_skill_count = progression
        .and_then(|progression| progression.get("unlocked_skill_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unlocked_tool_count = progression
        .and_then(|progression| progression.get("unlocked_tool_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unlocked_skin_count = progression
        .and_then(|progression| progression.get("unlocked_skin_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let onboarding = value.get("onboarding");
    let onboarding_contract_version = onboarding
        .and_then(|rail| rail.get("contract_version"))
        .and_then(Value::as_str)
        .unwrap_or("trillionnium_first_playable_onboarding_v1");
    let onboarding_label = onboarding
        .and_then(|rail| rail.get("rail_label"))
        .and_then(Value::as_str)
        .unwrap_or("新手主线：从地图到成交");
    let onboarding_completion_target = onboarding
        .and_then(|rail| rail.get("completion_target"))
        .and_then(Value::as_str)
        .unwrap_or("first_playable_loop_100");
    let onboarding_step_count = onboarding
        .and_then(|rail| rail.get("steps"))
        .and_then(Value::as_array)
        .map(|steps| steps.len() as u64)
        .unwrap_or(0);
    let onboarding_start_command = onboarding
        .and_then(|rail| rail.get("steps"))
        .and_then(Value::as_array)
        .and_then(|steps| {
            steps.iter().find_map(|step| {
                (step.get("step_id").and_then(Value::as_str) == Some("start_world_action"))
                    .then(|| step.get("command").and_then(Value::as_str))
                    .flatten()
            })
        })
        .unwrap_or("/world action Start the first bounty: define customer deliverable, evidence package, risk controls, next action, and self-review.");
    let quick_path_label = onboarding
        .and_then(|rail| rail.get("quick_path_label"))
        .and_then(Value::as_str)
        .unwrap_or("Quick Path");
    let quick_path_summary = onboarding
        .and_then(|rail| rail.get("quick_path_summary"))
        .and_then(Value::as_str)
        .unwrap_or("Choose map focus → run one bounty → submit/review reward");
    let command_disclosure_label = onboarding
        .and_then(|rail| rail.get("command_disclosure_label"))
        .and_then(Value::as_str)
        .unwrap_or("Full Commands");
    let command_disclosure = onboarding
        .and_then(|rail| rail.get("command_disclosure"))
        .and_then(Value::as_str)
        .unwrap_or("Use these when you are ready to submit real work with deliverable, evidence, risk controls, next action, and self-review anchors.");
    let body = format!(
        "📱 Trillionnium Client App\n{quick_path_label}: {quick_path_summary}\nStart Here: {onboarding_label} · {onboarding_step_count} steps · target {onboarding_completion_target}\nStart Command: {onboarding_start_command}\nNow: {next_action} @ {next_location}\nWhy: {next_outcome}\n{command_disclosure_label}: {command_disclosure}\nNext Command: {next_command}\nOpportunity Command: {opportunity_command}\n{runner_text_block}\nWorld: {map_node} · {nearby_poi_count} POIs · {live_event_count} live events · {tile_shard_count} tiles\nProgression: Lv.{progression_level} {progression_rank} · {successful_task_count} successes · skills/tools/skins {unlocked_skill_count}/{unlocked_tool_count}/{unlocked_skin_count}\n入口：/map /duel nearby /social /wallet /progression",
        next_action = &route.route_next_action_label,
        next_location = &route.route_next_location_id,
        next_outcome = &route.route_next_outcome_summary,
        next_command = &route.route_next_command_hint,
        opportunity_command = &route.route_next_opportunity_command,
        runner_text_block = &route_runner_text_block,
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>📱 Trillionnium Client App</h3><p><strong>{}</strong>: {}</p><p><strong>Start Here</strong>: {} · {} steps · target <code>{}</code></p><p><strong>Start Command</strong>: <code>{}</code></p><p><strong>Now</strong>: {} @ <code>{}</code></p><p><strong>Why</strong>: {}</p><p><strong>{}</strong>: {}</p><p><strong>Next Command</strong>: <code>{}</code></p><p><strong>Opportunity Command</strong>: <code>{}</code></p>{}<p><strong>World</strong>: <code>{}</code> · {} POIs · {} live events · {} tiles</p><p><strong>Progression</strong>: Lv.{} {} · {} successes · skills/tools/skins {}/{}/{}</p><p><code>/map</code> <code>/duel nearby</code> <code>/social</code> <code>/wallet</code> <code>/progression</code></p></blockquote>",
            escape_html(quick_path_label), escape_html(quick_path_summary), escape_html(onboarding_label), onboarding_step_count, escape_html(onboarding_completion_target), escape_html(onboarding_start_command), escape_html(&route.route_next_action_label), escape_html(&route.route_next_location_id), escape_html(&route.route_next_outcome_summary), escape_html(command_disclosure_label), escape_html(command_disclosure), escape_html(&route.route_next_command_hint), escape_html(&route.route_next_opportunity_command), route_runner_html_block, escape_html(map_node), nearby_poi_count, live_event_count, tile_shard_count, progression_level, escape_html(progression_rank), successful_task_count, unlocked_skill_count, unlocked_tool_count, unlocked_skin_count,
        ),
        "cex_card": route_runner.card_json(route_story_card_json(json!({
            "type": "trillionnium_client_app",
            "version": 1,
            "client": "trillionnium_mobile_shell",
            "module_count": module_count,
            "modules": module_names,
            "primary_entry_module_id": primary_entry_module_id,
            "current_node_id": map_node,
            "has_world_map": true,
            "has_real_world_map_engine": true,
            "map_engine_id": map_engine_id,
            "tile_provider": tile_provider,
            "active_region_id": active_region_id,
            "map_renderer_adapter_id": renderer_adapter.adapter_id,
            "map_renderer_adapter_version": renderer_adapter.adapter_contract_version,
            "map_runtime_handle_name": renderer_adapter.runtime_handle_name,
            "map_renderer_future_engine_candidate": renderer_adapter.future_engine_candidate,
            "map_renderer_supports_future_engine_swap": renderer_adapter.supports_future_engine_swap,
            "map_planned_upgrade_engine_id": renderer_adapter.planned_upgrade_engine_id,
            "map_planned_upgrade_gating_contract": renderer_adapter.planned_upgrade_gating_contract,
            "tile_shard_count": tile_shard_count,
            "nearby_poi_count": nearby_poi_count,
            "prefetch_count": prefetch_count,
            "live_event_count": live_event_count,
            "player_density_mode": player_density_mode,
            "has_first_playable_onboarding": true,
            "onboarding_contract_version": onboarding_contract_version,
            "onboarding_label": onboarding_label,
            "onboarding_completion_target": onboarding_completion_target,
            "onboarding_step_count": onboarding_step_count,
            "onboarding_quick_path_label": quick_path_label,
            "onboarding_quick_path_summary": quick_path_summary,
            "onboarding_command_disclosure_label": command_disclosure_label,
            "onboarding_command_disclosure": command_disclosure,
            "onboarding_start_command": onboarding_start_command,
            "has_face_duel": true,
            "has_social": true,
            "has_wallet": true,
            "has_progression": true,
            "progression_level": progression_level,
            "successful_task_count": successful_task_count,
            "unlocked_skill_count": unlocked_skill_count,
            "unlocked_tool_count": unlocked_tool_count,
            "unlocked_skin_count": unlocked_skin_count
        }), &route, true))
    })
}

fn client_feed_group_count(items: &[Value], group: &str) -> usize {
    items
        .iter()
        .filter(|item| item.get("feed_group").and_then(Value::as_str) == Some(group))
        .count()
}

fn client_feed_matches_filter(item: &Value, filter: &str) -> bool {
    item.get("feed_group").and_then(Value::as_str) == Some(filter)
}

fn client_feed_filter_label(filter: Option<&str>) -> &'static str {
    match filter {
        Some("live_event") => "事件",
        Some("route_task") => "任务",
        Some("contract") => "委托",
        Some("completion") => "完成",
        Some("commerce") => "成交",
        Some("social") => "社交",
        _ => "全部",
    }
}

fn build_trillionnium_client_feed_matrix_reply(value: &Value, filter: Option<&str>) -> Value {
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let item_count = value
        .get("item_count")
        .and_then(Value::as_u64)
        .unwrap_or(items.len() as u64);
    let source_count = value
        .get("source_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let filter_label = client_feed_filter_label(filter);
    let visible_items = filter
        .map(|group| {
            items
                .iter()
                .filter(|item| client_feed_matches_filter(item, group))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| items.clone());
    let visible_item_count = visible_items.len() as u64;
    let active_region_id = value
        .get("active_region_id")
        .and_then(Value::as_str)
        .unwrap_or("cn-shanghai-core");
    let live_event_count = client_feed_group_count(&items, "live_event");
    let route_task_count = client_feed_group_count(&items, "route_task");
    let contract_count = client_feed_group_count(&items, "contract");
    let completion_count = client_feed_group_count(&items, "completion");
    let commerce_count = client_feed_group_count(&items, "commerce");
    let social_count = client_feed_group_count(&items, "social");
    let top_item = visible_items.first();
    let top_feed_kind = top_item
        .and_then(|item| item.get("feed_kind"))
        .and_then(Value::as_str)
        .unwrap_or("update");
    let top_feed_group = top_item
        .and_then(|item| item.get("feed_group"))
        .and_then(Value::as_str)
        .unwrap_or(top_feed_kind);
    let top_source = top_item
        .and_then(|item| item.get("source"))
        .and_then(Value::as_str)
        .unwrap_or("feed");
    let top_title = top_item
        .and_then(|item| item.get("title"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if filter.is_some() {
                "该分组暂时安静"
            } else {
                "动态等待中"
            }
        });
    let top_summary = top_item
        .and_then(|item| item.get("summary"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if filter.is_some() {
                "这个 feed 分组暂时没有新动态，先回到 /feed 全部视图或去 /world 推进下一步。"
            } else {
                "切到 /app 看完整 timeline，或在 /world 里推进下一个 live event。"
            }
        });
    let top_detail = top_item
        .and_then(|item| item.get("detail"))
        .and_then(Value::as_str)
        .unwrap_or("feed waiting");
    let top_action_label = top_item
        .and_then(|item| item.get("action_label"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if filter.is_some() {
                "回到 /feed 全部"
            } else {
                "打开 /app"
            }
        });
    let top_action_panel_id = top_item
        .and_then(|item| item.get("action_panel_id"))
        .and_then(Value::as_str)
        .unwrap_or("world-action-console");
    let top_action_input_id = top_item
        .and_then(|item| item.get("action_input_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let top_action_input_value = top_item
        .and_then(|item| item.get("action_input_value"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let top_action_textarea_id = top_item
        .and_then(|item| item.get("action_textarea_id"))
        .and_then(Value::as_str)
        .unwrap_or("world-action-body");
    let top_action_location_id = top_item
        .and_then(|item| item.get("action_location_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let top_action_target_node_id = top_item
        .and_then(|item| item.get("action_target_node_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let top_action_task_id = top_item
        .and_then(|item| item.get("action_task_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let top_action_contract_id = top_item
        .and_then(|item| item.get("action_contract_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let top_action_listing_id = top_item
        .and_then(|item| item.get("action_listing_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let top_action_work_order_id = top_item
        .and_then(|item| item.get("action_work_order_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let fallback_node = if top_action_target_node_id.trim().is_empty() {
        "client-board"
    } else {
        top_action_target_node_id
    };
    let route =
        RouteStoryCardContext::from_value(value, fallback_node).specialize_opportunity("app");
    let route_text_block = route.text_block("Route Cockpit", true);
    let route_html_block = route.html_block("Route Cockpit", true);
    let route_runner = RouteRunnerHandoffCardContext::from_map_hub(Some(value));
    let route_runner_text_block = route_runner.text_block();
    let route_runner_html_block = route_runner.html_block();
    let body = format!(
        "📰 Trillionnium Feed\nView: {filter_label} · Visible {visible_item_count}/{item_count}\nSources: {source_count} · Region: {active_region_id}\n分组：事件 {live_event_count} · 任务 {route_task_count} · 委托 {contract_count} · 完成 {completion_count} · 成交 {commerce_count} · 社交 {social_count}\nTop Signal: {top_title}\nSignal: {top_feed_group} / {top_source} · {top_detail}\nSummary: {top_summary}\nAction: {top_action_label} @ {top_action_panel_id}\n{route_text_block}\n{route_runner_text_block}\n入口：/feed tasks /feed commerce /feed social /app"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>📰 Trillionnium Feed</h3><p><strong>View</strong>: {} · <strong>Visible</strong>: {}/{} · <strong>Sources</strong>: {} · <strong>Region</strong>: <code>{}</code></p><p><strong>分组</strong>: 事件 {} · 任务 {} · 委托 {} · 完成 {} · 成交 {} · 社交 {}</p><p><strong>Top Signal</strong>: {}</p><p><strong>Signal</strong>: <code>{}</code> / <code>{}</code> · {}</p><p><strong>Summary</strong>: {}</p><p><strong>Action</strong>: {} @ <code>{}</code></p>{}{}<p><code>/feed tasks</code> <code>/feed commerce</code> <code>/feed social</code> <code>/app</code></p></blockquote>",
            escape_html(filter_label),
            visible_item_count,
            item_count,
            source_count,
            escape_html(active_region_id),
            live_event_count,
            route_task_count,
            contract_count,
            completion_count,
            commerce_count,
            social_count,
            escape_html(top_title),
            escape_html(top_feed_group),
            escape_html(top_source),
            escape_html(top_detail),
            escape_html(top_summary),
            escape_html(top_action_label),
            escape_html(top_action_panel_id),
            route_html_block,
            route_runner_html_block,
        ),
        "cex_card": route_runner.card_json(route_story_card_json(json!({
            "type": "trillionnium_client_feed",
            "version": 1,
            "client": "trillionnium_mobile_shell",
            "feed_filter": filter,
            "feed_filter_label": filter_label,
            "item_count": item_count,
            "visible_item_count": visible_item_count,
            "source_count": source_count,
            "active_region_id": active_region_id,
            "live_event_feed_count": live_event_count,
            "route_task_feed_count": route_task_count,
            "contract_feed_count": contract_count,
            "completion_feed_count": completion_count,
            "commerce_feed_count": commerce_count,
            "social_feed_count": social_count,
            "top_feed_kind": top_feed_kind,
            "top_feed_group": top_feed_group,
            "top_source": top_source,
            "top_title": top_title,
            "top_summary": top_summary,
            "top_detail": top_detail,
            "top_action_label": top_action_label,
            "top_action_panel_id": top_action_panel_id,
            "top_action_input_id": top_action_input_id,
            "top_action_input_value": top_action_input_value,
            "top_action_textarea_id": top_action_textarea_id,
            "top_action_location_id": top_action_location_id,
            "top_action_target_node_id": top_action_target_node_id,
            "top_action_task_id": top_action_task_id,
            "top_action_contract_id": top_action_contract_id,
            "top_action_listing_id": top_action_listing_id,
            "top_action_work_order_id": top_action_work_order_id
        }), &route, true))
    })
}

fn build_trillionnium_client_social_matrix_reply(value: &Value) -> Value {
    let social = value.get("social").unwrap_or(value);
    let contact_count = social
        .get("contact_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let guild_count = social
        .get("guild_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let nearby_agents = value
        .get("nearby_agents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let top_contact = nearby_agents
        .first()
        .and_then(|agent| agent.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("Oracle Scout");
    let body = format!(
        "💬 Trillionnium Social\nContacts: {contact_count}\nGuilds: {guild_count}\nTop Contact: {top_contact}\n风格：WeChat / Telegram rooms + Agent contacts"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>💬 Trillionnium Social</h3><p><strong>Contacts</strong>: {} · <strong>Guilds</strong>: {}</p><p><strong>Top Contact</strong>: {}</p><p>WeChat / Telegram rooms + Agent contacts.</p></blockquote>",
            contact_count, guild_count, escape_html(top_contact),
        ),
        "cex_card": {"type": "trillionnium_client_social", "version": 1, "client": "trillionnium_mobile_shell", "contact_count": contact_count, "guild_count": guild_count, "top_contact": top_contact}
    })
}

fn build_trillionnium_client_duel_matrix_reply(value: &Value, opponent: &str) -> Value {
    let task = value.get("task").unwrap_or(value);
    let match_id = value
        .get("match")
        .and_then(|value| value.get("match_id"))
        .and_then(Value::as_str)
        .unwrap_or("face-duel-001");
    let task_id = task
        .get("task_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown-task");
    let invocation_status = task
        .get("invocation_status")
        .and_then(Value::as_str)
        .unwrap_or("Queued");
    let body = format!(
        "⚔️ Face Duel Started\nOpponent: {opponent}\nMatch: {match_id}\nTask: {task_id}\nStatus: {invocation_status}\n下一步：/submit {match_id} <对战结果>"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>⚔️ Face Duel Started</h3><p><strong>Opponent</strong>: {}</p><p><strong>Match</strong>: <code>{}</code></p><p><strong>Task</strong>: <code>{}</code> · <strong>Status</strong>: {}</p><p><code>/submit {} &lt;对战结果&gt;</code></p></blockquote>",
            escape_html(opponent), escape_html(match_id), escape_html(task_id), escape_html(invocation_status), escape_html(match_id),
        ),
        "cex_task_id": task_id,
        "cex_card": {"type": "trillionnium_client_duel", "version": 1, "client": "trillionnium_mobile_shell", "opponent": opponent, "match_id": match_id, "task_id": task_id, "invocation_status": invocation_status}
    })
}

fn build_trillionnium_craft_action_matrix_reply(value: &Value) -> Value {
    let mut reply = build_trillionnium_world_action_matrix_reply(value);
    if let Some(card) = reply.get_mut("cex_card").and_then(Value::as_object_mut) {
        card.insert("type".to_string(), json!("trillionnium_craft_action"));
        card.insert("module".to_string(), json!("trillionnium_craft"));
    }
    reply
}

const TRILLIONNIUM_ASSET_UPGRADE_EXAMPLE_COMMAND: &str =
    "/upgrade latest 资产化升级方案：沉淀客户交付方案、证据包、风险控制、下一步行动和自检复盘。";
const TRILLIONNIUM_COMPANY_EXAMPLE_COMMAND: &str =
    "/company latest 公司经营方案：定义客户交付方案、证据包、风险控制、下一步行动和自检复盘。";
const TRILLIONNIUM_SELL_EXAMPLE_COMMAND: &str =
    "/sell latest 服务上架方案：说明客户交付方案、证据包、风险控制、价格、下一步行动和自检复盘。";
const TRILLIONNIUM_BUY_EXAMPLE_COMMAND: &str =
    "/buy latest 采购需求：说明客户交付方案、证据包、风险控制、验收标准、下一步行动和自检复盘。";
const TRILLIONNIUM_WORK_DELIVER_EXAMPLE_COMMAND: &str =
    "/work deliver latest 交付包：提交客户交付方案、证据包、风险控制、下一步行动和自检复盘。";
const TRILLIONNIUM_WORK_ACCEPT_EXAMPLE_COMMAND: &str =
    "/work accept latest 验收确认：确认客户交付方案、证据包、风险控制、下一步合作行动和自检复盘。";
const TRILLIONNIUM_WORK_REJECT_EXAMPLE_COMMAND: &str =
    "/work reject latest 驳回说明：记录客户交付缺口、证据包、风险控制、下一步返工行动和自检复盘。";
const TRILLIONNIUM_WORK_REOPEN_EXAMPLE_COMMAND: &str =
    "/work reopen latest 返工要求：补齐客户交付方案、证据包、风险控制、下一步交付行动和自检复盘。";
const TRILLIONNIUM_WORK_CANCEL_EXAMPLE_COMMAND: &str =
    "/work cancel latest 取消原因：记录客户交付风险、证据包、退款控制、下一步校准行动和自检复盘。";

fn matrix_reward_settlement_explanation(payout_status: &str, ledger_status: &str) -> &'static str {
    if payout_status == "review_hold" {
        return "说明：命中复核，奖励暂缓；补齐客户交付、证据包、风险控制和自检复盘后再释放。";
    }
    if matches!(ledger_status, "settled" | "duplicate") {
        return "说明：账本已结算，奖励和进度已安全入账；现在可以查看奖励、排名或推进下一条路线。";
    }
    if ledger_status.contains("failed") || ledger_status.contains("error") {
        return "说明：账本结算失败，奖励不会提前释放；先重试结算或检查账本/资金状态。";
    }
    "说明：评分已记录，奖励等待账本确认；结算成功后才会释放进度、库存和排行榜收益。"
}

fn matrix_purchase_escrow_explanation(
    seller_ledger_status: &str,
    buyer_ledger_status: &str,
) -> &'static str {
    if matches!(
        seller_ledger_status,
        "settled" | "duplicate" | "reopened_settled"
    ) && matches!(buyer_ledger_status, "reserved" | "duplicate")
    {
        return "说明：买家托管已锁定、卖家结算已确认，工单可以安全交付。";
    }
    if buyer_ledger_status.contains("failed") || buyer_ledger_status.contains("pending") {
        return "说明：买家托管还没完成，先恢复预留资金，避免无资金工单继续流转。";
    }
    if seller_ledger_status.contains("failed") || seller_ledger_status.contains("pending") {
        return "说明：卖家结算还没完成，交付和进度会暂缓，避免未入账奖励被刷出。";
    }
    "说明：托管和卖家结算状态会决定工单能否安全交付；未确认前不提前释放经济进度。"
}

fn matrix_acceptance_settlement_explanation(buyer_consume_status: &str) -> &'static str {
    if matches!(buyer_consume_status, "consumed" | "duplicate") {
        return "说明：买家托管已消费，验收声望和后续协作已释放。";
    }
    if buyer_consume_status.contains("failed") || buyer_consume_status.contains("pending") {
        return "说明：验收已记录但托管消费未完成，声望/进度会等资金结清后再释放。";
    }
    "说明：验收奖励受托管消费保护，资金确认后才会进入声望和下一步路线。"
}

fn build_trillionnium_world_assets_matrix_reply(value: &Value) -> Value {
    let asset_count = value
        .get("assets")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let upgrade_count = value
        .get("upgrades")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let top_asset = value
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|assets| assets.last())
        .and_then(|asset| asset.get("asset_id"))
        .and_then(Value::as_str)
        .unwrap_or("latest");
    let route =
        RouteStoryCardContext::from_value(value, "asset-yard").specialize_opportunity("assets");
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let example_command = TRILLIONNIUM_ASSET_UPGRADE_EXAMPLE_COMMAND;
    let body = format!(
        "🏗️ World Assets\nAssets: {asset_count}\nUpgrades: {upgrade_count}\nTop Asset: {top_asset}\n{route_text_block}\n升级：{example_command}"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🏗️ World Assets</h3><p><strong>Assets</strong>: {} · <strong>Upgrades</strong>: {}</p><p><strong>Top</strong>: <code>{}</code></p>{}<p><code>{}</code></p></blockquote>",
            asset_count, upgrade_count, escape_html(top_asset), route_html_block, escape_html(example_command),
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_assets", "version": 1, "world": "trillionnium_world", "asset_count": asset_count, "upgrade_count": upgrade_count, "top_asset_id": top_asset}), &route, true)
    })
}

fn build_trillionnium_world_companies_matrix_reply(value: &Value) -> Value {
    let company_count = value
        .get("companies")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let top_company = value
        .get("companies")
        .and_then(Value::as_array)
        .and_then(|companies| companies.last())
        .and_then(|company| company.get("company_id"))
        .and_then(Value::as_str)
        .unwrap_or("latest");
    let route = RouteStoryCardContext::from_value(value, "starter-studio")
        .specialize_opportunity("companies");
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let example_command = TRILLIONNIUM_COMPANY_EXAMPLE_COMMAND;
    let body = format!(
        "🏢 World Companies\nCompanies: {company_count}\nTop Company: {top_company}\n{route_text_block}\n开公司：{example_command}"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🏢 World Companies</h3><p><strong>Companies</strong>: {}</p><p><strong>Top</strong>: <code>{}</code></p>{}<p><code>{}</code></p></blockquote>",
            company_count, escape_html(top_company), route_html_block, escape_html(example_command),
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_companies", "version": 1, "world": "trillionnium_world", "company_count": company_count, "top_company_id": top_company}), &route, true)
    })
}

fn build_trillionnium_world_company_matrix_reply(value: &Value) -> Value {
    let company = value.get("company").unwrap_or(value);
    let company_id = company
        .get("company_id")
        .and_then(Value::as_str)
        .unwrap_or("world-company");
    let level = company.get("level").and_then(Value::as_i64).unwrap_or(1);
    let revenue = company
        .get("revenue_score")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let reputation = company
        .get("reputation_score")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let route = RouteStoryCardContext::from_value(value, "starter-studio");
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let body = format!(
        "🏢 Company Launched\nCompany: {company_id}\nLevel: {level}\nRevenue: {revenue}\nReputation: {reputation}\n{route_text_block}\n查看：/companies"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🏢 Company Launched</h3><p><strong>Company</strong>: <code>{}</code></p><p><strong>Level</strong>: {} · <strong>Revenue</strong>: {} · <strong>Rep</strong>: {}</p>{}<p><code>/companies</code></p></blockquote>",
            escape_html(company_id), level, revenue, reputation, route_html_block,
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_company_created", "version": 1, "world": "trillionnium_world", "company_id": company_id, "company_level": level, "revenue_score": revenue, "reputation_score": reputation}), &route, true)
    })
}

fn build_trillionnium_world_shops_matrix_reply(value: &Value) -> Value {
    let shop_count = value
        .get("shops")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let listing_count = value
        .get("listings")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let top_shop = value
        .get("shops")
        .and_then(Value::as_array)
        .and_then(|shops| shops.last())
        .and_then(|shop| shop.get("shop_id"))
        .and_then(Value::as_str)
        .unwrap_or("latest");
    let route =
        RouteStoryCardContext::from_value(value, "client-board").specialize_opportunity("shops");
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let example_command = TRILLIONNIUM_SELL_EXAMPLE_COMMAND;
    let body = format!(
        "🛒 World Shops\nShops: {shop_count}\nListings: {listing_count}\nTop Shop: {top_shop}\n{route_text_block}\n上架：{example_command}"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🛒 World Shops</h3><p><strong>Shops</strong>: {} · <strong>Listings</strong>: {}</p><p><strong>Top</strong>: <code>{}</code></p>{}<p><code>{}</code></p></blockquote>",
            shop_count, listing_count, escape_html(top_shop), route_html_block, escape_html(example_command),
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_shops", "version": 1, "world": "trillionnium_world", "shop_count": shop_count, "listing_count": listing_count, "top_shop_id": top_shop}), &route, true)
    })
}

fn build_trillionnium_world_listing_matrix_reply(value: &Value) -> Value {
    let listing = value.get("listing").unwrap_or(value);
    let listing_id = listing
        .get("listing_id")
        .and_then(Value::as_str)
        .unwrap_or("world-listing");
    let price = listing
        .get("price_credits")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let quality = listing
        .get("quality_score")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let status = listing
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("listed");
    let route =
        RouteStoryCardContext::from_value(value, "client-board").specialize_opportunity("listing");
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let body = format!(
        "🛒 Listing Published\nListing: {listing_id}\nPrice: {price}\nQuality: {quality}\nStatus: {status}\n{route_text_block}\n查看：/shops"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🛒 Listing Published</h3><p><strong>Listing</strong>: <code>{}</code></p><p><strong>Price</strong>: {} · <strong>Quality</strong>: {} · <strong>Status</strong>: {}</p>{}<p><code>/shops</code></p></blockquote>",
            escape_html(listing_id), price, quality, escape_html(status), route_html_block,
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_listing_created", "version": 1, "world": "trillionnium_world", "listing_id": listing_id, "price_credits": price, "quality_score": quality, "status": status}), &route, true)
    })
}

fn build_trillionnium_world_purchase_matrix_reply(value: &Value) -> Value {
    let purchase = value.get("purchase").unwrap_or(value);
    let work_order = value.get("work_order").unwrap_or(value);
    let purchase_id = purchase
        .get("purchase_id")
        .and_then(Value::as_str)
        .unwrap_or("world-purchase");
    let work_order_id = work_order
        .get("work_order_id")
        .and_then(Value::as_str)
        .unwrap_or("world-work");
    let price = purchase
        .get("price_credits")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let status = purchase
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    let ledger_status = purchase
        .get("ledger_status")
        .and_then(Value::as_str)
        .or_else(|| value.get("ledger_status").and_then(Value::as_str))
        .unwrap_or("pending");
    let buyer_ledger_status = purchase
        .get("buyer_ledger_status")
        .and_then(Value::as_str)
        .or_else(|| value.get("buyer_ledger_status").and_then(Value::as_str))
        .unwrap_or("pending");
    let standing_rank = value
        .get("seller_standing")
        .and_then(|standing| standing.get("rank"))
        .and_then(Value::as_str)
        .unwrap_or("new_contact");
    let route = RouteStoryCardContext::from_value(value, "delivery-dock")
        .specialize_opportunity("purchase");
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let settlement_explanation =
        matrix_purchase_escrow_explanation(ledger_status, buyer_ledger_status);
    let body = format!(
        "💸 Listing Purchased\nPurchase: {purchase_id}\nWork Order: {work_order_id}\nPrice: {price}\nStatus: {status}\nSeller Ledger: {ledger_status}\nBuyer Reserve: {buyer_ledger_status}\nFaction Rank: {standing_rank}\n{settlement_explanation}\n{route_text_block}\n查看工作：/work"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>💸 Listing Purchased</h3><p><strong>Purchase</strong>: <code>{}</code></p><p><strong>Work</strong>: <code>{}</code></p><p><strong>Price</strong>: {} · <strong>Status</strong>: {}</p><p><strong>Seller Ledger</strong>: {} · <strong>Buyer Reserve</strong>: {}</p><p><strong>Faction</strong>: {}</p><p>{}</p>{}<p><code>/work</code> <code>/factions</code></p></blockquote>",
            escape_html(purchase_id), escape_html(work_order_id), price, escape_html(status), escape_html(ledger_status), escape_html(buyer_ledger_status), escape_html(standing_rank), escape_html(settlement_explanation), route_html_block,
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_listing_purchase", "version": 1, "world": "trillionnium_world", "purchase_id": purchase_id, "work_order_id": work_order_id, "price_credits": price, "status": status, "ledger_status": ledger_status, "buyer_ledger_status": buyer_ledger_status, "seller_faction_rank": standing_rank, "settlement_explanation": settlement_explanation}), &route, true)
    })
}

fn build_trillionnium_world_commerce_matrix_reply(value: &Value) -> Value {
    let purchase_count = value
        .get("purchases")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let work_count = value
        .get("work_orders")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let delivery_count = value
        .get("work_deliveries")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let acceptance_count = value
        .get("work_acceptances")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let rejection_count = value
        .get("work_rejections")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let reopen_count = value
        .get("work_reopens")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let cancellation_count = value
        .get("work_cancellations")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let latest_work = value
        .get("work_orders")
        .and_then(Value::as_array)
        .and_then(|items| items.last())
        .and_then(|work| work.get("work_order_id"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    let route =
        RouteStoryCardContext::from_value(value, "delivery-dock").specialize_opportunity("work");
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let buy_example = TRILLIONNIUM_BUY_EXAMPLE_COMMAND;
    let deliver_example = TRILLIONNIUM_WORK_DELIVER_EXAMPLE_COMMAND;
    let accept_example = TRILLIONNIUM_WORK_ACCEPT_EXAMPLE_COMMAND;
    let reject_example = TRILLIONNIUM_WORK_REJECT_EXAMPLE_COMMAND;
    let reopen_example = TRILLIONNIUM_WORK_REOPEN_EXAMPLE_COMMAND;
    let cancel_example = TRILLIONNIUM_WORK_CANCEL_EXAMPLE_COMMAND;
    let body = format!(
        "🧾 World Commerce\nPurchases: {purchase_count}\nWork Orders: {work_count}\nDeliveries: {delivery_count}\nAcceptances: {acceptance_count}\nRejections: {rejection_count}\nReopens: {reopen_count}\nCancellations: {cancellation_count}\nLatest Work: {latest_work}\n{route_text_block}\n操作示例：\n购买：{buy_example}\n交付：{deliver_example}\n验收：{accept_example}\n拒收：{reject_example}\n返工：{reopen_example}\n取消：{cancel_example}"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🧾 World Commerce</h3><p><strong>Purchases</strong>: {} · <strong>Work Orders</strong>: {} · <strong>Deliveries</strong>: {} · <strong>Acceptances</strong>: {} · <strong>Rejections</strong>: {} · <strong>Reopens</strong>: {} · <strong>Cancellations</strong>: {}</p><p><strong>Latest</strong>: <code>{}</code></p>{}<p><code>{}</code></p><p><code>{}</code></p><p><code>{}</code></p><p><code>{}</code></p><p><code>{}</code></p><p><code>{}</code></p></blockquote>",
            purchase_count, work_count, delivery_count, acceptance_count, rejection_count, reopen_count, cancellation_count, escape_html(latest_work), route_html_block, escape_html(buy_example), escape_html(deliver_example), escape_html(accept_example), escape_html(reject_example), escape_html(reopen_example), escape_html(cancel_example),
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_commerce", "version": 1, "world": "trillionnium_world", "purchase_count": purchase_count, "work_order_count": work_count, "delivery_count": delivery_count, "acceptance_count": acceptance_count, "rejection_count": rejection_count, "reopen_count": reopen_count, "cancellation_count": cancellation_count, "latest_work_order_id": latest_work}), &route, true)
    })
}

fn build_trillionnium_world_work_delivery_matrix_reply(value: &Value) -> Value {
    let work_order = value.get("work_order").unwrap_or(value);
    let delivery = value.get("delivery").unwrap_or(value);
    let work_order_id = work_order
        .get("work_order_id")
        .and_then(Value::as_str)
        .unwrap_or("world-work");
    let delivery_id = delivery
        .get("delivery_id")
        .and_then(Value::as_str)
        .unwrap_or("world-delivery");
    let score = delivery.get("score").and_then(Value::as_f64).unwrap_or(0.0);
    let status = delivery
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("delivered");
    let judge_status = delivery
        .get("judge_status")
        .and_then(Value::as_str)
        .unwrap_or("rubric_scored");
    let route = RouteStoryCardContext::from_value(value, "delivery-dock");
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let accept_next_command = format!(
        "/work accept {work_order_id} 验收确认：确认客户交付方案、证据包、风险控制、下一步合作行动和自检复盘。"
    );
    let body = format!(
        "📮 Work Delivered\nWork Order: {work_order_id}\nDelivery: {delivery_id}\nScore: {score:.1}\nStatus: {status}\nJudge: {judge_status}\n{route_text_block}\n下一步：{accept_next_command}"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>📮 Work Delivered</h3><p><strong>Work</strong>: <code>{}</code></p><p><strong>Delivery</strong>: <code>{}</code></p><p><strong>Score</strong>: {:.1} · <strong>Status</strong>: {} · <strong>Judge</strong>: {}</p>{}<p><code>{}</code></p></blockquote>",
            escape_html(work_order_id), escape_html(delivery_id), score, escape_html(status), escape_html(judge_status), route_html_block, escape_html(&accept_next_command),
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_work_delivery", "version": 1, "world": "trillionnium_world", "work_order_id": work_order_id, "delivery_id": delivery_id, "score": format!("{score:.1}"), "status": status, "judge_status": judge_status, "next_step_command": accept_next_command}), &route, true)
    })
}

fn build_trillionnium_world_work_acceptance_matrix_reply(value: &Value) -> Value {
    let work_order = value.get("work_order").unwrap_or(value);
    let acceptance = value.get("acceptance").unwrap_or(value);
    let purchase = value.get("purchase").unwrap_or(value);
    let work_order_id = work_order
        .get("work_order_id")
        .and_then(Value::as_str)
        .unwrap_or("world-work");
    let acceptance_id = acceptance
        .get("acceptance_id")
        .and_then(Value::as_str)
        .unwrap_or("world-acceptance");
    let status = acceptance
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("accepted");
    let reputation_delta = acceptance
        .get("reputation_delta")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let buyer_consume_status = purchase
        .get("buyer_consume_status")
        .and_then(Value::as_str)
        .or_else(|| value.get("buyer_consume_status").and_then(Value::as_str))
        .unwrap_or("pending");
    let route = RouteStoryCardContext::from_value(value, "delivery-dock");
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let settlement_explanation = matrix_acceptance_settlement_explanation(buyer_consume_status);
    let body = format!(
        "✅ Work Accepted\nWork Order: {work_order_id}\nAcceptance: {acceptance_id}\nStatus: {status}\nBuyer Consume: {buyer_consume_status}\nReputation: +{reputation_delta}\n{settlement_explanation}\n{route_text_block}\n查看：/work /factions"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>✅ Work Accepted</h3><p><strong>Work</strong>: <code>{}</code></p><p><strong>Acceptance</strong>: <code>{}</code></p><p><strong>Status</strong>: {} · <strong>Buyer Consume</strong>: {} · <strong>Reputation</strong>: +{}</p><p>{}</p>{}<p><code>/work</code> <code>/factions</code></p></blockquote>",
            escape_html(work_order_id), escape_html(acceptance_id), escape_html(status), escape_html(buyer_consume_status), reputation_delta, escape_html(settlement_explanation), route_html_block,
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_work_acceptance", "version": 1, "world": "trillionnium_world", "work_order_id": work_order_id, "acceptance_id": acceptance_id, "status": status, "reputation_delta": reputation_delta, "buyer_consume_status": buyer_consume_status, "settlement_explanation": settlement_explanation}), &route, true)
    })
}

fn build_trillionnium_world_work_rejection_matrix_reply(value: &Value) -> Value {
    let work_order = value.get("work_order").unwrap_or(value);
    let rejection = value.get("rejection").unwrap_or(value);
    let purchase = value.get("purchase").unwrap_or(value);
    let work_order_id = work_order
        .get("work_order_id")
        .and_then(Value::as_str)
        .unwrap_or("world-work");
    let rejection_id = rejection
        .get("rejection_id")
        .and_then(Value::as_str)
        .unwrap_or("world-rejection");
    let status = rejection
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("rejected_refunded");
    let refund_status = rejection
        .get("refund_status")
        .and_then(Value::as_str)
        .or_else(|| value.get("buyer_refund_status").and_then(Value::as_str))
        .unwrap_or("pending");
    let purchase_status = purchase
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("rejected_refunded");
    let rejection_needs_refund_retry = matches!(
        status,
        "rejected_refund_hold" | "rejected_refund_failed" | "rejected_pending_refund"
    );
    let rejection_needs_chargeback_retry = matches!(
        status,
        "rejected_chargeback_failed" | "rejected_pending_chargeback"
    );
    let (
        action_label,
        command_hint,
        stage_summary,
        opportunity_kind,
        feedback_focus,
        opportunity_hint,
        opportunity_playbook,
    ) = if rejection_needs_chargeback_retry {
        (
            "Retry rejection settlement",
            format!(
                "/work reject latest 重试拒收卖家扣回：围绕 {} 确认买家不二次退款、卖家扣回资金、ledger blocker、evidence gap 和 next action。",
                work_order_id
            ),
            format!(
                "Work {} · {} → recover seller chargeback → reopen only after settlement",
                work_order_id, status
            ),
            "rejection_chargeback_recovery",
            "Buyer refund is already protected; recover seller chargeback before any reopen or redelivery.",
            format!(
                "Recover {} by retrying rejection settlement first, not by sending the player into a redelivery lane.",
                work_order_id
            ),
            "Verify buyer is not refunded twice, restore seller chargeback funds, clear the ledger blocker, then reopen the revision route.",
        )
    } else if rejection_needs_refund_retry {
        (
            "Retry rejection refund",
            format!(
                "/work reject latest 重试拒收退款：围绕 {} 确认买家预留资金、refund blocker、卖家扣回风险、evidence gap 和 next action。",
                work_order_id
            ),
            format!(
                "Work {} · {} → recover buyer refund → continue settlement",
                work_order_id, status
            ),
            "rejection_refund_recovery",
            "Buyer refund is blocked; recover settlement before any reopen or redelivery.",
            format!(
                "Recover {} by retrying the refund leg before reopening the revision route.",
                work_order_id
            ),
            "Restore buyer reserve/refund state, record the blocker, then continue seller chargeback and reopen only after settlement is clear.",
        )
    } else {
        (
            "Reopen revision route",
            format!(
                "/work reopen latest 重开返工委托：围绕 {} 补齐 evidence、修复 gap、重申 acceptance standard、timeline 和 next action。",
                work_order_id
            ),
            format!(
                "Work {} · {} → settlement clear → reopen revision route",
                work_order_id, status
            ),
            "revision_reopen",
            "Capture the buyer objections, patch missing proof, relock acceptance criteria, and reopen before redelivery.",
            format!(
                "Recover {} with a controlled reopen step before sending any revised result.",
                work_order_id
            ),
            "List the rejection reasons, close each evidence gap, reserve the revision lane again, then redeliver with explicit proof checkpoints.",
        )
    };
    let route = RouteStoryCardContext::from_value(value, "delivery-dock").with_custom_follow_up(
        work_order_id,
        action_label,
        "world-commerce-panel",
        command_hint,
        "zbj-market-gate",
        "delivery-dock",
        stage_summary,
        opportunity_kind,
        format!("Work order {} · {}", work_order_id, status),
        feedback_focus,
        opportunity_hint,
        opportunity_playbook,
    );
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let body = format!(
        "↩️ Work Rejected / Refunded\nWork Order: {work_order_id}\nRejection: {rejection_id}\nStatus: {status}\nBuyer Refund: {refund_status}\nPurchase: {purchase_status}\n{route_text_block}\n查看：/work /factions"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>↩️ Work Rejected / Refunded</h3><p><strong>Work</strong>: <code>{}</code></p><p><strong>Rejection</strong>: <code>{}</code></p><p><strong>Status</strong>: {} · <strong>Buyer Refund</strong>: {} · <strong>Purchase</strong>: {}</p>{}<p><code>/work</code> <code>/factions</code></p></blockquote>",
            escape_html(work_order_id), escape_html(rejection_id), escape_html(status), escape_html(refund_status), escape_html(purchase_status), route_html_block,
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_work_rejection", "version": 1, "world": "trillionnium_world", "work_order_id": work_order_id, "rejection_id": rejection_id, "status": status, "buyer_refund_status": refund_status, "purchase_status": purchase_status}), &route, true)
    })
}

fn build_trillionnium_world_work_reopen_matrix_reply(value: &Value) -> Value {
    let work_order = value.get("work_order").unwrap_or(value);
    let reopen = value.get("reopen").unwrap_or(value);
    let purchase = value.get("purchase").unwrap_or(value);
    let work_order_id = work_order
        .get("work_order_id")
        .and_then(Value::as_str)
        .unwrap_or("world-work");
    let reopen_id = reopen
        .get("reopen_id")
        .and_then(Value::as_str)
        .unwrap_or("world-reopen");
    let status = reopen
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("reopened");
    let reserve_status = reopen
        .get("reserve_status")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("buyer_reopen_reserve_status")
                .and_then(Value::as_str)
        })
        .unwrap_or("pending");
    let purchase_status = purchase
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("reopened_reserved");
    let work_status = work_order
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("open");
    let route = RouteStoryCardContext::from_value(value, "delivery-dock").with_custom_follow_up(
        work_order_id,
        "Lock reopen redelivery",
        "world-commerce-panel",
        format!(
            "/work deliver latest 重开返工交付：围绕 {} 更新 revision scope、evidence、acceptance checklist、owner 和 next action。",
            work_order_id
        ),
        "zbj-market-gate",
        "delivery-dock",
        format!(
            "Work {} · {} → relock scope and proof → redeliver",
            work_order_id, status
        ),
        "reopen_recovery",
        format!("Work order {} · {}", work_order_id, status),
        "Close the evidence gap, re-lock the revised scope, and make the next redelivery criteria unambiguous.",
        format!(
            "Recover {} through a controlled reopen loop with a tighter redelivery package.",
            work_order_id
        ),
        "Restate scope, owner, deadline, and acceptance checks, then redeliver with explicit proof and revision tracking.",
    );
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let deliver_next_command = format!(
        "/work deliver {work_order_id} 返工交付包：提交修订后的客户交付方案、证据包、风险控制、下一步行动和自检复盘。"
    );
    let body = format!(
        "🔁 Work Reopened / Reserved\nWork Order: {work_order_id}\nReopen: {reopen_id}\nStatus: {status}\nBuyer Reserve: {reserve_status}\nWork: {work_status}\nPurchase: {purchase_status}\n{route_text_block}\n下一步：{deliver_next_command}"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🔁 Work Reopened / Reserved</h3><p><strong>Work</strong>: <code>{}</code></p><p><strong>Reopen</strong>: <code>{}</code></p><p><strong>Status</strong>: {} · <strong>Buyer Reserve</strong>: {} · <strong>Work</strong>: {} · <strong>Purchase</strong>: {}</p>{}<p><code>{}</code></p></blockquote>",
            escape_html(work_order_id), escape_html(reopen_id), escape_html(status), escape_html(reserve_status), escape_html(work_status), escape_html(purchase_status), route_html_block, escape_html(&deliver_next_command),
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_work_reopen", "version": 1, "world": "trillionnium_world", "work_order_id": work_order_id, "reopen_id": reopen_id, "status": status, "buyer_reopen_reserve_status": reserve_status, "work_status": work_status, "purchase_status": purchase_status, "next_step_command": deliver_next_command}), &route, true)
    })
}

fn build_trillionnium_world_work_cancellation_matrix_reply(value: &Value) -> Value {
    let work_order = value.get("work_order").unwrap_or(value);
    let cancellation = value.get("cancellation").unwrap_or(value);
    let purchase = value.get("purchase").unwrap_or(value);
    let work_order_id = work_order
        .get("work_order_id")
        .and_then(Value::as_str)
        .unwrap_or("world-work");
    let cancellation_id = cancellation
        .get("cancellation_id")
        .and_then(Value::as_str)
        .unwrap_or("world-cancel");
    let status = cancellation
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("cancelled_refunded");
    let refund_status = cancellation
        .get("refund_status")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("buyer_cancel_refund_status")
                .and_then(Value::as_str)
        })
        .unwrap_or("pending");
    let purchase_status = purchase
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("cancelled_refunded");
    let cancellation_needs_refund_retry = matches!(
        status,
        "cancelled_refund_hold" | "cancelled_refund_failed" | "cancel_pending_refund"
    );
    let cancellation_needs_chargeback_retry = matches!(
        status,
        "cancelled_chargeback_failed" | "cancel_pending_chargeback"
    );
    let (
        action_label,
        panel_id,
        command_hint,
        node_id,
        stage_summary,
        opportunity_kind,
        feedback_focus,
        opportunity_hint,
        opportunity_playbook,
    ) = if cancellation_needs_refund_retry || cancellation_needs_chargeback_retry {
        (
            "Retry cancellation settlement",
            "world-commerce-panel",
            format!(
                "/work cancel latest 重试取消清算：围绕 {} 确认买家不二次退款、卖家扣回资金、ledger blocker、重新校准需求和 next action。",
                work_order_id
            ),
            "delivery-dock",
            format!(
                "Work {} · {} → recover cancellation settlement → requal only after ledger clear",
                work_order_id, status
            ),
            "cancellation_settlement_recovery",
            "Cancellation settlement is blocked; retry refund/chargeback before relisting or opening new scope.",
            format!(
                "Recover {} by clearing cancellation settlement before sending the player into a relist lane.",
                work_order_id
            ),
            "Verify buyer is not refunded twice, restore seller chargeback funds if needed, clear the ledger blocker, then re-scope only after settlement is complete.",
        )
    } else {
        (
            "Launch smaller-scope requal",
            "world-listings-panel",
            format!(
                "/sell latest 小范围试单方案：围绕 {} 重做更小 scope、deliverable、evidence、risk gate、price 和 next action。",
                work_order_id
            ),
            "client-board",
            format!(
                "Work {} · {} → shrink scope and risk → relist",
                work_order_id, status
            ),
            "smaller_scope_requalification",
            "Reduce risk, tighten the starter deliverable, and relaunch only with proof the buyer can validate quickly.",
            format!(
                "Recover {} with a smaller-scoped or better-qualified offer before restarting the work lane.",
                work_order_id
            ),
            "Re-scope the offer, lower the commitment surface, clarify evidence and acceptance, then relist a safer starter package.",
        )
    };
    let route = RouteStoryCardContext::from_value(value, "client-board").with_custom_follow_up(
        work_order_id,
        action_label,
        panel_id,
        command_hint,
        "zbj-market-gate",
        node_id,
        stage_summary,
        opportunity_kind,
        format!("Work order {} · {}", work_order_id, status),
        feedback_focus,
        opportunity_hint,
        opportunity_playbook,
    );
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let body = format!(
        "🛑 Work Cancelled / Refunded\nWork Order: {work_order_id}\nCancellation: {cancellation_id}\nStatus: {status}\nBuyer Refund: {refund_status}\nPurchase: {purchase_status}\n{route_text_block}\n查看：/work /factions"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🛑 Work Cancelled / Refunded</h3><p><strong>Work</strong>: <code>{}</code></p><p><strong>Cancellation</strong>: <code>{}</code></p><p><strong>Status</strong>: {} · <strong>Buyer Refund</strong>: {} · <strong>Purchase</strong>: {}</p>{}<p><code>/work</code> <code>/factions</code></p></blockquote>",
            escape_html(work_order_id), escape_html(cancellation_id), escape_html(status), escape_html(refund_status), escape_html(purchase_status), route_html_block,
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_work_cancellation", "version": 1, "world": "trillionnium_world", "work_order_id": work_order_id, "cancellation_id": cancellation_id, "status": status, "buyer_cancel_refund_status": refund_status, "purchase_status": purchase_status}), &route, true)
    })
}

fn build_trillionnium_world_factions_matrix_reply(value: &Value) -> Value {
    let faction_count = value
        .get("factions")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let standing_count = value
        .get("standings")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let top_standing = value
        .get("standings")
        .and_then(Value::as_array)
        .and_then(|items| items.last())
        .and_then(|standing| standing.get("rank"))
        .and_then(Value::as_str)
        .unwrap_or("stranger");
    let body = format!(
        "🏛️ World Factions\nFactions: {faction_count}\nStandings: {standing_count}\nLatest Rank: {top_standing}\n交易和工作会提升阵营声望。"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🏛️ World Factions</h3><p><strong>Factions</strong>: {} · <strong>Standings</strong>: {}</p><p><strong>Latest Rank</strong>: {}</p><p>交易和工作会提升阵营声望。</p></blockquote>",
            faction_count, standing_count, escape_html(top_standing),
        ),
        "cex_card": {"type": "trillionnium_world_factions", "version": 1, "world": "trillionnium_world", "faction_count": faction_count, "standing_count": standing_count, "latest_rank": top_standing}
    })
}

fn build_trillionnium_world_asset_upgrade_matrix_reply(value: &Value) -> Value {
    let asset = value.get("asset").unwrap_or(value);
    let upgrade = value.get("upgrade").unwrap_or(value);
    let asset_id = asset
        .get("asset_id")
        .and_then(Value::as_str)
        .unwrap_or("world-asset");
    let level = asset
        .get("upgrade_level")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let value_score = asset
        .get("value_score")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let delta = upgrade
        .get("value_delta")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let judge_status = upgrade
        .get("judge_status")
        .and_then(Value::as_str)
        .unwrap_or("rubric_scored");
    let route = RouteStoryCardContext::from_value(value, "asset-yard");
    let route_text_block = route.text_block("Route Follow-up", true);
    let route_html_block = route.html_block("Route Follow-up", true);
    let body = format!(
        "🏗️ Asset Upgraded\nAsset: {asset_id}\nLevel: {level}\nValue: {value_score} (+{delta})\nJudge: {judge_status}\n{route_text_block}\n查看资产：/assets"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🏗️ Asset Upgraded</h3><p><strong>Asset</strong>: <code>{}</code></p><p><strong>Level</strong>: {} · <strong>Value</strong>: {} (+{})</p><p><strong>Judge</strong>: {}</p>{}<p><code>/assets</code></p></blockquote>",
            escape_html(asset_id), level, value_score, delta, escape_html(judge_status), route_html_block,
        ),
        "cex_card": route_story_card_json(json!({"type": "trillionnium_world_asset_upgrade", "version": 1, "world": "trillionnium_world", "asset_id": asset_id, "asset_level": level, "value_score": value_score, "value_delta": delta, "judge_status": judge_status}), &route, true)
    })
}

fn build_trillionnium_world_contract_matrix_reply(value: &Value) -> Value {
    let mut reply = build_trillionnium_world_action_matrix_reply(value);
    if let Some(card) = reply.get_mut("cex_card").and_then(Value::as_object_mut) {
        card.insert("type".to_string(), json!("trillionnium_world_contract"));
        card.insert("module".to_string(), json!("world_contract"));
        if let Some(contract_id) = value
            .get("contract")
            .and_then(|contract| contract.get("contract_id"))
            .and_then(Value::as_str)
        {
            card.insert("contract_id".to_string(), json!(contract_id));
        }
    }
    reply
}

fn build_trillionnium_world_contract_completion_matrix_reply(value: &Value) -> Value {
    let completion = value.get("completion").unwrap_or(value);
    let contract_id = completion
        .get("contract_id")
        .and_then(Value::as_str)
        .unwrap_or("world-contract");
    let score = completion
        .get("score")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let reward = completion
        .get("reward_amount")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let ledger_status = completion
        .get("ledger_status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    let judge_status = completion
        .get("judge_status")
        .and_then(Value::as_str)
        .unwrap_or("rubric_scored");
    let settlement_explanation = matrix_reward_settlement_explanation("eligible", ledger_status);
    let body = format!(
        "✅ World Contract Complete\nContract: {contract_id}\nScore: {score:.1}\nReward: {reward:.2}\nLedger: {ledger_status}\nJudge: {judge_status}\n{settlement_explanation}\n查看世界：/world"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>✅ World Contract Complete</h3><p><strong>Contract</strong>: <code>{}</code></p><p><strong>Score</strong>: {:.1} · <strong>Reward</strong>: {:.2}</p><p><strong>Ledger</strong>: {} · <strong>Judge</strong>: {}</p><p>{}</p><p><code>/world</code></p></blockquote>",
            escape_html(contract_id), score, reward, escape_html(ledger_status), escape_html(judge_status), escape_html(settlement_explanation),
        ),
        "cex_card": {"type": "trillionnium_world_contract_completion", "version": 1, "world": "trillionnium_world", "contract_id": contract_id, "score": format!("{score:.1}"), "reward": format!("{reward:.2}"), "ledger_status": ledger_status, "judge_status": judge_status, "settlement_explanation": settlement_explanation}
    })
}

fn build_trillionnium_world_action_matrix_reply(value: &Value) -> Value {
    let event = value.get("event").unwrap_or(value);
    let event_kind = event
        .get("event_kind")
        .and_then(Value::as_str)
        .unwrap_or("explore");
    let location_id = event
        .get("location_id")
        .and_then(Value::as_str)
        .unwrap_or("mirror-city-square");
    let result = event
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or("世界发生了变化。");
    let impact = event
        .get("impact_score")
        .and_then(Value::as_i64)
        .unwrap_or(10);
    let task_id = event.get("cex_task_id").and_then(Value::as_str);
    let contract_id = value
        .get("contract")
        .and_then(|contract| contract.get("contract_id"))
        .and_then(Value::as_str);
    let task_line = task_id
        .map(|task_id| format!("\nCEX Task: {task_id}"))
        .unwrap_or_default();
    let body = format!(
        "🌍 World Action\nKind: {event_kind}\nLocation: {location_id}\nImpact: +{impact}{task_line}\nResult: {result}\n查看世界：/world"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🌍 World Action</h3><p><strong>Kind</strong>: {}</p><p><strong>Location</strong>: <code>{}</code></p><p><strong>Impact</strong>: +{}</p><p>{}</p><p><code>/world</code></p></blockquote>",
            escape_html(event_kind), escape_html(location_id), impact, escape_html(result),
        ),
        "cex_card": {"type": "trillionnium_world_action", "version": 1, "world": "trillionnium_world", "event_id": event.get("event_id").and_then(Value::as_str), "event_kind": event_kind, "location_id": location_id, "impact_score": impact, "contract_id": contract_id, "task_id": task_id, "cex_status": event.get("cex_status").and_then(Value::as_str)}
    })
}

fn build_league_season_matrix_reply(value: &Value) -> Value {
    let season = value.get("season").unwrap_or(value);
    let name = season
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Preseason Zero");
    let player_count = season
        .get("player_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let battle_count = season
        .get("battle_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let reward_count = season
        .get("reward_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let top_player = value
        .get("leaderboards")
        .and_then(|leaderboards| leaderboards.get("players"))
        .and_then(Value::as_array)
        .and_then(|players| players.first())
        .and_then(|player| {
            player
                .get("display_name")
                .or_else(|| player.get("matrix_user_id"))
        })
        .and_then(Value::as_str)
        .unwrap_or("@alice:local.dev");
    let top_guild = value
        .get("leaderboards")
        .and_then(|leaderboards| leaderboards.get("guilds"))
        .and_then(Value::as_array)
        .and_then(|guilds| guilds.first())
        .and_then(|guild| guild.get("name").or_else(|| guild.get("guild_id")))
        .and_then(Value::as_str)
        .unwrap_or("Prompt Forge");
    let body = format!(
        "📅 League Season\n{name}\nPlayers: {player_count}\nBattles: {battle_count}\nRewards: {reward_count}\nTop Player: {top_player}\nTop Guild: {top_guild}\n/rank 查看天梯"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>📅 League Season</h3><p><strong>{}</strong></p><p>Players: {} · Battles: {} · Rewards: {}</p><p>Top Player: <code>{}</code></p><p>Top Guild: <strong>{}</strong></p><p><code>/rank</code></p></blockquote>",
            escape_html(name), player_count, battle_count, reward_count, escape_html(top_player), escape_html(top_guild),
        ),
        "cex_card": {"type": "league_season", "version": 1, "league": "trillionnium_league", "season_name": name, "player_count": player_count, "battle_count": battle_count, "reward_count": reward_count, "top_player": top_player, "top_guild": top_guild}
    })
}

fn build_league_rank_matrix_reply(rankings: Option<&Value>) -> Value {
    let first_player = rankings
        .and_then(|value| value.get("players"))
        .and_then(Value::as_array)
        .and_then(|players| players.first())
        .and_then(|player| player.get("matrix_user_id"))
        .and_then(Value::as_str)
        .unwrap_or("@alice:local.dev");
    let body = format!("🏅 Trillionnium Rank\n赛季：Preseason Zero\n1. {first_player}｜Bronze I｜1000 RP｜0W-0L\n\n更多排名将在真实提交评分后更新。");
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": "<blockquote><h3>🏅 Trillionnium Rank</h3><p>赛季：<strong>Preseason Zero</strong></p><ol><li><code>@alice:local.dev</code>｜Bronze I｜1000 RP｜0W-0L</li></ol><p>更多排名将在真实提交评分后更新。</p></blockquote>",
        "cex_card": {
            "type": "league_rank",
            "version": 1,
            "league": "trillionnium_league",
            "season": "preseason-zero"
        }
    })
}

fn build_league_loadout_matrix_reply(_loadout: Option<&Value>) -> Value {
    let body = "🧙 Agent Loadout\n当前默认阵容：\n1. Oracle Scout｜侦察/调研\n2. Forge Builder｜生成/构建\n3. Mirror Auditor｜审核/测试\n4. Courier Closer｜交付/包装\n\n下一步会开放 /draft 自定义阵容。";
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": "<blockquote><h3>🧙 Agent Loadout</h3><p><strong>当前默认阵容</strong></p><ol><li>Oracle Scout｜侦察/调研</li><li>Forge Builder｜生成/构建</li><li>Mirror Auditor｜审核/测试</li><li>Courier Closer｜交付/包装</li></ol><p>下一步会开放 <code>/draft</code> 自定义阵容。</p></blockquote>",
        "cex_card": {
            "type": "league_loadout",
            "version": 1,
            "league": "trillionnium_league",
            "heroes": ["oracle_scout", "forge_builder", "mirror_auditor", "courier_closer"]
        }
    })
}

fn build_league_join_matrix_reply_from(value: &Value) -> Value {
    let match_id = value
        .get("match")
        .and_then(|value| value.get("match_id"))
        .and_then(Value::as_str)
        .or_else(|| value.get("match_id").and_then(Value::as_str))
        .unwrap_or("daily-dungeon-001");
    let safe_match_id = escape_html(match_id);
    let body = format!("✅ 已加入 Trillionnium League 赛场\nMatch: {match_id}\n下一步：/battle {match_id} <你的行动>");
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>✅ 已加入 Trillionnium League 赛场</h3><p><strong>Match</strong>: <code>{}</code></p><p>下一步：<code>/battle {} &lt;你的行动&gt;</code></p></blockquote>",
            safe_match_id,
            safe_match_id,
        ),
        "cex_card": {
            "type": "league_joined",
            "version": 1,
            "league": "trillionnium_league",
            "match_id": match_id,
            "status": "joined"
        }
    })
}

fn build_league_raids_matrix_reply(value: &Value) -> Value {
    let raid_count = value
        .get("raids")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let first_raid = value
        .get("raids")
        .and_then(Value::as_array)
        .and_then(|raids| raids.first())
        .and_then(|raid| raid.get("match_id"))
        .and_then(Value::as_str)
        .unwrap_or("guild-raid-001");
    let progress = value
        .get("progress")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("progress_percent"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let body = format!(
        "🐉 Guild Raids\nOpen raids: {raid_count}\n{first_raid}: {progress:.1}%\n贡献：/raid {first_raid} <你的团队行动>"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🐉 Guild Raids</h3><p><strong>Open raids</strong>: {}</p><p><code>{}</code>: {:.1}%</p><p><code>/raid {} &lt;你的团队行动&gt;</code></p></blockquote>",
            raid_count,
            escape_html(first_raid),
            progress,
            escape_html(first_raid),
        ),
        "cex_card": {"type": "league_raids", "version": 1, "league": "trillionnium_league", "raid_count": raid_count, "match_id": first_raid, "progress_percent": format!("{progress:.1}")}
    })
}

fn build_league_raid_roster_matrix_reply(value: &Value) -> Value {
    let roster = value.get("roster").unwrap_or(value);
    let match_id = roster
        .get("match_id")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("slot")
                .and_then(|slot| slot.get("match_id"))
                .and_then(Value::as_str)
        })
        .unwrap_or("guild-raid-001");
    let slot_count = roster
        .get("slot_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let ready = roster
        .get("ready")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let missing = roster
        .get("missing_roles")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("/")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "none".to_string());
    let body = format!(
        "👥 Raid Team\nMatch: {match_id}\nSlots: {slot_count}\nReady: {ready}\nMissing: {missing}\n加入：/team {match_id} scout"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>👥 Raid Team</h3><p><strong>Match</strong>: <code>{}</code></p><p><strong>Slots</strong>: {}</p><p><strong>Ready</strong>: {}</p><p><strong>Missing</strong>: {}</p><p><code>/team {} scout</code></p></blockquote>",
            escape_html(match_id), slot_count, ready, escape_html(&missing), escape_html(match_id),
        ),
        "cex_card": {"type": "league_raid_roster", "version": 1, "league": "trillionnium_league", "match_id": match_id, "slot_count": slot_count, "ready": ready, "missing_roles": missing}
    })
}

fn build_league_raid_contribution_matrix_reply(value: &Value) -> Value {
    let contribution = value.get("contribution").unwrap_or(value);
    let progress = value.get("progress").unwrap_or(value);
    let match_id = contribution
        .get("match_id")
        .and_then(Value::as_str)
        .unwrap_or("guild-raid-001");
    let score = contribution
        .get("contribution_score")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let progress_percent = progress
        .get("progress_percent")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let phase = progress
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("opening");
    let body = format!(
        "🐉 Raid Contribution\nMatch: {match_id}\nScore: {score:.1}\nProgress: {progress_percent:.1}%\nPhase: {phase}\n继续：/raid {match_id} <下一步>"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🐉 Raid Contribution</h3><p><strong>Match</strong>: <code>{}</code></p><p><strong>Score</strong>: {:.1}</p><p><strong>Progress</strong>: {:.1}%</p><p><strong>Phase</strong>: {}</p><p><code>/raid {} &lt;下一步&gt;</code></p></blockquote>",
            escape_html(match_id),
            score,
            progress_percent,
            escape_html(phase),
            escape_html(match_id),
        ),
        "cex_card": {"type": "league_raid_contribution", "version": 1, "league": "trillionnium_league", "match_id": match_id, "score": format!("{score:.1}"), "progress_percent": format!("{progress_percent:.1}"), "phase": phase}
    })
}

fn build_league_guilds_matrix_reply(value: &Value) -> Value {
    let guild_count = value
        .get("guilds")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let first_guild = value
        .get("guilds")
        .and_then(Value::as_array)
        .and_then(|guilds| guilds.first())
        .and_then(|guild| guild.get("guild_id"))
        .and_then(Value::as_str)
        .unwrap_or("guild-prompt-forge");
    let top_standing = value
        .get("standings")
        .and_then(Value::as_array)
        .and_then(|standings| standings.first());
    let top_guild = top_standing
        .and_then(|guild| guild.get("name").or_else(|| guild.get("guild_id")))
        .and_then(Value::as_str)
        .unwrap_or("Prompt Forge");
    let top_power = top_standing
        .and_then(|guild| guild.get("power_score"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let body = format!(
        "🛡️ League Guilds\n开放公会：{guild_count}\nTop Guild: {top_guild} ({top_power:.1})\n加入推荐：/guild {first_guild}\n公会团本：/join guild-raid-001"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🛡️ League Guilds</h3><p><strong>开放公会</strong>: {}</p><p>Top Guild: <strong>{}</strong> ({:.1})</p><p><code>/guild {}</code></p><p><code>/join guild-raid-001</code></p></blockquote>",
            guild_count,
            escape_html(top_guild),
            top_power,
            escape_html(first_guild),
        ),
        "cex_card": {"type": "league_guilds", "version": 1, "league": "trillionnium_league", "guild_count": guild_count, "recommended_guild_id": first_guild, "top_guild": top_guild, "top_power": format!("{top_power:.1}")}
    })
}

fn build_league_guild_join_matrix_reply(value: &Value) -> Value {
    let guild = value.get("guild").unwrap_or(value);
    let guild_id = guild
        .get("guild_id")
        .and_then(Value::as_str)
        .unwrap_or("guild-prompt-forge");
    let name = guild
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Prompt Forge");
    let body = format!("🛡️ 已加入公会\n{name}\nGuild: {guild_id}\n下一步：/join guild-raid-001");
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🛡️ 已加入公会</h3><p><strong>{}</strong></p><p><code>{}</code></p><p><code>/join guild-raid-001</code></p></blockquote>",
            escape_html(name), escape_html(guild_id),
        ),
        "cex_card": {"type": "league_guild_joined", "version": 1, "league": "trillionnium_league", "guild_id": guild_id, "guild_name": name}
    })
}

fn build_league_draft_matrix_reply(value: &Value) -> Value {
    let heroes = value
        .get("loadout")
        .and_then(|loadout| loadout.get("heroes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let hero_names: Vec<String> = heroes
        .iter()
        .filter_map(|hero| {
            hero.get("name")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect();
    let display = if hero_names.is_empty() {
        "Oracle Scout / Forge Builder / Mirror Auditor".to_string()
    } else {
        hero_names.join(" / ")
    };
    let body = format!("🧬 Draft Locked\n阵容：{display}\n出战：/battle daily-dungeon-001 <行动>");
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🧬 Draft Locked</h3><p><strong>阵容</strong>: {}</p><p><code>/battle daily-dungeon-001 &lt;行动&gt;</code></p></blockquote>",
            escape_html(&display),
        ),
        "cex_card": {"type": "league_draft", "version": 1, "league": "trillionnium_league", "heroes": hero_names}
    })
}

fn build_league_battle_matrix_reply(value: &Value) -> Value {
    let task = value.get("task").unwrap_or(value);
    let task_id = task
        .get("task_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown-task");
    let consumer_status = task
        .get("consumer_status")
        .and_then(Value::as_str)
        .unwrap_or("queued");
    let invocation_status = task
        .get("invocation_status")
        .and_then(Value::as_str)
        .unwrap_or("Queued");
    let match_id = value
        .get("match")
        .and_then(|value| value.get("match_id"))
        .and_then(Value::as_str)
        .unwrap_or("daily-dungeon-001");
    let entry_id = value
        .get("entry")
        .and_then(|value| value.get("entry_id"))
        .and_then(Value::as_str)
        .unwrap_or("unknown-entry");
    let body = format!(
        "⚡ League Battle 已出招\nMatch: {match_id}\nTask: {task_id}\n状态：{}\n执行：{invocation_status}\n查看：/status {task_id}\n排行榜：/rank",
        status_label(consumer_status)
    );

    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>⚡ League Battle 已出招</h3><p><strong>Match</strong>: <code>{}</code></p><p><strong>Task</strong>: <code>{}</code></p><p><strong>状态</strong>: {}</p><p><strong>执行</strong>: {}</p><p><code>/status {}</code> · <code>/rank</code></p></blockquote>",
            escape_html(match_id),
            escape_html(task_id),
            escape_html(status_label(consumer_status)),
            escape_html(invocation_status),
            escape_html(task_id),
        ),
        "cex_task_id": task_id,
        "consumer_status": consumer_status,
        "invocation_status": invocation_status,
        "cex_card": {
            "type": "league_battle",
            "version": 1,
            "league": "trillionnium_league",
            "match_id": match_id,
            "entry_id": entry_id,
            "task_id": task_id,
            "consumer_status": consumer_status,
            "invocation_status": invocation_status,
        }
    })
}

fn build_league_submission_matrix_reply(value: &Value) -> Value {
    let submission = value.get("submission").unwrap_or(value);
    let reward = value.get("reward").unwrap_or(&Value::Null);
    let match_id = submission
        .get("match_id")
        .and_then(Value::as_str)
        .unwrap_or("daily-dungeon-001");
    let submission_id = submission
        .get("submission_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown-submission");
    let score = submission
        .get("score")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let grade = submission
        .get("grade")
        .and_then(Value::as_str)
        .unwrap_or("D");
    let reward_amount = reward
        .get("amount")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| {
            submission
                .get("reward_amount")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
        });
    let ledger_status = reward
        .get("ledger_status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    let ledger_entry_id = reward
        .get("ledger_entry_id")
        .and_then(Value::as_str)
        .unwrap_or("n/a");
    let judge_status = submission
        .get("judge_status")
        .and_then(Value::as_str)
        .unwrap_or("rubric_scored");
    let payout_status = submission
        .get("payout_status")
        .and_then(Value::as_str)
        .unwrap_or("eligible");
    let score_event_count = submission
        .get("score_events")
        .and_then(Value::as_array)
        .map(|events| events.len())
        .unwrap_or(0);
    let settlement_explanation = matrix_reward_settlement_explanation(payout_status, ledger_status);
    let body = format!(
        "🏁 League Submission 已评分\nMatch: {match_id}\nScore: {score:.1}\nGrade: {grade}\nReward: {reward_amount:.2} credit\nJudge: {judge_status} ({score_event_count} dims)\nPayout: {payout_status}\nLedger: {ledger_status}\n{settlement_explanation}\n排行榜：/rank\n奖励：/rewards"
    );

    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🏁 League Submission 已评分</h3><p><strong>Match</strong>: <code>{}</code></p><p><strong>Score</strong>: {:.1}</p><p><strong>Grade</strong>: {}</p><p><strong>Reward</strong>: {:.2} credit</p><p><strong>Judge</strong>: {} / {} dims</p><p><strong>Payout</strong>: {}</p><p><strong>Ledger</strong>: {}</p><p>{}</p><p><code>/rank</code> · <code>/rewards</code></p></blockquote>",
            escape_html(match_id),
            score,
            escape_html(grade),
            reward_amount,
            escape_html(judge_status),
            score_event_count,
            escape_html(payout_status),
            escape_html(ledger_status),
            escape_html(settlement_explanation),
        ),
        "cex_card": {
            "type": "league_submission",
            "version": 1,
            "league": "trillionnium_league",
            "match_id": match_id,
            "submission_id": submission_id,
            "score": format!("{score:.1}"),
            "grade": grade,
            "reward_amount": format!("{reward_amount:.2}"),
            "currency_unit": "credit",
            "judge_status": judge_status,
            "score_event_count": score_event_count,
            "payout_status": payout_status,
            "ledger_status": ledger_status,
            "ledger_entry_id": ledger_entry_id,
            "settlement_explanation": settlement_explanation,
        }
    })
}

fn build_league_profile_matrix_reply(value: &Value) -> Value {
    let player = value.get("player").unwrap_or(value);
    let display_name = player
        .get("display_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown-player");
    let rank_tier = player
        .get("rank_tier")
        .and_then(Value::as_str)
        .unwrap_or("Bronze I");
    let rating = player.get("rating").and_then(Value::as_i64).unwrap_or(1000);
    let xp = player.get("xp").and_then(Value::as_i64).unwrap_or(0);
    let earned = player
        .get("earned_credits")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let body = format!(
        "🪪 Trillionnium Profile\nPlayer: {display_name}\nRank: {rank_tier} / {rating} RP\nXP: {xp}\nEarned: {earned:.2} credit\n/loadout · /history · /rewards"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🪪 Trillionnium Profile</h3><p><strong>Player</strong>: {}</p><p><strong>Rank</strong>: {} / {} RP</p><p><strong>XP</strong>: {}</p><p><strong>Earned</strong>: {:.2} credit</p><p><code>/loadout</code> · <code>/history</code> · <code>/rewards</code></p></blockquote>",
            escape_html(display_name),
            escape_html(rank_tier),
            rating,
            xp,
            earned,
        ),
        "cex_card": {
            "type": "league_profile",
            "version": 1,
            "league": "trillionnium_league",
            "display_name": display_name,
            "rank_tier": rank_tier,
            "rating": rating,
            "xp": xp,
            "earned_credits": format!("{earned:.2}"),
        }
    })
}

fn build_league_rewards_matrix_reply(value: &Value) -> Value {
    let total = value
        .get("total_earned")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let count = value
        .get("rewards")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let body = format!("💎 League Rewards\nTotal Earned: {total:.2} credit\nReward Events: {count}\n继续打副本：/arena");
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>💎 League Rewards</h3><p><strong>Total Earned</strong>: {:.2} credit</p><p><strong>Reward Events</strong>: {}</p><p><code>/arena</code></p></blockquote>",
            total,
            count,
        ),
        "cex_card": {
            "type": "league_rewards",
            "version": 1,
            "league": "trillionnium_league",
            "total_earned": format!("{total:.2}"),
            "reward_count": count,
        }
    })
}

fn build_league_inventory_matrix_reply(value: &Value) -> Value {
    let item_count = value
        .get("item_count")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            value
                .get("items")
                .and_then(Value::as_array)
                .map(|items| items.len() as u64)
                .unwrap_or(0)
        });
    let total_power = value
        .get("total_power")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let top_item = value
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("No loot yet");
    let body = format!(
        "🎒 League Inventory\nItems: {item_count}\nPower: {total_power}\nTop Loot: {top_item}\n继续打本：/arena"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🎒 League Inventory</h3><p><strong>Items</strong>: {}</p><p><strong>Power</strong>: {}</p><p><strong>Top Loot</strong>: {}</p><p><code>/arena</code></p></blockquote>",
            item_count,
            total_power,
            escape_html(top_item),
        ),
        "cex_card": {
            "type": "league_inventory",
            "version": 1,
            "league": "trillionnium_league",
            "item_count": item_count,
            "total_power": total_power,
            "top_item": top_item,
        }
    })
}

fn progression_item_names(value: &Value, key: &str, only_unlocked: bool) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    !only_unlocked
                        || item
                            .get("unlocked")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                })
                .filter_map(|item| item.get("name").and_then(Value::as_str))
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn build_league_progression_matrix_reply(value: &Value) -> Value {
    let player = value.get("player").unwrap_or(value);
    let display_name = player
        .get("display_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown-player");
    let level = value.get("level").and_then(Value::as_i64).unwrap_or(1);
    let rank_title = value
        .get("rank_title")
        .and_then(Value::as_str)
        .unwrap_or("Apprentice");
    let successes = value
        .get("successful_task_count")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let to_next = value
        .get("successes_to_next_level")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let data_points = value
        .get("experience_data_points")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let current_school = value
        .get("current_school")
        .and_then(|school| school.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("City Clerks");
    let unlocked_skill_count = value
        .get("unlocked_skill_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unlocked_tool_count = value
        .get("unlocked_tool_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unlocked_skin_count = value
        .get("unlocked_skin_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let body = format!(
        "🧭 League Progression\nPlayer: {display_name}\n门派: {current_school}\nLevel: {level} / {rank_title}\n成功任务: {successes} · 下一级还需 {to_next}\n经验数据点: {data_points}\nSkills/Tools/Skins: {unlocked_skill_count}/{unlocked_tool_count}/{unlocked_skin_count}\n/skills · /tools · /skins"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🧭 League Progression</h3><p><strong>Player</strong>: {}</p><p><strong>门派</strong>: {}</p><p><strong>Level</strong>: {} / {}</p><p><strong>成功任务</strong>: {} · 下一级还需 {}</p><p><strong>经验数据点</strong>: {}</p><p><strong>Unlocks</strong>: Skills {} · Tools {} · Skins {}</p><p><code>/skills</code> · <code>/tools</code> · <code>/skins</code></p></blockquote>",
            escape_html(display_name),
            escape_html(current_school),
            level,
            escape_html(rank_title),
            successes,
            to_next,
            data_points,
            unlocked_skill_count,
            unlocked_tool_count,
            unlocked_skin_count,
        ),
        "cex_card": {
            "type": "league_progression",
            "version": 1,
            "league": "trillionnium_league",
            "display_name": display_name,
            "current_school": current_school,
            "level": level,
            "rank_title": rank_title,
            "successful_task_count": successes,
            "successes_to_next_level": to_next,
            "experience_data_points": data_points,
            "unlocked_skill_count": unlocked_skill_count,
            "unlocked_tool_count": unlocked_tool_count,
            "unlocked_skin_count": unlocked_skin_count,
        }
    })
}

fn build_league_skills_matrix_reply(value: &Value) -> Value {
    let total = value
        .get("skill_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unlocked = value
        .get("unlocked_skill_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let names = progression_item_names(value, "skills", true);
    let top = names
        .first()
        .map(String::as_str)
        .unwrap_or("Reality Scouting");
    let list = if names.is_empty() {
        "暂无已解锁技能".to_string()
    } else {
        names.join(" / ")
    };
    let body = format!("✨ League Skills\nUnlocked: {unlocked}/{total}\n已解锁：{list}\nTop Skill: {top}\n/progression 查看等级");
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>✨ League Skills</h3><p><strong>Unlocked</strong>: {}/{}</p><p>{}</p><p><strong>Top Skill</strong>: {}</p><p><code>/progression</code></p></blockquote>",
            unlocked,
            total,
            escape_html(&list),
            escape_html(top),
        ),
        "cex_card": {"type": "league_skills", "version": 1, "league": "trillionnium_league", "skill_count": total, "unlocked_skill_count": unlocked, "top_skill": top}
    })
}

fn build_league_tools_matrix_reply(value: &Value) -> Value {
    let total = value.get("tool_count").and_then(Value::as_u64).unwrap_or(0);
    let unlocked = value
        .get("unlocked_tool_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let earned_items = value
        .get("earned_item_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let earned_power = value
        .get("earned_item_power")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let names = progression_item_names(value, "tools", true);
    let list = if names.is_empty() {
        "暂无已解锁装备".to_string()
    } else {
        names.join(" / ")
    };
    let body = format!("🧰 League Tools / 装备\nCatalog: {unlocked}/{total}\nLoot Items: {earned_items} · Power {earned_power}\n已解锁：{list}\n/inventory 查看掉落");
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🧰 League Tools / 装备</h3><p><strong>Catalog</strong>: {}/{}</p><p><strong>Loot Items</strong>: {} · Power {}</p><p>{}</p><p><code>/inventory</code></p></blockquote>",
            unlocked,
            total,
            earned_items,
            earned_power,
            escape_html(&list),
        ),
        "cex_card": {"type": "league_tools", "version": 1, "league": "trillionnium_league", "tool_count": total, "unlocked_tool_count": unlocked, "earned_item_count": earned_items, "earned_item_power": earned_power}
    })
}

fn build_league_skins_matrix_reply(value: &Value) -> Value {
    let total = value.get("skin_count").and_then(Value::as_u64).unwrap_or(0);
    let unlocked = value
        .get("unlocked_skin_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let loadout_agent_count = value
        .get("loadout_agent_count")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let names = progression_item_names(value, "skins", true);
    let list = if names.is_empty() {
        "暂无已解锁皮肤".to_string()
    } else {
        names.join(" / ")
    };
    let body = format!("🎭 League Skins / Multi-Agent 能力\nUnlocked: {unlocked}/{total}\nLoadout Agents: {loadout_agent_count}\n已解锁：{list}\n/draft 可以改变 multi-agent 形态");
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🎭 League Skins / Multi-Agent 能力</h3><p><strong>Unlocked</strong>: {}/{}</p><p><strong>Loadout Agents</strong>: {}</p><p>{}</p><p><code>/draft</code></p></blockquote>",
            unlocked,
            total,
            loadout_agent_count,
            escape_html(&list),
        ),
        "cex_card": {"type": "league_skins", "version": 1, "league": "trillionnium_league", "skin_count": total, "unlocked_skin_count": unlocked, "loadout_agent_count": loadout_agent_count}
    })
}

fn build_league_history_matrix_reply(value: &Value) -> Value {
    let battles = value
        .get("battles")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let submissions = value
        .get("submissions")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let body =
        format!("📜 League History\nBattles: {battles}\nSubmissions: {submissions}\n继续：/arena");
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>📜 League History</h3><p><strong>Battles</strong>: {}</p><p><strong>Submissions</strong>: {}</p><p><code>/arena</code></p></blockquote>",
            battles,
            submissions,
        ),
        "cex_card": {
            "type": "league_history",
            "version": 1,
            "league": "trillionnium_league",
            "battle_count": battles,
            "submission_count": submissions,
        }
    })
}

fn build_plain_matrix_reply(body: &str) -> Value {
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!("<p>{}</p>", escape_html(body)),
    })
}

fn build_help_matrix_reply() -> Value {
    build_plain_matrix_reply(
        "可用命令:\n/app - 打开客户端超级入口（地图/对战/社交/钱包）\n/feed [all|events|tasks|contracts|completion|commerce|social] - 查看 Unified Feed Timeline 的 Matrix 动态投影\n/duel nearby <出招> - 面对面 Agent 对战\n/social - 社交联系人/房间\n/league - 进入 Trillionnium League\n/world - 进入 Trillionnium World 开放世界\n/map 或 /look - 查看细地图和当前位置\n/go <direction|node-id> - 在地图上移动\n/world action <自由行动> - 在现实镜像世界里行动/建造/经营\n/assets - 查看 World 资产\n/upgrade <asset-id|latest> <升级内容> - 升级 World 资产\n/companies - 查看公司\n/company <asset-id|latest> <公司方案> - 把资产变成公司/店铺\n/shops - 查看店铺和货架\n/sell <company-id|latest> <服务/商品> - 上架服务\n/buy <listing-id|latest> <需求> - 购买/雇佣货架服务并生成 work order\n/work - 查看购买与 work orders\n/work deliver <work-id|latest> <交付内容> - 卖方提交 work order 交付\n/work accept <work-id|latest> <验收内容> - 买方验收完成 work order\n/work reject <work-id|latest> <拒收原因> - 买方拒收并退回预留款\n/work reopen <work-id|latest> <返工要求> - 买方重新预留资金并打开返工/重交付\n/work cancel <work-id|latest> <取消原因> - 买方在交付前取消并退回预留款\n/factions - 查看 World 阵营声望\n/contract <委托内容> - 把现实需求登记成 World Contract 并创建 CEX 任务\n/complete <contract-id> <交付内容> - 完成 World Contract、评分并结算\n/craft <建造内容> - 进入 Trillionnium Craft 工坊建造\n/season - 赛季\n/arena - 查看赛场\n/quest - 今日副本\n/guild - 公会列表，/guild <guild-id> 加入\n/raid - 团本列表，/raid <raid-id> <行动> 贡献团本\n/team - 团本队伍，/team <raid-id> <role> 认领职责\n/draft <hero...> - 锁定 Agent 英雄阵容\n/join <match-id> - 加入赛场\n/battle <match-id> <行动> - 在赛场中出招并创建 CEX 执行\n/submit <match-id> <提交内容> - 交卷评分并领取奖励\n/profile - 玩家档案\n/progression 或 /level - 门派/等级/经验总览\n/skills - 技能树\n/tools - 装备/工具\n/skins - 皮肤/multi-agent 能力\n/rank - 排行榜\n/loadout - Agent 阵容\n/rewards - 奖励记录\n/inventory - 背包/装备\n/history - 战斗历史\n/task <内容> [cap=<能力id>] [account=<账户id>] - 创建普通任务\n/status <task-id> - 查询任务状态\n/balance 或 /wallet - 查看余额\n/plans 或 /套餐 - 查看套餐",
    )
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::{
        build_consumer_entry_session_auth_headers, build_league_home_matrix_reply,
        build_matrix_event_rate_limit_key, build_matrix_request_fingerprint_from_body,
        build_wallet_matrix_reply, consumer_entry_session_auth_governance_overview_json,
        extract_matrix_body, get_consumer_entry_session_auth_status, health,
        load_consumer_entry_session_auth_issuer_registry_revision_approval_state,
        load_rate_limit_cache, load_session_auth_issuer_registry, metrics, parse_matrix_command,
        prune_rate_limit_cache, prune_recent_event_cache,
        reload_consumer_entry_session_auth_runtime, validate_consumer_entry_session_auth_runtime,
        validate_text_payload, AppState, AppStateInner, MatrixAdapterConfig, MatrixEntryMetrics,
        MatrixEventEnvelope, ParsedCommand, RateLimitCache, RecentEventCache, RuntimeProfile,
        SessionAuthIssuerRegistryIssuer, SessionAuthIssuerRegistryMetadata,
        SessionAuthIssuerRegistryRevisionApprovalState, SessionAuthIssuerRegistryRuntimeState,
        SessionAuthIssuerRegistrySelection, UserSessionAuthClaims, DEFAULT_MAX_TEXT_CHARS,
        USER_SESSION_ASSERTION_HEADER, USER_SESSION_SIGNATURE_HEADER,
    };
    use axum::{
        body::to_bytes,
        extract::State,
        http::{HeaderMap, StatusCode},
        Json,
    };
    use base64::Engine as _;
    use chrono::Utc;
    use reqwest::Client;
    use serde_json::{json, Value};
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, RwLock as StdRwLock};
    use tokio::sync::Mutex;

    fn matrix_route_command_body(command: &str) -> String {
        let trimmed = command.trim();
        for prefix in [
            "/upgrade latest",
            "/company latest",
            "/sell latest",
            "/buy latest",
            "/work deliver latest",
            "/work accept latest",
            "/work reject latest",
            "/work reopen latest",
            "/work cancel latest",
            "/world action",
            "/contract",
        ] {
            if let Some(body) = trimmed.strip_prefix(prefix) {
                return body.trim().to_string();
            }
        }
        if let Some(rest) = trimmed.strip_prefix("/complete ") {
            return rest
                .trim()
                .split_once(' ')
                .map(|(_, body)| body.trim().to_string())
                .unwrap_or_default();
        }
        trimmed.to_string()
    }

    fn assert_matrix_route_hidden_anchor_ready(label: &str, body: &str) {
        let lower = body.to_ascii_lowercase();
        assert!(
            super::matrix_route_has_delivery_anchor(body, &lower),
            "{label} missing delivery/customer anchor: {body}"
        );
        assert!(
            super::matrix_route_has_evidence_anchor(body, &lower),
            "{label} missing evidence anchor: {body}"
        );
        assert!(
            super::matrix_route_has_risk_anchor(body, &lower),
            "{label} missing risk anchor: {body}"
        );
        assert!(
            super::matrix_route_has_next_anchor(body, &lower),
            "{label} missing next-action anchor: {body}"
        );
        assert!(
            super::matrix_route_has_review_anchor(body, &lower),
            "{label} missing review/self-check anchor: {body}"
        );
    }

    #[test]
    fn trillionnium_world_overview_examples_are_playability_anchored() {
        for (label, command) in [
            (
                "asset_upgrade_example",
                super::TRILLIONNIUM_ASSET_UPGRADE_EXAMPLE_COMMAND,
            ),
            (
                "company_example",
                super::TRILLIONNIUM_COMPANY_EXAMPLE_COMMAND,
            ),
            ("sell_example", super::TRILLIONNIUM_SELL_EXAMPLE_COMMAND),
            ("buy_example", super::TRILLIONNIUM_BUY_EXAMPLE_COMMAND),
            (
                "work_deliver_example",
                super::TRILLIONNIUM_WORK_DELIVER_EXAMPLE_COMMAND,
            ),
            (
                "work_accept_example",
                super::TRILLIONNIUM_WORK_ACCEPT_EXAMPLE_COMMAND,
            ),
            (
                "work_reject_example",
                super::TRILLIONNIUM_WORK_REJECT_EXAMPLE_COMMAND,
            ),
            (
                "work_reopen_example",
                super::TRILLIONNIUM_WORK_REOPEN_EXAMPLE_COMMAND,
            ),
            (
                "work_cancel_example",
                super::TRILLIONNIUM_WORK_CANCEL_EXAMPLE_COMMAND,
            ),
        ] {
            assert_matrix_route_hidden_anchor_ready(label, &matrix_route_command_body(command));
        }

        for (label, reply, expected_commands) in [
            (
                "assets_reply",
                super::build_trillionnium_world_assets_matrix_reply(&json!({})),
                vec![super::TRILLIONNIUM_ASSET_UPGRADE_EXAMPLE_COMMAND],
            ),
            (
                "companies_reply",
                super::build_trillionnium_world_companies_matrix_reply(&json!({})),
                vec![super::TRILLIONNIUM_COMPANY_EXAMPLE_COMMAND],
            ),
            (
                "shops_reply",
                super::build_trillionnium_world_shops_matrix_reply(&json!({})),
                vec![super::TRILLIONNIUM_SELL_EXAMPLE_COMMAND],
            ),
            (
                "commerce_reply",
                super::build_trillionnium_world_commerce_matrix_reply(&json!({})),
                vec![
                    super::TRILLIONNIUM_BUY_EXAMPLE_COMMAND,
                    super::TRILLIONNIUM_WORK_DELIVER_EXAMPLE_COMMAND,
                    super::TRILLIONNIUM_WORK_ACCEPT_EXAMPLE_COMMAND,
                    super::TRILLIONNIUM_WORK_REJECT_EXAMPLE_COMMAND,
                    super::TRILLIONNIUM_WORK_REOPEN_EXAMPLE_COMMAND,
                    super::TRILLIONNIUM_WORK_CANCEL_EXAMPLE_COMMAND,
                ],
            ),
        ] {
            let body = reply
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let formatted_body = reply
                .get("formatted_body")
                .and_then(Value::as_str)
                .unwrap_or_default();
            assert!(
                !body.contains("<方案>"),
                "{label} still has terse body placeholder"
            );
            assert!(
                !formatted_body.contains("&lt;方案&gt;")
                    && !formatted_body.contains("&lt;需求&gt;")
                    && !formatted_body.contains("&lt;商品/服务&gt;"),
                "{label} still has terse formatted placeholder: {formatted_body}"
            );
            for command in expected_commands {
                assert!(
                    body.contains(command),
                    "{label} body missing {command}: {body}"
                );
                assert!(
                    formatted_body.contains(&super::escape_html(command)),
                    "{label} formatted body missing {command}: {formatted_body}"
                );
            }
        }
    }

    #[test]
    fn trillionnium_matrix_reward_states_are_player_readable() {
        let settled_submission = super::build_league_submission_matrix_reply(&json!({
            "submission": {
                "match_id": "daily-dungeon-001",
                "submission_id": "sub-settled",
                "score": 88.0,
                "grade": "A",
                "judge_status": "rubric_hidden_pipeline_v2",
                "payout_status": "eligible",
                "score_events": [{"dimension": "delivery"}]
            },
            "reward": {"amount": 4.4, "ledger_status": "settled", "ledger_entry_id": "entry-1"}
        }));
        let settled_body = settled_submission
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(settled_body.contains("账本已结算"));
        assert!(settled_body.contains("奖励和进度已安全入账"));
        assert_eq!(
            settled_submission
                .get("cex_card")
                .and_then(|card| card.get("settlement_explanation"))
                .and_then(Value::as_str),
            Some(
                "说明：账本已结算，奖励和进度已安全入账；现在可以查看奖励、排名或推进下一条路线。"
            )
        );

        let held_submission = super::build_league_submission_matrix_reply(&json!({
            "submission": {
                "match_id": "daily-dungeon-001",
                "submission_id": "sub-held",
                "score": 91.0,
                "grade": "A",
                "judge_status": "rubric_hidden_pipeline_v2",
                "payout_status": "review_hold",
                "score_events": [{"dimension": "delivery"}]
            },
            "reward": {"amount": 4.5, "ledger_status": "pending"}
        }));
        let held_body = held_submission
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(held_body.contains("命中复核"));
        assert!(held_body.contains("奖励暂缓"));
        assert!(held_body.contains("证据包"));
        assert!(held_body.contains("自检复盘"));

        let contract_completion =
            super::build_trillionnium_world_contract_completion_matrix_reply(&json!({
                "completion": {
                    "contract_id": "world-contract-001",
                    "score": 86.0,
                    "reward_amount": 4.3,
                    "ledger_status": "settled",
                    "judge_status": "rubric_hidden_pipeline_v2"
                }
            }));
        assert!(contract_completion
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("奖励和进度已安全入账"));

        let purchase = super::build_trillionnium_world_purchase_matrix_reply(&json!({
            "purchase": {"purchase_id": "world-purchase-001", "price_credits": 12, "status": "open", "ledger_status": "settled", "buyer_ledger_status": "reserved"},
            "work_order": {"work_order_id": "world-work-001"}
        }));
        assert!(purchase
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("买家托管已锁定、卖家结算已确认"));

        let acceptance = super::build_trillionnium_world_work_acceptance_matrix_reply(&json!({
            "work_order": {"work_order_id": "world-work-001"},
            "acceptance": {"acceptance_id": "world-acceptance-001", "status": "accepted", "reputation_delta": 10},
            "purchase": {"buyer_consume_status": "consumed"}
        }));
        assert!(acceptance
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("买家托管已消费"));
    }

    #[test]
    fn trillionnium_work_next_steps_are_full_player_commands() {
        let delivery = super::build_trillionnium_world_work_delivery_matrix_reply(&json!({
            "work_order": {"work_order_id": "world-work-accept-anchor"},
            "delivery": {"delivery_id": "world-delivery-001", "score": 84.0, "status": "delivered", "judge_status": "rubric_hidden_pipeline_v2"}
        }));
        let reopen = super::build_trillionnium_world_work_reopen_matrix_reply(&json!({
            "work_order": {"work_order_id": "world-work-deliver-anchor", "status": "open"},
            "purchase": {"status": "reopened_reserved"},
            "reopen": {"reopen_id": "world-reopen-001", "status": "reopened", "reserve_status": "reserved"}
        }));

        for (label, reply) in [("delivery", delivery), ("reopen", reopen)] {
            let body = reply
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let formatted_body = reply
                .get("formatted_body")
                .and_then(Value::as_str)
                .unwrap_or_default();
            assert!(!body.contains("<验收>") && !body.contains("<返工交付>"));
            assert!(
                !formatted_body.contains("&lt;验收&gt;")
                    && !formatted_body.contains("&lt;返工交付&gt;")
            );
            let command = reply
                .get("cex_card")
                .and_then(|card| card.get("next_step_command"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            assert!(
                body.contains(command),
                "{label} body missing {command}: {body}"
            );
            assert!(
                formatted_body.contains(&super::escape_html(command)),
                "{label} formatted body missing command: {formatted_body}"
            );
            assert_matrix_route_hidden_anchor_ready(label, command);
        }
    }

    #[test]
    fn specialize_route_opportunity_uses_surface_specific_commands() {
        let base = super::specialize_route_opportunity(
            "world",
            "repeat_order_upsell_referral",
            "Completion world-contract-completion-001 · settled",
            "Capture client feedback and archive final delivery evidence.",
            "Use this completion as a springboard for the next route.",
            "Archive proof, ask for testimonial, and package the next offer.",
            "/sell latest generic",
            "task-route-001",
            "zbj-market-gate",
            "1 events → 1 contracts → 1 completions · latest completion/settled",
        );
        let assets = super::specialize_route_opportunity(
            "assets",
            "repeat_order_upsell_referral",
            "Completion world-contract-completion-001 · settled",
            "Capture client feedback and archive final delivery evidence.",
            "Use this completion as a springboard for the next route.",
            "Archive proof, ask for testimonial, and package the next offer.",
            "/sell latest generic",
            "task-route-001",
            "zbj-market-gate",
            "1 events → 1 contracts → 1 completions · latest completion/settled",
        );
        let companies = super::specialize_route_opportunity(
            "companies",
            "repeat_order_upsell_referral",
            "Completion world-contract-completion-001 · settled",
            "Capture client feedback and archive final delivery evidence.",
            "Use this completion as a springboard for the next route.",
            "Archive proof, ask for testimonial, and package the next offer.",
            "/sell latest generic",
            "task-route-001",
            "zbj-market-gate",
            "1 events → 1 contracts → 1 completions · latest completion/settled",
        );
        let shops = super::specialize_route_opportunity(
            "shops",
            "repeat_order_upsell_referral",
            "Completion world-contract-completion-001 · settled",
            "Capture client feedback and archive final delivery evidence.",
            "Use this completion as a springboard for the next route.",
            "Archive proof, ask for testimonial, and package the next offer.",
            "/sell latest generic",
            "task-route-001",
            "zbj-market-gate",
            "1 events → 1 contracts → 1 completions · latest completion/settled",
        );

        assert!(base.2.starts_with("/world action "));
        assert!(assets.2.starts_with("/upgrade latest "));
        assert!(companies.2.starts_with("/company latest "));
        assert!(shops.2.starts_with("/sell latest "));
        for (label, command) in [
            ("world", &base.2),
            ("assets", &assets.2),
            ("companies", &companies.2),
            ("shops", &shops.2),
        ] {
            assert_matrix_route_hidden_anchor_ready(label, &matrix_route_command_body(command));
        }
        assert!(assets.0.contains("Asset") || assets.0.contains("asset"));
        assert!(companies.0.contains("company") || companies.0.contains("Company"));
        assert!(shops.0.contains("listing") || shops.0.contains("market"));
    }

    #[test]
    fn specialize_route_opportunity_uses_recovery_specific_copy() {
        let world = super::specialize_route_opportunity(
            "world",
            "reopen_recovery",
            "Work order world-work-order-001 · reopened",
            "Close the evidence gap and relock acceptance criteria.",
            "Recover the route with a tighter revision pass.",
            "Capture revision scope, owner, deadline, and next command.",
            "/sell latest generic",
            "task-route-002",
            "zbj-market-gate",
            "2 events → 1 work orders → reopened",
        );
        let listing = super::specialize_route_opportunity(
            "listing",
            "smaller_scope_requalification",
            "Work order world-work-order-002 · rejected",
            "Reduce scope and rebuild buyer confidence.",
            "Recover the route with a smaller scope offer.",
            "Reset deliverable size, evidence bar, and next command.",
            "/sell latest generic",
            "task-route-003",
            "zbj-market-gate",
            "2 events → 1 work orders → rejected",
        );

        assert!(
            world.0.contains("recover")
                || world.0.contains("Recovery")
                || world.0.contains("revision")
        );
        assert!(
            world.1.contains("revision")
                || world.1.contains("reopen")
                || world.1.contains("evidence gap")
        );
        assert!(world.2.starts_with("/world action "));
        assert_matrix_route_hidden_anchor_ready(
            "recovery_world",
            &matrix_route_command_body(&world.2),
        );
        assert!(
            listing.0.contains("scope")
                || listing.0.contains("listing")
                || listing.0.contains("offer")
        );
        assert!(
            listing.1.contains("smaller")
                || listing.1.contains("requal")
                || listing.1.contains("scope")
        );
        assert!(listing.2.starts_with("/sell latest "));
        assert_matrix_route_hidden_anchor_ready(
            "recovery_listing",
            &matrix_route_command_body(&listing.2),
        );
    }

    #[test]
    fn route_cards_preserve_focus_node_fields() {
        let reply = super::build_trillionnium_client_app_matrix_reply(&json!({
            "module_count": 1,
            "modules": [{"name": "World Map"}],
            "real_world_map_engine": {
                "engine_id": "leaflet_openstreetmap_v1",
                "tile_provider": "OpenStreetMap",
                "renderer_adapter": {
                    "adapter_id": "leaflet_renderer_adapter_v1",
                    "adapter_contract_version": 1,
                    "runtime_handle_name": "mapRuntime",
                    "future_engine_candidate": "maplibre_gl_v1",
                    "adapter_contract": {"supports_future_engine_swap": true}
                },
                "planned_upgrade_engine": {
                    "engine_id": "maplibre_gl_v1",
                    "gating_contract": "renderer_adapter.adapter_contract_version >= 1"
                }
            },
            "map": {"current_node_id": "mirror-city-square"},
            "map_hub": {
                "route_preview": {"item_count": 1, "task_linked_count": 1},
                "route_story": {
                    "preview_item_count": 4,
                    "task_linked_count": 2,
                    "task_graph_count": 1,
                    "next_task_id": "task-route-focus-story",
                    "next_action_label": "Prepare delivery lane from story",
                    "next_panel_id": "world-commerce-panel",
                    "next_command_hint": "/world action story follow-up",
                    "next_location_id": "zbj-market-gate-story",
                    "next_node_id": "delivery-dock-story",
                    "next_opportunity_node_id": "client-board-story",
                    "next_stage_summary": "story stage summary",
                    "next_opportunity_kind": "repeat_order_upsell_referral",
                    "next_outcome_summary": "Story outcome summary",
                    "next_feedback_focus": "Story feedback focus",
                    "next_opportunity_hint": "Story opportunity hint",
                    "next_opportunity_playbook": "Story opportunity playbook",
                    "next_opportunity_command": "/sell latest Story route offer",
                    "next_opportunity_target": {
                        "action_label": "Open listing lane",
                        "panel_id": "world-listings-panel",
                        "input_id": "world-listing-company-id",
                        "input_value": "latest",
                        "textarea_id": "world-listing-body",
                        "body": "Story route offer",
                        "node_id": "client-board-story"
                    }
                },
                "route_task_graph": {"task_count": 1, "tasks": [{
                    "task_id": "task-route-focus-raw",
                    "suggested_action_label": "Prepare delivery lane raw",
                    "suggested_panel_id": "world-commerce-panel",
                    "suggested_matrix_command": "/world action raw follow-up",
                    "latest_location_id": "zbj-market-gate-raw",
                    "suggested_node_id": "delivery-dock-raw",
                    "next_opportunity_node_id": "client-board-raw",
                    "route_stage_summary": "raw stage summary",
                    "outcome_summary": "Work order world-work-focus-raw · delivered",
                    "feedback_focus": "Raw feedback focus.",
                    "next_opportunity_kind": "repeat_order_upsell_referral",
                    "next_opportunity_hint": "Raw opportunity hint.",
                    "next_opportunity_playbook": "Raw opportunity playbook.",
                    "next_opportunity_command": "/sell latest 原始复购方案"
                }]},
                "route_runner_handoff": {
                    "contract_version": "trillionnium_route_runner_handoff_v1",
                    "runner_count": 2,
                    "avatar_task_route_count": 3,
                    "reward_claim_action_count": 2,
                    "next_route_action_count": 2,
                    "reward_claim_ready_count": 1,
                    "next_route_ready_count": 1,
                    "supports_checkpoint_reward_history": true,
                    "supports_route_runner_lifecycle": true,
                    "supports_route_runner_reward_claim_actions": true,
                    "supports_route_runner_next_route_actions": true,
                    "lifecycle_contract_version": "trillionnium_route_runner_lifecycle_v1",
                    "supports_route_mastery_progression": true,
                    "route_mastery_contract_version": "trillionnium_route_mastery_v1",
                    "route_mastery_runner_count": 2,
                    "first_route_mastery_xp": 694,
                    "first_route_mastery_tier": "checkpoint_adept",
                    "first_route_mastery_tier_label": "Checkpoint Adept / 检查点熟手",
                    "first_route_mastery_streak": 2,
                    "first_route_mastery_next_goal": "Claim the rating/reward, then chain the next route with the same evidence and self-review anchors.",
                    "first_route_mastery_summary": "Route mastery: Checkpoint Adept / 检查点熟手 · 694 XP",
                    "first_runner_id": "avatar-route-runner:@alice:local.dev:task-route-focus-story",
                    "first_task_id": "task-route-focus-story",
                    "first_to_node_id": "delivery-dock-story",
                    "first_latest_location_id": "zbj-market-gate-story",
                    "first_progress_label": "82% route progress / 82% 路线进度",
                    "first_telemetry_summary": "82% complete · 120m remaining · ETA 7 min",
                    "first_reward_claim_label": "Claim rating/reward / 领取评级奖励",
                    "first_reward_claim_status": "claimable_after_evidence",
                    "first_reward_claim_action_body": "Claim rating/reward for task task-route-focus-story: submit the deliverable, evidence package, risk controls, next action, and self-review for final reward settlement.",
                    "first_next_route_label": "Open next route / 开启下一条路线",
                    "first_next_route_status": "next_route_ready_after_reward_claim",
                    "first_next_route_action_body": "Open next route after task task-route-focus-story: choose the next Trillionnium World Map node and carry deliverable, evidence package, risk controls, next action, and self-review into the follow-up bounty.",
                    "first_next_route_sequence_summary": "After reward claim, open the next Trillionnium World Map route with the same deliverable → evidence → risk controls → next action → self-review anchors.",
                    "summary": "Route runner handoff: 2 runners · 2 reward claims · 2 next-route actions · next Claim rating/reward / Open next route",
                    "handoff_prompt": "Claim rating/reward, then open the next route with deliverable → evidence → risk controls → next action → self-review anchors."
                }
            }
        }));

        let card = reply.get("cex_card").unwrap();
        assert_eq!(
            card.get("route_next_task_id").and_then(Value::as_str),
            Some("task-route-focus-story")
        );
        assert_eq!(
            card.get("route_next_node_id").and_then(Value::as_str),
            Some("delivery-dock-story")
        );
        assert_eq!(
            card.get("route_next_location_id").and_then(Value::as_str),
            Some("zbj-market-gate-story")
        );
        assert_eq!(
            card.get("route_next_opportunity_node_id")
                .and_then(Value::as_str),
            Some("client-board-story")
        );
        assert_eq!(
            card.get("route_next_opportunity_action_label")
                .and_then(Value::as_str),
            Some("Open world action lane")
        );
        assert_eq!(
            card.get("route_next_opportunity_panel_id")
                .and_then(Value::as_str),
            Some("world-action-console")
        );
        assert_eq!(
            card.get("route_next_opportunity_textarea_id")
                .and_then(Value::as_str),
            Some("world-action-body")
        );
        let route_next_command_hint = card
            .get("route_next_command_hint")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(route_next_command_hint.starts_with("/world action story follow-up"));
        assert_matrix_route_hidden_anchor_ready(
            "route_story_command_hint",
            &matrix_route_command_body(route_next_command_hint),
        );
        assert_eq!(
            card.get("route_story")
                .and_then(|story| story.get("next_task_id"))
                .and_then(Value::as_str),
            Some("task-route-focus-story")
        );
        assert_eq!(
            card.get("route_story")
                .and_then(|story| story.get("next_opportunity_target"))
                .and_then(|target| target.get("node_id"))
                .and_then(Value::as_str),
            Some("client-board-story")
        );
        assert_eq!(
            card.get("route_next_opportunity_target_node_id")
                .and_then(Value::as_str),
            Some("client-board-story")
        );
        assert_eq!(
            card.get("map_renderer_adapter_id").and_then(Value::as_str),
            Some("leaflet_renderer_adapter_v1")
        );
        assert_eq!(
            card.get("has_first_playable_onboarding")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            card.get("onboarding_contract_version")
                .and_then(Value::as_str),
            Some("trillionnium_first_playable_onboarding_v1")
        );
        assert_eq!(
            card.get("onboarding_completion_target")
                .and_then(Value::as_str),
            Some("first_playable_loop_100")
        );
        let body = reply
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(body.contains("Quick Path:"));
        assert!(body.contains("run one bounty"));
        assert!(body.contains("Start Here:"));
        assert!(body.contains("Start Command:"));
        assert!(body.contains("Full Commands:"));
        assert!(body.contains("Next Command:"));
        assert!(body.contains("Opportunity Command:"));
        assert!(body.contains("Runner Handoff:"));
        assert!(body.contains("Next Route: Open next route"));
        assert!(!body.contains("Renderer Adapter:"));
        assert!(!body.contains("future maplibre_gl_v1"));
        assert!(!body.contains("Primary Entry:"));
        assert!(body.contains("/world action story follow-up"));
        assert_eq!(
            card.get("onboarding_start_command")
                .and_then(Value::as_str),
            Some("/world action Start the first bounty: define customer deliverable, evidence package, risk controls, next action, and self-review.")
        );
        assert_eq!(
            card.get("onboarding_quick_path_label")
                .and_then(Value::as_str),
            Some("Quick Path")
        );
        assert_eq!(
            card.get("onboarding_quick_path_summary")
                .and_then(Value::as_str),
            Some("Choose map focus → run one bounty → submit/review reward")
        );
        assert_eq!(
            card.get("onboarding_command_disclosure_label")
                .and_then(Value::as_str),
            Some("Full Commands")
        );
        assert!(card
            .get("onboarding_command_disclosure")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("Use these when you are ready"));
        assert_eq!(
            card.get("map_runtime_handle_name").and_then(Value::as_str),
            Some("mapRuntime")
        );
        assert_eq!(
            card.get("avatar_route_runner_count")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            card.get("route_runner_next_route_action_count")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            card.get("route_runner_next_route_status")
                .and_then(Value::as_str),
            Some("next_route_ready_after_reward_claim")
        );
        assert_eq!(
            card.get("route_runner_mastery_contract_version")
                .and_then(Value::as_str),
            Some("trillionnium_route_mastery_v1")
        );
        assert_eq!(
            card.get("route_runner_first_mastery_tier")
                .and_then(Value::as_str),
            Some("checkpoint_adept")
        );
        assert!(card
            .get("route_runner_first_mastery_next_goal")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("evidence"));
        assert!(card
            .get("route_runner_next_route_action_body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains(
                "deliverable, evidence package, risk controls, next action, and self-review"
            ));
        assert_eq!(
            card.get("route_runner_handoff")
                .and_then(|handoff| handoff.get("first_task_id"))
                .and_then(Value::as_str),
            Some("task-route-focus-story")
        );
        assert_eq!(
            card.get("map_renderer_supports_future_engine_swap")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(card
            .get("route_next_opportunity_body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .contains("增长机会推进任务 task-route-focus-story"));

        let route_task_graph = json!({"task_count": 1, "tasks": [{
            "task_id": "task-route-focus-002",
            "suggested_action_label": "Prepare delivery lane",
            "suggested_panel_id": "world-commerce-panel",
            "suggested_matrix_command": "/world action 记录交付准备、证据包和下一步。",
            "latest_location_id": "zbj-market-gate",
            "suggested_node_id": "delivery-dock",
            "next_opportunity_node_id": "client-board",
            "route_stage_summary": "2 events → 1 work orders → delivered",
            "outcome_summary": "Work order world-work-focus-002 · delivered",
            "feedback_focus": "Capture the delivery proof and decide the next lane.",
            "next_opportunity_kind": "repeat_order_upsell_referral",
            "next_opportunity_hint": "Use the result to tee up a tighter next offer.",
            "next_opportunity_playbook": "Archive proof, extract testimonial hooks, and line up the next proposal.",
            "next_opportunity_command": "/sell latest 复购方案：围绕本次交付补齐升级包、推荐语和下一步。"
        }]});
        let route_runner_handoff = json!({
            "contract_version": "trillionnium_route_runner_handoff_v1",
            "runner_count": 2,
            "avatar_task_route_count": 2,
            "reward_claim_action_count": 2,
            "next_route_action_count": 2,
            "reward_claim_ready_count": 1,
            "next_route_ready_count": 1,
            "supports_checkpoint_reward_history": true,
            "supports_route_runner_lifecycle": true,
            "supports_route_runner_reward_claim_actions": true,
            "supports_route_runner_next_route_actions": true,
            "lifecycle_contract_version": "trillionnium_route_runner_lifecycle_v1",
            "supports_route_mastery_progression": true,
            "route_mastery_contract_version": "trillionnium_route_mastery_v1",
            "route_mastery_runner_count": 2,
            "first_route_mastery_xp": 694,
            "first_route_mastery_tier": "checkpoint_adept",
            "first_route_mastery_tier_label": "Checkpoint Adept / 检查点熟手",
            "first_route_mastery_streak": 2,
            "first_route_mastery_next_goal": "Claim the rating/reward, then chain the next route with the same evidence and self-review anchors.",
            "first_route_mastery_summary": "Route mastery: Checkpoint Adept / 检查点熟手 · 694 XP",
            "first_runner_id": "avatar-route-runner:@alice:local.dev:task-route-focus-002",
            "first_task_id": "task-route-focus-002",
            "first_to_node_id": "delivery-dock",
            "first_latest_location_id": "zbj-market-gate",
            "first_progress_label": "82% route progress / 82% 路线进度",
            "first_telemetry_summary": "82% complete · 120m remaining · ETA 7 min",
            "first_reward_claim_label": "Claim rating/reward / 领取评级奖励",
            "first_reward_claim_status": "claimable_after_evidence",
            "first_reward_claim_action_body": "Claim rating/reward for task task-route-focus-002: submit deliverable, evidence package, risk controls, next action, and self-review.",
            "first_next_route_label": "Open next route / 开启下一条路线",
            "first_next_route_status": "next_route_ready_after_reward_claim",
            "first_next_route_action_body": "Open next route after task task-route-focus-002: carry deliverable, evidence package, risk controls, next action, and self-review into the follow-up bounty.",
            "first_next_route_sequence_summary": "After reward claim, open the next Trillionnium World Map route with the same deliverable → evidence → risk controls → next action → self-review anchors.",
            "summary": "Route runner handoff: 2 runners · 2 reward claims · 2 next-route actions · next Claim rating/reward / Open next route",
            "handoff_prompt": "Claim rating/reward, then open the next route with deliverable → evidence → risk controls → next action → self-review anchors."
        });
        let world_reply = super::build_trillionnium_world_matrix_reply(&json!({
            "counts": {"zones": 4, "locations": 4, "assets": 1, "companies": 1, "shops": 1, "listings": 1, "purchases": 1, "work_orders": 1, "factions": 4, "events": 1},
            "current_node_id": "mirror-city-square",
            "route_preview": {"item_count": 1, "task_linked_count": 1},
            "route_task_graph": route_task_graph.clone(),
            "route_runner_handoff": route_runner_handoff.clone()
        }));
        let map_reply = super::build_trillionnium_world_map_matrix_reply(&json!({
            "counts": {"map_nodes": 8},
            "current_node": {"node_id": "mirror-city-square", "name": "镜像城市广场", "description": "map"},
            "real_world_map_engine": {
                "engine_id": "leaflet_openstreetmap_v1",
                "tile_provider": "OpenStreetMap",
                "mirror_scope": "global_real_world_tiles",
                "active_region_id": "cn-shanghai-core",
                "renderer_adapter": {
                    "adapter_id": "leaflet_renderer_adapter_v1",
                    "adapter_contract_version": 1,
                    "runtime_handle_name": "mapRuntime",
                    "future_engine_candidate": "maplibre_gl_v1",
                    "adapter_contract": {"supports_future_engine_swap": true}
                },
                "planned_upgrade_engine": {
                    "engine_id": "maplibre_gl_v1",
                    "gating_contract": "renderer_adapter.adapter_contract_version >= 1"
                }
            },
            "route_preview": {"item_count": 1, "task_linked_count": 1},
            "route_task_graph": route_task_graph,
            "route_runner_handoff": route_runner_handoff,
            "tactics_board": {
                "trillionnium_osm_objective_contract_version": "trillionnium_osm_objective_v1",
                "trillionnium_npc_relationship_contract_version": "trillionnium_npc_relationship_v1",
                "tactics_combat_resolution_contract_version": "trillionnium_tactics_combat_resolution_v1",
                "tactics_game_session_contract_version": "trillionnium_tactics_game_session_v1",
                "tactics_simulation_tick_contract_version": "trillionnium_tactics_simulation_tick_v1",
                "tactics_reward_settlement_contract_version": "trillionnium_tactics_reward_settlement_v1",
                "map_overlay_identity_contract_version": "trillionnium_map_overlay_identity_v1",
                "osm_objectives": [
                    {"contract_version": "trillionnium_osm_objective_v1", "objective_id": "trillionnium-osm-objective-test"}
                ],
                "map_overlay_identity_index": [
                    {"contract_version": "trillionnium_map_overlay_identity_v1", "game_overlay_id": "trillionnium-world-node:mirror-city-square"}
                ],
                "game_session": {
                    "contract_version": "trillionnium_tactics_game_session_v1",
                    "session_id": "world-tactics-session-test",
                    "persistence_status": "persisted",
                    "objective_progress": 1,
                    "objective_goal": 1,
                    "victory_state": "victory",
                    "reward_status": "settled"
                },
                "simulation_ticks": [
                    {"contract_version": "trillionnium_tactics_simulation_tick_v1", "tick_id": "world-tactics-tick-test"}
                ],
                "combat_log": {
                    "contract_version": "trillionnium_combat_log_v1",
                    "style": "trillionnium_wuxia_log_v1",
                    "beats": [
                        {"kind": "stance", "text": "镜城风从巷口压低，游侠稳住气息。"},
                        {"kind": "result", "text": "战报封存，账本结算后才释放奖励。"}
                    ]
                }
            }
        }));

        assert!(map_reply
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .contains("Trillionnium战报:"));
        let map_card = map_reply.get("cex_card").unwrap();
        assert_eq!(
            map_card
                .get("trillionnium_combat_log_contract")
                .and_then(Value::as_str),
            Some("trillionnium_combat_log_v1")
        );
        assert_eq!(
            map_card
                .get("trillionnium_combat_log_beat_count")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            map_card
                .get("trillionnium_osm_objective_contract")
                .and_then(Value::as_str),
            Some("trillionnium_osm_objective_v1")
        );
        assert_eq!(
            map_card
                .get("trillionnium_osm_objective_count")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            map_card
                .get("trillionnium_npc_relationship_contract")
                .and_then(Value::as_str),
            Some("trillionnium_npc_relationship_v1")
        );
        assert_eq!(
            map_card
                .get("tactics_combat_resolution_contract")
                .and_then(Value::as_str),
            Some("trillionnium_tactics_combat_resolution_v1")
        );
        assert_eq!(
            map_card
                .get("tactics_game_session_contract")
                .and_then(Value::as_str),
            Some("trillionnium_tactics_game_session_v1")
        );
        assert_eq!(
            map_card.get("tactics_session_id").and_then(Value::as_str),
            Some("world-tactics-session-test")
        );
        assert_eq!(
            map_card
                .get("tactics_reward_settlement_contract")
                .and_then(Value::as_str),
            Some("trillionnium_tactics_reward_settlement_v1")
        );
        assert_eq!(
            map_card
                .get("tactics_objective_progress")
                .and_then(Value::as_i64),
            Some(1)
        );
        assert_eq!(
            map_card
                .get("tactics_objective_goal")
                .and_then(Value::as_i64),
            Some(1)
        );
        assert_eq!(
            map_card
                .get("tactics_victory_state")
                .and_then(Value::as_str),
            Some("victory")
        );
        assert_eq!(
            map_card
                .get("tactics_reward_status")
                .and_then(Value::as_str),
            Some("settled")
        );
        assert_eq!(
            map_card
                .get("tactics_simulation_tick_contract")
                .and_then(Value::as_str),
            Some("trillionnium_tactics_simulation_tick_v1")
        );
        assert_eq!(
            map_card
                .get("tactics_simulation_tick_count")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            map_card
                .get("map_overlay_identity_contract")
                .and_then(Value::as_str),
            Some("trillionnium_map_overlay_identity_v1")
        );
        assert_eq!(
            map_card
                .get("map_overlay_identity_count")
                .and_then(Value::as_u64),
            Some(1)
        );

        for reply in [&world_reply, &map_reply] {
            let body = reply.get("body").and_then(Value::as_str).unwrap_or("");
            assert!(body.contains("Runner Handoff:"));
            assert!(body.contains("Next Route: Open next route"));
            let card = reply.get("cex_card").unwrap();
            assert_eq!(
                card.get("route_next_opportunity_action_label")
                    .and_then(Value::as_str),
                Some("Open world action lane")
            );
            assert_eq!(
                card.get("route_next_opportunity_panel_id")
                    .and_then(Value::as_str),
                Some("world-action-console")
            );
            assert_eq!(
                card.get("route_next_opportunity_textarea_id")
                    .and_then(Value::as_str),
                Some("world-action-body")
            );
            assert_eq!(
                card.get("route_next_opportunity_target_node_id")
                    .and_then(Value::as_str),
                Some("client-board")
            );
            assert_eq!(
                card.get("map_renderer_adapter_id").and_then(Value::as_str),
                Some("leaflet_renderer_adapter_v1")
            );
            assert_eq!(
                card.get("map_runtime_handle_name").and_then(Value::as_str),
                Some("mapRuntime")
            );
            assert_eq!(
                card.get("route_runner_next_route_status")
                    .and_then(Value::as_str),
                Some("next_route_ready_after_reward_claim")
            );
            assert_eq!(
                card.get("route_runner_mastery_contract_version")
                    .and_then(Value::as_str),
                Some("trillionnium_route_mastery_v1")
            );
            assert_eq!(
                card.get("route_runner_first_mastery_tier")
                    .and_then(Value::as_str),
                Some("checkpoint_adept")
            );
            assert!(card
                .get("route_runner_next_route_action_body")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .contains("risk controls, next action, and self-review"));
            assert!(card
                .get("route_next_opportunity_body")
                .and_then(Value::as_str)
                .unwrap_or("")
                .contains("增长机会推进任务 task-route-focus-002"));
        }
    }

    #[test]
    fn client_feed_reply_exposes_top_signal_and_route_story() {
        let feed_value = json!({
            "item_count": 4,
            "source_count": 7,
            "active_region_id": "cn-shanghai-core",
            "route_runner_handoff": {
                "contract_version": "trillionnium_route_runner_handoff_v1",
                "runner_count": 2,
                "avatar_task_route_count": 2,
                "reward_claim_action_count": 2,
                "next_route_action_count": 2,
                "reward_claim_ready_count": 1,
                "next_route_ready_count": 1,
                "supports_checkpoint_reward_history": true,
                "supports_route_runner_lifecycle": true,
                "supports_route_runner_reward_claim_actions": true,
                "supports_route_runner_next_route_actions": true,
                "lifecycle_contract_version": "trillionnium_route_runner_lifecycle_v1",
                "supports_route_mastery_progression": true,
                "route_mastery_contract_version": "trillionnium_route_mastery_v1",
                "route_mastery_runner_count": 2,
                "first_route_mastery_xp": 1020,
                "first_route_mastery_tier": "world_pathfinder",
                "first_route_mastery_tier_label": "World Pathfinder / 世界寻路者",
                "first_route_mastery_streak": 3,
                "first_route_mastery_next_goal": "Claim the rating/reward, then chain the next route with the same evidence and self-review anchors.",
                "first_route_mastery_summary": "Route mastery: World Pathfinder / 世界寻路者 · 1020 XP",
                "first_runner_id": "runner-feed-001",
                "first_task_id": "task-feed-001",
                "first_to_node_id": "client-board",
                "first_latest_location_id": "zbj-market-gate",
                "first_progress_label": "100% route progress / 100% 路线进度",
                "first_telemetry_summary": "100% complete · 0m remaining · ETA 0 min",
                "first_reward_claim_label": "Claim route reward / 领取路线奖励",
                "first_reward_claim_status": "claimable_after_evidence",
                "first_reward_claim_action_body": "Claim with deliverable, evidence package, risk controls, next action, and self-review.",
                "first_next_route_label": "Open next route / 打开下一路线",
                "first_next_route_status": "next_route_ready_after_reward_claim",
                "first_next_route_action_body": "Open next route with deliverable, evidence package, risk controls, next action, and self-review anchors.",
                "first_next_route_sequence_summary": "Reward claim → next route handoff",
                "summary": "Route runner handoff: 2 runners · 2 reward claims · 2 next-route actions · next Claim route reward / Open next route",
                "handoff_prompt": "Claim rating/reward, then open the next route with deliverable → evidence → risk controls → next action → self-review anchors."
            },
            "items": [
                {
                    "feed_kind": "route_task",
                    "feed_group": "route_task",
                    "source": "route_task_graph",
                    "title": "Route task for premium repeat order",
                    "summary": "Push the current completion into a repeat-order offer.",
                    "detail": "completion/settled · zbj-market-gate",
                    "action_label": "Open listing lane",
                    "action_panel_id": "world-listings-panel",
                    "action_input_id": "world-listing-company-id",
                    "action_input_value": "latest",
                    "action_textarea_id": "world-listing-body",
                    "action_location_id": "zbj-market-gate",
                    "action_target_node_id": "client-board",
                    "action_task_id": "task-feed-001",
                    "action_contract_id": "world-contract-feed-001",
                    "action_listing_id": "world-listing-feed-001",
                    "action_work_order_id": "world-work-feed-001"
                },
                {
                    "feed_kind": "live_event",
                    "feed_group": "live_event",
                    "source": "live_event_stream",
                    "title": "world_contract",
                    "summary": "A new client brief just landed.",
                    "detail": "Mirror City Square · queued · impact +6",
                    "action_label": "继续推进",
                    "action_panel_id": "world-action-console"
                },
                {
                    "feed_kind": "commerce_purchase",
                    "feed_group": "commerce",
                    "source": "commerce_snapshot",
                    "title": "Purchase world-purchase-1",
                    "summary": "listing premium-design-pack",
                    "detail": "52 credits · reserved",
                    "action_label": "打开成交",
                    "action_panel_id": "world-commerce-panel"
                },
                {
                    "feed_kind": "social_agent",
                    "feed_group": "social",
                    "source": "social_snapshot",
                    "title": "Ledger Clerk",
                    "summary": "agent · available",
                    "detail": "available near zbj-market-gate",
                    "action_label": "去世界",
                    "action_panel_id": "world-action-console"
                }
            ],
            "route_story": {
                "preview_item_count": 4,
                "task_linked_count": 2,
                "task_graph_count": 1,
                "next_task_id": "task-feed-story-001",
                "next_action_label": "Draft completion follow-up",
                "next_panel_id": "world-action-console",
                "next_command_hint": "/world action 围绕 feed 跟进完成后的下一步。",
                "next_location_id": "zbj-market-gate",
                "next_node_id": "zbj-market-gate",
                "next_opportunity_node_id": "client-board",
                "next_stage_summary": "1 events → 1 contracts → 1 completions · latest completion/settled",
                "next_opportunity_kind": "repeat_order_upsell_referral",
                "next_outcome_summary": "Completion world-contract-completion-feed-1 · settled · score 80.9 · reward 4.05",
                "next_feedback_focus": "Capture the delivery proof and customer feedback before opening the next lane.",
                "next_opportunity_hint": "Turn the current completion into a repeat-order or upsell offer.",
                "next_opportunity_playbook": "Archive proof, extract the buyer quote, and draft the next premium offer.",
                "next_opportunity_command": "/sell latest Feed premium repeat-order package"
            }
        });
        let reply = super::build_trillionnium_client_feed_matrix_reply(&feed_value, None);

        let card = reply.get("cex_card").unwrap();
        assert_eq!(
            card.get("type").and_then(Value::as_str),
            Some("trillionnium_client_feed")
        );
        assert_eq!(card.get("feed_filter"), Some(&Value::Null));
        assert_eq!(
            card.get("feed_filter_label").and_then(Value::as_str),
            Some("全部")
        );
        assert_eq!(card.get("item_count").and_then(Value::as_u64), Some(4));
        assert_eq!(
            card.get("visible_item_count").and_then(Value::as_u64),
            Some(4)
        );
        assert_eq!(card.get("source_count").and_then(Value::as_u64), Some(7));
        assert_eq!(
            card.get("route_runner_next_route_status")
                .and_then(Value::as_str),
            Some("next_route_ready_after_reward_claim")
        );
        assert_eq!(
            card.get("route_runner_next_route_action_count")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert!(card
            .get("route_runner_handoff")
            .and_then(|handoff| handoff.get("summary"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("next-route actions"));
        assert!(reply
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("Runner Handoff"));
        assert_eq!(
            card.get("route_task_feed_count").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            card.get("live_event_feed_count").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            card.get("commerce_feed_count").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            card.get("social_feed_count").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            card.get("top_title").and_then(Value::as_str),
            Some("Route task for premium repeat order")
        );
        assert_eq!(
            card.get("top_action_label").and_then(Value::as_str),
            Some("Open listing lane")
        );
        assert_eq!(
            card.get("top_action_panel_id").and_then(Value::as_str),
            Some("world-listings-panel")
        );
        assert_eq!(
            card.get("top_action_input_id").and_then(Value::as_str),
            Some("world-listing-company-id")
        );
        assert_eq!(
            card.get("top_action_input_value").and_then(Value::as_str),
            Some("latest")
        );
        assert_eq!(
            card.get("top_action_textarea_id").and_then(Value::as_str),
            Some("world-listing-body")
        );
        assert_eq!(
            card.get("top_action_target_node_id")
                .and_then(Value::as_str),
            Some("client-board")
        );
        assert_eq!(
            card.get("route_next_task_id").and_then(Value::as_str),
            Some("task-feed-story-001")
        );
        assert_eq!(
            card.get("route_next_opportunity_action_label")
                .and_then(Value::as_str),
            Some("Open world action lane")
        );
        assert_eq!(
            card.get("route_next_opportunity_panel_id")
                .and_then(Value::as_str),
            Some("world-action-console")
        );
        assert_eq!(
            card.get("route_story")
                .and_then(|story| story.get("next_task_id"))
                .and_then(Value::as_str),
            Some("task-feed-story-001")
        );

        let filtered =
            super::build_trillionnium_client_feed_matrix_reply(&feed_value, Some("commerce"));
        let filtered_card = filtered.get("cex_card").unwrap();
        assert_eq!(
            filtered_card.get("feed_filter").and_then(Value::as_str),
            Some("commerce")
        );
        assert_eq!(
            filtered_card
                .get("feed_filter_label")
                .and_then(Value::as_str),
            Some("成交")
        );
        assert_eq!(
            filtered_card
                .get("visible_item_count")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            filtered_card.get("top_feed_group").and_then(Value::as_str),
            Some("commerce")
        );
        assert_eq!(
            filtered_card.get("top_title").and_then(Value::as_str),
            Some("Purchase world-purchase-1")
        );
        assert_eq!(
            filtered_card
                .get("top_action_label")
                .and_then(Value::as_str),
            Some("打开成交")
        );
        assert_eq!(
            filtered_card
                .get("top_action_panel_id")
                .and_then(Value::as_str),
            Some("world-commerce-panel")
        );
    }

    #[test]
    fn route_opportunity_target_mapping_covers_world_command_families() {
        for (
            command,
            expected_action_label,
            expected_panel_id,
            expected_input_id,
            expected_input_value,
            expected_textarea_id,
            expected_body,
        ) in [
            (
                "/upgrade latest 资产化升级包",
                "Open asset upgrade lane",
                "world-assets-panel",
                "world-asset-id",
                "latest",
                "world-asset-body",
                "资产化升级包",
            ),
            (
                "/company latest 公司经营方案",
                "Open company lane",
                "world-companies-panel",
                "world-company-asset-id",
                "latest",
                "world-company-body",
                "公司经营方案",
            ),
            (
                "/sell latest 市场上架方案",
                "Open listing lane",
                "world-listings-panel",
                "world-listing-company-id",
                "latest",
                "world-listing-body",
                "市场上架方案",
            ),
            (
                "/buy latest 采购需求摘要",
                "Open purchase lane",
                "world-commerce-panel",
                "world-buy-listing-id",
                "latest",
                "world-buy-body",
                "采购需求摘要",
            ),
            (
                "/work deliver latest 交付包",
                "Open delivery lane",
                "world-commerce-panel",
                "world-work-deliver-id",
                "latest",
                "world-work-deliver-body",
                "交付包",
            ),
            (
                "/work accept latest 验收确认",
                "Open acceptance lane",
                "world-commerce-panel",
                "world-work-accept-id",
                "latest",
                "world-work-accept-body",
                "验收确认",
            ),
            (
                "/work reject latest 驳回说明",
                "Open rejection lane",
                "world-commerce-panel",
                "world-work-reject-id",
                "latest",
                "world-work-reject-body",
                "驳回说明",
            ),
            (
                "/work reopen latest 重开返工要求",
                "Open reopen lane",
                "world-commerce-panel",
                "world-work-reopen-id",
                "latest",
                "world-work-reopen-body",
                "重开返工要求",
            ),
            (
                "/work cancel latest 取消原因",
                "Open cancellation lane",
                "world-commerce-panel",
                "world-work-cancel-id",
                "latest",
                "world-work-cancel-body",
                "取消原因",
            ),
            (
                "/world action 记录新的增长机会",
                "Open world action lane",
                "world-action-console",
                "",
                "",
                "world-action-body",
                "记录新的增长机会",
            ),
            (
                "/contract 整理成交委托",
                "Open contract capture lane",
                "world-action-console",
                "",
                "",
                "world-action-body",
                "整理成交委托",
            ),
            (
                "/complete world-contract-001 交付总结",
                "Open contract completion lane",
                "world-contracts-panel",
                "world-contract-completion-id",
                "world-contract-001",
                "world-contract-completion-body",
                "交付总结",
            ),
        ] {
            let target = super::route_opportunity_target_from_command(command, "client-board");
            assert_eq!(target.action_label, expected_action_label);
            assert_eq!(target.panel_id, expected_panel_id);
            assert_eq!(target.input_id, expected_input_id);
            assert_eq!(target.input_value, expected_input_value);
            assert_eq!(target.textarea_id, expected_textarea_id);
            assert!(
                target.body.starts_with(expected_body),
                "target body should preserve original body prefix: {}",
                target.body
            );
            assert_matrix_route_hidden_anchor_ready(command, &target.body);
            assert_eq!(target.node_id, "client-board");
        }
    }

    #[test]
    fn world_adjacent_route_cards_preserve_target_fields() {
        let route_preview = json!({"item_count": 2, "task_linked_count": 1});
        let route_task_graph = json!({"task_count": 1, "tasks": [{
            "task_id": "task-route-world-017",
            "suggested_action_label": "Prepare delivery lane",
            "suggested_panel_id": "world-commerce-panel",
            "suggested_matrix_command": "/world action 记录交付结果、证据和下一步。",
            "latest_location_id": "zbj-market-gate",
            "suggested_node_id": "delivery-dock",
            "next_opportunity_node_id": "client-board",
            "route_stage_summary": "2 events → 1 work orders → delivered",
            "outcome_summary": "Work order world-work-world-017 · delivered",
            "feedback_focus": "Capture proof, pick the best follow-up lane, and keep the next action concrete.",
            "next_opportunity_kind": "repeat_order_upsell_referral",
            "next_opportunity_hint": "Use the completed route to open the next upsell lane.",
            "next_opportunity_playbook": "Archive proof, pull out offer hooks, and draft the next move.",
            "next_opportunity_command": "/world action 记录复购机会、proof 包和下一步。"
        }]});

        let assets = super::build_trillionnium_world_assets_matrix_reply(&json!({
            "assets": [{"asset_id": "world-asset-001"}],
            "upgrades": [{"upgrade_id": "world-asset-upgrade-001"}],
            "route_preview": route_preview.clone(),
            "route_task_graph": route_task_graph.clone()
        }));
        let companies = super::build_trillionnium_world_companies_matrix_reply(&json!({
            "companies": [{"company_id": "world-company-001"}],
            "route_preview": route_preview.clone(),
            "route_task_graph": route_task_graph.clone()
        }));
        let shops = super::build_trillionnium_world_shops_matrix_reply(&json!({
            "shops": [{"shop_id": "world-shop-001"}],
            "listings": [{"listing_id": "world-listing-001"}],
            "route_preview": route_preview.clone(),
            "route_task_graph": route_task_graph.clone()
        }));
        let listing = super::build_trillionnium_world_listing_matrix_reply(&json!({
            "listing": {
                "listing_id": "world-listing-001",
                "price_credits": 12,
                "quality_score": 88,
                "status": "listed"
            },
            "route_preview": route_preview.clone(),
            "route_task_graph": route_task_graph.clone()
        }));
        let purchase = super::build_trillionnium_world_purchase_matrix_reply(&json!({
            "purchase": {
                "purchase_id": "world-purchase-001",
                "price_credits": 18,
                "status": "reserved",
                "ledger_status": "settled",
                "buyer_ledger_status": "reserved"
            },
            "work_order": {"work_order_id": "world-work-001"},
            "seller_standing": {"rank_label": "Guild Ally"},
            "route_preview": route_preview.clone(),
            "route_task_graph": route_task_graph.clone()
        }));
        let work = super::build_trillionnium_world_commerce_matrix_reply(&json!({
            "purchases": [{"purchase_id": "world-purchase-001"}],
            "work_orders": [{"work_order_id": "world-work-001"}],
            "work_deliveries": [{"delivery_id": "world-delivery-001"}],
            "work_acceptances": [{"acceptance_id": "world-acceptance-001"}],
            "work_rejections": [{"rejection_id": "world-rejection-001"}],
            "work_reopens": [{"reopen_id": "world-reopen-001"}],
            "work_cancellations": [{"cancellation_id": "world-cancel-001"}],
            "route_preview": route_preview,
            "route_task_graph": route_task_graph
        }));

        for (
            reply,
            expected_task_id,
            expected_prefix,
            expected_action_label,
            expected_panel_id,
            expected_input_id,
            expected_input_value,
            expected_textarea_id,
        ) in [
            (
                &assets,
                "task-route-world-017",
                "/upgrade latest",
                "Open asset upgrade lane",
                "world-assets-panel",
                "world-asset-id",
                "latest",
                "world-asset-body",
            ),
            (
                &companies,
                "task-route-world-017",
                "/company latest",
                "Open company lane",
                "world-companies-panel",
                "world-company-asset-id",
                "latest",
                "world-company-body",
            ),
            (
                &shops,
                "task-route-world-017",
                "/sell latest",
                "Open listing lane",
                "world-listings-panel",
                "world-listing-company-id",
                "latest",
                "world-listing-body",
            ),
            (
                &listing,
                "task-route-world-017",
                "/sell latest",
                "Open listing lane",
                "world-listings-panel",
                "world-listing-company-id",
                "latest",
                "world-listing-body",
            ),
            (
                &purchase,
                "task-route-world-017",
                "/world action",
                "Open world action lane",
                "world-action-console",
                "",
                "",
                "world-action-body",
            ),
            (
                &work,
                "task-route-world-017",
                "/world action",
                "Open world action lane",
                "world-action-console",
                "",
                "",
                "world-action-body",
            ),
        ] {
            let card = reply.get("cex_card").unwrap();
            assert!(card
                .get("route_next_opportunity_command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .starts_with(expected_prefix));
            assert_eq!(
                card.get("route_next_opportunity_action_label")
                    .and_then(Value::as_str),
                Some(expected_action_label)
            );
            assert_eq!(
                card.get("route_next_opportunity_panel_id")
                    .and_then(Value::as_str),
                Some(expected_panel_id)
            );
            assert_eq!(
                card.get("route_next_opportunity_input_id")
                    .and_then(Value::as_str),
                Some(expected_input_id)
            );
            assert_eq!(
                card.get("route_next_opportunity_input_value")
                    .and_then(Value::as_str),
                Some(expected_input_value)
            );
            assert_eq!(
                card.get("route_next_opportunity_textarea_id")
                    .and_then(Value::as_str),
                Some(expected_textarea_id)
            );
            assert_eq!(
                card.get("route_next_opportunity_target_node_id")
                    .and_then(Value::as_str),
                Some("client-board")
            );
            assert_eq!(
                card.get("route_story")
                    .and_then(|story| story.get("next_task_id"))
                    .and_then(Value::as_str),
                Some(expected_task_id)
            );
            assert_eq!(
                card.get("route_story")
                    .and_then(|story| story.get("next_opportunity_target"))
                    .and_then(|target| target.get("node_id"))
                    .and_then(Value::as_str),
                Some("client-board")
            );
            assert!(card
                .get("route_next_opportunity_body")
                .and_then(Value::as_str)
                .unwrap_or("")
                .contains("world-work-world-017"));
        }
    }

    #[test]
    fn world_result_route_cards_preserve_target_fields() {
        let route_preview = json!({"item_count": 3, "task_linked_count": 1});
        let asset_upgrade = super::build_trillionnium_world_asset_upgrade_matrix_reply(&json!({
            "asset": {
                "asset_id": "world-asset-777",
                "upgrade_level": 4,
                "value_score": 91
            },
            "upgrade": {
                "value_delta": 12,
                "judge_status": "rubric_hidden_pipeline_v2"
            },
            "route_preview": route_preview.clone(),
            "route_task_graph": {"task_count": 1, "tasks": [{
                "task_id": "task-upgrade-001",
                "suggested_action_label": "Route asset launch",
                "suggested_panel_id": "world-assets-panel",
                "suggested_matrix_command": "/upgrade latest 把升级包整理成标准套餐。",
                "latest_location_id": "asset-yard",
                "suggested_node_id": "asset-yard",
                "next_opportunity_node_id": "asset-yard",
                "route_stage_summary": "1 events → 1 assets → upgraded",
                "outcome_summary": "Asset world-asset-777 · upgraded",
                "feedback_focus": "Capture what changed in the upgrade and what proof now exists.",
                "next_opportunity_kind": "asset_upgrade_scaling",
                "next_opportunity_hint": "Turn the stronger asset into a reusable package.",
                "next_opportunity_playbook": "Package the asset, attach evidence, and prepare the next publishable version.",
                "next_opportunity_command": "/upgrade latest 把升级包整理成标准套餐。"
            }]}
        }));
        let company = super::build_trillionnium_world_company_matrix_reply(&json!({
            "company": {
                "company_id": "world-company-777",
                "level": 3,
                "revenue_score": 28,
                "reputation_score": 16
            },
            "route_preview": route_preview.clone(),
            "route_task_graph": {"task_count": 1, "tasks": [{
                "task_id": "task-company-001",
                "suggested_action_label": "Route company expansion",
                "suggested_panel_id": "world-companies-panel",
                "suggested_matrix_command": "/company latest 把这家公司的运营闭环写成 SOP。",
                "latest_location_id": "starter-studio",
                "suggested_node_id": "starter-studio",
                "next_opportunity_node_id": "starter-studio",
                "route_stage_summary": "1 assets → 1 companies → launched",
                "outcome_summary": "Company world-company-777 · launched",
                "feedback_focus": "Capture why the company launch works and what operating proof exists.",
                "next_opportunity_kind": "company_operating_loop",
                "next_opportunity_hint": "Lock the operating loop before scaling sales.",
                "next_opportunity_playbook": "Document the offer, owner cadence, and proof loop for the next team member.",
                "next_opportunity_command": "/company latest 把这家公司的运营闭环写成 SOP。"
            }]}
        }));
        let delivery = super::build_trillionnium_world_work_delivery_matrix_reply(&json!({
            "work_order": {"work_order_id": "world-work-777"},
            "delivery": {
                "delivery_id": "world-delivery-777",
                "score": 87.5,
                "status": "delivered",
                "judge_status": "rubric_hidden_pipeline_v2"
            },
            "route_preview": route_preview.clone(),
            "route_task_graph": {"task_count": 1, "tasks": [{
                "task_id": "task-delivery-001",
                "suggested_action_label": "Route buyer review",
                "suggested_panel_id": "world-commerce-panel",
                "suggested_matrix_command": "/work accept latest 买家验收总结与后续合作建议。",
                "latest_location_id": "delivery-dock",
                "suggested_node_id": "delivery-dock",
                "next_opportunity_node_id": "delivery-dock",
                "route_stage_summary": "1 purchases → 1 work orders → delivered",
                "outcome_summary": "Work world-work-777 · delivered",
                "feedback_focus": "Push the buyer review with proof and close any gaps quickly.",
                "next_opportunity_kind": "acceptance_closeout",
                "next_opportunity_hint": "Secure acceptance while the proof is still hot.",
                "next_opportunity_playbook": "Highlight deliverables, evidence, and the next collaboration angle during review.",
                "next_opportunity_command": "/work accept latest 买家验收总结与后续合作建议。"
            }]}
        }));
        let acceptance = super::build_trillionnium_world_work_acceptance_matrix_reply(&json!({
            "work_order": {"work_order_id": "world-work-888"},
            "acceptance": {
                "acceptance_id": "world-acceptance-888",
                "status": "accepted",
                "reputation_delta": 7
            },
            "purchase": {"buyer_consume_status": "consumed"},
            "route_preview": route_preview,
            "route_task_graph": {"task_count": 1, "tasks": [{
                "task_id": "task-acceptance-001",
                "suggested_action_label": "Route upsell follow-up",
                "suggested_panel_id": "world-listings-panel",
                "suggested_matrix_command": "/sell latest 验收后升级方案与复购路径。",
                "latest_location_id": "client-board",
                "suggested_node_id": "client-board",
                "next_opportunity_node_id": "client-board",
                "route_stage_summary": "1 deliveries → 1 acceptances → settled",
                "outcome_summary": "Work world-work-888 · accepted",
                "feedback_focus": "Capture why the delivery worked and what the buyer loved.",
                "next_opportunity_kind": "acceptance_upsell",
                "next_opportunity_hint": "Use acceptance proof to tee up the premium next step.",
                "next_opportunity_playbook": "Turn the acceptance into a testimonial, upgrade offer, and referral ask.",
                "next_opportunity_command": "/sell latest 验收后升级方案与复购路径。"
            }]}
        }));

        for (
            reply,
            expected_task_id,
            expected_prefix,
            expected_action_label,
            expected_panel_id,
            expected_input_id,
            expected_input_value,
            expected_textarea_id,
            expected_target_node_id,
        ) in [
            (
                &asset_upgrade,
                "task-upgrade-001",
                "/upgrade latest",
                "Open asset upgrade lane",
                "world-assets-panel",
                "world-asset-id",
                "latest",
                "world-asset-body",
                "asset-yard",
            ),
            (
                &company,
                "task-company-001",
                "/company latest",
                "Open company lane",
                "world-companies-panel",
                "world-company-asset-id",
                "latest",
                "world-company-body",
                "starter-studio",
            ),
            (
                &delivery,
                "task-delivery-001",
                "/work accept latest",
                "Open acceptance lane",
                "world-commerce-panel",
                "world-work-accept-id",
                "latest",
                "world-work-accept-body",
                "delivery-dock",
            ),
            (
                &acceptance,
                "task-acceptance-001",
                "/sell latest",
                "Open listing lane",
                "world-listings-panel",
                "world-listing-company-id",
                "latest",
                "world-listing-body",
                "client-board",
            ),
        ] {
            let card = reply.get("cex_card").unwrap();
            assert!(card
                .get("route_next_opportunity_command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .starts_with(expected_prefix));
            assert_eq!(
                card.get("route_next_opportunity_action_label")
                    .and_then(Value::as_str),
                Some(expected_action_label)
            );
            assert_eq!(
                card.get("route_next_opportunity_panel_id")
                    .and_then(Value::as_str),
                Some(expected_panel_id)
            );
            assert_eq!(
                card.get("route_next_opportunity_input_id")
                    .and_then(Value::as_str),
                Some(expected_input_id)
            );
            assert_eq!(
                card.get("route_next_opportunity_input_value")
                    .and_then(Value::as_str),
                Some(expected_input_value)
            );
            assert_eq!(
                card.get("route_next_opportunity_textarea_id")
                    .and_then(Value::as_str),
                Some(expected_textarea_id)
            );
            assert_eq!(
                card.get("route_next_opportunity_target_node_id")
                    .and_then(Value::as_str),
                Some(expected_target_node_id)
            );
            assert_eq!(
                card.get("route_story")
                    .and_then(|story| story.get("next_task_id"))
                    .and_then(Value::as_str),
                Some(expected_task_id)
            );
            assert_eq!(
                card.get("route_story")
                    .and_then(|story| story.get("next_opportunity_target"))
                    .and_then(|target| target.get("node_id"))
                    .and_then(Value::as_str),
                Some(expected_target_node_id)
            );
            assert!(!card
                .get("route_next_opportunity_body")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty());
        }
    }

    #[test]
    fn work_recovery_reply_cards_preserve_route_story_fields() {
        let rejection = super::build_trillionnium_world_work_rejection_matrix_reply(&json!({
            "work_order": {"work_order_id": "world-work-order-001", "status": "rejected_refunded"},
            "purchase": {"status": "rejected_refunded"},
            "rejection": {"rejection_id": "world-rejection-001", "status": "rejected_refunded", "refund_status": "refunded"},
            "route_preview": {"item_count": 2, "task_linked_count": 1},
            "route_task_graph": {"task_count": 1, "tasks": [{
                "task_id": "task-route-010",
                "suggested_action_label": "Patch revision evidence",
                "suggested_panel_id": "world-action-console",
                "suggested_matrix_command": "/world action 记录修订范围、证据缺口和下一步。",
                "latest_location_id": "zbj-market-gate",
                "route_stage_summary": "2 events → 1 work orders → rejected",
                "outcome_summary": "Work order world-work-order-001 · rejected",
                "feedback_focus": "Capture objections, patch evidence, and reset acceptance criteria.",
                "next_opportunity_kind": "repeat_order_upsell_referral",
                "next_opportunity_hint": "Use the completion lane to tee up the next offer at zbj-market-gate.",
                "next_opportunity_playbook": "Archive proof, ask for feedback, and tee up the next upsell path.",
                "next_opportunity_command": "/sell latest 复购方案：延续上一次完成结果，补充 deliverable、proof、price 和 next action。"
            }]}
        }));
        let rejection_settlement = super::build_trillionnium_world_work_rejection_matrix_reply(
            &json!({
                "work_order": {"work_order_id": "world-work-order-004", "status": "rejected_chargeback_failed"},
                "purchase": {"status": "rejected_chargeback_failed"},
                "rejection": {"rejection_id": "world-rejection-004", "status": "rejected_chargeback_failed", "refund_status": "refunded"},
                "route_preview": {"item_count": 2, "task_linked_count": 1},
                "route_task_graph": {"task_count": 1, "tasks": [{
                    "task_id": "task-route-013",
                    "suggested_action_label": "Patch revision evidence",
                    "suggested_panel_id": "world-action-console",
                    "suggested_matrix_command": "/world action 记录修订范围、证据缺口和下一步。",
                    "latest_location_id": "zbj-market-gate",
                    "route_stage_summary": "2 events → 1 work orders → rejected chargeback failed",
                    "outcome_summary": "Work order world-work-order-004 · rejected_chargeback_failed",
                    "feedback_focus": "Recover seller chargeback before reopening.",
                    "next_opportunity_kind": "repeat_order_upsell_referral",
                    "next_opportunity_hint": "Do not upsell until settlement is clear.",
                    "next_opportunity_playbook": "Clear settlement before growth.",
                    "next_opportunity_command": "/sell latest stale growth command should be overridden."
                }]}
            }),
        );
        let reopen = super::build_trillionnium_world_work_reopen_matrix_reply(&json!({
            "work_order": {"work_order_id": "world-work-order-002", "status": "open"},
            "purchase": {"status": "reopened_reserved"},
            "reopen": {"reopen_id": "world-reopen-001", "status": "reopened", "reserve_status": "reserved"},
            "route_preview": {"item_count": 2, "task_linked_count": 1},
            "route_task_graph": {"task_count": 1, "tasks": [{
                "task_id": "task-route-011",
                "suggested_action_label": "Lock redelivery loop",
                "suggested_panel_id": "world-action-console",
                "suggested_matrix_command": "/world action 记录 reopen 要求、证据缺口和下一步。",
                "latest_location_id": "zbj-market-gate",
                "route_stage_summary": "2 events → 1 work orders → reopened",
                "outcome_summary": "Work order world-work-order-002 · reopened",
                "feedback_focus": "Close the evidence gap and relock acceptance criteria.",
                "next_opportunity_kind": "repeat_order_upsell_referral",
                "next_opportunity_hint": "Use the completion lane to tee up the next offer at zbj-market-gate.",
                "next_opportunity_playbook": "Archive proof, ask for feedback, and tee up the next upsell path.",
                "next_opportunity_command": "/sell latest 复购方案：延续上一次完成结果，补充 deliverable、proof、price 和 next action。"
            }]}
        }));
        let cancellation = super::build_trillionnium_world_work_cancellation_matrix_reply(&json!({
            "work_order": {"work_order_id": "world-work-order-003", "status": "cancelled_refunded"},
            "purchase": {"status": "cancelled_refunded"},
            "cancellation": {"cancellation_id": "world-cancel-001", "status": "cancelled_refunded", "refund_status": "refunded"},
            "route_preview": {"item_count": 2, "task_linked_count": 1},
            "route_task_graph": {"task_count": 1, "tasks": [{
                "task_id": "task-route-012",
                "suggested_action_label": "Launch smaller pilot",
                "suggested_panel_id": "world-action-console",
                "suggested_matrix_command": "/world action 记录重资格筛选、starter scope 和下一步。",
                "latest_location_id": "zbj-market-gate",
                "route_stage_summary": "2 events → 1 work orders → cancelled",
                "outcome_summary": "Work order world-work-order-003 · cancelled",
                "feedback_focus": "Reduce scope, lower risk, and clarify starter proof.",
                "next_opportunity_kind": "repeat_order_upsell_referral",
                "next_opportunity_hint": "Use the completion lane to tee up the next offer at zbj-market-gate.",
                "next_opportunity_playbook": "Archive proof, ask for feedback, and tee up the next upsell path.",
                "next_opportunity_command": "/world action 记录复购机会、proof 包和下一步。"
            }]}
        }));
        let cancellation_settlement =
            super::build_trillionnium_world_work_cancellation_matrix_reply(&json!({
                "work_order": {"work_order_id": "world-work-order-005", "status": "cancelled_chargeback_failed"},
                "purchase": {"status": "cancelled_chargeback_failed"},
                "cancellation": {"cancellation_id": "world-cancel-005", "status": "cancelled_chargeback_failed", "refund_status": "refunded"},
                "route_preview": {"item_count": 2, "task_linked_count": 1},
                "route_task_graph": {"task_count": 1, "tasks": [{
                    "task_id": "task-route-014",
                    "suggested_action_label": "Launch smaller pilot",
                    "suggested_panel_id": "world-action-console",
                    "suggested_matrix_command": "/world action 记录重资格筛选、starter scope 和下一步。",
                    "latest_location_id": "zbj-market-gate",
                    "route_stage_summary": "2 events → 1 work orders → cancellation chargeback failed",
                    "outcome_summary": "Work order world-work-order-005 · cancelled_chargeback_failed",
                    "feedback_focus": "Recover cancellation settlement before relisting.",
                    "next_opportunity_kind": "repeat_order_upsell_referral",
                    "next_opportunity_hint": "Do not relist until settlement is clear.",
                    "next_opportunity_playbook": "Clear settlement before growth.",
                    "next_opportunity_command": "/sell latest stale relist command should be overridden."
                }]}
            }));

        for (
            reply,
            expected_task_id,
            expected_kind,
            expected_prefix,
            expected_node_id,
            expected_action_label,
            expected_panel_id,
            expected_input_id,
            expected_textarea_id,
        ) in [
            (
                &rejection,
                "task-route-010",
                "revision_reopen",
                "/work reopen latest",
                "delivery-dock",
                "Open reopen lane",
                "world-commerce-panel",
                "world-work-reopen-id",
                "world-work-reopen-body",
            ),
            (
                &rejection_settlement,
                "task-route-013",
                "rejection_chargeback_recovery",
                "/work reject latest",
                "delivery-dock",
                "Open rejection lane",
                "world-commerce-panel",
                "world-work-reject-id",
                "world-work-reject-body",
            ),
            (
                &reopen,
                "task-route-011",
                "reopen_recovery",
                "/work deliver latest",
                "delivery-dock",
                "Open delivery lane",
                "world-commerce-panel",
                "world-work-deliver-id",
                "world-work-deliver-body",
            ),
            (
                &cancellation,
                "task-route-012",
                "smaller_scope_requalification",
                "/sell latest",
                "client-board",
                "Open listing lane",
                "world-listings-panel",
                "world-listing-company-id",
                "world-listing-body",
            ),
            (
                &cancellation_settlement,
                "task-route-014",
                "cancellation_settlement_recovery",
                "/work cancel latest",
                "delivery-dock",
                "Open cancellation lane",
                "world-commerce-panel",
                "world-work-cancel-id",
                "world-work-cancel-body",
            ),
        ] {
            let card = reply.get("cex_card").unwrap();
            assert_eq!(
                card.get("route_next_opportunity_kind")
                    .and_then(Value::as_str),
                Some(expected_kind)
            );
            assert!(card
                .get("route_next_opportunity_command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .starts_with(expected_prefix));
            assert_eq!(
                card.get("route_next_node_id").and_then(Value::as_str),
                Some(expected_node_id)
            );
            assert_eq!(
                card.get("route_next_opportunity_node_id")
                    .and_then(Value::as_str),
                Some(expected_node_id)
            );
            assert_eq!(
                card.get("route_next_opportunity_action_label")
                    .and_then(Value::as_str),
                Some(expected_action_label)
            );
            assert_eq!(
                card.get("route_next_opportunity_panel_id")
                    .and_then(Value::as_str),
                Some(expected_panel_id)
            );
            assert_eq!(
                card.get("route_next_opportunity_input_id")
                    .and_then(Value::as_str),
                Some(expected_input_id)
            );
            assert_eq!(
                card.get("route_next_opportunity_input_value")
                    .and_then(Value::as_str),
                Some("latest")
            );
            assert_eq!(
                card.get("route_next_opportunity_textarea_id")
                    .and_then(Value::as_str),
                Some(expected_textarea_id)
            );
            assert_eq!(
                card.get("route_next_opportunity_target_node_id")
                    .and_then(Value::as_str),
                Some(expected_node_id)
            );
            assert_eq!(
                card.get("route_story")
                    .and_then(|story| story.get("next_task_id"))
                    .and_then(Value::as_str),
                Some(expected_task_id)
            );
            assert_eq!(
                card.get("route_story")
                    .and_then(|story| story.get("next_opportunity_target"))
                    .and_then(|target| target.get("node_id"))
                    .and_then(Value::as_str),
                Some(expected_node_id)
            );
            assert!(
                card.get("route_task_graph_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    >= 1
            );
        }
    }

    fn test_config() -> MatrixAdapterConfig {
        MatrixAdapterConfig {
            runtime_profile: RuntimeProfile::LocalDev,
            bind_addr: "127.0.0.1:8091".to_string(),
            consumer_entry_base_url: "http://127.0.0.1:8090".to_string(),
            consumer_entry_api_key: None,
            consumer_entry_ingress_token: None,
            consumer_entry_session_auth_explicit_secret: None,
            consumer_entry_session_auth_explicit_key_id: None,
            consumer_entry_session_auth_secret: None,
            consumer_entry_session_auth_key_id: None,
            consumer_entry_session_auth_issuer_registry_path: None,
            consumer_entry_session_auth_issuer_registry: HashMap::new(),
            consumer_entry_session_auth_issuer_registry_load_error: None,
            consumer_entry_session_auth_issuer_registry_metadata:
                SessionAuthIssuerRegistryMetadata::default(),
            consumer_entry_session_auth_issuer_registry_approved_revisions_path: None,
            consumer_entry_session_auth_issuer_registry_require_approved_revision: false,
            consumer_entry_session_auth_selection: SessionAuthIssuerRegistrySelection::default(),
            consumer_entry_session_auth_issuer: "matrix-entry-adapter".to_string(),
            consumer_entry_session_auth_audience: "consumer-entry-api".to_string(),
            consumer_entry_session_auth_ttl_secs: 300,
            ingress_token: None,
            bot_user_id: "@cex-bot:local.dev".to_string(),
            max_text_chars: DEFAULT_MAX_TEXT_CHARS,
            rate_limit_window_secs: 60,
            rate_limit_max_requests: 20,
            rate_limit_store_path: None,
            recent_event_window_secs: 600,
            recent_event_cache_size: 2048,
            recent_event_store_path: None,
        }
    }

    fn test_state(config: MatrixAdapterConfig) -> AppState {
        let consumer_entry_session_auth_issuer_registry_state =
            SessionAuthIssuerRegistryRuntimeState {
                metadata: config
                    .consumer_entry_session_auth_issuer_registry_metadata
                    .clone(),
                registry: config.consumer_entry_session_auth_issuer_registry.clone(),
            };
        AppState {
            inner: Arc::new(AppStateInner {
                http: Client::new(),
                config,
                consumer_entry_session_auth_issuer_registry_state: StdRwLock::new(
                    consumer_entry_session_auth_issuer_registry_state,
                ),
                rate_limits: Mutex::new(RateLimitCache::default()),
                recent_event_cache: Mutex::new(RecentEventCache::default()),
                metrics: MatrixEntryMetrics::default(),
            }),
        }
    }

    async fn decode_json_response(response: axum::response::Response) -> (StatusCode, Value) {
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        (status, value)
    }

    #[test]
    fn local_dev_profile_allows_default_matrix_config() {
        let config = test_config();
        assert!(config.validate_runtime_profile().is_ok());
    }

    #[test]
    fn beta_profile_requires_ingress_and_recent_event_store() {
        let mut config = test_config();
        config.runtime_profile = RuntimeProfile::Beta;

        let errors = config.validate_runtime_profile().unwrap_err();
        assert!(errors
            .iter()
            .any(|item| item.contains("MATRIX_ENTRY_INGRESS_TOKEN")));
        assert!(errors
            .iter()
            .any(|item| item.contains("CONSUMER_ENTRY_INGRESS_TOKEN")));
        assert!(errors.iter().any(|item| {
            item.contains("MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET")
                || item.contains("CONSUMER_ENTRY_SESSION_AUTH_SECRET")
                || item.contains("no explicit secret or shared issuer registry configured")
        }));
        assert!(errors
            .iter()
            .any(|item| item.contains("MATRIX_ENTRY_RECENT_EVENT_STORE_PATH")));
        assert!(errors
            .iter()
            .any(|item| item.contains("MATRIX_ENTRY_RATE_LIMIT_STORE_PATH")));
        assert!(errors
            .iter()
            .any(|item| item.contains("non-default MATRIX_BOT_USER_ID")));
    }

    #[test]
    fn load_session_auth_issuer_registry_rejects_invalid_active_key() {
        let temp_path = std::env::temp_dir().join(format!(
            "matrix-session-auth-issuer-registry-invalid-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_millis()
        ));
        std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"missing","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write issuer registry");

        let (metadata, registry) = load_session_auth_issuer_registry(temp_path.to_str());
        assert_eq!(metadata.load_status, "invalid_active_key");
        assert_eq!(metadata.revision.as_deref(), Some("rev-a"));
        assert!(metadata
            .load_error
            .as_deref()
            .unwrap_or_default()
            .contains("active key missing"));
        assert!(registry.is_empty());

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn load_consumer_entry_session_auth_issuer_registry_revision_approval_state_reads_revisions() {
        let path = std::env::temp_dir().join(format!(
            "matrix-session-auth-approval-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_millis()
        ));
        std::fs::write(
            &path,
            r#"{"version":1,"revision":"sess-approval-a","approved_revisions":["sess-reg-a","sess-reg-b","sess-reg-a","  "]}"#,
        )
        .unwrap();

        let mut config = test_config();
        config.consumer_entry_session_auth_issuer_registry_approved_revisions_path =
            Some(path.display().to_string());

        let state =
            load_consumer_entry_session_auth_issuer_registry_revision_approval_state(&config);
        assert_eq!(state.load_status, "loaded");
        assert_eq!(state.revision.as_deref(), Some("sess-approval-a"));
        assert_eq!(state.approved_revisions, vec!["sess-reg-a", "sess-reg-b"]);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn builds_signed_consumer_entry_session_auth_headers() {
        let mut config = test_config();
        config.consumer_entry_session_auth_explicit_secret =
            Some("matrix-session-secret".to_string());
        config.consumer_entry_session_auth_explicit_key_id = Some("v1".to_string());
        config.consumer_entry_session_auth_secret = Some("matrix-session-secret".to_string());
        config.consumer_entry_session_auth_key_id = Some("v1".to_string());
        let state = test_state(config);
        let request_body = json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!room:local.dev",
            "message": "hello"
        });

        let (assertion, signature) =
            build_consumer_entry_session_auth_headers(&state, &request_body)
                .unwrap()
                .unwrap();
        assert!(!assertion.is_empty());
        assert!(!signature.is_empty());
        let claims: UserSessionAuthClaims = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(&assertion)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(claims.issuer, "matrix-entry-adapter");
        assert_eq!(claims.key_id.as_deref(), Some("v1"));
        assert_eq!(claims.audience.as_deref(), Some("consumer-entry-api"));
        assert_eq!(
            claims.request_fingerprint.as_deref(),
            Some(build_matrix_request_fingerprint_from_body(&request_body).as_str())
        );
        assert_eq!(USER_SESSION_ASSERTION_HEADER, "x-cex-user-session");
        assert_eq!(
            USER_SESSION_SIGNATURE_HEADER,
            "x-cex-user-session-signature"
        );
    }

    #[test]
    fn builds_signed_consumer_entry_session_auth_headers_from_shared_issuer_registry() {
        let mut config = test_config();
        config.consumer_entry_session_auth_issuer_registry.insert(
            "matrix-entry-adapter".to_string(),
            SessionAuthIssuerRegistryIssuer {
                active_key_id: Some("v1".to_string()),
                keys: HashMap::from([("v1".to_string(), "registry-secret".to_string())]),
            },
        );
        let state = test_state(config);
        let request_body = json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!room:local.dev",
            "message": "hello"
        });

        let (assertion, signature) =
            build_consumer_entry_session_auth_headers(&state, &request_body)
                .unwrap()
                .unwrap();
        assert!(!assertion.is_empty());
        assert!(!signature.is_empty());
        let claims: UserSessionAuthClaims = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(&assertion)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(claims.issuer, "matrix-entry-adapter");
        assert_eq!(claims.key_id.as_deref(), Some("v1"));
    }

    #[tokio::test]
    async fn consumer_entry_session_auth_runtime_reload_updates_live_signer() {
        let registry_path = std::env::temp_dir().join(format!(
            "matrix-session-auth-runtime-reload-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_millis()
        ));
        let approval_path = std::env::temp_dir().join(format!(
            "matrix-session-auth-runtime-reload-approval-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_millis()
        ));
        std::fs::write(
            &registry_path,
            r#"{"version":1,"revision":"sess-reg-live-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .unwrap();
        std::fs::write(
            &approval_path,
            r#"{"version":1,"revision":"sess-approval-live","approved_revisions":["sess-reg-live-a","sess-reg-live-b"]}"#,
        )
        .unwrap();

        let (metadata, registry) = load_session_auth_issuer_registry(registry_path.to_str());
        let mut config = test_config();
        config.ingress_token = Some("admin-token".to_string());
        config.consumer_entry_session_auth_issuer_registry_path =
            Some(registry_path.display().to_string());
        config.consumer_entry_session_auth_issuer_registry = registry;
        config.consumer_entry_session_auth_issuer_registry_load_error = metadata.load_error.clone();
        config.consumer_entry_session_auth_issuer_registry_metadata = metadata;
        config.consumer_entry_session_auth_issuer_registry_approved_revisions_path =
            Some(approval_path.display().to_string());
        config.consumer_entry_session_auth_issuer_registry_require_approved_revision = true;
        let state = test_state(config);
        let request_body = json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!room:local.dev",
            "message": "hello"
        });

        let (assertion_before, _) =
            build_consumer_entry_session_auth_headers(&state, &request_body)
                .unwrap()
                .unwrap();
        let claims_before: UserSessionAuthClaims = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(&assertion_before)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(claims_before.key_id.as_deref(), Some("v1"));

        std::fs::write(
            &registry_path,
            r#"{"version":1,"revision":"sess-reg-live-b","issuers":{"matrix-entry-adapter":{"activeKeyId":"v2","keys":{"v1":"secret-a","v2":"secret-b"}}}}"#,
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("x-entry-token", "admin-token".parse().unwrap());

        let (validate_status, validate_body) = decode_json_response(
            validate_consumer_entry_session_auth_runtime(State(state.clone()), headers.clone())
                .await,
        )
        .await;
        assert_eq!(validate_status, StatusCode::OK);
        assert_eq!(validate_body["valid"], Value::Bool(true));
        assert_eq!(
            validate_body["consumer_entry_session_auth_runtime"]["metadata"]["revision"],
            Value::String("sess-reg-live-b".to_string())
        );

        let (reload_status, reload_body) = decode_json_response(
            reload_consumer_entry_session_auth_runtime(State(state.clone()), headers.clone()).await,
        )
        .await;
        assert_eq!(reload_status, StatusCode::OK);
        assert_eq!(reload_body["reloaded"], Value::Bool(true));
        assert_eq!(
            reload_body["consumer_entry_session_auth_runtime"]["metadata"]["revision"],
            Value::String("sess-reg-live-b".to_string())
        );

        let (status_code, status_body) = decode_json_response(
            get_consumer_entry_session_auth_status(State(state.clone()), headers).await,
        )
        .await;
        assert_eq!(status_code, StatusCode::OK);
        assert_eq!(
            status_body["consumer_entry_session_auth"]["issuer_registry_metadata"]["revision"],
            Value::String("sess-reg-live-b".to_string())
        );

        let (assertion_after, _) = build_consumer_entry_session_auth_headers(&state, &request_body)
            .unwrap()
            .unwrap();
        let claims_after: UserSessionAuthClaims = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(&assertion_after)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(claims_after.key_id.as_deref(), Some("v2"));

        let _ = std::fs::remove_file(registry_path);
        let _ = std::fs::remove_file(approval_path);
    }

    #[tokio::test]
    async fn consumer_entry_session_auth_runtime_reload_rejects_unapproved_revision() {
        let registry_path = std::env::temp_dir().join(format!(
            "matrix-session-auth-runtime-reject-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_millis()
        ));
        let approval_path = std::env::temp_dir().join(format!(
            "matrix-session-auth-runtime-reject-approval-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_millis()
        ));
        std::fs::write(
            &registry_path,
            r#"{"version":1,"revision":"sess-reg-reject-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .unwrap();
        std::fs::write(
            &approval_path,
            r#"{"version":1,"revision":"sess-approval-reject","approved_revisions":["sess-reg-reject-a"]}"#,
        )
        .unwrap();

        let (metadata, registry) = load_session_auth_issuer_registry(registry_path.to_str());
        let mut config = test_config();
        config.ingress_token = Some("admin-token".to_string());
        config.consumer_entry_session_auth_issuer_registry_path =
            Some(registry_path.display().to_string());
        config.consumer_entry_session_auth_issuer_registry = registry;
        config.consumer_entry_session_auth_issuer_registry_load_error = metadata.load_error.clone();
        config.consumer_entry_session_auth_issuer_registry_metadata = metadata;
        config.consumer_entry_session_auth_issuer_registry_approved_revisions_path =
            Some(approval_path.display().to_string());
        config.consumer_entry_session_auth_issuer_registry_require_approved_revision = true;
        let state = test_state(config);
        let request_body = json!({
            "matrix_user_id": "@alice:local.dev",
            "room_id": "!room:local.dev",
            "message": "hello"
        });

        std::fs::write(
            &registry_path,
            r#"{"version":1,"revision":"sess-reg-reject-b","issuers":{"matrix-entry-adapter":{"activeKeyId":"v2","keys":{"v1":"secret-a","v2":"secret-b"}}}}"#,
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("x-entry-token", "admin-token".parse().unwrap());
        let (reload_status, reload_body) = decode_json_response(
            reload_consumer_entry_session_auth_runtime(State(state.clone()), headers).await,
        )
        .await;
        assert_eq!(reload_status, StatusCode::CONFLICT);
        assert_eq!(reload_body["reloaded"], Value::Bool(false));
        assert_eq!(
            reload_body["status"],
            Value::String("current_revision_not_approved".to_string())
        );

        let (assertion_after, _) = build_consumer_entry_session_auth_headers(&state, &request_body)
            .unwrap()
            .unwrap();
        let claims_after: UserSessionAuthClaims = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(&assertion_after)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(claims_after.key_id.as_deref(), Some("v1"));

        let _ = std::fs::remove_file(registry_path);
        let _ = std::fs::remove_file(approval_path);
    }

    #[test]
    fn consumer_entry_session_auth_governance_overview_reports_selection_failure() {
        let mut config = test_config();
        config.runtime_profile = RuntimeProfile::Beta;
        config.consumer_entry_session_auth_issuer_registry_path =
            Some("./run/local-runtime/session-auth-issuer-registry.json".to_string());
        config.consumer_entry_session_auth_issuer_registry_metadata =
            SessionAuthIssuerRegistryMetadata {
                version: 1,
                revision: Some("sess-reg-a".to_string()),
                source_path: Some(
                    "./run/local-runtime/session-auth-issuer-registry.json".to_string(),
                ),
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
                issuer_count: 1,
                key_count: 1,
            };
        config.consumer_entry_session_auth_selection = SessionAuthIssuerRegistrySelection {
            source: "issuer_registry".to_string(),
            status: "issuer_missing".to_string(),
            issuer: "matrix-entry-adapter".to_string(),
            key_id: None,
            registry_path: Some(
                "./run/local-runtime/session-auth-issuer-registry.json".to_string(),
            ),
            detail: Some(
                "shared issuer registry does not contain issuer matrix-entry-adapter".to_string(),
            ),
        };

        let approval_state = SessionAuthIssuerRegistryRevisionApprovalState::default();
        let overview = consumer_entry_session_auth_governance_overview_json(
            &config,
            &config.consumer_entry_session_auth_issuer_registry_metadata,
            &config.consumer_entry_session_auth_selection,
            &approval_state,
            5,
        );
        assert_eq!(overview["status"], "issuer_missing");
        assert_eq!(overview["valid"], Value::Bool(false));
        assert_eq!(overview["expected"], Value::Bool(true));
        assert_eq!(overview["checks"]["selection_ok"], Value::Bool(false));
        assert_eq!(overview["checks"]["registry_loaded"], Value::Bool(true));
        assert_eq!(overview["checks"]["revision_present"], Value::Bool(true));
    }

    #[tokio::test]
    async fn health_and_metrics_expose_consumer_entry_session_auth_governance() {
        let mut config = test_config();
        config.runtime_profile = RuntimeProfile::Beta;
        config.consumer_entry_session_auth_issuer_registry_path =
            Some("./run/local-runtime/session-auth-issuer-registry.json".to_string());
        config.consumer_entry_session_auth_issuer_registry_metadata =
            SessionAuthIssuerRegistryMetadata {
                version: 1,
                revision: None,
                source_path: Some(
                    "./run/local-runtime/session-auth-issuer-registry.json".to_string(),
                ),
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
                issuer_count: 1,
                key_count: 1,
            };
        config.consumer_entry_session_auth_issuer_registry.insert(
            "matrix-entry-adapter".to_string(),
            SessionAuthIssuerRegistryIssuer {
                active_key_id: Some("v1".to_string()),
                keys: HashMap::from([("v1".to_string(), "registry-secret".to_string())]),
            },
        );
        config.consumer_entry_session_auth_selection = SessionAuthIssuerRegistrySelection {
            source: "issuer_registry".to_string(),
            status: "ok".to_string(),
            issuer: "matrix-entry-adapter".to_string(),
            key_id: Some("v1".to_string()),
            registry_path: Some(
                "./run/local-runtime/session-auth-issuer-registry.json".to_string(),
            ),
            detail: Some("selected signing secret from shared issuer registry".to_string()),
        };
        config.consumer_entry_session_auth_secret = Some("registry-secret".to_string());

        let state = test_state(config);
        let Json(health_body) = health(State(state.clone())).await;
        assert_eq!(
            health_body["consumer_entry_session_auth_governance_overview"]["status"],
            "issuer_registry_revision_missing"
        );
        assert_eq!(
            health_body["profile_validation"]["checks"]
                ["consumer_entry_session_auth_governance_valid"],
            Value::Bool(false)
        );

        let response = metrics(State(state)).await;
        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body.contains("cex_matrix_entry_consumer_entry_session_auth_governance_valid 0"));
    }

    #[tokio::test]
    async fn health_and_metrics_expose_consumer_entry_session_auth_approval_governance() {
        let approval_path = std::env::temp_dir().join(format!(
            "matrix-session-auth-approval-health-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_millis()
        ));
        std::fs::write(
            &approval_path,
            r#"{"version":1,"revision":"sess-approval-b","approved_revisions":["sess-reg-other"]}"#,
        )
        .unwrap();

        let mut config = test_config();
        config.runtime_profile = RuntimeProfile::Beta;
        config.consumer_entry_session_auth_issuer_registry_path =
            Some("./run/local-runtime/session-auth-issuer-registry.json".to_string());
        config.consumer_entry_session_auth_issuer_registry_metadata =
            SessionAuthIssuerRegistryMetadata {
                version: 1,
                revision: Some("sess-reg-live".to_string()),
                source_path: Some(
                    "./run/local-runtime/session-auth-issuer-registry.json".to_string(),
                ),
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
                issuer_count: 1,
                key_count: 1,
            };
        config.consumer_entry_session_auth_issuer_registry.insert(
            "matrix-entry-adapter".to_string(),
            SessionAuthIssuerRegistryIssuer {
                active_key_id: Some("v1".to_string()),
                keys: HashMap::from([("v1".to_string(), "registry-secret".to_string())]),
            },
        );
        config.consumer_entry_session_auth_selection = SessionAuthIssuerRegistrySelection {
            source: "issuer_registry".to_string(),
            status: "ok".to_string(),
            issuer: "matrix-entry-adapter".to_string(),
            key_id: Some("v1".to_string()),
            registry_path: Some(
                "./run/local-runtime/session-auth-issuer-registry.json".to_string(),
            ),
            detail: Some("selected signing secret from shared issuer registry".to_string()),
        };
        config.consumer_entry_session_auth_secret = Some("registry-secret".to_string());
        config.consumer_entry_session_auth_issuer_registry_approved_revisions_path =
            Some(approval_path.display().to_string());
        config.consumer_entry_session_auth_issuer_registry_require_approved_revision = true;

        let state = test_state(config);
        let Json(health_body) = health(State(state.clone())).await;
        assert_eq!(
            health_body["consumer_entry_session_auth_governance_overview"]["status"],
            "current_revision_not_approved"
        );
        assert_eq!(
            health_body["profile_validation"]["checks"]
                ["consumer_entry_session_auth_issuer_registry_revision_approved"],
            Value::Bool(false)
        );

        let response = metrics(State(state)).await;
        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(
            body.contains("cex_matrix_entry_consumer_entry_session_auth_approval_source_valid 1")
        );
        assert!(
            body.contains("cex_matrix_entry_consumer_entry_session_auth_approval_coverage_valid 0")
        );

        let _ = std::fs::remove_file(approval_path);
    }

    #[test]
    fn prune_rate_limit_cache_drops_expired_entries() {
        let now = 1_760_000_100;
        let mut cache = RateLimitCache {
            seen: HashMap::from([(
                "matrix:@alice:local.dev:!room:local.dev".to_string(),
                VecDeque::from([now - 120, now - 10]),
            )]),
        };

        prune_rate_limit_cache(&mut cache, now, 60);
        assert_eq!(
            cache
                .seen
                .get("matrix:@alice:local.dev:!room:local.dev")
                .cloned(),
            Some(VecDeque::from([now - 10]))
        );
    }

    #[test]
    fn load_rate_limit_cache_prunes_persisted_entries() {
        let path = std::env::temp_dir().join(format!(
            "matrix-entry-rate-limit-{}.json",
            std::process::id()
        ));
        let now = Utc::now().timestamp();
        std::fs::write(
            &path,
            serde_json::to_vec(&RateLimitCache {
                seen: HashMap::from([(
                    "matrix:@alice:local.dev:!room:local.dev".to_string(),
                    VecDeque::from([now - 120, now - 5]),
                )]),
            })
            .unwrap(),
        )
        .unwrap();

        let mut config = test_config();
        config.rate_limit_store_path = Some(path.display().to_string());
        let cache = load_rate_limit_cache(&config);
        assert_eq!(
            cache
                .seen
                .get("matrix:@alice:local.dev:!room:local.dev")
                .cloned(),
            Some(VecDeque::from([now - 5]))
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn extracts_body_from_matrix_content() {
        let content = json!({ "body": "hello" });
        assert_eq!(
            extract_matrix_body(Some(&content)).as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn builds_reply_from_forwarded_task() {
        let projected = super::build_projected_matrix_reply(&json!({
            "task_id": "task-123",
            "consumer_status": "queued",
            "invocation_status": "Queued"
        }));
        assert_eq!(projected["msgtype"], "m.text");
        assert_eq!(projected["cex_task_id"], "task-123");
    }

    #[test]
    fn parse_matrix_command_task_with_flags() {
        let command =
            parse_matrix_command("/task cap=cap.demo.summarize account=acc-1 生成摘要内容");
        assert_eq!(
            command,
            ParsedCommand::Task {
                text: "生成摘要内容".to_string(),
                capability_id: Some("cap.demo.summarize".to_string()),
                account_id: Some("acc-1".to_string())
            }
        );
    }

    #[test]
    fn parse_matrix_command_help() {
        assert_eq!(parse_matrix_command("/help"), ParsedCommand::Help);
    }

    #[test]
    fn parse_matrix_command_status() {
        assert_eq!(
            parse_matrix_command("/status task-123"),
            ParsedCommand::Status {
                task_id: "task-123".to_string()
            }
        );
    }

    #[test]
    fn parse_matrix_command_wallet_and_plans_aliases() {
        assert_eq!(parse_matrix_command("/balance"), ParsedCommand::Wallet);
        assert_eq!(parse_matrix_command("/钱包"), ParsedCommand::Wallet);
        assert_eq!(parse_matrix_command("/plans"), ParsedCommand::Plans);
        assert_eq!(parse_matrix_command("/套餐"), ParsedCommand::Plans);
    }

    #[test]
    fn builds_wallet_reply_with_matrix_safe_card_values() {
        let reply = build_wallet_matrix_reply(&json!({
            "account": {
                "account_id": "00000000-0000-0000-0000-00000000ce31",
                "currency_unit": "credit",
                "balance": 1000.0,
                "reserved": 0.0,
                "available": 1000.0
            },
            "package": {
                "name": "Local Production Credits",
                "billing_model": "credit_wallet"
            }
        }));

        assert_eq!(reply["msgtype"], "m.text");
        assert_eq!(reply["cex_card"]["type"], "wallet_summary");
        assert_eq!(reply["cex_card"]["available"], "1000.00");
        assert_eq!(reply["cex_card"]["reserved"], "0.00");
    }

    #[test]
    fn parse_trillionnium_league_commands() {
        assert_eq!(parse_matrix_command("/league"), ParsedCommand::League);
        assert_eq!(parse_matrix_command("/tl"), ParsedCommand::League);
        assert_eq!(parse_matrix_command("/app"), ParsedCommand::ClientApp);
        assert_eq!(
            parse_matrix_command("/feed"),
            ParsedCommand::ClientFeed { filter: None }
        );
        assert_eq!(
            parse_matrix_command("/feed tasks"),
            ParsedCommand::ClientFeed {
                filter: Some("route_task".to_string())
            }
        );
        assert_eq!(
            parse_matrix_command("/feed 成交"),
            ParsedCommand::ClientFeed {
                filter: Some("commerce".to_string())
            }
        );
        assert_eq!(parse_matrix_command("/social"), ParsedCommand::ClientSocial);
        assert_eq!(
            parse_matrix_command("/duel nearby 用 Forge Builder 发起开局出招"),
            ParsedCommand::ClientDuel {
                opponent: Some("nearby".to_string()),
                body: "用 Forge Builder 发起开局出招".to_string()
            }
        );
        assert_eq!(parse_matrix_command("/arena"), ParsedCommand::Arena);
        assert_eq!(parse_matrix_command("/quest"), ParsedCommand::Quest);
        assert_eq!(parse_matrix_command("/world"), ParsedCommand::World);
        assert_eq!(parse_matrix_command("/map"), ParsedCommand::WorldMap);
        assert_eq!(parse_matrix_command("/world map"), ParsedCommand::WorldMap);
        assert_eq!(
            parse_matrix_command("/go east"),
            ParsedCommand::WorldMapMove {
                target: "east".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/world action 我要开一家 AI 设计公司"),
            ParsedCommand::WorldAction {
                body: "我要开一家 AI 设计公司".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/craft 建一个自动交付工坊"),
            ParsedCommand::CraftAction {
                body: "建一个自动交付工坊".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/contract 帮客户整理店铺启动方案"),
            ParsedCommand::WorldContract {
                body: "帮客户整理店铺启动方案".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/complete world-contract-1 交付方案和证据"),
            ParsedCommand::WorldContractComplete {
                contract_id: "world-contract-1".to_string(),
                body: "交付方案和证据".to_string()
            }
        );
        assert_eq!(parse_matrix_command("/assets"), ParsedCommand::WorldAssets);
        assert_eq!(
            parse_matrix_command("/upgrade latest 增强交付证据和商业闭环"),
            ParsedCommand::WorldAssetUpgrade {
                asset_id: "latest".to_string(),
                body: "增强交付证据和商业闭环".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/company latest 开一家 AI 交付公司"),
            ParsedCommand::WorldCompanyCreate {
                asset_id: "latest".to_string(),
                body: "开一家 AI 交付公司".to_string()
            }
        );
        assert_eq!(parse_matrix_command("/shops"), ParsedCommand::WorldShops);
        assert_eq!(
            parse_matrix_command("/sell latest 上架一套 AI 设计服务"),
            ParsedCommand::WorldListingCreate {
                company_id: "latest".to_string(),
                body: "上架一套 AI 设计服务".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/buy latest 购买这套服务并生成工作单"),
            ParsedCommand::WorldListingBuy {
                listing_id: "latest".to_string(),
                body: "购买这套服务并生成工作单".to_string()
            }
        );
        assert_eq!(parse_matrix_command("/work"), ParsedCommand::WorldWork);
        assert_eq!(
            parse_matrix_command("/work deliver latest 交付证据包和下一步计划"),
            ParsedCommand::WorldWorkDeliver {
                work_order_id: "latest".to_string(),
                body: "交付证据包和下一步计划".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/work accept latest 验收通过并进入复购"),
            ParsedCommand::WorldWorkAccept {
                work_order_id: "latest".to_string(),
                body: "验收通过并进入复购".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/work reject latest 拒收并退回预留款"),
            ParsedCommand::WorldWorkReject {
                work_order_id: "latest".to_string(),
                body: "拒收并退回预留款".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/work reopen latest 补齐证据后重新交付"),
            ParsedCommand::WorldWorkReopen {
                work_order_id: "latest".to_string(),
                body: "补齐证据后重新交付".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/work cancel latest 交付前取消并退回预留款"),
            ParsedCommand::WorldWorkCancel {
                work_order_id: "latest".to_string(),
                body: "交付前取消并退回预留款".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/factions"),
            ParsedCommand::WorldFactions
        );
        assert_eq!(parse_matrix_command("/season"), ParsedCommand::Season);
        assert_eq!(parse_matrix_command("/rank"), ParsedCommand::Rank);
        assert_eq!(parse_matrix_command("/loadout"), ParsedCommand::Loadout);
        assert_eq!(parse_matrix_command("/level"), ParsedCommand::Progression);
        assert_eq!(parse_matrix_command("/技能"), ParsedCommand::Skills);
        assert_eq!(parse_matrix_command("/装备"), ParsedCommand::Tools);
        assert_eq!(parse_matrix_command("/皮肤"), ParsedCommand::Skins);
        assert_eq!(parse_matrix_command("/profile"), ParsedCommand::Profile);
        assert_eq!(parse_matrix_command("/rewards"), ParsedCommand::Rewards);
        assert_eq!(parse_matrix_command("/history"), ParsedCommand::History);
        assert_eq!(
            parse_matrix_command("/guild guild-prompt-forge"),
            ParsedCommand::Guild {
                guild_id: Some("guild-prompt-forge".to_string())
            }
        );
        assert_eq!(
            parse_matrix_command("/draft oracle_scout forge_builder mirror_auditor"),
            ParsedCommand::Draft {
                heroes: vec![
                    "oracle_scout".to_string(),
                    "forge_builder".to_string(),
                    "mirror_auditor".to_string()
                ]
            }
        );
        assert_eq!(
            parse_matrix_command("/join daily-dungeon-001"),
            ParsedCommand::Join {
                match_id: "daily-dungeon-001".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/battle daily-dungeon-001 生成一版方案"),
            ParsedCommand::Battle {
                match_id: "daily-dungeon-001".to_string(),
                text: "生成一版方案".to_string()
            }
        );
        assert_eq!(
            parse_matrix_command("/submit daily-dungeon-001 最终提交"),
            ParsedCommand::Submit {
                match_id: "daily-dungeon-001".to_string(),
                body: "最终提交".to_string()
            }
        );
    }

    #[test]
    fn builds_trillionnium_league_home_card() {
        let reply = build_league_home_matrix_reply(None);
        assert_eq!(reply["msgtype"], "m.text");
        assert_eq!(reply["cex_card"]["type"], "league_home");
        assert_eq!(reply["cex_card"]["league"], "trillionnium_league");
    }

    #[test]
    fn validate_text_payload_rejects_empty_and_large_input() {
        assert!(validate_text_payload("   ", DEFAULT_MAX_TEXT_CHARS).is_err());
        assert!(validate_text_payload(
            &"x".repeat(DEFAULT_MAX_TEXT_CHARS + 1),
            DEFAULT_MAX_TEXT_CHARS
        )
        .is_err());
        assert_eq!(
            validate_text_payload("  hello  ", DEFAULT_MAX_TEXT_CHARS).unwrap(),
            "hello"
        );
    }

    #[test]
    fn builds_matrix_event_rate_limit_key() {
        let event = MatrixEventEnvelope {
            event_id: Some("$event-1".to_string()),
            event_type: Some("m.room.message".to_string()),
            room_id: "!room:local.dev".to_string(),
            sender: "@alice:local.dev".to_string(),
            text: Some("hello".to_string()),
            content: None,
            timestamp_ms: None,
            metadata: None,
        };
        assert_eq!(
            build_matrix_event_rate_limit_key(&event),
            "matrix:@alice:local.dev:!room:local.dev"
        );
    }

    #[test]
    fn prune_recent_event_cache_drops_expired_entries() {
        let mut cache = RecentEventCache::default();
        let now = Utc::now().timestamp();
        let old = now - 120;
        cache.seen.insert("old".to_string(), old);
        cache.order.push_back("old".to_string());
        prune_recent_event_cache(&mut cache, now, 60, 10);
        assert!(cache.seen.is_empty());
        assert!(cache.order.is_empty());
    }
}
