use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

pub mod trnm_v1;
pub use hepta_paper_raid_contracts as paper_raid_contracts;
mod paper_raid_v2;
mod workflows;

pub use paper_raid_v2::*;

pub const AGENT_PROTOCOL_V1: &str = "hepta_agent_protocol_v1";
pub const EVENT_SCHEMA_V1: &str = "hepta_event_envelope_v1";
pub const KEY_ROTATION_PROTOCOL_V1: &str = "hepta_agent_key_rotation_v1";
pub const NAKAMA_AUTHORIZATION_SCHEMA_V1: &str = "trnm.match.authorization.v1";
pub const OPERATOR_TOKEN_HEADER: &str = "x-hepta-operator-token";
pub const NAKAMA_TOKEN_HEADER: &str = "x-hepta-nakama-token";
pub const TRNM_TOKEN_HEADER: &str = "x-hepta-trnm-token";
pub const USER_ASSERTION_HEADER: &str = "x-hepta-user-assertion";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinalityMode {
    PendingOnly,
    Verified,
}

impl FinalityMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PendingOnly => "pending_only",
            Self::Verified => "verified",
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<RwLock<LeagueState>>,
    paper_raid: Arc<RwLock<paper_raid_v2::PaperRaidMemory>>,
    pool: Option<PgPool>,
    security: Arc<SecurityConfig>,
    rate_limits: Arc<Mutex<HashMap<(String, String), RateWindow>>>,
}

#[derive(Debug, Clone)]
struct RateWindow {
    started_at: DateTime<Utc>,
    requests: u32,
}

#[derive(Clone)]
pub struct SecurityConfig {
    operator_token: String,
    nakama_token: String,
    trnm_token: String,
    nakama_authorization_issuer_key_id: String,
    nakama_authorization_signing_key: SigningKey,
    consumer_edge_issuer: String,
    consumer_edge_audience: String,
    consumer_edge_issuer_key_id: String,
    consumer_edge_verifying_key: VerifyingKey,
    trusted_nakama_research_authorities: HashMap<String, VerifyingKey>,
    finality_mode: FinalityMode,
    trusted_trnm_validator_sets: Vec<trnm_v1::TrustedValidatorSetV1>,
}

impl SecurityConfig {
    pub fn new(operator_token: impl Into<String>, nakama_token: impl Into<String>) -> Self {
        Self {
            operator_token: operator_token.into(),
            nakama_token: nakama_token.into(),
            trnm_token: "hepta-test-trnm-token".to_string(),
            nakama_authorization_issuer_key_id: "hepta-test-issuer-key-v1".to_string(),
            nakama_authorization_signing_key: SigningKey::from_bytes(&[0x5a; 32]),
            consumer_edge_issuer: "hepta-test-consumer-edge".to_string(),
            consumer_edge_audience: "hepta-paper-raid-v2".to_string(),
            consumer_edge_issuer_key_id: "hepta-test-consumer-edge-key-v2".to_string(),
            consumer_edge_verifying_key: SigningKey::from_bytes(&[0x6c; 32]).verifying_key(),
            trusted_nakama_research_authorities: HashMap::new(),
            finality_mode: FinalityMode::PendingOnly,
            trusted_trnm_validator_sets: Vec::new(),
        }
    }

    pub fn with_nakama_authorization_signer(
        mut self,
        issuer_key_id: impl Into<String>,
        seed: [u8; 32],
    ) -> Result<Self, String> {
        let issuer_key_id = issuer_key_id.into();
        validate_contract_text("issuer_key_id", &issuer_key_id)?;
        self.nakama_authorization_issuer_key_id = issuer_key_id;
        self.nakama_authorization_signing_key = SigningKey::from_bytes(&seed);
        Ok(self)
    }

    pub fn with_trnm_token(mut self, trnm_token: impl Into<String>) -> Self {
        self.trnm_token = trnm_token.into();
        self
    }

    pub fn with_consumer_edge_trust(
        mut self,
        issuer: impl Into<String>,
        audience: impl Into<String>,
        issuer_key_id: impl Into<String>,
        public_key: [u8; 32],
    ) -> Result<Self, String> {
        let issuer = issuer.into();
        let audience = audience.into();
        let issuer_key_id = issuer_key_id.into();
        validate_contract_text("consumer_edge_issuer", &issuer)?;
        validate_contract_text("consumer_edge_audience", &audience)?;
        validate_contract_text("consumer_edge_issuer_key_id", &issuer_key_id)?;
        self.consumer_edge_issuer = issuer;
        self.consumer_edge_audience = audience;
        self.consumer_edge_issuer_key_id = issuer_key_id;
        self.consumer_edge_verifying_key = VerifyingKey::from_bytes(&public_key)
            .map_err(|_| "consumer Edge public key is not valid Ed25519".to_string())?;
        Ok(self)
    }

    pub fn with_finality_mode(mut self, mode: FinalityMode) -> Self {
        self.finality_mode = mode;
        self
    }

    pub fn with_trusted_nakama_research_authority(
        mut self,
        authority_key_id: impl Into<String>,
        public_key: [u8; 32],
    ) -> Result<Self, String> {
        let authority_key_id = authority_key_id.into();
        validate_contract_text("nakama_research_authority_key_id", &authority_key_id)?;
        if self
            .trusted_nakama_research_authorities
            .contains_key(&authority_key_id)
        {
            return Err("duplicate trusted Nakama research authority key ID".to_string());
        }
        let verifying_key = VerifyingKey::from_bytes(&public_key)
            .map_err(|_| "Nakama research authority public key is not valid Ed25519".to_string())?;
        self.trusted_nakama_research_authorities
            .insert(authority_key_id, verifying_key);
        Ok(self)
    }

    pub fn verify_nakama_research_completion(
        &self,
        completion: &paper_raid_contracts::ResearchSessionCompletionV1,
        archive: &[paper_raid_contracts::ResearchSessionEventV1],
    ) -> Result<(), String> {
        let verifying_key = self
            .trusted_nakama_research_authorities
            .get(&completion.authority_key_id)
            .ok_or_else(|| {
                format!(
                    "untrusted Nakama research authority key {}",
                    completion.authority_key_id
                )
            })?;
        paper_raid_contracts::verify_research_session_completion_against_archive(
            completion,
            archive,
            verifying_key,
        )
    }

    pub fn with_trusted_trnm_validator_set(
        mut self,
        validator_set: trnm_v1::TrustedValidatorSetV1,
    ) -> Result<Self, String> {
        validator_set.validate()?;
        if self.trusted_trnm_validator_sets.iter().any(|existing| {
            existing.chain_id == validator_set.chain_id
                && existing.validator_set_id == validator_set.validator_set_id
        }) {
            return Err("duplicate trusted TRNM validator set".to_string());
        }
        self.trusted_trnm_validator_sets.push(validator_set);
        self.finality_mode = FinalityMode::Verified;
        Ok(self)
    }

    pub fn from_env() -> Result<Self, String> {
        let operator_token = std::env::var("HEPTA_OPERATOR_TOKEN")
            .map_err(|_| "HEPTA_OPERATOR_TOKEN must be set".to_string())?;
        let nakama_token = std::env::var("HEPTA_NAKAMA_TOKEN")
            .map_err(|_| "HEPTA_NAKAMA_TOKEN must be set".to_string())?;
        let trnm_token = std::env::var("HEPTA_TRNM_TOKEN")
            .map_err(|_| "HEPTA_TRNM_TOKEN must be set".to_string())?;
        let nakama_authorization_issuer_key_id =
            std::env::var("HEPTA_NAKAMA_AUTHORIZATION_ISSUER_KEY_ID")
                .map_err(|_| "HEPTA_NAKAMA_AUTHORIZATION_ISSUER_KEY_ID must be set".to_string())?;
        validate_contract_text(
            "HEPTA_NAKAMA_AUTHORIZATION_ISSUER_KEY_ID",
            &nakama_authorization_issuer_key_id,
        )?;
        let nakama_authorization_seed_base64 =
            std::env::var("HEPTA_NAKAMA_AUTHORIZATION_ED25519_SEED_BASE64").map_err(|_| {
                "HEPTA_NAKAMA_AUTHORIZATION_ED25519_SEED_BASE64 must be set".to_string()
            })?;
        let nakama_authorization_seed =
            BASE64
                .decode(&nakama_authorization_seed_base64)
                .map_err(|_| {
                    "HEPTA_NAKAMA_AUTHORIZATION_ED25519_SEED_BASE64 must be canonical base64"
                        .to_string()
                })?;
        if BASE64.encode(&nakama_authorization_seed) != nakama_authorization_seed_base64 {
            return Err(
                "HEPTA_NAKAMA_AUTHORIZATION_ED25519_SEED_BASE64 must be canonical padded base64"
                    .to_string(),
            );
        }
        let nakama_authorization_seed: [u8; 32] =
            nakama_authorization_seed.try_into().map_err(|_| {
                "HEPTA_NAKAMA_AUTHORIZATION_ED25519_SEED_BASE64 must decode to exactly 32 bytes"
                    .to_string()
            })?;
        if operator_token.trim().is_empty()
            || nakama_token.trim().is_empty()
            || trnm_token.trim().is_empty()
        {
            return Err("Hepta service tokens must not be empty".to_string());
        }
        if operator_token == nakama_token
            || operator_token == trnm_token
            || nakama_token == trnm_token
        {
            return Err("operator, Nakama, and TRNM tokens must be different".to_string());
        }
        let consumer_edge_issuer = std::env::var("HEPTA_CONSUMER_EDGE_ISSUER")
            .map_err(|_| "HEPTA_CONSUMER_EDGE_ISSUER must be set".to_string())?;
        let consumer_edge_audience = std::env::var("HEPTA_CONSUMER_EDGE_AUDIENCE")
            .map_err(|_| "HEPTA_CONSUMER_EDGE_AUDIENCE must be set".to_string())?;
        let consumer_edge_issuer_key_id = std::env::var("HEPTA_CONSUMER_EDGE_ISSUER_KEY_ID")
            .map_err(|_| "HEPTA_CONSUMER_EDGE_ISSUER_KEY_ID must be set".to_string())?;
        let consumer_edge_public_key_base64 =
            std::env::var("HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEY_BASE64").map_err(|_| {
                "HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEY_BASE64 must be set".to_string()
            })?;
        let consumer_edge_public_key =
            BASE64
                .decode(&consumer_edge_public_key_base64)
                .map_err(|_| {
                    "HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEY_BASE64 must be canonical base64"
                        .to_string()
                })?;
        if BASE64.encode(&consumer_edge_public_key) != consumer_edge_public_key_base64 {
            return Err(
                "HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEY_BASE64 must be canonical padded base64"
                    .to_string(),
            );
        }
        let consumer_edge_public_key: [u8; 32] =
            consumer_edge_public_key.try_into().map_err(|_| {
                "HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEY_BASE64 must decode to 32 bytes".to_string()
            })?;
        let nakama_authority_key_id = std::env::var("TRNM_NAKAMA_AUTHORITY_KEY_ID")
            .map_err(|_| "TRNM_NAKAMA_AUTHORITY_KEY_ID must be set".to_string())?;
        validate_contract_text("TRNM_NAKAMA_AUTHORITY_KEY_ID", &nakama_authority_key_id)?;
        let nakama_authority_public_key_base64 =
            std::env::var("TRNM_NAKAMA_AUTHORITY_PUBLIC_KEY_BASE64")
                .map_err(|_| "TRNM_NAKAMA_AUTHORITY_PUBLIC_KEY_BASE64 must be set".to_string())?;
        let nakama_authority_public_key = BASE64
            .decode(&nakama_authority_public_key_base64)
            .map_err(|_| {
                "TRNM_NAKAMA_AUTHORITY_PUBLIC_KEY_BASE64 must be canonical base64".to_string()
            })?;
        if BASE64.encode(&nakama_authority_public_key) != nakama_authority_public_key_base64 {
            return Err(
                "TRNM_NAKAMA_AUTHORITY_PUBLIC_KEY_BASE64 must be canonical padded base64"
                    .to_string(),
            );
        }
        let nakama_authority_public_key: [u8; 32] =
            nakama_authority_public_key.try_into().map_err(|_| {
                "TRNM_NAKAMA_AUTHORITY_PUBLIC_KEY_BASE64 must decode to 32 bytes".to_string()
            })?;
        let finality_mode = match std::env::var("HEPTA_FINALITY_MODE")
            .map_err(|_| "HEPTA_FINALITY_MODE must be pending_only or verified".to_string())?
            .as_str()
        {
            "pending_only" => FinalityMode::PendingOnly,
            "verified" => FinalityMode::Verified,
            _ => return Err("HEPTA_FINALITY_MODE must be pending_only or verified".to_string()),
        };
        let trusted_sets_json =
            std::env::var("HEPTA_TRNM_VALIDATOR_SETS_JSON").unwrap_or_else(|_| "[]".to_string());
        let trusted_sets: Vec<trnm_v1::TrustedValidatorSetV1> =
            serde_json::from_str(&trusted_sets_json)
                .map_err(|error| format!("decode HEPTA_TRNM_VALIDATOR_SETS_JSON: {error}"))?;
        if finality_mode == FinalityMode::Verified && trusted_sets.is_empty() {
            return Err(
                "HEPTA_TRNM_VALIDATOR_SETS_JSON must contain at least one validator set"
                    .to_string(),
            );
        }
        let mut security = Self::new(operator_token, nakama_token)
            .with_trnm_token(trnm_token)
            .with_consumer_edge_trust(
                consumer_edge_issuer,
                consumer_edge_audience,
                consumer_edge_issuer_key_id,
                consumer_edge_public_key,
            )?
            .with_nakama_authorization_signer(
                nakama_authorization_issuer_key_id,
                nakama_authorization_seed,
            )?
            .with_trusted_nakama_research_authority(
                nakama_authority_key_id,
                nakama_authority_public_key,
            )?;
        security.finality_mode = finality_mode;
        for validator_set in trusted_sets {
            validator_set.validate()?;
            if security.trusted_trnm_validator_sets.iter().any(|existing| {
                existing.chain_id == validator_set.chain_id
                    && existing.validator_set_id == validator_set.validator_set_id
            }) {
                return Err("duplicate trusted TRNM validator set".to_string());
            }
            security.trusted_trnm_validator_sets.push(validator_set);
        }
        Ok(security)
    }

    fn readiness_errors(&self) -> Vec<&'static str> {
        let mut errors = Vec::new();
        if self.operator_token.trim().is_empty()
            || self.nakama_token.trim().is_empty()
            || self.trnm_token.trim().is_empty()
            || self.operator_token == self.nakama_token
            || self.operator_token == self.trnm_token
            || self.nakama_token == self.trnm_token
        {
            errors.push("service_token_configuration_invalid");
        }
        if self.nakama_authorization_issuer_key_id.trim().is_empty()
            || self.consumer_edge_issuer.trim().is_empty()
            || self.consumer_edge_audience.trim().is_empty()
            || self.consumer_edge_issuer_key_id.trim().is_empty()
        {
            errors.push("issuer_configuration_invalid");
        }
        if self.trusted_nakama_research_authorities.is_empty() {
            errors.push("nakama_research_authority_missing");
        }
        if self.finality_mode == FinalityMode::Verified {
            if self.trusted_trnm_validator_sets.is_empty() {
                errors.push("trnm_validator_set_missing");
            } else if self
                .trusted_trnm_validator_sets
                .iter()
                .any(|validator_set| validator_set.validate().is_err())
            {
                errors.push("trnm_validator_set_invalid");
            }
        }
        errors
    }
}

impl AppState {
    pub fn new(security: SecurityConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(LeagueState::default())),
            paper_raid: Arc::new(RwLock::new(paper_raid_v2::PaperRaidMemory::default())),
            pool: None,
            security: Arc::new(security),
            rate_limits: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn connect(database_url: &str, security: SecurityConfig) -> Result<Self, String> {
        let pool = PgPool::connect(database_url)
            .await
            .map_err(|error| format!("connect Hepta PostgreSQL: {error}"))?;
        sqlx::raw_sql(include_str!(
            "../../../migrations/0031_add_hepta_research_league.sql"
        ))
        .execute(&pool)
        .await
        .map_err(|error| format!("apply Hepta migration: {error}"))?;
        sqlx::raw_sql(include_str!(
            "../../../migrations/0032_add_hepta_paper_raid_v2.sql"
        ))
        .execute(&pool)
        .await
        .map_err(|error| format!("apply Hepta Paper Raid migration: {error}"))?;
        sqlx::raw_sql(include_str!(
            "../../../migrations/0033_add_hepta_paper_collaboration_kernel.sql"
        ))
        .execute(&pool)
        .await
        .map_err(|error| format!("apply Hepta collaboration migration: {error}"))?;
        sqlx::raw_sql(include_str!(
            "../../../migrations/0034_add_hepta_paper_review_appeal.sql"
        ))
        .execute(&pool)
        .await
        .map_err(|error| format!("apply Hepta paper review migration: {error}"))?;
        sqlx::query(
            "insert into hepta_league_state (state_key, revision, state_json)
             values ('primary', 0, $1::jsonb)
             on conflict (state_key) do nothing",
        )
        .bind(
            serde_json::to_value(LeagueState::default())
                .map_err(|error| format!("serialize initial Hepta state: {error}"))?,
        )
        .execute(&pool)
        .await
        .map_err(|error| format!("initialize Hepta state: {error}"))?;
        Ok(Self {
            inner: Arc::new(RwLock::new(LeagueState::default())),
            paper_raid: Arc::new(RwLock::new(paper_raid_v2::PaperRaidMemory::default())),
            pool: Some(pool),
            security: Arc::new(security),
            rate_limits: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    async fn inspect<R>(
        &self,
        operation: impl FnOnce(&LeagueState) -> Result<R, ApiError>,
    ) -> Result<R, ApiError> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query(
                "select state_json from hepta_league_state where state_key = 'primary'",
            )
            .fetch_one(pool)
            .await
            .map_err(ApiError::database)?;
            let state: LeagueState = serde_json::from_value(row.get("state_json"))
                .map_err(|error| ApiError::internal(format!("decode Hepta state: {error}")))?;
            operation(&state)
        } else {
            let state = self.inner.read().await;
            operation(&state)
        }
    }

    async fn transact<R>(
        &self,
        operation: impl FnOnce(&mut LeagueState) -> Result<R, ApiError>,
    ) -> Result<R, ApiError> {
        let Some(pool) = &self.pool else {
            let mut state = self.inner.write().await;
            return operation(&mut state);
        };
        let mut tx = pool.begin().await.map_err(ApiError::database)?;
        sqlx::query("select pg_advisory_xact_lock(hashtext('hepta-research-league-state-v1'))")
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
        let row = sqlx::query(
            "select revision, state_json from hepta_league_state
             where state_key = 'primary' for update",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        let revision: i64 = row.get("revision");
        let mut state: LeagueState = serde_json::from_value(row.get("state_json"))
            .map_err(|error| ApiError::internal(format!("decode Hepta state: {error}")))?;
        let event_offset = state.events.len();
        let inbox_before = state.inbox_events.keys().cloned().collect::<HashSet<_>>();
        let output = operation(&mut state)?;
        let state_json = serde_json::to_value(&state)
            .map_err(|error| ApiError::internal(format!("encode Hepta state: {error}")))?;
        sqlx::query(
            "update hepta_league_state
             set revision = $1, state_json = $2::jsonb, updated_at = now()
             where state_key = 'primary' and revision = $3",
        )
        .bind(revision + 1)
        .bind(state_json)
        .bind(revision)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        for event in &state.events[event_offset..] {
            sqlx::query(
                "insert into hepta_outbox (
                    event_id, event_type, aggregate_id, aggregate_version,
                    correlation_id, causation_id, idempotency_key, schema_version,
                    producer, payload_hash, payload, occurred_at
                 ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11::jsonb,$12)
                 on conflict (event_id) do nothing",
            )
            .bind(event.event_id)
            .bind(&event.event_type)
            .bind(&event.aggregate_id)
            .bind(event.aggregate_version as i64)
            .bind(event.correlation_id)
            .bind(event.causation_id)
            .bind(&event.idempotency_key)
            .bind(&event.schema_version)
            .bind(&event.producer)
            .bind(&event.payload_hash)
            .bind(&event.payload)
            .bind(event.occurred_at)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
        }
        for (inbox_key, payload_hash) in state
            .inbox_events
            .iter()
            .filter(|(key, _)| !inbox_before.contains(*key))
        {
            let (consumer, event_id) = inbox_key
                .rsplit_once(':')
                .and_then(|(consumer, event_id)| {
                    Uuid::parse_str(event_id)
                        .ok()
                        .map(|event_id| (consumer, event_id))
                })
                .ok_or_else(|| ApiError::internal("invalid durable inbox key"))?;
            sqlx::query(
                "insert into hepta_inbox (consumer, event_id, event_type, payload_hash)
                 values ($1, $2, $3, $4)
                 on conflict (consumer, event_id) do nothing",
            )
            .bind(consumer)
            .bind(event_id)
            .bind("cross_module_event_v1")
            .bind(payload_hash)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
        }
        tx.commit().await.map_err(ApiError::database)?;
        Ok(output)
    }

    pub fn is_durable(&self) -> bool {
        self.pool.is_some()
    }

    async fn enforce_rate_limit(
        &self,
        bucket: &str,
        subject: &str,
        max_requests: u32,
        window_seconds: i64,
    ) -> Result<(), ApiError> {
        let now = Utc::now();
        let mut limits = self.rate_limits.lock().await;
        let window = limits
            .entry((bucket.to_string(), subject.to_string()))
            .or_insert(RateWindow {
                started_at: now,
                requests: 0,
            });
        if now - window.started_at >= chrono::Duration::seconds(window_seconds) {
            window.started_at = now;
            window.requests = 0;
        }
        if window.requests >= max_requests {
            return Err(ApiError::too_many_requests(format!(
                "{bucket} rate limit exceeded"
            )));
        }
        window.requests += 1;
        Ok(())
    }

    pub async fn claim_outbox(
        &self,
        worker_id: &str,
        limit: i64,
        lease_seconds: i64,
    ) -> Result<Vec<EventEnvelope>, String> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| "outbox leasing requires PostgreSQL".to_string())?;
        let rows = sqlx::query(
            "with claimed as (
                select event_id from hepta_outbox
                where delivered_at is null
                  and available_at <= now()
                  and (lease_expires_at is null or lease_expires_at < now())
                order by occurred_at, event_id
                for update skip locked
                limit $1
             )
             update hepta_outbox o
             set lease_owner = $2,
                 lease_expires_at = now() + make_interval(secs => $3::int),
                 attempt_count = attempt_count + 1
             from claimed
             where o.event_id = claimed.event_id
             returning o.schema_version, o.event_id, o.event_type, o.aggregate_id,
                       o.aggregate_version, o.correlation_id, o.causation_id,
                       o.idempotency_key, o.occurred_at, o.producer,
                       o.payload_hash, o.payload",
        )
        .bind(limit.clamp(1, 500))
        .bind(worker_id)
        .bind(lease_seconds.clamp(5, 300) as i32)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("claim Hepta outbox: {error}"))?;
        Ok(rows
            .into_iter()
            .map(|row| EventEnvelope {
                schema_version: row.get("schema_version"),
                event_id: row.get("event_id"),
                event_type: row.get("event_type"),
                aggregate_id: row.get("aggregate_id"),
                aggregate_version: row.get::<i64, _>("aggregate_version") as u64,
                correlation_id: row.get("correlation_id"),
                causation_id: row.get("causation_id"),
                idempotency_key: row.get("idempotency_key"),
                occurred_at: row.get("occurred_at"),
                producer: row.get("producer"),
                payload_hash: row.get("payload_hash"),
                payload: row.get("payload"),
            })
            .collect())
    }

    pub async fn acknowledge_outbox(
        &self,
        worker_id: &str,
        event_id: Uuid,
    ) -> Result<bool, String> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| "outbox acknowledgement requires PostgreSQL".to_string())?;
        let result = sqlx::query(
            "update hepta_outbox
             set delivered_at = now(), lease_owner = null, lease_expires_at = null,
                 last_error = null
             where event_id = $1 and delivered_at is null and lease_owner = $2",
        )
        .bind(event_id)
        .bind(worker_id)
        .execute(pool)
        .await
        .map_err(|error| format!("acknowledge Hepta outbox: {error}"))?;
        Ok(result.rows_affected() == 1)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(SecurityConfig::new(
            "hepta-test-operator-token",
            "hepta-test-nakama-token",
        ))
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct LeagueState {
    agents: HashMap<String, AgentRegistration>,
    challenges: HashMap<Uuid, ResearchChallenge>,
    enrollments: HashMap<String, Enrollment>,
    match_authorizations: HashMap<String, MatchAuthorizationRecord>,
    submissions: HashMap<Uuid, SubmissionRecord>,
    used_agent_nonces: HashSet<(String, String)>,
    events: Vec<EventEnvelope>,
    #[serde(default)]
    evaluator_manifests: HashMap<Uuid, workflows::EvaluatorManifest>,
    #[serde(default)]
    evaluation_reports: HashMap<Uuid, workflows::EvaluationReport>,
    #[serde(default)]
    reproduction_reports: HashMap<Uuid, workflows::ReproductionReport>,
    #[serde(default)]
    appeal_cases: HashMap<Uuid, workflows::AppealCase>,
    #[serde(default)]
    trnm_commands: HashMap<Uuid, workflows::TrnmCommand>,
    #[serde(default)]
    trnm_finality: HashMap<Uuid, workflows::TrnmFinalityProjection>,
    #[serde(default)]
    trnm_live_finality: HashMap<Uuid, workflows::LiveTrnmFinalityProjection>,
    #[serde(default)]
    nakama_matches: HashMap<Uuid, workflows::NakamaMatchProjection>,
    #[serde(default)]
    inbox_events: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRegistration {
    pub agent_id: String,
    pub owner_id: String,
    pub organization_id: Option<String>,
    pub protocol_version: String,
    pub public_key: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterAgentRequest {
    pub agent_id: String,
    pub owner_id: String,
    pub organization_id: Option<String>,
    pub protocol_version: String,
    pub public_key: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RotateAgentKeyRequest {
    pub agent_id: String,
    pub new_public_key: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Serialize)]
pub struct RotateAgentKeyResponse {
    pub agent_id: String,
    pub public_key: String,
    pub rotated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeStatus {
    Draft,
    Open,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchChallenge {
    pub challenge_id: Uuid,
    pub title: String,
    pub description: String,
    pub ruleset_version: String,
    pub ruleset_hash: String,
    pub dataset_manifest_hash: String,
    pub evaluator_manifest_hash: String,
    pub status: ChallengeStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateChallengeRequest {
    pub title: String,
    pub description: String,
    pub ruleset_version: String,
    pub ruleset_hash: String,
    pub dataset_manifest_hash: String,
    pub evaluator_manifest_hash: String,
    pub status: ChallengeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Enrollment {
    pub enrollment_id: Uuid,
    pub challenge_id: Uuid,
    pub agent_id: String,
    pub enrolled_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct EnrollAgentRequest {
    pub agent_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizeMatchRequest {
    pub match_id: Option<Uuid>,
    pub challenge_id: Uuid,
    pub agent_id: String,
    pub subject_user_id: String,
    pub participant_slot: u16,
    pub role: String,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NakamaAuthorizationClaimV1 {
    pub schema: String,
    pub authorization_id: String,
    pub match_id: String,
    pub challenge_id: String,
    pub agent_id: String,
    pub agent_did: String,
    pub agent_key_id: String,
    pub agent_public_key: String,
    pub subject_user_id: String,
    pub participant_slot: u32,
    pub role: String,
    pub ruleset_hash: String,
    pub dataset_hash: String,
    pub challenge_snapshot_hash: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedNakamaMatchAuthorizationV1 {
    pub claim: NakamaAuthorizationClaimV1,
    pub issuer_key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MatchAuthorizationRecord {
    signed_authorization: SignedNakamaMatchAuthorizationV1,
    consumed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcknowledgeMatchAuthorizationConsumedRequest {
    pub authorization_id: Uuid,
    pub match_id: Uuid,
    pub agent_id: String,
}

#[derive(Debug, Serialize)]
pub struct MatchAuthorizationClaims {
    pub authorization_id: Uuid,
    pub match_id: Uuid,
    pub challenge_id: Uuid,
    pub agent_id: String,
    pub subject_user_id: String,
    pub participant_slot: u32,
    pub role: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
    pub consumed_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitArtifactRequest {
    pub protocol_version: String,
    pub submission_id: Uuid,
    pub challenge_id: Uuid,
    pub match_id: Uuid,
    pub agent_id: String,
    pub artifact_hash: String,
    pub evidence_manifest_hash: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubmissionRecord {
    pub submission_id: Uuid,
    pub challenge_id: Uuid,
    pub match_id: Uuid,
    pub agent_id: String,
    pub artifact_hash: String,
    pub evidence_manifest_hash: String,
    pub nonce: String,
    pub signature: String,
    pub accepted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventEnvelope {
    pub schema_version: String,
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_id: String,
    pub aggregate_version: u64,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub idempotency_key: String,
    pub occurred_at: DateTime<Utc>,
    pub producer: String,
    pub payload_hash: String,
    pub payload: Value,
}

#[derive(Debug, Serialize)]
struct ManifestResponse {
    service: &'static str,
    contract_version: &'static str,
    event_schema_version: &'static str,
    evaluator_schema_version: &'static str,
    nakama_contract_version: &'static str,
    trnm_adapter_version: &'static str,
    agent_execution_mode: &'static str,
    top_level_modules: [&'static str; 3],
    capabilities: [&'static str; 10],
    finality_mode: &'static str,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }

    fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code,
            message: message.into(),
        }
    }

    fn forbidden(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code,
            message: message.into(),
        }
    }

    fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: message.into(),
        }
    }

    fn too_many_requests(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "rate_limit_exceeded",
            message: message.into(),
        }
    }

    fn database(error: sqlx::Error) -> Self {
        Self::internal(format!("Hepta persistence failure: {error}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                code: self.code,
                message: self.message,
            }),
        )
            .into_response()
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/hepta/manifest", get(manifest))
        .route("/v1/hepta/agents", post(register_agent))
        .route("/v1/hepta/agents/rotate-key", post(rotate_agent_key))
        .route(
            "/v1/hepta/challenges",
            get(list_challenges).post(create_challenge),
        )
        .route("/v1/hepta/challenges/:challenge_id", get(get_challenge))
        .route(
            "/v1/hepta/challenges/:challenge_id/enrollments",
            post(enroll_agent),
        )
        .route("/v1/hepta/match-authorizations", post(authorize_match))
        .route(
            "/v1/hepta/nakama/match-authorizations/consumed",
            post(acknowledge_match_authorization_consumed),
        )
        .route("/v1/hepta/submissions", post(submit_artifact))
        .route("/v1/hepta/events", get(list_events))
        .merge(paper_raid_v2::router())
        .merge(workflows::router())
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "hepta-research-league",
    })
}

async fn manifest(State(state): State<AppState>) -> Json<ManifestResponse> {
    Json(ManifestResponse {
        service: "hepta-research-league",
        contract_version: AGENT_PROTOCOL_V1,
        event_schema_version: EVENT_SCHEMA_V1,
        evaluator_schema_version: workflows::EVALUATOR_MANIFEST_V1,
        nakama_contract_version: workflows::NAKAMA_EVENT_V2,
        trnm_adapter_version: workflows::TRNM_ADAPTER_V2,
        agent_execution_mode: "external_only",
        top_level_modules: ["hepta", "nakama", "trnm"],
        capabilities: [
            "agent_registry",
            "research_challenges",
            "challenge_enrollment",
            "nakama_match_authorization",
            "signed_artifact_submission",
            "deterministic_evaluation",
            "reproduction_and_appeals",
            "trnm_finality_projection",
            "nakama_event_root_reconciliation",
            "transactional_outbox_inbox",
        ],
        finality_mode: state.security.finality_mode.as_str(),
    })
}

async fn register_agent(
    State(state): State<AppState>,
    Json(request): Json<RegisterAgentRequest>,
) -> Result<(StatusCode, Json<AgentRegistration>), ApiError> {
    validate_non_empty("agent_id", &request.agent_id)?;
    validate_non_empty("owner_id", &request.owner_id)?;
    validate_protocol(&request.protocol_version)?;
    decode_verifying_key(&request.public_key)?;
    state
        .enforce_rate_limit("agent_registration", &request.owner_id, 20, 60)
        .await?;

    state
        .transact(|league| {
            if let Some(existing) = league.agents.get(&request.agent_id) {
                let same_registration = existing.owner_id == request.owner_id
                    && existing.organization_id == request.organization_id
                    && existing.protocol_version == request.protocol_version
                    && existing.public_key == request.public_key
                    && existing.capabilities == request.capabilities;
                if same_registration {
                    return Ok((StatusCode::OK, Json(existing.clone())));
                }
                return Err(ApiError::conflict(
                    "agent_registration_conflict",
                    format!(
                        "agent {} already exists with different data",
                        request.agent_id
                    ),
                ));
            }

            let registration = AgentRegistration {
                agent_id: request.agent_id,
                owner_id: request.owner_id,
                organization_id: request.organization_id,
                protocol_version: request.protocol_version,
                public_key: request.public_key,
                capabilities: request.capabilities,
                registered_at: Utc::now(),
            };
            league
                .agents
                .insert(registration.agent_id.clone(), registration.clone());
            push_event(
                league,
                "hepta.agent.registered.v1",
                registration.agent_id.clone(),
                json!({
                    "agent_id": registration.agent_id,
                    "owner_id": registration.owner_id,
                    "protocol_version": registration.protocol_version,
                }),
            );

            Ok((StatusCode::CREATED, Json(registration)))
        })
        .await
}

async fn rotate_agent_key(
    State(state): State<AppState>,
    Json(request): Json<RotateAgentKeyRequest>,
) -> Result<Json<RotateAgentKeyResponse>, ApiError> {
    validate_non_empty("agent_id", &request.agent_id)?;
    validate_non_empty("nonce", &request.nonce)?;
    state
        .enforce_rate_limit("artifact_submission", &request.agent_id, 120, 60)
        .await?;
    decode_verifying_key(&request.new_public_key)?;

    state
        .transact(|league| {
            if league
                .used_agent_nonces
                .contains(&(request.agent_id.clone(), request.nonce.clone()))
            {
                return Err(ApiError::conflict(
                    "agent_nonce_reused",
                    "Agent nonce has already been accepted",
                ));
            }
            let current_key = league
                .agents
                .get(&request.agent_id)
                .map(|agent| agent.public_key.clone())
                .ok_or_else(|| {
                    ApiError::not_found(
                        "agent_not_found",
                        format!("agent {} does not exist", request.agent_id),
                    )
                })?;
            let signing_message = key_rotation_signing_message(&request);
            verify_signature(&current_key, signing_message.as_bytes(), &request.signature)?;

            let rotated_at = Utc::now();
            league
                .agents
                .get_mut(&request.agent_id)
                .expect("Agent exists after lookup")
                .public_key = request.new_public_key.clone();
            league
                .used_agent_nonces
                .insert((request.agent_id.clone(), request.nonce));
            push_event(
                league,
                "hepta.agent.key_rotated.v1",
                request.agent_id.clone(),
                json!({
                    "agent_id": request.agent_id,
                    "new_public_key_hash": format!("sha256:{}", sha256_hex(request.new_public_key.as_bytes())),
                    "rotated_at": rotated_at,
                }),
            );
            Ok(Json(RotateAgentKeyResponse {
                agent_id: request.agent_id,
                public_key: request.new_public_key,
                rotated_at,
            }))
        })
        .await
}

async fn create_challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateChallengeRequest>,
) -> Result<(StatusCode, Json<ResearchChallenge>), ApiError> {
    require_service_token(
        &headers,
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    validate_non_empty("title", &request.title)?;
    validate_non_empty("ruleset_version", &request.ruleset_version)?;
    validate_hash("ruleset_hash", &request.ruleset_hash)?;
    validate_hash("dataset_manifest_hash", &request.dataset_manifest_hash)?;
    validate_hash("evaluator_manifest_hash", &request.evaluator_manifest_hash)?;

    let challenge = ResearchChallenge {
        challenge_id: Uuid::new_v4(),
        title: request.title,
        description: request.description,
        ruleset_version: request.ruleset_version,
        ruleset_hash: request.ruleset_hash,
        dataset_manifest_hash: request.dataset_manifest_hash,
        evaluator_manifest_hash: request.evaluator_manifest_hash,
        status: request.status,
        created_at: Utc::now(),
    };
    state
        .transact(|league| {
            league
                .challenges
                .insert(challenge.challenge_id, challenge.clone());
            push_event(
                league,
                "hepta.challenge.created.v1",
                challenge.challenge_id.to_string(),
                json!({
                    "challenge_id": challenge.challenge_id,
                    "ruleset_version": challenge.ruleset_version,
                    "ruleset_hash": challenge.ruleset_hash,
                    "status": challenge.status,
                }),
            );
            Ok((StatusCode::CREATED, Json(challenge)))
        })
        .await
}

async fn list_challenges(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ResearchChallenge>>, ApiError> {
    require_service_token(
        &headers,
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    state
        .inspect(|league| {
            let mut challenges = league.challenges.values().cloned().collect::<Vec<_>>();
            challenges.sort_by_key(|challenge| (challenge.created_at, challenge.challenge_id));
            Ok(Json(challenges))
        })
        .await
}

async fn get_challenge(
    State(state): State<AppState>,
    Path(challenge_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ResearchChallenge>, ApiError> {
    require_service_token(
        &headers,
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    state
        .inspect(|league| {
            league
                .challenges
                .get(&challenge_id)
                .cloned()
                .map(Json)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "challenge_not_found",
                        format!("challenge {challenge_id} does not exist"),
                    )
                })
        })
        .await
}

async fn enroll_agent(
    State(state): State<AppState>,
    Path(challenge_id): Path<Uuid>,
    Json(request): Json<EnrollAgentRequest>,
) -> Result<(StatusCode, Json<Enrollment>), ApiError> {
    state
        .transact(|league| {
            let challenge = league.challenges.get(&challenge_id).ok_or_else(|| {
                ApiError::not_found(
                    "challenge_not_found",
                    format!("challenge {challenge_id} does not exist"),
                )
            })?;
            if challenge.status != ChallengeStatus::Open {
                return Err(ApiError::conflict(
                    "challenge_not_open",
                    format!("challenge {challenge_id} is not open"),
                ));
            }
            if !league.agents.contains_key(&request.agent_id) {
                return Err(ApiError::not_found(
                    "agent_not_found",
                    format!("agent {} does not exist", request.agent_id),
                ));
            }
            if let Some(existing) = league
                .enrollments
                .get(&enrollment_key(challenge_id, &request.agent_id))
            {
                return Ok((StatusCode::OK, Json(existing.clone())));
            }

            let enrollment = Enrollment {
                enrollment_id: Uuid::new_v4(),
                challenge_id,
                agent_id: request.agent_id,
                enrolled_at: Utc::now(),
            };
            league.enrollments.insert(
                enrollment_key(challenge_id, &enrollment.agent_id),
                enrollment.clone(),
            );
            push_event(
                league,
                "hepta.challenge.agent_enrolled.v1",
                challenge_id.to_string(),
                json!({
                    "enrollment_id": enrollment.enrollment_id,
                    "challenge_id": enrollment.challenge_id,
                    "agent_id": enrollment.agent_id,
                }),
            );

            Ok((StatusCode::CREATED, Json(enrollment)))
        })
        .await
}

async fn authorize_match(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AuthorizeMatchRequest>,
) -> Result<(StatusCode, Json<SignedNakamaMatchAuthorizationV1>), ApiError> {
    require_service_token(
        &headers,
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    validate_contract_text_api("subject_user_id", &request.subject_user_id)?;
    validate_contract_text_api("role", &request.role)?;
    if request.participant_slot != 1 && request.participant_slot != 2 {
        return Err(ApiError::bad_request(
            "invalid_participant_slot",
            "participant_slot must be 1 or 2",
        ));
    }
    let ttl_seconds = request.ttl_seconds.unwrap_or(300).clamp(30, 900);
    let issuer_key_id = state.security.nakama_authorization_issuer_key_id.clone();
    let issuer_signing_key = state.security.nakama_authorization_signing_key.clone();
    state
        .transact(|league| {
            if !league
                .enrollments
                .contains_key(&enrollment_key(request.challenge_id, &request.agent_id))
            {
                return Err(ApiError::forbidden(
                    "agent_not_enrolled",
                    format!(
                        "agent {} is not enrolled in challenge {}",
                        request.agent_id, request.challenge_id
                    ),
                ));
            }

            let challenge = league
                .challenges
                .get(&request.challenge_id)
                .cloned()
                .ok_or_else(|| {
                    ApiError::not_found(
                        "challenge_not_found",
                        format!("challenge {} does not exist", request.challenge_id),
                    )
                })?;
            let registration = league
                .agents
                .get(&request.agent_id)
                .cloned()
                .ok_or_else(|| {
                    ApiError::not_found(
                        "agent_not_found",
                        format!("agent {} does not exist", request.agent_id),
                    )
                })?;
            let agent_public_key = canonical_agent_public_key(&registration.public_key)?;
            let agent_key_id = format!(
                "sha256:{}",
                sha256_hex(
                    &BASE64
                        .decode(&agent_public_key)
                        .expect("canonical Agent key was just encoded")
                )
            );
            let ruleset_hash = canonical_digest("ruleset_hash", &challenge.ruleset_hash)?;
            let dataset_hash =
                canonical_digest("dataset_manifest_hash", &challenge.dataset_manifest_hash)?;
            let challenge_snapshot_hash = challenge_snapshot_hash(&challenge)?;
            let match_id = request.match_id.unwrap_or_else(Uuid::new_v4);
            let match_key = match_agent_key(match_id, &request.agent_id);

            if let Some(existing) = league.match_authorizations.get(&match_key) {
                let claim = &existing.signed_authorization.claim;
                let exact_retry = claim.match_id == match_id.to_string()
                    && claim.challenge_id == request.challenge_id.to_string()
                    && claim.agent_id == request.agent_id
                    && claim.agent_did == request.agent_id
                    && claim.agent_key_id == agent_key_id
                    && claim.agent_public_key == agent_public_key
                    && claim.subject_user_id == request.subject_user_id
                    && claim.participant_slot == u32::from(request.participant_slot)
                    && claim.role == request.role
                    && claim.ruleset_hash == ruleset_hash
                    && claim.dataset_hash == dataset_hash
                    && claim.challenge_snapshot_hash == challenge_snapshot_hash
                    && claim.expires_at_unix - claim.issued_at_unix == ttl_seconds as i64;
                if exact_retry {
                    return Ok((
                        StatusCode::OK,
                        Json(existing.signed_authorization.clone()),
                    ));
                }
                return Err(ApiError::conflict(
                    "match_authorization_conflict",
                    "match id and Agent already identify a different immutable authorization",
                ));
            }

            if league.match_authorizations.values().any(|authorization| {
                let claim = &authorization.signed_authorization.claim;
                claim.match_id == match_id.to_string()
                    && (claim.challenge_id != request.challenge_id.to_string()
                        || claim.participant_slot == u32::from(request.participant_slot)
                        || claim.agent_id == request.agent_id)
            }) {
                return Err(ApiError::conflict(
                    "match_authorization_conflict",
                    "match id, challenge, participant slot, and Agent assignment must be consistent",
                ));
            }

            let issued_at_unix = Utc::now().timestamp();
            let claim = NakamaAuthorizationClaimV1 {
                schema: NAKAMA_AUTHORIZATION_SCHEMA_V1.to_string(),
                authorization_id: Uuid::new_v4().to_string(),
                match_id: match_id.to_string(),
                challenge_id: request.challenge_id.to_string(),
                agent_id: request.agent_id.clone(),
                agent_did: request.agent_id,
                agent_key_id,
                agent_public_key,
                subject_user_id: request.subject_user_id,
                participant_slot: u32::from(request.participant_slot),
                role: request.role,
                ruleset_hash,
                dataset_hash,
                challenge_snapshot_hash,
                issued_at_unix,
                expires_at_unix: issued_at_unix + ttl_seconds as i64,
            };
            let response = sign_nakama_match_authorization(
                claim,
                &issuer_key_id,
                &issuer_signing_key,
            )
            .map_err(|error| {
                ApiError::internal(format!("sign Nakama match authorization: {error}"))
            })?;
            let record = MatchAuthorizationRecord {
                signed_authorization: response.clone(),
                consumed_at: None,
            };
            league
                .match_authorizations
                .insert(match_key, record.clone());
            push_event(
                league,
                "hepta.match.authorized.v1",
                response.claim.match_id.clone(),
                json!({
                    "authorization_id": response.claim.authorization_id,
                    "match_id": response.claim.match_id,
                    "challenge_id": response.claim.challenge_id,
                    "agent_id": response.claim.agent_id,
                    "subject_user_id": response.claim.subject_user_id,
                    "participant_slot": response.claim.participant_slot,
                    "role": response.claim.role,
                    "issuer_key_id": response.issuer_key_id,
                    "signature_hash": format!("sha256:{}", sha256_hex(response.signature.as_bytes())),
                    "issued_at_unix": response.claim.issued_at_unix,
                    "expires_at_unix": response.claim.expires_at_unix,
                }),
            );

            Ok((StatusCode::CREATED, Json(response)))
        })
        .await
}

async fn acknowledge_match_authorization_consumed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AcknowledgeMatchAuthorizationConsumedRequest>,
) -> Result<Json<MatchAuthorizationClaims>, ApiError> {
    require_service_token(
        &headers,
        NAKAMA_TOKEN_HEADER,
        &state.security.nakama_token,
        "nakama_auth_failed",
    )?;
    state
        .transact(|league| {
            let now = Utc::now();
            let (claims, newly_consumed) = {
                let authorization = league
                    .match_authorizations
                    .get_mut(&match_agent_key(request.match_id, &request.agent_id))
                    .ok_or_else(|| {
                        ApiError::not_found(
                            "match_authorization_not_found",
                            format!("match {} is not authorized", request.match_id),
                        )
                    })?;
                let claim = authorization.signed_authorization.claim.clone();
                if claim.agent_id != request.agent_id
                    || claim.authorization_id != request.authorization_id.to_string()
                {
                    return Err(ApiError::forbidden(
                        "match_authorization_mismatch",
                        "match authorization identity does not match the Nakama consumption acknowledgement",
                    ));
                }
                let (consumed_at, newly_consumed) =
                    if let Some(consumed_at) = authorization.consumed_at {
                        (consumed_at, false)
                    } else {
                        if claim.expires_at_unix <= now.timestamp() {
                            return Err(ApiError::forbidden(
                                "match_authorization_expired",
                                format!(
                                    "match authorization {} expired",
                                    claim.authorization_id
                                ),
                            ));
                        }
                        authorization.consumed_at = Some(now);
                        (now, true)
                    };
                (MatchAuthorizationClaims {
                    authorization_id: request.authorization_id,
                    match_id: request.match_id,
                    challenge_id: Uuid::parse_str(&claim.challenge_id).map_err(|error| {
                        ApiError::internal(format!("stored authorization challenge_id: {error}"))
                    })?,
                    agent_id: claim.agent_id.clone(),
                    subject_user_id: claim.subject_user_id.clone(),
                    participant_slot: claim.participant_slot,
                    role: claim.role.clone(),
                    issued_at_unix: claim.issued_at_unix,
                    expires_at_unix: claim.expires_at_unix,
                    consumed_at,
                }, newly_consumed)
            };
            if newly_consumed {
                push_event(
                    league,
                    "hepta.match.authorization_consumed.v1",
                    claims.match_id.to_string(),
                    json!({
                        "authorization_id": claims.authorization_id,
                        "match_id": claims.match_id,
                        "challenge_id": claims.challenge_id,
                        "agent_id": claims.agent_id,
                        "subject_user_id": claims.subject_user_id,
                        "participant_slot": claims.participant_slot,
                        "role": claims.role,
                        "consumed_at": claims.consumed_at,
                    }),
                );
            }
            Ok(Json(claims))
        })
        .await
}

async fn submit_artifact(
    State(state): State<AppState>,
    Json(request): Json<SubmitArtifactRequest>,
) -> Result<(StatusCode, Json<SubmissionRecord>), ApiError> {
    validate_protocol(&request.protocol_version)?;
    validate_hash("artifact_hash", &request.artifact_hash)?;
    validate_hash("evidence_manifest_hash", &request.evidence_manifest_hash)?;
    validate_non_empty("nonce", &request.nonce)?;

    state
        .transact(|league| {
            if let Some(existing) = league.submissions.get(&request.submission_id) {
                let same_submission = existing.challenge_id == request.challenge_id
                    && existing.match_id == request.match_id
                    && existing.agent_id == request.agent_id
                    && existing.artifact_hash == request.artifact_hash
                    && existing.evidence_manifest_hash == request.evidence_manifest_hash
                    && existing.nonce == request.nonce
                    && existing.signature == request.signature;
                if same_submission {
                    return Ok((StatusCode::OK, Json(existing.clone())));
                }
                return Err(ApiError::conflict(
                    "submission_id_conflict",
                    format!(
                        "submission {} already exists with different data",
                        request.submission_id
                    ),
                ));
            }
            if league
                .used_agent_nonces
                .contains(&(request.agent_id.clone(), request.nonce.clone()))
            {
                return Err(ApiError::conflict(
                    "agent_nonce_reused",
                    "Agent nonce has already been accepted",
                ));
            }
            let registration = league.agents.get(&request.agent_id).ok_or_else(|| {
                ApiError::not_found(
                    "agent_not_found",
                    format!("agent {} does not exist", request.agent_id),
                )
            })?;
            let authorization = league
                .match_authorizations
                .get(&match_agent_key(request.match_id, &request.agent_id))
                .ok_or_else(|| {
                    ApiError::not_found(
                        "match_authorization_not_found",
                        format!("match {} is not authorized", request.match_id),
                    )
                })?;
            let authorization_claim = &authorization.signed_authorization.claim;
            if authorization_claim.challenge_id != request.challenge_id.to_string()
                || authorization_claim.agent_id != request.agent_id
            {
                return Err(ApiError::forbidden(
                    "match_authorization_mismatch",
                    "match authorization does not match the submitted challenge and agent",
                ));
            }
            if authorization_claim.expires_at_unix <= Utc::now().timestamp() {
                return Err(ApiError::forbidden(
                    "match_authorization_expired",
                    format!(
                        "match authorization {} expired",
                        authorization_claim.authorization_id
                    ),
                ));
            }
            if authorization.consumed_at.is_none() {
                return Err(ApiError::forbidden(
                    "match_authorization_not_consumed",
                    "Nakama must verify and consume the match authorization before submission",
                ));
            }

            let signed_message = submission_signing_message(&request);
            verify_signature(
                &registration.public_key,
                signed_message.as_bytes(),
                &request.signature,
            )?;

            let submission = SubmissionRecord {
                submission_id: request.submission_id,
                challenge_id: request.challenge_id,
                match_id: request.match_id,
                agent_id: request.agent_id,
                artifact_hash: request.artifact_hash,
                evidence_manifest_hash: request.evidence_manifest_hash,
                nonce: request.nonce,
                signature: request.signature,
                accepted_at: Utc::now(),
            };
            league
                .submissions
                .insert(submission.submission_id, submission.clone());
            league
                .used_agent_nonces
                .insert((submission.agent_id.clone(), submission.nonce.clone()));
            push_event(
                league,
                "hepta.submission.accepted.v1",
                submission.submission_id.to_string(),
                json!({
                    "submission_id": submission.submission_id,
                    "challenge_id": submission.challenge_id,
                    "match_id": submission.match_id,
                    "agent_id": submission.agent_id,
                    "artifact_hash": submission.artifact_hash,
                    "evidence_manifest_hash": submission.evidence_manifest_hash,
                }),
            );

            Ok((StatusCode::ACCEPTED, Json(submission)))
        })
        .await
}

async fn list_events(State(state): State<AppState>) -> Result<Json<Vec<EventEnvelope>>, ApiError> {
    state
        .inspect(|league| Ok(Json(league.events.clone())))
        .await
}

pub fn submission_signing_message(request: &SubmitArtifactRequest) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        request.protocol_version,
        request.submission_id,
        request.challenge_id,
        request.match_id,
        request.agent_id,
        request.artifact_hash,
        request.evidence_manifest_hash,
        request.nonce,
    )
}

pub fn key_rotation_signing_message(request: &RotateAgentKeyRequest) -> String {
    format!(
        "{}\n{}\n{}\n{}",
        KEY_ROTATION_PROTOCOL_V1, request.agent_id, request.new_public_key, request.nonce,
    )
}

fn require_service_token(
    headers: &HeaderMap,
    header_name: &'static str,
    expected: &str,
    error_code: &'static str,
) -> Result<(), ApiError> {
    let presented = headers
        .get(header_name)
        .and_then(|value| value.to_str().ok());
    if presented != Some(expected) {
        return Err(ApiError::forbidden(
            error_code,
            format!("valid {header_name} is required"),
        ));
    }
    Ok(())
}

fn validate_protocol(protocol_version: &str) -> Result<(), ApiError> {
    if protocol_version != AGENT_PROTOCOL_V1 {
        return Err(ApiError::bad_request(
            "unsupported_protocol_version",
            format!("expected {AGENT_PROTOCOL_V1}"),
        ));
    }
    Ok(())
}

fn validate_contract_text(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{field} is required"));
    }
    if value.chars().count() > 512 {
        return Err(format!("{field} exceeds 512 Unicode code points"));
    }
    if value.as_bytes().contains(&0) {
        return Err(format!("{field} contains a NUL byte"));
    }
    Ok(())
}

fn validate_contract_text_api(field: &'static str, value: &str) -> Result<(), ApiError> {
    validate_contract_text(field, value)
        .map_err(|message| ApiError::bad_request("invalid_field", message))
}

fn validate_logical_match_id(value: &str) -> Result<(), String> {
    let mut bytes = value.bytes();
    let first = bytes
        .next()
        .ok_or_else(|| "match_id is required".to_string())?;
    if value.len() > 128
        || !first.is_ascii_alphanumeric()
        || !bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err("match_id must match [A-Za-z0-9][A-Za-z0-9._:-]{0,127}".to_string());
    }
    Ok(())
}

fn canonical_digest(field: &'static str, value: &str) -> Result<String, ApiError> {
    validate_hash(field, value)?;
    Ok(value.to_ascii_lowercase())
}

fn decode_digest(value: &str) -> Result<[u8; 32], String> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| "digest must start with sha256:".to_string())?;
    if hex.len() != 64
        || hex.bytes().any(|byte| {
            !byte.is_ascii_hexdigit() || (byte.is_ascii_alphabetic() && !byte.is_ascii_lowercase())
        })
    {
        return Err("digest must contain 64 lowercase hexadecimal characters".to_string());
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| "digest contains invalid hexadecimal".to_string())?;
    }
    Ok(output)
}

fn decode_canonical_base64_exact<const N: usize>(
    field: &str,
    value: &str,
) -> Result<[u8; N], String> {
    let decoded = BASE64
        .decode(value)
        .map_err(|_| format!("{field} must be canonical padded base64"))?;
    if BASE64.encode(&decoded) != value {
        return Err(format!("{field} must be canonical padded base64"));
    }
    decoded
        .try_into()
        .map_err(|_| format!("{field} must decode to exactly {N} bytes"))
}

fn canonical_agent_public_key(value: &str) -> Result<String, ApiError> {
    let key = decode_verifying_key(value)?;
    Ok(BASE64.encode(key.to_bytes()))
}

struct CanonicalFrame {
    bytes: Vec<u8>,
}

impl CanonicalFrame {
    fn new(domain: &str) -> Self {
        let mut bytes = Vec::with_capacity(domain.len() + 1);
        bytes.extend_from_slice(domain.as_bytes());
        bytes.push(0);
        Self { bytes }
    }

    fn bytes(mut self, value: &[u8]) -> Result<Self, String> {
        let size = u32::try_from(value.len())
            .map_err(|_| "canonical field exceeds uint32 length".to_string())?;
        self.bytes.extend_from_slice(&size.to_be_bytes());
        self.bytes.extend_from_slice(value);
        Ok(self)
    }

    fn string(self, value: &str) -> Result<Self, String> {
        self.bytes(value.as_bytes())
    }

    fn u32(mut self, value: u32) -> Self {
        self.bytes.extend_from_slice(&value.to_be_bytes());
        self
    }

    fn i64(mut self, value: i64) -> Self {
        self.bytes.extend_from_slice(&value.to_be_bytes());
        self
    }

    fn digest(mut self, value: &str) -> Result<Self, String> {
        self.bytes.extend_from_slice(&decode_digest(value)?);
        Ok(self)
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

fn validate_nakama_authorization_claim(claim: &NakamaAuthorizationClaimV1) -> Result<(), String> {
    if claim.schema != NAKAMA_AUTHORIZATION_SCHEMA_V1 {
        return Err(format!("unsupported authorization schema {}", claim.schema));
    }
    for (field, value) in [
        ("authorization_id", claim.authorization_id.as_str()),
        ("match_id", claim.match_id.as_str()),
        ("challenge_id", claim.challenge_id.as_str()),
        ("agent_id", claim.agent_id.as_str()),
        ("agent_did", claim.agent_did.as_str()),
        ("agent_key_id", claim.agent_key_id.as_str()),
        ("subject_user_id", claim.subject_user_id.as_str()),
        ("role", claim.role.as_str()),
    ] {
        validate_contract_text(field, value)?;
    }
    validate_logical_match_id(&claim.match_id)?;
    decode_canonical_base64_exact::<32>("agent_public_key", &claim.agent_public_key)?;
    if claim.participant_slot != 1 && claim.participant_slot != 2 {
        return Err("participant_slot must be 1 or 2".to_string());
    }
    decode_digest(&claim.ruleset_hash)?;
    decode_digest(&claim.dataset_hash)?;
    decode_digest(&claim.challenge_snapshot_hash)?;
    if claim.issued_at_unix < 0 || claim.expires_at_unix <= claim.issued_at_unix {
        return Err("authorization validity interval is invalid".to_string());
    }
    Ok(())
}

pub fn nakama_authorization_claim_frame(
    claim: &NakamaAuthorizationClaimV1,
) -> Result<Vec<u8>, String> {
    validate_nakama_authorization_claim(claim)?;
    let agent_public_key =
        decode_canonical_base64_exact::<32>("agent_public_key", &claim.agent_public_key)?;
    let frame = CanonicalFrame::new("trnm_match_authorization_claim_v1")
        .string(&claim.schema)?
        .string(&claim.authorization_id)?
        .string(&claim.match_id)?
        .string(&claim.challenge_id)?
        .string(&claim.agent_id)?
        .string(&claim.agent_did)?
        .string(&claim.agent_key_id)?
        .bytes(&agent_public_key)?
        .string(&claim.subject_user_id)?
        .u32(claim.participant_slot)
        .string(&claim.role)?
        .digest(&claim.ruleset_hash)?
        .digest(&claim.dataset_hash)?
        .digest(&claim.challenge_snapshot_hash)?
        .i64(claim.issued_at_unix)
        .i64(claim.expires_at_unix);
    Ok(frame.finish())
}

pub fn nakama_authorization_signing_bytes(
    claim: &NakamaAuthorizationClaimV1,
    issuer_key_id: &str,
) -> Result<Vec<u8>, String> {
    validate_contract_text("issuer_key_id", issuer_key_id)?;
    let claim_frame = nakama_authorization_claim_frame(claim)?;
    Ok(CanonicalFrame::new("trnm_match_authorization_signature_v1")
        .string(issuer_key_id)?
        .bytes(&claim_frame)?
        .finish())
}

pub fn sign_nakama_match_authorization(
    claim: NakamaAuthorizationClaimV1,
    issuer_key_id: &str,
    signing_key: &SigningKey,
) -> Result<SignedNakamaMatchAuthorizationV1, String> {
    let signing_bytes = nakama_authorization_signing_bytes(&claim, issuer_key_id)?;
    Ok(SignedNakamaMatchAuthorizationV1 {
        claim,
        issuer_key_id: issuer_key_id.to_string(),
        signature: BASE64.encode(signing_key.sign(&signing_bytes).to_bytes()),
    })
}

fn challenge_snapshot_hash(challenge: &ResearchChallenge) -> Result<String, ApiError> {
    let status = match challenge.status {
        ChallengeStatus::Draft => "draft",
        ChallengeStatus::Open => "open",
        ChallengeStatus::Closed => "closed",
    };
    let ruleset_hash = canonical_digest("ruleset_hash", &challenge.ruleset_hash)?;
    let dataset_hash = canonical_digest("dataset_manifest_hash", &challenge.dataset_manifest_hash)?;
    let evaluator_hash = canonical_digest(
        "evaluator_manifest_hash",
        &challenge.evaluator_manifest_hash,
    )?;
    let frame = CanonicalFrame::new("hepta_challenge_snapshot_v1")
        .string(&challenge.challenge_id.to_string())
        .and_then(|frame| frame.string(&challenge.title))
        .and_then(|frame| frame.string(&challenge.description))
        .and_then(|frame| frame.string(&challenge.ruleset_version))
        .and_then(|frame| frame.digest(&ruleset_hash))
        .and_then(|frame| frame.digest(&dataset_hash))
        .and_then(|frame| frame.digest(&evaluator_hash))
        .and_then(|frame| frame.string(status))
        .map(|frame| frame.i64(challenge.created_at.timestamp()).finish())
        .map_err(|error| ApiError::internal(format!("frame challenge snapshot: {error}")))?;
    Ok(format!("sha256:{}", sha256_hex(&frame)))
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::bad_request(
            "invalid_field",
            format!("{field} must not be empty"),
        ));
    }
    Ok(())
}

fn validate_hash(field: &'static str, value: &str) -> Result<(), ApiError> {
    let valid = value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    });
    if !valid {
        return Err(ApiError::bad_request(
            "invalid_hash",
            format!("{field} must be a sha256:<64 hex characters> commitment"),
        ));
    }
    Ok(())
}

fn decode_verifying_key(value: &str) -> Result<VerifyingKey, ApiError> {
    let bytes = BASE64.decode(value).map_err(|_| {
        ApiError::bad_request(
            "invalid_public_key",
            "public_key must be base64-encoded Ed25519 bytes",
        )
    })?;
    let key_bytes: [u8; 32] = bytes.try_into().map_err(|_| {
        ApiError::bad_request(
            "invalid_public_key",
            "public_key must decode to 32 Ed25519 bytes",
        )
    })?;
    VerifyingKey::from_bytes(&key_bytes).map_err(|_| {
        ApiError::bad_request(
            "invalid_public_key",
            "public_key is not a valid Ed25519 key",
        )
    })
}

fn verify_signature(public_key: &str, message: &[u8], signature: &str) -> Result<(), ApiError> {
    let verifying_key = decode_verifying_key(public_key)?;
    let signature_bytes = BASE64.decode(signature).map_err(|_| {
        ApiError::bad_request(
            "invalid_signature",
            "signature must be base64-encoded Ed25519 bytes",
        )
    })?;
    let signature = Signature::from_slice(&signature_bytes).map_err(|_| {
        ApiError::bad_request(
            "invalid_signature",
            "signature must decode to 64 Ed25519 bytes",
        )
    })?;
    verifying_key.verify(message, &signature).map_err(|_| {
        ApiError::forbidden(
            "signature_verification_failed",
            "submission signature does not match the registered Agent key",
        )
    })
}

fn push_event(league: &mut LeagueState, event_type: &str, aggregate_id: String, payload: Value) {
    let payload_bytes = serde_json::to_vec(&payload).expect("serialize Hepta event payload");
    let aggregate_version = league
        .events
        .iter()
        .filter(|event| event.aggregate_id == aggregate_id)
        .count() as u64
        + 1;
    let idempotency_key = format!("{event_type}:{aggregate_id}:{aggregate_version}");
    league.events.push(EventEnvelope {
        schema_version: EVENT_SCHEMA_V1.to_string(),
        event_id: Uuid::new_v4(),
        event_type: event_type.to_string(),
        aggregate_id,
        aggregate_version,
        correlation_id: Uuid::new_v4(),
        causation_id: None,
        idempotency_key,
        occurred_at: Utc::now(),
        producer: "hepta-research-league".to_string(),
        payload_hash: format!("sha256:{}", sha256_hex(&payload_bytes)),
        payload,
    });
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn enrollment_key(challenge_id: Uuid, agent_id: &str) -> String {
    format!("{challenge_id}\n{agent_id}")
}

fn match_agent_key(match_id: Uuid, agent_id: &str) -> String {
    format!("{match_id}\n{agent_id}")
}
