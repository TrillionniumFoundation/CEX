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
    League,
    Arena,
    Quest,
    World,
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
        ParsedCommand::World => match fetch_consumer_entry_get(&state, "/v1/league/world").await {
            Ok(value) => league_response(
                "league_world",
                event,
                value.clone(),
                build_league_world_matrix_reply(&value),
            ),
            Err(response) => response,
        },
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
    loop {
        let Some(front) = cache.order.front().cloned() else {
            break;
        };

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
        "/league" | "/tl" | "/trillionnium" => ParsedCommand::League,
        "/arena" | "/matches" => ParsedCommand::Arena,
        "/quest" | "/quests" | "/daily" => ParsedCommand::Quest,
        "/world" | "/map" => ParsedCommand::World,
        "/season" => ParsedCommand::Season,
        "/raid" | "/raids" => parse_raid_command(parts),
        "/team" | "/party" | "/roster" => parse_team_command(parts),
        "/rank" | "/leaderboard" => ParsedCommand::Rank,
        "/loadout" | "/agent" | "/agents" => ParsedCommand::Loadout,
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
        "/balance" | "/wallet" | "/余额" | "/钱包" => ParsedCommand::Wallet,
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

fn build_league_world_matrix_reply(value: &Value) -> Value {
    let zone_count = value
        .get("zones")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(5);
    let body = format!(
        "🗺️ Trillionnium World Map\n已开放/预告区域：{zone_count}\nPrompt Forge｜Research Wilds｜Code Citadel｜Audit Sanctum｜Market Bazaar\n\n进入赛场：/arena\n加入公会：/guild"
    );
    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🗺️ Trillionnium World Map</h3><p><strong>Zones</strong>: {}</p><p>Prompt Forge｜Research Wilds｜Code Citadel｜Audit Sanctum｜Market Bazaar</p><p><code>/arena</code> · <code>/guild</code></p></blockquote>",
            zone_count,
        ),
        "cex_card": {"type": "league_world", "version": 1, "league": "trillionnium_league", "zone_count": zone_count}
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
    let body = format!(
        "🏁 League Submission 已评分\nMatch: {match_id}\nScore: {score:.1}\nGrade: {grade}\nReward: {reward_amount:.2} credit\nJudge: {judge_status} ({score_event_count} dims)\nPayout: {payout_status}\nLedger: {ledger_status}\n排行榜：/rank\n奖励：/rewards"
    );

    json!({
        "msgtype": "m.text",
        "body": body,
        "format": "org.matrix.custom.html",
        "formatted_body": format!(
            "<blockquote><h3>🏁 League Submission 已评分</h3><p><strong>Match</strong>: <code>{}</code></p><p><strong>Score</strong>: {:.1}</p><p><strong>Grade</strong>: {}</p><p><strong>Reward</strong>: {:.2} credit</p><p><strong>Judge</strong>: {} / {} dims</p><p><strong>Payout</strong>: {}</p><p><strong>Ledger</strong>: {}</p><p><code>/rank</code> · <code>/rewards</code></p></blockquote>",
            escape_html(match_id),
            score,
            escape_html(grade),
            reward_amount,
            escape_html(judge_status),
            score_event_count,
            escape_html(payout_status),
            escape_html(ledger_status),
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
        "可用命令:\n/league - 进入 Trillionnium League\n/world - 世界地图\n/season - 赛季\n/arena - 查看赛场\n/quest - 今日副本\n/guild - 公会列表，/guild <guild-id> 加入\n/raid - 团本列表，/raid <raid-id> <行动> 贡献团本\n/team - 团本队伍，/team <raid-id> <role> 认领职责\n/draft <hero...> - 锁定 Agent 英雄阵容\n/join <match-id> - 加入赛场\n/battle <match-id> <行动> - 在赛场中出招并创建 CEX 执行\n/submit <match-id> <提交内容> - 交卷评分并领取奖励\n/profile - 玩家档案\n/rank - 排行榜\n/loadout - Agent 阵容\n/rewards - 奖励记录\n/inventory - 背包/装备\n/history - 战斗历史\n/task <内容> [cap=<能力id>] [account=<账户id>] - 创建普通任务\n/status <task-id> - 查询任务状态\n/balance 或 /wallet - 查看余额\n/plans 或 /套餐 - 查看套餐",
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
        assert_eq!(parse_matrix_command("/arena"), ParsedCommand::Arena);
        assert_eq!(parse_matrix_command("/quest"), ParsedCommand::Quest);
        assert_eq!(parse_matrix_command("/world"), ParsedCommand::World);
        assert_eq!(parse_matrix_command("/season"), ParsedCommand::Season);
        assert_eq!(parse_matrix_command("/rank"), ParsedCommand::Rank);
        assert_eq!(parse_matrix_command("/loadout"), ParsedCommand::Loadout);
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
