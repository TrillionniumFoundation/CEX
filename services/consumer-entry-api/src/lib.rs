#![recursion_limit = "256"]

use axum::{
    extract::{Form, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

const DEFAULT_MAX_TEXT_CHARS: usize = 4_000;
const DEFAULT_RATE_LIMIT_WINDOW_SECS: u64 = 60;
const DEFAULT_RATE_LIMIT_MAX_REQUESTS: usize = 30;
const DEFAULT_REPLAY_WINDOW_SECS: u64 = 600;
const DEFAULT_REPLAY_CACHE_SIZE: usize = 2048;
const DEFAULT_LEAGUE_LLM_JUDGE_TIMEOUT_MS: u64 = 2500;
const DEFAULT_LEAGUE_WEB_SESSION_TTL_SECS: u64 = 3600;
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
    env, fs,
    path::Path as StdPath,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock as StdRwLock,
    },
    time::{Duration, UNIX_EPOCH},
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
    league_state: Mutex<LeagueState>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueMatch {
    match_id: String,
    title: String,
    mode: String,
    status: String,
    objective: String,
    reward: String,
    recommended_roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeaguePlayer {
    player_id: String,
    matrix_user_id: String,
    display_name: String,
    class_tag: String,
    rank_tier: String,
    rating: i64,
    xp: i64,
    reputation: i64,
    battles: i64,
    submissions: i64,
    wins: i64,
    earned_credits: f64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueMatchEntry {
    entry_id: String,
    match_id: String,
    player_id: String,
    matrix_user_id: String,
    status: String,
    battles_started: i64,
    submissions: i64,
    best_score: f64,
    rewards_earned: f64,
    joined_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueBattle {
    battle_id: String,
    match_id: String,
    entry_id: String,
    player_id: String,
    matrix_user_id: String,
    task_id: String,
    prompt: String,
    status: String,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueSubmission {
    submission_id: String,
    match_id: String,
    entry_id: String,
    player_id: String,
    matrix_user_id: String,
    task_id: Option<String>,
    body: String,
    score: f64,
    grade: String,
    reward_amount: f64,
    #[serde(default)]
    judge_status: Option<String>,
    #[serde(default)]
    payout_status: Option<String>,
    #[serde(default)]
    anti_cheat_flags: Vec<String>,
    #[serde(default)]
    score_events: Vec<LeagueScoreEvent>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueScoreEvent {
    dimension: String,
    score: f64,
    weight: f64,
    judge_kind: String,
    evidence: Value,
}

#[derive(Debug, Clone)]
struct LeagueJudgement {
    score: f64,
    grade: String,
    reward_amount: f64,
    judge_status: String,
    payout_status: String,
    anti_cheat_flags: Vec<String>,
    score_events: Vec<LeagueScoreEvent>,
}

#[derive(Debug, Clone, Deserialize)]
struct LeagueExternalJudgeResponse {
    score: Option<f64>,
    grade: Option<String>,
    verdict: Option<String>,
    explanation: Option<String>,
    flags: Option<Vec<String>>,
    evidence: Option<Value>,
}

#[derive(Debug, Clone)]
struct LeagueExternalJudgeOutcome {
    event: LeagueScoreEvent,
    flags: Vec<String>,
    grade_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueReward {
    reward_id: String,
    match_id: String,
    entry_id: String,
    player_id: String,
    matrix_user_id: String,
    amount: f64,
    currency_unit: String,
    reason: String,
    #[serde(default)]
    ledger_status: Option<String>,
    #[serde(default)]
    ledger_account_id: Option<String>,
    #[serde(default)]
    ledger_entry_id: Option<String>,
    #[serde(default)]
    ledger_balance_after: Option<f64>,
    #[serde(default)]
    ledger_error: Option<String>,
    #[serde(default)]
    review_status: Option<String>,
    #[serde(default)]
    reviewed_by: Option<String>,
    #[serde(default)]
    review_note: Option<String>,
    #[serde(default)]
    reviewed_at_epoch: Option<i64>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueInventoryItem {
    item_id: String,
    player_id: String,
    matrix_user_id: String,
    source_submission_id: String,
    item_kind: String,
    name: String,
    rarity: String,
    power: i64,
    cosmetic: bool,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueGuild {
    guild_id: String,
    name: String,
    motto: String,
    status: String,
    rating: i64,
    reputation: i64,
    treasury_credits: f64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueGuildMembership {
    guild_id: String,
    player_id: String,
    matrix_user_id: String,
    role: String,
    joined_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueRaidContribution {
    contribution_id: String,
    match_id: String,
    guild_id: Option<String>,
    player_id: String,
    matrix_user_id: String,
    room_id: Option<String>,
    role: String,
    body: String,
    contribution_score: f64,
    progress_delta: f64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueRaidRosterSlot {
    slot_id: String,
    match_id: String,
    guild_id: Option<String>,
    player_id: String,
    matrix_user_id: String,
    room_id: Option<String>,
    role: String,
    hero_id: String,
    status: String,
    joined_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldZone {
    zone_id: String,
    name: String,
    status: String,
    theme: String,
    mirror_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldLocation {
    location_id: String,
    zone_id: String,
    name: String,
    location_kind: String,
    description: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldEntity {
    entity_id: String,
    location_id: String,
    name: String,
    entity_kind: String,
    role: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldAsset {
    asset_id: String,
    owner_matrix_user_id: String,
    location_id: String,
    asset_kind: String,
    name: String,
    status: String,
    value_score: i64,
    #[serde(default)]
    upgrade_level: i64,
    #[serde(default)]
    upgrade_points: i64,
    #[serde(default)]
    last_upgrade_kind: Option<String>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldAssetUpgrade {
    upgrade_id: String,
    asset_id: String,
    matrix_user_id: String,
    body: String,
    upgrade_kind: String,
    score: f64,
    grade: String,
    judge_status: String,
    status: String,
    value_delta: i64,
    level_before: i64,
    level_after: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldCompany {
    company_id: String,
    owner_matrix_user_id: String,
    asset_id: String,
    location_id: String,
    name: String,
    company_kind: String,
    status: String,
    revenue_score: i64,
    reputation_score: i64,
    level: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldShop {
    shop_id: String,
    company_id: String,
    owner_matrix_user_id: String,
    location_id: String,
    name: String,
    shop_kind: String,
    status: String,
    listing_count: i64,
    gross_merchandise_score: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldListing {
    listing_id: String,
    shop_id: String,
    company_id: String,
    owner_matrix_user_id: String,
    asset_id: String,
    title: String,
    listing_kind: String,
    status: String,
    price_credits: i64,
    quality_score: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldEconomyEvent {
    economy_event_id: String,
    matrix_user_id: String,
    event_kind: String,
    subject_id: String,
    credits_delta: i64,
    reputation_delta: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldEvent {
    event_id: String,
    actor_matrix_user_id: String,
    room_id: Option<String>,
    location_id: String,
    event_kind: String,
    body: String,
    result: String,
    impact_score: i64,
    #[serde(default)]
    cex_task_id: Option<String>,
    #[serde(default)]
    cex_status: Option<String>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldContract {
    contract_id: String,
    event_id: String,
    actor_matrix_user_id: String,
    location_id: String,
    task_id: String,
    title: String,
    body: String,
    status: String,
    cex_status: Option<String>,
    value_score: i64,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldContractCompletion {
    completion_id: String,
    contract_id: String,
    matrix_user_id: String,
    body: String,
    score: f64,
    grade: String,
    reward_amount: f64,
    judge_status: String,
    payout_status: String,
    anti_cheat_flags: Vec<String>,
    score_events: Vec<LeagueScoreEvent>,
    ledger_status: Option<String>,
    ledger_account_id: Option<String>,
    ledger_entry_id: Option<String>,
    ledger_balance_after: Option<f64>,
    ledger_error: Option<String>,
    created_at_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldRelationship {
    relationship_id: String,
    from_id: String,
    to_id: String,
    relation_kind: String,
    strength: i64,
    updated_at_epoch: i64,
}

#[derive(Debug, Deserialize)]
struct WorldActionRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    location_id: Option<String>,
    body: String,
    message: Option<String>,
    event_id: Option<String>,
    capability_id: Option<String>,
    account_id: Option<String>,
    cex_task_id: Option<String>,
    cex_status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LeagueDraftRequest {
    matrix_user_id: String,
    heroes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LeagueState {
    #[serde(default)]
    matches: HashMap<String, LeagueMatch>,
    #[serde(default)]
    players_by_matrix_user: HashMap<String, LeaguePlayer>,
    #[serde(default)]
    entries: HashMap<String, LeagueMatchEntry>,
    #[serde(default)]
    battles: HashMap<String, LeagueBattle>,
    #[serde(default)]
    submissions: HashMap<String, LeagueSubmission>,
    #[serde(default)]
    rewards: Vec<LeagueReward>,
    #[serde(default)]
    player_loadouts: HashMap<String, Vec<String>>,
    #[serde(default)]
    guilds: HashMap<String, LeagueGuild>,
    #[serde(default)]
    guild_memberships: HashMap<String, LeagueGuildMembership>,
    #[serde(default)]
    inventory_items: Vec<LeagueInventoryItem>,
    #[serde(default)]
    raid_contributions: Vec<LeagueRaidContribution>,
    #[serde(default)]
    raid_rosters: Vec<LeagueRaidRosterSlot>,
    #[serde(default)]
    world_zones: HashMap<String, WorldZone>,
    #[serde(default)]
    world_locations: HashMap<String, WorldLocation>,
    #[serde(default)]
    world_entities: HashMap<String, WorldEntity>,
    #[serde(default)]
    world_assets: Vec<WorldAsset>,
    #[serde(default)]
    world_asset_upgrades: Vec<WorldAssetUpgrade>,
    #[serde(default)]
    world_companies: Vec<WorldCompany>,
    #[serde(default)]
    world_shops: Vec<WorldShop>,
    #[serde(default)]
    world_listings: Vec<WorldListing>,
    #[serde(default)]
    world_economy_events: Vec<WorldEconomyEvent>,
    #[serde(default)]
    world_events: Vec<WorldEvent>,
    #[serde(default)]
    world_contracts: Vec<WorldContract>,
    #[serde(default)]
    world_contract_completions: Vec<WorldContractCompletion>,
    #[serde(default)]
    world_relationships: Vec<WorldRelationship>,
}

#[derive(Debug, Deserialize)]
struct LeagueJoinRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LeagueBattleRequest {
    matrix_user_id: String,
    room_id: String,
    message: String,
    event_id: Option<String>,
    capability_id: Option<String>,
    account_id: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct LeagueSubmitRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    task_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct LeagueRaidContributionRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    role: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct LeagueRaidRosterRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    role: Option<String>,
    hero_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LeagueReviewRequest {
    reviewer_id: Option<String>,
    room_id: Option<String>,
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LeagueWebActionRequest {
    action: String,
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    match_id: Option<String>,
    guild_id: Option<String>,
    role: Option<String>,
    heroes: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldWebActionRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    location_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldContractCompleteRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWebContractCompleteRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    contract_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldAssetUpgradeRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWebAssetUpgradeRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    asset_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldCompanyRequest {
    matrix_user_id: String,
    asset_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWebCompanyRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    asset_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorldListingRequest {
    matrix_user_id: String,
    company_id: Option<String>,
    body: String,
}

#[derive(Debug, Deserialize)]
struct WorldWebListingRequest {
    matrix_user_id: Option<String>,
    csrf: Option<String>,
    company_id: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LeagueWebSessionRequest {
    matrix_user_id: String,
    room_id: Option<String>,
    session_id: Option<String>,
    csrf: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueWebSessionClaims {
    version: u32,
    matrix_user_id: String,
    room_id: Option<String>,
    session_id: Option<String>,
    csrf: String,
    issued_at_epoch: i64,
    expires_at_epoch: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RateLimitCache {
    seen: HashMap<String, VecDeque<i64>>,
}

fn default_league_state() -> LeagueState {
    let matches = [
        LeagueMatch {
            match_id: "daily-dungeon-001".to_string(),
            title: "Prompt Forge 入门战".to_string(),
            mode: "daily_dungeon".to_string(),
            status: "open".to_string(),
            objective: "用最少成本生成一个可交付方案，并给出自评/风险。".to_string(),
            reward: "XP + credits".to_string(),
            recommended_roles: vec![
                "strategist".to_string(),
                "builder".to_string(),
                "auditor".to_string(),
            ],
        },
        LeagueMatch {
            match_id: "bounty-arena-001".to_string(),
            title: "赏金赛：真实客户任务预备场".to_string(),
            mode: "bounty_arena".to_string(),
            status: "preview".to_string(),
            objective: "多人提交方案，按质量、速度、成本和客户适配评分。".to_string(),
            reward: "Prize Pool".to_string(),
            recommended_roles: vec![
                "scout".to_string(),
                "builder".to_string(),
                "closer".to_string(),
            ],
        },
        LeagueMatch {
            match_id: "guild-raid-001".to_string(),
            title: "公会团本：多阶段交付".to_string(),
            mode: "guild_raid".to_string(),
            status: "preview".to_string(),
            objective: "团队分工完成调研、构建、审核和交付。".to_string(),
            reward: "Contribution split".to_string(),
            recommended_roles: vec![
                "scout".to_string(),
                "builder".to_string(),
                "auditor".to_string(),
                "closer".to_string(),
            ],
        },
    ]
    .into_iter()
    .map(|league_match| (league_match.match_id.clone(), league_match))
    .collect();

    let now = Utc::now().timestamp();
    let guilds = [
        LeagueGuild {
            guild_id: "guild-prompt-forge".to_string(),
            name: "Prompt Forge".to_string(),
            motto: "Draft fast. Ship clean.".to_string(),
            status: "open".to_string(),
            rating: 1000,
            reputation: 0,
            treasury_credits: 0.0,
            created_at_epoch: now,
        },
        LeagueGuild {
            guild_id: "guild-audit-sanctum".to_string(),
            name: "Audit Sanctum".to_string(),
            motto: "No hallucination survives the raid.".to_string(),
            status: "open".to_string(),
            rating: 1000,
            reputation: 0,
            treasury_credits: 0.0,
            created_at_epoch: now,
        },
    ]
    .into_iter()
    .map(|guild| (guild.guild_id.clone(), guild))
    .collect();

    let world_zones = [
        WorldZone {
            zone_id: "reality-mirror-city".to_string(),
            name: "Reality Mirror City".to_string(),
            status: "open".to_string(),
            theme: "现实世界映射、身份、关系和城市自由行动".to_string(),
            mirror_kind: "city".to_string(),
        },
        WorldZone {
            zone_id: "craft-district".to_string(),
            name: "Trillionnium Craft District".to_string(),
            status: "open".to_string(),
            theme: "建造、工坊、资产、店铺和创造系统".to_string(),
            mirror_kind: "builder_sandbox".to_string(),
        },
        WorldZone {
            zone_id: "market-bazaar".to_string(),
            name: "Market Bazaar".to_string(),
            status: "open".to_string(),
            theme: "真实任务、客户、交易、雇佣和声望".to_string(),
            mirror_kind: "market".to_string(),
        },
        WorldZone {
            zone_id: "league-arena".to_string(),
            name: "Trillionnium League Arena".to_string(),
            status: "open".to_string(),
            theme: "竞技、团本、赛季和裁判结算".to_string(),
            mirror_kind: "arena".to_string(),
        },
    ]
    .into_iter()
    .map(|zone| (zone.zone_id.clone(), zone))
    .collect();

    let world_locations = [
        WorldLocation {
            location_id: "mirror-city-square".to_string(),
            zone_id: "reality-mirror-city".to_string(),
            name: "镜像城市广场".to_string(),
            location_kind: "public_hub".to_string(),
            description: "玩家、Agent 居民、公会和现实事件进入世界的公共入口。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "starter-studio".to_string(),
            zone_id: "craft-district".to_string(),
            name: "Starter Studio".to_string(),
            location_kind: "workshop".to_string(),
            description: "自由建造第一间店、工作室、Agent 工坊或公司总部。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "zbj-market-gate".to_string(),
            zone_id: "market-bazaar".to_string(),
            name: "ZBJ Market Gate".to_string(),
            location_kind: "real_task_gateway".to_string(),
            description: "现实任务和客户需求映射为世界委托的入口。".to_string(),
            status: "open".to_string(),
        },
        WorldLocation {
            location_id: "league-coliseum".to_string(),
            zone_id: "league-arena".to_string(),
            name: "League Coliseum".to_string(),
            location_kind: "arena".to_string(),
            description: "League 赛事、团本、评分、奖励与排行榜发生地。".to_string(),
            status: "open".to_string(),
        },
    ]
    .into_iter()
    .map(|location| (location.location_id.clone(), location))
    .collect();

    let world_entities = [
        WorldEntity {
            entity_id: "agent-oracle-scout".to_string(),
            location_id: "mirror-city-square".to_string(),
            name: "Oracle Scout".to_string(),
            entity_kind: "agent_resident".to_string(),
            role: "侦察、现实情报、任务发现".to_string(),
            status: "available".to_string(),
        },
        WorldEntity {
            entity_id: "agent-forge-builder".to_string(),
            location_id: "starter-studio".to_string(),
            name: "Forge Builder".to_string(),
            entity_kind: "agent_resident".to_string(),
            role: "建造、生成、工坊资产".to_string(),
            status: "available".to_string(),
        },
        WorldEntity {
            entity_id: "city-clerk-ledger".to_string(),
            location_id: "mirror-city-square".to_string(),
            name: "Ledger Clerk".to_string(),
            entity_kind: "npc".to_string(),
            role: "资产登记、声望、合约和结算提示".to_string(),
            status: "available".to_string(),
        },
    ]
    .into_iter()
    .map(|entity| (entity.entity_id.clone(), entity))
    .collect();

    LeagueState {
        matches,
        players_by_matrix_user: HashMap::new(),
        entries: HashMap::new(),
        battles: HashMap::new(),
        submissions: HashMap::new(),
        rewards: Vec::new(),
        player_loadouts: HashMap::new(),
        guilds,
        guild_memberships: HashMap::new(),
        inventory_items: Vec::new(),
        raid_contributions: Vec::new(),
        raid_rosters: Vec::new(),
        world_zones,
        world_locations,
        world_entities,
        world_assets: Vec::new(),
        world_asset_upgrades: Vec::new(),
        world_companies: Vec::new(),
        world_shops: Vec::new(),
        world_listings: Vec::new(),
        world_economy_events: Vec::new(),
        world_events: Vec::new(),
        world_contracts: Vec::new(),
        world_contract_completions: Vec::new(),
        world_relationships: Vec::new(),
    }
}

fn load_league_state(config: &ConsumerEntryConfig) -> LeagueState {
    let mut state = config
        .league_state_path
        .as_deref()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str::<LeagueState>(&raw).ok())
        .unwrap_or_else(default_league_state);

    let defaults = default_league_state();
    for (match_id, league_match) in defaults.matches {
        state.matches.entry(match_id).or_insert(league_match);
    }
    for (guild_id, guild) in defaults.guilds {
        state.guilds.entry(guild_id).or_insert(guild);
    }
    for (zone_id, zone) in defaults.world_zones {
        state.world_zones.entry(zone_id).or_insert(zone);
    }
    for (location_id, location) in defaults.world_locations {
        state.world_locations.entry(location_id).or_insert(location);
    }
    for (entity_id, entity) in defaults.world_entities {
        state.world_entities.entry(entity_id).or_insert(entity);
    }
    state
}

fn sql_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn league_state_hash(league: &LeagueState) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(league)?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn league_state_sql_snapshot_bytes(league: &LeagueState) -> Result<Vec<u8>, serde_json::Error> {
    let state_json = serde_json::to_string_pretty(league)?;
    let state_hash = league_state_hash(league)?;
    let generated_at = Utc::now().to_rfc3339();
    let sql = format!(
        "-- Trillionnium League SQL-ready snapshot.\n\
         -- Generated by consumer-entry-api at {generated_at}.\n\
         -- Apply migrations/0011_add_trillionnium_league_state_snapshots.sql before loading.\n\
         begin;\n\
         insert into league_state_snapshots (snapshot_kind, state_hash, state)\n\
         values ('consumer_entry_json_v1', '{}', '{}'::jsonb);\n\
         commit;\n",
        sql_quote(&state_hash),
        sql_quote(&state_json),
    );
    Ok(sql.into_bytes())
}

async fn write_file_with_parent(path: &str, bytes: Vec<u8>, label: &str) -> Result<(), Response> {
    if let Some(parent) = StdPath::new(path).parent() {
        if let Err(err) = tokio::fs::create_dir_all(parent).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to create {label} dir: {err}") })),
            )
                .into_response());
        }
    }
    tokio::fs::write(path, bytes).await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to persist {label}: {err}") })),
        )
            .into_response()
    })
}

async fn persist_league_state(state: &AppState, league: &LeagueState) -> Result<(), Response> {
    if let Some(path) = state.config().league_state_path.as_deref() {
        let bytes = serde_json::to_vec_pretty(league).map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to serialize league state: {err}") })),
            )
                .into_response()
        })?;
        write_file_with_parent(path, bytes, "league state").await?;
    }
    if let Some(path) = state.config().league_sql_snapshot_path.as_deref() {
        let bytes = league_state_sql_snapshot_bytes(league).map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to serialize league SQL snapshot: {err}") })),
            )
                .into_response()
        })?;
        write_file_with_parent(path, bytes, "league SQL snapshot").await?;
    }
    Ok(())
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
    pub ledger_base_url: String,
    pub ledger_admin_token: Option<String>,
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
    pub league_state_path: Option<String>,
    pub league_sql_snapshot_path: Option<String>,
    pub league_hidden_tests_enabled: bool,
    pub league_llm_judge_url: Option<String>,
    pub league_llm_judge_token: Option<String>,
    pub league_llm_judge_required: bool,
    pub league_llm_judge_timeout_ms: u64,
    pub league_web_session_required: bool,
    pub league_web_session_secret: Option<String>,
    pub league_web_session_cookie_name: String,
    pub league_web_session_ttl_secs: u64,
}

impl ConsumerEntryConfig {
    pub fn from_env() -> Self {
        let session_auth_issuer_registry_path = first_present_env(&[
            "CONSUMER_ENTRY_SESSION_AUTH_ISSUER_REGISTRY_PATH",
            "CEX_SESSION_AUTH_ISSUER_REGISTRY_PATH",
        ]);
        let (session_auth_issuer_registry_metadata, session_auth_issuer_registry) =
            load_session_auth_issuer_registry(session_auth_issuer_registry_path.as_deref());
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
            ledger_base_url: first_present_env(&[
                "CONSUMER_ENTRY_LEDGER_BASE_URL",
                "LEDGER_BASE_URL",
            ])
            .unwrap_or_else(|| "http://127.0.0.1:7002".to_string()),
            ledger_admin_token: first_present_env(&[
                "CONSUMER_ENTRY_LEDGER_ADMIN_TOKEN",
                "LEDGER_ADMIN_TOKEN",
            ])
            .or_else(parse_first_ledger_admin_token_from_env),
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
            session_auth_issuer_keys: env::var("CONSUMER_ENTRY_SESSION_AUTH_ISSUER_KEYS_JSON")
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
            league_state_path: first_present_env(&[
                "CONSUMER_ENTRY_LEAGUE_STATE_PATH",
                "CEX_LEAGUE_STATE_PATH",
            ]),
            league_sql_snapshot_path: first_present_env(&[
                "CONSUMER_ENTRY_LEAGUE_SQL_SNAPSHOT_PATH",
                "CEX_LEAGUE_SQL_SNAPSHOT_PATH",
            ]),
            league_hidden_tests_enabled: boolean_env("CONSUMER_ENTRY_LEAGUE_HIDDEN_TESTS", true),
            league_llm_judge_url: first_present_env(&[
                "CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_URL",
                "CEX_LEAGUE_LLM_JUDGE_URL",
            ]),
            league_llm_judge_token: first_present_env(&[
                "CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_TOKEN",
                "CEX_LEAGUE_LLM_JUDGE_TOKEN",
            ]),
            league_llm_judge_required: boolean_env(
                "CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_REQUIRED",
                false,
            ),
            league_llm_judge_timeout_ms: positive_u64_env(
                "CONSUMER_ENTRY_LEAGUE_LLM_JUDGE_TIMEOUT_MS",
                DEFAULT_LEAGUE_LLM_JUDGE_TIMEOUT_MS,
            ),
            league_web_session_required: boolean_env(
                "CONSUMER_ENTRY_LEAGUE_WEB_SESSION_REQUIRED",
                !matches!(RuntimeProfile::from_env(), RuntimeProfile::LocalDev),
            ),
            league_web_session_secret: first_present_env(&[
                "CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET",
                "CONSUMER_ENTRY_WEB_SESSION_SECRET",
            ]),
            league_web_session_cookie_name: env::var("CONSUMER_ENTRY_LEAGUE_WEB_SESSION_COOKIE")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "cex_league_session".to_string()),
            league_web_session_ttl_secs: positive_u64_env(
                "CONSUMER_ENTRY_LEAGUE_WEB_SESSION_TTL_SECS",
                DEFAULT_LEAGUE_WEB_SESSION_TTL_SECS,
            ),
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
            if self.league_web_session_required && league_web_session_secret(self).is_none() {
                errors.push(
                    "beta/production League web actions require CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET or CONSUMER_ENTRY_SESSION_AUTH_SECRET"
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

fn parse_first_ledger_admin_token_from_env() -> Option<String> {
    let value = env::var("LEDGER_ADMIN_TOKENS_JSON").ok()?;
    let parsed: Value = serde_json::from_str(&value).ok()?;

    match parsed {
        Value::Array(items) => items.into_iter().find_map(|item| {
            item.get("token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(ToString::to_string)
        }),
        Value::Object(map) => map.keys().next().cloned(),
        _ => None,
    }
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
    let Ok(parsed) = serde_json::from_str::<HashMap<String, HashMap<String, String>>>(value) else {
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
        let league_state = load_league_state(&config);
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
                session_auth_issuer_registry_state: StdRwLock::new(
                    session_auth_issuer_registry_state,
                ),
                league_state: Mutex::new(league_state),
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
        .route("/league", get(get_league_web_shell))
        .route("/world", get(get_world_web_shell))
        .route("/league/web/session", post(post_league_web_session))
        .route("/league/web/action", post(post_league_web_action))
        .route("/world/web/action", post(post_world_web_action))
        .route(
            "/world/web/contract",
            post(post_world_web_contract_complete),
        )
        .route("/world/web/asset", post(post_world_web_asset_upgrade))
        .route("/world/web/company", post(post_world_web_company))
        .route("/world/web/listing", post(post_world_web_listing))
        .route("/v1/chat/tasks", post(create_chat_task))
        .route("/v1/chat/tasks/:id", get(get_chat_task))
        .route("/v1/matrix/messages", post(create_matrix_message_task))
        .route("/v1/league/home", get(get_league_home))
        .route("/v1/league/world", get(get_league_world))
        .route("/v1/world/home", get(get_world_home))
        .route("/v1/world/action", post(post_world_action))
        .route("/v1/world/assets", get(get_world_assets))
        .route(
            "/v1/world/assets/:asset_id/upgrade",
            post(upgrade_world_asset),
        )
        .route(
            "/v1/world/companies",
            get(get_world_companies).post(create_world_company),
        )
        .route("/v1/world/shops", get(get_world_shops))
        .route("/v1/world/listings", post(create_world_listing))
        .route("/v1/world/contracts", get(get_world_contracts))
        .route(
            "/v1/world/contracts/:contract_id/complete",
            post(complete_world_contract),
        )
        .route("/v1/league/season", get(get_league_season))
        .route("/v1/league/matches", get(get_league_matches))
        .route("/v1/league/state/snapshot", get(get_league_state_snapshot))
        .route("/v1/league/raids", get(get_league_raids))
        .route(
            "/v1/league/raids/:match_id/contribute",
            post(contribute_league_raid),
        )
        .route(
            "/v1/league/raids/:match_id/roster",
            get(get_league_raid_roster).post(join_league_raid_roster),
        )
        .route("/v1/league/reviews/held", get(get_league_held_reviews))
        .route(
            "/v1/league/reviews/:reward_id/approve",
            post(approve_league_review),
        )
        .route(
            "/v1/league/reviews/:reward_id/reject",
            post(reject_league_review),
        )
        .route("/v1/league/guilds", get(get_league_guilds))
        .route("/v1/league/guilds/:guild_id/join", post(join_league_guild))
        .route("/v1/league/matches/:match_id/join", post(join_league_match))
        .route(
            "/v1/league/matches/:match_id/battle",
            post(create_league_battle),
        )
        .route(
            "/v1/league/matches/:match_id/submit",
            post(submit_league_match),
        )
        .route("/v1/league/rankings", get(get_league_rankings))
        .route(
            "/v1/league/players/:matrix_user_id/profile",
            get(get_league_player_profile),
        )
        .route(
            "/v1/league/players/:matrix_user_id/loadout",
            get(get_league_player_loadout),
        )
        .route(
            "/v1/league/players/:matrix_user_id/draft",
            post(update_league_player_draft),
        )
        .route(
            "/v1/league/players/:matrix_user_id/rewards",
            get(get_league_player_rewards),
        )
        .route(
            "/v1/league/players/:matrix_user_id/inventory",
            get(get_league_player_inventory),
        )
        .route(
            "/v1/league/players/:matrix_user_id/history",
            get(get_league_player_history),
        )
        .route(
            "/v1/matrix/users/:matrix_user_id/wallet",
            get(get_matrix_wallet),
        )
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
        let status = match (
            current_active_key_id.as_ref(),
            candidate_active_key_id.as_ref(),
        ) {
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
            entry
                .active_key_id
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
    } else if config
        .session_auth_issuer_registry_allowed_actors
        .is_empty()
    {
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
    let approval_coverage_valid =
        if !registry_configured || !config.session_auth_issuer_registry_require_approved_revision {
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
    let (candidate_metadata, candidate_registry) = load_session_auth_issuer_registry(
        state.config().session_auth_issuer_registry_path.as_deref(),
    );
    let candidate_status = session_auth_issuer_registry_status_json(
        state.config(),
        &candidate_metadata,
        &candidate_registry,
    );
    let approval_state = load_session_auth_issuer_registry_revision_approval_state(state.config());
    let actor_checks = session_auth_issuer_registry_actor_checks_json(state.config());
    let requesting_actor = headers
        .get(
            state
                .config()
                .session_auth_issuer_registry_actor_header
                .as_str(),
        )
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
    let matches_loaded_status =
        candidate_metadata.load_status == current_state.metadata.load_status;
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
        .get(
            state
                .config()
                .session_auth_issuer_registry_actor_header
                .as_str(),
        )
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

async fn get_league_home(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_home",
            "league": "trillionnium_league",
            "name": "Trillionnium League",
            "season": "preseason-zero",
            "status": "preseason",
            "tagline": "AI Agent esports for real work, skill, and earnings.",
            "player_count": league.players_by_matrix_user.len(),
            "match_count": league.matches.len(),
            "commands": ["/arena", "/quest", "/join <match-id>", "/battle <match-id> <action>", "/rank", "/loadout", "/wallet"]
        })),
    )
        .into_response()
}

async fn get_league_world(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_world",
            "league": "trillionnium_league",
            "zones": [
                {"zone_id": "prompt-forge", "name": "Prompt Forge", "status": "open", "theme": "drafting and agent tuning"},
                {"zone_id": "research-wilds", "name": "Research Wilds", "status": "preview", "theme": "sourcing and evidence"},
                {"zone_id": "code-citadel", "name": "Code Citadel", "status": "preview", "theme": "tests and execution"},
                {"zone_id": "audit-sanctum", "name": "Audit Sanctum", "status": "open", "theme": "review and anti-hallucination"},
                {"zone_id": "market-bazaar", "name": "Market Bazaar", "status": "preview", "theme": "bounties and client tasks"}
            ],
            "match_count": league.matches.len(),
            "guild_count": league.guilds.len(),
        })),
    )
        .into_response()
}

fn world_action_kind(body: &str) -> (&'static str, &'static str, i64) {
    let lower = body.to_ascii_lowercase();
    if lower.contains("contract")
        || lower.contains("bounty")
        || lower.contains("commission")
        || body.contains("委托")
        || body.contains("悬赏")
        || body.contains("任务")
    {
        (
            "contract",
            "把现实需求登记成 World Contract，并生成可执行的 CEX 委托任务。",
            20,
        )
    } else if lower.contains("company")
        || lower.contains("shop")
        || lower.contains("studio")
        || body.contains("公司")
        || body.contains("店")
        || body.contains("工作室")
    {
        (
            "venture",
            "创建了一个现实映射经营体，获得资产雏形和声望入口。",
            18,
        )
    } else if lower.contains("build")
        || lower.contains("craft")
        || body.contains("建")
        || body.contains("造")
        || body.contains("工坊")
    {
        (
            "craft",
            "在 Craft District 完成一次建造/创造行动，生成可迭代资产。",
            14,
        )
    } else if lower.contains("hire")
        || lower.contains("agent")
        || body.contains("招募")
        || body.contains("雇佣")
    {
        (
            "recruit",
            "与 Agent 居民建立合作关系，队伍能力获得提升。",
            12,
        )
    } else if lower.contains("market")
        || lower.contains("client")
        || body.contains("客户")
        || body.contains("接单")
        || body.contains("市场")
    {
        (
            "market",
            "进入 Market Bazaar，把现实机会映射为世界委托。",
            16,
        )
    } else {
        (
            "explore",
            "完成一次开放世界探索，产生新的线索和关系变化。",
            10,
        )
    }
}

fn world_default_location_for_kind(kind: &str) -> &'static str {
    match kind {
        "venture" | "market" | "contract" => "zbj-market-gate",
        "craft" => "starter-studio",
        "recruit" => "mirror-city-square",
        _ => "mirror-city-square",
    }
}

fn world_home_json(league: &LeagueState) -> Value {
    let mut zones: Vec<WorldZone> = league.world_zones.values().cloned().collect();
    zones.sort_by(|left, right| left.zone_id.cmp(&right.zone_id));
    let mut locations: Vec<WorldLocation> = league.world_locations.values().cloned().collect();
    locations.sort_by(|left, right| left.location_id.cmp(&right.location_id));
    let mut entities: Vec<WorldEntity> = league.world_entities.values().cloned().collect();
    entities.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
    let recent_events: Vec<WorldEvent> =
        league.world_events.iter().rev().take(8).cloned().collect();
    json!({
        "kind": "trillionnium_world",
        "world": "trillionnium_world",
        "tagline": "现实世界被游戏引擎化：城市、工坊、市场、Agent 居民、资产和自由行动。",
        "modules": {
            "league": "Trillionnium League",
            "craft": "Trillionnium Craft",
            "ledger": "Trillionnium Ledger",
            "agents": "Trillionnium Agents"
        },
        "zones": zones,
        "locations": locations,
        "entities": entities,
        "assets": league.world_assets,
        "asset_upgrades": league.world_asset_upgrades,
        "companies": league.world_companies,
        "shops": league.world_shops,
        "listings": league.world_listings,
        "economy_events": league.world_economy_events,
        "contracts": league.world_contracts,
        "contract_completions": league.world_contract_completions,
        "recent_events": recent_events,
        "counts": {
            "zones": league.world_zones.len(),
            "locations": league.world_locations.len(),
            "entities": league.world_entities.len(),
            "assets": league.world_assets.len(),
            "asset_upgrades": league.world_asset_upgrades.len(),
            "companies": league.world_companies.len(),
            "shops": league.world_shops.len(),
            "listings": league.world_listings.len(),
            "economy_events": league.world_economy_events.len(),
            "contracts": league.world_contracts.len(),
            "contract_completions": league.world_contract_completions.len(),
            "events": league.world_events.len(),
            "relationships": league.world_relationships.len(),
        }
    })
}

async fn get_world_home(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    (StatusCode::OK, Json(world_home_json(&league))).into_response()
}

async fn create_world_contract_task(
    state: &AppState,
    headers: &HeaderMap,
    payload: &WorldActionRequest,
    matrix_user_id: &str,
    body: &str,
) -> Result<ConsumerTaskResponse, Response> {
    let room_id = payload
        .room_id
        .clone()
        .unwrap_or_else(|| "!world-contract:local.dev".to_string());
    let contract_prompt = format!(
        "Trillionnium World Contract: {body}\n\n请把这条现实镜像委托转成可执行交付计划，包含目标、证据、风险、下一步和验收标准。"
    );
    let matrix_payload = MatrixMessageRequest {
        matrix_user_id: matrix_user_id.to_string(),
        room_id,
        session_id: None,
        org_id: None,
        message: contract_prompt,
        capability_id: payload.capability_id.clone(),
        account_id: payload.account_id.clone(),
        event_id: payload.event_id.clone(),
        idempotency_key: None,
        metadata: Some(json!({
            "world": "trillionnium_world",
            "module": "world_contract",
            "location_id": payload.location_id,
            "source_body": body,
        })),
    };
    let resolved_identity = resolve_matrix_identity(state, &matrix_payload).await?;
    let request_fingerprint = build_world_action_request_fingerprint(payload);
    let authorized_session = authorize_user_session(
        state,
        headers,
        &resolved_identity.scope,
        request_fingerprint.as_str(),
    )?;
    let prompt = validate_text_payload(&matrix_payload.message, state.config().max_text_chars)?;
    let resolved_account_id = resolved_identity.scope.account_id.clone();
    let source = json!({
        "kind": "trillionnium_world_contract",
        "world": "trillionnium_world",
        "identity_scope": resolved_identity.scope,
        "identity_resolution": resolved_identity.resolution,
        "matrix_user_id": matrix_user_id,
        "room_id": matrix_payload.room_id,
        "event_id": matrix_payload.event_id,
        "metadata": matrix_payload.metadata,
        "session_auth": authorized_session,
    });
    forward_to_cex_task(
        state.clone(),
        matrix_payload.capability_id,
        resolved_account_id,
        source,
        prompt,
    )
    .await
}

async fn record_world_action(
    state: &AppState,
    payload: WorldActionRequest,
) -> Result<(LeagueState, WorldEvent, Option<WorldContract>, Value), Response> {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response())
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return Err(response),
    };
    let (kind, result, impact) = world_action_kind(&body);
    let snapshot = {
        let mut league = state.inner.league_state.lock().await;
        let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
        let location_id = payload
            .location_id
            .as_deref()
            .filter(|location_id| league.world_locations.contains_key(*location_id))
            .unwrap_or_else(|| world_default_location_for_kind(kind))
            .to_string();
        let now = Utc::now().timestamp();
        let event = WorldEvent {
            event_id: league_hash_id(
                "world-event",
                &format!("{}:{}:{}", matrix_user_id, now, body),
            ),
            actor_matrix_user_id: matrix_user_id.clone(),
            room_id: payload.room_id.clone(),
            location_id: location_id.clone(),
            event_kind: kind.to_string(),
            body: body.clone(),
            result: result.to_string(),
            impact_score: impact,
            cex_task_id: payload.cex_task_id.clone(),
            cex_status: payload.cex_status.clone(),
            created_at_epoch: now,
        };
        let mut created_contract = None;
        if let Some(task_id) = payload.cex_task_id.clone() {
            let contract = WorldContract {
                contract_id: league_hash_id("world-contract", &event.event_id),
                event_id: event.event_id.clone(),
                actor_matrix_user_id: matrix_user_id.clone(),
                location_id: location_id.clone(),
                task_id,
                title: "World Contract".to_string(),
                body: body.clone(),
                status: "task_created".to_string(),
                cex_status: payload.cex_status.clone(),
                value_score: impact,
                created_at_epoch: now,
            };
            league.world_contracts.push(contract.clone());
            created_contract = Some(contract);
        }
        if matches!(kind, "venture" | "craft") {
            league.world_assets.push(WorldAsset {
                asset_id: league_hash_id(
                    "world-asset",
                    &format!("{}:{}:{}", matrix_user_id, kind, now),
                ),
                owner_matrix_user_id: matrix_user_id.clone(),
                location_id: location_id.clone(),
                asset_kind: kind.to_string(),
                name: if kind == "venture" {
                    "Reality Venture Seed".to_string()
                } else {
                    "Craft Build Seed".to_string()
                },
                status: "active".to_string(),
                value_score: impact,
                upgrade_level: 1,
                upgrade_points: impact,
                last_upgrade_kind: Some(kind.to_string()),
                created_at_epoch: now,
            });
        }
        league.world_relationships.push(WorldRelationship {
            relationship_id: league_hash_id(
                "world-rel",
                &format!("{}:{}:{}", matrix_user_id, location_id, now),
            ),
            from_id: matrix_user_id.clone(),
            to_id: location_id.clone(),
            relation_kind: kind.to_string(),
            strength: impact,
            updated_at_epoch: now,
        });
        player.xp += impact;
        player.reputation += (impact / 4).max(1);
        player.rating += (impact / 3).max(1);
        league
            .players_by_matrix_user
            .insert(matrix_user_id.clone(), player);
        league.world_events.push(event.clone());
        let home = world_home_json(&league);
        (league.clone(), event, created_contract, home)
    };
    Ok(snapshot)
}

async fn post_world_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut payload): Json<WorldActionRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let (kind, _result, _impact) = world_action_kind(&body);
    let task = if kind == "contract" && payload.cex_task_id.is_none() {
        match create_world_contract_task(&state, &headers, &payload, &matrix_user_id, &body).await {
            Ok(task) => {
                payload.cex_task_id = Some(task.task_id.clone());
                payload.cex_status = Some(task.consumer_status.clone());
                Some(task)
            }
            Err(response) => return response,
        }
    } else {
        None
    };
    let snapshot = match record_world_action(&state, payload).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(response) = persist_league_state(&state, &snapshot.0).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_action",
            "world": "trillionnium_world",
            "event": snapshot.1,
            "contract": snapshot.2,
            "task": task,
            "home": snapshot.3,
        })),
    )
        .into_response()
}

async fn get_world_web_shell(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let web_session = authorize_league_web_session(&state, &headers, None)
        .ok()
        .flatten();
    let csrf_input = web_session
        .as_ref()
        .map(|session| {
            format!(
                "<input type=\"hidden\" name=\"csrf\" value=\"{}\" />",
                escape_html_text(&session.csrf)
            )
        })
        .unwrap_or_default();
    let console_note = if web_session.is_some() {
        "Authenticated web session: world actions are CSRF-protected and bound to the signed player."
    } else if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        "Local-dev World shell: create ventures, craft assets, recruit Agents, and mirror real opportunities without exposing tokens to the browser."
    } else {
        "Read-only World shell: request a signed /league/web/session before submitting world actions."
    };
    let league = state.inner.league_state.lock().await;
    let mut zones: Vec<WorldZone> = league.world_zones.values().cloned().collect();
    zones.sort_by(|left, right| left.zone_id.cmp(&right.zone_id));
    let mut locations: Vec<WorldLocation> = league.world_locations.values().cloned().collect();
    locations.sort_by(|left, right| left.location_id.cmp(&right.location_id));
    let mut entities: Vec<WorldEntity> = league.world_entities.values().cloned().collect();
    entities.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));

    let zone_cards = zones
        .iter()
        .map(|zone| {
            format!(
                "<article class=\"card zone\"><div class=\"pill\">{}</div><h3>{}</h3><p>{}</p><footer><code>{}</code><span>{}</span></footer></article>",
                escape_html_text(&zone.status),
                escape_html_text(&zone.name),
                escape_html_text(&zone.theme),
                escape_html_text(&zone.zone_id),
                escape_html_text(&zone.mirror_kind),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let location_cards = locations
        .iter()
        .map(|location| {
            format!(
                "<article class=\"mini\"><strong>{}</strong><span>{}</span><code>{}</code><small>{}</small></article>",
                escape_html_text(&location.name),
                escape_html_text(&location.description),
                escape_html_text(&location.location_id),
                escape_html_text(&location.location_kind),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let location_options = locations
        .iter()
        .map(|location| {
            format!(
                "<option value=\"{}\">{} · {}</option>",
                escape_html_text(&location.location_id),
                escape_html_text(&location.name),
                escape_html_text(&location.location_kind),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let entity_cards = entities
        .iter()
        .map(|entity| {
            format!(
                "<article class=\"mini\"><strong>{}</strong><span>{}</span><code>{}</code><small>{}</small></article>",
                escape_html_text(&entity.name),
                escape_html_text(&entity.role),
                escape_html_text(&entity.entity_id),
                escape_html_text(&entity.status),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let entity_cards = if entity_cards.is_empty() {
        "<article class=\"mini\"><strong>No Agent residents yet</strong><span>Use /world action 招募 Agent</span><code>agent</code></article>".to_string()
    } else {
        entity_cards
    };
    let latest_asset_id = league
        .world_assets
        .iter()
        .rev()
        .find(|asset| asset.owner_matrix_user_id == "@alice:local.dev")
        .map(|asset| asset.asset_id.clone())
        .unwrap_or_else(|| "latest".to_string());
    let asset_cards = league
        .world_assets
        .iter()
        .rev()
        .take(8)
        .map(|asset| {
            format!(
                "<article class=\"mini asset\"><strong>{}</strong><span>{} · Lv {} · value {}</span><code>{}</code><small>{}</small></article>",
                escape_html_text(&asset.name),
                escape_html_text(&asset.asset_kind),
                asset.upgrade_level.max(1),
                asset.value_score,
                escape_html_text(&asset.asset_id),
                escape_html_text(&asset.status),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let asset_cards = if asset_cards.is_empty() {
        "<article class=\"mini asset\"><strong>No player assets yet</strong><span>Create a venture or craft build to mint the first asset.</span><code>/world action</code></article>".to_string()
    } else {
        asset_cards
    };
    let company_cards = league
        .world_companies
        .iter()
        .rev()
        .take(8)
        .map(|company| {
            format!(
                "<article class=\"mini company\"><strong>{}</strong><span>{} · Lv {} · revenue {}</span><code>{}</code><small>asset {} · rep {}</small></article>",
                escape_html_text(&company.name),
                escape_html_text(&company.company_kind),
                company.level,
                company.revenue_score,
                escape_html_text(&company.company_id),
                escape_html_text(&company.asset_id),
                company.reputation_score,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let company_cards = if company_cards.is_empty() {
        "<article class=\"mini company\"><strong>No companies yet</strong><span>Use /company latest to turn an asset into an operating company.</span><code>/company latest</code></article>".to_string()
    } else {
        company_cards
    };
    let latest_company_id = league
        .world_companies
        .iter()
        .rev()
        .find(|company| company.owner_matrix_user_id == "@alice:local.dev")
        .map(|company| company.company_id.clone())
        .unwrap_or_else(|| "latest".to_string());
    let shop_cards = league
        .world_shops
        .iter()
        .rev()
        .take(8)
        .map(|shop| {
            format!(
                "<article class=\"mini shop\"><strong>{}</strong><span>{} · listings {} · GMV {}</span><code>{}</code><small>company {} · {}</small></article>",
                escape_html_text(&shop.name),
                escape_html_text(&shop.shop_kind),
                shop.listing_count,
                shop.gross_merchandise_score,
                escape_html_text(&shop.shop_id),
                escape_html_text(&shop.company_id),
                escape_html_text(&shop.status),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let shop_cards = if shop_cards.is_empty() {
        "<article class=\"mini shop\"><strong>No shops yet</strong><span>Launch a company to open the first storefront.</span><code>/company latest</code></article>".to_string()
    } else {
        shop_cards
    };
    let listing_cards = league
        .world_listings
        .iter()
        .rev()
        .take(8)
        .map(|listing| {
            format!(
                "<article class=\"mini listing\"><strong>{}</strong><span>{} · {} credits · quality {}</span><code>{}</code><small>shop {} · {}</small></article>",
                escape_html_text(&listing.title),
                escape_html_text(&listing.listing_kind),
                listing.price_credits,
                listing.quality_score,
                escape_html_text(&listing.listing_id),
                escape_html_text(&listing.shop_id),
                escape_html_text(&listing.status),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let listing_cards = if listing_cards.is_empty() {
        "<article class=\"mini listing\"><strong>No listings yet</strong><span>Use /sell latest to publish an offer.</span><code>/sell latest</code></article>".to_string()
    } else {
        listing_cards
    };
    let latest_contract_id = league
        .world_contracts
        .iter()
        .rev()
        .find(|contract| contract.actor_matrix_user_id == "@alice:local.dev")
        .map(|contract| contract.contract_id.clone())
        .unwrap_or_default();
    let contract_cards = league
        .world_contracts
        .iter()
        .rev()
        .take(8)
        .map(|contract| {
            format!(
                "<article class=\"mini contract\"><strong>{}</strong><span>{} · value {}</span><code>{}</code><small>task {} · {}</small></article>",
                escape_html_text(&contract.title),
                escape_html_text(&contract.status),
                contract.value_score,
                escape_html_text(&contract.location_id),
                escape_html_text(&contract.task_id),
                escape_html_text(contract.cex_status.as_deref().unwrap_or("created")),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let contract_cards = if contract_cards.is_empty() {
        "<article class=\"mini contract\"><strong>No World contracts yet</strong><span>Use /contract to mirror a real need into CEX execution.</span><code>/contract</code></article>".to_string()
    } else {
        contract_cards
    };
    let event_items = league
        .world_events
        .iter()
        .rev()
        .take(12)
        .map(|event| {
            format!(
                "<li><b>🌍 {}</b><span>{}</span><small>{} · +{} · {}</small><em>{}</em></li>",
                escape_html_text(&event.event_kind),
                escape_html_text(&event.body),
                escape_html_text(&event.location_id),
                event.impact_score,
                escape_html_text(event.cex_task_id.as_deref().unwrap_or("no-task")),
                escape_html_text(&event.result),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let event_items = if event_items.is_empty() {
        "<li><b>🌍 World is waiting</b><span>Use the action console to create the first world event.</span><small>/world action</small><em>Reality mirror booting.</em></li>".to_string()
    } else {
        event_items
    };

    Html(format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Trillionnium World</title>
  <style>
    :root {{ color-scheme: dark; --bg:#060711; --panel:#111426; --panel2:#171b31; --gold:#f8c35b; --cyan:#64e3ff; --green:#7dff9b; --text:#f6f7fb; --muted:#9aa3b2; }}
    * {{ box-sizing:border-box; }}
    body {{ margin:0; min-height:100vh; font-family:Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background:radial-gradient(circle at 22% 0%, #133f38 0, transparent 32rem), radial-gradient(circle at 90% 18%, #3b245c 0, transparent 30rem), var(--bg); color:var(--text); }}
    header {{ padding:42px min(6vw,72px) 18px; display:grid; gap:22px; grid-template-columns:1.3fr .7fr; align-items:end; }}
    h1 {{ margin:0; font-size:clamp(44px,7vw,96px); line-height:.88; letter-spacing:-.075em; }}
    h2 {{ margin:0 0 16px; letter-spacing:-.03em; }}
    .subtitle {{ color:var(--muted); font-size:18px; max-width:840px; line-height:1.55; }}
    .hero-card,.card,.panel {{ border:1px solid rgba(255,255,255,.11); background:linear-gradient(145deg,rgba(255,255,255,.09),rgba(255,255,255,.035)); box-shadow:0 24px 80px rgba(0,0,0,.35); backdrop-filter: blur(14px); border-radius:24px; }}
    .hero-card,.panel,.card {{ padding:24px; }}
    .stats {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(120px,1fr)); gap:14px; margin-top:22px; }}
    .stat {{ padding:18px; background:rgba(255,255,255,.06); border-radius:18px; }}
    .stat b {{ display:block; font-size:26px; color:var(--gold); }}
    main {{ padding:20px min(6vw,72px) 60px; display:grid; gap:24px; }}
    .grid {{ display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:18px; }}
    .mini-grid {{ display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:12px; }}
    .card h3 {{ margin:12px 0; font-size:24px; }}
    .card p {{ color:var(--muted); line-height:1.55; }}
    .card footer {{ display:grid; gap:8px; margin-top:18px; color:var(--gold); }}
    .pill {{ display:inline-flex; border:1px solid rgba(100,227,255,.35); color:var(--cyan); padding:5px 10px; border-radius:999px; font-size:12px; text-transform:uppercase; letter-spacing:.12em; }}
    .play {{ display:grid; grid-template-columns:.8fr 1.2fr; gap:18px; }}
    form {{ display:grid; gap:10px; margin:0; }}
    input,textarea,select {{ width:100%; color:var(--text); background:rgba(255,255,255,.07); border:1px solid rgba(255,255,255,.14); border-radius:14px; padding:12px 14px; font:inherit; }}
    textarea {{ min-height:140px; resize:vertical; }}
    button {{ border:0; cursor:pointer; color:var(--bg); background:linear-gradient(135deg,var(--gold),#7dff9b); padding:12px 16px; border-radius:14px; font-weight:800; }}
    .mini {{ display:grid; gap:7px; padding:14px; border-radius:16px; background:rgba(255,255,255,.06); border:1px solid rgba(255,255,255,.08); }}
    .asset strong {{ color:var(--green); }}
    .mini span,.mini small,.timeline small,.timeline em {{ color:var(--muted); }}
    .timeline {{ list-style:none; padding:0; margin:0; display:grid; gap:10px; }}
    .timeline li {{ display:grid; grid-template-columns:.55fr 1.35fr .55fr; gap:10px; padding:12px; border-radius:14px; background:rgba(255,255,255,.055); }}
    .timeline em {{ grid-column:1 / -1; font-style:normal; }}
    code {{ color:var(--cyan); background:rgba(100,227,255,.08); padding:3px 7px; border-radius:8px; }}
    .cta {{ color:var(--bg); background:linear-gradient(135deg,var(--gold),#7dff9b); padding:14px 18px; border-radius:16px; display:inline-block; font-weight:800; text-decoration:none; }}
    @media (max-width:1050px) {{ header,.play {{ grid-template-columns:1fr; }} .grid,.stats,.mini-grid {{ grid-template-columns:1fr; }} }}
  </style>
</head>
<body>
  <header>
    <section>
      <div class="pill">Reality Mirror Sandbox</div>
      <h1>Trillionnium World</h1>
      <p class="subtitle">开放世界总层：现实镜像城市、Craft 工坊、Market、League 竞技场、Agent 居民、资产、关系与自由行动。League 是竞技模块，Craft 是建造模块，Ledger 是结算层。</p>
    </section>
    <aside class="hero-card">
      <strong>World Shell Online</strong>
      <p class="subtitle">Build a company, craft an asset, recruit Agents, enter markets, or jump into League competition.</p>
      <a class="cta" href="/league">Enter League Arena</a>
    </aside>
  </header>
  <main>
    <section class="stats">
      <div class="stat"><span>Zones</span><b>{zones}</b></div>
      <div class="stat"><span>Locations</span><b>{locations}</b></div>
      <div class="stat"><span>Agents</span><b>{entities}</b></div>
      <div class="stat"><span>Assets</span><b>{assets}</b></div>
      <div class="stat"><span>Upgrades</span><b>{asset_upgrades}</b></div>
      <div class="stat"><span>Companies</span><b>{companies}</b></div>
      <div class="stat"><span>Shops</span><b>{shops}</b></div>
      <div class="stat"><span>Listings</span><b>{listings}</b></div>
      <div class="stat"><span>Contracts</span><b>{contracts}</b></div>
      <div class="stat"><span>Done</span><b>{completions}</b></div>
      <div class="stat"><span>Events</span><b>{events}</b></div>
      <div class="stat"><span>Relations</span><b>{relationships}</b></div>
    </section>
    <section>
      <h2>World Zones</h2>
      <div class="grid">{zone_cards}</div>
    </section>
    <section class="play">
      <div class="panel">
        <h2>World Action Console</h2>
        <p class="subtitle">{console_note}</p>
        <form method="post" action="/world/web/action">
          {csrf_input}
          <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
          <select name="location_id">{location_options}</select>
          <textarea name="body">我要在镜像城市开一家 AI 设计公司，招募 Agent，服务真实客户，并把客户需求转成 League 任务。</textarea>
          <button type="submit">Commit World Action</button>
        </form>
      </div>
      <div class="panel">
        <h2>World Event Timeline</h2>
        <ul class="timeline">{event_items}</ul>
      </div>
    </section>
    <section class="panel">
      <h2>Locations</h2>
      <div class="mini-grid">{location_cards}</div>
    </section>
    <section class="panel">
      <h2>Agent Residents / NPCs</h2>
      <div class="mini-grid">{entity_cards}</div>
    </section>
    <section class="panel">
      <h2>Player Assets</h2>
      <div class="mini-grid">{asset_cards}</div>
      <form method="post" action="/world/web/asset" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input name="asset_id" value="{latest_asset_id}" placeholder="latest or world-asset-id" />
        <textarea name="body">Upgrade this World asset with a stronger offer, proof, risk control, operating loop, and next customer path.</textarea>
        <button type="submit">Upgrade Asset</button>
      </form>
    </section>
    <section class="panel">
      <h2>Companies / Shops</h2>
      <div class="mini-grid">{company_cards}</div>
      <form method="post" action="/world/web/company" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input name="asset_id" value="{latest_asset_id}" placeholder="latest or world-asset-id" />
        <textarea name="body">Launch a shop/company from this asset with offer, customer segment, operating loop, proof, and first revenue path.</textarea>
        <button type="submit">Launch Company</button>
      </form>
    </section>
    <section class="panel">
      <h2>Shops / Listings</h2>
      <div class="mini-grid">{shop_cards}</div>
      <div class="mini-grid" style="margin-top:12px">{listing_cards}</div>
      <form method="post" action="/world/web/listing" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input name="company_id" value="{latest_company_id}" placeholder="latest or world-company-id" />
        <textarea name="body">Publish a service listing with clear deliverable, price logic, evidence package, customer promise, risk controls, self-review, and next action.</textarea>
        <button type="submit">Publish Listing</button>
      </form>
    </section>
    <section class="panel">
      <h2>World Contracts</h2>
      <div class="mini-grid">{contract_cards}</div>
      <form method="post" action="/world/web/contract" style="margin-top:16px">
        {csrf_input}
        <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
        <input name="contract_id" value="{latest_contract_id}" placeholder="world-contract-id" />
        <textarea name="body">World contract delivery: deliverable, evidence, risk review, next step, acceptance standard.</textarea>
        <button type="submit">Complete Contract</button>
      </form>
    </section>
    <section class="panel">
      <h2>Playable Commands</h2>
      <p class="subtitle"><code>/world</code> <code>/world action 我要开一家 AI 设计公司</code> <code>/league</code> <code>/arena</code> <code>/guild</code> <code>/raid</code></p>
    </section>
  </main>
</body>
</html>"#,
        zones = league.world_zones.len(),
        locations = league.world_locations.len(),
        entities = league.world_entities.len(),
        assets = league.world_assets.len(),
        asset_upgrades = league.world_asset_upgrades.len(),
        companies = league.world_companies.len(),
        shops = league.world_shops.len(),
        listings = league.world_listings.len(),
        contracts = league.world_contracts.len(),
        completions = league.world_contract_completions.len(),
        events = league.world_events.len(),
        relationships = league.world_relationships.len(),
        zone_cards = zone_cards,
        location_cards = location_cards,
        location_options = location_options,
        entity_cards = entity_cards,
        asset_cards = asset_cards,
        latest_asset_id = escape_html_text(&latest_asset_id),
        company_cards = company_cards,
        latest_company_id = escape_html_text(&latest_company_id),
        shop_cards = shop_cards,
        listing_cards = listing_cards,
        contract_cards = contract_cards,
        latest_contract_id = escape_html_text(&latest_contract_id),
        event_items = event_items,
        console_note = escape_html_text(console_note),
        csrf_input = csrf_input,
    ))
}

async fn post_world_web_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebActionRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("我要在镜像城市开一家 AI 设计公司，招募 Agent，服务真实客户。")
        .to_string();
    let request = WorldActionRequest {
        matrix_user_id,
        room_id: web_session
            .as_ref()
            .and_then(|session| session.room_id.clone())
            .or_else(|| Some("!web-local:local.dev".to_string())),
        location_id: payload
            .location_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        body,
        message: None,
        event_id: None,
        capability_id: None,
        account_id: None,
        cex_task_id: None,
        cex_status: None,
    };
    let snapshot = match record_world_action(&state, request).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(response) = persist_league_state(&state, &snapshot.0).await {
        return response;
    }
    Redirect::to("/world?played=1").into_response()
}

async fn get_world_assets(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_assets",
            "world": "trillionnium_world",
            "assets": league.world_assets,
            "upgrades": league.world_asset_upgrades,
        })),
    )
        .into_response()
}

async fn upgrade_world_asset_inner(
    state: AppState,
    asset_id: String,
    payload: WorldAssetUpgradeRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let _room_id = payload.room_id.clone();
    let resolved_asset_id = {
        let league = state.inner.league_state.lock().await;
        if asset_id == "latest" {
            match league
                .world_assets
                .iter()
                .rev()
                .find(|asset| asset.owner_matrix_user_id == matrix_user_id)
                .map(|asset| asset.asset_id.clone())
            {
                Some(value) => value,
                None => {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(json!({ "error": "no world asset found for player" })),
                    )
                        .into_response()
                }
            }
        } else {
            asset_id.clone()
        }
    };
    let judgement =
        judge_league_submission_with_pipeline(&state, &body, "world_asset_upgrade").await;
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let Some(asset_index) = league
            .world_assets
            .iter()
            .position(|asset| asset.asset_id == resolved_asset_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world asset not found", "asset_id": resolved_asset_id })),
            )
                .into_response();
        };
        if league.world_assets[asset_index].owner_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "world asset belongs to another player", "asset_id": resolved_asset_id })),
            )
                .into_response();
        }
        let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
        let level_before = league.world_assets[asset_index].upgrade_level.max(1);
        let points_before = league.world_assets[asset_index].upgrade_points.max(0);
        let value_delta = if judgement.payout_status == "eligible" {
            (judgement.score / 4.0).round().max(1.0) as i64
        } else {
            0
        };
        let points_after = points_before + value_delta;
        let level_after = level_before.max(1) + (points_after / 80) - (points_before / 80);
        let upgrade_status = if judgement.payout_status == "eligible" {
            "applied".to_string()
        } else {
            "review_hold".to_string()
        };
        if value_delta > 0 {
            let asset = &mut league.world_assets[asset_index];
            asset.value_score += value_delta;
            asset.upgrade_points = points_after;
            asset.upgrade_level = level_after.max(level_before);
            asset.last_upgrade_kind = Some("manual_upgrade".to_string());
            asset.status = "upgraded".to_string();
        }
        let upgrade = WorldAssetUpgrade {
            upgrade_id: league_hash_id(
                "world-asset-upgrade",
                &format!("{}:{}:{}", resolved_asset_id, now, body),
            ),
            asset_id: resolved_asset_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            body: body.clone(),
            upgrade_kind: "manual_upgrade".to_string(),
            score: judgement.score,
            grade: judgement.grade.clone(),
            judge_status: judgement.judge_status.clone(),
            status: upgrade_status,
            value_delta,
            level_before,
            level_after: level_after.max(level_before),
            created_at_epoch: now,
        };
        player.xp += judgement.score.round() as i64;
        player.reputation += (judgement.score / 10.0).round() as i64;
        player.rating += ((judgement.score - 50.0) / 4.0).round() as i64;
        league
            .players_by_matrix_user
            .insert(matrix_user_id.clone(), player);
        league.world_asset_upgrades.push(upgrade.clone());
        (
            league.clone(),
            league.world_assets[asset_index].clone(),
            upgrade,
        )
    };
    if let Err(response) = persist_league_state(&state, &snapshot.0).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_asset_upgrade",
            "world": "trillionnium_world",
            "asset": snapshot.1,
            "upgrade": snapshot.2,
        })),
    )
        .into_response()
}

async fn upgrade_world_asset(
    Path(asset_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldAssetUpgradeRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    upgrade_world_asset_inner(state, asset_id, payload).await
}

async fn post_world_web_asset_upgrade(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebAssetUpgradeRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let asset_id = payload
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Upgrade this World asset with a stronger offer, proof, risk control, and operating loop.")
        .to_string();
    let request = WorldAssetUpgradeRequest {
        matrix_user_id,
        room_id: web_session
            .as_ref()
            .and_then(|session| session.room_id.clone())
            .or_else(|| Some("!web-local:local.dev".to_string())),
        body,
    };
    let response = upgrade_world_asset_inner(state, asset_id, request).await;
    if response.status().is_success() {
        Redirect::to("/world?asset=upgraded").into_response()
    } else {
        response
    }
}

async fn get_world_companies(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_companies",
            "world": "trillionnium_world",
            "companies": league.world_companies,
        })),
    )
        .into_response()
}

async fn create_world_company_inner(state: AppState, payload: WorldCompanyRequest) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let requested_asset_id = payload
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let judgement = judge_league_submission_with_pipeline(&state, &body, "world_company").await;
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let Some(asset) = (if requested_asset_id == "latest" {
            league
                .world_assets
                .iter()
                .rev()
                .find(|asset| asset.owner_matrix_user_id == matrix_user_id)
                .cloned()
        } else {
            league
                .world_assets
                .iter()
                .find(|asset| asset.asset_id == requested_asset_id)
                .cloned()
        }) else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world asset not found", "asset_id": requested_asset_id })),
            )
                .into_response();
        };
        if asset.owner_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "world asset belongs to another player", "asset_id": asset.asset_id })),
            )
                .into_response();
        }
        let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
        let revenue_score = ((asset.value_score as f64) * 0.6 + judgement.score).round() as i64;
        let reputation_score =
            ((asset.upgrade_level.max(1) * 10) as f64 + judgement.score / 2.0).round() as i64;
        let level = 1 + (revenue_score / 100).max(0);
        let company_kind = if body.contains("店") || body.to_ascii_lowercase().contains("shop") {
            "shop"
        } else if body.contains("工坊") || body.to_ascii_lowercase().contains("studio") {
            "studio"
        } else {
            "company"
        };
        let company = WorldCompany {
            company_id: league_hash_id(
                "world-company",
                &format!("{}:{}:{}", matrix_user_id, asset.asset_id, now),
            ),
            owner_matrix_user_id: matrix_user_id.clone(),
            asset_id: asset.asset_id.clone(),
            location_id: asset.location_id.clone(),
            name: if company_kind == "shop" {
                "Mirror Market Shop".to_string()
            } else if company_kind == "studio" {
                "Trillionnium Craft Studio".to_string()
            } else {
                "Reality Venture Company".to_string()
            },
            company_kind: company_kind.to_string(),
            status: if judgement.payout_status == "eligible" {
                "operating".to_string()
            } else {
                "review_hold".to_string()
            },
            revenue_score,
            reputation_score,
            level,
            created_at_epoch: now,
        };
        let shop = WorldShop {
            shop_id: league_hash_id(
                "world-shop",
                &format!("{}:{}:{}", matrix_user_id, company.company_id, now),
            ),
            company_id: company.company_id.clone(),
            owner_matrix_user_id: matrix_user_id.clone(),
            location_id: company.location_id.clone(),
            name: format!("{} Storefront", company.name),
            shop_kind: company_kind.to_string(),
            status: company.status.clone(),
            listing_count: 1,
            gross_merchandise_score: revenue_score.max(0),
            created_at_epoch: now,
        };
        let listing = WorldListing {
            listing_id: league_hash_id(
                "world-listing",
                &format!("{}:{}:{}", matrix_user_id, shop.shop_id, now),
            ),
            shop_id: shop.shop_id.clone(),
            company_id: company.company_id.clone(),
            owner_matrix_user_id: matrix_user_id.clone(),
            asset_id: asset.asset_id.clone(),
            title: body.chars().take(42).collect::<String>(),
            listing_kind: "service_offer".to_string(),
            status: company.status.clone(),
            price_credits: (revenue_score / 2).max(10),
            quality_score: judgement.score.round() as i64,
            created_at_epoch: now,
        };
        let economy_event = WorldEconomyEvent {
            economy_event_id: league_hash_id(
                "world-econ",
                &format!("{}:{}:{}", matrix_user_id, listing.listing_id, now),
            ),
            matrix_user_id: matrix_user_id.clone(),
            event_kind: "company_launch".to_string(),
            subject_id: company.company_id.clone(),
            credits_delta: listing.price_credits,
            reputation_delta: reputation_score,
            created_at_epoch: now,
        };
        if judgement.payout_status == "eligible" {
            player.xp += judgement.score.round() as i64;
            player.reputation += (judgement.score / 6.0).round() as i64;
            player.rating += ((judgement.score - 50.0) / 4.0).round() as i64;
        }
        league
            .players_by_matrix_user
            .insert(matrix_user_id.clone(), player);
        league.world_relationships.push(WorldRelationship {
            relationship_id: league_hash_id(
                "world-rel",
                &format!("{}:{}:{}", matrix_user_id, company.company_id, now),
            ),
            from_id: matrix_user_id.clone(),
            to_id: company.company_id.clone(),
            relation_kind: "owner".to_string(),
            strength: reputation_score,
            updated_at_epoch: now,
        });
        league.world_companies.push(company.clone());
        league.world_shops.push(shop.clone());
        league.world_listings.push(listing.clone());
        league.world_economy_events.push(economy_event.clone());
        (
            league.clone(),
            company,
            shop,
            listing,
            economy_event,
            judgement,
        )
    };
    if let Err(response) = persist_league_state(&state, &snapshot.0).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_company_created",
            "world": "trillionnium_world",
            "company": snapshot.1,
            "shop": snapshot.2,
            "listing": snapshot.3,
            "economy_event": snapshot.4,
            "judge_status": snapshot.5.judge_status,
            "payout_status": snapshot.5.payout_status,
        })),
    )
        .into_response()
}

async fn create_world_company(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldCompanyRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    create_world_company_inner(state, payload).await
}

async fn post_world_web_company(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebCompanyRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Launch a Trillionnium World company from this asset with offer, market, operating loop, proof, and next revenue path.")
        .to_string();
    let request = WorldCompanyRequest {
        matrix_user_id,
        asset_id: payload
            .asset_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        body,
    };
    let response = create_world_company_inner(state, request).await;
    if response.status().is_success() {
        Redirect::to("/world?company=created").into_response()
    } else {
        response
    }
}

async fn get_world_shops(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_shops",
            "world": "trillionnium_world",
            "companies": league.world_companies,
            "shops": league.world_shops,
            "listings": league.world_listings,
            "economy_events": league.world_economy_events,
        })),
    )
        .into_response()
}

async fn create_world_listing_inner(state: AppState, payload: WorldListingRequest) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let requested_company_id = payload
        .company_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let judgement = judge_league_submission_with_pipeline(&state, &body, "world_listing").await;
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let Some(company_index) = (if requested_company_id == "latest" {
            league
                .world_companies
                .iter()
                .rposition(|company| company.owner_matrix_user_id == matrix_user_id)
        } else {
            league
                .world_companies
                .iter()
                .position(|company| company.company_id == requested_company_id)
        }) else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world company not found", "company_id": requested_company_id })),
            )
                .into_response();
        };
        let company_seed = league.world_companies[company_index].clone();
        if company_seed.owner_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "world company belongs to another player", "company_id": company_seed.company_id })),
            )
                .into_response();
        }
        let shop_index = match league
            .world_shops
            .iter()
            .position(|shop| shop.company_id == company_seed.company_id)
        {
            Some(index) => index,
            None => {
                league.world_shops.push(WorldShop {
                    shop_id: league_hash_id(
                        "world-shop",
                        &format!("{}:{}:{}", matrix_user_id, company_seed.company_id, now),
                    ),
                    company_id: company_seed.company_id.clone(),
                    owner_matrix_user_id: matrix_user_id.clone(),
                    location_id: company_seed.location_id.clone(),
                    name: format!("{} Storefront", company_seed.name),
                    shop_kind: company_seed.company_kind.clone(),
                    status: company_seed.status.clone(),
                    listing_count: 0,
                    gross_merchandise_score: 0,
                    created_at_epoch: now,
                });
                league.world_shops.len() - 1
            }
        };
        let quality_score = judgement.score.round() as i64;
        let price_credits =
            (((company_seed.revenue_score.max(10) as f64) * 0.35) + judgement.score).round() as i64;
        let listing = WorldListing {
            listing_id: league_hash_id(
                "world-listing",
                &format!(
                    "{}:{}:{}",
                    matrix_user_id, league.world_shops[shop_index].shop_id, now
                ),
            ),
            shop_id: league.world_shops[shop_index].shop_id.clone(),
            company_id: company_seed.company_id.clone(),
            owner_matrix_user_id: matrix_user_id.clone(),
            asset_id: company_seed.asset_id.clone(),
            title: body.chars().take(48).collect::<String>(),
            listing_kind: if body.contains("订阅")
                || body.to_ascii_lowercase().contains("subscription")
            {
                "subscription_offer".to_string()
            } else {
                "service_offer".to_string()
            },
            status: if judgement.payout_status == "eligible" {
                "listed".to_string()
            } else {
                "review_hold".to_string()
            },
            price_credits: price_credits.max(10),
            quality_score,
            created_at_epoch: now,
        };
        let economy_event = WorldEconomyEvent {
            economy_event_id: league_hash_id(
                "world-econ",
                &format!("{}:{}:{}", matrix_user_id, listing.listing_id, now),
            ),
            matrix_user_id: matrix_user_id.clone(),
            event_kind: "listing_published".to_string(),
            subject_id: listing.listing_id.clone(),
            credits_delta: if judgement.payout_status == "eligible" {
                listing.price_credits
            } else {
                0
            },
            reputation_delta: if judgement.payout_status == "eligible" {
                (judgement.score / 5.0).round() as i64
            } else {
                0
            },
            created_at_epoch: now,
        };
        if judgement.payout_status == "eligible" {
            league.world_shops[shop_index].listing_count += 1;
            league.world_shops[shop_index].gross_merchandise_score += listing.price_credits;
            league.world_companies[company_index].revenue_score += listing.price_credits;
            league.world_companies[company_index].reputation_score +=
                economy_event.reputation_delta;
            league.world_companies[company_index].level =
                1 + (league.world_companies[company_index].revenue_score / 100).max(0);
            let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
            player.xp += quality_score;
            player.reputation += economy_event.reputation_delta;
            player.rating += ((judgement.score - 50.0) / 5.0).round() as i64;
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), player);
        }
        league.world_listings.push(listing.clone());
        league.world_economy_events.push(economy_event.clone());
        let company = league.world_companies[company_index].clone();
        let shop = league.world_shops[shop_index].clone();
        (
            league.clone(),
            company,
            shop,
            listing,
            economy_event,
            judgement,
        )
    };
    if let Err(response) = persist_league_state(&state, &snapshot.0).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_listing_created",
            "world": "trillionnium_world",
            "company": snapshot.1,
            "shop": snapshot.2,
            "listing": snapshot.3,
            "economy_event": snapshot.4,
            "judge_status": snapshot.5.judge_status,
            "payout_status": snapshot.5.payout_status,
        })),
    )
        .into_response()
}

async fn create_world_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldListingRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    create_world_listing_inner(state, payload).await
}

async fn post_world_web_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebListingRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Publish a Trillionnium World service listing with deliverable, price logic, evidence package, customer promise, risk controls, self-review, and next action.")
        .to_string();
    let request = WorldListingRequest {
        matrix_user_id,
        company_id: payload
            .company_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        body,
    };
    let response = create_world_listing_inner(state, request).await;
    if response.status().is_success() {
        Redirect::to("/world?listing=created").into_response()
    } else {
        response
    }
}

async fn get_world_contracts(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_contracts",
            "world": "trillionnium_world",
            "contracts": league.world_contracts,
            "completions": league.world_contract_completions,
        })),
    )
        .into_response()
}

async fn settle_world_contract_completion_with_ledger(
    state: &AppState,
    payload: &WorldContractCompleteRequest,
    matrix_user_id: &str,
    contract: &WorldContract,
    completion: &WorldContractCompletion,
) -> LeagueLedgerSettlement {
    if completion.reward_amount <= 0.0 {
        return LeagueLedgerSettlement {
            status: "skipped_zero_reward".to_string(),
            ..Default::default()
        };
    }
    if completion.payout_status != "eligible" || !completion.anti_cheat_flags.is_empty() {
        return LeagueLedgerSettlement {
            status: "held_review".to_string(),
            error: Some(format!(
                "world contract payout held: status={} flags={}",
                completion.payout_status,
                completion.anti_cheat_flags.join(",")
            )),
            ..Default::default()
        };
    }
    let Some(room_id) = payload
        .room_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_room".to_string(),
            ..Default::default()
        };
    };
    let matrix_payload = MatrixMessageRequest {
        matrix_user_id: matrix_user_id.to_string(),
        room_id: room_id.to_string(),
        session_id: None,
        org_id: None,
        message: "world contract reward settlement".to_string(),
        capability_id: None,
        account_id: None,
        event_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let resolved_identity = match resolve_matrix_identity(state, &matrix_payload).await {
        Ok(identity) => identity,
        Err(_) => {
            return LeagueLedgerSettlement {
                status: "failed_identity".to_string(),
                error: Some(
                    "matrix identity could not be resolved for world contract settlement"
                        .to_string(),
                ),
                ..Default::default()
            }
        }
    };
    let Some(account_id) = resolved_identity.scope.account_id.clone() else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_account".to_string(),
            error: Some("matrix identity did not resolve a ledger account_id".to_string()),
            ..Default::default()
        };
    };
    let Some(ledger_admin_token) = state.config().ledger_admin_token.clone() else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_ledger_token".to_string(),
            account_id: Some(account_id),
            error: Some("consumer-entry ledger admin token is not configured".to_string()),
            ..Default::default()
        };
    };
    let url = format!(
        "{}/v1/ledger/grant",
        state.config().ledger_base_url.trim_end_matches('/')
    );
    let body = json!({
        "account_id": account_id,
        "amount": completion.reward_amount,
        "idempotency_key": format!("world_contract_completion:{}", completion.completion_id),
        "reference_id": contract.task_id,
    });
    let response = match state
        .inner
        .http
        .post(url)
        .header("x-admin-token", ledger_admin_token)
        .json(&body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return LeagueLedgerSettlement {
                status: "failed_network".to_string(),
                account_id: body
                    .get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                error: Some(format!("failed to reach ledger-service: {err}")),
                ..Default::default()
            }
        }
    };
    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return LeagueLedgerSettlement {
                status: "failed_bad_response".to_string(),
                account_id: body
                    .get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                error: Some(format!("ledger-service returned non-json response: {err}")),
                ..Default::default()
            }
        }
    };
    if !status.is_success() {
        let error = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("ledger grant failed");
        return LeagueLedgerSettlement {
            status: if status.as_u16() == 409 {
                "duplicate".to_string()
            } else {
                "failed_ledger".to_string()
            },
            account_id: body
                .get("account_id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            error: Some(format!("{}: {error}", status.as_u16())),
            ..Default::default()
        };
    }
    LeagueLedgerSettlement {
        status: "settled".to_string(),
        account_id: value
            .get("account")
            .and_then(|account| account.get("account_id"))
            .and_then(Value::as_str)
            .or_else(|| body.get("account_id").and_then(Value::as_str))
            .map(ToString::to_string),
        entry_id: value
            .get("entry")
            .and_then(|entry| entry.get("entry_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        balance_after: value
            .get("account")
            .and_then(|account| account.get("balance"))
            .and_then(Value::as_f64),
        error: None,
    }
}

async fn complete_world_contract_inner(
    state: AppState,
    contract_id: String,
    payload: WorldContractCompleteRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let contract = {
        let league = state.inner.league_state.lock().await;
        let Some(contract) = league
            .world_contracts
            .iter()
            .find(|contract| contract.contract_id == contract_id)
            .cloned()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world contract not found", "contract_id": contract_id })),
            )
                .into_response();
        };
        contract
    };
    if contract.actor_matrix_user_id != matrix_user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "world contract can only be completed by its creator",
                "contract_id": contract.contract_id,
            })),
        )
            .into_response();
    }
    let judgement = judge_league_submission_with_pipeline(&state, &body, "world_contract").await;
    let mut completion = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
        let completion_id = league_hash_id(
            "world-contract-completion",
            &format!("{}:{}:{}", contract.contract_id, now, body),
        );
        let completion = WorldContractCompletion {
            completion_id: completion_id.clone(),
            contract_id: contract.contract_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            body: body.clone(),
            score: judgement.score,
            grade: judgement.grade.clone(),
            reward_amount: judgement.reward_amount,
            judge_status: judgement.judge_status.clone(),
            payout_status: judgement.payout_status.clone(),
            anti_cheat_flags: judgement.anti_cheat_flags.clone(),
            score_events: judgement.score_events.clone(),
            ledger_status: Some("pending".to_string()),
            ledger_account_id: None,
            ledger_entry_id: None,
            ledger_balance_after: None,
            ledger_error: None,
            created_at_epoch: now,
        };
        let asset_delta = (judgement.score / 5.0).round() as i64;
        if let Some(stored_contract) = league
            .world_contracts
            .iter_mut()
            .find(|stored| stored.contract_id == contract.contract_id)
        {
            stored_contract.status = if judgement.payout_status == "eligible" {
                "completed_pending_settlement".to_string()
            } else {
                "review_hold".to_string()
            };
            stored_contract.value_score += asset_delta.max(1);
            stored_contract.cex_status = Some("completed".to_string());
        }
        if let Some(asset) = league
            .world_assets
            .iter_mut()
            .rev()
            .find(|asset| asset.owner_matrix_user_id == matrix_user_id)
        {
            asset.value_score += asset_delta.max(1);
            asset.upgrade_points += asset_delta.max(1);
            asset.upgrade_level = asset.upgrade_level.max(1) + (asset.upgrade_points / 60).max(0);
            asset.last_upgrade_kind = Some("contract_completion".to_string());
            asset.status = "upgraded_by_contract".to_string();
        } else {
            league.world_assets.push(WorldAsset {
                asset_id: league_hash_id("world-asset", &completion_id),
                owner_matrix_user_id: matrix_user_id.clone(),
                location_id: contract.location_id.clone(),
                asset_kind: "contract_proof".to_string(),
                name: "World Contract Proof".to_string(),
                status: "active".to_string(),
                value_score: asset_delta.max(1),
                upgrade_level: 1,
                upgrade_points: asset_delta.max(1),
                last_upgrade_kind: Some("contract_completion".to_string()),
                created_at_epoch: now,
            });
        }
        player.xp += judgement.score.round() as i64;
        player.reputation += (judgement.score / 8.0).round() as i64;
        player.rating += ((judgement.score - 50.0) / 3.0).round() as i64;
        if judgement.payout_status == "eligible" {
            player.earned_credits += judgement.reward_amount;
        }
        league
            .players_by_matrix_user
            .insert(matrix_user_id.clone(), player);
        league.world_contract_completions.push(completion.clone());
        completion
    };
    let settlement = settle_world_contract_completion_with_ledger(
        &state,
        &payload,
        &matrix_user_id,
        &contract,
        &completion,
    )
    .await;
    completion.ledger_status = Some(settlement.status);
    completion.ledger_account_id = settlement.account_id;
    completion.ledger_entry_id = settlement.entry_id;
    completion.ledger_balance_after = settlement.balance_after;
    completion.ledger_error = settlement.error;
    let snapshot = {
        let mut league = state.inner.league_state.lock().await;
        if let Some(stored_completion) = league
            .world_contract_completions
            .iter_mut()
            .find(|stored| stored.completion_id == completion.completion_id)
        {
            *stored_completion = completion.clone();
        }
        if let Some(stored_contract) = league
            .world_contracts
            .iter_mut()
            .find(|stored| stored.contract_id == contract.contract_id)
        {
            stored_contract.status = match completion.ledger_status.as_deref() {
                Some("settled") | Some("duplicate") => "completed_settled".to_string(),
                Some("held_review") => "review_hold".to_string(),
                Some(status) => format!("completed_{status}"),
                None => stored_contract.status.clone(),
            };
        }
        league.clone()
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_contract_completion",
            "world": "trillionnium_world",
            "contract": snapshot
                .world_contracts
                .iter()
                .find(|stored| stored.contract_id == contract.contract_id),
            "completion": completion,
        })),
    )
        .into_response()
}

async fn complete_world_contract(
    Path(contract_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldContractCompleteRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    complete_world_contract_inner(state, contract_id, payload).await
}

async fn post_world_web_contract_complete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebContractCompleteRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let contract_id = match payload
        .contract_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
    {
        Some(value) => value,
        None => {
            let league = state.inner.league_state.lock().await;
            match league
                .world_contracts
                .iter()
                .rev()
                .find(|contract| contract.actor_matrix_user_id == matrix_user_id)
                .map(|contract| contract.contract_id.clone())
            {
                Some(value) => value,
                None => return Redirect::to("/world?contract=missing").into_response(),
            }
        }
    };
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("World contract delivery: deliverable, evidence, risk review, next step, acceptance standard.")
        .to_string();
    let request = WorldContractCompleteRequest {
        matrix_user_id,
        room_id: web_session
            .as_ref()
            .and_then(|session| session.room_id.clone())
            .or_else(|| Some("!web-local:local.dev".to_string())),
        body,
    };
    let response = complete_world_contract_inner(state, contract_id, request).await;
    if response.status().is_success() {
        Redirect::to("/world?contract=completed").into_response()
    } else {
        response
    }
}

async fn get_league_season(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let mut players: Vec<LeaguePlayer> = league.players_by_matrix_user.values().cloned().collect();
    players.sort_by(|left, right| {
        right
            .rating
            .cmp(&left.rating)
            .then_with(|| right.xp.cmp(&left.xp))
            .then_with(|| left.matrix_user_id.cmp(&right.matrix_user_id))
    });
    let top_players: Vec<Value> = players
        .iter()
        .take(5)
        .map(|player| {
            json!({
                "player_id": player.player_id,
                "matrix_user_id": player.matrix_user_id,
                "display_name": player.display_name,
                "rating": player.rating,
                "xp": player.xp,
                "earned_credits": player.earned_credits,
            })
        })
        .collect();
    let guild_standings = league_guild_standings(&league);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_season",
            "league": "trillionnium_league",
            "season": {
                "code": "preseason-zero",
                "name": "Preseason Zero",
                "status": "active",
                "theme": "Founding Summoners",
                "player_count": league.players_by_matrix_user.len(),
                "battle_count": league.battles.len(),
                "submission_count": league.submissions.len(),
                "reward_count": league.rewards.len()
            },
            "leaderboards": {
                "players": top_players,
                "guilds": guild_standings,
            }
        })),
    )
        .into_response()
}

async fn get_league_web_shell(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let web_session = authorize_league_web_session(&state, &headers, None)
        .ok()
        .flatten();
    let csrf_input = web_session
        .as_ref()
        .map(|session| {
            format!(
                "<input type=\"hidden\" name=\"csrf\" value=\"{}\" />",
                escape_html_text(&session.csrf)
            )
        })
        .unwrap_or_default();
    let console_note = if web_session.is_some() {
        "Authenticated web session: actions are CSRF-protected and bound to the signed player."
    } else if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        "Local-dev interactive shell: join, draft, submit, and settle rewards without exposing Matrix or ledger tokens to the browser."
    } else {
        "Read-only shell: request a signed /league/web/session before submitting web actions."
    };
    let league = state.inner.league_state.lock().await;
    let mut matches: Vec<LeagueMatch> = league.matches.values().cloned().collect();
    matches.sort_by(|left, right| left.match_id.cmp(&right.match_id));
    let mut players: Vec<LeaguePlayer> = league.players_by_matrix_user.values().cloned().collect();
    players.sort_by(|left, right| {
        right
            .rating
            .cmp(&left.rating)
            .then_with(|| right.xp.cmp(&left.xp))
            .then_with(|| left.matrix_user_id.cmp(&right.matrix_user_id))
    });
    let total_rewards: f64 = league.rewards.iter().map(|reward| reward.amount).sum();
    let match_cards = matches
        .iter()
        .map(|league_match| {
            format!(
                "<article class=\"card match\"><div class=\"pill\">{}</div><h3>{}</h3><p>{}</p><footer><code>{}</code><span>{}</span></footer></article>",
                escape_html_text(&league_match.mode),
                escape_html_text(&league_match.title),
                escape_html_text(&league_match.objective),
                escape_html_text(&league_match.match_id),
                escape_html_text(&league_match.reward),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let leaderboard = players
        .iter()
        .take(8)
        .enumerate()
        .map(|(idx, player)| {
            format!(
                "<tr><td>#{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.2}</td></tr>",
                idx + 1,
                escape_html_text(&player.display_name),
                escape_html_text(&player.rank_tier),
                player.rating,
                player.earned_credits,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let leaderboard = if leaderboard.is_empty() {
        "<tr><td>#1</td><td>@alice:local.dev</td><td>Bronze I</td><td>1000</td><td>0.00</td></tr>"
            .to_string()
    } else {
        leaderboard
    };
    let guild_cards = league
        .guilds
        .values()
        .map(|guild| {
            format!(
                "<article class=\"mini\"><strong>{}</strong><span>{}</span><code>{}</code></article>",
                escape_html_text(&guild.name),
                escape_html_text(&guild.motto),
                escape_html_text(&guild.guild_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut timeline_items: Vec<(i64, String)> = Vec::new();
    for battle in league.battles.values() {
        timeline_items.push((
            battle.created_at_epoch,
            format!(
                "<li><b>⚔️ Battle</b><span>{}</span><small>{}</small></li>",
                escape_html_text(&battle.match_id),
                escape_html_text(&battle.task_id),
            ),
        ));
    }
    for submission in league.submissions.values() {
        timeline_items.push((
            submission.created_at_epoch,
            format!(
                "<li><b>🏁 Score {:.1}</b><span>{} · Judge {} · {} dims</span><small>{}</small></li>",
                submission.score,
                escape_html_text(&submission.grade),
                escape_html_text(submission.judge_status.as_deref().unwrap_or("rubric_scored")),
                submission.score_events.len(),
                escape_html_text(&submission.submission_id),
            ),
        ));
    }
    for reward in &league.rewards {
        timeline_items.push((
            reward.created_at_epoch,
            format!(
                "<li><b>💰 +{:.2} {}</b><span>{}</span><small>{}</small></li>",
                reward.amount,
                escape_html_text(&reward.currency_unit),
                escape_html_text(reward.ledger_status.as_deref().unwrap_or("pending")),
                escape_html_text(&reward.reward_id),
            ),
        ));
    }
    timeline_items.sort_by(|left, right| right.0.cmp(&left.0));
    let timeline = timeline_items
        .into_iter()
        .take(10)
        .map(|(_, html)| html)
        .collect::<Vec<_>>()
        .join("\n");
    let timeline = if timeline.is_empty() {
        "<li><b>No replays yet</b><span>Submit your first result</span><small>/submit</small></li>"
            .to_string()
    } else {
        timeline
    };
    let loadout = league_loadout_for_player(&league, "@alice:local.dev");
    let player_items: Vec<&LeagueInventoryItem> = league
        .inventory_items
        .iter()
        .filter(|item| item.matrix_user_id == "@alice:local.dev")
        .collect();
    let top_loot = player_items
        .iter()
        .max_by(|left, right| left.power.cmp(&right.power))
        .map(|item| format!("{} ({})", item.name, item.rarity))
        .unwrap_or_else(|| "No loot yet".to_string());
    let loadout_line = loadout
        .get("heroes")
        .and_then(Value::as_array)
        .map(|heroes| {
            heroes
                .iter()
                .filter_map(|hero| hero.get("name").and_then(Value::as_str))
                .map(escape_html_text)
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Oracle Scout · Forge Builder · Mirror Auditor".to_string());

    Html(format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Trillionnium League</title>
  <style>
    :root {{ color-scheme: dark; --bg:#060711; --panel:#111426; --panel2:#171b31; --gold:#f8c35b; --cyan:#64e3ff; --pink:#ff5ca8; --text:#f6f7fb; --muted:#9aa3b2; }}
    * {{ box-sizing:border-box; }}
    body {{ margin:0; min-height:100vh; font-family:Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background:radial-gradient(circle at 20% 0%, #213064 0, transparent 32rem), radial-gradient(circle at 88% 14%, #532044 0, transparent 30rem), var(--bg); color:var(--text); }}
    header {{ padding:42px min(6vw,72px) 18px; display:grid; gap:22px; grid-template-columns:1.25fr .75fr; align-items:end; }}
    h1 {{ margin:0; font-size:clamp(42px,7vw,92px); line-height:.9; letter-spacing:-.07em; }}
    h2 {{ margin:0 0 16px; letter-spacing:-.03em; }}
    .subtitle {{ color:var(--muted); font-size:18px; max-width:760px; }}
    .hero-card,.card,.panel {{ border:1px solid rgba(255,255,255,.11); background:linear-gradient(145deg,rgba(255,255,255,.09),rgba(255,255,255,.035)); box-shadow:0 24px 80px rgba(0,0,0,.35); backdrop-filter: blur(14px); border-radius:24px; }}
    .hero-card {{ padding:24px; }}
    .stats {{ display:grid; grid-template-columns:repeat(4,1fr); gap:14px; margin-top:22px; }}
    .stat {{ padding:18px; background:rgba(255,255,255,.06); border-radius:18px; }}
    .stat b {{ display:block; font-size:26px; color:var(--gold); }}
    main {{ padding:20px min(6vw,72px) 60px; display:grid; gap:24px; }}
    .grid {{ display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:18px; }}
    .card {{ padding:20px; min-height:210px; }}
    .card h3 {{ margin:12px 0; font-size:24px; }}
    .card p {{ color:var(--muted); line-height:1.55; }}
    .card footer {{ display:flex; justify-content:space-between; gap:10px; align-items:center; margin-top:18px; color:var(--gold); }}
    .pill {{ display:inline-flex; border:1px solid rgba(100,227,255,.35); color:var(--cyan); padding:5px 10px; border-radius:999px; font-size:12px; text-transform:uppercase; letter-spacing:.12em; }}
    .panel {{ padding:24px; }}
    table {{ width:100%; border-collapse:collapse; }}
    td,th {{ padding:12px 10px; border-bottom:1px solid rgba(255,255,255,.08); text-align:left; }}
    th {{ color:var(--muted); font-weight:600; }}
    .commands {{ display:flex; flex-wrap:wrap; gap:10px; }}
    .play {{ display:grid; grid-template-columns:1fr 1fr; gap:18px; }}
    form {{ display:grid; gap:10px; margin:0; }}
    input,textarea,select {{ width:100%; color:var(--text); background:rgba(255,255,255,.07); border:1px solid rgba(255,255,255,.14); border-radius:14px; padding:12px 14px; font:inherit; }}
    textarea {{ min-height:92px; resize:vertical; }}
    button {{ border:0; cursor:pointer; color:var(--bg); background:linear-gradient(135deg,var(--gold),#ff8d4d); padding:12px 16px; border-radius:14px; font-weight:800; }}
    .mini-grid {{ display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:12px; }}
    .mini {{ display:grid; gap:7px; padding:14px; border-radius:16px; background:rgba(255,255,255,.06); border:1px solid rgba(255,255,255,.08); }}
    .mini span, .timeline small {{ color:var(--muted); }}
    .timeline {{ list-style:none; padding:0; margin:0; display:grid; gap:10px; }}
    .timeline li {{ display:grid; grid-template-columns:1.3fr .6fr 1.1fr; gap:10px; padding:12px; border-radius:14px; background:rgba(255,255,255,.055); }}
    code {{ color:var(--cyan); background:rgba(100,227,255,.08); padding:3px 7px; border-radius:8px; }}
    .cta {{ color:var(--bg); background:linear-gradient(135deg,var(--gold),#ff8d4d); padding:14px 18px; border-radius:16px; display:inline-block; font-weight:800; }}
    @media (max-width:900px) {{ header {{ grid-template-columns:1fr; }} .grid,.stats,.play,.mini-grid {{ grid-template-columns:1fr; }} }}
  </style>
</head>
<body>
  <header>
    <section>
      <div class="pill">Preseason Zero</div>
      <h1>Trillionnium League</h1>
      <p class="subtitle">AI Agent Esports League for real work, skill, and earnings. Draft your agent squad, enter matches, clear quests, score, rank up, and earn credits.</p>
    </section>
    <aside class="hero-card">
      <strong>Live MVP</strong>
      <p class="subtitle">Playable through Matrix/Element now. Web shell is online as the first game lobby.</p>
      <a class="cta">Enter via /league</a>
    </aside>
  </header>
  <main>
    <section class="stats">
      <div class="stat"><span>Players</span><b>{players}</b></div>
      <div class="stat"><span>Matches</span><b>{matches}</b></div>
      <div class="stat"><span>Battles</span><b>{battles}</b></div>
      <div class="stat"><span>Rewards</span><b>{rewards:.2}</b></div>
      <div class="stat"><span>Items</span><b>{items}</b></div>
    </section>
    <section>
      <h2>Active Game Modes</h2>
      <div class="grid">{match_cards}</div>
    </section>
    <section class="panel">
      <h2>Trillionnium World</h2>
      <p class="subtitle">现实镜像开放世界：城市、Craft 工坊、Market、Agent 居民、资产和自由行动。</p>
      <div class="commands"><code>/world</code><code>/world action 我要开一家 AI 设计公司</code><code>Assets {world_assets}</code><code>Events {world_events}</code></div>
    </section>
    <section class="play">
      <div class="panel">
        <h2>Web Battle Console</h2>
        <p class="subtitle">{console_note}</p>
        <form method="post" action="/league/web/action">
          {csrf_input}
          <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
          <select name="action"><option value="join">Join Match</option><option value="guild">Join Guild</option><option value="team">Join Raid Team</option><option value="draft">Draft Loadout</option><option value="raid">Contribute Raid</option><option value="submit">Submit Result</option></select>
          <input name="match_id" value="daily-dungeon-001" aria-label="match id" />
          <input name="guild_id" value="guild-prompt-forge" aria-label="guild id" />
          <input name="role" value="scout" aria-label="raid role" />
          <input name="heroes" value="oracle_scout forge_builder mirror_auditor courier_closer" aria-label="heroes" />
          <textarea name="body">Web clear: deliverable, evidence, risk, self-review, next action. Raid option: scout evidence, assign builder, define boss risk gate.</textarea>
          <button type="submit">Play Action</button>
        </form>
      </div>
      <div class="panel">
        <h2>Battle Timeline / Replay</h2>
        <p class="subtitle">Current loadout: {loadout_line}</p>
        <p class="subtitle">Top loot: {top_loot}</p>
        <ul class="timeline">{timeline}</ul>
      </div>
    </section>
    <section class="panel">
      <h2>Guild Halls</h2>
      <div class="mini-grid">{guild_cards}</div>
    </section>
    <section class="panel">
      <h2>Leaderboard</h2>
      <table><thead><tr><th>#</th><th>Player</th><th>Rank</th><th>RP</th><th>Earned</th></tr></thead><tbody>{leaderboard}</tbody></table>
    </section>
    <section class="panel">
      <h2>Playable Commands</h2>
      <div class="commands"><code>/arena</code><code>/join daily-dungeon-001</code><code>/battle daily-dungeon-001 &lt;action&gt;</code><code>/submit daily-dungeon-001 &lt;result&gt;</code><code>/rank</code><code>/profile</code><code>/rewards</code><code>/history</code></div>
    </section>
  </main>
</body>
</html>"#,
        players = league.players_by_matrix_user.len(),
        matches = league.matches.len(),
        battles = league.battles.len(),
        rewards = total_rewards,
        items = player_items.len(),
        match_cards = match_cards,
        guild_cards = guild_cards,
        timeline = timeline,
        loadout_line = loadout_line,
        top_loot = escape_html_text(&top_loot),
        leaderboard = leaderboard,
        world_assets = league.world_assets.len(),
        world_events = league.world_events.len(),
        console_note = escape_html_text(console_note),
        csrf_input = csrf_input,
    ))
}

async fn post_league_web_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueWebSessionRequest>,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let Some(secret) = league_web_session_secret(state.config()) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "league web session secret is not configured" })),
        )
            .into_response();
    };
    if !matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        let scope = IdentityScope {
            source_kind: "league_web_session",
            user_id: Some(matrix_user_id.clone()),
            room_id: payload.room_id.clone(),
            session_id: payload.session_id.clone(),
            org_id: None,
            account_id: None,
        };
        let fingerprint = format!(
            "league-web-session:{}:{}:{}",
            matrix_user_id,
            payload.room_id.as_deref().unwrap_or_default(),
            payload.session_id.as_deref().unwrap_or_default(),
        );
        if let Err(response) = authorize_user_session(&state, &headers, &scope, &fingerprint) {
            return response;
        }
    }
    let now = Utc::now().timestamp();
    let csrf = payload
        .csrf
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            league_web_csrf(secret, &matrix_user_id, payload.room_id.as_deref(), now)
        });
    let claims = LeagueWebSessionClaims {
        version: 1,
        matrix_user_id: matrix_user_id.clone(),
        room_id: payload.room_id.clone(),
        session_id: payload.session_id.clone(),
        csrf,
        issued_at_epoch: now,
        expires_at_epoch: now + state.config().league_web_session_ttl_secs as i64,
    };
    let token = match encode_league_web_session(&claims, secret) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let secure = if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        ""
    } else {
        "; Secure"
    };
    let cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        state.config().league_web_session_cookie_name,
        token,
        state.config().league_web_session_ttl_secs,
        secure,
    );
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(json!({
            "kind": "league_web_session",
            "league": "trillionnium_league",
            "matrix_user_id": matrix_user_id,
            "room_id": payload.room_id,
            "session_id": payload.session_id,
            "csrf": claims.csrf,
            "expires_at_epoch": claims.expires_at_epoch,
        })),
    )
        .into_response()
}

async fn post_league_web_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<LeagueWebActionRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };

    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let action = payload.action.trim().to_ascii_lowercase();

    let snapshot = match action.as_str() {
        "join" => {
            let match_id = payload
                .match_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("daily-dungeon-001")
                .to_string();
            let mut league = state.inner.league_state.lock().await;
            if !league.matches.contains_key(&match_id) {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "league match not found", "match_id": match_id })),
                )
                    .into_response();
            }
            let player = ensure_league_player(&mut league, &matrix_user_id, None);
            ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
            league.clone()
        }
        "guild" => {
            let guild_id = payload
                .guild_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("guild-prompt-forge")
                .to_string();
            let mut league = state.inner.league_state.lock().await;
            if !league.guilds.contains_key(&guild_id) {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "league guild not found", "guild_id": guild_id })),
                )
                    .into_response();
            }
            let player = ensure_league_player(&mut league, &matrix_user_id, None);
            league.guild_memberships.insert(
                matrix_user_id.clone(),
                LeagueGuildMembership {
                    guild_id,
                    player_id: player.player_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    role: "member".to_string(),
                    joined_at_epoch: Utc::now().timestamp(),
                },
            );
            league.clone()
        }
        "draft" => {
            let heroes = normalize_hero_draft(
                payload
                    .heroes
                    .as_deref()
                    .unwrap_or("oracle_scout forge_builder mirror_auditor courier_closer")
                    .split_whitespace()
                    .map(ToString::to_string)
                    .collect(),
            );
            if heroes.len() < 3 {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "draft requires at least 3 unique heroes",
                        "examples": "oracle_scout forge_builder mirror_auditor courier_closer"
                    })),
                )
                    .into_response();
            }
            let mut league = state.inner.league_state.lock().await;
            ensure_league_player(&mut league, &matrix_user_id, None);
            league
                .player_loadouts
                .insert(matrix_user_id.clone(), heroes);
            league.clone()
        }
        "submit" => {
            let match_id = payload
                .match_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("daily-dungeon-001")
                .to_string();
            let body = match validate_text_payload(
                payload
                    .body
                    .as_deref()
                    .unwrap_or("Web action: evidence, risk, deliverable, next step."),
                state.config().max_text_chars,
            ) {
                Ok(value) => value,
                Err(response) => return response,
            };
            let league_match_mode = {
                let league = state.inner.league_state.lock().await;
                let Some(league_match) = league.matches.get(&match_id).cloned() else {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(json!({ "error": "league match not found", "match_id": match_id })),
                    )
                        .into_response();
                };
                league_match.mode
            };
            let judgement =
                judge_league_submission_with_pipeline(&state, &body, &league_match_mode).await;
            let (submission, mut reward) = {
                let mut league = state.inner.league_state.lock().await;
                if !league.matches.contains_key(&match_id) {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(json!({ "error": "league match not found", "match_id": match_id })),
                    )
                        .into_response();
                }
                let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
                let mut entry =
                    ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
                let score = judgement.score;
                let grade = judgement.grade.clone();
                let reward_amount = judgement.reward_amount;
                let now = Utc::now().timestamp();
                let submission_id = league_hash_id(
                    "submission",
                    &format!("web:{}:{}:{}:{}", match_id, entry.entry_id, now, body),
                );
                let submission = LeagueSubmission {
                    submission_id: submission_id.clone(),
                    match_id: match_id.clone(),
                    entry_id: entry.entry_id.clone(),
                    player_id: player.player_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    task_id: None,
                    body: body.clone(),
                    score,
                    grade: grade.clone(),
                    reward_amount,
                    judge_status: Some(judgement.judge_status.clone()),
                    payout_status: Some(judgement.payout_status.clone()),
                    anti_cheat_flags: judgement.anti_cheat_flags.clone(),
                    score_events: judgement.score_events.clone(),
                    created_at_epoch: now,
                };
                let reward = LeagueReward {
                    reward_id: league_hash_id("reward", &submission_id),
                    match_id: match_id.clone(),
                    entry_id: entry.entry_id.clone(),
                    player_id: player.player_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    amount: reward_amount,
                    currency_unit: "credit".to_string(),
                    reason: format!("league_web_submission_score_{score:.1}_{grade}"),
                    ledger_status: Some("pending".to_string()),
                    ledger_account_id: None,
                    ledger_entry_id: None,
                    ledger_balance_after: None,
                    ledger_error: None,
                    review_status: if judgement.payout_status == "review_hold" {
                        Some("pending_review".to_string())
                    } else {
                        None
                    },
                    reviewed_by: None,
                    review_note: None,
                    reviewed_at_epoch: None,
                    created_at_epoch: now,
                };
                player.submissions += 1;
                player.xp += score.round() as i64;
                player.reputation += (score / 10.0).round() as i64;
                player.rating += ((score - 50.0) / 2.0).round() as i64;
                if judgement.payout_status == "eligible" {
                    player.earned_credits += reward_amount;
                }
                if score >= 80.0 {
                    player.wins += 1;
                }
                entry.submissions += 1;
                entry.best_score = entry.best_score.max(score);
                if judgement.payout_status == "eligible" {
                    entry.rewards_earned += reward_amount;
                }
                league
                    .players_by_matrix_user
                    .insert(matrix_user_id.clone(), player.clone());
                league
                    .entries
                    .insert(league_entry_key(&match_id, &matrix_user_id), entry.clone());
                league
                    .submissions
                    .insert(submission_id.clone(), submission.clone());
                league.rewards.push(reward.clone());
                if judgement.payout_status == "eligible" {
                    league
                        .inventory_items
                        .push(league_item_for_submission(&submission));
                }
                (submission, reward)
            };
            let submit_payload = LeagueSubmitRequest {
                matrix_user_id: matrix_user_id.clone(),
                room_id: Some("!web-local:local.dev".to_string()),
                task_id: None,
                body: submission.body.clone(),
            };
            let settlement = settle_league_reward_with_ledger(
                &state,
                &submit_payload,
                &matrix_user_id,
                &submission,
                &reward,
            )
            .await;
            reward.ledger_status = Some(settlement.status);
            reward.ledger_account_id = settlement.account_id;
            reward.ledger_entry_id = settlement.entry_id;
            reward.ledger_balance_after = settlement.balance_after;
            reward.ledger_error = settlement.error;

            let mut league = state.inner.league_state.lock().await;
            if let Some(stored_reward) = league
                .rewards
                .iter_mut()
                .find(|stored| stored.reward_id == reward.reward_id)
            {
                *stored_reward = reward;
            }
            league.clone()
        }
        "raid" => {
            let match_id = payload
                .match_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("guild-raid-001")
                .to_string();
            let body = match validate_text_payload(
                payload.body.as_deref().unwrap_or(
                    "Raid contribution: scout evidence, assign builder, define risk gate.",
                ),
                state.config().max_text_chars,
            ) {
                Ok(value) => value,
                Err(response) => return response,
            };
            let request = LeagueRaidContributionRequest {
                matrix_user_id: matrix_user_id.clone(),
                room_id: Some("!web-local:local.dev".to_string()),
                role: Some("web_raider".to_string()),
                body,
            };
            let snapshot = match record_league_raid_contribution(&state, &match_id, request).await {
                Ok((snapshot, _contribution, _progress)) => snapshot,
                Err(response) => return response,
            };
            snapshot
        }
        "team" => {
            let match_id = payload
                .match_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("guild-raid-001")
                .to_string();
            let hero_id = payload
                .heroes
                .as_deref()
                .and_then(|heroes| heroes.split_whitespace().next())
                .map(ToString::to_string)
                .or_else(|| Some("oracle_scout".to_string()));
            let request = LeagueRaidRosterRequest {
                matrix_user_id: matrix_user_id.clone(),
                room_id: Some("!web-local:local.dev".to_string()),
                role: payload.role.clone().or_else(|| Some("scout".to_string())),
                hero_id,
            };
            let snapshot = match record_league_raid_roster_slot(&state, &match_id, request).await {
                Ok((snapshot, _slot, _roster)) => snapshot,
                Err(response) => return response,
            };
            snapshot
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "unsupported league web action",
                    "allowed": ["join", "guild", "draft", "submit", "raid", "team"]
                })),
            )
                .into_response()
        }
    };

    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }

    Redirect::to("/league?played=1").into_response()
}

async fn get_league_raids(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let raids: Vec<LeagueMatch> = league
        .matches
        .values()
        .filter(|league_match| league_match.mode == "guild_raid")
        .cloned()
        .collect();
    let progress: Vec<Value> = raids
        .iter()
        .map(|raid| league_raid_progress(&league, &raid.match_id))
        .collect();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_raids",
            "league": "trillionnium_league",
            "raids": raids,
            "progress": progress,
            "guilds": league_guild_standings(&league),
        })),
    )
        .into_response()
}

async fn contribute_league_raid(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueRaidContributionRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let (snapshot, contribution, progress) =
        match record_league_raid_contribution(&state, &match_id, payload).await {
            Ok(value) => value,
            Err(response) => return response,
        };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_raid_contribution",
            "league": "trillionnium_league",
            "contribution": contribution,
            "progress": progress,
        })),
    )
        .into_response()
}

async fn get_league_raid_roster(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    if !league
        .matches
        .get(&match_id)
        .is_some_and(|league_match| league_match.mode == "guild_raid")
    {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "league raid not found", "match_id": match_id })),
        )
            .into_response();
    }
    let roster = league_raid_roster_summary(&league, &match_id);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_raid_roster",
            "league": "trillionnium_league",
            "match_id": match_id,
            "roster": roster,
        })),
    )
        .into_response()
}

async fn join_league_raid_roster(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueRaidRosterRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let (snapshot, slot, roster) =
        match record_league_raid_roster_slot(&state, &match_id, payload).await {
            Ok(value) => value,
            Err(response) => return response,
        };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_raid_roster_joined",
            "league": "trillionnium_league",
            "slot": slot,
            "roster": roster,
        })),
    )
        .into_response()
}

fn league_submission_for_reward<'a>(
    league: &'a LeagueState,
    reward: &LeagueReward,
) -> Option<&'a LeagueSubmission> {
    league
        .submissions
        .values()
        .find(|submission| league_hash_id("reward", &submission.submission_id) == reward.reward_id)
}

async fn get_league_held_reviews(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let held: Vec<Value> = league
        .rewards
        .iter()
        .filter(|reward| {
            reward
                .ledger_status
                .as_deref()
                .is_some_and(|status| status == "held_review")
                || reward
                    .review_status
                    .as_deref()
                    .is_some_and(|status| status == "pending_review")
        })
        .map(|reward| {
            json!({
                "reward": reward,
                "submission": league_submission_for_reward(&league, reward),
            })
        })
        .collect();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_review_queue",
            "league": "trillionnium_league",
            "held_count": held.len(),
            "held": held,
        })),
    )
        .into_response()
}

async fn approve_league_review(
    Path(reward_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueReviewRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let reviewer_id = payload
        .reviewer_id
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local-reviewer".to_string());
    let review_note = payload
        .note
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let (reward, mut submission) = {
        let league = state.inner.league_state.lock().await;
        let Some(reward) = league
            .rewards
            .iter()
            .find(|reward| reward.reward_id == reward_id)
            .cloned()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league reward not found", "reward_id": reward_id })),
            )
                .into_response();
        };
        if reward.ledger_status.as_deref() == Some("settled") {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "league reward already settled", "reward_id": reward_id })),
            )
                .into_response();
        }
        let Some(submission) = league_submission_for_reward(&league, &reward).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league submission for reward not found", "reward_id": reward_id })),
            )
                .into_response();
        };
        (reward, submission)
    };
    submission.payout_status = Some("approved_release".to_string());
    let submit_payload = LeagueSubmitRequest {
        matrix_user_id: reward.matrix_user_id.clone(),
        room_id: payload
            .room_id
            .clone()
            .or_else(|| Some("!web-local:local.dev".to_string())),
        task_id: submission.task_id.clone(),
        body: submission.body.clone(),
    };
    let settlement = settle_league_reward_with_ledger(
        &state,
        &submit_payload,
        &reward.matrix_user_id,
        &submission,
        &reward,
    )
    .await;
    let now = Utc::now().timestamp();
    let released = settlement.status == "settled" || settlement.status == "duplicate";
    let snapshot = {
        let mut league = state.inner.league_state.lock().await;
        if let Some(stored_submission) = league.submissions.get_mut(&submission.submission_id) {
            stored_submission.payout_status = Some("approved_release".to_string());
            stored_submission.score_events.push(LeagueScoreEvent {
                dimension: "human_review_release".to_string(),
                score: stored_submission.score,
                weight: 0.0,
                judge_kind: "review_admin_v1".to_string(),
                evidence: json!({"reviewer_id": reviewer_id.clone(), "note": review_note.clone()}),
            });
        }
        if released {
            if let Some(player) = league
                .players_by_matrix_user
                .get_mut(&reward.matrix_user_id)
            {
                player.earned_credits += reward.amount;
            }
            if let Some(entry) = league
                .entries
                .get_mut(&league_entry_key(&reward.match_id, &reward.matrix_user_id))
            {
                entry.rewards_earned += reward.amount;
            }
        }
        if let Some(stored_reward) = league
            .rewards
            .iter_mut()
            .find(|stored| stored.reward_id == reward.reward_id)
        {
            stored_reward.ledger_status = Some(settlement.status.clone());
            stored_reward.ledger_account_id = settlement.account_id.clone();
            stored_reward.ledger_entry_id = settlement.entry_id.clone();
            stored_reward.ledger_balance_after = settlement.balance_after;
            stored_reward.ledger_error = settlement.error.clone();
            stored_reward.review_status = Some(
                if released {
                    "approved"
                } else {
                    "approval_failed"
                }
                .to_string(),
            );
            stored_reward.reviewed_by = Some(reviewer_id.clone());
            stored_reward.review_note = review_note.clone();
            stored_reward.reviewed_at_epoch = Some(now);
        }
        league.clone()
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_review_approved",
            "league": "trillionnium_league",
            "reward_id": reward.reward_id,
            "review_status": if released { "approved" } else { "approval_failed" },
            "ledger_status": settlement.status,
            "ledger_entry_id": settlement.entry_id,
            "ledger_error": settlement.error,
        })),
    )
        .into_response()
}

async fn reject_league_review(
    Path(reward_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueReviewRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let reviewer_id = payload
        .reviewer_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local-reviewer".to_string());
    let review_note = payload
        .note
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let now = Utc::now().timestamp();
    let snapshot = {
        let mut league = state.inner.league_state.lock().await;
        let Some(reward_index) = league
            .rewards
            .iter()
            .position(|reward| reward.reward_id == reward_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league reward not found", "reward_id": reward_id })),
            )
                .into_response();
        };
        let reward = league.rewards[reward_index].clone();
        let submission_id = league_submission_for_reward(&league, &reward)
            .map(|submission| submission.submission_id.clone());
        if let Some(submission_id) = submission_id {
            if let Some(submission) = league.submissions.get_mut(&submission_id) {
                submission.payout_status = Some("rejected".to_string());
                submission.score_events.push(LeagueScoreEvent {
                    dimension: "human_review_reject".to_string(),
                    score: submission.score,
                    weight: 0.0,
                    judge_kind: "review_admin_v1".to_string(),
                    evidence: json!({"reviewer_id": reviewer_id.clone(), "note": review_note.clone()}),
                });
            }
        }
        let reward = &mut league.rewards[reward_index];
        reward.ledger_status = Some("rejected".to_string());
        reward.review_status = Some("rejected".to_string());
        reward.reviewed_by = Some(reviewer_id);
        reward.review_note = review_note;
        reward.reviewed_at_epoch = Some(now);
        league.clone()
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_review_rejected",
            "league": "trillionnium_league",
            "reward_id": reward_id,
            "review_status": "rejected",
            "ledger_status": "rejected",
        })),
    )
        .into_response()
}

async fn get_league_matches(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let league = state.inner.league_state.lock().await;
    let mut matches: Vec<LeagueMatch> = league.matches.values().cloned().collect();
    matches.sort_by(|left, right| left.match_id.cmp(&right.match_id));
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_matches",
            "league": "trillionnium_league",
            "matches": matches,
        })),
    )
        .into_response()
}

async fn get_league_state_snapshot(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let hash = match league_state_hash(&league) {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to hash league state: {err}") })),
            )
                .into_response()
        }
    };
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_state_snapshot",
            "league": "trillionnium_league",
            "repository": "json_file_with_sql_snapshot",
            "state_hash": hash,
            "json_state_path_configured": state.config().league_state_path.is_some(),
            "sql_snapshot_path_configured": state.config().league_sql_snapshot_path.is_some(),
            "counts": {
                "players": league.players_by_matrix_user.len(),
                "matches": league.matches.len(),
                "entries": league.entries.len(),
                "battles": league.battles.len(),
                "submissions": league.submissions.len(),
                "rewards": league.rewards.len(),
                "inventory_items": league.inventory_items.len(),
                "raid_contributions": league.raid_contributions.len(),
                "raid_rosters": league.raid_rosters.len(),
            },
        })),
    )
        .into_response()
}

async fn get_league_guilds(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let mut guilds: Vec<LeagueGuild> = league.guilds.values().cloned().collect();
    guilds.sort_by(|left, right| {
        right
            .rating
            .cmp(&left.rating)
            .then_with(|| left.guild_id.cmp(&right.guild_id))
    });
    let standings = league_guild_standings(&league);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_guilds",
            "league": "trillionnium_league",
            "guilds": guilds,
            "standings": standings,
        })),
    )
        .into_response()
}

async fn join_league_guild(
    Path(guild_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueJoinRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let (guild, player, membership, snapshot) = {
        let mut league = state.inner.league_state.lock().await;
        let Some(guild) = league.guilds.get(&guild_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league guild not found", "guild_id": guild_id })),
            )
                .into_response();
        };
        let player = ensure_league_player(
            &mut league,
            &matrix_user_id,
            payload.display_name.as_deref(),
        );
        let membership = LeagueGuildMembership {
            guild_id: guild_id.clone(),
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            role: "member".to_string(),
            joined_at_epoch: Utc::now().timestamp(),
        };
        league
            .guild_memberships
            .insert(matrix_user_id.clone(), membership.clone());
        let snapshot = league.clone();
        (guild, player, membership, snapshot)
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_guild_joined",
            "league": "trillionnium_league",
            "guild": guild,
            "player": player,
            "membership": membership,
            "room_id": payload.room_id,
        })),
    )
        .into_response()
}

async fn join_league_match(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueJoinRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };

    let (league_match, player, entry, snapshot) = {
        let mut league = state.inner.league_state.lock().await;
        let Some(league_match) = league.matches.get(&match_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league match not found", "match_id": match_id })),
            )
                .into_response();
        };
        let player = ensure_league_player(
            &mut league,
            &matrix_user_id,
            payload.display_name.as_deref(),
        );
        let entry = ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
        let snapshot = league.clone();
        (league_match, player, entry, snapshot)
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_joined",
            "league": "trillionnium_league",
            "match": league_match,
            "player": player,
            "entry": entry,
            "room_id": payload.room_id,
        })),
    )
        .into_response()
}

async fn create_league_battle(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueBattleRequest>,
) -> Response {
    state.inner.metrics.inc_task_create_requests();
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };

    let (league_match, player, entry) = {
        let mut league = state.inner.league_state.lock().await;
        let Some(league_match) = league.matches.get(&match_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league match not found", "match_id": match_id })),
            )
                .into_response();
        };
        let player = ensure_league_player(&mut league, &matrix_user_id, None);
        let mut entry =
            ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
        entry.battles_started += 1;
        league
            .entries
            .insert(league_entry_key(&match_id, &matrix_user_id), entry.clone());
        (league_match, player, entry)
    };

    let metadata = merge_league_battle_metadata(payload.metadata, &match_id, &entry.entry_id);
    let matrix_payload = MatrixMessageRequest {
        matrix_user_id: matrix_user_id.clone(),
        room_id: payload.room_id,
        session_id: None,
        org_id: None,
        message: payload.message,
        capability_id: payload.capability_id,
        account_id: payload.account_id,
        event_id: payload.event_id,
        idempotency_key: None,
        metadata: Some(metadata),
    };

    let resolved_identity = match resolve_matrix_identity(&state, &matrix_payload).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let request_fingerprint = build_matrix_request_fingerprint(&matrix_payload);
    let authorized_session = match authorize_user_session(
        &state,
        &headers,
        &resolved_identity.scope,
        request_fingerprint.as_str(),
    ) {
        Ok(session) => session,
        Err(response) => return response,
    };
    let prompt = match validate_text_payload(&matrix_payload.message, state.config().max_text_chars)
    {
        Ok(prompt) => prompt,
        Err(response) => return response,
    };
    let resolved_account_id = resolved_identity.scope.account_id.clone();
    let source = json!({
        "kind": "league_battle",
        "league": "trillionnium_league",
        "match_id": match_id,
        "entry_id": entry.entry_id,
        "player_id": player.player_id,
        "identity_scope": resolved_identity.scope,
        "identity_resolution": resolved_identity.resolution,
        "matrix_user_id": matrix_user_id,
        "room_id": matrix_payload.room_id,
        "event_id": matrix_payload.event_id,
        "metadata": matrix_payload.metadata,
        "session_auth": authorized_session,
    });

    let task = match forward_to_cex_task(
        state.clone(),
        matrix_payload.capability_id,
        resolved_account_id,
        source,
        prompt,
    )
    .await
    {
        Ok(response) => response,
        Err(response) => return response,
    };

    let (player, entry, battle, snapshot) = {
        let mut league = state.inner.league_state.lock().await;
        let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
        player.battles += 1;
        let mut entry =
            ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
        let task_id = task.task_id.clone();
        let battle_key = league_hash_id(
            "battle",
            &format!("{}:{}:{}", match_id, entry.entry_id, task_id),
        );
        let battle = LeagueBattle {
            battle_id: battle_key.clone(),
            match_id: match_id.clone(),
            entry_id: entry.entry_id.clone(),
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            task_id,
            prompt: task
                .request
                .as_ref()
                .and_then(|value| value.get("prompt"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            status: "created".to_string(),
            created_at_epoch: Utc::now().timestamp(),
        };
        entry.battles_started = entry.battles_started.max(1);
        league
            .players_by_matrix_user
            .insert(matrix_user_id.clone(), player.clone());
        league
            .entries
            .insert(league_entry_key(&match_id, &matrix_user_id), entry.clone());
        league.battles.insert(battle_key, battle.clone());
        let snapshot = league.clone();
        (player, entry, battle, snapshot)
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "kind": "league_battle",
            "league": "trillionnium_league",
            "match": league_match,
            "player": player,
            "entry": entry,
            "battle": battle,
            "task": task,
        })),
    )
        .into_response()
}

async fn submit_league_match(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueSubmitRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };

    let league_match = {
        let league = state.inner.league_state.lock().await;
        let Some(league_match) = league.matches.get(&match_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league match not found", "match_id": match_id })),
            )
                .into_response();
        };
        league_match
    };
    let judgement = judge_league_submission_with_pipeline(&state, &body, &league_match.mode).await;

    let (player, entry, submission, reward, _snapshot) = {
        let mut league = state.inner.league_state.lock().await;
        if !league.matches.contains_key(&match_id) {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league match not found", "match_id": match_id })),
            )
                .into_response();
        }
        let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
        let mut entry =
            ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
        let score = judgement.score;
        let grade = judgement.grade.clone();
        let reward_amount = judgement.reward_amount;
        let now = Utc::now().timestamp();
        let submission_id = league_hash_id(
            "submission",
            &format!("{}:{}:{}:{}", match_id, entry.entry_id, now, body),
        );
        let submission = LeagueSubmission {
            submission_id: submission_id.clone(),
            match_id: match_id.clone(),
            entry_id: entry.entry_id.clone(),
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            task_id: payload.task_id.clone(),
            body: body.clone(),
            score,
            grade: grade.clone(),
            reward_amount,
            judge_status: Some(judgement.judge_status.clone()),
            payout_status: Some(judgement.payout_status.clone()),
            anti_cheat_flags: judgement.anti_cheat_flags.clone(),
            score_events: judgement.score_events.clone(),
            created_at_epoch: now,
        };
        let reward = LeagueReward {
            reward_id: league_hash_id("reward", &submission_id),
            match_id: match_id.clone(),
            entry_id: entry.entry_id.clone(),
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            amount: reward_amount,
            currency_unit: "credit".to_string(),
            reason: format!("league_submission_score_{score:.1}_{grade}"),
            ledger_status: Some("pending".to_string()),
            ledger_account_id: None,
            ledger_entry_id: None,
            ledger_balance_after: None,
            ledger_error: None,
            review_status: if judgement.payout_status == "review_hold" {
                Some("pending_review".to_string())
            } else {
                None
            },
            reviewed_by: None,
            review_note: None,
            reviewed_at_epoch: None,
            created_at_epoch: now,
        };
        player.submissions += 1;
        player.xp += score.round() as i64;
        player.reputation += (score / 10.0).round() as i64;
        player.rating += ((score - 50.0) / 2.0).round() as i64;
        if judgement.payout_status == "eligible" {
            player.earned_credits += reward_amount;
        }
        if score >= 80.0 {
            player.wins += 1;
        }
        entry.submissions += 1;
        entry.best_score = entry.best_score.max(score);
        if judgement.payout_status == "eligible" {
            entry.rewards_earned += reward_amount;
        }
        league
            .players_by_matrix_user
            .insert(matrix_user_id.clone(), player.clone());
        league
            .entries
            .insert(league_entry_key(&match_id, &matrix_user_id), entry.clone());
        league.submissions.insert(submission_id, submission.clone());
        league.rewards.push(reward.clone());
        if judgement.payout_status == "eligible" {
            league
                .inventory_items
                .push(league_item_for_submission(&submission));
        }
        let snapshot = league.clone();
        (player, entry, submission, reward, snapshot)
    };

    let mut reward = reward;
    let settlement =
        settle_league_reward_with_ledger(&state, &payload, &matrix_user_id, &submission, &reward)
            .await;
    reward.ledger_status = Some(settlement.status);
    reward.ledger_account_id = settlement.account_id;
    reward.ledger_entry_id = settlement.entry_id;
    reward.ledger_balance_after = settlement.balance_after;
    reward.ledger_error = settlement.error;

    let snapshot = {
        let mut league = state.inner.league_state.lock().await;
        if let Some(stored_reward) = league
            .rewards
            .iter_mut()
            .find(|stored| stored.reward_id == reward.reward_id)
        {
            *stored_reward = reward.clone();
        }
        league.clone()
    };

    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }

    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_submission",
            "league": "trillionnium_league",
            "match": league_match,
            "player": player,
            "entry": entry,
            "submission": submission,
            "reward": reward,
            "room_id": payload.room_id,
        })),
    )
        .into_response()
}

#[derive(Debug, Clone, Default)]
struct LeagueLedgerSettlement {
    status: String,
    account_id: Option<String>,
    entry_id: Option<String>,
    balance_after: Option<f64>,
    error: Option<String>,
}

async fn settle_league_reward_with_ledger(
    state: &AppState,
    payload: &LeagueSubmitRequest,
    matrix_user_id: &str,
    submission: &LeagueSubmission,
    reward: &LeagueReward,
) -> LeagueLedgerSettlement {
    if reward.amount <= 0.0 {
        return LeagueLedgerSettlement {
            status: "skipped_zero_reward".to_string(),
            ..Default::default()
        };
    }
    let payout_status = submission.payout_status.as_deref().unwrap_or("eligible");
    let approved_release = payout_status == "approved_release";
    if !(payout_status == "eligible" || approved_release)
        || (!approved_release && !submission.anti_cheat_flags.is_empty())
    {
        return LeagueLedgerSettlement {
            status: "held_review".to_string(),
            error: Some(format!(
                "payout held by review gate: status={payout_status} flags={}",
                submission.anti_cheat_flags.join(",")
            )),
            ..Default::default()
        };
    }

    let Some(room_id) = payload
        .room_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_room".to_string(),
            ..Default::default()
        };
    };

    let matrix_payload = MatrixMessageRequest {
        matrix_user_id: matrix_user_id.to_string(),
        room_id: room_id.to_string(),
        session_id: None,
        org_id: None,
        message: "league reward settlement".to_string(),
        capability_id: None,
        account_id: None,
        event_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let resolved_identity = match resolve_matrix_identity(state, &matrix_payload).await {
        Ok(identity) => identity,
        Err(_) => {
            return LeagueLedgerSettlement {
                status: "failed_identity".to_string(),
                error: Some(
                    "matrix identity could not be resolved for reward settlement".to_string(),
                ),
                ..Default::default()
            }
        }
    };

    let Some(account_id) = resolved_identity.scope.account_id.clone() else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_account".to_string(),
            error: Some("matrix identity did not resolve a ledger account_id".to_string()),
            ..Default::default()
        };
    };
    let Some(ledger_admin_token) = state.config().ledger_admin_token.clone() else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_ledger_token".to_string(),
            account_id: Some(account_id),
            error: Some("consumer-entry ledger admin token is not configured".to_string()),
            ..Default::default()
        };
    };

    let url = format!(
        "{}/v1/ledger/grant",
        state.config().ledger_base_url.trim_end_matches('/')
    );
    let mut body = json!({
        "account_id": account_id,
        "amount": reward.amount,
        "idempotency_key": format!("league_reward:{}", reward.reward_id),
    });
    if let Some(task_id) = submission
        .task_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        body["reference_id"] = json!(task_id);
    }

    let response = match state
        .inner
        .http
        .post(url)
        .header("x-admin-token", ledger_admin_token)
        .json(&body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return LeagueLedgerSettlement {
                status: "failed_network".to_string(),
                account_id: body
                    .get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                error: Some(format!("failed to reach ledger-service: {err}")),
                ..Default::default()
            }
        }
    };

    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return LeagueLedgerSettlement {
                status: "failed_bad_response".to_string(),
                account_id: body
                    .get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                error: Some(format!("ledger-service returned non-json response: {err}")),
                ..Default::default()
            }
        }
    };

    if !status.is_success() {
        let error = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("ledger grant failed");
        return LeagueLedgerSettlement {
            status: if status.as_u16() == 409 {
                "duplicate".to_string()
            } else {
                "failed_ledger".to_string()
            },
            account_id: body
                .get("account_id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            error: Some(format!("{}: {error}", status.as_u16())),
            ..Default::default()
        };
    }

    LeagueLedgerSettlement {
        status: "settled".to_string(),
        account_id: value
            .get("account")
            .and_then(|account| account.get("account_id"))
            .and_then(Value::as_str)
            .or_else(|| body.get("account_id").and_then(Value::as_str))
            .map(ToString::to_string),
        entry_id: value
            .get("entry")
            .and_then(|entry| entry.get("entry_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        balance_after: value
            .get("account")
            .and_then(|account| account.get("balance"))
            .and_then(Value::as_f64),
        error: None,
    }
}

async fn get_league_rankings(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let mut players: Vec<LeaguePlayer> = league.players_by_matrix_user.values().cloned().collect();
    players.sort_by(|left, right| {
        right
            .rating
            .cmp(&left.rating)
            .then_with(|| right.xp.cmp(&left.xp))
            .then_with(|| left.matrix_user_id.cmp(&right.matrix_user_id))
    });
    let guild_standings = league_guild_standings(&league);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_rankings",
            "league": "trillionnium_league",
            "season": "preseason-zero",
            "players": players,
            "guilds": guild_standings,
        })),
    )
        .into_response()
}

async fn get_league_player_profile(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let mut league = state.inner.league_state.lock().await;
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let player = ensure_league_player(&mut league, &matrix_user_id, None);
    let loadout = league_loadout_for_player(&league, &matrix_user_id);
    let guild = league
        .guild_memberships
        .get(&matrix_user_id)
        .and_then(|membership| league.guilds.get(&membership.guild_id))
        .cloned();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_player_profile",
            "league": "trillionnium_league",
            "player": player,
            "guild": guild,
            "loadout": loadout,
        })),
    )
        .into_response()
}

async fn get_league_player_loadout(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let mut league = state.inner.league_state.lock().await;
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let player = ensure_league_player(&mut league, &matrix_user_id, None);
    let loadout = league_loadout_for_player(&league, &matrix_user_id);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_loadout",
            "league": "trillionnium_league",
            "player": player,
            "loadout": loadout,
        })),
    )
        .into_response()
}

async fn update_league_player_draft(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueDraftRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(path_matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let request_matrix_user_id = normalize_league_matrix_user(&payload.matrix_user_id)
        .unwrap_or_else(|| path_matrix_user_id.clone());
    if request_matrix_user_id != path_matrix_user_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "draft matrix_user_id does not match path" })),
        )
            .into_response();
    }
    let heroes = normalize_hero_draft(payload.heroes);
    if heroes.len() < 3 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "draft requires at least 3 unique heroes" })),
        )
            .into_response();
    }
    let (player, loadout, snapshot) = {
        let mut league = state.inner.league_state.lock().await;
        let player = ensure_league_player(&mut league, &path_matrix_user_id, None);
        league
            .player_loadouts
            .insert(path_matrix_user_id.clone(), heroes.clone());
        let loadout = league_loadout_for_player(&league, &path_matrix_user_id);
        let snapshot = league.clone();
        (player, loadout, snapshot)
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_draft",
            "league": "trillionnium_league",
            "player": player,
            "loadout": loadout,
        })),
    )
        .into_response()
}

async fn get_league_player_rewards(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let league = state.inner.league_state.lock().await;
    let rewards: Vec<LeagueReward> = league
        .rewards
        .iter()
        .filter(|reward| reward.matrix_user_id == matrix_user_id)
        .cloned()
        .collect();
    let total_earned: f64 = rewards.iter().map(|reward| reward.amount).sum();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_rewards",
            "league": "trillionnium_league",
            "matrix_user_id": matrix_user_id,
            "total_earned": total_earned,
            "currency_unit": "credit",
            "rewards": rewards,
        })),
    )
        .into_response()
}

async fn get_league_player_inventory(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let league = state.inner.league_state.lock().await;
    let mut items: Vec<LeagueInventoryItem> = league
        .inventory_items
        .iter()
        .filter(|item| item.matrix_user_id == matrix_user_id)
        .cloned()
        .collect();
    items.sort_by(|left, right| {
        right
            .power
            .cmp(&left.power)
            .then_with(|| right.created_at_epoch.cmp(&left.created_at_epoch))
    });
    let total_power: i64 = items.iter().map(|item| item.power).sum();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_inventory",
            "league": "trillionnium_league",
            "matrix_user_id": matrix_user_id,
            "item_count": items.len(),
            "total_power": total_power,
            "items": items,
        })),
    )
        .into_response()
}

async fn get_league_player_history(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let league = state.inner.league_state.lock().await;
    let battles: Vec<LeagueBattle> = league
        .battles
        .values()
        .filter(|battle| battle.matrix_user_id == matrix_user_id)
        .cloned()
        .collect();
    let submissions: Vec<LeagueSubmission> = league
        .submissions
        .values()
        .filter(|submission| submission.matrix_user_id == matrix_user_id)
        .cloned()
        .collect();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_history",
            "league": "trillionnium_league",
            "matrix_user_id": matrix_user_id,
            "battles": battles,
            "submissions": submissions,
        })),
    )
        .into_response()
}

fn normalize_league_matrix_user(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn escape_html_text(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn league_web_session_secret(config: &ConsumerEntryConfig) -> Option<&str> {
    config
        .league_web_session_secret
        .as_deref()
        .or(config.session_auth_secret.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn league_web_csrf(
    secret: &str,
    matrix_user_id: &str,
    room_id: Option<&str>,
    issued_at: i64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update(b":league-web-csrf:");
    hasher.update(matrix_user_id.as_bytes());
    hasher.update(b":");
    hasher.update(room_id.unwrap_or_default().as_bytes());
    hasher.update(b":");
    hasher.update(issued_at.to_string().as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn encode_league_web_session(
    claims: &LeagueWebSessionClaims,
    secret: &str,
) -> Result<String, Response> {
    let assertion = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to serialize web session: {err}") })),
        )
            .into_response()
    })?);
    let signature = sign_user_session_assertion(&assertion, secret)?;
    Ok(format!("{assertion}.{signature}"))
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookie_header| {
            cookie_header.split(';').find_map(|part| {
                let (key, value) = part.trim().split_once('=')?;
                (key == name).then(|| value.to_string())
            })
        })
}

fn authorize_league_web_session(
    state: &AppState,
    headers: &HeaderMap,
    csrf: Option<&str>,
) -> Result<Option<LeagueWebSessionClaims>, Response> {
    let Some(raw_cookie) = cookie_value(headers, &state.config().league_web_session_cookie_name)
    else {
        if state.config().league_web_session_required {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "missing league web session cookie",
                    "session_endpoint": "/league/web/session",
                })),
            )
                .into_response());
        }
        return Ok(None);
    };
    let Some((assertion, signature)) = raw_cookie.split_once('.') else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid league web session cookie format" })),
        )
            .into_response());
    };
    let Some(secret) = league_web_session_secret(state.config()) else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "league web session is required but no secret is configured" })),
        )
            .into_response());
    };
    let expected_signature = sign_user_session_assertion(assertion, secret)?;
    if expected_signature != signature {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid league web session signature" })),
        )
            .into_response());
    }
    let bytes = URL_SAFE_NO_PAD.decode(assertion).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "league web session assertion must be base64url JSON" })),
        )
            .into_response()
    })?;
    let claims = serde_json::from_slice::<LeagueWebSessionClaims>(&bytes).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("invalid league web session payload: {err}") })),
        )
            .into_response()
    })?;
    let now = Utc::now().timestamp();
    if claims.expires_at_epoch < now {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "league web session expired" })),
        )
            .into_response());
    }
    if let Some(provided_csrf) = csrf.map(str::trim).filter(|value| !value.is_empty()) {
        if provided_csrf != claims.csrf {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "league web csrf mismatch" })),
            )
                .into_response());
        }
    } else if state.config().league_web_session_required {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "league web csrf token is required" })),
        )
            .into_response());
    }
    Ok(Some(claims))
}

fn league_hash_id(prefix: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let encoded = URL_SAFE_NO_PAD.encode(hasher.finalize());
    format!("{prefix}-{}", &encoded[..16])
}

fn league_entry_key(match_id: &str, matrix_user_id: &str) -> String {
    format!("{match_id}\u{1f}{matrix_user_id}")
}

fn league_raid_roster_summary(league: &LeagueState, match_id: &str) -> Value {
    let mut slots: Vec<LeagueRaidRosterSlot> = league
        .raid_rosters
        .iter()
        .filter(|slot| slot.match_id == match_id && slot.status == "active")
        .cloned()
        .collect();
    slots.sort_by(|left, right| {
        left.role
            .cmp(&right.role)
            .then_with(|| left.matrix_user_id.cmp(&right.matrix_user_id))
    });
    let required_roles = ["scout", "builder", "auditor", "closer"];
    let filled_roles: Vec<String> = slots.iter().map(|slot| slot.role.clone()).collect();
    let missing_roles: Vec<&str> = required_roles
        .iter()
        .copied()
        .filter(|role| !filled_roles.iter().any(|filled| filled == role))
        .collect();
    json!({
        "match_id": match_id,
        "slots": slots,
        "slot_count": filled_roles.len(),
        "required_roles": required_roles,
        "missing_roles": missing_roles,
        "ready": missing_roles.is_empty(),
    })
}

async fn record_league_raid_roster_slot(
    state: &AppState,
    match_id: &str,
    payload: LeagueRaidRosterRequest,
) -> Result<(LeagueState, LeagueRaidRosterSlot, Value), Response> {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response())
        }
    };
    let role = payload
        .role
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("scout")
        .to_ascii_lowercase()
        .replace('-', "_");
    let hero_id = payload
        .hero_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("oracle_scout")
        .to_ascii_lowercase()
        .replace('-', "_");
    let (snapshot, slot, roster) = {
        let mut league = state.inner.league_state.lock().await;
        let Some(raid_match) = league.matches.get(match_id).cloned() else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league raid not found", "match_id": match_id })),
            )
                .into_response());
        };
        if raid_match.mode != "guild_raid" {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "match is not a guild raid", "match_id": match_id })),
            )
                .into_response());
        }
        let player = ensure_league_player(&mut league, &matrix_user_id, None);
        let guild_id = league
            .guild_memberships
            .get(&matrix_user_id)
            .map(|membership| membership.guild_id.clone());
        for existing in &mut league.raid_rosters {
            if existing.match_id == match_id && existing.matrix_user_id == matrix_user_id {
                existing.status = "replaced".to_string();
            }
        }
        let now = Utc::now().timestamp();
        let slot = LeagueRaidRosterSlot {
            slot_id: league_hash_id(
                "slot",
                &format!("{}:{}:{}:{}", match_id, matrix_user_id, role, now),
            ),
            match_id: match_id.to_string(),
            guild_id,
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            room_id: payload.room_id.clone(),
            role,
            hero_id,
            status: "active".to_string(),
            joined_at_epoch: now,
        };
        league.raid_rosters.push(slot.clone());
        let roster = league_raid_roster_summary(&league, match_id);
        (league.clone(), slot, roster)
    };
    Ok((snapshot, slot, roster))
}

fn league_raid_progress(league: &LeagueState, match_id: &str) -> Value {
    let contributions: Vec<&LeagueRaidContribution> = league
        .raid_contributions
        .iter()
        .filter(|contribution| contribution.match_id == match_id)
        .collect();
    let total_progress: f64 = contributions
        .iter()
        .map(|contribution| contribution.progress_delta)
        .sum::<f64>()
        .min(100.0);
    let average_score = if contributions.is_empty() {
        0.0
    } else {
        contributions
            .iter()
            .map(|contribution| contribution.contribution_score)
            .sum::<f64>()
            / contributions.len() as f64
    };
    let mut roles: Vec<String> = contributions
        .iter()
        .map(|contribution| contribution.role.clone())
        .collect();
    roles.sort();
    roles.dedup();
    json!({
        "match_id": match_id,
        "phase": if total_progress >= 100.0 { "cleared" } else if total_progress >= 60.0 { "boss" } else if total_progress >= 25.0 { "mid" } else { "opening" },
        "progress_percent": (total_progress * 10.0).round() / 10.0,
        "contribution_count": contributions.len(),
        "average_score": (average_score * 10.0).round() / 10.0,
        "roles": roles,
        "target_percent": 100.0,
    })
}

async fn record_league_raid_contribution(
    state: &AppState,
    match_id: &str,
    payload: LeagueRaidContributionRequest,
) -> Result<(LeagueState, LeagueRaidContribution, Value), Response> {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response())
        }
    };
    let body = validate_text_payload(&payload.body, state.config().max_text_chars)?;
    let role = payload
        .role
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("raider")
        .to_string();
    let raid_mode = {
        let league = state.inner.league_state.lock().await;
        let Some(raid_match) = league.matches.get(match_id).cloned() else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league raid not found", "match_id": match_id })),
            )
                .into_response());
        };
        if raid_match.mode != "guild_raid" {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "match is not a guild raid", "match_id": match_id })),
            )
                .into_response());
        }
        raid_match.mode
    };
    let judgement = judge_league_submission_with_pipeline(state, &body, &raid_mode).await;
    let (snapshot, contribution, progress) = {
        let mut league = state.inner.league_state.lock().await;
        if !league
            .matches
            .get(match_id)
            .is_some_and(|league_match| league_match.mode == "guild_raid")
        {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league raid not found", "match_id": match_id })),
            )
                .into_response());
        }
        let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
        let mut entry =
            ensure_league_entry(&mut league, match_id, &matrix_user_id, &player.player_id);
        let guild_id = league
            .guild_memberships
            .get(&matrix_user_id)
            .map(|membership| membership.guild_id.clone());
        let progress_delta = (judgement.score / 8.0).clamp(4.0, 14.0);
        let now = Utc::now().timestamp();
        let contribution = LeagueRaidContribution {
            contribution_id: league_hash_id(
                "raid",
                &format!("{}:{}:{}:{}", match_id, matrix_user_id, now, body),
            ),
            match_id: match_id.to_string(),
            guild_id,
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            room_id: payload.room_id.clone(),
            role,
            body: body.clone(),
            contribution_score: judgement.score,
            progress_delta,
            created_at_epoch: now,
        };
        player.xp += (judgement.score / 2.0).round() as i64;
        player.reputation += (judgement.score / 20.0).round() as i64;
        player.rating += ((judgement.score - 50.0) / 6.0).round() as i64;
        entry.battles_started += 1;
        league
            .players_by_matrix_user
            .insert(matrix_user_id.clone(), player);
        league
            .entries
            .insert(league_entry_key(match_id, &matrix_user_id), entry);
        league.raid_contributions.push(contribution.clone());
        let progress = league_raid_progress(&league, match_id);
        (league.clone(), contribution, progress)
    };
    Ok((snapshot, contribution, progress))
}

fn league_guild_standings(league: &LeagueState) -> Vec<Value> {
    let mut standings: Vec<Value> = league
        .guilds
        .values()
        .map(|guild| {
            let members: Vec<&LeagueGuildMembership> = league
                .guild_memberships
                .values()
                .filter(|membership| membership.guild_id == guild.guild_id)
                .collect();
            let member_count = members.len();
            let mut total_rating = guild.rating as f64;
            let mut total_earned = guild.treasury_credits;
            let mut submissions = 0_i64;
            for membership in members {
                if let Some(player) = league
                    .players_by_matrix_user
                    .get(&membership.matrix_user_id)
                {
                    total_rating += player.rating as f64;
                    total_earned += player.earned_credits;
                    submissions += player.submissions;
                }
            }
            let power_score = total_rating + total_earned * 10.0 + submissions as f64 * 25.0;
            json!({
                "guild_id": guild.guild_id,
                "name": guild.name,
                "member_count": member_count,
                "rating": guild.rating,
                "power_score": (power_score * 10.0).round() / 10.0,
                "earned_credits": (total_earned * 100.0).round() / 100.0,
                "submissions": submissions,
            })
        })
        .collect();
    standings.sort_by(|left, right| {
        let left_score = left
            .get("power_score")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let right_score = right
            .get("power_score")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.get("guild_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("guild_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
    });
    standings
}

fn ensure_league_player(
    league: &mut LeagueState,
    matrix_user_id: &str,
    display_name: Option<&str>,
) -> LeaguePlayer {
    if let Some(player) = league.players_by_matrix_user.get(matrix_user_id) {
        return player.clone();
    }
    let now = Utc::now().timestamp();
    let display_name = display_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(matrix_user_id)
        .to_string();
    let player = LeaguePlayer {
        player_id: league_hash_id("player", matrix_user_id),
        matrix_user_id: matrix_user_id.to_string(),
        display_name,
        class_tag: "summoner".to_string(),
        rank_tier: "Bronze I".to_string(),
        rating: 1000,
        xp: 0,
        reputation: 0,
        battles: 0,
        submissions: 0,
        wins: 0,
        earned_credits: 0.0,
        created_at_epoch: now,
    };
    league
        .players_by_matrix_user
        .insert(matrix_user_id.to_string(), player.clone());
    player
}

fn ensure_league_entry(
    league: &mut LeagueState,
    match_id: &str,
    matrix_user_id: &str,
    player_id: &str,
) -> LeagueMatchEntry {
    let key = league_entry_key(match_id, matrix_user_id);
    if let Some(entry) = league.entries.get(&key) {
        return entry.clone();
    }
    let entry = LeagueMatchEntry {
        entry_id: league_hash_id("entry", &key),
        match_id: match_id.to_string(),
        player_id: player_id.to_string(),
        matrix_user_id: matrix_user_id.to_string(),
        status: "joined".to_string(),
        battles_started: 0,
        submissions: 0,
        best_score: 0.0,
        rewards_earned: 0.0,
        joined_at_epoch: Utc::now().timestamp(),
    };
    league.entries.insert(key, entry.clone());
    entry
}

fn merge_league_battle_metadata(metadata: Option<Value>, match_id: &str, entry_id: &str) -> Value {
    let mut value = metadata.unwrap_or_else(|| json!({}));
    if !value.is_object() {
        value = json!({ "original_metadata": value });
    }
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "trillionnium_league".to_string(),
            json!({
                "version": 1,
                "match_id": match_id,
                "entry_id": entry_id,
                "action": "battle",
            }),
        );
    }
    value
}

fn league_item_for_submission(submission: &LeagueSubmission) -> LeagueInventoryItem {
    let (rarity, item_kind, name, power) = if submission.score >= 90.0 {
        ("mythic", "sigil", "Mythic Forge Sigil", 95)
    } else if submission.score >= 80.0 {
        ("epic", "badge", "Epic Quest Badge", 82)
    } else if submission.score >= 70.0 {
        ("rare", "charm", "Rare Evidence Charm", 68)
    } else {
        ("common", "token", "League Practice Token", 45)
    };
    LeagueInventoryItem {
        item_id: league_hash_id("item", &submission.submission_id),
        player_id: submission.player_id.clone(),
        matrix_user_id: submission.matrix_user_id.clone(),
        source_submission_id: submission.submission_id.clone(),
        item_kind: item_kind.to_string(),
        name: name.to_string(),
        rarity: rarity.to_string(),
        power,
        cosmetic: true,
        created_at_epoch: submission.created_at_epoch,
    }
}

fn judge_league_submission(body: &str, mode: &str) -> LeagueJudgement {
    let chars = body.chars().count() as f64;
    let lower = body.to_ascii_lowercase();
    let has_deliver = lower.contains("deliver")
        || lower.contains("customer")
        || body.contains("客户")
        || body.contains("交付")
        || body.contains("方案");
    let has_evidence = lower.contains("evidence")
        || lower.contains("source")
        || lower.contains("data")
        || body.contains("证据")
        || body.contains("依据");
    let has_risk = lower.contains("risk") || body.contains("风险");
    let has_review = lower.contains("review") || body.contains("自评") || body.contains("复盘");
    let has_next = lower.contains("next") || body.contains("下一步") || body.contains("计划");

    let delivery_score = if has_deliver { 86.0 } else { 54.0 };
    let evidence_score = if has_evidence { 82.0 } else { 48.0 };
    let risk_score = if has_risk { 80.0 } else { 46.0 };
    let action_score = if has_next { 84.0 } else { 52.0 };
    let polish_score =
        ((chars / 1.4).clamp(40.0, 88.0) + if has_review { 8.0 } else { 0.0 }).min(96.0);

    let events = vec![
        LeagueScoreEvent {
            dimension: "delivery_fit".to_string(),
            score: delivery_score,
            weight: 0.30,
            judge_kind: "rubric_v1".to_string(),
            evidence: json!({"has_deliverable": has_deliver}),
        },
        LeagueScoreEvent {
            dimension: "evidence_grounding".to_string(),
            score: evidence_score,
            weight: 0.24,
            judge_kind: "rubric_v1".to_string(),
            evidence: json!({"has_evidence": has_evidence}),
        },
        LeagueScoreEvent {
            dimension: "risk_control".to_string(),
            score: risk_score,
            weight: 0.18,
            judge_kind: "rubric_v1".to_string(),
            evidence: json!({"has_risk": has_risk}),
        },
        LeagueScoreEvent {
            dimension: "actionability".to_string(),
            score: action_score,
            weight: 0.16,
            judge_kind: "rubric_v1".to_string(),
            evidence: json!({"has_next_step": has_next}),
        },
        LeagueScoreEvent {
            dimension: "craft_polish".to_string(),
            score: polish_score,
            weight: 0.12,
            judge_kind: "rubric_v1".to_string(),
            evidence: json!({"chars": chars, "has_self_review": has_review}),
        },
    ];
    let score = events
        .iter()
        .map(|event| event.score * event.weight)
        .sum::<f64>()
        .clamp(0.0, 100.0);
    let score = (score * 10.0).round() / 10.0;
    let grade = league_judgement_grade(score);
    let mut anti_cheat_flags = Vec::new();
    if chars < 24.0 {
        anti_cheat_flags.push("too_short".to_string());
    }
    if lower.matches("copy").count() >= 3 || body.matches('复').count() >= 12 {
        anti_cheat_flags.push("repetition_suspected".to_string());
    }
    let mode_multiplier = match mode {
        "bounty_arena" => 1.5,
        "guild_raid" => 1.25,
        _ => 1.0,
    };
    let reward_amount = ((score / 20.0) * mode_multiplier * 100.0).round() / 100.0;
    LeagueJudgement {
        score,
        grade,
        reward_amount,
        judge_status: "rubric_scored".to_string(),
        payout_status: if anti_cheat_flags.is_empty() {
            "eligible".to_string()
        } else {
            "review_hold".to_string()
        },
        anti_cheat_flags,
        score_events: events,
    }
}

fn league_judgement_grade(score: f64) -> String {
    if score >= 90.0 {
        "S"
    } else if score >= 80.0 {
        "A"
    } else if score >= 70.0 {
        "B"
    } else if score >= 60.0 {
        "C"
    } else {
        "D"
    }
    .to_string()
}

fn league_hidden_test_event(body: &str, mode: &str) -> (LeagueScoreEvent, Vec<String>) {
    let lower = body.to_ascii_lowercase();
    let mut tests = vec![
        json!({"id": "deliverable_anchor", "passed": lower.contains("deliver") || lower.contains("customer") || body.contains("客户") || body.contains("交付") || body.contains("方案")}),
        json!({"id": "evidence_anchor", "passed": lower.contains("evidence") || lower.contains("source") || lower.contains("data") || body.contains("证据") || body.contains("依据")}),
        json!({"id": "risk_gate", "passed": lower.contains("risk") || body.contains("风险")}),
        json!({"id": "next_action", "passed": lower.contains("next") || body.contains("下一步") || body.contains("计划")}),
        json!({"id": "substance_length", "passed": body.chars().count() >= 40}),
        json!({"id": "not_repetitive", "passed": lower.matches("copy").count() < 3 && body.matches('复').count() < 12}),
    ];
    if mode == "guild_raid" {
        tests.push(json!({"id": "raid_coordination", "passed": lower.contains("scout") || lower.contains("builder") || lower.contains("team") || lower.contains("gate") || body.contains("团队") || body.contains("分工")}));
    }
    let passed = tests
        .iter()
        .filter(|test| test.get("passed").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let score = ((passed as f64 / tests.len() as f64) * 1000.0).round() / 10.0;
    let mut flags = Vec::new();
    if score < 70.0 {
        flags.push("hidden_tests_failed".to_string());
    }
    if tests.iter().any(|test| {
        test.get("id").and_then(Value::as_str) == Some("evidence_anchor")
            && !test.get("passed").and_then(Value::as_bool).unwrap_or(false)
    }) {
        flags.push("hidden_missing_evidence".to_string());
    }
    (
        LeagueScoreEvent {
            dimension: "hidden_tests".to_string(),
            score,
            weight: 0.0,
            judge_kind: "hidden_tests_v1".to_string(),
            evidence: json!({
                "mode": mode,
                "passed": passed,
                "total": tests.len(),
                "tests": tests,
            }),
        },
        flags,
    )
}

async fn call_league_llm_judge_adapter(
    state: &AppState,
    body: &str,
    mode: &str,
    judgement: &LeagueJudgement,
) -> LeagueExternalJudgeOutcome {
    let Some(url) = state.config().league_llm_judge_url.clone() else {
        return LeagueExternalJudgeOutcome {
            event: LeagueScoreEvent {
                dimension: "llm_judge_adapter".to_string(),
                score: judgement.score,
                weight: 0.0,
                judge_kind: "llm_adapter_v1".to_string(),
                evidence: json!({"status": "not_configured"}),
            },
            flags: Vec::new(),
            grade_override: None,
        };
    };
    let mut request = state
        .inner
        .http
        .post(url.trim())
        .timeout(Duration::from_millis(
            state.config().league_llm_judge_timeout_ms,
        ))
        .json(&json!({
            "league": "trillionnium_league",
            "mode": mode,
            "submission": {"body": body},
            "rubric": {
                "score": judgement.score,
                "grade": &judgement.grade,
                "events": &judgement.score_events,
            }
        }));
    if let Some(token) = state.config().league_llm_judge_token.as_deref() {
        request = request.bearer_auth(token);
    }
    let required = state.config().league_llm_judge_required;
    let response = match request.send().await {
        Ok(response) => response,
        Err(err) => {
            return LeagueExternalJudgeOutcome {
                event: LeagueScoreEvent {
                    dimension: "llm_judge_adapter".to_string(),
                    score: judgement.score,
                    weight: 0.0,
                    judge_kind: "llm_adapter_v1".to_string(),
                    evidence: json!({"status": "network_error", "error": err.to_string()}),
                },
                flags: if required {
                    vec!["judge_adapter_unavailable".to_string()]
                } else {
                    Vec::new()
                },
                grade_override: None,
            }
        }
    };
    let status = response.status();
    if !status.is_success() {
        return LeagueExternalJudgeOutcome {
            event: LeagueScoreEvent {
                dimension: "llm_judge_adapter".to_string(),
                score: judgement.score,
                weight: 0.0,
                judge_kind: "llm_adapter_v1".to_string(),
                evidence: json!({"status": "http_error", "http_status": status.as_u16()}),
            },
            flags: if required {
                vec!["judge_adapter_unavailable".to_string()]
            } else {
                Vec::new()
            },
            grade_override: None,
        };
    }
    let parsed = match response.json::<LeagueExternalJudgeResponse>().await {
        Ok(value) => value,
        Err(err) => {
            return LeagueExternalJudgeOutcome {
                event: LeagueScoreEvent {
                    dimension: "llm_judge_adapter".to_string(),
                    score: judgement.score,
                    weight: 0.0,
                    judge_kind: "llm_adapter_v1".to_string(),
                    evidence: json!({"status": "bad_json", "error": err.to_string()}),
                },
                flags: if required {
                    vec!["judge_adapter_unavailable".to_string()]
                } else {
                    Vec::new()
                },
                grade_override: None,
            }
        }
    };
    let adapter_score = parsed.score.unwrap_or(judgement.score).clamp(0.0, 100.0);
    let delta = ((adapter_score - judgement.score).abs() * 10.0).round() / 10.0;
    let mut flags = parsed.flags.unwrap_or_default();
    if delta >= 18.0 {
        flags.push("judge_disagreement".to_string());
    }
    LeagueExternalJudgeOutcome {
        event: LeagueScoreEvent {
            dimension: "llm_judge_adapter".to_string(),
            score: (adapter_score * 10.0).round() / 10.0,
            weight: 0.0,
            judge_kind: "llm_adapter_v1".to_string(),
            evidence: json!({
                "status": "scored",
                "delta_from_rubric": delta,
                "verdict": parsed.verdict,
                "explanation": parsed.explanation,
                "evidence": parsed.evidence,
            }),
        },
        flags,
        grade_override: parsed.grade,
    }
}

async fn judge_league_submission_with_pipeline(
    state: &AppState,
    body: &str,
    mode: &str,
) -> LeagueJudgement {
    let mut judgement = judge_league_submission(body, mode);
    if state.config().league_hidden_tests_enabled {
        let (event, mut flags) = league_hidden_test_event(body, mode);
        judgement.score_events.push(event);
        judgement.anti_cheat_flags.append(&mut flags);
    }
    let external = call_league_llm_judge_adapter(state, body, mode, &judgement).await;
    judgement.score_events.push(external.event);
    judgement.anti_cheat_flags.extend(external.flags);
    if let Some(grade) = external
        .grade_override
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        judgement.grade = grade;
    }
    judgement.anti_cheat_flags.sort();
    judgement.anti_cheat_flags.dedup();
    judgement.payout_status = if judgement.anti_cheat_flags.is_empty() {
        "eligible".to_string()
    } else {
        "review_hold".to_string()
    };
    judgement.judge_status = if state.config().league_llm_judge_url.is_some() {
        "rubric_hidden_llm_pipeline_v2".to_string()
    } else {
        "rubric_hidden_pipeline_v2".to_string()
    };
    judgement
}

fn default_league_loadout() -> Value {
    loadout_json_from_heroes(&[
        "oracle_scout".to_string(),
        "forge_builder".to_string(),
        "mirror_auditor".to_string(),
        "courier_closer".to_string(),
    ])
}

fn league_loadout_for_player(league: &LeagueState, matrix_user_id: &str) -> Value {
    league
        .player_loadouts
        .get(matrix_user_id)
        .map(|heroes| loadout_json_from_heroes(heroes))
        .unwrap_or_else(default_league_loadout)
}

fn normalize_hero_draft(values: Vec<String>) -> Vec<String> {
    let mut heroes = Vec::new();
    for value in values {
        let hero = value.trim().to_ascii_lowercase().replace('-', "_");
        if hero.is_empty() || heroes.contains(&hero) {
            continue;
        }
        heroes.push(hero);
        if heroes.len() >= 5 {
            break;
        }
    }
    heroes
}

fn loadout_json_from_heroes(heroes: &[String]) -> Value {
    let hero_values: Vec<Value> = heroes
        .iter()
        .map(|hero| {
            let (name, role) = match hero.as_str() {
                "oracle_scout" => ("Oracle Scout", "侦察/调研"),
                "forge_builder" => ("Forge Builder", "生成/构建"),
                "mirror_auditor" => ("Mirror Auditor", "审核/测试"),
                "courier_closer" => ("Courier Closer", "交付/包装"),
                "ledger_warden" => ("Ledger Warden", "成本/风控"),
                "muse_designer" => ("Muse Designer", "视觉/设计"),
                _ => (hero.as_str(), "自定义英雄"),
            };
            json!({"hero_id": hero, "name": name, "role": role})
        })
        .collect();
    json!({
        "heroes": hero_values,
        "draft_unlocked": true,
        "max_slots": 5,
    })
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

async fn get_matrix_wallet(
    Path(matrix_user_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let room_id = query.get("room_id").cloned().unwrap_or_default();
    let payload = MatrixMessageRequest {
        matrix_user_id: matrix_user_id.clone(),
        room_id: room_id.clone(),
        session_id: query.get("session_id").cloned(),
        org_id: query.get("org_id").cloned(),
        message: "wallet lookup".to_string(),
        capability_id: None,
        account_id: query.get("account_id").cloned(),
        event_id: None,
        idempotency_key: None,
        metadata: None,
    };

    let resolved_identity = match resolve_matrix_identity(&state, &payload).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };

    let Some(account_id) = resolved_identity.scope.account_id.clone() else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "wallet account not resolved",
                "identity_scope": resolved_identity.scope,
                "identity_resolution": resolved_identity.resolution,
            })),
        )
            .into_response();
    };

    let Some(ledger_admin_token) = state.config().ledger_admin_token.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "consumer-entry ledger admin token not configured",
                "message": "set CONSUMER_ENTRY_LEDGER_ADMIN_TOKEN or LEDGER_ADMIN_TOKENS_JSON to enable wallet projection",
            })),
        )
            .into_response();
    };

    let url = format!(
        "{}/v1/accounts/{}",
        state.config().ledger_base_url.trim_end_matches('/'),
        account_id
    );
    let response = match state
        .inner
        .http
        .get(url)
        .header("x-admin-token", ledger_admin_token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("failed to reach ledger-service: {err}") })),
            )
                .into_response()
        }
    };

    let status = response.status();
    let account =
        match response.json::<Value>().await {
            Ok(value) => value,
            Err(err) => return (
                StatusCode::BAD_GATEWAY,
                Json(
                    json!({ "error": format!("ledger-service returned non-json response: {err}") }),
                ),
            )
                .into_response(),
        };

    if !status.is_success() {
        return (status, Json(account)).into_response();
    }

    let balance = account
        .get("balance")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let reserved = account
        .get("reserved")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let available = balance - reserved;
    let currency_unit = account
        .get("currency_unit")
        .and_then(Value::as_str)
        .unwrap_or("credit");

    (
        StatusCode::OK,
        Json(json!({
            "kind": "wallet_projection",
            "matrix_user_id": matrix_user_id,
            "room_id": room_id,
            "identity_scope": resolved_identity.scope,
            "identity_resolution": resolved_identity.resolution,
            "account": {
                "account_id": account_id,
                "account_type": account.get("account_type").cloned().unwrap_or(Value::Null),
                "currency_unit": currency_unit,
                "balance": balance,
                "reserved": reserved,
                "available": available,
                "raw": account,
            },
            "package": {
                "code": "local-production-basic",
                "name": "Local Production Credits",
                "status": "active",
                "billing_model": "credit_wallet",
                "features": ["chat_tasks", "matrix_entry", "provider_dispatch"]
            },
            "display": {
                "title": "CEX 钱包 / 套餐",
                "summary": format!("可用 {available:.2} {currency_unit}，已预留 {reserved:.2}，总额 {balance:.2}"),
            }
        })),
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

fn build_world_action_request_fingerprint(payload: &WorldActionRequest) -> String {
    build_request_fingerprint(&[
        "matrix_message".to_string(),
        normalize_request_fingerprint_value(Some(payload.matrix_user_id.as_str())),
        normalize_request_fingerprint_value(payload.room_id.as_deref()),
        normalize_request_fingerprint_value(None),
        normalize_request_fingerprint_value(None),
        normalize_request_fingerprint_value(payload.account_id.as_deref()),
        normalize_request_fingerprint_value(payload.capability_id.as_deref()),
        normalize_request_fingerprint_value(payload.event_id.as_deref()),
        normalize_request_fingerprint_value(None),
        normalize_request_fingerprint_text(payload.message.as_deref().unwrap_or("")),
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

        issuer_keys
            .get(&key_id)
            .map(|value| value.as_str())
            .ok_or_else(|| {
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

    let claimed_request_fingerprint =
        normalize_identity_value(claims.request_fingerprint.as_deref()).ok_or_else(|| {
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
    if let Err(response) =
        verify_session_auth_claim_field("org_id", claims.org_id.as_deref(), scope.org_id.as_deref())
    {
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
        build_chat_room_rate_limit_key, build_chat_session_rate_limit_key,
        build_chat_user_rate_limit_key, build_matrix_identity_scope,
        build_matrix_org_rate_limit_key, build_matrix_rate_limit_key, build_matrix_replay_key,
        build_matrix_room_rate_limit_key, build_matrix_session_rate_limit_key,
        build_matrix_user_rate_limit_key, build_router, default_league_state,
        evaluate_identity_binding_reload_governance, load_identity_binding_revision_approval_state,
        load_identity_binding_store, load_rate_limit_cache, load_session_auth_issuer_registry,
        load_session_auth_issuer_registry_revision_approval_state, parse_csv_list,
        project_consumer_status, prune_rate_limit_cache, resolve_chat_identity,
        session_auth_issuer_registry_active_key_diff_json, sign_user_session_assertion,
        validate_text_payload, AppState, AppStateInner, ConsumerEntryConfig, ConsumerEntryMetrics,
        CreateChatTaskRequest, IdentityBindingAuditState, IdentityBindingEntry,
        IdentityBindingMetadata, IdentityBindingRevisionApprovalState, IdentityBindingStore,
        IdentityBindings, MatrixMessageRequest, ProductUserIdentity, RateLimitCache, ReplayCache,
        RuntimeProfile, SessionAuthIssuerRegistryIssuer, SessionAuthIssuerRegistryMetadata,
        SessionAuthIssuerRegistryRuntimeState, UserSessionAuthClaims,
        DEFAULT_LEAGUE_LLM_JUDGE_TIMEOUT_MS, DEFAULT_LEAGUE_WEB_SESSION_TTL_SECS,
        DEFAULT_MAX_TEXT_CHARS, USER_SESSION_ASSERTION_HEADER, USER_SESSION_SIGNATURE_HEADER,
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
            ledger_base_url: "http://127.0.0.1:7002".to_string(),
            ledger_admin_token: None,
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
            session_auth_issuer_registry_actor_header: "x-session-auth-issuer-registry-actor"
                .to_string(),
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
            league_state_path: None,
            league_sql_snapshot_path: None,
            league_hidden_tests_enabled: true,
            league_llm_judge_url: None,
            league_llm_judge_token: None,
            league_llm_judge_required: false,
            league_llm_judge_timeout_ms: DEFAULT_LEAGUE_LLM_JUDGE_TIMEOUT_MS,
            league_web_session_required: false,
            league_web_session_secret: None,
            league_web_session_cookie_name: "cex_league_session".to_string(),
            league_web_session_ttl_secs: DEFAULT_LEAGUE_WEB_SESSION_TTL_SECS,
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
                session_auth_issuer_registry_state: StdRwLock::new(
                    session_auth_issuer_registry_state,
                ),
                league_state: Mutex::new(default_league_state()),
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
        assert_eq!(
            authorized.claims.audience.as_deref(),
            Some("consumer-entry-api")
        );
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
        let signature = sign_user_session_assertion(&assertion, "issuer-key-secret").unwrap();
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
        let signature = sign_user_session_assertion(&assertion, "issuer-registry-secret").unwrap();
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
            diff.get("changes").and_then(Value::as_array).map(Vec::len),
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
        assert_eq!(
            approval_state.revision.as_deref(),
            Some("session-approval-a")
        );
        assert_eq!(
            approval_state.approved_revisions,
            vec!["sess-reg-a", "sess-reg-b"]
        );

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

        let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
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
        assert_eq!(
            registry.get("issuer_count").and_then(Value::as_u64),
            Some(2)
        );
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

        let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
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
        let temp_registry_path =
            temp_identity_bindings_path("session-auth-registry-active-key-diff");
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
        let temp_registry_path =
            temp_identity_bindings_path("session-auth-registry-validate-actor");
        std::fs::write(
            &temp_registry_path,
            r#"{"version":1,"revision":"sess-reg-actor-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}"#,
        )
        .expect("write session auth issuer registry");

        let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
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
        assert_eq!(
            body_missing.get("status").and_then(Value::as_str),
            Some("actor_missing")
        );
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

        let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
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
        assert_eq!(
            body.get("status").and_then(Value::as_str),
            Some("no_allowed_actors_configured")
        );
    }

    #[tokio::test]
    async fn session_auth_issuer_registry_approval_validate_endpoint_rejects_unapproved_revision() {
        let temp_registry_path =
            temp_identity_bindings_path("session-auth-registry-approval-validate");
        let temp_approval_path =
            temp_identity_bindings_path("session-auth-registry-approval-validate-source");
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

        let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
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
        assert_eq!(
            body.get("status").and_then(Value::as_str),
            Some("current_revision_not_approved")
        );
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
        let temp_registry_path =
            temp_identity_bindings_path("session-auth-registry-approval-status");
        let temp_approval_path =
            temp_identity_bindings_path("session-auth-registry-approval-status-source");
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

        let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
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

        let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
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

        let (metadata, registry) = load_session_auth_issuer_registry(temp_registry_path.to_str());
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
        assert!(body
            .contains("cex_consumer_entry_session_auth_issuer_registry_approval_source_valid 0"));
        assert!(body
            .contains("cex_consumer_entry_session_auth_issuer_registry_approval_coverage_valid 0"));

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
