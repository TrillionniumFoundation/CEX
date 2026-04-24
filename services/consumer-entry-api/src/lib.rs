#![recursion_limit = "256"]

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

const DEFAULT_MAX_TEXT_CHARS: usize = 4_000;
const DEFAULT_RATE_LIMIT_WINDOW_SECS: u64 = 60;
const DEFAULT_RATE_LIMIT_MAX_REQUESTS: usize = 30;
const DEFAULT_REPLAY_WINDOW_SECS: u64 = 600;
const DEFAULT_REPLAY_CACHE_SIZE: usize = 2048;
const DEFAULT_SESSION_AUTH_MAX_CLOCK_SKEW_SECS: u64 = 300;
const DEFAULT_SESSION_AUTH_MAX_TTL_SECS: u64 = 900;
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
        Arc,
        RwLock as StdRwLock,
    },
    time::UNIX_EPOCH,
};
use tokio::sync::{Mutex, RwLock};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    http: Client,
    config: ConsumerEntryConfig,
    identity_binding_store: RwLock<IdentityBindingStore>,
    identity_binding_audit_state: RwLock<IdentityBindingAuditState>,
    session_auth_issuer_registry_state: StdRwLock<SessionAuthIssuerRegistryRuntimeState>,
    rate_limits: Mutex<RateLimitCache>,
    replay_cache: Mutex<ReplayCache>,
    metrics: ConsumerEntryMetrics,
}

#[derive(Debug, Default)]
struct ConsumerEntryMetrics {
    task_create_requests: AtomicU64,
    task_lookup_requests: AtomicU64,
    rate_limited_requests: AtomicU64,
    rate_limited_source_scope_requests: AtomicU64,
    rate_limited_user_requests: AtomicU64,
    rate_limited_room_requests: AtomicU64,
    rate_limited_session_requests: AtomicU64,
    rate_limited_org_requests: AtomicU64,
    identity_binding_failures: AtomicU64,
    identity_binding_matches: AtomicU64,
    identity_binding_reload_requests: AtomicU64,
    identity_binding_reload_successes: AtomicU64,
    identity_binding_reload_rejections: AtomicU64,
    identity_binding_reload_actor_rejections: AtomicU64,
    identity_binding_audit_failures: AtomicU64,
    ingress_auth_failures: AtomicU64,
    session_auth_successes: AtomicU64,
    session_auth_failures: AtomicU64,
    replay_hits: AtomicU64,
}

impl ConsumerEntryMetrics {
    fn inc_task_create_requests(&self) {
        self.task_create_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_task_lookup_requests(&self) {
        self.task_lookup_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_requests(&self) {
        self.rate_limited_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_source_scope_requests(&self) {
        self.rate_limited_source_scope_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_user_requests(&self) {
        self.rate_limited_user_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_room_requests(&self) {
        self.rate_limited_room_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_session_requests(&self) {
        self.rate_limited_session_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_rate_limited_org_requests(&self) {
        self.rate_limited_org_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_failures(&self) {
        self.identity_binding_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_matches(&self) {
        self.identity_binding_matches
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_reload_requests(&self) {
        self.identity_binding_reload_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_reload_successes(&self) {
        self.identity_binding_reload_successes
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_reload_rejections(&self) {
        self.identity_binding_reload_rejections
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_audit_failures(&self) {
        self.identity_binding_audit_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_identity_binding_reload_actor_rejections(&self) {
        self.identity_binding_reload_actor_rejections
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_ingress_auth_failures(&self) {
        self.ingress_auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_session_auth_successes(&self) {
        self.session_auth_successes.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_session_auth_failures(&self) {
        self.session_auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_replay_hits(&self) {
        self.replay_hits.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> Value {
        json!({
            "task_create_requests": self.task_create_requests.load(Ordering::Relaxed),
            "task_lookup_requests": self.task_lookup_requests.load(Ordering::Relaxed),
            "rate_limited_requests": self.rate_limited_requests.load(Ordering::Relaxed),
            "rate_limited_source_scope_requests": self.rate_limited_source_scope_requests.load(Ordering::Relaxed),
            "rate_limited_user_requests": self.rate_limited_user_requests.load(Ordering::Relaxed),
            "rate_limited_room_requests": self.rate_limited_room_requests.load(Ordering::Relaxed),
            "rate_limited_session_requests": self.rate_limited_session_requests.load(Ordering::Relaxed),
            "rate_limited_org_requests": self.rate_limited_org_requests.load(Ordering::Relaxed),
            "identity_binding_failures": self.identity_binding_failures.load(Ordering::Relaxed),
            "identity_binding_matches": self.identity_binding_matches.load(Ordering::Relaxed),
            "identity_binding_reload_requests": self.identity_binding_reload_requests.load(Ordering::Relaxed),
            "identity_binding_reload_successes": self.identity_binding_reload_successes.load(Ordering::Relaxed),
            "identity_binding_reload_rejections": self.identity_binding_reload_rejections.load(Ordering::Relaxed),
            "identity_binding_reload_actor_rejections": self.identity_binding_reload_actor_rejections.load(Ordering::Relaxed),
            "identity_binding_audit_failures": self.identity_binding_audit_failures.load(Ordering::Relaxed),
            "ingress_auth_failures": self.ingress_auth_failures.load(Ordering::Relaxed),
            "session_auth_successes": self.session_auth_successes.load(Ordering::Relaxed),
            "session_auth_failures": self.session_auth_failures.load(Ordering::Relaxed),
            "replay_hits": self.replay_hits.load(Ordering::Relaxed),
        })
    }
}

#[derive(Clone, Copy)]
enum RateLimitBucketKind {
    SourceScope,
    User,
    Room,
    Session,
    Org,
}

#[derive(Debug, Clone, Serialize)]
struct IdentityScope {
    source_kind: &'static str,
    user_id: Option<String>,
    room_id: Option<String>,
    session_id: Option<String>,
    org_id: Option<String>,
    account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct IdentityResolution {
    matched: bool,
    required: bool,
    binding_subject: Option<String>,
    binding_source_format: String,
    binding_version: u32,
    binding_revision: Option<String>,
    product_user_id: Option<String>,
    binding_source_kind: String,
}

#[derive(Debug, Clone, Serialize)]
struct ResolvedIdentity {
    scope: IdentityScope,
    resolution: IdentityResolution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserSessionAuthClaims {
    #[serde(default = "default_session_auth_version")]
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionAuthIssuerRegistryIssuer {
    #[serde(default, alias = "activeKeyId")]
    active_key_id: Option<String>,
    #[serde(default)]
    keys: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct SessionAuthIssuerRegistryRuntimeState {
    metadata: SessionAuthIssuerRegistryMetadata,
    registry: HashMap<String, SessionAuthIssuerRegistryIssuer>,
}

#[derive(Debug, Clone, Serialize)]
struct AuthorizedUserSession {
    claims: UserSessionAuthClaims,
    assertion_header: &'static str,
    signature_header: &'static str,
}

#[derive(Debug, Clone, Default)]
struct IdentityBindingStore {
    metadata: IdentityBindingMetadata,
    registry_metadata: IdentityBindingMetadata,
    bindings: IdentityBindings,
    product_users: HashMap<String, ProductUserIdentity>,
}

#[derive(Debug, Clone)]
struct IdentityBindingAuditState {
    path: Option<String>,
    last_event_kind: Option<String>,
    last_event_epoch: Option<i64>,
    last_status: String,
    last_error: Option<String>,
    last_policy_decision: Option<String>,
    last_policy_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct IdentityAuditQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
struct IdentityApprovalQuery {
    limit: Option<usize>,
}

impl Default for IdentityBindingAuditState {
    fn default() -> Self {
        Self {
            path: None,
            last_event_kind: None,
            last_event_epoch: None,
            last_status: "disabled".to_string(),
            last_error: None,
            last_policy_decision: None,
            last_policy_reason: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct IdentityBindingMetadata {
    format: String,
    version: u32,
    revision: Option<String>,
    source_path: Option<String>,
    source_modified_epoch: Option<i64>,
    loaded_at_epoch: Option<i64>,
    load_status: String,
    load_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct IdentityBindingReloadGovernance {
    accepted: bool,
    decision: String,
    reason: String,
    current_format: String,
    current_revision: Option<String>,
    current_registry_revision: Option<String>,
    current_effective_revision: Option<String>,
    current_missing_product_user_refs: usize,
    candidate_format: String,
    candidate_revision: Option<String>,
    candidate_registry_revision: Option<String>,
    candidate_effective_revision: Option<String>,
    candidate_load_status: String,
    candidate_registry_load_status: String,
    candidate_missing_product_user_refs: usize,
    separate_registry_configured: bool,
    approval_state_status: String,
    approval_state_revision: Option<String>,
    approved_revision_count: usize,
    candidate_revision_approved: Option<bool>,
    rollback_blocked: bool,
    requesting_actor: Option<String>,
    actor_header: String,
    actor_authorized: Option<bool>,
    actor_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct IdentityBindingRevisionApprovalState {
    source_path: Option<String>,
    source_modified_epoch: Option<i64>,
    loaded_at_epoch: Option<i64>,
    load_status: String,
    load_error: Option<String>,
    version: u32,
    revision: Option<String>,
    approved_revisions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
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

impl Default for IdentityBindingRevisionApprovalState {
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

impl Default for IdentityBindingMetadata {
    fn default() -> Self {
        Self {
            format: "none".to_string(),
            version: 0,
            revision: None,
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: None,
            load_status: "disabled".to_string(),
            load_error: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IdentityBindings {
    #[serde(default)]
    chat_users: HashMap<String, IdentityBindingEntry>,
    #[serde(default)]
    matrix_users: HashMap<String, IdentityBindingEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IdentityBindingsDocument {
    #[serde(default = "default_identity_bindings_version")]
    version: u32,
    revision: Option<String>,
    #[serde(default)]
    product_users: HashMap<String, ProductUserIdentity>,
    #[serde(default)]
    chat_users: HashMap<String, IdentityBindingEntry>,
    #[serde(default)]
    matrix_users: HashMap<String, IdentityBindingEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProductUserRegistryDocument {
    #[serde(default = "default_identity_bindings_version")]
    version: u32,
    revision: Option<String>,
    #[serde(default)]
    product_users: HashMap<String, ProductUserIdentity>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IdentityBindingRevisionApprovalDocument {
    #[serde(default = "default_identity_binding_approval_version")]
    version: u32,
    revision: Option<String>,
    #[serde(default)]
    approved_revisions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SessionAuthIssuerRegistryRevisionApprovalDocument {
    #[serde(default = "default_identity_binding_approval_version")]
    version: u32,
    revision: Option<String>,
    #[serde(default)]
    approved_revisions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IdentityBindingEntry {
    product_user_id: Option<String>,
    org_id: Option<String>,
    account_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProductUserIdentity {
    org_id: Option<String>,
    account_id: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ReplayCache {
    seen: HashMap<String, ReplayEntry>,
    order: VecDeque<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplayEntry {
    seen_at_epoch: i64,
    response: Option<Value>,
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
        match first_present_env(&["CONSUMER_ENTRY_RUNTIME_PROFILE", "CEX_RUNTIME_PROFILE"])
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
pub struct ConsumerEntryConfig {
    pub runtime_profile: RuntimeProfile,
    pub bind_addr: String,
    pub cex_gateway_base_url: String,
    pub cex_gateway_api_key: String,
    pub default_capability_id: Option<String>,
    pub default_account_id: Option<String>,
    pub ingress_token: Option<String>,
    pub require_session_auth: bool,
    pub session_auth_secret: Option<String>,
    pub session_auth_issuer_secrets: HashMap<String, String>,
    pub session_auth_issuer_keys: HashMap<String, HashMap<String, String>>,
    pub session_auth_issuer_registry_path: Option<String>,
    pub session_auth_issuer_registry: HashMap<String, SessionAuthIssuerRegistryIssuer>,
    pub session_auth_issuer_registry_load_error: Option<String>,
    pub session_auth_issuer_registry_metadata: SessionAuthIssuerRegistryMetadata,
    pub session_auth_allowed_issuers: Vec<String>,
    pub session_auth_expected_audience: Option<String>,
    pub session_auth_issuer_registry_approved_revisions_path: Option<String>,
    pub session_auth_issuer_registry_require_approved_revision: bool,
    pub session_auth_issuer_registry_require_actor: bool,
    pub session_auth_issuer_registry_actor_header: String,
    pub session_auth_issuer_registry_allowed_actors: Vec<String>,
    pub session_auth_max_clock_skew_secs: u64,
    pub session_auth_max_ttl_secs: u64,
    pub identity_bindings_path: Option<String>,
    pub identity_registry_path: Option<String>,
    pub identity_binding_audit_log_path: Option<String>,
    pub identity_binding_approved_revisions_path: Option<String>,
    pub identity_binding_reload_require_revision: bool,
    pub identity_binding_reload_reject_same_revision: bool,
    pub identity_binding_reload_allow_legacy_format: bool,
    pub identity_binding_reload_require_approved_revision: bool,
    pub identity_binding_reload_allow_rollback: bool,
    pub identity_binding_reload_require_actor: bool,
    pub identity_binding_reload_actor_header: String,
    pub identity_binding_reload_allowed_actors: Vec<String>,
    pub require_identity_binding: bool,
    pub max_text_chars: usize,
    pub rate_limit_window_secs: u64,
    pub rate_limit_max_requests: usize,
    pub rate_limit_user_max_requests: usize,
    pub rate_limit_room_max_requests: usize,
    pub rate_limit_session_max_requests: usize,
    pub rate_limit_org_max_requests: usize,
    pub rate_limit_store_path: Option<String>,
    pub replay_window_secs: u64,
    pub replay_cache_size: usize,
    pub replay_store_path: Option<String>,
}

impl ConsumerEntryConfig {
    pub fn from_env() -> Self {
        let session_auth_issuer_registry_path = first_present_env(&[
            "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH",
            "CEX_SESSION_AUTH_ISSUER_REGISTRY_PATH",
        ]);
        let (
            session_auth_issuer_registry_metadata,
            session_auth_issuer_registry,
        ) = load_session_auth_issuer_registry(session_auth_issuer_registry_path.as_deref());
        let session_auth_issuer_registry_load_error =
            session_auth_issuer_registry_metadata.load_error.clone();

        Self {
            runtime_profile: RuntimeProfile::from_env(),
            bind_addr: env::var("CONSUMER_ENTRY_BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8090".to_string()),
            cex_gateway_base_url: env::var("CEX_GATEWAY_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string()),
            cex_gateway_api_key: env::var("CEX_GATEWAY_API_KEY")
                .unwrap_or_else(|_| "local-dev-key".to_string()),
            default_capability_id: env::var("CONSUMER_ENTRY_DEFAULT_CAPABILITY_ID")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            default_account_id: env::var("CONSUMER_ENTRY_DEFAULT_ACCOUNT_ID")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            ingress_token: env::var("CONSUMER_ENTRY_INGRESS_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            require_session_auth: boolean_env("CONSUMER_ENTRY_REQUIRE_SESSION_AUTH", false),
            session_auth_secret: env::var("CONSUMER_ENTRY_SESSION_AUTH_SECRET")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            session_auth_issuer_secrets: env::var(
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_SECRETS_JSON",
            )
            .ok()
            .map(|value| parse_session_auth_issuer_secrets_json(&value))
            .unwrap_or_default(),
            session_auth_issuer_keys: env::var(
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON",
            )
            .ok()
            .map(|value| parse_session_auth_issuer_keys_json(&value))
            .unwrap_or_default(),
            session_auth_issuer_registry_path,
            session_auth_issuer_registry,
            session_auth_issuer_registry_load_error,
            session_auth_issuer_registry_metadata,
            session_auth_allowed_issuers: env::var("CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS")
                .ok()
                .map(|value| parse_csv_list(&value))
                .unwrap_or_default(),
            session_auth_expected_audience: env::var(
                "CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE",
            )
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
            session_auth_issuer_registry_approved_revisions_path: first_present_env(&[
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH",
                "CEX_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH",
            ]),
            session_auth_issuer_registry_require_approved_revision: boolean_env(
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION",
                false,
            ),
            session_auth_issuer_registry_require_actor: boolean_env(
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_ACTOR",
                false,
            ),
            session_auth_issuer_registry_actor_header: env::var(
                "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_ACTOR_HEADER",
            )
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "x-session-auth-issuer-registry-actor".to_string()),
            session_auth_issuer_registry_allowed_actors: parse_csv_list(
                &env::var("CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_ALLOWED_ACTORS")
                    .ok()
                    .unwrap_or_default(),
            ),
            session_auth_max_clock_skew_secs: positive_u64_env(
                "CONSUMER_ENTRY_SESSION_AUTH_MAX_CLOCK_SKEW_SECS",
                DEFAULT_SESSION_AUTH_MAX_CLOCK_SKEW_SECS,
            ),
            session_auth_max_ttl_secs: positive_u64_env(
                "CONSUMER_ENTRY_SESSION_AUTH_MAX_TTL_SECS",
                DEFAULT_SESSION_AUTH_MAX_TTL_SECS,
            ),
            identity_bindings_path: env::var("CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            identity_registry_path: env::var("CONSUMER_ENTRY_IDENTITY_REGISTRY_PATH")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            identity_binding_audit_log_path: env::var(
                "CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH",
            )
            .ok()
            .filter(|v| !v.trim().is_empty()),
            identity_binding_approved_revisions_path: env::var(
                "CONSUMER_ENTRY_IDENTITY_BINDING_APPROVED_REVISIONS_PATH",
            )
            .ok()
            .filter(|v| !v.trim().is_empty()),
            identity_binding_reload_require_revision: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_REVISION",
                false,
            ),
            identity_binding_reload_reject_same_revision: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REJECT_SAME_REVISION",
                false,
            ),
            identity_binding_reload_allow_legacy_format: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOW_LEGACY_FORMAT",
                true,
            ),
            identity_binding_reload_require_approved_revision: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_APPROVED_REVISION",
                false,
            ),
            identity_binding_reload_allow_rollback: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOW_ROLLBACK",
                true,
            ),
            identity_binding_reload_require_actor: boolean_env(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR",
                false,
            ),
            identity_binding_reload_actor_header: env::var(
                "CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ACTOR_HEADER",
            )
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "x-identity-binding-actor".to_string()),
            identity_binding_reload_allowed_actors: parse_csv_list(
                &env::var("CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS")
                    .ok()
                    .unwrap_or_default(),
            ),
            require_identity_binding: boolean_env("CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING", false),
            max_text_chars: positive_usize_env(
                "CONSUMER_ENTRY_MAX_TEXT_CHARS",
                DEFAULT_MAX_TEXT_CHARS,
            ),
            rate_limit_window_secs: positive_u64_env(
                "CONSUMER_ENTRY_RATE_LIMIT_WINDOW_SECS",
                DEFAULT_RATE_LIMIT_WINDOW_SECS,
            ),
            rate_limit_max_requests: positive_usize_env(
                "CONSUMER_ENTRY_RATE_LIMIT_MAX_REQUESTS",
                DEFAULT_RATE_LIMIT_MAX_REQUESTS,
            ),
            rate_limit_user_max_requests: non_negative_usize_env(
                "CONSUMER_ENTRY_RATE_LIMIT_USER_MAX_REQUESTS",
                0,
            ),
            rate_limit_room_max_requests: non_negative_usize_env(
                "CONSUMER_ENTRY_RATE_LIMIT_ROOM_MAX_REQUESTS",
                0,
            ),
            rate_limit_session_max_requests: non_negative_usize_env(
                "CONSUMER_ENTRY_RATE_LIMIT_SESSION_MAX_REQUESTS",
                0,
            ),
            rate_limit_org_max_requests: non_negative_usize_env(
                "CONSUMER_ENTRY_RATE_LIMIT_ORG_MAX_REQUESTS",
                0,
            ),
            rate_limit_store_path: first_present_env(&["CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH"]),
            replay_window_secs: positive_u64_env_alias(
                &[
                    "CONSUMER_ENTRY_REPLAY_WINDOW_SECS",
                    "CONSUMER_ENTRY_MATRIX_EVENT_WINDOW_SECS",
                ],
                DEFAULT_REPLAY_WINDOW_SECS,
            ),
            replay_cache_size: positive_usize_env_alias(
                &[
                    "CONSUMER_ENTRY_REPLAY_CACHE_SIZE",
                    "CONSUMER_ENTRY_MATRIX_EVENT_CACHE_SIZE",
                ],
                DEFAULT_REPLAY_CACHE_SIZE,
            ),
            replay_store_path: first_present_env(&[
                "CONSUMER_ENTRY_REPLAY_STORE_PATH",
                "CONSUMER_ENTRY_MATRIX_EVENT_STORE_PATH",
            ]),
        }
    }

    fn profile_validation_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if matches!(
            self.runtime_profile,
            RuntimeProfile::Beta | RuntimeProfile::Production
        ) {
            if self.ingress_token.is_none() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_INGRESS_TOKEN".to_string(),
                );
            }
            if self.replay_store_path.is_none() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_REPLAY_STORE_PATH".to_string(),
                );
            }
            if !self.require_session_auth {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_REQUIRE_SESSION_AUTH=true"
                        .to_string(),
                );
            }
            if let Some(error) = self.session_auth_issuer_registry_load_error.as_deref() {
                errors.push(format!(
                    "failed to load CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH: {error}"
                ));
            } else if self.session_auth_issuer_registry_path.is_some()
                && self.session_auth_issuer_registry.is_empty()
            {
                errors.push(
                    "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH is configured but contains no issuers"
                        .to_string(),
                );
            }
            if self.session_auth_secret.is_none()
                && self.session_auth_issuer_secrets.is_empty()
                && self.session_auth_issuer_keys.is_empty()
                && self.session_auth_issuer_registry.is_empty()
            {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_SESSION_AUTH_SECRET or CONSUMER_ENTRY_SESSION_AUTH_ISSUER_SECRETS_JSON or CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON or CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH"
                        .to_string(),
                );
            }
            if self.session_auth_allowed_issuers.is_empty() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS"
                        .to_string(),
                );
            }
            if self.session_auth_expected_audience.is_none() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE"
                        .to_string(),
                );
            }
            if self.session_auth_issuer_registry_require_approved_revision {
                let approval_state =
                    load_session_auth_issuer_registry_revision_approval_state(self);
                let approval_checks = session_auth_issuer_registry_approval_checks_json(
                    self,
                    &self.session_auth_issuer_registry_metadata,
                    &approval_state,
                );
                let approval_status = approval_checks
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("approval_state_not_loaded");
                if self
                    .session_auth_issuer_registry_approved_revisions_path
                    .is_none()
                {
                    errors.push(
                        "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION=true requires CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_APPROVED_REVISIONS_PATH"
                            .to_string(),
                    );
                } else if approval_status != "ok" {
                    errors.push(format!(
                        "session auth issuer registry approval invalid: {approval_status}"
                    ));
                }
            }
            if self.session_auth_issuer_registry_require_actor {
                let actor_checks = session_auth_issuer_registry_actor_checks_json(self);
                let actor_status = actor_checks
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("actor_header_missing");
                if actor_status != "ok" {
                    errors.push(format!(
                        "session auth issuer registry actor gate invalid: {actor_status}"
                    ));
                }
            }
            if self.rate_limit_store_path.is_none() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH"
                        .to_string(),
                );
            }
            if self.identity_bindings_path.is_none() {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH"
                        .to_string(),
                );
            }
            if !self.require_identity_binding {
                errors.push(
                    "beta/production profile requires CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING=true"
                        .to_string(),
                );
            }
            if self.cex_gateway_api_key.trim() == "local-dev-key" {
                errors.push(
                    "beta/production profile requires non-default CEX_GATEWAY_API_KEY".to_string(),
                );
            }
        }

        if self.runtime_profile == RuntimeProfile::Production {
            if self.identity_binding_audit_log_path.is_none() {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_AUDIT_LOG_PATH"
                        .to_string(),
                );
            }
            if self.identity_binding_approved_revisions_path.is_none() {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_APPROVED_REVISIONS_PATH"
                        .to_string(),
                );
            }
            if !self.identity_binding_reload_require_revision {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_REVISION=true"
                        .to_string(),
                );
            }
            if !self.identity_binding_reload_require_approved_revision {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_APPROVED_REVISION=true"
                        .to_string(),
                );
            }
            if !self.identity_binding_reload_require_actor {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_REQUIRE_ACTOR=true"
                        .to_string(),
                );
            }
            if self.identity_binding_reload_allowed_actors.is_empty() {
                errors.push(
                    "production profile requires CONSUMER_ENTRY_IDENTITY_BINDING_RELOAD_ALLOWED_ACTORS"
                        .to_string(),
                );
            }
            if self.session_auth_issuer_registry_path.is_some() {
                if !self.session_auth_issuer_registry_require_approved_revision {
                    errors.push(
                        "production profile requires CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_APPROVED_REVISION=true when CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH is configured"
                            .to_string(),
                    );
                }
                if !self.session_auth_issuer_registry_require_actor {
                    errors.push(
                        "production profile requires CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_REQUIRE_ACTOR=true when CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH is configured"
                            .to_string(),
                    );
                }
                if self.session_auth_issuer_registry_allowed_actors.is_empty() {
                    errors.push(
                        "production profile requires CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_ALLOWED_ACTORS when CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH is configured"
                            .to_string(),
                    );
                }
            }
        }

        errors
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

fn positive_usize_env(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn boolean_env(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .and_then(|v| match v.as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

fn positive_u64_env(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn non_negative_usize_env(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn first_present_env(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        env::var(name)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn positive_usize_env_alias(names: &[&str], default: usize) -> usize {
    names
        .iter()
        .find_map(|name| {
            env::var(name)
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
        })
        .unwrap_or(default)
}

fn parse_csv_list(value: &str) -> Vec<String> {
    let mut values = Vec::new();
    for item in value.split(',') {
        let normalized = item.trim();
        if normalized.is_empty() {
            continue;
        }
        if values.iter().any(|existing| existing == normalized) {
            continue;
        }
        values.push(normalized.to_string());
    }
    values
}

fn parse_session_auth_issuer_secrets_json(value: &str) -> HashMap<String, String> {
    let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(value) else {
        return HashMap::new();
    };

    let mut normalized = HashMap::new();
    for (issuer, secret) in parsed {
        let issuer = issuer.trim();
        let secret = secret.trim();
        if issuer.is_empty() || secret.is_empty() {
            continue;
        }
        normalized.insert(issuer.to_string(), secret.to_string());
    }
    normalized
}

fn parse_session_auth_issuer_keys_json(value: &str) -> HashMap<String, HashMap<String, String>> {
    let Ok(parsed) = serde_json::from_str::<HashMap<String, HashMap<String, String>>>(value)
    else {
        return HashMap::new();
    };

    let mut normalized = HashMap::new();
    for (issuer, keys) in parsed {
        let issuer = issuer.trim();
        if issuer.is_empty() {
            continue;
        }
        let mut normalized_keys = HashMap::new();
        for (key_id, secret) in keys {
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
        normalized.insert(issuer.to_string(), normalized_keys);
    }
    normalized
}

fn positive_u64_env_alias(names: &[&str], default: u64) -> u64 {
    names
        .iter()
        .find_map(|name| {
            env::var(name)
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
        })
        .unwrap_or(default)
}

fn default_identity_bindings_version() -> u32 {
    1
}

fn default_identity_binding_approval_version() -> u32 {
    1
}

fn default_session_auth_version() -> u32 {
    1
}

fn default_session_auth_issuer_registry_version() -> u32 {
    1
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

fn system_time_to_epoch(time: std::time::SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
}

fn load_product_user_registry(
    path: Option<&str>,
    fallback_users: HashMap<String, ProductUserIdentity>,
    fallback_metadata: &IdentityBindingMetadata,
) -> (
    IdentityBindingMetadata,
    HashMap<String, ProductUserIdentity>,
) {
    let Some(path) = path else {
        if fallback_users.is_empty() {
            return (IdentityBindingMetadata::default(), HashMap::new());
        }
        return (
            IdentityBindingMetadata {
                format: "embedded-in-binding".to_string(),
                version: fallback_metadata.version,
                revision: fallback_metadata.revision.clone(),
                source_path: fallback_metadata.source_path.clone(),
                source_modified_epoch: fallback_metadata.source_modified_epoch,
                loaded_at_epoch: fallback_metadata.loaded_at_epoch,
                load_status: fallback_metadata.load_status.clone(),
                load_error: fallback_metadata.load_error.clone(),
            },
            fallback_users,
        );
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
            return (
                IdentityBindingMetadata {
                    format: "separate-registry".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "read_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                HashMap::new(),
            )
        }
    };

    let value = match serde_json::from_str::<Value>(&raw) {
        Ok(value) => value,
        Err(err) => {
            return (
                IdentityBindingMetadata {
                    format: "separate-registry".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                HashMap::new(),
            )
        }
    };

    let Some(object) = value.as_object() else {
        return (
            IdentityBindingMetadata {
                format: "separate-registry".to_string(),
                version: 0,
                revision: None,
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "invalid_shape".to_string(),
                load_error: Some("identity registry document must be a JSON object".to_string()),
            },
            HashMap::new(),
        );
    };

    if object.contains_key("version")
        || object.contains_key("revision")
        || object.contains_key("product_users")
    {
        return match serde_json::from_value::<ProductUserRegistryDocument>(value.clone()) {
            Ok(document) => (
                IdentityBindingMetadata {
                    format: "separate-registry-document".to_string(),
                    version: document.version,
                    revision: document.revision,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "loaded".to_string(),
                    load_error: None,
                },
                document.product_users,
            ),
            Err(err) => (
                IdentityBindingMetadata {
                    format: "separate-registry-document".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                HashMap::new(),
            ),
        };
    }

    match serde_json::from_value::<HashMap<String, ProductUserIdentity>>(value) {
        Ok(product_users) => (
            IdentityBindingMetadata {
                format: "separate-registry-flat-map".to_string(),
                version: 1,
                revision: None,
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "loaded".to_string(),
                load_error: None,
            },
            product_users,
        ),
        Err(err) => (
            IdentityBindingMetadata {
                format: "separate-registry-flat-map".to_string(),
                version: 1,
                revision: None,
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "parse_error".to_string(),
                load_error: Some(err.to_string()),
            },
            HashMap::new(),
        ),
    }
}

fn load_identity_binding_store(config: &ConsumerEntryConfig) -> IdentityBindingStore {
    let Some(path) = config.identity_bindings_path.as_deref() else {
        return IdentityBindingStore::default();
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
            return IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "none".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "read_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings: IdentityBindings::default(),
                product_users: HashMap::new(),
            }
        }
    };

    let value = match serde_json::from_str::<Value>(&raw) {
        Ok(value) => value,
        Err(err) => {
            return IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "none".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings: IdentityBindings::default(),
                product_users: HashMap::new(),
            }
        }
    };

    let Some(object) = value.as_object() else {
        return IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "none".to_string(),
                version: 0,
                revision: None,
                source_path,
                source_modified_epoch,
                loaded_at_epoch,
                load_status: "invalid_shape".to_string(),
                load_error: Some("identity binding document must be a JSON object".to_string()),
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
    };

    let store = if object.contains_key("version") || object.contains_key("revision") {
        match serde_json::from_value::<IdentityBindingsDocument>(value.clone()) {
            Ok(document) => IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "versioned-document".to_string(),
                    version: document.version,
                    revision: document.revision,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "loaded".to_string(),
                    load_error: None,
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings: IdentityBindings {
                    chat_users: document.chat_users,
                    matrix_users: document.matrix_users,
                },
                product_users: document.product_users,
            },
            Err(err) => IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "versioned-document".to_string(),
                    version: 0,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings: IdentityBindings::default(),
                product_users: HashMap::new(),
            },
        }
    } else {
        match serde_json::from_value::<IdentityBindings>(value) {
            Ok(bindings) => IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "legacy-flat-map".to_string(),
                    version: 1,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "loaded".to_string(),
                    load_error: None,
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings,
                product_users: HashMap::new(),
            },
            Err(err) => IdentityBindingStore {
                metadata: IdentityBindingMetadata {
                    format: "legacy-flat-map".to_string(),
                    version: 1,
                    revision: None,
                    source_path,
                    source_modified_epoch,
                    loaded_at_epoch,
                    load_status: "parse_error".to_string(),
                    load_error: Some(err.to_string()),
                },
                registry_metadata: IdentityBindingMetadata::default(),
                bindings: IdentityBindings::default(),
                product_users: HashMap::new(),
            },
        }
    };

    let (registry_metadata, product_users) = load_product_user_registry(
        config.identity_registry_path.as_deref(),
        store.product_users.clone(),
        &store.metadata,
    );

    IdentityBindingStore {
        registry_metadata,
        product_users,
        ..store
    }
}

fn count_product_user_refs(bindings: &HashMap<String, IdentityBindingEntry>) -> usize {
    bindings
        .values()
        .filter(|binding| {
            binding
                .product_user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
        })
        .count()
}

fn count_inline_identity_bindings(bindings: &HashMap<String, IdentityBindingEntry>) -> usize {
    bindings
        .values()
        .filter(|binding| {
            binding
                .product_user_id
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
                && (binding.org_id.is_some() || binding.account_id.is_some())
        })
        .count()
}

fn count_missing_product_user_refs(store: &IdentityBindingStore) -> usize {
    store
        .bindings
        .chat_users
        .values()
        .chain(store.bindings.matrix_users.values())
        .filter(|binding| {
            let Some(product_user_id) = binding
                .product_user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return false;
            };
            !store.product_users.contains_key(product_user_id)
        })
        .count()
}

fn identity_source_of_truth_mode(store: &IdentityBindingStore) -> &'static str {
    let registry_refs = count_product_user_refs(&store.bindings.chat_users)
        + count_product_user_refs(&store.bindings.matrix_users);
    let inline_only = count_inline_identity_bindings(&store.bindings.chat_users)
        + count_inline_identity_bindings(&store.bindings.matrix_users);

    match (registry_refs > 0, inline_only > 0) {
        (true, true) => "mixed",
        (true, false) => "product_user_registry",
        _ => "inline_bindings",
    }
}

fn identity_binding_counts_json(store: &IdentityBindingStore) -> Value {
    json!({
        "chat_users": store.bindings.chat_users.len(),
        "matrix_users": store.bindings.matrix_users.len(),
        "product_users": store.product_users.len(),
        "chat_product_user_refs": count_product_user_refs(&store.bindings.chat_users),
        "matrix_product_user_refs": count_product_user_refs(&store.bindings.matrix_users),
        "inline_chat_users": count_inline_identity_bindings(&store.bindings.chat_users),
        "inline_matrix_users": count_inline_identity_bindings(&store.bindings.matrix_users),
        "missing_product_user_refs": count_missing_product_user_refs(store),
        "source_of_truth_mode": identity_source_of_truth_mode(store),
    })
}

fn metadata_revision(metadata: &IdentityBindingMetadata) -> Option<String> {
    metadata
        .revision
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn effective_identity_revision(
    binding_metadata: &IdentityBindingMetadata,
    registry_metadata: &IdentityBindingMetadata,
    separate_registry_configured: bool,
) -> Option<String> {
    let binding_revision = metadata_revision(binding_metadata);
    if !separate_registry_configured {
        return binding_revision;
    }
    let registry_revision = metadata_revision(registry_metadata);
    match (binding_revision, registry_revision) {
        (Some(binding_revision), Some(registry_revision)) => Some(format!(
            "binding:{binding_revision}|registry:{registry_revision}"
        )),
        _ => None,
    }
}

fn normalize_revision_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if normalized.iter().any(|existing| existing == trimmed) {
            continue;
        }
        normalized.push(trimmed.to_string());
    }
    normalized
}

fn load_identity_binding_revision_approval_state(
    config: &ConsumerEntryConfig,
) -> IdentityBindingRevisionApprovalState {
    let Some(path) = config.identity_binding_approved_revisions_path.as_deref() else {
        return IdentityBindingRevisionApprovalState::default();
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
            return IdentityBindingRevisionApprovalState {
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

    let document = match serde_json::from_str::<IdentityBindingRevisionApprovalDocument>(&raw) {
        Ok(document) => document,
        Err(err) => {
            return IdentityBindingRevisionApprovalState {
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

    IdentityBindingRevisionApprovalState {
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

fn load_session_auth_issuer_registry_revision_approval_state(
    config: &ConsumerEntryConfig,
) -> SessionAuthIssuerRegistryRevisionApprovalState {
    let Some(path) = config
        .session_auth_issuer_registry_approved_revisions_path
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

fn evaluate_identity_binding_reload_governance(
    config: &ConsumerEntryConfig,
    current: &IdentityBindingStore,
    candidate: &IdentityBindingStore,
    approval_state: &IdentityBindingRevisionApprovalState,
    requesting_actor: Option<&str>,
) -> IdentityBindingReloadGovernance {
    let separate_registry_configured = config.identity_registry_path.is_some();
    let candidate_revision = metadata_revision(&candidate.metadata);
    let current_revision = metadata_revision(&current.metadata);
    let candidate_registry_revision = metadata_revision(&candidate.registry_metadata);
    let current_registry_revision = metadata_revision(&current.registry_metadata);
    let candidate_effective_revision = effective_identity_revision(
        &candidate.metadata,
        &candidate.registry_metadata,
        separate_registry_configured,
    );
    let current_effective_revision = effective_identity_revision(
        &current.metadata,
        &current.registry_metadata,
        separate_registry_configured,
    );
    let current_missing_product_user_refs = count_missing_product_user_refs(current);
    let candidate_missing_product_user_refs = count_missing_product_user_refs(candidate);
    let candidate_revision_approved = candidate_effective_revision.as_ref().map(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .any(|approved| approved == revision)
    });
    let current_revision_index = current_effective_revision.as_ref().and_then(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .position(|approved| approved == revision)
    });
    let candidate_revision_index = candidate_effective_revision.as_ref().and_then(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .position(|approved| approved == revision)
    });

    let (actor_authorized, actor_reason) = if config.identity_binding_reload_require_actor {
        if config.identity_binding_reload_allowed_actors.is_empty() {
            (
                Some(false),
                Some("no_allowed_actors_configured".to_string()),
            )
        } else {
            match requesting_actor {
                Some(actor) if !actor.trim().is_empty() => {
                    let actor_is_allowed = config
                        .identity_binding_reload_allowed_actors
                        .iter()
                        .any(|allowed| allowed == actor.trim());
                    if actor_is_allowed {
                        (Some(true), None)
                    } else {
                        (Some(false), Some("actor_not_allowed".to_string()))
                    }
                }
                _ => (Some(false), Some("actor_missing".to_string())),
            }
        }
    } else {
        (None, None)
    };

    let decision = if actor_authorized == Some(false) {
        (false, "rejected", "actor_not_authorized", true)
    } else if candidate.metadata.load_status != "loaded" {
        (false, "rejected", "candidate_not_loaded", false)
    } else if separate_registry_configured && candidate.registry_metadata.load_status != "loaded" {
        (false, "rejected", "candidate_registry_not_loaded", false)
    } else if !config.identity_binding_reload_allow_legacy_format
        && candidate.metadata.format != "versioned-document"
    {
        (false, "rejected", "legacy_format_not_allowed", false)
    } else if candidate_missing_product_user_refs > 0 {
        (false, "rejected", "missing_product_user_refs", false)
    } else if config.identity_binding_reload_require_revision
        && candidate_effective_revision.is_none()
    {
        (
            false,
            "rejected",
            if separate_registry_configured {
                "effective_revision_required"
            } else {
                "revision_required"
            },
            false,
        )
    } else if config.identity_binding_reload_reject_same_revision
        && current_effective_revision.is_some()
        && current_effective_revision == candidate_effective_revision
    {
        (false, "rejected", "same_revision_rejected", false)
    } else if config.identity_binding_reload_require_approved_revision
        && approval_state.load_status != "loaded"
    {
        (false, "rejected", "approval_state_not_loaded", false)
    } else if config.identity_binding_reload_require_approved_revision
        && candidate_revision_approved != Some(true)
    {
        (false, "rejected", "candidate_revision_not_approved", false)
    } else if !config.identity_binding_reload_allow_rollback
        && current_effective_revision.is_some()
        && candidate_effective_revision.is_some()
        && approval_state.load_status != "loaded"
    {
        (
            false,
            "rejected",
            "rollback_policy_requires_approval_state",
            true,
        )
    } else if !config.identity_binding_reload_allow_rollback
        && current_effective_revision.is_some()
        && candidate_effective_revision.is_some()
        && (current_revision_index.is_none() || candidate_revision_index.is_none())
    {
        (false, "rejected", "rollback_order_unavailable", true)
    } else if !config.identity_binding_reload_allow_rollback
        && current_revision_index
            .zip(candidate_revision_index)
            .is_some_and(|(current_idx, candidate_idx)| candidate_idx < current_idx)
    {
        (false, "rejected", "rollback_revision_rejected", true)
    } else {
        (true, "accepted", "policy_ok", false)
    };

    IdentityBindingReloadGovernance {
        accepted: decision.0,
        decision: decision.1.to_string(),
        reason: decision.2.to_string(),
        current_format: current.metadata.format.clone(),
        current_revision,
        current_registry_revision,
        current_effective_revision,
        current_missing_product_user_refs,
        candidate_format: candidate.metadata.format.clone(),
        candidate_revision,
        candidate_registry_revision,
        candidate_effective_revision,
        candidate_load_status: candidate.metadata.load_status.clone(),
        candidate_registry_load_status: candidate.registry_metadata.load_status.clone(),
        candidate_missing_product_user_refs,
        separate_registry_configured,
        approval_state_status: approval_state.load_status.clone(),
        approval_state_revision: approval_state.revision.clone(),
        approved_revision_count: approval_state.approved_revisions.len(),
        candidate_revision_approved,
        rollback_blocked: decision.3,
        requesting_actor: requesting_actor.map(str::to_string),
        actor_header: config.identity_binding_reload_actor_header.clone(),
        actor_authorized,
        actor_reason,
    }
}

fn load_replay_cache(config: &ConsumerEntryConfig) -> ReplayCache {
    let Some(path) = config.replay_store_path.as_deref() else {
        return ReplayCache::default();
    };

    let Ok(raw) = std::fs::read_to_string(path) else {
        return ReplayCache::default();
    };

    let Ok(mut cache) = serde_json::from_str::<ReplayCache>(&raw) else {
        return ReplayCache::default();
    };

    prune_replay_cache(
        &mut cache,
        Utc::now().timestamp(),
        config.replay_window_secs,
        config.replay_cache_size,
    );
    cache
}

fn persist_replay_cache(cache: &ReplayCache, config: &ConsumerEntryConfig) {
    let Some(path) = config.replay_store_path.as_deref() else {
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

fn max_rate_limit_entries(config: &ConsumerEntryConfig) -> usize {
    [
        config.rate_limit_max_requests,
        config.rate_limit_user_max_requests,
        config.rate_limit_room_max_requests,
        config.rate_limit_session_max_requests,
        config.rate_limit_org_max_requests,
    ]
    .into_iter()
    .max()
    .unwrap_or(DEFAULT_RATE_LIMIT_MAX_REQUESTS)
    .max(1)
}

fn prune_rate_limit_entries(
    entries: &mut VecDeque<i64>,
    now_epoch: i64,
    window_secs: u64,
    max_entries: usize,
) -> bool {
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
    while entries.len() > max_entries {
        entries.pop_front();
        modified = true;
    }
    modified
}

fn prune_rate_limit_cache(
    cache: &mut RateLimitCache,
    now_epoch: i64,
    window_secs: u64,
    max_entries: usize,
) {
    cache.seen.retain(|_, entries| {
        prune_rate_limit_entries(entries, now_epoch, window_secs, max_entries);
        !entries.is_empty()
    });
}

fn load_rate_limit_cache(config: &ConsumerEntryConfig) -> RateLimitCache {
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
        max_rate_limit_entries(config),
    );
    cache
}

fn persist_rate_limit_cache(cache: &RateLimitCache, config: &ConsumerEntryConfig) {
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

fn append_identity_binding_audit_event(
    config: &ConsumerEntryConfig,
    event_kind: &str,
    store: &IdentityBindingStore,
    governance: Option<&IdentityBindingReloadGovernance>,
) -> IdentityBindingAuditState {
    let Some(path) = config.identity_binding_audit_log_path.as_deref() else {
        return IdentityBindingAuditState::default();
    };

    let event_epoch = Utc::now().timestamp();
    let audit_event = json!({
        "event_kind": event_kind,
        "event_epoch": event_epoch,
        "identity_binding_metadata": {
            "format": store.metadata.format.clone(),
            "version": store.metadata.version,
            "revision": store.metadata.revision.clone(),
            "source_path": store.metadata.source_path.clone(),
            "source_modified_epoch": store.metadata.source_modified_epoch,
            "loaded_at_epoch": store.metadata.loaded_at_epoch,
            "load_status": store.metadata.load_status.clone(),
            "load_error": store.metadata.load_error.clone(),
        },
        "identity_binding_counts": identity_binding_counts_json(store),
        "identity_source_of_truth": {
            "mode": identity_source_of_truth_mode(store),
            "product_users": store.product_users.len(),
            "missing_product_user_refs": count_missing_product_user_refs(store),
        },
        "identity_registry_metadata": {
            "format": store.registry_metadata.format.clone(),
            "version": store.registry_metadata.version,
            "revision": store.registry_metadata.revision.clone(),
            "source_path": store.registry_metadata.source_path.clone(),
            "source_modified_epoch": store.registry_metadata.source_modified_epoch,
            "loaded_at_epoch": store.registry_metadata.loaded_at_epoch,
            "load_status": store.registry_metadata.load_status.clone(),
            "load_error": store.registry_metadata.load_error.clone(),
        },
        "governance": governance,
    });

    let parent = std::path::Path::new(path).parent().map(|p| p.to_path_buf());
    if let Some(parent) = parent {
        if let Err(err) = std::fs::create_dir_all(parent) {
            return IdentityBindingAuditState {
                path: Some(path.to_string()),
                last_event_kind: Some(event_kind.to_string()),
                last_event_epoch: Some(event_epoch),
                last_status: "write_error".to_string(),
                last_error: Some(err.to_string()),
                last_policy_decision: governance.map(|g| g.decision.clone()),
                last_policy_reason: governance.map(|g| g.reason.clone()),
            };
        }
    }

    let line = match serde_json::to_string(&audit_event) {
        Ok(line) => format!("{line}\n"),
        Err(err) => {
            return IdentityBindingAuditState {
                path: Some(path.to_string()),
                last_event_kind: Some(event_kind.to_string()),
                last_event_epoch: Some(event_epoch),
                last_status: "serialize_error".to_string(),
                last_error: Some(err.to_string()),
                last_policy_decision: governance.map(|g| g.decision.clone()),
                last_policy_reason: governance.map(|g| g.reason.clone()),
            }
        }
    };

    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(file) => file,
        Err(err) => {
            return IdentityBindingAuditState {
                path: Some(path.to_string()),
                last_event_kind: Some(event_kind.to_string()),
                last_event_epoch: Some(event_epoch),
                last_status: "write_error".to_string(),
                last_error: Some(err.to_string()),
                last_policy_decision: governance.map(|g| g.decision.clone()),
                last_policy_reason: governance.map(|g| g.reason.clone()),
            }
        }
    };

    match std::io::Write::write_all(&mut file, line.as_bytes()) {
        Ok(_) => IdentityBindingAuditState {
            path: Some(path.to_string()),
            last_event_kind: Some(event_kind.to_string()),
            last_event_epoch: Some(event_epoch),
            last_status: "written".to_string(),
            last_error: None,
            last_policy_decision: governance.map(|g| g.decision.clone()),
            last_policy_reason: governance.map(|g| g.reason.clone()),
        },
        Err(err) => IdentityBindingAuditState {
            path: Some(path.to_string()),
            last_event_kind: Some(event_kind.to_string()),
            last_event_epoch: Some(event_epoch),
            last_status: "write_error".to_string(),
            last_error: Some(err.to_string()),
            last_policy_decision: governance.map(|g| g.decision.clone()),
            last_policy_reason: governance.map(|g| g.reason.clone()),
        },
    }
}

impl AppState {
    pub async fn from_env() -> Result<Self, String> {
        let config = ConsumerEntryConfig::from_env();
        if let Err(errors) = config.validate_runtime_profile() {
            return Err(format!(
                "invalid consumer-entry-api runtime profile ({}): {}",
                config.runtime_profile.as_str(),
                errors.join("; ")
            ));
        }
        Ok(Self::new(config))
    }

    pub fn new(config: ConsumerEntryConfig) -> Self {
        let identity_binding_store = load_identity_binding_store(&config);
        let identity_binding_audit_state = append_identity_binding_audit_event(
            &config,
            "startup_load",
            &identity_binding_store,
            None,
        );
        let session_auth_issuer_registry_state = SessionAuthIssuerRegistryRuntimeState {
            metadata: config.session_auth_issuer_registry_metadata.clone(),
            registry: config.session_auth_issuer_registry.clone(),
        };
        let rate_limits = load_rate_limit_cache(&config);
        let replay_cache = load_replay_cache(&config);
        let metrics = ConsumerEntryMetrics::default();
        if identity_binding_audit_state.last_status != "written"
            && config.identity_binding_audit_log_path.is_some()
        {
            metrics.inc_identity_binding_audit_failures();
        }
        Self {
            inner: Arc::new(AppStateInner {
                http: Client::new(),
                config,
                identity_binding_store: RwLock::new(identity_binding_store),
                identity_binding_audit_state: RwLock::new(identity_binding_audit_state),
                session_auth_issuer_registry_state: StdRwLock::new(session_auth_issuer_registry_state),
                rate_limits: Mutex::new(rate_limits),
                replay_cache: Mutex::new(replay_cache),
                metrics,
            }),
        }
    }

    pub fn config(&self) -> &ConsumerEntryConfig {
        &self.inner.config
    }
}

fn session_auth_issuer_registry_runtime_state(
    state: &AppState,
) -> SessionAuthIssuerRegistryRuntimeState {
    state
        .inner
        .session_auth_issuer_registry_state
        .read()
        .expect("session auth issuer registry state lock poisoned")
        .clone()
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/v1/chat/tasks", post(create_chat_task))
        .route("/v1/chat/tasks/:id", get(get_chat_task))
        .route("/v1/matrix/messages", post(create_matrix_message_task))
        .route(
            "/v1/admin/identity-bindings/reload",
            post(reload_identity_bindings),
        )
        .route(
            "/v1/admin/identity-registry/reload",
            post(reload_identity_registry),
        )
        .route(
            "/v1/admin/identity-registry/validate",
            post(validate_identity_registry),
        )
        .route(
            "/v1/admin/identity-registry/status",
            get(get_identity_registry_status),
        )
        .route(
            "/v1/admin/identity-registry/audit",
            get(get_identity_registry_audit),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/reload",
            post(reload_session_auth_issuer_registry),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/status",
            get(get_session_auth_issuer_registry_status),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/validate",
            post(validate_session_auth_issuer_registry),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/approval/status",
            get(get_session_auth_issuer_registry_approval_status),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/approval/validate",
            post(validate_session_auth_issuer_registry_approval),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/actors/status",
            get(get_session_auth_issuer_registry_actor_status),
        )
        .route(
            "/v1/admin/session-auth/issuer-registry/actors/validate",
            post(validate_session_auth_issuer_registry_actors),
        )
        .route(
            "/v1/admin/identity-approval/status",
            get(get_identity_approval_status),
        )
        .route(
            "/v1/admin/identity-approval/validate",
            post(validate_identity_approval),
        )
        .route(
            "/v1/admin/identity-approval/source",
            get(get_identity_approval_source),
        )
        .route(
            "/v1/admin/identity-approval/source/validate",
            post(validate_identity_approval_source),
        )
        .route(
            "/v1/admin/identity-governance/status",
            get(get_identity_governance_status),
        )
        .route(
            "/v1/admin/identity-governance/validate",
            post(validate_identity_governance),
        )
        .route(
            "/v1/admin/identity-actors/status",
            get(get_identity_actor_status),
        )
        .route(
            "/v1/admin/identity-actors/validate",
            post(validate_identity_actors),
        )
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    let store = state.inner.identity_binding_store.read().await;
    let audit = state.inner.identity_binding_audit_state.read().await;
    let rate_limits = state.inner.rate_limits.lock().await;
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let session_auth_registry_state = session_auth_issuer_registry_runtime_state(&state);
    let session_auth_registry_approval_state =
        load_session_auth_issuer_registry_revision_approval_state(state.config());
    let session_auth_registry_actor_checks =
        session_auth_issuer_registry_actor_checks_json(state.config());
    let session_auth_registry_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &session_auth_registry_state.metadata,
        &session_auth_registry_approval_state,
    );
    let session_auth_registry_governance_overview =
        session_auth_issuer_registry_governance_overview_json(
            state.config(),
            &session_auth_registry_state.metadata,
            &session_auth_registry_approval_state,
            5,
        );
    let profile_errors = state.config().profile_validation_errors();
    let identity_governance_overview =
        identity_governance_overview_json(state.config(), &store, &approval_state, &audit, 5);
    let identity_governance_valid = identity_governance_overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let session_auth_registry_governance_valid = session_auth_registry_governance_overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Json(json!({
        "status": "ok",
        "service": "consumer-entry-api",
        "runtime_profile": state.config().runtime_profile.as_str(),
        "profile_validation": {
            "ok": profile_errors.is_empty(),
            "errors": profile_errors,
            "checks": {
                "ingress_token_present": state.config().ingress_token.is_some(),
                "require_session_auth": state.config().require_session_auth,
                "session_auth_secret_present": state.config().session_auth_secret.is_some(),
                "session_auth_issuer_secret_count": state.config().session_auth_issuer_secrets.len(),
                "session_auth_issuer_key_issuer_count": state.config().session_auth_issuer_keys.len(),
                "session_auth_issuer_key_count": state.config().session_auth_issuer_keys.values().map(|keys| keys.len()).sum::<usize>(),
                "session_auth_issuer_registry_configured": state.config().session_auth_issuer_registry_path.is_some(),
                "session_auth_issuer_registry_loaded": session_auth_registry_state.metadata.load_status == "loaded",
                "session_auth_issuer_registry_revision_present": session_auth_registry_state.metadata.revision.is_some(),
                "session_auth_issuer_registry_issuer_count": session_auth_registry_state.metadata.issuer_count,
                "session_auth_issuer_registry_key_count": session_auth_registry_state.metadata.key_count,
                "session_auth_issuer_registry_approved_revisions_configured": state.config().session_auth_issuer_registry_approved_revisions_path.is_some(),
                "session_auth_issuer_registry_require_approved_revision": state.config().session_auth_issuer_registry_require_approved_revision,
                "session_auth_issuer_registry_require_actor": state.config().session_auth_issuer_registry_require_actor,
                "session_auth_issuer_registry_actor_gate_valid": session_auth_registry_actor_checks.get("valid").and_then(Value::as_bool),
                "session_auth_issuer_registry_approval_loaded": session_auth_registry_approval_state.load_status == "loaded",
                "session_auth_issuer_registry_revision_approved": session_auth_registry_approval_checks.get("current_revision_approved").and_then(Value::as_bool),
                "session_auth_issuer_registry_governance_valid": session_auth_registry_governance_valid,
                "session_auth_allowed_issuer_count": state.config().session_auth_allowed_issuers.len(),
                "session_auth_expected_audience_configured": state.config().session_auth_expected_audience.is_some(),
                "replay_store_configured": state.config().replay_store_path.is_some(),
                "rate_limit_store_configured": state.config().rate_limit_store_path.is_some(),
                "identity_bindings_configured": state.config().identity_bindings_path.is_some(),
                "identity_registry_configured": state.config().identity_registry_path.is_some(),
                "require_identity_binding": state.config().require_identity_binding,
                "gateway_api_key_is_non_default": state.config().cex_gateway_api_key.trim() != "local-dev-key",
                "identity_binding_audit_log_configured": state.config().identity_binding_audit_log_path.is_some(),
                "identity_binding_approved_revisions_configured": state.config().identity_binding_approved_revisions_path.is_some(),
                "reload_requires_revision": state.config().identity_binding_reload_require_revision,
                "reload_requires_approved_revision": state.config().identity_binding_reload_require_approved_revision,
                "reload_requires_actor": state.config().identity_binding_reload_require_actor,
                "reload_allowed_actor_count": state.config().identity_binding_reload_allowed_actors.len(),
                "identity_registry_users": store.product_users.len(),
                "identity_registry_missing_refs": count_missing_product_user_refs(&store),
                "identity_governance_valid": identity_governance_valid,
            }
        },
        "cex_gateway_base_url": state.config().cex_gateway_base_url,
        "default_capability_id": state.config().default_capability_id,
        "ingress_protected": state.config().ingress_token.is_some(),
        "require_session_auth": state.config().require_session_auth,
        "session_auth": {
            "enabled": state.config().require_session_auth,
            "secret_present": state.config().session_auth_secret.is_some(),
            "issuer_secret_count": state.config().session_auth_issuer_secrets.len(),
            "issuer_secret_issuers": state.config().session_auth_issuer_secrets.keys().cloned().collect::<Vec<_>>(),
            "issuer_key_issuer_count": state.config().session_auth_issuer_keys.len(),
            "issuer_key_count": state.config().session_auth_issuer_keys.values().map(|keys| keys.len()).sum::<usize>(),
            "issuer_key_issuers": state.config().session_auth_issuer_keys.keys().cloned().collect::<Vec<_>>(),
            "issuer_registry_path": state.config().session_auth_issuer_registry_path,
            "issuer_registry_loaded": session_auth_registry_state.metadata.load_status == "loaded",
            "issuer_registry_load_error": session_auth_registry_state.metadata.load_error.clone(),
            "issuer_registry_issuer_count": session_auth_registry_state.metadata.issuer_count,
            "issuer_registry_key_count": session_auth_registry_state.metadata.key_count,
            "issuer_registry_issuers": session_auth_registry_state.registry.keys().cloned().collect::<Vec<_>>(),
            "issuer_registry_metadata": session_auth_registry_state.metadata.clone(),
            "issuer_registry_approved_revisions_path": state
                .config()
                .session_auth_issuer_registry_approved_revisions_path,
            "issuer_registry_approval_required": state
                .config()
                .session_auth_issuer_registry_require_approved_revision,
            "issuer_registry_actor_required": state
                .config()
                .session_auth_issuer_registry_require_actor,
            "issuer_registry_actor_checks": session_auth_registry_actor_checks,
            "issuer_registry_approval_state": session_auth_registry_approval_state,
            "issuer_registry_approval_checks": session_auth_registry_approval_checks,
            "allowed_issuers": state.config().session_auth_allowed_issuers,
            "allowed_issuer_count": state.config().session_auth_allowed_issuers.len(),
            "expected_audience": state.config().session_auth_expected_audience,
            "assertion_header": USER_SESSION_ASSERTION_HEADER,
            "signature_header": USER_SESSION_SIGNATURE_HEADER,
            "max_clock_skew_secs": state.config().session_auth_max_clock_skew_secs,
            "max_ttl_secs": state.config().session_auth_max_ttl_secs,
        },
        "identity_bindings_path": state.config().identity_bindings_path,
        "identity_bindings_enabled": state.config().identity_bindings_path.is_some(),
        "identity_registry_path": state.config().identity_registry_path,
        "identity_registry_enabled": state.config().identity_registry_path.is_some(),
        "require_identity_binding": state.config().require_identity_binding,
        "identity_binding_metadata": {
            "format": store.metadata.format.clone(),
            "version": store.metadata.version,
            "revision": store.metadata.revision.clone(),
            "source_path": store.metadata.source_path.clone(),
            "source_modified_epoch": store.metadata.source_modified_epoch,
            "loaded_at_epoch": store.metadata.loaded_at_epoch,
            "load_status": store.metadata.load_status.clone(),
            "load_error": store.metadata.load_error.clone(),
        },
        "identity_binding_counts": identity_binding_counts_json(&store),
        "identity_source_of_truth": {
            "mode": identity_source_of_truth_mode(&store),
            "product_users": store.product_users.len(),
            "missing_product_user_refs": count_missing_product_user_refs(&store),
        },
        "identity_registry_metadata": {
            "format": store.registry_metadata.format.clone(),
            "version": store.registry_metadata.version,
            "revision": store.registry_metadata.revision.clone(),
            "source_path": store.registry_metadata.source_path.clone(),
            "source_modified_epoch": store.registry_metadata.source_modified_epoch,
            "loaded_at_epoch": store.registry_metadata.loaded_at_epoch,
            "load_status": store.registry_metadata.load_status.clone(),
            "load_error": store.registry_metadata.load_error.clone(),
        },
        "identity_binding_audit": {
            "path": audit.path.clone(),
            "last_event_kind": audit.last_event_kind.clone(),
            "last_event_epoch": audit.last_event_epoch,
            "last_status": audit.last_status.clone(),
            "last_error": audit.last_error.clone(),
            "last_policy_decision": audit.last_policy_decision.clone(),
            "last_policy_reason": audit.last_policy_reason.clone(),
        },
        "identity_binding_reload_policy": {
            "require_revision": state.config().identity_binding_reload_require_revision,
            "reject_same_revision": state.config().identity_binding_reload_reject_same_revision,
            "allow_legacy_format": state.config().identity_binding_reload_allow_legacy_format,
            "require_approved_revision": state.config().identity_binding_reload_require_approved_revision,
            "allow_rollback": state.config().identity_binding_reload_allow_rollback,
            "require_actor": state.config().identity_binding_reload_require_actor,
            "actor_header": state.config().identity_binding_reload_actor_header,
            "allowed_actors_count": state.config().identity_binding_reload_allowed_actors.len(),
        },
        "identity_binding_revision_approval": {
            "source_path": approval_state.source_path,
            "source_modified_epoch": approval_state.source_modified_epoch,
            "loaded_at_epoch": approval_state.loaded_at_epoch,
            "load_status": approval_state.load_status,
            "load_error": approval_state.load_error,
            "version": approval_state.version,
            "revision": approval_state.revision,
            "approved_revisions": approval_state.approved_revisions,
        },
        "identity_governance_overview": identity_governance_overview,
        "session_auth_issuer_registry_governance_overview": session_auth_registry_governance_overview,
        "max_text_chars": state.config().max_text_chars,
        "rate_limit_window_secs": state.config().rate_limit_window_secs,
        "rate_limit_max_requests": state.config().rate_limit_max_requests,
        "rate_limit_user_max_requests": state.config().rate_limit_user_max_requests,
        "rate_limit_room_max_requests": state.config().rate_limit_room_max_requests,
        "rate_limit_session_max_requests": state.config().rate_limit_session_max_requests,
        "rate_limit_org_max_requests": state.config().rate_limit_org_max_requests,
        "rate_limit_store_path": state.config().rate_limit_store_path,
        "rate_limit_store_enabled": state.config().rate_limit_store_path.is_some(),
        "rate_limit_bucket_count": rate_limits.seen.len(),
        "replay_window_secs": state.config().replay_window_secs,
        "replay_cache_size": state.config().replay_cache_size,
        "replay_store_path": state.config().replay_store_path,
        "replay_store_enabled": state.config().replay_store_path.is_some(),
        "metrics": state.inner.metrics.snapshot(),
    }))
}

async fn metrics(State(state): State<AppState>) -> Response {
    let profile_ok = if state.config().profile_validation_errors().is_empty() {
        1
    } else {
        0
    };
    let rate_limit_bucket_count = {
        let rate_limits = state.inner.rate_limits.lock().await;
        rate_limits.seen.len()
    };
    let identity_binding_store = state.inner.identity_binding_store.read().await;
    let audit = {
        let audit = state.inner.identity_binding_audit_state.read().await;
        audit.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let governance_overview = identity_governance_overview_json(
        state.config(),
        &identity_binding_store,
        &approval_state,
        &audit,
        5,
    );
    let product_user_count = identity_binding_store.product_users.len();
    let product_user_ref_count =
        count_product_user_refs(&identity_binding_store.bindings.chat_users)
            + count_product_user_refs(&identity_binding_store.bindings.matrix_users);
    let missing_product_user_ref_count = count_missing_product_user_refs(&identity_binding_store);
    let governance_valid = governance_overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let governance_checks = governance_overview
        .get("checks")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let session_auth_registry_state = session_auth_issuer_registry_runtime_state(&state);
    let session_auth_registry_approval_state =
        load_session_auth_issuer_registry_revision_approval_state(state.config());
    let session_auth_registry_actor_checks =
        session_auth_issuer_registry_actor_checks_json(state.config());
    let session_auth_registry_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &session_auth_registry_state.metadata,
        &session_auth_registry_approval_state,
    );
    let session_auth_registry_governance_overview =
        session_auth_issuer_registry_governance_overview_json(
            state.config(),
            &session_auth_registry_state.metadata,
            &session_auth_registry_approval_state,
            5,
        );
    let session_auth_registry_governance_checks = session_auth_registry_governance_overview
        .get("checks")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let session_auth_registry_governance_valid = session_auth_registry_governance_overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let body = format!(
        concat!(
            "# TYPE cex_consumer_entry_task_create_requests_total counter\n",
            "cex_consumer_entry_task_create_requests_total {}\n",
            "# TYPE cex_consumer_entry_task_lookup_requests_total counter\n",
            "cex_consumer_entry_task_lookup_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_requests_total counter\n",
            "cex_consumer_entry_rate_limited_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_source_scope_requests_total counter\n",
            "cex_consumer_entry_rate_limited_source_scope_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_user_requests_total counter\n",
            "cex_consumer_entry_rate_limited_user_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_room_requests_total counter\n",
            "cex_consumer_entry_rate_limited_room_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_session_requests_total counter\n",
            "cex_consumer_entry_rate_limited_session_requests_total {}\n",
            "# TYPE cex_consumer_entry_rate_limited_org_requests_total counter\n",
            "cex_consumer_entry_rate_limited_org_requests_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_failures_total counter\n",
            "cex_consumer_entry_identity_binding_failures_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_matches_total counter\n",
            "cex_consumer_entry_identity_binding_matches_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_reload_requests_total counter\n",
            "cex_consumer_entry_identity_binding_reload_requests_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_reload_successes_total counter\n",
            "cex_consumer_entry_identity_binding_reload_successes_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_reload_rejections_total counter\n",
            "cex_consumer_entry_identity_binding_reload_rejections_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_reload_actor_rejections_total counter\n",
            "cex_consumer_entry_identity_binding_reload_actor_rejections_total {}\n",
            "# TYPE cex_consumer_entry_identity_binding_audit_failures_total counter\n",
            "cex_consumer_entry_identity_binding_audit_failures_total {}\n",
            "# TYPE cex_consumer_entry_ingress_auth_failures_total counter\n",
            "cex_consumer_entry_ingress_auth_failures_total {}\n",
            "# TYPE cex_consumer_entry_session_auth_successes_total counter\n",
            "cex_consumer_entry_session_auth_successes_total {}\n",
            "# TYPE cex_consumer_entry_session_auth_failures_total counter\n",
            "cex_consumer_entry_session_auth_failures_total {}\n",
            "# TYPE cex_consumer_entry_replay_hits_total counter\n",
            "cex_consumer_entry_replay_hits_total {}\n",
            "# TYPE cex_consumer_entry_profile_validation_ok gauge\n",
            "cex_consumer_entry_profile_validation_ok {}\n",
            "# TYPE cex_consumer_entry_ingress_protected gauge\n",
            "cex_consumer_entry_ingress_protected {}\n",
            "# TYPE cex_consumer_entry_require_session_auth gauge\n",
            "cex_consumer_entry_require_session_auth {}\n",
            "# TYPE cex_consumer_entry_session_auth_global_secret_present gauge\n",
            "cex_consumer_entry_session_auth_global_secret_present {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_secrets gauge\n",
            "cex_consumer_entry_session_auth_issuer_secrets {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_key_issuers gauge\n",
            "cex_consumer_entry_session_auth_issuer_key_issuers {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_keys gauge\n",
            "cex_consumer_entry_session_auth_issuer_keys {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_configured gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_configured {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_loaded gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_loaded {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_revision_present gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_revision_present {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_issuers gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_issuers {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_keys gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_keys {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_approval_configured gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_approval_configured {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_approval_required gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_approval_required {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_actor_gate_valid gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_actor_gate_valid {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_approval_loaded gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_approval_loaded {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_revision_approved gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_revision_approved {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_governance_valid gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_governance_valid {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_approval_source_valid gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_approval_source_valid {}\n",
            "# TYPE cex_consumer_entry_session_auth_issuer_registry_approval_coverage_valid gauge\n",
            "cex_consumer_entry_session_auth_issuer_registry_approval_coverage_valid {}\n",
            "# TYPE cex_consumer_entry_session_auth_allowed_issuers gauge\n",
            "cex_consumer_entry_session_auth_allowed_issuers {}\n",
            "# TYPE cex_consumer_entry_session_auth_expected_audience_configured gauge\n",
            "cex_consumer_entry_session_auth_expected_audience_configured {}\n",
            "# TYPE cex_consumer_entry_require_identity_binding gauge\n",
            "cex_consumer_entry_require_identity_binding {}\n",
            "# TYPE cex_consumer_entry_identity_registry_users gauge\n",
            "cex_consumer_entry_identity_registry_users {}\n",
            "# TYPE cex_consumer_entry_identity_registry_refs gauge\n",
            "cex_consumer_entry_identity_registry_refs {}\n",
            "# TYPE cex_consumer_entry_identity_registry_missing_refs gauge\n",
            "cex_consumer_entry_identity_registry_missing_refs {}\n",
            "# TYPE cex_consumer_entry_rate_limit_store_enabled gauge\n",
            "cex_consumer_entry_rate_limit_store_enabled {}\n",
            "# TYPE cex_consumer_entry_rate_limit_bucket_count gauge\n",
            "cex_consumer_entry_rate_limit_bucket_count {}\n",
            "# TYPE cex_consumer_entry_identity_governance_valid gauge\n",
            "cex_consumer_entry_identity_governance_valid {}\n",
            "# TYPE cex_consumer_entry_identity_binding_loaded gauge\n",
            "cex_consumer_entry_identity_binding_loaded {}\n",
            "# TYPE cex_consumer_entry_identity_registry_loaded gauge\n",
            "cex_consumer_entry_identity_registry_loaded {}\n",
            "# TYPE cex_consumer_entry_identity_ref_integrity_ok gauge\n",
            "cex_consumer_entry_identity_ref_integrity_ok {}\n",
            "# TYPE cex_consumer_entry_identity_actor_gate_valid gauge\n",
            "cex_consumer_entry_identity_actor_gate_valid {}\n",
            "# TYPE cex_consumer_entry_identity_approval_source_valid gauge\n",
            "cex_consumer_entry_identity_approval_source_valid {}\n",
            "# TYPE cex_consumer_entry_identity_approval_coverage_valid gauge\n",
            "cex_consumer_entry_identity_approval_coverage_valid {}\n"
        ),
        state
            .inner
            .metrics
            .task_create_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .task_lookup_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_source_scope_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_user_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_room_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_session_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .rate_limited_org_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_failures
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_matches
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_reload_requests
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_reload_successes
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_reload_rejections
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_reload_actor_rejections
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .identity_binding_audit_failures
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .ingress_auth_failures
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .session_auth_successes
            .load(Ordering::Relaxed),
        state
            .inner
            .metrics
            .session_auth_failures
            .load(Ordering::Relaxed),
        state.inner.metrics.replay_hits.load(Ordering::Relaxed),
        profile_ok,
        if state.config().ingress_token.is_some() {
            1
        } else {
            0
        },
        if state.config().require_session_auth {
            1
        } else {
            0
        },
        if state.config().session_auth_secret.is_some() {
            1
        } else {
            0
        },
        state.config().session_auth_issuer_secrets.len(),
        state.config().session_auth_issuer_keys.len(),
        state.config().session_auth_issuer_keys.values().map(|keys| keys.len()).sum::<usize>(),
        if state.config().session_auth_issuer_registry_path.is_some() {
            1
        } else {
            0
        },
        if session_auth_registry_state.metadata.load_status == "loaded" {
            1
        } else {
            0
        },
        if session_auth_registry_state.metadata.revision.is_some() {
            1
        } else {
            0
        },
        session_auth_registry_state.metadata.issuer_count,
        session_auth_registry_state.metadata.key_count,
        if state
            .config()
            .session_auth_issuer_registry_approved_revisions_path
            .is_some()
        {
            1
        } else {
            0
        },
        if state
            .config()
            .session_auth_issuer_registry_require_approved_revision
        {
            1
        } else {
            0
        },
        if session_auth_registry_actor_checks
            .get("valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if session_auth_registry_approval_state.load_status == "loaded" {
            1
        } else {
            0
        },
        if session_auth_registry_approval_checks
            .get("current_revision_approved")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if session_auth_registry_governance_valid { 1 } else { 0 },
        if session_auth_registry_governance_checks
            .get("approval_source_valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if session_auth_registry_governance_checks
            .get("approval_coverage_valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        state.config().session_auth_allowed_issuers.len(),
        if state.config().session_auth_expected_audience.is_some() {
            1
        } else {
            0
        },
        if state.config().require_identity_binding {
            1
        } else {
            0
        },
        product_user_count,
        product_user_ref_count,
        missing_product_user_ref_count,
        if state.config().rate_limit_store_path.is_some() {
            1
        } else {
            0
        },
        rate_limit_bucket_count,
        if governance_valid { 1 } else { 0 },
        if governance_checks
            .get("binding_loaded")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if governance_checks
            .get("registry_loaded")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if governance_checks
            .get("ref_integrity_ok")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if governance_checks
            .get("actor_gate_valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if governance_checks
            .get("approval_source_valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
        if governance_checks
            .get("approval_coverage_valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            1
        } else {
            0
        },
    );
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}

fn identity_binding_metadata_json(metadata: &IdentityBindingMetadata) -> Value {
    json!({
        "format": metadata.format.clone(),
        "version": metadata.version,
        "revision": metadata.revision.clone(),
        "source_path": metadata.source_path.clone(),
        "source_modified_epoch": metadata.source_modified_epoch,
        "loaded_at_epoch": metadata.loaded_at_epoch,
        "load_status": metadata.load_status.clone(),
        "load_error": metadata.load_error.clone(),
    })
}

fn identity_binding_audit_json(audit_state: &IdentityBindingAuditState) -> Value {
    json!({
        "path": audit_state.path.clone(),
        "last_event_kind": audit_state.last_event_kind.clone(),
        "last_event_epoch": audit_state.last_event_epoch,
        "last_status": audit_state.last_status.clone(),
        "last_error": audit_state.last_error.clone(),
        "last_policy_decision": audit_state.last_policy_decision.clone(),
        "last_policy_reason": audit_state.last_policy_reason.clone(),
    })
}

fn identity_source_of_truth_json(store: &IdentityBindingStore) -> Value {
    json!({
        "mode": identity_source_of_truth_mode(store),
        "product_users": store.product_users.len(),
        "missing_product_user_refs": count_missing_product_user_refs(store),
    })
}

fn identity_reload_policy_json(config: &ConsumerEntryConfig) -> Value {
    json!({
        "require_revision": config.identity_binding_reload_require_revision,
        "reject_same_revision": config.identity_binding_reload_reject_same_revision,
        "allow_legacy_format": config.identity_binding_reload_allow_legacy_format,
        "require_approved_revision": config.identity_binding_reload_require_approved_revision,
        "allow_rollback": config.identity_binding_reload_allow_rollback,
        "require_actor": config.identity_binding_reload_require_actor,
        "actor_header": config.identity_binding_reload_actor_header,
        "allowed_actors_count": config.identity_binding_reload_allowed_actors.len(),
    })
}

fn identity_actor_checks_json(config: &ConsumerEntryConfig) -> Value {
    let actor_header_valid = !config
        .identity_binding_reload_actor_header
        .trim()
        .is_empty();
    let status = if !config.identity_binding_reload_require_actor {
        "disabled"
    } else if !actor_header_valid {
        "actor_header_missing"
    } else if config.identity_binding_reload_allowed_actors.is_empty() {
        "no_allowed_actors_configured"
    } else {
        "ok"
    };
    let valid = matches!(status, "disabled" | "ok");

    json!({
        "status": status,
        "valid": valid,
        "require_actor": config.identity_binding_reload_require_actor,
        "actor_header": config.identity_binding_reload_actor_header,
        "actor_header_valid": actor_header_valid,
        "allowed_actor_count": config.identity_binding_reload_allowed_actors.len(),
        "allowed_actors": config.identity_binding_reload_allowed_actors,
    })
}

fn current_effective_identity_revision(
    config: &ConsumerEntryConfig,
    store: &IdentityBindingStore,
) -> Option<String> {
    effective_identity_revision(
        &store.metadata,
        &store.registry_metadata,
        config.identity_registry_path.is_some(),
    )
}

fn identity_approval_checks_json(
    config: &ConsumerEntryConfig,
    store: &IdentityBindingStore,
    approval_state: &IdentityBindingRevisionApprovalState,
) -> Value {
    let current_effective_revision = current_effective_identity_revision(config, store);
    let current_effective_revision_index =
        current_effective_revision.as_ref().and_then(|revision| {
            approval_state
                .approved_revisions
                .iter()
                .position(|approved| approved == revision)
        });
    let latest_approved_revision = approval_state.approved_revisions.last().cloned();
    let latest_approved_revision_index = approval_state.approved_revisions.len().checked_sub(1);
    let current_effective_revision_approved = current_effective_revision.as_ref().map(|revision| {
        approval_state
            .approved_revisions
            .iter()
            .any(|approved| approved == revision)
    });
    let current_matches_latest_approved = current_effective_revision
        .as_ref()
        .zip(latest_approved_revision.as_ref())
        .map(|(current, latest)| current == latest);
    let status = if config.identity_binding_approved_revisions_path.is_none() {
        "approval_not_configured"
    } else if approval_state.load_status != "loaded" {
        "approval_state_not_loaded"
    } else if current_effective_revision.is_none() {
        "current_effective_revision_missing"
    } else if current_effective_revision_approved != Some(true) {
        "current_effective_revision_not_approved"
    } else {
        "ok"
    };

    json!({
        "configured": config.identity_binding_approved_revisions_path.is_some(),
        "status": status,
        "current_effective_revision": current_effective_revision,
        "current_effective_revision_approved": current_effective_revision_approved,
        "current_effective_revision_index": current_effective_revision_index,
        "latest_approved_revision": latest_approved_revision,
        "latest_approved_revision_index": latest_approved_revision_index,
        "current_matches_latest_approved": current_matches_latest_approved,
        "approved_revision_count": approval_state.approved_revisions.len(),
        "approval_state_loaded": approval_state.load_status == "loaded",
        "rollback_order_available": current_effective_revision_index.is_some() && !approval_state.approved_revisions.is_empty(),
    })
}

fn identity_approval_source_json(
    config: &ConsumerEntryConfig,
    store: &IdentityBindingStore,
    approval_state: &IdentityBindingRevisionApprovalState,
    limit: usize,
) -> Value {
    let current_effective_revision = current_effective_identity_revision(config, store);
    let current_effective_revision_index =
        current_effective_revision.as_ref().and_then(|revision| {
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
                "is_current_effective": current_effective_revision
                    .as_ref()
                    .map(|current| current == revision)
                    .unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();
    let status = if config.identity_binding_approved_revisions_path.is_none() {
        "approval_not_configured"
    } else if approval_state.load_status != "loaded" {
        "approval_state_not_loaded"
    } else if approval_state.approved_revisions.is_empty() {
        "approved_revision_set_empty"
    } else {
        "ok"
    };

    json!({
        "configured": config.identity_binding_approved_revisions_path.is_some(),
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
        "current_effective_revision": current_effective_revision,
        "current_effective_revision_index": current_effective_revision_index,
        "current_effective_revision_approved": current_effective_revision_index.is_some(),
        "revisions": revisions,
    })
}

fn identity_governance_overview_json(
    config: &ConsumerEntryConfig,
    store: &IdentityBindingStore,
    approval_state: &IdentityBindingRevisionApprovalState,
    audit_state: &IdentityBindingAuditState,
    approval_limit: usize,
) -> Value {
    let binding_loaded = store.metadata.load_status == "loaded";
    let registry_configured = config.identity_registry_path.is_some();
    let registry_loaded = !registry_configured || store.registry_metadata.load_status == "loaded";
    let missing_product_user_refs = count_missing_product_user_refs(store);
    let actor_checks = identity_actor_checks_json(config);
    let actor_valid = actor_checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let approval_checks = identity_approval_checks_json(config, store, approval_state);
    let approval_coverage_status = approval_checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let approval_coverage_valid = approval_coverage_status == "ok";
    let approval_source =
        identity_approval_source_json(config, store, approval_state, approval_limit);
    let approval_source_valid = approval_source
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let actor_status = actor_checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("actor_header_missing");
    let approval_source_status = approval_source
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let status = if !binding_loaded {
        "identity_bindings_not_loaded"
    } else if !registry_loaded {
        "identity_registry_not_loaded"
    } else if missing_product_user_refs > 0 {
        "missing_product_user_refs"
    } else if !actor_valid {
        actor_status
    } else if !approval_source_valid {
        approval_source_status
    } else if !approval_coverage_valid {
        approval_coverage_status
    } else {
        "ok"
    };

    json!({
        "status": status,
        "valid": status == "ok",
        "effective_revision": current_effective_identity_revision(config, store),
        "registry_configured": registry_configured,
        "missing_product_user_refs": missing_product_user_refs,
        "checks": {
            "binding_loaded": binding_loaded,
            "registry_loaded": registry_loaded,
            "ref_integrity_ok": missing_product_user_refs == 0,
            "actor_gate_valid": actor_valid,
            "approval_source_valid": approval_source_valid,
            "approval_coverage_valid": approval_coverage_valid,
        },
        "identity_binding_metadata": identity_binding_metadata_json(&store.metadata),
        "identity_registry_metadata": identity_binding_metadata_json(&store.registry_metadata),
        "identity_binding_counts": identity_binding_counts_json(store),
        "identity_source_of_truth": identity_source_of_truth_json(store),
        "identity_binding_reload_policy": identity_reload_policy_json(config),
        "identity_binding_audit": identity_binding_audit_json(audit_state),
        "identity_actor_checks": actor_checks,
        "identity_binding_revision_approval": approval_state,
        "identity_approval_checks": approval_checks,
        "identity_approval_source": approval_source,
    })
}

fn identity_admin_snapshot_json(
    config: &ConsumerEntryConfig,
    store: &IdentityBindingStore,
    approval_state: &IdentityBindingRevisionApprovalState,
) -> Value {
    json!({
        "registry_configured": config.identity_registry_path.is_some(),
        "effective_revision": current_effective_identity_revision(config, store),
        "missing_product_user_refs": count_missing_product_user_refs(store),
        "identity_binding_metadata": identity_binding_metadata_json(&store.metadata),
        "identity_registry_metadata": identity_binding_metadata_json(&store.registry_metadata),
        "identity_binding_counts": identity_binding_counts_json(store),
        "identity_source_of_truth": identity_source_of_truth_json(store),
        "identity_binding_reload_policy": identity_reload_policy_json(config),
        "identity_actor_checks": identity_actor_checks_json(config),
        "identity_binding_revision_approval": approval_state,
        "identity_approval_checks": identity_approval_checks_json(config, store, approval_state),
    })
}

fn session_auth_issuer_registry_active_key_rows(
    registry: &HashMap<String, SessionAuthIssuerRegistryIssuer>,
) -> Vec<Value> {
    let mut rows = registry
        .iter()
        .map(|(issuer, entry)| {
            let mut key_ids = entry.keys.keys().cloned().collect::<Vec<_>>();
            key_ids.sort();
            let active_key_id = entry
                .active_key_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
            let active_key_present = active_key_id
                .as_ref()
                .map(|key_id| entry.keys.contains_key(key_id))
                .unwrap_or(false);
            json!({
                "issuer": issuer,
                "active_key_id": active_key_id,
                "active_key_present": active_key_present,
                "key_count": key_ids.len(),
                "key_ids": key_ids,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.get("issuer")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(
                right
                    .get("issuer")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
    });
    rows
}

fn session_auth_issuer_registry_active_key_diff_json(
    current: &HashMap<String, SessionAuthIssuerRegistryIssuer>,
    candidate: &HashMap<String, SessionAuthIssuerRegistryIssuer>,
) -> Value {
    let mut issuers = current
        .keys()
        .chain(candidate.keys())
        .cloned()
        .collect::<Vec<_>>();
    issuers.sort();
    issuers.dedup();

    let mut changes = Vec::new();
    for issuer in issuers {
        let current_active_key_id = current
            .get(&issuer)
            .and_then(|entry| entry.active_key_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let candidate_active_key_id = candidate
            .get(&issuer)
            .and_then(|entry| entry.active_key_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        if current_active_key_id == candidate_active_key_id {
            continue;
        }
        let status = match (current_active_key_id.as_ref(), candidate_active_key_id.as_ref()) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            (Some(_), Some(_)) => "changed",
            (None, None) => continue,
        };
        changes.push(json!({
            "issuer": issuer,
            "status": status,
            "current_active_key_id": current_active_key_id,
            "candidate_active_key_id": candidate_active_key_id,
        }));
    }

    json!({
        "changed_active_key_count": changes.len(),
        "matches": changes.is_empty(),
        "changes": changes,
    })
}

fn session_auth_issuer_registry_status_json(
    config: &ConsumerEntryConfig,
    metadata: &SessionAuthIssuerRegistryMetadata,
    registry: &HashMap<String, SessionAuthIssuerRegistryIssuer>,
) -> Value {
    let mut issuers = registry.keys().cloned().collect::<Vec<_>>();
    issuers.sort();

    let mut issuers_without_active_key = registry
        .iter()
        .filter_map(|(issuer, entry)| {
            entry.active_key_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
                .then_some(issuer.clone())
        })
        .collect::<Vec<_>>();
    issuers_without_active_key.sort();

    let mut allowed_issuers_in_registry = config
        .session_auth_allowed_issuers
        .iter()
        .filter(|issuer| registry.contains_key(issuer.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    allowed_issuers_in_registry.sort();

    let mut allowed_issuers_missing = config
        .session_auth_allowed_issuers
        .iter()
        .filter(|issuer| !registry.contains_key(issuer.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    allowed_issuers_missing.sort();

    let issuer_active_keys = session_auth_issuer_registry_active_key_rows(registry);
    let status = if config.session_auth_issuer_registry_path.is_none() {
        "disabled".to_string()
    } else if metadata.load_status != "loaded" {
        metadata.load_status.clone()
    } else if metadata.issuer_count == 0 {
        "empty".to_string()
    } else {
        "ok".to_string()
    };
    let valid = status == "ok";

    json!({
        "status": status,
        "valid": valid,
        "configured": config.session_auth_issuer_registry_path.is_some(),
        "loaded": metadata.load_status == "loaded",
        "metadata": metadata,
        "issuer_count": metadata.issuer_count,
        "key_count": metadata.key_count,
        "issuers": issuers,
        "active_key_issuer_count": metadata.issuer_count.saturating_sub(issuers_without_active_key.len()),
        "issuers_without_active_key": issuers_without_active_key,
        "issuer_active_keys": issuer_active_keys,
        "allowed_issuers": config.session_auth_allowed_issuers,
        "allowed_issuer_count": config.session_auth_allowed_issuers.len(),
        "allowed_issuers_in_registry": allowed_issuers_in_registry,
        "allowed_issuers_missing": allowed_issuers_missing,
        "expected_audience": config.session_auth_expected_audience,
    })
}

fn session_auth_issuer_registry_actor_checks_json(config: &ConsumerEntryConfig) -> Value {
    let actor_header_valid = !config
        .session_auth_issuer_registry_actor_header
        .trim()
        .is_empty();
    let status = if !config.session_auth_issuer_registry_require_actor {
        "disabled"
    } else if !actor_header_valid {
        "actor_header_missing"
    } else if config.session_auth_issuer_registry_allowed_actors.is_empty() {
        "no_allowed_actors_configured"
    } else {
        "ok"
    };
    let valid = matches!(status, "disabled" | "ok");

    json!({
        "status": status,
        "valid": valid,
        "require_actor": config.session_auth_issuer_registry_require_actor,
        "actor_header": config.session_auth_issuer_registry_actor_header,
        "actor_header_valid": actor_header_valid,
        "allowed_actor_count": config.session_auth_issuer_registry_allowed_actors.len(),
        "allowed_actors": config.session_auth_issuer_registry_allowed_actors,
    })
}

fn session_auth_issuer_registry_actor_request_json(
    config: &ConsumerEntryConfig,
    requesting_actor: Option<&str>,
) -> Value {
    let actor_checks = session_auth_issuer_registry_actor_checks_json(config);
    let normalized_actor = requesting_actor
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let actor_header = actor_checks
        .get("actor_header")
        .and_then(Value::as_str)
        .unwrap_or(config.session_auth_issuer_registry_actor_header.as_str());
    let actor_checks_status = actor_checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("actor_header_missing");

    let (authorized, status, reason) = if !config.session_auth_issuer_registry_require_actor {
        (None, "disabled".to_string(), None)
    } else if actor_checks_status != "ok" {
        (
            Some(false),
            actor_checks_status.to_string(),
            Some(actor_checks_status.to_string()),
        )
    } else if let Some(actor) = normalized_actor.as_deref() {
        if config
            .session_auth_issuer_registry_allowed_actors
            .iter()
            .any(|allowed| allowed == actor)
        {
            (Some(true), "ok".to_string(), None)
        } else {
            (
                Some(false),
                "actor_not_allowed".to_string(),
                Some("actor_not_allowed".to_string()),
            )
        }
    } else {
        (
            Some(false),
            "actor_missing".to_string(),
            Some("actor_missing".to_string()),
        )
    };

    json!({
        "required": config.session_auth_issuer_registry_require_actor,
        "actor_header": actor_header,
        "request_actor": normalized_actor,
        "authorized": authorized,
        "status": status,
        "reason": reason,
    })
}

fn session_auth_issuer_registry_current_revision(
    metadata: &SessionAuthIssuerRegistryMetadata,
) -> Option<String> {
    metadata
        .revision
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn session_auth_issuer_registry_approval_checks_json(
    config: &ConsumerEntryConfig,
    metadata: &SessionAuthIssuerRegistryMetadata,
    approval_state: &SessionAuthIssuerRegistryRevisionApprovalState,
) -> Value {
    let current_revision = session_auth_issuer_registry_current_revision(metadata);
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
        .session_auth_issuer_registry_approved_revisions_path
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
            .session_auth_issuer_registry_approved_revisions_path
            .is_some(),
        "required": config.session_auth_issuer_registry_require_approved_revision,
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

fn session_auth_issuer_registry_approval_source_json(
    config: &ConsumerEntryConfig,
    metadata: &SessionAuthIssuerRegistryMetadata,
    approval_state: &SessionAuthIssuerRegistryRevisionApprovalState,
    limit: usize,
) -> Value {
    let current_revision = session_auth_issuer_registry_current_revision(metadata);
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
        .session_auth_issuer_registry_approved_revisions_path
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
            .session_auth_issuer_registry_approved_revisions_path
            .is_some(),
        "required": config.session_auth_issuer_registry_require_approved_revision,
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

fn session_auth_issuer_registry_governance_overview_json(
    config: &ConsumerEntryConfig,
    metadata: &SessionAuthIssuerRegistryMetadata,
    approval_state: &SessionAuthIssuerRegistryRevisionApprovalState,
    approval_limit: usize,
) -> Value {
    let registry_configured = config.session_auth_issuer_registry_path.is_some();
    let approval_source_configured = config
        .session_auth_issuer_registry_approved_revisions_path
        .is_some();
    let registry_loaded = !registry_configured || metadata.load_status == "loaded";
    let revision_present = !registry_configured || metadata.revision.is_some();
    let actor_checks = session_auth_issuer_registry_actor_checks_json(config);
    let actor_gate_valid = if !registry_configured {
        true
    } else {
        actor_checks
            .get("valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let actor_gate_status = actor_checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("actor_header_missing");
    let approval_checks =
        session_auth_issuer_registry_approval_checks_json(config, metadata, approval_state);
    let approval_source = session_auth_issuer_registry_approval_source_json(
        config,
        metadata,
        approval_state,
        approval_limit,
    );
    let approval_source_valid = if !registry_configured {
        true
    } else if !approval_source_configured {
        !config.session_auth_issuer_registry_require_approved_revision
    } else {
        approval_source
            .get("valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let approval_coverage_valid = if !registry_configured
        || !config.session_auth_issuer_registry_require_approved_revision
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
    let status = if !config.require_session_auth {
        "session_auth_disabled"
    } else if !registry_configured {
        "issuer_registry_not_configured"
    } else if !registry_loaded {
        "issuer_registry_not_loaded"
    } else if !revision_present {
        "issuer_registry_revision_missing"
    } else if !actor_gate_valid {
        actor_gate_status
    } else if !approval_source_valid {
        approval_source_status
    } else if !approval_coverage_valid {
        approval_coverage_status
    } else {
        "ok"
    };

    json!({
        "status": status,
        "valid": status == "ok" || status == "issuer_registry_not_configured" || status == "session_auth_disabled",
        "session_auth_required": config.require_session_auth,
        "configured": registry_configured,
        "approval_required": config.session_auth_issuer_registry_require_approved_revision,
        "current_revision": session_auth_issuer_registry_current_revision(metadata),
        "checks": {
            "registry_loaded": registry_loaded,
            "revision_present": revision_present,
            "actor_gate_valid": actor_gate_valid,
            "approval_source_valid": approval_source_valid,
            "approval_coverage_valid": approval_coverage_valid,
        },
        "issuer_registry_metadata": metadata,
        "issuer_registry_actor_checks": actor_checks,
        "issuer_registry_approval": approval_state,
        "issuer_registry_approval_checks": approval_checks,
        "issuer_registry_approval_source": approval_source,
    })
}

fn normalize_identity_approval_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(20).clamp(1, 100)
}

fn normalize_identity_audit_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(20).clamp(1, 100)
}

fn is_registry_audit_event_kind(kind: &str) -> bool {
    matches!(kind, "registry_reload" | "registry_reload_rejected")
}

fn read_registry_audit_events(path: &str, limit: usize) -> Result<(Vec<Value>, usize), String> {
    let raw = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    let mut events = Vec::new();
    let mut parse_error_count = 0;

    for line in raw.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = match serde_json::from_str::<Value>(trimmed) {
            Ok(value) => value,
            Err(_) => {
                parse_error_count += 1;
                continue;
            }
        };
        let is_registry_event = value
            .get("event_kind")
            .and_then(Value::as_str)
            .map(is_registry_audit_event_kind)
            .unwrap_or(false);
        if !is_registry_event {
            continue;
        }
        events.push(value);
        if events.len() >= limit {
            break;
        }
    }

    Ok((events, parse_error_count))
}

fn identity_reload_response_json(
    ok: bool,
    reloaded: bool,
    store: &IdentityBindingStore,
    audit_state: &IdentityBindingAuditState,
    governance: &IdentityBindingReloadGovernance,
    approval_state: &IdentityBindingRevisionApprovalState,
) -> Value {
    json!({
        "ok": ok,
        "reloaded": reloaded,
        "identity_binding_metadata": identity_binding_metadata_json(&store.metadata),
        "identity_registry_metadata": identity_binding_metadata_json(&store.registry_metadata),
        "identity_binding_counts": identity_binding_counts_json(store),
        "identity_source_of_truth": identity_source_of_truth_json(store),
        "identity_binding_audit": identity_binding_audit_json(audit_state),
        "identity_binding_reload_governance": governance,
        "identity_binding_revision_approval": approval_state,
    })
}

async fn apply_identity_store_reload(
    state: &AppState,
    headers: &HeaderMap,
    reloaded_store: IdentityBindingStore,
    accepted_event_kind: &str,
    rejected_event_kind: &str,
) -> Response {
    if let Err(response) = authorize_ingress(headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    state.inner.metrics.inc_identity_binding_reload_requests();
    let current_store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let requesting_actor = headers
        .get(state.config().identity_binding_reload_actor_header.as_str())
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let governance = evaluate_identity_binding_reload_governance(
        state.config(),
        &current_store,
        &reloaded_store,
        &approval_state,
        requesting_actor.as_deref(),
    );
    let event_kind = if governance.accepted {
        accepted_event_kind
    } else {
        rejected_event_kind
    };
    let audit_state = append_identity_binding_audit_event(
        state.config(),
        event_kind,
        &reloaded_store,
        Some(&governance),
    );
    if audit_state.last_status != "written"
        && state.config().identity_binding_audit_log_path.is_some()
    {
        state.inner.metrics.inc_identity_binding_audit_failures();
    }

    let response_body = if governance.accepted {
        identity_reload_response_json(
            true,
            true,
            &reloaded_store,
            &audit_state,
            &governance,
            &approval_state,
        )
    } else {
        identity_reload_response_json(
            false,
            false,
            &reloaded_store,
            &audit_state,
            &governance,
            &approval_state,
        )
    };

    {
        let mut audit_state_guard = state.inner.identity_binding_audit_state.write().await;
        *audit_state_guard = audit_state;
    }

    if !governance.accepted {
        state.inner.metrics.inc_identity_binding_reload_rejections();
        if governance.actor_authorized == Some(false) {
            state
                .inner
                .metrics
                .inc_identity_binding_reload_actor_rejections();
            return (StatusCode::FORBIDDEN, Json(response_body)).into_response();
        }
        return (StatusCode::CONFLICT, Json(response_body)).into_response();
    }

    state.inner.metrics.inc_identity_binding_reload_successes();
    {
        let mut store = state.inner.identity_binding_store.write().await;
        *store = reloaded_store;
    }

    (StatusCode::OK, Json(response_body)).into_response()
}

async fn reload_identity_bindings(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let reloaded_store = load_identity_binding_store(state.config());
    apply_identity_store_reload(
        &state,
        &headers,
        reloaded_store,
        "reload",
        "reload_rejected",
    )
    .await
}

async fn reload_identity_registry(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(_) = state.config().identity_registry_path.as_deref() else {
        if let Err(response) = authorize_ingress(&headers, state.config()) {
            state.inner.metrics.inc_ingress_auth_failures();
            return response;
        }
        let current_store = {
            let store = state.inner.identity_binding_store.read().await;
            store.clone()
        };
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "ok": false,
                "reloaded": false,
                "error": "identity_registry_not_configured",
                "identity_binding_metadata": identity_binding_metadata_json(&current_store.metadata),
                "identity_registry_metadata": identity_binding_metadata_json(&current_store.registry_metadata),
                "identity_binding_counts": identity_binding_counts_json(&current_store),
                "identity_source_of_truth": identity_source_of_truth_json(&current_store),
            })),
        )
            .into_response();
    };

    let current_store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let (registry_metadata, product_users) = load_product_user_registry(
        state.config().identity_registry_path.as_deref(),
        current_store.product_users.clone(),
        &current_store.metadata,
    );
    let reloaded_store = IdentityBindingStore {
        registry_metadata,
        product_users,
        ..current_store
    };

    apply_identity_store_reload(
        &state,
        &headers,
        reloaded_store,
        "registry_reload",
        "registry_reload_rejected",
    )
    .await
}

async fn validate_identity_registry(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let Some(_) = state.config().identity_registry_path.as_deref() else {
        let current_store = {
            let store = state.inner.identity_binding_store.read().await;
            store.clone()
        };
        let approval_state = load_identity_binding_revision_approval_state(state.config());
        let mut body =
            identity_admin_snapshot_json(state.config(), &current_store, &approval_state);
        let object = body
            .as_object_mut()
            .expect("identity admin snapshot should be json object");
        object.insert("ok".to_string(), json!(false));
        object.insert("validated".to_string(), json!(true));
        object.insert("valid".to_string(), json!(false));
        object.insert("would_reload".to_string(), json!(false));
        object.insert(
            "error".to_string(),
            json!("identity_registry_not_configured"),
        );
        return (StatusCode::CONFLICT, Json(body)).into_response();
    };

    let current_store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let (registry_metadata, product_users) = load_product_user_registry(
        state.config().identity_registry_path.as_deref(),
        current_store.product_users.clone(),
        &current_store.metadata,
    );
    let candidate_store = IdentityBindingStore {
        registry_metadata,
        product_users,
        ..current_store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let requesting_actor = headers
        .get(state.config().identity_binding_reload_actor_header.as_str())
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let governance = evaluate_identity_binding_reload_governance(
        state.config(),
        &current_store,
        &candidate_store,
        &approval_state,
        requesting_actor.as_deref(),
    );
    let mut body = identity_admin_snapshot_json(state.config(), &candidate_store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(governance.accepted));
    object.insert("validated".to_string(), json!(true));
    object.insert("valid".to_string(), json!(governance.accepted));
    object.insert("would_reload".to_string(), json!(governance.accepted));
    object.insert("checked_only".to_string(), json!(true));
    object.insert(
        "identity_binding_reload_governance".to_string(),
        serde_json::to_value(&governance).expect("serialize governance"),
    );

    if !governance.accepted {
        if governance.actor_authorized == Some(false) {
            return (StatusCode::FORBIDDEN, Json(body)).into_response();
        }
        return (StatusCode::CONFLICT, Json(body)).into_response();
    }

    (StatusCode::OK, Json(body)).into_response()
}

async fn get_identity_registry_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let audit = {
        let audit = state.inner.identity_binding_audit_state.read().await;
        audit.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert(
        "identity_binding_audit".to_string(),
        identity_binding_audit_json(&audit),
    );

    (StatusCode::OK, Json(body)).into_response()
}

async fn get_identity_registry_audit(
    State(state): State<AppState>,
    Query(query): Query<IdentityAuditQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let audit = {
        let audit = state.inner.identity_binding_audit_state.read().await;
        audit.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let Some(path) = state.config().identity_binding_audit_log_path.as_deref() else {
        let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
        let object = body
            .as_object_mut()
            .expect("identity admin snapshot should be json object");
        object.insert("ok".to_string(), json!(false));
        object.insert("status".to_string(), json!("error"));
        object.insert("error".to_string(), json!("identity_audit_not_configured"));
        object.insert(
            "identity_binding_audit".to_string(),
            identity_binding_audit_json(&audit),
        );
        return (StatusCode::CONFLICT, Json(body)).into_response();
    };

    let limit = normalize_identity_audit_limit(query.limit);
    let (events, parse_error_count) = match read_registry_audit_events(path, limit) {
        Ok(result) => result,
        Err(err) => {
            let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
            let object = body
                .as_object_mut()
                .expect("identity admin snapshot should be json object");
            object.insert("ok".to_string(), json!(false));
            object.insert("status".to_string(), json!("error"));
            object.insert("error".to_string(), json!("identity_audit_read_error"));
            object.insert("error_detail".to_string(), json!(err));
            object.insert(
                "identity_binding_audit".to_string(),
                identity_binding_audit_json(&audit),
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response();
        }
    };

    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert("audit_path".to_string(), json!(path));
    object.insert("limit".to_string(), json!(limit));
    object.insert("returned_event_count".to_string(), json!(events.len()));
    object.insert("parse_error_count".to_string(), json!(parse_error_count));
    object.insert("events".to_string(), json!(events));
    object.insert(
        "identity_binding_audit".to_string(),
        identity_binding_audit_json(&audit),
    );

    (StatusCode::OK, Json(body)).into_response()
}

async fn get_session_auth_issuer_registry_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let current_state = session_auth_issuer_registry_runtime_state(&state);
    let status = session_auth_issuer_registry_status_json(
        state.config(),
        &current_state.metadata,
        &current_state.registry,
    );
    let approval_state = load_session_auth_issuer_registry_revision_approval_state(state.config());
    let actor_checks = session_auth_issuer_registry_actor_checks_json(state.config());
    let approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
    );
    let approval_source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
        20,
    );

    let body = json!({
        "ok": true,
        "status": "ok",
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": status,
        "session_auth_issuer_registry_actor_checks": actor_checks,
        "session_auth_issuer_registry_approval": approval_checks,
        "session_auth_issuer_registry_approval_source": approval_source,
    });

    (StatusCode::OK, Json(body)).into_response()
}

async fn validate_session_auth_issuer_registry(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let current_state = session_auth_issuer_registry_runtime_state(&state);
    let current_status = session_auth_issuer_registry_status_json(
        state.config(),
        &current_state.metadata,
        &current_state.registry,
    );
    let (candidate_metadata, candidate_registry) =
        load_session_auth_issuer_registry(state.config().session_auth_issuer_registry_path.as_deref());
    let candidate_status = session_auth_issuer_registry_status_json(
        state.config(),
        &candidate_metadata,
        &candidate_registry,
    );
    let approval_state = load_session_auth_issuer_registry_revision_approval_state(state.config());
    let actor_checks = session_auth_issuer_registry_actor_checks_json(state.config());
    let requesting_actor = headers
        .get(state.config().session_auth_issuer_registry_actor_header.as_str())
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let actor_request =
        session_auth_issuer_registry_actor_request_json(state.config(), requesting_actor);
    let current_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
    );
    let current_approval_source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
        20,
    );
    let candidate_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &candidate_metadata,
        &approval_state,
    );
    let candidate_approval_source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &candidate_metadata,
        &approval_state,
        20,
    );
    let candidate_registry_valid = candidate_status
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let actor_gate_valid = if state.config().session_auth_issuer_registry_require_actor {
        actor_request
            .get("authorized")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    } else {
        true
    };
    let candidate_approval_valid = candidate_approval_checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let validation_status = if !candidate_registry_valid {
        candidate_status
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("error")
            .to_string()
    } else if state.config().session_auth_issuer_registry_require_actor && !actor_gate_valid {
        actor_request
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("actor_missing")
            .to_string()
    } else if state
        .config()
        .session_auth_issuer_registry_require_approved_revision
        && !candidate_approval_valid
    {
        candidate_approval_checks
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("approval_state_not_loaded")
            .to_string()
    } else {
        "ok".to_string()
    };
    let is_valid = candidate_registry_valid
        && (!state.config().session_auth_issuer_registry_require_actor || actor_gate_valid)
        && (!state
            .config()
            .session_auth_issuer_registry_require_approved_revision
            || candidate_approval_valid);
    let active_key_diff = session_auth_issuer_registry_active_key_diff_json(
        &current_state.registry,
        &candidate_registry,
    );
    let matches_loaded_revision = candidate_metadata.revision == current_state.metadata.revision;
    let matches_loaded_key_count = candidate_metadata.key_count == current_state.metadata.key_count;
    let matches_loaded_issuer_count =
        candidate_metadata.issuer_count == current_state.metadata.issuer_count;
    let matches_loaded_status = candidate_metadata.load_status == current_state.metadata.load_status;
    let matches_loaded_active_keys = active_key_diff
        .get("matches")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let body = json!({
        "ok": is_valid,
        "validated": true,
        "valid": is_valid,
        "status": validation_status,
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": current_status,
        "session_auth_issuer_registry_source": candidate_status,
        "session_auth_issuer_registry_actor_checks": actor_checks,
        "session_auth_issuer_registry_actor_request": actor_request,
        "session_auth_issuer_registry_approval": current_approval_checks,
        "session_auth_issuer_registry_approval_source": current_approval_source,
        "session_auth_issuer_registry_source_approval": candidate_approval_checks,
        "session_auth_issuer_registry_source_approval_source": candidate_approval_source,
        "session_auth_issuer_registry_active_key_diff": active_key_diff,
        "matches_loaded_status": matches_loaded_status,
        "matches_loaded_revision": matches_loaded_revision,
        "matches_loaded_issuer_count": matches_loaded_issuer_count,
        "matches_loaded_key_count": matches_loaded_key_count,
        "matches_loaded_active_keys": matches_loaded_active_keys,
    });

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

async fn reload_session_auth_issuer_registry(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let current_state = session_auth_issuer_registry_runtime_state(&state);
    let current_status = session_auth_issuer_registry_status_json(
        state.config(),
        &current_state.metadata,
        &current_state.registry,
    );
    let actor_checks = session_auth_issuer_registry_actor_checks_json(state.config());

    if state.config().session_auth_issuer_registry_path.is_none() {
        let approval_state =
            load_session_auth_issuer_registry_revision_approval_state(state.config());
        let approval_checks = session_auth_issuer_registry_approval_checks_json(
            state.config(),
            &current_state.metadata,
            &approval_state,
        );
        let approval_source = session_auth_issuer_registry_approval_source_json(
            state.config(),
            &current_state.metadata,
            &approval_state,
            20,
        );
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "ok": false,
                "reloaded": false,
                "error": "session_auth_issuer_registry_not_configured",
                "require_session_auth": state.config().require_session_auth,
                "session_auth_issuer_registry": current_status,
                "session_auth_issuer_registry_actor_checks": actor_checks,
                "session_auth_issuer_registry_approval": approval_checks,
                "session_auth_issuer_registry_approval_source": approval_source,
            })),
        )
            .into_response();
    }

    let (candidate_metadata, candidate_registry) = load_session_auth_issuer_registry(
        state.config().session_auth_issuer_registry_path.as_deref(),
    );
    let candidate_status = session_auth_issuer_registry_status_json(
        state.config(),
        &candidate_metadata,
        &candidate_registry,
    );
    let approval_state = load_session_auth_issuer_registry_revision_approval_state(state.config());
    let requesting_actor = headers
        .get(state.config().session_auth_issuer_registry_actor_header.as_str())
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let actor_request =
        session_auth_issuer_registry_actor_request_json(state.config(), requesting_actor);
    let current_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
    );
    let current_approval_source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
        20,
    );
    let candidate_approval_checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &candidate_metadata,
        &approval_state,
    );
    let candidate_approval_source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &candidate_metadata,
        &approval_state,
        20,
    );
    let candidate_registry_valid = candidate_status
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let actor_authorized = if state.config().session_auth_issuer_registry_require_actor {
        actor_request
            .get("authorized")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    } else {
        true
    };
    let candidate_approval_valid = candidate_approval_checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (accepted, reason) = if !candidate_registry_valid {
        (
            false,
            candidate_status
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("error")
                .to_string(),
        )
    } else if state.config().session_auth_issuer_registry_require_actor && !actor_authorized {
        (
            false,
            actor_request
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("actor_missing")
                .to_string(),
        )
    } else if state
        .config()
        .session_auth_issuer_registry_require_approved_revision
        && !candidate_approval_valid
    {
        (
            false,
            candidate_approval_checks
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("approval_state_not_loaded")
                .to_string(),
        )
    } else {
        (true, "ok".to_string())
    };
    let active_key_diff = session_auth_issuer_registry_active_key_diff_json(
        &current_state.registry,
        &candidate_registry,
    );
    let matches_loaded_active_keys = active_key_diff
        .get("matches")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if accepted {
        let mut live_state = state
            .inner
            .session_auth_issuer_registry_state
            .write()
            .expect("session auth issuer registry state lock poisoned");
        *live_state = SessionAuthIssuerRegistryRuntimeState {
            metadata: candidate_metadata.clone(),
            registry: candidate_registry.clone(),
        };
    }

    let live_state = session_auth_issuer_registry_runtime_state(&state);
    let live_status = session_auth_issuer_registry_status_json(
        state.config(),
        &live_state.metadata,
        &live_state.registry,
    );
    let governance = json!({
        "accepted": accepted,
        "status": if accepted { "accepted" } else { "rejected" },
        "reason": reason,
        "actor_authorized": actor_request.get("authorized").cloned().unwrap_or(Value::Null),
        "actor_reason": actor_request.get("reason").cloned().unwrap_or(Value::Null),
        "current_revision": session_auth_issuer_registry_current_revision(&current_state.metadata),
        "candidate_revision": session_auth_issuer_registry_current_revision(&candidate_metadata),
        "candidate_loaded": candidate_metadata.load_status == "loaded",
        "candidate_revision_approved": candidate_approval_checks
            .get("current_revision_approved")
            .cloned()
            .unwrap_or(Value::Null),
        "matches_loaded_active_keys": matches_loaded_active_keys,
    });

    let body = json!({
        "ok": accepted,
        "reloaded": accepted,
        "status": if accepted { "ok" } else { "rejected" },
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": live_status,
        "session_auth_issuer_registry_previous": current_status,
        "session_auth_issuer_registry_source": candidate_status,
        "session_auth_issuer_registry_actor_checks": actor_checks,
        "session_auth_issuer_registry_actor_request": actor_request,
        "session_auth_issuer_registry_approval": current_approval_checks,
        "session_auth_issuer_registry_approval_source": current_approval_source,
        "session_auth_issuer_registry_source_approval": candidate_approval_checks,
        "session_auth_issuer_registry_source_approval_source": candidate_approval_source,
        "session_auth_issuer_registry_active_key_diff": active_key_diff,
        "session_auth_issuer_registry_reload_governance": governance,
    });

    if accepted {
        return (StatusCode::OK, Json(body)).into_response();
    }

    if state.config().session_auth_issuer_registry_require_actor && !actor_authorized {
        return (StatusCode::FORBIDDEN, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

async fn get_session_auth_issuer_registry_actor_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let checks = session_auth_issuer_registry_actor_checks_json(state.config());
    let is_valid = checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let current_state = session_auth_issuer_registry_runtime_state(&state);

    let body = json!({
        "ok": true,
        "status": "ok",
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": session_auth_issuer_registry_status_json(
            state.config(),
            &current_state.metadata,
            &current_state.registry,
        ),
        "session_auth_issuer_registry_actor_checks": checks,
        "actor_valid": is_valid,
    });

    (StatusCode::OK, Json(body)).into_response()
}

async fn validate_session_auth_issuer_registry_actors(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let checks = session_auth_issuer_registry_actor_checks_json(state.config());
    let status = checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("actor_header_missing");
    let is_valid = checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let current_state = session_auth_issuer_registry_runtime_state(&state);

    let body = json!({
        "ok": is_valid,
        "validated": true,
        "valid": is_valid,
        "status": status,
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": session_auth_issuer_registry_status_json(
            state.config(),
            &current_state.metadata,
            &current_state.registry,
        ),
        "session_auth_issuer_registry_actor_checks": checks,
    });

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

async fn get_session_auth_issuer_registry_approval_status(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let current_state = session_auth_issuer_registry_runtime_state(&state);
    let approval_state = load_session_auth_issuer_registry_revision_approval_state(state.config());
    let checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
    );
    let source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
        normalize_identity_approval_limit(query.limit),
    );

    let body = json!({
        "ok": true,
        "status": "ok",
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": session_auth_issuer_registry_status_json(
            state.config(),
            &current_state.metadata,
            &current_state.registry,
        ),
        "session_auth_issuer_registry_approval": checks,
        "session_auth_issuer_registry_approval_source": source,
    });

    (StatusCode::OK, Json(body)).into_response()
}

async fn validate_session_auth_issuer_registry_approval(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let current_state = session_auth_issuer_registry_runtime_state(&state);
    let approval_state = load_session_auth_issuer_registry_revision_approval_state(state.config());
    let checks = session_auth_issuer_registry_approval_checks_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
    );
    let source = session_auth_issuer_registry_approval_source_json(
        state.config(),
        &current_state.metadata,
        &approval_state,
        normalize_identity_approval_limit(query.limit),
    );
    let status = checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let is_valid = checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let body = json!({
        "ok": is_valid,
        "validated": true,
        "valid": is_valid,
        "status": status,
        "require_session_auth": state.config().require_session_auth,
        "session_auth_issuer_registry": session_auth_issuer_registry_status_json(
            state.config(),
            &current_state.metadata,
            &current_state.registry,
        ),
        "session_auth_issuer_registry_approval": checks,
        "session_auth_issuer_registry_approval_source": source,
    });

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

async fn get_identity_approval_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let checks = identity_approval_checks_json(state.config(), &store, &approval_state);
    let is_valid = checks
        .get("status")
        .and_then(Value::as_str)
        .map(|status| status == "ok")
        .unwrap_or(false);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert("approval_valid".to_string(), json!(is_valid));

    (StatusCode::OK, Json(body)).into_response()
}

async fn validate_identity_approval(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let checks = identity_approval_checks_json(state.config(), &store, &approval_state);
    let status = checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let is_valid = status == "ok";
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(is_valid));
    object.insert("validated".to_string(), json!(true));
    object.insert("valid".to_string(), json!(is_valid));
    object.insert("status".to_string(), json!(status));

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

async fn get_identity_approval_source(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let limit = normalize_identity_approval_limit(query.limit);
    let source = identity_approval_source_json(state.config(), &store, &approval_state, limit);
    let is_valid = source
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert("source_valid".to_string(), json!(is_valid));
    object.insert("identity_approval_source".to_string(), source);

    (StatusCode::OK, Json(body)).into_response()
}

async fn validate_identity_approval_source(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let limit = normalize_identity_approval_limit(query.limit);
    let source = identity_approval_source_json(state.config(), &store, &approval_state, limit);
    let status = source
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let is_valid = source
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(is_valid));
    object.insert("validated".to_string(), json!(true));
    object.insert("valid".to_string(), json!(is_valid));
    object.insert("status".to_string(), json!(status));
    object.insert("identity_approval_source".to_string(), source);

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

async fn get_identity_governance_status(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let audit = {
        let audit = state.inner.identity_binding_audit_state.read().await;
        audit.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let limit = normalize_identity_approval_limit(query.limit);
    let overview =
        identity_governance_overview_json(state.config(), &store, &approval_state, &audit, limit);
    let is_valid = overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert("governance_valid".to_string(), json!(is_valid));
    object.insert(
        "identity_binding_audit".to_string(),
        identity_binding_audit_json(&audit),
    );
    object.insert("identity_governance_overview".to_string(), overview);

    (StatusCode::OK, Json(body)).into_response()
}

async fn validate_identity_governance(
    State(state): State<AppState>,
    Query(query): Query<IdentityApprovalQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let audit = {
        let audit = state.inner.identity_binding_audit_state.read().await;
        audit.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let limit = normalize_identity_approval_limit(query.limit);
    let overview =
        identity_governance_overview_json(state.config(), &store, &approval_state, &audit, limit);
    let status = overview
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("approval_state_not_loaded");
    let is_valid = overview
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(is_valid));
    object.insert("validated".to_string(), json!(true));
    object.insert("valid".to_string(), json!(is_valid));
    object.insert("status".to_string(), json!(status));
    object.insert(
        "identity_binding_audit".to_string(),
        identity_binding_audit_json(&audit),
    );
    object.insert("identity_governance_overview".to_string(), overview);

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

async fn get_identity_actor_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let checks = identity_actor_checks_json(state.config());
    let is_valid = checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(true));
    object.insert("status".to_string(), json!("ok"));
    object.insert("actor_valid".to_string(), json!(is_valid));

    (StatusCode::OK, Json(body)).into_response()
}

async fn validate_identity_actors(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let store = {
        let store = state.inner.identity_binding_store.read().await;
        store.clone()
    };
    let approval_state = load_identity_binding_revision_approval_state(state.config());
    let checks = identity_actor_checks_json(state.config());
    let status = checks
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("actor_header_missing");
    let is_valid = checks
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut body = identity_admin_snapshot_json(state.config(), &store, &approval_state);
    let object = body
        .as_object_mut()
        .expect("identity admin snapshot should be json object");
    object.insert("ok".to_string(), json!(is_valid));
    object.insert("validated".to_string(), json!(true));
    object.insert("valid".to_string(), json!(is_valid));
    object.insert("status".to_string(), json!(status));

    if is_valid {
        return (StatusCode::OK, Json(body)).into_response();
    }

    (StatusCode::CONFLICT, Json(body)).into_response()
}

#[derive(Debug, Deserialize)]
pub struct CreateChatTaskRequest {
    pub user_id: Option<String>,
    pub room_id: Option<String>,
    pub session_id: Option<String>,
    pub org_id: Option<String>,
    pub text: String,
    pub capability_id: Option<String>,
    pub account_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct MatrixMessageRequest {
    pub matrix_user_id: String,
    pub room_id: String,
    pub session_id: Option<String>,
    pub org_id: Option<String>,
    pub message: String,
    pub capability_id: Option<String>,
    pub account_id: Option<String>,
    pub event_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct ConsumerTaskResponse {
    pub task_id: String,
    pub consumer_status: String,
    pub invocation_status: Option<String>,
    pub execution: Option<Value>,
    pub trace: Option<Value>,
    pub request: Option<Value>,
    pub source: Value,
    pub raw: Value,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

async fn create_chat_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateChatTaskRequest>,
) -> Response {
    state.inner.metrics.inc_task_create_requests();

    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let resolved_identity = match resolve_chat_identity(&state, &payload).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let request_fingerprint = build_chat_request_fingerprint(&payload);
    let authorized_session = match authorize_user_session(
        &state,
        &headers,
        &resolved_identity.scope,
        request_fingerprint.as_str(),
    ) {
        Ok(session) => session,
        Err(response) => return response,
    };

    let replay_key = build_chat_replay_key(&payload);
    if let Some(response) = replay_cached_response(&state, replay_key.as_deref()).await {
        return response;
    }

    if let Err(response) = enforce_rate_limit(
        &state,
        build_chat_rate_limit_key(&payload),
        state.config().rate_limit_max_requests,
        "consumer_entry_chat_task_rate_limited",
        RateLimitBucketKind::SourceScope,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_chat_user_rate_limit_key(&payload),
        state.config().rate_limit_user_max_requests,
        "consumer_entry_chat_user_rate_limited",
        RateLimitBucketKind::User,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_chat_room_rate_limit_key(&payload),
        state.config().rate_limit_room_max_requests,
        "consumer_entry_chat_room_rate_limited",
        RateLimitBucketKind::Room,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_chat_session_rate_limit_key(&payload),
        state.config().rate_limit_session_max_requests,
        "consumer_entry_chat_session_rate_limited",
        RateLimitBucketKind::Session,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_chat_org_rate_limit_key(&payload),
        state.config().rate_limit_org_max_requests,
        "consumer_entry_chat_org_rate_limited",
        RateLimitBucketKind::Org,
    )
    .await
    {
        return response;
    }

    let prompt = match validate_text_payload(&payload.text, state.config().max_text_chars) {
        Ok(prompt) => prompt,
        Err(response) => return response,
    };

    let resolved_account_id = resolved_identity.scope.account_id.clone();
    let source = json!({
        "kind": "chat_task",
        "identity_scope": resolved_identity.scope,
        "identity_resolution": resolved_identity.resolution,
        "user_id": payload.user_id,
        "room_id": payload.room_id,
        "session_id": payload.session_id,
        "org_id": payload.org_id,
        "text": prompt,
        "idempotency_key": payload.idempotency_key,
        "metadata": payload.metadata,
        "session_auth": authorized_session,
    });

    let response = match forward_to_cex_task(
        state.clone(),
        payload.capability_id,
        resolved_account_id,
        source,
        prompt,
    )
    .await
    {
        Ok(response) => response,
        Err(response) => return response,
    };

    remember_replay_response(&state, replay_key.as_deref(), &response).await;
    (StatusCode::ACCEPTED, Json(response)).into_response()
}

async fn create_matrix_message_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<MatrixMessageRequest>,
) -> Response {
    state.inner.metrics.inc_task_create_requests();

    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let resolved_identity = match resolve_matrix_identity(&state, &payload).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let request_fingerprint = build_matrix_request_fingerprint(&payload);
    let authorized_session = match authorize_user_session(
        &state,
        &headers,
        &resolved_identity.scope,
        request_fingerprint.as_str(),
    ) {
        Ok(session) => session,
        Err(response) => return response,
    };

    let replay_key = build_matrix_replay_key(&payload);
    if let Some(response) = replay_cached_response(&state, replay_key.as_deref()).await {
        return response;
    }

    if let Err(response) = enforce_rate_limit(
        &state,
        build_matrix_rate_limit_key(&payload),
        state.config().rate_limit_max_requests,
        "consumer_entry_matrix_message_rate_limited",
        RateLimitBucketKind::SourceScope,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        Some(build_matrix_user_rate_limit_key(&payload)),
        state.config().rate_limit_user_max_requests,
        "consumer_entry_matrix_user_rate_limited",
        RateLimitBucketKind::User,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        Some(build_matrix_room_rate_limit_key(&payload)),
        state.config().rate_limit_room_max_requests,
        "consumer_entry_matrix_room_rate_limited",
        RateLimitBucketKind::Room,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_matrix_session_rate_limit_key(&payload),
        state.config().rate_limit_session_max_requests,
        "consumer_entry_matrix_session_rate_limited",
        RateLimitBucketKind::Session,
    )
    .await
    {
        return response;
    }

    if let Err(response) = enforce_optional_rate_limit(
        &state,
        build_matrix_org_rate_limit_key(&payload),
        state.config().rate_limit_org_max_requests,
        "consumer_entry_matrix_org_rate_limited",
        RateLimitBucketKind::Org,
    )
    .await
    {
        return response;
    }

    let prompt = match validate_text_payload(&payload.message, state.config().max_text_chars) {
        Ok(prompt) => prompt,
        Err(response) => return response,
    };

    let resolved_account_id = resolved_identity.scope.account_id.clone();
    let source = json!({
        "kind": "matrix_message",
        "identity_scope": resolved_identity.scope,
        "identity_resolution": resolved_identity.resolution,
        "matrix_user_id": payload.matrix_user_id,
        "room_id": payload.room_id,
        "session_id": payload.session_id,
        "org_id": payload.org_id,
        "event_id": payload.event_id,
        "idempotency_key": payload.idempotency_key,
        "metadata": payload.metadata,
        "session_auth": authorized_session,
    });

    let response = match forward_to_cex_task(
        state.clone(),
        payload.capability_id,
        resolved_account_id,
        source,
        prompt,
    )
    .await
    {
        Ok(response) => response,
        Err(response) => return response,
    };

    remember_replay_response(&state, replay_key.as_deref(), &response).await;
    (StatusCode::ACCEPTED, Json(response)).into_response()
}

async fn forward_to_cex_task(
    state: AppState,
    capability_id: Option<String>,
    account_id: Option<String>,
    source: Value,
    prompt: String,
) -> Result<ConsumerTaskResponse, Response> {
    let capability_id = capability_id
        .or_else(|| state.config().default_capability_id.clone())
        .filter(|v| !v.trim().is_empty());

    let account_id = account_id
        .or_else(|| state.config().default_account_id.clone())
        .filter(|v| !v.trim().is_empty());

    let mut body = json!({
        "prompt": prompt,
    });

    if let Some(capability_id) = capability_id {
        body["capability_id"] = Value::String(capability_id);
    }
    if let Some(account_id) = account_id {
        body["account_id"] = Value::String(account_id);
    }

    let url = format!(
        "{}/v1/invocations",
        state.config().cex_gateway_base_url.trim_end_matches('/')
    );

    let response = match state
        .inner
        .http
        .post(url)
        .header("x-api-key", state.config().cex_gateway_api_key.clone())
        .json(&body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(ErrorBody {
                    error: format!("failed to reach cex gateway: {err}"),
                }),
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
                Json(ErrorBody {
                    error: format!("cex gateway returned non-json response: {err}"),
                }),
            )
                .into_response())
        }
    };

    if !status.is_success() {
        return Err((status, Json(value)).into_response());
    }

    let task_id = value
        .get("invocation_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let invocation_status = value
        .get("status")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let consumer_status = invocation_status
        .as_deref()
        .map(project_consumer_status)
        .unwrap_or("received")
        .to_string();

    Ok(ConsumerTaskResponse {
        task_id,
        consumer_status,
        invocation_status,
        execution: value.get("execution").cloned(),
        trace: value.get("trace").cloned(),
        request: value.get("request").cloned(),
        source,
        raw: value,
    })
}

async fn get_chat_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    state.inner.metrics.inc_task_lookup_requests();

    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let url = format!(
        "{}/v1/invocations/{}",
        state.config().cex_gateway_base_url.trim_end_matches('/'),
        id
    );

    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string)
        .unwrap_or_else(|| state.config().cex_gateway_api_key.clone());

    let response = match state
        .inner
        .http
        .get(url)
        .header("x-api-key", api_key)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorBody {
                    error: format!("failed to reach cex gateway: {err}"),
                }),
            )
                .into_response()
        }
    };

    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorBody {
                    error: format!("cex gateway returned non-json response: {err}"),
                }),
            )
                .into_response()
        }
    };

    if !status.is_success() {
        return (status, Json(value)).into_response();
    }

    let invocation_status = value
        .get("status")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    (
        StatusCode::OK,
        Json(ConsumerTaskResponse {
            task_id: id,
            consumer_status: invocation_status
                .as_deref()
                .map(project_consumer_status)
                .unwrap_or("received")
                .to_string(),
            invocation_status,
            execution: value.get("execution").cloned(),
            trace: value.get("trace").cloned(),
            request: value.get("request").cloned(),
            source: json!({ "kind": "task_lookup" }),
            raw: value,
        }),
    )
        .into_response()
}

fn authorize_ingress(headers: &HeaderMap, config: &ConsumerEntryConfig) -> Result<(), Response> {
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

fn normalize_identity_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_request_fingerprint_value(value: Option<&str>) -> String {
    normalize_identity_value(value).unwrap_or_default()
}

fn normalize_request_fingerprint_text(value: &str) -> String {
    value.trim().to_string()
}

fn build_request_fingerprint(parts: &[String]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0x1f]);
    }
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn build_chat_request_fingerprint(payload: &CreateChatTaskRequest) -> String {
    build_request_fingerprint(&[
        "chat_task".to_string(),
        normalize_request_fingerprint_value(payload.user_id.as_deref()),
        normalize_request_fingerprint_value(payload.room_id.as_deref()),
        normalize_request_fingerprint_value(payload.session_id.as_deref()),
        normalize_request_fingerprint_value(payload.org_id.as_deref()),
        normalize_request_fingerprint_value(payload.account_id.as_deref()),
        normalize_request_fingerprint_value(payload.capability_id.as_deref()),
        normalize_request_fingerprint_value(payload.idempotency_key.as_deref()),
        normalize_request_fingerprint_text(&payload.text),
    ])
}

fn build_matrix_request_fingerprint(payload: &MatrixMessageRequest) -> String {
    build_request_fingerprint(&[
        "matrix_message".to_string(),
        normalize_request_fingerprint_value(Some(payload.matrix_user_id.as_str())),
        normalize_request_fingerprint_value(Some(payload.room_id.as_str())),
        normalize_request_fingerprint_value(payload.session_id.as_deref()),
        normalize_request_fingerprint_value(payload.org_id.as_deref()),
        normalize_request_fingerprint_value(payload.account_id.as_deref()),
        normalize_request_fingerprint_value(payload.capability_id.as_deref()),
        normalize_request_fingerprint_value(payload.event_id.as_deref()),
        normalize_request_fingerprint_value(payload.idempotency_key.as_deref()),
        normalize_request_fingerprint_text(&payload.message),
    ])
}

fn verify_session_auth_claim_field(
    field_name: &str,
    claimed: Option<&str>,
    actual: Option<&str>,
) -> Result<(), Response> {
    let claimed = normalize_identity_value(claimed);
    if claimed.is_none() {
        return Ok(());
    }

    let actual = normalize_identity_value(actual);
    if claimed == actual {
        return Ok(());
    }

    Err((
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "signed session claim mismatch",
            "field": field_name,
            "claimed": claimed,
            "actual": actual,
        })),
    )
        .into_response())
}

fn sign_user_session_assertion(assertion_b64: &str, secret: &str) -> Result<String, Response> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "invalid session auth secret configuration"
            })),
        )
            .into_response()
    })?;
    mac.update(assertion_b64.as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

fn authorize_user_session(
    state: &AppState,
    headers: &HeaderMap,
    scope: &IdentityScope,
    expected_request_fingerprint: &str,
) -> Result<Option<AuthorizedUserSession>, Response> {
    let assertion = headers
        .get(USER_SESSION_ASSERTION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let signature = headers
        .get(USER_SESSION_SIGNATURE_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let provided = assertion.is_some() || signature.is_some();

    if !state.config().require_session_auth && !provided {
        return Ok(None);
    }

    let assertion = assertion.ok_or_else(|| {
        state.inner.metrics.inc_session_auth_failures();
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "missing signed user session assertion",
                "assertion_header": USER_SESSION_ASSERTION_HEADER,
                "signature_header": USER_SESSION_SIGNATURE_HEADER,
            })),
        )
            .into_response()
    })?;
    let signature = signature.ok_or_else(|| {
        state.inner.metrics.inc_session_auth_failures();
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "missing signed user session signature",
                "assertion_header": USER_SESSION_ASSERTION_HEADER,
                "signature_header": USER_SESSION_SIGNATURE_HEADER,
            })),
        )
            .into_response()
    })?;

    let assertion_bytes = match URL_SAFE_NO_PAD.decode(assertion) {
        Ok(value) => value,
        Err(_) => {
            state.inner.metrics.inc_session_auth_failures();
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "session assertion must be base64url encoded JSON"
                })),
            )
                .into_response());
        }
    };
    let claims = match serde_json::from_slice::<UserSessionAuthClaims>(&assertion_bytes) {
        Ok(value) => value,
        Err(err) => {
            state.inner.metrics.inc_session_auth_failures();
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("invalid session assertion payload: {err}")
                })),
            )
                .into_response());
        }
    };

    let issuer = normalize_identity_value(Some(claims.issuer.as_str())).ok_or_else(|| {
        state.inner.metrics.inc_session_auth_failures();
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "signed session assertion is missing issuer"
            })),
        )
            .into_response()
    })?;

    let key_id = normalize_identity_value(claims.key_id.as_deref());
    let session_auth_registry_state = session_auth_issuer_registry_runtime_state(state);

    let secret = if let Some(registry_entry) = session_auth_registry_state.registry.get(&issuer) {
        let key_id = key_id.clone().ok_or_else(|| {
            state.inner.metrics.inc_session_auth_failures();
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "signed session assertion is missing key_id for issuer registry verification",
                    "issuer": issuer,
                })),
            )
                .into_response()
        })?;

        registry_entry
            .keys
            .get(&key_id)
            .map(|value| value.as_str())
            .ok_or_else(|| {
                state.inner.metrics.inc_session_auth_failures();
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "error": "unknown signed session key_id for issuer registry verification",
                        "issuer": issuer,
                        "key_id": key_id,
                    })),
                )
                    .into_response()
            })?
    } else if let Some(issuer_keys) = state.config().session_auth_issuer_keys.get(&issuer) {
        let key_id = key_id.clone().ok_or_else(|| {
            state.inner.metrics.inc_session_auth_failures();
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "signed session assertion is missing key_id for issuer-managed keys",
                    "issuer": issuer,
                })),
            )
                .into_response()
        })?;

        issuer_keys.get(&key_id).map(|value| value.as_str()).ok_or_else(|| {
            state.inner.metrics.inc_session_auth_failures();
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "unknown signed session key_id for issuer-managed keys",
                    "issuer": issuer,
                    "key_id": key_id,
                })),
            )
                .into_response()
        })?
    } else {
        state
            .config()
            .session_auth_issuer_secrets
            .get(&issuer)
            .map(|value| value.as_str())
            .or_else(|| {
                state
                    .config()
                    .session_auth_secret
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
            })
            .ok_or_else(|| {
                state.inner.metrics.inc_session_auth_failures();
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": "session auth is required but no matching issuer secret is configured",
                        "issuer": issuer,
                    })),
                )
                    .into_response()
            })?
    };

    let expected_signature = match sign_user_session_assertion(assertion, secret) {
        Ok(value) => value,
        Err(response) => {
            state.inner.metrics.inc_session_auth_failures();
            return Err(response);
        }
    };
    if expected_signature != signature {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "invalid signed user session signature",
                "assertion_header": USER_SESSION_ASSERTION_HEADER,
                "signature_header": USER_SESSION_SIGNATURE_HEADER,
            })),
        )
            .into_response());
    }

    if !state.config().session_auth_allowed_issuers.is_empty()
        && !state
            .config()
            .session_auth_allowed_issuers
            .iter()
            .any(|allowed| allowed == &issuer)
    {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "signed session issuer is not allowed",
                "issuer": issuer,
                "allowed_issuers": state.config().session_auth_allowed_issuers,
            })),
        )
            .into_response());
    }

    if let Some(expected_audience) = state.config().session_auth_expected_audience.as_deref() {
        let actual_audience = normalize_identity_value(claims.audience.as_deref());
        if actual_audience.as_deref() != Some(expected_audience) {
            state.inner.metrics.inc_session_auth_failures();
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "signed session audience mismatch",
                    "claimed": actual_audience,
                    "expected": expected_audience,
                })),
            )
                .into_response());
        }
    }

    let claimed_request_fingerprint = normalize_identity_value(claims.request_fingerprint.as_deref())
        .ok_or_else(|| {
            state.inner.metrics.inc_session_auth_failures();
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "signed session assertion is missing request_fingerprint"
                })),
            )
                .into_response()
        })?;
    if claimed_request_fingerprint != expected_request_fingerprint {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "signed session request_fingerprint mismatch",
                "claimed": claimed_request_fingerprint,
                "expected": expected_request_fingerprint,
            })),
        )
            .into_response());
    }

    let now_epoch = Utc::now().timestamp();
    let max_skew = state.config().session_auth_max_clock_skew_secs as i64;
    if claims.issued_at_epoch > now_epoch + max_skew {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "signed session assertion is not valid yet",
                "issued_at_epoch": claims.issued_at_epoch,
                "now_epoch": now_epoch,
            })),
        )
            .into_response());
    }
    if claims.expires_at_epoch < now_epoch - max_skew {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "signed session assertion has expired",
                "expires_at_epoch": claims.expires_at_epoch,
                "now_epoch": now_epoch,
            })),
        )
            .into_response());
    }
    if claims.expires_at_epoch < claims.issued_at_epoch {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "signed session assertion has invalid lifetime",
                "issued_at_epoch": claims.issued_at_epoch,
                "expires_at_epoch": claims.expires_at_epoch,
            })),
        )
            .into_response());
    }
    if claims
        .expires_at_epoch
        .saturating_sub(claims.issued_at_epoch)
        > state.config().session_auth_max_ttl_secs as i64
    {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "signed session assertion ttl exceeds configured maximum",
                "issued_at_epoch": claims.issued_at_epoch,
                "expires_at_epoch": claims.expires_at_epoch,
                "max_ttl_secs": state.config().session_auth_max_ttl_secs,
            })),
        )
            .into_response());
    }

    if normalize_identity_value(Some(claims.source_kind.as_str()))
        != normalize_identity_value(Some(scope.source_kind))
    {
        state.inner.metrics.inc_session_auth_failures();
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "signed session source_kind mismatch",
                "claimed": claims.source_kind,
                "actual": scope.source_kind,
            })),
        )
            .into_response());
    }

    if let Err(response) = verify_session_auth_claim_field(
        "subject",
        Some(claims.subject.as_str()),
        scope.user_id.as_deref(),
    ) {
        state.inner.metrics.inc_session_auth_failures();
        return Err(response);
    }
    if let Err(response) = verify_session_auth_claim_field(
        "room_id",
        claims.room_id.as_deref(),
        scope.room_id.as_deref(),
    ) {
        state.inner.metrics.inc_session_auth_failures();
        return Err(response);
    }
    if let Err(response) = verify_session_auth_claim_field(
        "session_id",
        claims.session_id.as_deref(),
        scope.session_id.as_deref(),
    ) {
        state.inner.metrics.inc_session_auth_failures();
        return Err(response);
    }
    if let Err(response) = verify_session_auth_claim_field(
        "org_id",
        claims.org_id.as_deref(),
        scope.org_id.as_deref(),
    ) {
        state.inner.metrics.inc_session_auth_failures();
        return Err(response);
    }
    if let Err(response) = verify_session_auth_claim_field(
        "account_id",
        claims.account_id.as_deref(),
        scope.account_id.as_deref(),
    ) {
        state.inner.metrics.inc_session_auth_failures();
        return Err(response);
    }

    state.inner.metrics.inc_session_auth_successes();
    Ok(Some(AuthorizedUserSession {
        claims,
        assertion_header: USER_SESSION_ASSERTION_HEADER,
        signature_header: USER_SESSION_SIGNATURE_HEADER,
    }))
}

async fn resolve_chat_identity(
    state: &AppState,
    payload: &CreateChatTaskRequest,
) -> Result<ResolvedIdentity, Response> {
    let requested = build_chat_identity_scope(payload);
    let store = state.inner.identity_binding_store.read().await;
    let binding = store
        .bindings
        .chat_users
        .get(
            payload
                .user_id
                .as_deref()
                .map(str::trim)
                .unwrap_or_default(),
        )
        .cloned();
    let metadata = store.metadata.clone();
    let product_users = store.product_users.clone();
    drop(store);

    resolve_identity_scope(
        state,
        requested,
        payload.user_id.as_deref(),
        binding.as_ref(),
        &metadata,
        &product_users,
    )
}

async fn resolve_matrix_identity(
    state: &AppState,
    payload: &MatrixMessageRequest,
) -> Result<ResolvedIdentity, Response> {
    let requested = build_matrix_identity_scope(payload);
    let store = state.inner.identity_binding_store.read().await;
    let binding = store
        .bindings
        .matrix_users
        .get(payload.matrix_user_id.trim())
        .cloned();
    let metadata = store.metadata.clone();
    let product_users = store.product_users.clone();
    drop(store);

    resolve_identity_scope(
        state,
        requested,
        Some(payload.matrix_user_id.as_str()),
        binding.as_ref(),
        &metadata,
        &product_users,
    )
}

fn resolve_identity_scope(
    state: &AppState,
    mut requested: IdentityScope,
    binding_subject: Option<&str>,
    binding: Option<&IdentityBindingEntry>,
    metadata: &IdentityBindingMetadata,
    product_users: &HashMap<String, ProductUserIdentity>,
) -> Result<ResolvedIdentity, Response> {
    let binding_subject = binding_subject
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string);
    let base_resolution = IdentityResolution {
        matched: false,
        required: state.config().require_identity_binding,
        binding_subject: binding_subject.clone(),
        binding_source_format: metadata.format.clone(),
        binding_version: metadata.version,
        binding_revision: metadata.revision.clone(),
        product_user_id: None,
        binding_source_kind: "none".to_string(),
    };

    if let Some(binding) = binding {
        let product_user_id = binding
            .product_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let product_user = match product_user_id.as_deref() {
            Some(product_user_id) => match product_users.get(product_user_id) {
                Some(product_user) => Some(product_user),
                None => {
                    state.inner.metrics.inc_identity_binding_failures();
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "error": "identity binding references unknown product_user_id",
                            "subject": binding_subject,
                            "product_user_id": product_user_id,
                            "binding_source_format": metadata.format.clone(),
                            "binding_version": metadata.version,
                            "binding_revision": metadata.revision.clone(),
                        })),
                    )
                        .into_response());
                }
            },
            None => None,
        };

        let bound_org_id = match merge_bound_identity_source(
            binding.org_id.clone(),
            product_user.and_then(|value| value.org_id.clone()),
            "org_id",
            product_user_id.as_deref(),
        ) {
            Ok(value) => value,
            Err(response) => {
                state.inner.metrics.inc_identity_binding_failures();
                return Err(response);
            }
        };
        let bound_account_id = match merge_bound_identity_source(
            binding.account_id.clone(),
            product_user.and_then(|value| value.account_id.clone()),
            "account_id",
            product_user_id.as_deref(),
        ) {
            Ok(value) => value,
            Err(response) => {
                state.inner.metrics.inc_identity_binding_failures();
                return Err(response);
            }
        };

        requested.org_id = match merge_identity_field(requested.org_id, bound_org_id, "org_id") {
            Ok(value) => value,
            Err(response) => {
                state.inner.metrics.inc_identity_binding_failures();
                return Err(response);
            }
        };
        requested.account_id =
            match merge_identity_field(requested.account_id, bound_account_id, "account_id") {
                Ok(value) => value,
                Err(response) => {
                    state.inner.metrics.inc_identity_binding_failures();
                    return Err(response);
                }
            };
        state.inner.metrics.inc_identity_binding_matches();
        return Ok(ResolvedIdentity {
            scope: requested,
            resolution: IdentityResolution {
                matched: true,
                product_user_id: product_user_id.clone(),
                binding_source_kind: if product_user_id.is_some() {
                    "product_user_registry".to_string()
                } else {
                    "inline_binding".to_string()
                },
                ..base_resolution
            },
        });
    }

    if state.config().require_identity_binding {
        state.inner.metrics.inc_identity_binding_failures();
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "identity binding required",
                "subject": binding_subject,
                "source_kind": requested.source_kind,
                "binding_source_format": metadata.format.clone(),
                "binding_version": metadata.version,
                "binding_revision": metadata.revision.clone(),
            })),
        )
            .into_response());
    }

    Ok(ResolvedIdentity {
        scope: requested,
        resolution: base_resolution,
    })
}

fn merge_bound_identity_source(
    inline: Option<String>,
    registry: Option<String>,
    field_name: &str,
    product_user_id: Option<&str>,
) -> Result<Option<String>, Response> {
    match (
        inline
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        registry
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
    ) {
        (Some(inline), Some(registry)) if inline != registry => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "identity source-of-truth conflict",
                "field": field_name,
                "product_user_id": product_user_id,
                "inline_value": inline,
                "registry_value": registry,
            })),
        )
            .into_response()),
        (Some(inline), Some(_)) => Ok(Some(inline)),
        (None, Some(registry)) => Ok(Some(registry)),
        (Some(inline), None) => Ok(Some(inline)),
        (None, None) => Ok(None),
    }
}

fn merge_identity_field(
    requested: Option<String>,
    bound: Option<String>,
    field_name: &str,
) -> Result<Option<String>, Response> {
    match (
        requested
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        bound
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
    ) {
        (Some(requested), Some(bound)) if requested != bound => Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "identity binding mismatch",
                "field": field_name,
                "requested": requested,
                "bound": bound,
            })),
        )
            .into_response()),
        (Some(requested), _) => Ok(Some(requested)),
        (None, Some(bound)) => Ok(Some(bound)),
        (None, None) => Ok(None),
    }
}

async fn replay_cached_response(state: &AppState, replay_key: Option<&str>) -> Option<Response> {
    let Some(replay_key) = replay_key.map(str::trim).filter(|v| !v.is_empty()) else {
        return None;
    };

    let now_epoch = Utc::now().timestamp();
    let ttl_secs = state.config().replay_window_secs;
    let max_size = state.config().replay_cache_size;
    let mut cache = state.inner.replay_cache.lock().await;
    prune_replay_cache(&mut cache, now_epoch, ttl_secs, max_size);

    let existing = cache.seen.get(replay_key).cloned();
    let Some(existing) = existing else {
        return None;
    };

    state.inner.metrics.inc_replay_hits();
    if let Some(response) = existing.response {
        return Some((StatusCode::ACCEPTED, Json(response)).into_response());
    }

    Some(
        (
            StatusCode::OK,
            Json(json!({
                "accepted": false,
                "action": "duplicate_request",
                "replay_key": replay_key,
                "consumer_status": "duplicate_ignored"
            })),
        )
            .into_response(),
    )
}

async fn remember_replay_response(
    state: &AppState,
    replay_key: Option<&str>,
    response: &ConsumerTaskResponse,
) {
    let Some(replay_key) = replay_key.map(str::trim).filter(|v| !v.is_empty()) else {
        return;
    };

    let now_epoch = Utc::now().timestamp();
    let ttl_secs = state.config().replay_window_secs;
    let max_size = state.config().replay_cache_size;
    let mut cache = state.inner.replay_cache.lock().await;
    prune_replay_cache(&mut cache, now_epoch, ttl_secs, max_size);
    cache.seen.insert(
        replay_key.to_string(),
        ReplayEntry {
            seen_at_epoch: now_epoch,
            response: serde_json::to_value(response).ok(),
        },
    );
    cache.order.push_back(replay_key.to_string());
    prune_replay_cache(&mut cache, now_epoch, ttl_secs, max_size);
    persist_replay_cache(&cache, state.config());
}

fn prune_replay_cache(cache: &mut ReplayCache, now_epoch: i64, ttl_secs: u64, max_size: usize) {
    loop {
        let Some(front) = cache.order.front().cloned() else {
            break;
        };

        let should_drop = cache
            .seen
            .get(&front)
            .map(|entry| now_epoch.saturating_sub(entry.seen_at_epoch) >= ttl_secs as i64)
            .unwrap_or(true)
            || cache.seen.len() > max_size;

        if !should_drop {
            break;
        }

        cache.order.pop_front();
        cache.seen.remove(&front);
    }
}

fn build_chat_replay_key(payload: &CreateChatTaskRequest) -> Option<String> {
    let key = payload.idempotency_key.as_deref()?.trim();
    if key.is_empty() {
        return None;
    }

    Some(format!(
        "chat:{}:{}:{}",
        payload.user_id.as_deref().unwrap_or("anonymous"),
        payload.room_id.as_deref().unwrap_or("global"),
        key
    ))
}

fn build_matrix_replay_key(payload: &MatrixMessageRequest) -> Option<String> {
    if let Some(event_id) = payload
        .event_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return Some(format!("matrix-event:{}", event_id));
    }

    let key = payload.idempotency_key.as_deref()?.trim();
    if key.is_empty() {
        return None;
    }

    Some(format!(
        "matrix:{}:{}:{}",
        payload.matrix_user_id, payload.room_id, key
    ))
}

async fn enforce_optional_rate_limit(
    state: &AppState,
    key: Option<String>,
    max_requests: usize,
    error_code: &str,
    bucket_kind: RateLimitBucketKind,
) -> Result<(), Response> {
    let Some(key) = key else {
        return Ok(());
    };
    if max_requests == 0 {
        return Ok(());
    }

    enforce_rate_limit(state, key, max_requests, error_code, bucket_kind).await
}

async fn enforce_rate_limit(
    state: &AppState,
    key: String,
    max_requests: usize,
    error_code: &str,
    bucket_kind: RateLimitBucketKind,
) -> Result<(), Response> {
    let now_epoch = Utc::now().timestamp();
    let max_entries = max_rate_limit_entries(state.config());

    let mut rate_limits = state.inner.rate_limits.lock().await;
    let entries = rate_limits.seen.entry(key).or_default();
    let modified = prune_rate_limit_entries(
        entries,
        now_epoch,
        state.config().rate_limit_window_secs,
        max_entries,
    );

    if entries.len() >= max_requests {
        if modified {
            persist_rate_limit_cache(&rate_limits, state.config());
        }
        state.inner.metrics.inc_rate_limited_requests();
        match bucket_kind {
            RateLimitBucketKind::SourceScope => {
                state.inner.metrics.inc_rate_limited_source_scope_requests()
            }
            RateLimitBucketKind::User => state.inner.metrics.inc_rate_limited_user_requests(),
            RateLimitBucketKind::Room => state.inner.metrics.inc_rate_limited_room_requests(),
            RateLimitBucketKind::Session => state.inner.metrics.inc_rate_limited_session_requests(),
            RateLimitBucketKind::Org => state.inner.metrics.inc_rate_limited_org_requests(),
        }
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "error": error_code,
                "rate_limit_window_secs": state.config().rate_limit_window_secs,
                "rate_limit_max_requests": max_requests,
                "rate_limit_bucket": rate_limit_bucket_name(bucket_kind),
            })),
        )
            .into_response());
    }

    entries.push_back(now_epoch);
    prune_rate_limit_entries(
        entries,
        now_epoch,
        state.config().rate_limit_window_secs,
        max_entries,
    );
    persist_rate_limit_cache(&rate_limits, state.config());
    Ok(())
}

fn rate_limit_bucket_name(bucket_kind: RateLimitBucketKind) -> &'static str {
    match bucket_kind {
        RateLimitBucketKind::SourceScope => "source_scope",
        RateLimitBucketKind::User => "user",
        RateLimitBucketKind::Room => "room",
        RateLimitBucketKind::Session => "session",
        RateLimitBucketKind::Org => "org",
    }
}

fn build_chat_identity_scope(payload: &CreateChatTaskRequest) -> IdentityScope {
    IdentityScope {
        source_kind: "chat_task",
        user_id: payload.user_id.clone(),
        room_id: payload.room_id.clone(),
        session_id: payload.session_id.clone(),
        org_id: payload.org_id.clone(),
        account_id: payload.account_id.clone(),
    }
}

fn build_matrix_identity_scope(payload: &MatrixMessageRequest) -> IdentityScope {
    IdentityScope {
        source_kind: "matrix_message",
        user_id: Some(payload.matrix_user_id.clone()),
        room_id: Some(payload.room_id.clone()),
        session_id: payload.session_id.clone(),
        org_id: payload.org_id.clone(),
        account_id: payload.account_id.clone(),
    }
}

fn build_chat_rate_limit_key(payload: &CreateChatTaskRequest) -> String {
    format!(
        "chat:{}:{}",
        payload.user_id.as_deref().unwrap_or("anonymous"),
        payload.room_id.as_deref().unwrap_or("global")
    )
}

fn build_chat_user_rate_limit_key(payload: &CreateChatTaskRequest) -> Option<String> {
    payload
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|user_id| format!("chat-user:{}", user_id))
}

fn build_chat_room_rate_limit_key(payload: &CreateChatTaskRequest) -> Option<String> {
    payload
        .room_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|room_id| format!("chat-room:{}", room_id))
}

fn build_chat_session_rate_limit_key(payload: &CreateChatTaskRequest) -> Option<String> {
    payload
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|session_id| format!("chat-session:{}", session_id))
}

fn build_chat_org_rate_limit_key(payload: &CreateChatTaskRequest) -> Option<String> {
    payload
        .org_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|org_id| format!("chat-org:{}", org_id))
}

fn build_matrix_rate_limit_key(payload: &MatrixMessageRequest) -> String {
    format!("matrix:{}:{}", payload.matrix_user_id, payload.room_id)
}

fn build_matrix_user_rate_limit_key(payload: &MatrixMessageRequest) -> String {
    format!("matrix-user:{}", payload.matrix_user_id)
}

fn build_matrix_room_rate_limit_key(payload: &MatrixMessageRequest) -> String {
    format!("matrix-room:{}", payload.room_id)
}

fn build_matrix_session_rate_limit_key(payload: &MatrixMessageRequest) -> Option<String> {
    payload
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|session_id| format!("matrix-session:{}", session_id))
}

fn build_matrix_org_rate_limit_key(payload: &MatrixMessageRequest) -> Option<String> {
    payload
        .org_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|org_id| format!("matrix-org:{}", org_id))
}

fn validate_text_payload(raw: &str, max_text_chars: usize) -> Result<String, Response> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "text payload must not be empty" })),
        )
            .into_response());
    }

    let length = trimmed.chars().count();
    if length > max_text_chars {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "text payload too large",
                "max_text_chars": max_text_chars,
                "received_text_chars": length,
            })),
        )
            .into_response());
    }

    Ok(trimmed.to_string())
}

fn project_consumer_status(status: &str) -> &'static str {
    match status {
        "Created" => "received",
        "Queued" => "queued",
        "AwaitingApproval" => "waiting_for_confirmation",
        "Approved" | "Dispatching" | "Running" => "processing",
        "Succeeded" => "done",
        "Failed" => "failed",
        "Refunded" => "refunded",
        _ => "received",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        authorize_user_session, build_chat_identity_scope, build_chat_org_rate_limit_key,
        build_chat_rate_limit_key, build_chat_replay_key, build_chat_request_fingerprint,
        build_chat_room_rate_limit_key,
        build_chat_session_rate_limit_key, build_chat_user_rate_limit_key,
        build_matrix_identity_scope, build_matrix_org_rate_limit_key,
        build_matrix_rate_limit_key, build_matrix_replay_key, build_matrix_room_rate_limit_key,
        build_matrix_session_rate_limit_key, build_matrix_user_rate_limit_key, build_router,
        evaluate_identity_binding_reload_governance, load_identity_binding_revision_approval_state,
        load_identity_binding_store, load_rate_limit_cache,
        load_session_auth_issuer_registry, load_session_auth_issuer_registry_revision_approval_state,
        parse_csv_list, project_consumer_status, prune_rate_limit_cache, resolve_chat_identity,
        session_auth_issuer_registry_active_key_diff_json,
        sign_user_session_assertion, validate_text_payload, AppState, AppStateInner,
        ConsumerEntryConfig, ConsumerEntryMetrics, CreateChatTaskRequest,
        IdentityBindingAuditState, IdentityBindingEntry, IdentityBindingMetadata,
        IdentityBindingRevisionApprovalState, IdentityBindingStore, IdentityBindings,
        MatrixMessageRequest, ProductUserIdentity, RateLimitCache, ReplayCache, RuntimeProfile,
        SessionAuthIssuerRegistryIssuer, SessionAuthIssuerRegistryMetadata,
        SessionAuthIssuerRegistryRuntimeState, UserSessionAuthClaims, DEFAULT_MAX_TEXT_CHARS,
        USER_SESSION_ASSERTION_HEADER, USER_SESSION_SIGNATURE_HEADER,
    };
    use axum::{
        body::{to_bytes, Body},
        http::{HeaderMap, Request, StatusCode},
    };
    use base64::Engine as _;
    use chrono::Utc;
    use reqwest::Client;
    use serde_json::Value;
    use std::{
        collections::{HashMap, VecDeque},
        sync::{Arc, RwLock as StdRwLock},
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::{Mutex, RwLock};
    use tower::ServiceExt;

    #[test]
    fn projects_core_runtime_states() {
        assert_eq!(project_consumer_status("Created"), "received");
        assert_eq!(project_consumer_status("Queued"), "queued");
        assert_eq!(
            project_consumer_status("AwaitingApproval"),
            "waiting_for_confirmation"
        );
        assert_eq!(project_consumer_status("Running"), "processing");
        assert_eq!(project_consumer_status("Succeeded"), "done");
        assert_eq!(project_consumer_status("Failed"), "failed");
        assert_eq!(project_consumer_status("Refunded"), "refunded");
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
    fn builds_stable_rate_limit_keys() {
        let chat = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: Some("req-1".to_string()),
            metadata: None,
        };
        assert_eq!(build_chat_rate_limit_key(&chat), "chat:user-1:room-1");

        let matrix = MatrixMessageRequest {
            matrix_user_id: "@alice:local.dev".to_string(),
            room_id: "!room:local.dev".to_string(),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            message: "hello".to_string(),
            capability_id: None,
            account_id: None,
            event_id: None,
            idempotency_key: Some("msg-1".to_string()),
            metadata: None,
        };
        assert_eq!(
            build_matrix_rate_limit_key(&matrix),
            "matrix:@alice:local.dev:!room:local.dev"
        );
        assert_eq!(
            build_chat_replay_key(&chat).as_deref(),
            Some("chat:user-1:room-1:req-1")
        );
        assert_eq!(
            build_chat_user_rate_limit_key(&chat).as_deref(),
            Some("chat-user:user-1")
        );
        assert_eq!(
            build_chat_room_rate_limit_key(&chat).as_deref(),
            Some("chat-room:room-1")
        );
        assert_eq!(
            build_chat_session_rate_limit_key(&chat).as_deref(),
            Some("chat-session:session-1")
        );
        assert_eq!(
            build_chat_org_rate_limit_key(&chat).as_deref(),
            Some("chat-org:org-1")
        );
        assert_eq!(
            build_matrix_replay_key(&matrix).as_deref(),
            Some("matrix:@alice:local.dev:!room:local.dev:msg-1")
        );
        assert_eq!(
            build_matrix_user_rate_limit_key(&matrix),
            "matrix-user:@alice:local.dev"
        );
        assert_eq!(
            build_matrix_room_rate_limit_key(&matrix),
            "matrix-room:!room:local.dev"
        );
        assert_eq!(
            build_matrix_session_rate_limit_key(&matrix).as_deref(),
            Some("matrix-session:session-1")
        );
        assert_eq!(
            build_matrix_org_rate_limit_key(&matrix).as_deref(),
            Some("matrix-org:org-1")
        );

        let chat_scope = build_chat_identity_scope(&chat);
        assert_eq!(chat_scope.source_kind, "chat_task");
        assert_eq!(chat_scope.org_id.as_deref(), Some("org-1"));

        let matrix_scope = build_matrix_identity_scope(&matrix);
        assert_eq!(matrix_scope.source_kind, "matrix_message");
        assert_eq!(matrix_scope.user_id.as_deref(), Some("@alice:local.dev"));
        assert_eq!(matrix_scope.org_id.as_deref(), Some("org-1"));
    }

    #[test]
    fn matrix_event_id_wins_over_generic_idempotency_key() {
        let matrix = MatrixMessageRequest {
            matrix_user_id: "@alice:local.dev".to_string(),
            room_id: "!room:local.dev".to_string(),
            session_id: None,
            org_id: None,
            message: "hello".to_string(),
            capability_id: None,
            account_id: None,
            event_id: Some("$event-123".to_string()),
            idempotency_key: Some("msg-1".to_string()),
            metadata: None,
        };

        assert_eq!(
            build_matrix_replay_key(&matrix).as_deref(),
            Some("matrix-event:$event-123")
        );
    }

    fn test_config() -> ConsumerEntryConfig {
        ConsumerEntryConfig {
            runtime_profile: RuntimeProfile::LocalDev,
            bind_addr: "127.0.0.1:8090".to_string(),
            cex_gateway_base_url: "http://127.0.0.1:8080".to_string(),
            cex_gateway_api_key: "local-dev-key".to_string(),
            default_capability_id: None,
            default_account_id: None,
            ingress_token: None,
            require_session_auth: false,
            session_auth_secret: None,
            session_auth_issuer_secrets: HashMap::new(),
            session_auth_issuer_keys: HashMap::new(),
            session_auth_issuer_registry_path: None,
            session_auth_issuer_registry: HashMap::new(),
            session_auth_issuer_registry_load_error: None,
            session_auth_issuer_registry_metadata: SessionAuthIssuerRegistryMetadata::default(),
            session_auth_allowed_issuers: Vec::new(),
            session_auth_expected_audience: None,
            session_auth_issuer_registry_approved_revisions_path: None,
            session_auth_issuer_registry_require_approved_revision: false,
            session_auth_issuer_registry_require_actor: false,
            session_auth_issuer_registry_actor_header: "x-session-auth-issuer-registry-actor".to_string(),
            session_auth_issuer_registry_allowed_actors: Vec::new(),
            session_auth_max_clock_skew_secs: 300,
            session_auth_max_ttl_secs: 900,
            identity_bindings_path: None,
            identity_registry_path: None,
            identity_binding_audit_log_path: None,
            identity_binding_approved_revisions_path: None,
            identity_binding_reload_require_revision: false,
            identity_binding_reload_reject_same_revision: false,
            identity_binding_reload_allow_legacy_format: true,
            identity_binding_reload_require_approved_revision: false,
            identity_binding_reload_allow_rollback: true,
            identity_binding_reload_require_actor: false,
            identity_binding_reload_actor_header: "x-identity-binding-actor".to_string(),
            identity_binding_reload_allowed_actors: Vec::new(),
            require_identity_binding: false,
            max_text_chars: DEFAULT_MAX_TEXT_CHARS,
            rate_limit_window_secs: 60,
            rate_limit_max_requests: 30,
            rate_limit_user_max_requests: 0,
            rate_limit_room_max_requests: 0,
            rate_limit_session_max_requests: 0,
            rate_limit_org_max_requests: 0,
            rate_limit_store_path: None,
            replay_window_secs: 600,
            replay_cache_size: 2048,
            replay_store_path: None,
        }
    }

    fn test_state(
        config: ConsumerEntryConfig,
        identity_bindings: IdentityBindings,
        product_users: HashMap<String, ProductUserIdentity>,
    ) -> AppState {
        let session_auth_issuer_registry_state = SessionAuthIssuerRegistryRuntimeState {
            metadata: config.session_auth_issuer_registry_metadata.clone(),
            registry: config.session_auth_issuer_registry.clone(),
        };
        AppState {
            inner: Arc::new(AppStateInner {
                http: Client::new(),
                config,
                identity_binding_store: RwLock::new(IdentityBindingStore {
                    metadata: IdentityBindingMetadata {
                        format: "test".to_string(),
                        version: 1,
                        revision: Some("rev-test".to_string()),
                        source_path: None,
                        source_modified_epoch: None,
                        loaded_at_epoch: Some(1_760_000_000),
                        load_status: "loaded".to_string(),
                        load_error: None,
                    },
                    registry_metadata: IdentityBindingMetadata::default(),
                    bindings: identity_bindings,
                    product_users,
                }),
                identity_binding_audit_state: RwLock::new(IdentityBindingAuditState::default()),
                session_auth_issuer_registry_state: StdRwLock::new(session_auth_issuer_registry_state),
                rate_limits: Mutex::new(RateLimitCache::default()),
                replay_cache: Mutex::new(ReplayCache::default()),
                metrics: ConsumerEntryMetrics::default(),
            }),
        }
    }

    #[test]
    fn local_dev_profile_allows_default_consumer_config() {
        let config = test_config();
        assert!(config.validate_runtime_profile().is_ok());
    }

    #[test]
    fn beta_profile_requires_auth_replay_and_identity_binding() {
        let mut config = test_config();
        config.runtime_profile = RuntimeProfile::Beta;

        let errors = config.validate_runtime_profile().unwrap_err();
        assert!(errors
            .iter()
            .any(|item| item.contains("CONSUMER_ENTRY_INGRESS_TOKEN")));
        assert!(errors
            .iter()
            .any(|item| item.contains("CONSUMER_ENTRY_REQUIRE_SESSION_AUTH=true")));
        assert!(errors
            .iter()
            .any(|item| item.contains("CONSUMER_ENTRY_SESSION_AUTH_SECRET")));
        assert!(errors
            .iter()
            .any(|item| item.contains("CONSUMER_ENTRY_SESSION_AUTH_ALLOWED_ISSUERS")));
        assert!(errors
            .iter()
            .any(|item| item.contains("CONSUMER_ENTRY_SESSION_AUTH_EXPECTED_AUDIENCE")));
        assert!(errors
            .iter()
            .any(|item| item.contains("CONSUMER_ENTRY_REPLAY_STORE_PATH")));
        assert!(errors
            .iter()
            .any(|item| item.contains("CONSUMER_ENTRY_RATE_LIMIT_STORE_PATH")));
        assert!(errors
            .iter()
            .any(|item| item.contains("CONSUMER_ENTRY_IDENTITY_BINDINGS_PATH")));
        assert!(errors
            .iter()
            .any(|item| item.contains("CONSUMER_ENTRY_REQUIRE_IDENTITY_BINDING=true")));
        assert!(errors
            .iter()
            .any(|item| item.contains("non-default CEX_GATEWAY_API_KEY")));
    }

    #[test]
    fn authorizes_matching_signed_user_session() {
        let mut config = test_config();
        config.require_session_auth = true;
        config.session_auth_secret = Some("test-session-secret".to_string());
        config.session_auth_allowed_issuers = vec!["test-suite".to_string()];
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        let state = test_state(config, IdentityBindings::default(), HashMap::new());
        let payload = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: None,
            metadata: None,
        };
        let scope = build_chat_identity_scope(&payload);
        let now_epoch = Utc::now().timestamp();
        let claims = UserSessionAuthClaims {
            version: 1,
            issuer: "test-suite".to_string(),
            key_id: None,
            subject: "user-1".to_string(),
            source_kind: "chat_task".to_string(),
            audience: Some("consumer-entry-api".to_string()),
            request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            account_id: None,
            issued_at_epoch: now_epoch,
            expires_at_epoch: now_epoch + 60,
        };
        let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        let signature = sign_user_session_assertion(&assertion, "test-session-secret").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
        headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

        let authorized = authorize_user_session(
            &state,
            &headers,
            &scope,
            build_chat_request_fingerprint(&payload).as_str(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(authorized.claims.subject, "user-1");
        assert_eq!(authorized.claims.source_kind, "chat_task");
        assert_eq!(authorized.claims.audience.as_deref(), Some("consumer-entry-api"));
    }

    #[test]
    fn authorizes_matching_signed_user_session_with_issuer_specific_secret() {
        let mut config = test_config();
        config.require_session_auth = true;
        config.session_auth_secret = None;
        config
            .session_auth_issuer_secrets
            .insert("issuer-specific".to_string(), "issuer-secret".to_string());
        config.session_auth_allowed_issuers = vec!["issuer-specific".to_string()];
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        let state = test_state(config, IdentityBindings::default(), HashMap::new());
        let payload = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: None,
            metadata: None,
        };
        let scope = build_chat_identity_scope(&payload);
        let now_epoch = Utc::now().timestamp();
        let claims = UserSessionAuthClaims {
            version: 1,
            issuer: "issuer-specific".to_string(),
            key_id: None,
            subject: "user-1".to_string(),
            source_kind: "chat_task".to_string(),
            audience: Some("consumer-entry-api".to_string()),
            request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            account_id: None,
            issued_at_epoch: now_epoch,
            expires_at_epoch: now_epoch + 60,
        };
        let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        let signature = sign_user_session_assertion(&assertion, "issuer-secret").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
        headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

        let authorized = authorize_user_session(
            &state,
            &headers,
            &scope,
            build_chat_request_fingerprint(&payload).as_str(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(authorized.claims.issuer, "issuer-specific");
    }

    #[test]
    fn authorizes_matching_signed_user_session_with_issuer_key_registry() {
        let mut config = test_config();
        config.require_session_auth = true;
        config.session_auth_secret = None;
        config.session_auth_issuer_keys.insert(
            "issuer-keys".to_string(),
            HashMap::from([("v1".to_string(), "issuer-key-secret".to_string())]),
        );
        config.session_auth_allowed_issuers = vec!["issuer-keys".to_string()];
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        let state = test_state(config, IdentityBindings::default(), HashMap::new());
        let payload = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: None,
            metadata: None,
        };
        let scope = build_chat_identity_scope(&payload);
        let now_epoch = Utc::now().timestamp();
        let claims = UserSessionAuthClaims {
            version: 1,
            issuer: "issuer-keys".to_string(),
            key_id: Some("v1".to_string()),
            subject: "user-1".to_string(),
            source_kind: "chat_task".to_string(),
            audience: Some("consumer-entry-api".to_string()),
            request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            account_id: None,
            issued_at_epoch: now_epoch,
            expires_at_epoch: now_epoch + 60,
        };
        let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        let signature =
            sign_user_session_assertion(&assertion, "issuer-key-secret").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
        headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

        let authorized = authorize_user_session(
            &state,
            &headers,
            &scope,
            build_chat_request_fingerprint(&payload).as_str(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(authorized.claims.issuer, "issuer-keys");
        assert_eq!(authorized.claims.key_id.as_deref(), Some("v1"));
    }

    #[test]
    fn authorizes_matching_signed_user_session_with_shared_issuer_registry() {
        let mut config = test_config();
        config.require_session_auth = true;
        config.session_auth_secret = None;
        config.session_auth_issuer_registry.insert(
            "issuer-registry".to_string(),
            SessionAuthIssuerRegistryIssuer {
                active_key_id: Some("v1".to_string()),
                keys: HashMap::from([("v1".to_string(), "issuer-registry-secret".to_string())]),
            },
        );
        config.session_auth_allowed_issuers = vec!["issuer-registry".to_string()];
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        let state = test_state(config, IdentityBindings::default(), HashMap::new());
        let payload = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: None,
            metadata: None,
        };
        let scope = build_chat_identity_scope(&payload);
        let now_epoch = Utc::now().timestamp();
        let claims = UserSessionAuthClaims {
            version: 1,
            issuer: "issuer-registry".to_string(),
            key_id: Some("v1".to_string()),
            subject: "user-1".to_string(),
            source_kind: "chat_task".to_string(),
            audience: Some("consumer-entry-api".to_string()),
            request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            account_id: None,
            issued_at_epoch: now_epoch,
            expires_at_epoch: now_epoch + 60,
        };
        let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        let signature =
            sign_user_session_assertion(&assertion, "issuer-registry-secret").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
        headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

        let authorized = authorize_user_session(
            &state,
            &headers,
            &scope,
            build_chat_request_fingerprint(&payload).as_str(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(authorized.claims.issuer, "issuer-registry");
        assert_eq!(authorized.claims.key_id.as_deref(), Some("v1"));
    }

    #[test]
    fn session_auth_issuer_registry_active_key_diff_reports_changes() {
        let current = HashMap::from([
            (
                "matrix-entry-adapter".to_string(),
                SessionAuthIssuerRegistryIssuer {
                    active_key_id: Some("v1".to_string()),
                    keys: HashMap::from([
                        ("v1".to_string(), "secret-a".to_string()),
                        ("v2".to_string(), "secret-b".to_string()),
                    ]),
                },
            ),
            (
                "worker".to_string(),
                SessionAuthIssuerRegistryIssuer {
                    active_key_id: None,
                    keys: HashMap::from([("w1".to_string(), "secret-c".to_string())]),
                },
            ),
        ]);
        let candidate = HashMap::from([
            (
                "matrix-entry-adapter".to_string(),
                SessionAuthIssuerRegistryIssuer {
                    active_key_id: Some("v2".to_string()),
                    keys: HashMap::from([
                        ("v1".to_string(), "secret-a".to_string()),
                        ("v2".to_string(), "secret-b".to_string()),
                    ]),
                },
            ),
            (
                "worker".to_string(),
                SessionAuthIssuerRegistryIssuer {
                    active_key_id: Some("w1".to_string()),
                    keys: HashMap::from([("w1".to_string(), "secret-c".to_string())]),
                },
            ),
        ]);

        let diff = session_auth_issuer_registry_active_key_diff_json(&current, &candidate);
        assert_eq!(diff["matches"], Value::Bool(false));
        assert_eq!(diff["changed_active_key_count"], Value::from(2));
        assert_eq!(
            diff.get("changes")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
    }

    #[test]
    fn load_session_auth_issuer_registry_rejects_invalid_active_key() {
        let temp_path = std::env::temp_dir().join(format!(
            "cex-session-auth-issuer-registry-invalid-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_millis()
        ));
        std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"missing","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write issuer registry");

        let (metadata, registry) =
            load_session_auth_issuer_registry(temp_path.to_str());
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
    fn rejects_signed_user_session_with_unexpected_issuer() {
        let mut config = test_config();
        config.require_session_auth = true;
        config.session_auth_secret = Some("test-session-secret".to_string());
        config.session_auth_allowed_issuers = vec!["trusted-issuer".to_string()];
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        let state = test_state(config, IdentityBindings::default(), HashMap::new());
        let payload = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: None,
            metadata: None,
        };
        let scope = build_chat_identity_scope(&payload);
        let now_epoch = Utc::now().timestamp();
        let claims = UserSessionAuthClaims {
            version: 1,
            issuer: "unexpected-issuer".to_string(),
            key_id: None,
            subject: "user-1".to_string(),
            source_kind: "chat_task".to_string(),
            audience: Some("consumer-entry-api".to_string()),
            request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            account_id: None,
            issued_at_epoch: now_epoch,
            expires_at_epoch: now_epoch + 60,
        };
        let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        let signature = sign_user_session_assertion(&assertion, "test-session-secret").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
        headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

        let response = authorize_user_session(
            &state,
            &headers,
            &scope,
            build_chat_request_fingerprint(&payload).as_str(),
        )
        .unwrap_err();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn rejects_signed_user_session_with_mismatched_request_fingerprint() {
        let mut config = test_config();
        config.require_session_auth = true;
        config.session_auth_secret = Some("test-session-secret".to_string());
        config.session_auth_allowed_issuers = vec!["test-suite".to_string()];
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        let state = test_state(config, IdentityBindings::default(), HashMap::new());
        let payload = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: None,
            metadata: None,
        };
        let scope = build_chat_identity_scope(&payload);
        let now_epoch = Utc::now().timestamp();
        let claims = UserSessionAuthClaims {
            version: 1,
            issuer: "test-suite".to_string(),
            key_id: None,
            subject: "user-1".to_string(),
            source_kind: "chat_task".to_string(),
            audience: Some("consumer-entry-api".to_string()),
            request_fingerprint: Some("wrong-fingerprint".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            account_id: None,
            issued_at_epoch: now_epoch,
            expires_at_epoch: now_epoch + 60,
        };
        let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        let signature = sign_user_session_assertion(&assertion, "test-session-secret").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
        headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

        let response = authorize_user_session(
            &state,
            &headers,
            &scope,
            build_chat_request_fingerprint(&payload).as_str(),
        )
        .unwrap_err();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn production_profile_requires_strict_identity_reload_governance() {
        let mut config = test_config();
        config.runtime_profile = RuntimeProfile::Production;
        config.ingress_token = Some("entry-secret".to_string());
        config.replay_store_path = Some("/tmp/consumer-entry-replay.json".to_string());
        config.rate_limit_store_path = Some("/tmp/consumer-entry-rate-limits.json".to_string());
        config.identity_bindings_path = Some("/tmp/identity-bindings.json".to_string());
        config.require_identity_binding = true;
        config.cex_gateway_api_key = "prod-gateway-key".to_string();

        let errors = config.validate_runtime_profile().unwrap_err();
        assert!(errors
            .iter()
            .any(|item| item.contains("IDENTITY_BINDING_AUDIT_LOG_PATH")));
        assert!(errors
            .iter()
            .any(|item| item.contains("APPROVED_REVISIONS_PATH")));
        assert!(errors
            .iter()
            .any(|item| item.contains("RELOAD_REQUIRE_REVISION=true")));
        assert!(errors
            .iter()
            .any(|item| item.contains("RELOAD_REQUIRE_APPROVED_REVISION=true")));
        assert!(errors
            .iter()
            .any(|item| item.contains("RELOAD_REQUIRE_ACTOR=true")));
        assert!(errors
            .iter()
            .any(|item| item.contains("RELOAD_ALLOWED_ACTORS")));
    }

    #[test]
    fn prune_rate_limit_cache_drops_expired_and_oversized_entries() {
        let now = 1_760_000_100;
        let mut cache = RateLimitCache {
            seen: HashMap::from([(
                "chat:user-1:room-1".to_string(),
                VecDeque::from([now - 120, now - 30, now - 20, now - 10]),
            )]),
        };

        prune_rate_limit_cache(&mut cache, now, 60, 2);
        assert_eq!(
            cache.seen.get("chat:user-1:room-1").cloned(),
            Some(VecDeque::from([now - 20, now - 10]))
        );
    }

    #[test]
    fn load_rate_limit_cache_prunes_persisted_entries() {
        let path = std::env::temp_dir().join(format!(
            "consumer-entry-rate-limit-{}.json",
            std::process::id()
        ));
        let now = chrono::Utc::now().timestamp();
        std::fs::write(
            &path,
            serde_json::to_vec(&RateLimitCache {
                seen: HashMap::from([(
                    "chat:user-1:room-1".to_string(),
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
            cache.seen.get("chat:user-1:room-1").cloned(),
            Some(VecDeque::from([now - 5]))
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_identity_binding_store_prefers_separate_registry_file_when_configured() {
        let binding_path = std::env::temp_dir().join(format!(
            "consumer-entry-bindings-{}.json",
            std::process::id()
        ));
        let registry_path = std::env::temp_dir().join(format!(
            "consumer-entry-registry-{}.json",
            std::process::id()
        ));

        std::fs::write(
            &binding_path,
            r#"{
                "version": 1,
                "revision": "bindings-a",
                "product_users": {
                    "pu-1": { "org_id": "org-embedded", "account_id": "acct-embedded" }
                },
                "chat_users": {
                    "user-1": { "product_user_id": "pu-1" }
                }
            }"#,
        )
        .unwrap();
        std::fs::write(
            &registry_path,
            r#"{
                "version": 1,
                "revision": "registry-a",
                "product_users": {
                    "pu-1": { "org_id": "org-registry", "account_id": "acct-registry", "status": "active" }
                }
            }"#,
        )
        .unwrap();

        let mut config = test_config();
        config.identity_bindings_path = Some(binding_path.display().to_string());
        config.identity_registry_path = Some(registry_path.display().to_string());

        let store = load_identity_binding_store(&config);
        assert_eq!(store.metadata.revision.as_deref(), Some("bindings-a"));
        assert_eq!(
            store.registry_metadata.revision.as_deref(),
            Some("registry-a")
        );
        assert_eq!(store.registry_metadata.format, "separate-registry-document");
        assert_eq!(
            store
                .product_users
                .get("pu-1")
                .and_then(|user| user.org_id.as_deref()),
            Some("org-registry")
        );

        let _ = std::fs::remove_file(binding_path);
        let _ = std::fs::remove_file(registry_path);
    }

    #[test]
    fn load_identity_binding_revision_approval_state_reads_ordered_revisions() {
        let path = std::env::temp_dir().join(format!(
            "consumer-entry-approval-{}.json",
            std::process::id()
        ));
        std::fs::write(
            &path,
            r#"{
                "version": 1,
                "revision": "approval-doc-a",
                "approved_revisions": ["rev-a", "rev-b", "rev-a", "  "]
            }"#,
        )
        .unwrap();

        let mut config = test_config();
        config.identity_binding_approved_revisions_path = Some(path.display().to_string());

        let approval_state = load_identity_binding_revision_approval_state(&config);
        assert_eq!(approval_state.load_status, "loaded");
        assert_eq!(approval_state.revision.as_deref(), Some("approval-doc-a"));
        assert_eq!(approval_state.approved_revisions, vec!["rev-a", "rev-b"]);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_session_auth_issuer_registry_revision_approval_state_reads_ordered_revisions() {
        let path = std::env::temp_dir().join(format!(
            "consumer-entry-session-auth-approval-{}.json",
            std::process::id()
        ));
        std::fs::write(
            &path,
            r#"{
                "version": 1,
                "revision": "session-approval-a",
                "approved_revisions": ["sess-reg-a", "sess-reg-b", "sess-reg-a", "  "]
            }"#,
        )
        .unwrap();

        let mut config = test_config();
        config.session_auth_issuer_registry_approved_revisions_path =
            Some(path.display().to_string());

        let approval_state = load_session_auth_issuer_registry_revision_approval_state(&config);
        assert_eq!(approval_state.load_status, "loaded");
        assert_eq!(approval_state.revision.as_deref(), Some("session-approval-a"));
        assert_eq!(approval_state.approved_revisions, vec!["sess-reg-a", "sess-reg-b"]);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reload_governance_can_require_revision() {
        let mut config = test_config();
        config.identity_binding_reload_require_revision = true;

        let current = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let candidate = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "legacy-flat-map".to_string(),
                version: 1,
                revision: None,
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };

        let decision = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &IdentityBindingRevisionApprovalState::default(),
            None,
        );
        assert!(!decision.accepted);
        assert_eq!(decision.reason, "revision_required");
    }

    #[test]
    fn reload_governance_rejects_missing_product_user_refs() {
        let mut config = test_config();
        config.identity_registry_path = Some("/tmp/registry.json".to_string());

        let current = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("bind-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata {
                format: "separate-registry-document".to_string(),
                version: 1,
                revision: Some("reg-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            bindings: IdentityBindings {
                chat_users: HashMap::from([(
                    "chat-1".to_string(),
                    IdentityBindingEntry {
                        org_id: None,
                        account_id: None,
                        product_user_id: Some("pu-1".to_string()),
                    },
                )]),
                matrix_users: HashMap::new(),
            },
            product_users: HashMap::from([(
                "pu-1".to_string(),
                ProductUserIdentity {
                    org_id: Some("org-a".to_string()),
                    account_id: Some("acct-a".to_string()),
                    status: None,
                },
            )]),
        };
        let candidate = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("bind-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata {
                format: "separate-registry-document".to_string(),
                version: 1,
                revision: Some("reg-b".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            bindings: current.bindings.clone(),
            product_users: HashMap::new(),
        };

        let decision = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &IdentityBindingRevisionApprovalState::default(),
            None,
        );
        assert!(!decision.accepted);
        assert_eq!(decision.reason, "missing_product_user_refs");
        assert_eq!(decision.current_missing_product_user_refs, 0);
        assert_eq!(decision.candidate_missing_product_user_refs, 1);
    }

    #[test]
    fn reload_governance_with_separate_registry_requires_effective_revision() {
        let mut config = test_config();
        config.identity_registry_path = Some("/tmp/registry.json".to_string());
        config.identity_binding_reload_require_revision = true;

        let current = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("bind-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata {
                format: "separate-registry-document".to_string(),
                version: 1,
                revision: Some("reg-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let candidate = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("bind-b".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata {
                format: "separate-registry-document".to_string(),
                version: 1,
                revision: None,
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };

        let decision = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &IdentityBindingRevisionApprovalState::default(),
            None,
        );
        assert!(!decision.accepted);
        assert_eq!(decision.reason, "effective_revision_required");
    }

    #[test]
    fn reload_governance_can_reject_same_revision() {
        let mut config = test_config();
        config.identity_binding_reload_reject_same_revision = true;

        let current = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let candidate = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };

        let decision = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &IdentityBindingRevisionApprovalState::default(),
            None,
        );
        assert!(!decision.accepted);
        assert_eq!(decision.reason, "same_revision_rejected");
    }

    #[test]
    fn reload_governance_with_separate_registry_uses_effective_revision_for_approval() {
        let mut config = test_config();
        config.identity_registry_path = Some("/tmp/registry.json".to_string());
        config.identity_binding_reload_require_approved_revision = true;

        let current = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("bind-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata {
                format: "separate-registry-document".to_string(),
                version: 1,
                revision: Some("reg-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let candidate = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("bind-b".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata {
                format: "separate-registry-document".to_string(),
                version: 1,
                revision: Some("reg-b".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let approval_state = IdentityBindingRevisionApprovalState {
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(2),
            load_status: "loaded".to_string(),
            load_error: None,
            version: 1,
            revision: Some("approval-1".to_string()),
            approved_revisions: vec!["binding:bind-b|registry:reg-b".to_string()],
        };

        let decision = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &approval_state,
            None,
        );
        assert!(decision.accepted);
        assert_eq!(
            decision.candidate_effective_revision.as_deref(),
            Some("binding:bind-b|registry:reg-b")
        );
        assert_eq!(decision.candidate_revision_approved, Some(true));
    }

    #[test]
    fn reload_governance_can_require_approved_revision() {
        let mut config = test_config();
        config.identity_binding_reload_require_approved_revision = true;

        let current = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let candidate = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-c".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let approval_state = IdentityBindingRevisionApprovalState {
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(3),
            load_status: "loaded".to_string(),
            load_error: None,
            version: 1,
            revision: Some("approval-rev-1".to_string()),
            approved_revisions: vec!["rev-a".to_string(), "rev-b".to_string()],
        };

        let decision = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &approval_state,
            None,
        );
        assert!(!decision.accepted);
        assert_eq!(decision.reason, "candidate_revision_not_approved");
        assert_eq!(decision.candidate_revision_approved, Some(false));
    }

    #[test]
    fn reload_governance_can_reject_rollback_revision() {
        let mut config = test_config();
        config.identity_binding_reload_allow_rollback = false;

        let current = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-b".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let candidate = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let approval_state = IdentityBindingRevisionApprovalState {
            source_path: None,
            source_modified_epoch: None,
            loaded_at_epoch: Some(3),
            load_status: "loaded".to_string(),
            load_error: None,
            version: 1,
            revision: Some("approval-rev-1".to_string()),
            approved_revisions: vec!["rev-a".to_string(), "rev-b".to_string()],
        };

        let decision = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &approval_state,
            None,
        );
        assert!(!decision.accepted);
        assert_eq!(decision.reason, "rollback_revision_rejected");
        assert!(decision.rollback_blocked);
    }

    #[test]
    fn parse_csv_list_trims_empties_and_deduplicates() {
        assert_eq!(
            parse_csv_list(" alice , bob ,,alice, ,carol, bob ,,"),
            vec!["alice", "bob", "carol"]
        );
    }

    #[test]
    fn reload_governance_can_require_actor_allow_list_non_empty() {
        let mut config = test_config();
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_allowed_actors =
            vec!["alice".to_string(), "bob".to_string()];

        let current = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let candidate = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };

        let denied = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &IdentityBindingRevisionApprovalState::default(),
            Some("mallory"),
        );
        assert!(!denied.accepted);
        assert_eq!(denied.reason, "actor_not_authorized");
        assert_eq!(denied.actor_authorized, Some(false));
        assert_eq!(denied.actor_reason.as_deref(), Some("actor_not_allowed"));
    }

    #[test]
    fn reload_governance_can_require_actor_with_empty_allowlist() {
        let mut config = test_config();
        config.identity_binding_reload_require_actor = true;

        let current = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let candidate = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };

        let denied = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &IdentityBindingRevisionApprovalState::default(),
            Some("mallory"),
        );
        assert!(!denied.accepted);
        assert_eq!(denied.reason, "actor_not_authorized");
        assert_eq!(denied.actor_authorized, Some(false));
        assert_eq!(
            denied.actor_reason.as_deref(),
            Some("no_allowed_actors_configured")
        );
    }

    #[test]
    fn reload_governance_can_require_actor_header_missing() {
        let mut config = test_config();
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_allowed_actors = vec!["alice".to_string()];

        let current = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let candidate = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };

        let denied = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &IdentityBindingRevisionApprovalState::default(),
            None,
        );
        assert!(!denied.accepted);
        assert_eq!(denied.reason, "actor_not_authorized");
        assert_eq!(denied.actor_authorized, Some(false));
        assert_eq!(denied.actor_reason.as_deref(), Some("actor_missing"));
    }

    #[test]
    fn reload_governance_can_require_allowed_actor() {
        let mut config = test_config();
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_reject_same_revision = true;
        config.identity_binding_reload_allowed_actors =
            vec!["alice".to_string(), "bob".to_string()];

        let current = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(1),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };
        let candidate = IdentityBindingStore {
            metadata: IdentityBindingMetadata {
                format: "versioned-document".to_string(),
                version: 1,
                revision: Some("rev-a".to_string()),
                source_path: None,
                source_modified_epoch: None,
                loaded_at_epoch: Some(2),
                load_status: "loaded".to_string(),
                load_error: None,
            },
            registry_metadata: IdentityBindingMetadata::default(),
            bindings: IdentityBindings::default(),
            product_users: HashMap::new(),
        };

        let denied = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &IdentityBindingRevisionApprovalState::default(),
            Some("mallory"),
        );
        assert!(!denied.accepted);
        assert_eq!(denied.reason, "actor_not_authorized");
        assert_eq!(denied.actor_authorized, Some(false));
        assert_eq!(denied.actor_reason.as_deref(), Some("actor_not_allowed"));

        let allowed = evaluate_identity_binding_reload_governance(
            &config,
            &current,
            &candidate,
            &IdentityBindingRevisionApprovalState::default(),
            Some("alice"),
        );
        assert!(!allowed.accepted);
        assert_eq!(allowed.reason, "same_revision_rejected");
        assert_eq!(allowed.actor_authorized, Some(true));
        assert_eq!(allowed.actor_reason, None);
    }

    #[tokio::test]
    async fn chat_identity_binding_can_fill_org_and_account() {
        let mut bindings = IdentityBindings::default();
        bindings.chat_users.insert(
            "user-1".to_string(),
            IdentityBindingEntry {
                product_user_id: None,
                org_id: Some("org-bound".to_string()),
                account_id: Some("acct-bound".to_string()),
            },
        );
        let state = test_state(test_config(), bindings, HashMap::new());
        let payload = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: None,
            org_id: None,
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: None,
            metadata: None,
        };

        let resolved = resolve_chat_identity(&state, &payload).await.unwrap();
        assert_eq!(resolved.scope.org_id.as_deref(), Some("org-bound"));
        assert_eq!(resolved.scope.account_id.as_deref(), Some("acct-bound"));
        assert!(resolved.resolution.matched);
        assert_eq!(resolved.resolution.binding_version, 1);
    }

    #[tokio::test]
    async fn chat_identity_binding_rejects_org_mismatch() {
        let mut bindings = IdentityBindings::default();
        bindings.chat_users.insert(
            "user-1".to_string(),
            IdentityBindingEntry {
                product_user_id: None,
                org_id: Some("org-bound".to_string()),
                account_id: Some("acct-bound".to_string()),
            },
        );
        let state = test_state(test_config(), bindings, HashMap::new());
        let payload = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: None,
            org_id: Some("org-other".to_string()),
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: None,
            metadata: None,
        };

        assert!(resolve_chat_identity(&state, &payload).await.is_err());
    }

    #[tokio::test]
    async fn chat_identity_binding_can_resolve_via_product_user_registry() {
        let mut bindings = IdentityBindings::default();
        bindings.chat_users.insert(
            "user-1".to_string(),
            IdentityBindingEntry {
                product_user_id: Some("pu-1".to_string()),
                org_id: None,
                account_id: None,
            },
        );
        let product_users = HashMap::from([(
            "pu-1".to_string(),
            ProductUserIdentity {
                org_id: Some("org-registry".to_string()),
                account_id: Some("acct-registry".to_string()),
                status: Some("active".to_string()),
            },
        )]);
        let state = test_state(test_config(), bindings, product_users);
        let payload = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: None,
            org_id: None,
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: None,
            metadata: None,
        };

        let resolved = resolve_chat_identity(&state, &payload).await.unwrap();
        assert_eq!(resolved.scope.org_id.as_deref(), Some("org-registry"));
        assert_eq!(resolved.scope.account_id.as_deref(), Some("acct-registry"));
        assert_eq!(resolved.resolution.product_user_id.as_deref(), Some("pu-1"));
        assert_eq!(
            resolved.resolution.binding_source_kind,
            "product_user_registry".to_string()
        );
    }

    #[tokio::test]
    async fn chat_identity_binding_rejects_unknown_product_user_registry_ref() {
        let mut bindings = IdentityBindings::default();
        bindings.chat_users.insert(
            "user-1".to_string(),
            IdentityBindingEntry {
                product_user_id: Some("pu-missing".to_string()),
                org_id: None,
                account_id: None,
            },
        );
        let state = test_state(test_config(), bindings, HashMap::new());
        let payload = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: None,
            org_id: None,
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: None,
            metadata: None,
        };

        assert!(resolve_chat_identity(&state, &payload).await.is_err());
    }

    async fn send_text_request(
        app: &axum::Router,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, String) {
        let mut request = Request::builder().method(method).uri(uri);

        for (name, value) in headers {
            request = request.header(*name, *value);
        }

        let request = request.body(Body::empty()).expect("build request body");
        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("request response");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body bytes");
        let body = String::from_utf8(bytes.to_vec()).expect("decode response body as utf8");

        (status, body)
    }

    async fn send_identity_request(
        app: &axum::Router,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        let (status, body) = send_text_request(app, method, uri, headers).await;
        let body: Value = serde_json::from_str(&body).expect("decode reload response body");

        (status, body)
    }

    async fn send_health_request(app: &axum::Router) -> (StatusCode, Value) {
        send_identity_request(app, "GET", "/health", &[]).await
    }

    async fn send_metrics_request(app: &axum::Router) -> (StatusCode, String) {
        send_text_request(app, "GET", "/metrics", &[]).await
    }

    async fn send_identity_binding_reload_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "POST", "/v1/admin/identity-bindings/reload", headers).await
    }

    async fn send_identity_registry_reload_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "POST", "/v1/admin/identity-registry/reload", headers).await
    }

    async fn send_identity_registry_validate_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "POST", "/v1/admin/identity-registry/validate", headers).await
    }

    async fn send_identity_registry_status_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "GET", "/v1/admin/identity-registry/status", headers).await
    }

    async fn send_identity_registry_audit_request(
        app: &axum::Router,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "GET", uri, headers).await
    }

    async fn send_session_auth_issuer_registry_status_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(
            app,
            "GET",
            "/v1/admin/session-auth/issuer-registry/status",
            headers,
        )
        .await
    }

    async fn send_session_auth_issuer_registry_reload_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(
            app,
            "POST",
            "/v1/admin/session-auth/issuer-registry/reload",
            headers,
        )
        .await
    }

    async fn send_session_auth_issuer_registry_validate_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(
            app,
            "POST",
            "/v1/admin/session-auth/issuer-registry/validate",
            headers,
        )
        .await
    }

    async fn send_session_auth_issuer_registry_approval_status_request(
        app: &axum::Router,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "GET", uri, headers).await
    }

    async fn send_session_auth_issuer_registry_approval_validate_request(
        app: &axum::Router,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "POST", uri, headers).await
    }

    async fn send_session_auth_issuer_registry_actor_status_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(
            app,
            "GET",
            "/v1/admin/session-auth/issuer-registry/actors/status",
            headers,
        )
        .await
    }

    async fn send_session_auth_issuer_registry_actor_validate_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(
            app,
            "POST",
            "/v1/admin/session-auth/issuer-registry/actors/validate",
            headers,
        )
        .await
    }

    async fn send_identity_approval_status_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "GET", "/v1/admin/identity-approval/status", headers).await
    }

    async fn send_identity_approval_validate_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "POST", "/v1/admin/identity-approval/validate", headers).await
    }

    async fn send_identity_approval_source_request(
        app: &axum::Router,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "GET", uri, headers).await
    }

    async fn send_identity_approval_source_validate_request(
        app: &axum::Router,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "POST", uri, headers).await
    }

    async fn send_identity_governance_status_request(
        app: &axum::Router,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "GET", uri, headers).await
    }

    async fn send_identity_governance_validate_request(
        app: &axum::Router,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "POST", uri, headers).await
    }

    async fn send_identity_actor_status_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "GET", "/v1/admin/identity-actors/status", headers).await
    }

    async fn send_identity_actor_validate_request(
        app: &axum::Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send_identity_request(app, "POST", "/v1/admin/identity-actors/validate", headers).await
    }

    fn temp_identity_bindings_path(suffix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "consumer-entry-identity-bindings-{suffix}-{pid}-{nanos}.json",
            suffix = suffix,
            pid = std::process::id(),
            nanos = nanos
        ))
    }

    #[tokio::test]
    async fn session_auth_issuer_registry_status_endpoint_reports_live_metadata() {
        let temp_registry_path = temp_identity_bindings_path("session-auth-registry-status");
        let temp_approval_path =
            temp_identity_bindings_path("session-auth-registry-status-approval");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v2","keys":{"v1":"secret-a","v2":"secret-b"}},"worker":{"keys":{"w1":"secret-c"}}}}"#,
        )
        .expect("write session auth issuer registry");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-a","approved_revisions":["sess-reg-a","sess-reg-b"]}"#,
        )
        .expect("write session auth issuer registry approval");

        let (metadata, registry) =
            load_session_auth_issuer_registry(temp_registry_path.to_str());
        let mut config = test_config();
        config.ingress_token = Some("admin-token".to_string());
        config.require_session_auth = true;
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        config.session_auth_allowed_issuers = vec![
            "matrix-entry-adapter".to_string(),
            "missing-issuer".to_string(),
        ];
        config.session_auth_issuer_registry_path =
            Some(temp_registry_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry = registry;
        config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
        config.session_auth_issuer_registry_metadata = metadata;

        let app = build_router(AppState::new(config));
        let (status, body) = send_session_auth_issuer_registry_status_request(
            &app,
            &[("x-entry-token", "admin-token")],
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.get("status").and_then(Value::as_str), Some("ok"));
        let registry = body
            .get("session_auth_issuer_registry")
            .expect("session auth issuer registry object");
        assert_eq!(registry.get("status").and_then(Value::as_str), Some("ok"));
        assert_eq!(
            registry
                .get("metadata")
                .and_then(|value| value.get("revision"))
                .and_then(Value::as_str),
            Some("sess-reg-a")
        );
        assert_eq!(registry.get("issuer_count").and_then(Value::as_u64), Some(2));
        assert_eq!(registry.get("key_count").and_then(Value::as_u64), Some(3));
        assert_eq!(
            registry
                .get("allowed_issuers_missing")
                .and_then(Value::as_array)
                .map(|items| items.len()),
            Some(1)
        );
        assert_eq!(
            registry
                .get("issuers_without_active_key")
                .and_then(Value::as_array)
                .map(|items| items.len()),
            Some(1)
        );
        assert_eq!(
            registry
                .get("issuer_active_keys")
                .and_then(Value::as_array)
                .map(|items| items.len()),
            Some(2)
        );
        assert_eq!(
            registry
                .get("issuer_active_keys")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|value| value.get("active_key_id"))
                .and_then(Value::as_str),
            Some("v2")
        );
        assert_eq!(
            body.get("session_auth_issuer_registry_approval")
                .and_then(|value| value.get("current_revision_approved"))
                .and_then(Value::as_bool),
            Some(true)
        );

        let _ = std::fs::remove_file(&temp_registry_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn session_auth_issuer_registry_validate_endpoint_reloads_source_file() {
        let temp_registry_path = temp_identity_bindings_path("session-auth-registry-validate");
        let temp_approval_path =
            temp_identity_bindings_path("session-auth-registry-validate-approval");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-b","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-b","approved_revisions":["sess-reg-b"]}"#,
        )
        .expect("write session auth approval state");

        let (metadata, registry) =
            load_session_auth_issuer_registry(temp_registry_path.to_str());
        let mut config = test_config();
        config.ingress_token = Some("admin-token".to_string());
        config.require_session_auth = true;
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
        config.session_auth_issuer_registry_path =
            Some(temp_registry_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry_require_approved_revision = true;
        config.session_auth_issuer_registry = registry;
        config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
        config.session_auth_issuer_registry_metadata = metadata;

        let app = build_router(AppState::new(config));
        let (status, body) = send_session_auth_issuer_registry_validate_request(
            &app,
            &[("x-entry-token", "admin-token")],
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.get("valid").and_then(Value::as_bool), Some(true));
        assert_eq!(body.get("status").and_then(Value::as_str), Some("ok"));
        assert_eq!(
            body.get("matches_loaded_revision").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            body.get("session_auth_issuer_registry_source")
                .and_then(|value| value.get("metadata"))
                .and_then(|value| value.get("revision"))
                .and_then(Value::as_str),
            Some("sess-reg-b")
        );
        assert_eq!(
            body.get("session_auth_issuer_registry_source_approval")
                .and_then(|value| value.get("current_revision_approved"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            body.get("matches_loaded_active_keys")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            body.get("session_auth_issuer_registry_active_key_diff")
                .and_then(|value| value.get("changed_active_key_count"))
                .and_then(Value::as_u64),
            Some(0)
        );

        let _ = std::fs::remove_file(&temp_registry_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn session_auth_issuer_registry_validate_endpoint_reports_active_key_diff() {
        let temp_registry_path = temp_identity_bindings_path("session-auth-registry-active-key-diff");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-diff-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v2","keys":{"v1":"secret-a","v2":"secret-b"}}}}"#,
        )
        .expect("write session auth issuer registry");

        let mut config = test_config();
        config.ingress_token = Some("admin-token".to_string());
        config.require_session_auth = true;
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
        config.session_auth_issuer_registry_path =
            Some(temp_registry_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry = HashMap::from([(
            "matrix-entry-adapter".to_string(),
            SessionAuthIssuerRegistryIssuer {
                active_key_id: Some("v1".to_string()),
                keys: HashMap::from([
                    ("v1".to_string(), "secret-a".to_string()),
                    ("v2".to_string(), "secret-b".to_string()),
                ]),
            },
        )]);
        config.session_auth_issuer_registry_metadata = SessionAuthIssuerRegistryMetadata {
            version: 1,
            revision: Some("sess-reg-diff-a".to_string()),
            source_path: Some(temp_registry_path.to_string_lossy().to_string()),
            source_modified_epoch: None,
            loaded_at_epoch: Some(Utc::now().timestamp()),
            load_status: "loaded".to_string(),
            load_error: None,
            issuer_count: 1,
            key_count: 2,
        };

        let app = build_router(AppState::new(config));
        let (status, body) = send_session_auth_issuer_registry_validate_request(
            &app,
            &[("x-entry-token", "admin-token")],
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body.get("matches_loaded_active_keys")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            body.get("session_auth_issuer_registry_active_key_diff")
                .and_then(|value| value.get("changed_active_key_count"))
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            body.get("session_auth_issuer_registry_active_key_diff")
                .and_then(|value| value.get("changes"))
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|value| value.get("candidate_active_key_id"))
                .and_then(Value::as_str),
            Some("v2")
        );

        let _ = std::fs::remove_file(&temp_registry_path);
    }

    #[tokio::test]
    async fn session_auth_issuer_registry_validate_endpoint_requires_authorized_actor() {
        let temp_registry_path = temp_identity_bindings_path("session-auth-registry-validate-actor");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-actor-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");

        let (metadata, registry) =
            load_session_auth_issuer_registry(temp_registry_path.to_str());
        let mut config = test_config();
        config.ingress_token = Some("admin-token".to_string());
        config.require_session_auth = true;
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
        config.session_auth_issuer_registry_path =
            Some(temp_registry_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry_require_actor = true;
        config.session_auth_issuer_registry_actor_header =
            "x-session-auth-registry-actor".to_string();
        config.session_auth_issuer_registry_allowed_actors = vec!["alice".to_string()];
        config.session_auth_issuer_registry = registry;
        config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
        config.session_auth_issuer_registry_metadata = metadata;

        let app = build_router(AppState::new(config));
        let (status_missing, body_missing) = send_session_auth_issuer_registry_validate_request(
            &app,
            &[("x-entry-token", "admin-token")],
        )
        .await;

        assert_eq!(status_missing, StatusCode::CONFLICT);
        assert_eq!(body_missing.get("status").and_then(Value::as_str), Some("actor_missing"));
        assert_eq!(
            body_missing
                .get("session_auth_issuer_registry_actor_request")
                .and_then(|value| value.get("authorized"))
                .and_then(Value::as_bool),
            Some(false)
        );

        let (status_ok, body_ok) = send_session_auth_issuer_registry_validate_request(
            &app,
            &[
                ("x-entry-token", "admin-token"),
                ("x-session-auth-registry-actor", "alice"),
            ],
        )
        .await;

        assert_eq!(status_ok, StatusCode::OK);
        assert_eq!(body_ok.get("status").and_then(Value::as_str), Some("ok"));
        assert_eq!(
            body_ok
                .get("session_auth_issuer_registry_actor_request")
                .and_then(|value| value.get("authorized"))
                .and_then(Value::as_bool),
            Some(true)
        );

        let _ = std::fs::remove_file(&temp_registry_path);
    }

    #[tokio::test]
    async fn session_auth_issuer_registry_reload_endpoint_updates_live_runtime_state() {
        let temp_registry_path = temp_identity_bindings_path("session-auth-registry-reload-live");
        let temp_approval_path =
            temp_identity_bindings_path("session-auth-registry-reload-live-approval");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-live-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write live session auth issuer registry");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-live","approved_revisions":["sess-reg-live-a","sess-reg-live-b"]}"#,
        )
        .expect("write session auth approval state");

        let (metadata, registry) =
            load_session_auth_issuer_registry(temp_registry_path.to_str());
        let mut config = test_config();
        config.ingress_token = Some("admin-token".to_string());
        config.require_session_auth = true;
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
        config.session_auth_issuer_registry_path =
            Some(temp_registry_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry_require_approved_revision = true;
        config.session_auth_issuer_registry_require_actor = true;
        config.session_auth_issuer_registry_actor_header =
            "x-session-auth-registry-actor".to_string();
        config.session_auth_issuer_registry_allowed_actors = vec!["alice".to_string()];
        config.session_auth_issuer_registry = registry;
        config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
        config.session_auth_issuer_registry_metadata = metadata;

        let state = AppState::new(config);
        let app = build_router(state.clone());

        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-live-b","issuers":{"matrix-entry-adapter":{"activeKeyId":"v2","keys":{"v1":"secret-a","v2":"secret-b"}}}}"#,
        )
        .expect("rewrite live session auth issuer registry");

        let (status_missing, body_missing) = send_session_auth_issuer_registry_reload_request(
            &app,
            &[("x-entry-token", "admin-token")],
        )
        .await;
        assert_eq!(status_missing, StatusCode::FORBIDDEN);
        assert_eq!(
            body_missing
                .get("session_auth_issuer_registry_reload_governance")
                .and_then(|value| value.get("actor_reason"))
                .and_then(Value::as_str),
            Some("actor_missing")
        );

        let (status_before, body_before) = send_session_auth_issuer_registry_status_request(
            &app,
            &[("x-entry-token", "admin-token")],
        )
        .await;
        assert_eq!(status_before, StatusCode::OK);
        assert_eq!(
            body_before
                .get("session_auth_issuer_registry")
                .and_then(|value| value.get("metadata"))
                .and_then(|value| value.get("revision"))
                .and_then(Value::as_str),
            Some("sess-reg-live-a")
        );

        let (status_ok, body_ok) = send_session_auth_issuer_registry_reload_request(
            &app,
            &[
                ("x-entry-token", "admin-token"),
                ("x-session-auth-registry-actor", "alice"),
            ],
        )
        .await;
        assert_eq!(status_ok, StatusCode::OK);
        assert_eq!(body_ok.get("reloaded").and_then(Value::as_bool), Some(true));
        assert_eq!(
            body_ok
                .get("session_auth_issuer_registry")
                .and_then(|value| value.get("metadata"))
                .and_then(|value| value.get("revision"))
                .and_then(Value::as_str),
            Some("sess-reg-live-b")
        );

        let (status_after, body_after) = send_session_auth_issuer_registry_status_request(
            &app,
            &[("x-entry-token", "admin-token")],
        )
        .await;
        assert_eq!(status_after, StatusCode::OK);
        assert_eq!(
            body_after
                .get("session_auth_issuer_registry")
                .and_then(|value| value.get("metadata"))
                .and_then(|value| value.get("revision"))
                .and_then(Value::as_str),
            Some("sess-reg-live-b")
        );

        let payload = CreateChatTaskRequest {
            user_id: Some("user-1".to_string()),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            text: "hello".to_string(),
            capability_id: None,
            account_id: None,
            idempotency_key: None,
            metadata: None,
        };
        let scope = build_chat_identity_scope(&payload);
        let now_epoch = Utc::now().timestamp();
        let claims = UserSessionAuthClaims {
            version: 1,
            issuer: "matrix-entry-adapter".to_string(),
            key_id: Some("v2".to_string()),
            subject: "user-1".to_string(),
            source_kind: "chat_task".to_string(),
            audience: Some("consumer-entry-api".to_string()),
            request_fingerprint: Some(build_chat_request_fingerprint(&payload)),
            room_id: Some("room-1".to_string()),
            session_id: Some("session-1".to_string()),
            org_id: Some("org-1".to_string()),
            account_id: None,
            issued_at_epoch: now_epoch,
            expires_at_epoch: now_epoch + 60,
        };
        let assertion = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        let signature = sign_user_session_assertion(&assertion, "secret-b").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(USER_SESSION_ASSERTION_HEADER, assertion.parse().unwrap());
        headers.insert(USER_SESSION_SIGNATURE_HEADER, signature.parse().unwrap());

        let authorized = authorize_user_session(
            &state,
            &headers,
            &scope,
            build_chat_request_fingerprint(&payload).as_str(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(authorized.claims.key_id.as_deref(), Some("v2"));

        let _ = std::fs::remove_file(&temp_registry_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn session_auth_issuer_registry_actor_status_endpoint_reports_configuration() {
        let mut config = test_config();
        config.ingress_token = Some("admin-token".to_string());
        config.require_session_auth = true;
        config.session_auth_issuer_registry_path =
            Some("./run/local-runtime/session-auth-issuer-registry.json".to_string());
        config.session_auth_issuer_registry_require_actor = true;
        config.session_auth_issuer_registry_actor_header =
            "x-session-auth-registry-actor".to_string();
        config.session_auth_issuer_registry_allowed_actors =
            vec!["alice".to_string(), "bob".to_string()];

        let app = build_router(AppState::new(config));
        let (status, body) = send_session_auth_issuer_registry_actor_status_request(
            &app,
            &[("x-entry-token", "admin-token")],
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.get("actor_valid").and_then(Value::as_bool), Some(true));
        assert_eq!(
            body.get("session_auth_issuer_registry_actor_checks")
                .and_then(|value| value.get("actor_header"))
                .and_then(Value::as_str),
            Some("x-session-auth-registry-actor")
        );
        assert_eq!(
            body.get("session_auth_issuer_registry_actor_checks")
                .and_then(|value| value.get("allowed_actor_count"))
                .and_then(Value::as_u64),
            Some(2)
        );
    }

    #[tokio::test]
    async fn session_auth_issuer_registry_actor_validate_endpoint_rejects_missing_allowed_actors() {
        let mut config = test_config();
        config.ingress_token = Some("admin-token".to_string());
        config.require_session_auth = true;
        config.session_auth_issuer_registry_path =
            Some("./run/local-runtime/session-auth-issuer-registry.json".to_string());
        config.session_auth_issuer_registry_require_actor = true;
        config.session_auth_issuer_registry_actor_header =
            "x-session-auth-registry-actor".to_string();

        let app = build_router(AppState::new(config));
        let (status, body) = send_session_auth_issuer_registry_actor_validate_request(
            &app,
            &[("x-entry-token", "admin-token")],
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body.get("valid").and_then(Value::as_bool), Some(false));
        assert_eq!(body.get("status").and_then(Value::as_str), Some("no_allowed_actors_configured"));
    }

    #[tokio::test]
    async fn session_auth_issuer_registry_approval_validate_endpoint_rejects_unapproved_revision() {
        let temp_registry_path = temp_identity_bindings_path("session-auth-registry-approval-validate");
        let temp_approval_path = temp_identity_bindings_path(
            "session-auth-registry-approval-validate-source",
        );
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-c","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-c","approved_revisions":["sess-reg-a","sess-reg-b"]}"#,
        )
        .expect("write session auth approval state");

        let (metadata, registry) =
            load_session_auth_issuer_registry(temp_registry_path.to_str());
        let mut config = test_config();
        config.ingress_token = Some("admin-token".to_string());
        config.require_session_auth = true;
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
        config.session_auth_issuer_registry_path =
            Some(temp_registry_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry_require_approved_revision = true;
        config.session_auth_issuer_registry = registry;
        config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
        config.session_auth_issuer_registry_metadata = metadata;

        let app = build_router(AppState::new(config));
        let (status, body) = send_session_auth_issuer_registry_approval_validate_request(
            &app,
            "/v1/admin/session-auth/issuer-registry/approval/validate?limit=1",
            &[("x-entry-token", "admin-token")],
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body.get("status").and_then(Value::as_str), Some("current_revision_not_approved"));
        assert_eq!(
            body.get("session_auth_issuer_registry_approval")
                .and_then(|value| value.get("current_revision_approved"))
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            body.get("session_auth_issuer_registry_approval_source")
                .and_then(|value| value.get("returned_revision_count"))
                .and_then(Value::as_u64),
            Some(1)
        );

        let _ = std::fs::remove_file(&temp_registry_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn session_auth_issuer_registry_approval_status_endpoint_reports_source() {
        let temp_registry_path = temp_identity_bindings_path("session-auth-registry-approval-status");
        let temp_approval_path = temp_identity_bindings_path(
            "session-auth-registry-approval-status-source",
        );
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-d","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-d","approved_revisions":["sess-reg-c","sess-reg-d"]}"#,
        )
        .expect("write session auth approval state");

        let (metadata, registry) =
            load_session_auth_issuer_registry(temp_registry_path.to_str());
        let mut config = test_config();
        config.ingress_token = Some("admin-token".to_string());
        config.require_session_auth = true;
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
        config.session_auth_issuer_registry_path =
            Some(temp_registry_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry = registry;
        config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
        config.session_auth_issuer_registry_metadata = metadata;

        let app = build_router(AppState::new(config));
        let (status, body) = send_session_auth_issuer_registry_approval_status_request(
            &app,
            "/v1/admin/session-auth/issuer-registry/approval/status?limit=1",
            &[("x-entry-token", "admin-token")],
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.get("status").and_then(Value::as_str), Some("ok"));
        assert_eq!(
            body.get("session_auth_issuer_registry_approval")
                .and_then(|value| value.get("current_revision_approved"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            body.get("session_auth_issuer_registry_approval_source")
                .and_then(|value| value.get("returned_revision_count"))
                .and_then(Value::as_u64),
            Some(1)
        );

        let _ = std::fs::remove_file(&temp_registry_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn reload_identity_bindings_endpoint_rejects_actor_gate_miss_and_denied_with_403() {
        let temp_path = temp_identity_bindings_path("actor-gate");
        std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-a","chat_users":{},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_allowed_actors = vec!["alice".to_string()];
        config.identity_binding_reload_reject_same_revision = true;

        let app = build_router(AppState::new(config));

        let (status_missing, body_missing) = send_identity_binding_reload_request(&app, &[]).await;
        assert_eq!(status_missing, StatusCode::FORBIDDEN);
        assert_eq!(
            body_missing["identity_binding_reload_governance"]["actor_authorized"],
            false,
        );
        assert_eq!(
            body_missing["identity_binding_reload_governance"]["actor_reason"],
            "actor_missing",
        );

        let (status_denied, body_denied) =
            send_identity_binding_reload_request(&app, &[("x-identity-binding-actor", "mallory")])
                .await;
        assert_eq!(status_denied, StatusCode::FORBIDDEN);
        assert_eq!(
            body_denied["identity_binding_reload_governance"]["actor_authorized"],
            false,
        );
        assert_eq!(
            body_denied["identity_binding_reload_governance"]["actor_reason"],
            "actor_not_allowed",
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn reload_identity_bindings_endpoint_returns_409_when_actor_allowed_but_governance_rejects(
    ) {
        let temp_path = temp_identity_bindings_path("actor-allowed-409");
        std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-a","chat_users":{},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_allowed_actors = vec!["alice".to_string()];
        config.identity_binding_reload_reject_same_revision = true;

        let app = build_router(AppState::new(config));

        let (status_rejected, body_rejected) =
            send_identity_binding_reload_request(&app, &[("x-identity-binding-actor", "alice")])
                .await;
        assert_eq!(status_rejected, StatusCode::CONFLICT);
        assert_eq!(
            body_rejected["identity_binding_reload_governance"]["actor_authorized"],
            true,
        );
        assert!(body_rejected["identity_binding_reload_governance"]["actor_reason"].is_null());
        assert_eq!(
            body_rejected["identity_binding_reload_governance"]["reason"],
            "same_revision_rejected",
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn reload_identity_bindings_endpoint_supports_custom_actor_header() {
        let temp_path = temp_identity_bindings_path("actor-custom-header");
        std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-a","chat_users":{},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_actor_header = "x-deploy-actor".to_string();
        config.identity_binding_reload_allowed_actors = vec!["alice".to_string()];
        config.identity_binding_reload_reject_same_revision = true;

        let app = build_router(AppState::new(config));

        let (status_missing, body_missing) = send_identity_binding_reload_request(&app, &[]).await;
        assert_eq!(status_missing, StatusCode::FORBIDDEN);
        assert_eq!(
            body_missing["identity_binding_reload_governance"]["actor_authorized"],
            false,
        );
        assert_eq!(
            body_missing["identity_binding_reload_governance"]["actor_reason"],
            "actor_missing",
        );

        let (status_wrong_header, body_wrong_header) =
            send_identity_binding_reload_request(&app, &[("x-deploy-actor", "mallory")]).await;
        assert_eq!(status_wrong_header, StatusCode::FORBIDDEN);
        assert_eq!(
            body_wrong_header["identity_binding_reload_governance"]["actor_authorized"],
            false,
        );
        assert_eq!(
            body_wrong_header["identity_binding_reload_governance"]["actor_reason"],
            "actor_not_allowed",
        );

        let (status_ok, body_ok) =
            send_identity_binding_reload_request(&app, &[("x-deploy-actor", "alice")]).await;
        assert_eq!(status_ok, StatusCode::CONFLICT);
        assert_eq!(
            body_ok["identity_binding_reload_governance"]["actor_authorized"],
            true,
        );
        assert!(body_ok["identity_binding_reload_governance"]["actor_reason"].is_null());
        assert_eq!(
            body_ok["identity_binding_reload_governance"]["reason"],
            "same_revision_rejected",
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn reload_identity_bindings_endpoint_returns_success_after_revision_bump() {
        let temp_path = temp_identity_bindings_path("actor-success");
        std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-a","chat_users":{"chat-1":{"org_id":"org-old","account_id":"acct-old"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());
        config.identity_binding_reload_reject_same_revision = false;

        let app = build_router(AppState::new(config));

        std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-b","chat_users":{"chat-1":{"org_id":"org-new","account_id":"acct-new"},"chat-2":{"org_id":"org-new2"}},"matrix_users":{"mx-9":{"org_id":"org-mx"}}}"#,
        )
        .expect("write revised identity bindings");

        let (status_ok, body_ok) = send_identity_binding_reload_request(&app, &[]).await;
        assert_eq!(status_ok, StatusCode::OK);
        assert_eq!(body_ok["ok"], true);
        assert_eq!(body_ok["reloaded"], true);
        assert_eq!(
            body_ok["identity_binding_reload_governance"]["accepted"],
            true,
        );
        assert!(body_ok["identity_binding_reload_governance"]["actor_authorized"].is_null());
        assert!(body_ok["identity_binding_reload_governance"]["actor_reason"].is_null());
        assert_eq!(
            body_ok["identity_binding_reload_governance"]["reason"],
            "policy_ok",
        );
        assert_eq!(
            body_ok["identity_binding_reload_governance"]["current_revision"],
            "rev-a",
        );
        assert_eq!(
            body_ok["identity_binding_reload_governance"]["candidate_revision"],
            "rev-b",
        );
        assert_eq!(
            body_ok["identity_binding_reload_governance"]["candidate_revision_approved"],
            false,
        );
        assert_eq!(body_ok["identity_binding_counts"]["chat_users"], 2,);
        assert_eq!(body_ok["identity_binding_counts"]["matrix_users"], 1,);
        assert_eq!(body_ok["identity_binding_metadata"]["revision"], "rev-b",);

        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn reload_identity_registry_endpoint_returns_conflict_when_registry_not_configured() {
        let temp_path = temp_identity_bindings_path("registry-not-configured");
        std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());

        let app = build_router(AppState::new(config));

        let (status, body) = send_identity_registry_reload_request(&app, &[]).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["ok"], false);
        assert_eq!(body["reloaded"], false);
        assert_eq!(body["error"], "identity_registry_not_configured");

        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn reload_identity_registry_endpoint_rejects_missing_product_user_refs() {
        let temp_bindings_path = temp_identity_bindings_path("registry-missing-ref-bindings");
        let temp_registry_path = temp_identity_bindings_path("registry-missing-ref-file");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-old","account_id":"acct-old"}}}"#,
        )
        .expect("write initial identity registry");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
        config.identity_binding_reload_require_revision = true;
        config.identity_binding_reload_reject_same_revision = true;

        let app = build_router(AppState::new(config));

        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-b","product_users":{}}"#,
        )
        .expect("write broken identity registry");

        let (status, body) = send_identity_registry_reload_request(&app, &[]).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["ok"], false);
        assert_eq!(body["reloaded"], false);
        assert_eq!(
            body["identity_binding_reload_governance"]["reason"],
            "missing_product_user_refs",
        );
        assert_eq!(
            body["identity_binding_reload_governance"]["candidate_missing_product_user_refs"],
            1,
        );
        assert_eq!(
            body["identity_binding_counts"]["missing_product_user_refs"],
            1
        );

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_registry_path);
    }

    #[tokio::test]
    async fn reload_identity_registry_endpoint_returns_success_after_registry_revision_bump() {
        let temp_bindings_path = temp_identity_bindings_path("registry-reload-bindings");
        let temp_registry_path = temp_identity_bindings_path("registry-reload-file");
        let temp_audit_path = temp_identity_bindings_path("registry-reload-audit");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-old","account_id":"acct-old"}}}"#,
        )
        .expect("write initial identity registry");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
        config.identity_binding_audit_log_path =
            Some(temp_audit_path.to_string_lossy().to_string());
        config.identity_binding_reload_require_revision = true;
        config.identity_binding_reload_reject_same_revision = true;

        let app = build_router(AppState::new(config));

        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-b","product_users":{"pu-1":{"org_id":"org-new","account_id":"acct-new"},"pu-2":{"org_id":"org-extra"}}}"#,
        )
        .expect("write revised identity registry");

        let (status, body) = send_identity_registry_reload_request(&app, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], true);
        assert_eq!(body["reloaded"], true);
        assert_eq!(body["identity_binding_reload_governance"]["accepted"], true,);
        assert_eq!(
            body["identity_binding_reload_governance"]["reason"],
            "policy_ok",
        );
        assert_eq!(
            body["identity_binding_reload_governance"]["current_revision"],
            "bind-a",
        );
        assert_eq!(
            body["identity_binding_reload_governance"]["candidate_revision"],
            "bind-a",
        );
        assert_eq!(
            body["identity_binding_reload_governance"]["current_registry_revision"],
            "reg-a",
        );
        assert_eq!(
            body["identity_binding_reload_governance"]["candidate_registry_revision"],
            "reg-b",
        );
        assert_eq!(
            body["identity_binding_reload_governance"]["current_effective_revision"],
            "binding:bind-a|registry:reg-a",
        );
        assert_eq!(
            body["identity_binding_reload_governance"]["candidate_effective_revision"],
            "binding:bind-a|registry:reg-b",
        );
        assert_eq!(
            body["identity_binding_reload_governance"]["candidate_registry_load_status"],
            "loaded",
        );
        assert_eq!(body["identity_registry_metadata"]["revision"], "reg-b");
        assert_eq!(body["identity_binding_counts"]["product_users"], 2);
        assert_eq!(
            body["identity_binding_audit"]["last_event_kind"],
            "registry_reload"
        );

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_registry_path);
        let _ = std::fs::remove_file(&temp_audit_path);
    }

    #[tokio::test]
    async fn validate_identity_registry_endpoint_previews_candidate_without_applying_it() {
        let temp_bindings_path = temp_identity_bindings_path("registry-validate-bindings");
        let temp_registry_path = temp_identity_bindings_path("registry-validate-file");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-old","account_id":"acct-old"}}}"#,
        )
        .expect("write initial identity registry");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
        config.identity_binding_reload_require_revision = true;
        config.identity_binding_reload_reject_same_revision = true;

        let app = build_router(AppState::new(config));

        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-b","product_users":{"pu-1":{"org_id":"org-new","account_id":"acct-new"}}}"#,
        )
        .expect("write candidate identity registry");

        let (validate_status, validate_body) =
            send_identity_registry_validate_request(&app, &[]).await;
        assert_eq!(validate_status, StatusCode::OK);
        assert_eq!(validate_body["ok"], true);
        assert_eq!(validate_body["validated"], true);
        assert_eq!(validate_body["valid"], true);
        assert_eq!(validate_body["checked_only"], true);
        assert_eq!(validate_body["would_reload"], true);
        assert_eq!(
            validate_body["identity_registry_metadata"]["revision"],
            "reg-b"
        );
        assert_eq!(
            validate_body["identity_binding_reload_governance"]["current_effective_revision"],
            "binding:bind-a|registry:reg-a",
        );
        assert_eq!(
            validate_body["identity_binding_reload_governance"]["candidate_effective_revision"],
            "binding:bind-a|registry:reg-b",
        );
        assert_eq!(
            validate_body["effective_revision"],
            "binding:bind-a|registry:reg-b"
        );

        let (status_status, status_body) = send_identity_registry_status_request(&app, &[]).await;
        assert_eq!(status_status, StatusCode::OK);
        assert_eq!(status_body["ok"], true);
        assert_eq!(
            status_body["effective_revision"],
            "binding:bind-a|registry:reg-a"
        );
        assert_eq!(
            status_body["identity_registry_metadata"]["revision"],
            "reg-a"
        );

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_registry_path);
    }

    #[tokio::test]
    async fn validate_identity_registry_endpoint_rejects_missing_product_user_refs() {
        let temp_bindings_path = temp_identity_bindings_path("registry-validate-missing-bindings");
        let temp_registry_path = temp_identity_bindings_path("registry-validate-missing-file");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-old","account_id":"acct-old"}}}"#,
        )
        .expect("write initial identity registry");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
        config.identity_binding_reload_require_revision = true;
        config.identity_binding_reload_reject_same_revision = true;

        let app = build_router(AppState::new(config));

        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-b","product_users":{}}"#,
        )
        .expect("write broken candidate identity registry");

        let (status, body) = send_identity_registry_validate_request(&app, &[]).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["ok"], false);
        assert_eq!(body["validated"], true);
        assert_eq!(body["valid"], false);
        assert_eq!(body["would_reload"], false);
        assert_eq!(body["missing_product_user_refs"], 1);
        assert_eq!(
            body["identity_binding_reload_governance"]["reason"],
            "missing_product_user_refs",
        );

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_registry_path);
    }

    #[tokio::test]
    async fn identity_approval_status_endpoint_reports_current_effective_revision_state() {
        let temp_bindings_path = temp_identity_bindings_path("approval-status-bindings");
        let temp_registry_path = temp_identity_bindings_path("approval-status-registry");
        let temp_approval_path = temp_identity_bindings_path("approval-status-file");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-a","account_id":"acct-a"}}}"#,
        )
        .expect("write initial identity registry");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"approval-a","approved_revisions":["binding:bind-a|registry:reg-a","binding:bind-b|registry:reg-b"]}"#,
        )
        .expect("write approval state");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
        config.identity_binding_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());

        let app = build_router(AppState::new(config));
        let (status, body) = send_identity_approval_status_request(&app, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], true);
        assert_eq!(body["approval_valid"], true);
        assert_eq!(body["identity_approval_checks"]["status"], "ok");
        assert_eq!(
            body["identity_approval_checks"]["current_effective_revision"],
            "binding:bind-a|registry:reg-a",
        );
        assert_eq!(
            body["identity_approval_checks"]["current_effective_revision_approved"],
            true,
        );
        assert_eq!(
            body["identity_approval_checks"]["current_effective_revision_index"],
            0
        );
        assert_eq!(
            body["identity_approval_checks"]["latest_approved_revision"],
            "binding:bind-b|registry:reg-b",
        );
        assert_eq!(
            body["identity_approval_checks"]["current_matches_latest_approved"],
            false
        );

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_registry_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn identity_approval_validate_endpoint_rejects_unapproved_current_effective_revision() {
        let temp_bindings_path = temp_identity_bindings_path("approval-validate-bindings");
        let temp_registry_path = temp_identity_bindings_path("approval-validate-registry");
        let temp_approval_path = temp_identity_bindings_path("approval-validate-file");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"bind-a","chat_users":{"chat-1":{"product_user_id":"pu-1"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"reg-a","product_users":{"pu-1":{"org_id":"org-a","account_id":"acct-a"}}}"#,
        )
        .expect("write initial identity registry");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"approval-a","approved_revisions":["binding:bind-b|registry:reg-b"]}"#,
        )
        .expect("write approval state");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_registry_path = Some(temp_registry_path.to_string_lossy().to_string());
        config.identity_binding_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());

        let app = build_router(AppState::new(config));
        let (status, body) = send_identity_approval_validate_request(&app, &[]).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["ok"], false);
        assert_eq!(body["validated"], true);
        assert_eq!(body["valid"], false);
        assert_eq!(body["status"], "current_effective_revision_not_approved");
        assert_eq!(
            body["identity_approval_checks"]["current_effective_revision_approved"],
            false,
        );

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_registry_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn identity_approval_source_endpoint_returns_latest_revisions_preview() {
        let temp_bindings_path = temp_identity_bindings_path("approval-source-bindings");
        let temp_approval_path = temp_identity_bindings_path("approval-source-file");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"rev-b","chat_users":{},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"approval-a","approved_revisions":["rev-a","rev-b","rev-c"]}"#,
        )
        .expect("write approval state");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_binding_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());

        let app = build_router(AppState::new(config));
        let (status, body) = send_identity_approval_source_request(
            &app,
            "/v1/admin/identity-approval/source?limit=2",
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], true);
        assert_eq!(body["source_valid"], true);
        assert_eq!(body["identity_approval_source"]["status"], "ok");
        assert_eq!(
            body["identity_approval_source"]["approved_revision_count"],
            3
        );
        assert_eq!(
            body["identity_approval_source"]["returned_revision_count"],
            2
        );
        assert_eq!(
            body["identity_approval_source"]["latest_approved_revision"],
            "rev-c"
        );
        assert_eq!(
            body["identity_approval_source"]["current_effective_revision"],
            "rev-b"
        );
        assert_eq!(
            body["identity_approval_source"]["revisions"][0]["revision"],
            "rev-c"
        );
        assert_eq!(
            body["identity_approval_source"]["revisions"][0]["is_latest"],
            true
        );
        assert_eq!(
            body["identity_approval_source"]["revisions"][1]["revision"],
            "rev-b"
        );
        assert_eq!(
            body["identity_approval_source"]["revisions"][1]["is_current_effective"],
            true
        );

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn identity_approval_source_validate_endpoint_rejects_empty_revision_set() {
        let temp_approval_path = temp_identity_bindings_path("approval-source-empty");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"approval-a","approved_revisions":[]}"#,
        )
        .expect("write empty approval state");

        let mut config = test_config();
        config.identity_binding_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());

        let app = build_router(AppState::new(config));
        let (status, body) = send_identity_approval_source_validate_request(
            &app,
            "/v1/admin/identity-approval/source/validate",
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["ok"], false);
        assert_eq!(body["validated"], true);
        assert_eq!(body["valid"], false);
        assert_eq!(body["status"], "approved_revision_set_empty");
        assert_eq!(
            body["identity_approval_source"]["approved_revision_count"],
            0
        );
        assert_eq!(
            body["identity_approval_source"]["returned_revision_count"],
            0
        );

        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn health_endpoint_exposes_identity_governance_overview() {
        let temp_bindings_path = temp_identity_bindings_path("health-governance-bindings");
        let temp_approval_path = temp_identity_bindings_path("health-governance-approval");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"rev-b","chat_users":{},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"approval-a","approved_revisions":["rev-b"]}"#,
        )
        .expect("write approval state");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_binding_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_allowed_actors = Vec::new();

        let app = build_router(AppState::new(config));
        let (status, body) = send_health_request(&app).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert_eq!(
            body["identity_governance_overview"]["status"],
            "no_allowed_actors_configured"
        );
        assert_eq!(body["identity_governance_overview"]["valid"], false);
        assert_eq!(
            body["profile_validation"]["checks"]["identity_governance_valid"],
            false
        );

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn metrics_endpoint_exposes_identity_governance_gauges() {
        let temp_bindings_path = temp_identity_bindings_path("metrics-governance-bindings");
        let temp_approval_path = temp_identity_bindings_path("metrics-governance-approval");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"rev-b","chat_users":{},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"approval-a","approved_revisions":["rev-b"]}"#,
        )
        .expect("write approval state");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_binding_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_allowed_actors = Vec::new();

        let app = build_router(AppState::new(config));
        let (status, body) = send_metrics_request(&app).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("cex_consumer_entry_identity_governance_valid 0"));
        assert!(body.contains("cex_consumer_entry_identity_binding_loaded 1"));
        assert!(body.contains("cex_consumer_entry_identity_registry_loaded 1"));
        assert!(body.contains("cex_consumer_entry_identity_ref_integrity_ok 1"));
        assert!(body.contains("cex_consumer_entry_identity_actor_gate_valid 0"));
        assert!(body.contains("cex_consumer_entry_identity_approval_source_valid 1"));
        assert!(body.contains("cex_consumer_entry_identity_approval_coverage_valid 1"));

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn health_endpoint_exposes_session_auth_registry_governance_overview() {
        let temp_registry_path = temp_identity_bindings_path("health-session-auth-registry");
        let temp_approval_path = temp_identity_bindings_path("health-session-auth-approval");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-health-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-health-a","approved_revisions":["sess-reg-other"]}"#,
        )
        .expect("write session auth approval state");

        let (metadata, registry) =
            load_session_auth_issuer_registry(temp_registry_path.to_str());
        let mut config = test_config();
        config.require_session_auth = true;
        config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        config.session_auth_issuer_registry_path =
            Some(temp_registry_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry = registry;
        config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
        config.session_auth_issuer_registry_metadata = metadata;
        config.session_auth_issuer_registry_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry_require_approved_revision = true;

        let app = build_router(AppState::new(config));
        let (status, body) = send_health_request(&app).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["session_auth_issuer_registry_governance_overview"]["status"],
            "current_revision_not_approved"
        );
        assert_eq!(
            body["session_auth_issuer_registry_governance_overview"]["valid"],
            false
        );
        assert_eq!(
            body["profile_validation"]["checks"]["session_auth_issuer_registry_governance_valid"],
            false
        );

        let _ = std::fs::remove_file(&temp_registry_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn metrics_endpoint_exposes_session_auth_registry_governance_gauges() {
        let temp_registry_path = temp_identity_bindings_path("metrics-session-auth-registry");
        let temp_approval_path = temp_identity_bindings_path("metrics-session-auth-approval");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-metrics-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"sess-approval-metrics-a","approved_revisions":[]}"#,
        )
        .expect("write session auth approval state");

        let (metadata, registry) =
            load_session_auth_issuer_registry(temp_registry_path.to_str());
        let mut config = test_config();
        config.require_session_auth = true;
        config.session_auth_allowed_issuers = vec!["matrix-entry-adapter".to_string()];
        config.session_auth_expected_audience = Some("consumer-entry-api".to_string());
        config.session_auth_issuer_registry_path =
            Some(temp_registry_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry = registry;
        config.session_auth_issuer_registry_load_error = metadata.load_error.clone();
        config.session_auth_issuer_registry_metadata = metadata;
        config.session_auth_issuer_registry_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());
        config.session_auth_issuer_registry_require_approved_revision = true;

        let app = build_router(AppState::new(config));
        let (status, body) = send_metrics_request(&app).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("cex_consumer_entry_session_auth_issuer_registry_governance_valid 0"));
        assert!(body.contains("cex_consumer_entry_session_auth_issuer_registry_approval_source_valid 0"));
        assert!(body.contains("cex_consumer_entry_session_auth_issuer_registry_approval_coverage_valid 0"));

        let _ = std::fs::remove_file(&temp_registry_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn identity_governance_status_endpoint_returns_combined_overview() {
        let temp_bindings_path = temp_identity_bindings_path("governance-status-bindings");
        let temp_approval_path = temp_identity_bindings_path("governance-status-approval");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"rev-b","chat_users":{},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"approval-a","approved_revisions":["rev-a","rev-b"]}"#,
        )
        .expect("write approval state");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_binding_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_allowed_actors = vec!["alice".to_string()];

        let app = build_router(AppState::new(config));
        let (status, body) = send_identity_governance_status_request(
            &app,
            "/v1/admin/identity-governance/status?limit=1",
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], true);
        assert_eq!(body["governance_valid"], true);
        assert_eq!(body["identity_governance_overview"]["status"], "ok");
        assert_eq!(body["identity_governance_overview"]["valid"], true);
        assert_eq!(
            body["identity_governance_overview"]["checks"]["binding_loaded"],
            true
        );
        assert_eq!(
            body["identity_governance_overview"]["checks"]["actor_gate_valid"],
            true
        );
        assert_eq!(
            body["identity_governance_overview"]["checks"]["approval_source_valid"],
            true
        );
        assert_eq!(
            body["identity_governance_overview"]["checks"]["approval_coverage_valid"],
            true
        );
        assert_eq!(
            body["identity_governance_overview"]["identity_approval_source"]
                ["returned_revision_count"],
            1
        );
        assert_eq!(
            body["identity_governance_overview"]["identity_actor_checks"]["allowed_actor_count"],
            1
        );

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn identity_governance_validate_endpoint_rejects_invalid_actor_gate() {
        let temp_bindings_path = temp_identity_bindings_path("governance-validate-bindings");
        let temp_approval_path = temp_identity_bindings_path("governance-validate-approval");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"rev-b","chat_users":{},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");
        std::fs::write(
            &temp_approval_path,
            r#"{"version":1,"revision":"approval-a","approved_revisions":["rev-b"]}"#,
        )
        .expect("write approval state");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_binding_approved_revisions_path =
            Some(temp_approval_path.to_string_lossy().to_string());
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_allowed_actors = Vec::new();

        let app = build_router(AppState::new(config));
        let (status, body) = send_identity_governance_validate_request(
            &app,
            "/v1/admin/identity-governance/validate",
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["ok"], false);
        assert_eq!(body["validated"], true);
        assert_eq!(body["valid"], false);
        assert_eq!(body["status"], "no_allowed_actors_configured");
        assert_eq!(
            body["identity_governance_overview"]["checks"]["actor_gate_valid"],
            false
        );
        assert_eq!(
            body["identity_governance_overview"]["identity_actor_checks"]["status"],
            "no_allowed_actors_configured"
        );

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_approval_path);
    }

    #[tokio::test]
    async fn identity_actor_status_endpoint_reports_current_actor_gate_configuration() {
        let mut config = test_config();
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_actor_header = "x-deploy-actor".to_string();
        config.identity_binding_reload_allowed_actors =
            vec!["alice".to_string(), "bob".to_string()];

        let app = build_router(AppState::new(config));
        let (status, body) = send_identity_actor_status_request(&app, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], true);
        assert_eq!(body["actor_valid"], true);
        assert_eq!(body["identity_actor_checks"]["status"], "ok");
        assert_eq!(body["identity_actor_checks"]["require_actor"], true);
        assert_eq!(
            body["identity_actor_checks"]["actor_header"],
            "x-deploy-actor"
        );
        assert_eq!(body["identity_actor_checks"]["allowed_actor_count"], 2);
        assert_eq!(body["identity_actor_checks"]["allowed_actors"][0], "alice");
        assert_eq!(body["identity_actor_checks"]["allowed_actors"][1], "bob");
    }

    #[tokio::test]
    async fn identity_actor_validate_endpoint_rejects_missing_allowed_actors() {
        let mut config = test_config();
        config.identity_binding_reload_require_actor = true;
        config.identity_binding_reload_allowed_actors = Vec::new();

        let app = build_router(AppState::new(config));
        let (status, body) = send_identity_actor_validate_request(&app, &[]).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["ok"], false);
        assert_eq!(body["validated"], true);
        assert_eq!(body["valid"], false);
        assert_eq!(body["status"], "no_allowed_actors_configured");
        assert_eq!(body["identity_actor_checks"]["valid"], false);
        assert_eq!(body["identity_actor_checks"]["allowed_actor_count"], 0);
    }

    #[tokio::test]
    async fn identity_registry_audit_endpoint_returns_conflict_when_audit_not_configured() {
        let app = build_router(AppState::new(test_config()));

        let (status, body) =
            send_identity_registry_audit_request(&app, "/v1/admin/identity-registry/audit", &[])
                .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"], "identity_audit_not_configured");
    }

    #[tokio::test]
    async fn identity_registry_audit_endpoint_returns_latest_registry_events_only() {
        let temp_audit_path = temp_identity_bindings_path("registry-audit-read");
        let mut config = test_config();
        config.identity_binding_audit_log_path =
            Some(temp_audit_path.to_string_lossy().to_string());

        let app = build_router(AppState::new(config));
        std::fs::write(
            &temp_audit_path,
            concat!(
                "{\"event_kind\":\"reload\",\"event_epoch\":1}\n",
                "{\"event_kind\":\"registry_reload_rejected\",\"event_epoch\":2}\n",
                "not-json\n",
                "{\"event_kind\":\"registry_reload\",\"event_epoch\":3}\n"
            ),
        )
        .expect("write audit fixture");

        let (status_one, body_one) = send_identity_registry_audit_request(
            &app,
            "/v1/admin/identity-registry/audit?limit=1",
            &[],
        )
        .await;
        assert_eq!(status_one, StatusCode::OK);
        assert_eq!(body_one["ok"], true);
        assert_eq!(body_one["returned_event_count"], 1);
        assert_eq!(body_one["parse_error_count"], 0);
        assert_eq!(body_one["events"][0]["event_kind"], "registry_reload");

        let (status_all, body_all) = send_identity_registry_audit_request(
            &app,
            "/v1/admin/identity-registry/audit?limit=5",
            &[],
        )
        .await;
        assert_eq!(status_all, StatusCode::OK);
        assert_eq!(body_all["returned_event_count"], 2);
        assert_eq!(body_all["parse_error_count"], 1);
        assert_eq!(body_all["events"][0]["event_kind"], "registry_reload");
        assert_eq!(
            body_all["events"][1]["event_kind"],
            "registry_reload_rejected"
        );

        let _ = std::fs::remove_file(&temp_audit_path);
    }

    #[tokio::test]
    async fn reload_identity_bindings_endpoint_appends_audit_event_when_path_configured() {
        let temp_bindings_path = temp_identity_bindings_path("audit-path");
        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"rev-a","chat_users":{"chat-1":{"org_id":"org-old"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");

        let temp_audit_path = temp_identity_bindings_path("audit-log");
        let mut config = test_config();
        config.identity_bindings_path = Some(temp_bindings_path.to_string_lossy().to_string());
        config.identity_binding_audit_log_path =
            Some(temp_audit_path.to_string_lossy().to_string());
        config.identity_binding_reload_reject_same_revision = false;

        let app = build_router(AppState::new(config));

        std::fs::write(
            &temp_bindings_path,
            r#"{"version":1,"revision":"rev-b","chat_users":{"chat-1":{"org_id":"org-new"},"chat-2":{"org_id":"org-new2"}},"matrix_users":{}}"#,
        )
        .expect("write revised identity bindings");

        let (status_ok, body_ok) = send_identity_binding_reload_request(&app, &[]).await;
        assert_eq!(status_ok, StatusCode::OK);
        assert_eq!(
            body_ok["identity_binding_audit"]["path"],
            temp_audit_path.to_string_lossy().to_string()
        );
        assert_eq!(body_ok["identity_binding_audit"]["last_status"], "written");
        assert_eq!(
            body_ok["identity_binding_audit"]["last_event_kind"],
            "reload"
        );
        assert_eq!(
            body_ok["identity_binding_audit"]["last_policy_decision"],
            "accepted"
        );

        let raw_audit = std::fs::read_to_string(&temp_audit_path).expect("read audit log");
        let lines: Vec<&str> = raw_audit
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        assert!(!lines.is_empty(), "expected at least one audit event");
        let event: Value = serde_json::from_str(lines.last().expect("latest audit event exists"))
            .expect("decode audit event jsonl");
        assert_eq!(event["event_kind"], "reload");
        assert_eq!(event["identity_binding_metadata"]["revision"], "rev-b");
        assert_eq!(event["identity_binding_metadata"]["load_status"], "loaded");
        assert_eq!(event["identity_binding_counts"]["chat_users"], 2);
        assert_eq!(event["identity_binding_counts"]["matrix_users"], 0);
        assert_eq!(event["governance"]["accepted"], true);

        let _ = std::fs::remove_file(&temp_bindings_path);
        let _ = std::fs::remove_file(&temp_audit_path);
    }

    #[tokio::test]
    async fn reload_identity_bindings_endpoint_shows_disabled_audit_when_path_not_configured() {
        let temp_path = temp_identity_bindings_path("audit-path-missing");
        std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-a","chat_users":{"chat-1":{"org_id":"org-old"}},"matrix_users":{}}"#,
        )
        .expect("write initial identity bindings");

        let mut config = test_config();
        config.identity_bindings_path = Some(temp_path.to_string_lossy().to_string());
        config.identity_binding_reload_reject_same_revision = false;

        let app = build_router(AppState::new(config));

        std::fs::write(
            &temp_path,
            r#"{"version":1,"revision":"rev-b","chat_users":{"chat-1":{"org_id":"org-new"},"chat-2":{"org_id":"org-new2"}},"matrix_users":{"mx-1":{"org_id":"org-mx"}}}"#,
        )
        .expect("write revised identity bindings");

        let (status_ok, body_ok) = send_identity_binding_reload_request(&app, &[]).await;
        assert_eq!(status_ok, StatusCode::OK);
        assert!(body_ok["identity_binding_audit"]["path"].is_null());
        assert_eq!(body_ok["identity_binding_audit"]["last_status"], "disabled");
        assert!(body_ok["identity_binding_audit"]["last_policy_decision"].is_null());

        let _ = std::fs::remove_file(&temp_path);
    }
}
