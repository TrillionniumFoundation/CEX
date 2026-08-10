use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use axum::{
    extract::{MatchedPath, Path, Request, State},
    http::{HeaderMap, Method, StatusCode},
    middleware::{self, Next},
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
use tokio::sync::{Mutex, RwLock, Semaphore};
use trnm_finality_types::MAX_COMETBFT_RECEIPT_V2_WIRE_BYTES;
use uuid::Uuid;

pub mod trnm_v1;
pub use hepta_paper_raid_contracts as paper_raid_contracts;
mod challenge_ruleset_v1;
mod paper_chain_finality_v1;
mod paper_chain_finality_v2;
mod paper_raid_v2;
mod workflows;

pub use challenge_ruleset_v1::*;
pub use paper_chain_finality_v1::*;
pub use paper_chain_finality_v2::*;
pub use paper_raid_v2::*;

pub const AGENT_PROTOCOL_V1: &str = "hepta_agent_protocol_v1";
pub const EVENT_SCHEMA_V1: &str = "hepta_event_envelope_v1";
pub const KEY_ROTATION_PROTOCOL_V1: &str = "hepta_agent_key_rotation_v1";
pub const NAKAMA_AUTHORIZATION_SCHEMA_V1: &str = "trnm.match.authorization.v1";
pub const OPERATOR_TOKEN_HEADER: &str = "x-hepta-operator-token";
pub const NAKAMA_TOKEN_HEADER: &str = "x-hepta-nakama-token";
pub const TRNM_TOKEN_HEADER: &str = "x-hepta-trnm-token";
pub const USER_ASSERTION_HEADER: &str = "x-hepta-user-assertion";
// The upstream Receipt V2 wire type is intentionally generic and permits
// 128 MiB documents.  Hepta's Paper-bound lane is much narrower: the current
// live candidate receipts are about 16 KiB and have no opaque padding
// field.  Keep a separate deployment budget so a protocol-legal document
// cannot turn a 512 MiB Hepta container into an allocation oracle.
pub const DEFAULT_TRNM_RECEIPT_V2_MAX_BODY_BYTES: usize = 32 * 1024;
pub const MAX_TRNM_RECEIPT_V2_DEPLOYMENT_BODY_BYTES: usize = 1024 * 1024;
pub const DEFAULT_TRNM_RECEIPT_V2_MAX_IN_FLIGHT: usize = 1;
pub const MAX_TRNM_RECEIPT_V2_MAX_IN_FLIGHT: usize = 4;

const _: () =
    assert!(MAX_TRNM_RECEIPT_V2_DEPLOYMENT_BODY_BYTES <= MAX_COMETBFT_RECEIPT_V2_WIRE_BYTES);

const TRNM_RECEIPT_V2_MAX_BODY_BYTES_ENV: &str = "HEPTA_TRNM_RECEIPT_V2_MAX_BODY_BYTES";
const TRNM_RECEIPT_V2_MAX_IN_FLIGHT_ENV: &str = "HEPTA_TRNM_RECEIPT_V2_MAX_IN_FLIGHT";

const FINALITY_V2_EVIDENCE_TABLES: [&str; 3] = [
    "hepta_trnm_cometbft_time_checkpoints_v1",
    "hepta_paper_chain_finality_window_arms_v2",
    "hepta_paper_chain_finality_preparations_v2",
];

// Direct invocation of SECURITY DEFINER functions is denied by default.  This
// is the complete, deliberately small capability surface granted to the
// isolated finality writer.  Trigger functions do not belong here: PostgreSQL
// invokes them through their triggers without granting the login role a
// callable definer capability.
const FINALITY_WRITER_DEFINER_FUNCTIONS: [&str; 1] =
    ["public.hepta_assert_paper_finality_v2_source_unsealed(uuid)"];

fn parse_bounded_positive_decimal_env(
    field: &'static str,
    default: usize,
    maximum: usize,
) -> Result<usize, String> {
    let raw = match std::env::var(field) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(format!("{field} must be canonical UTF-8 decimal"));
        }
    };
    parse_bounded_positive_decimal(field, &raw, maximum)
}

fn parse_bounded_positive_decimal(field: &str, raw: &str, maximum: usize) -> Result<usize, String> {
    if raw.is_empty()
        || (raw.len() > 1 && raw.starts_with('0'))
        || raw.bytes().any(|byte| !byte.is_ascii_digit())
    {
        return Err(format!("{field} must be canonical positive decimal"));
    }
    let value = raw
        .parse::<usize>()
        .map_err(|_| format!("{field} exceeds the supported platform range"))?;
    if value == 0 || value > maximum {
        return Err(format!("{field} must be between 1 and {maximum}"));
    }
    Ok(value)
}

fn validate_trnm_receipt_v2_ingress_limits(
    max_body_bytes: usize,
    max_in_flight: usize,
) -> Result<(), String> {
    if max_body_bytes == 0 || max_body_bytes > MAX_TRNM_RECEIPT_V2_DEPLOYMENT_BODY_BYTES {
        return Err(format!(
            "{TRNM_RECEIPT_V2_MAX_BODY_BYTES_ENV} must be between 1 and {MAX_TRNM_RECEIPT_V2_DEPLOYMENT_BODY_BYTES}"
        ));
    }
    if max_in_flight == 0 || max_in_flight > MAX_TRNM_RECEIPT_V2_MAX_IN_FLIGHT {
        return Err(format!(
            "{TRNM_RECEIPT_V2_MAX_IN_FLIGHT_ENV} must be between 1 and {MAX_TRNM_RECEIPT_V2_MAX_IN_FLIGHT}"
        ));
    }
    Ok(())
}

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
    paper_chain_finality: Arc<RwLock<paper_chain_finality_v1::PaperChainFinalityMemory>>,
    cometbft_local_verification_clock:
        Arc<dyn Fn() -> std::time::SystemTime + Send + Sync + 'static>,
    paper_chain_verification_permits: Arc<Semaphore>,
    pool: Option<PgPool>,
    pub(crate) finality_pool: Option<PgPool>,
    security: Arc<SecurityConfig>,
    rate_limits: Arc<Mutex<HashMap<(String, String), RateWindow>>>,
    nakama_control_http: Option<Arc<NakamaControlHttpClient>>,
}

#[derive(Clone)]
struct NakamaControlHttpClient {
    base_url: reqwest::Url,
    runtime_http_key: Arc<str>,
    client: reqwest::Client,
}

impl NakamaControlHttpClient {
    fn new(base_url: &str, runtime_http_key: &str) -> Result<Self, String> {
        let base_url = reqwest::Url::parse(base_url)
            .map_err(|error| format!("HEPTA_NAKAMA_BASE_URL is invalid: {error}"))?;
        if !matches!(base_url.scheme(), "http" | "https")
            || base_url.host_str().is_none()
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
            || base_url.path() != "/"
        {
            return Err(
                "HEPTA_NAKAMA_BASE_URL must be an absolute credential-free HTTP(S) origin"
                    .to_string(),
            );
        }
        if runtime_http_key.is_empty()
            || runtime_http_key.len() > 512
            || runtime_http_key.chars().any(char::is_control)
        {
            return Err(
                "HEPTA_NAKAMA_RUNTIME_HTTP_KEY must contain 1-512 non-control characters"
                    .to_string(),
            );
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|error| format!("build Nakama control HTTP client: {error}"))?;
        Ok(Self {
            base_url,
            runtime_http_key: Arc::from(runtime_http_key),
            client,
        })
    }
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
    nakama_control_issuer_key_id: String,
    nakama_control_signing_key: SigningKey,
    consumer_edge_issuer: String,
    consumer_edge_audience: String,
    consumer_edge_verifying_keys: HashMap<String, VerifyingKey>,
    trusted_nakama_research_authorities: HashMap<String, VerifyingKey>,
    finality_mode: FinalityMode,
    trusted_trnm_validator_sets: Vec<trnm_v1::TrustedValidatorSetV1>,
    pinned_trnm_cometbft_trust_anchor_hashes: HashSet<String>,
    trnm_receipt_v2_max_body_bytes: usize,
    trnm_receipt_v2_max_in_flight: usize,
}

impl SecurityConfig {
    pub fn new(operator_token: impl Into<String>, nakama_token: impl Into<String>) -> Self {
        Self {
            operator_token: operator_token.into(),
            nakama_token: nakama_token.into(),
            trnm_token: "hepta-test-trnm-token".to_string(),
            nakama_authorization_issuer_key_id: "hepta-test-issuer-key-v1".to_string(),
            nakama_authorization_signing_key: SigningKey::from_bytes(&[0x5a; 32]),
            nakama_control_issuer_key_id: "hepta-test-control-key-v2".to_string(),
            nakama_control_signing_key: SigningKey::from_bytes(&[0x5b; 32]),
            consumer_edge_issuer: "hepta-test-consumer-edge".to_string(),
            consumer_edge_audience: "hepta-paper-raid-v2".to_string(),
            consumer_edge_verifying_keys: HashMap::from([(
                "hepta-test-consumer-edge-key-v2".to_string(),
                SigningKey::from_bytes(&[0x6c; 32]).verifying_key(),
            )]),
            trusted_nakama_research_authorities: HashMap::new(),
            finality_mode: FinalityMode::PendingOnly,
            trusted_trnm_validator_sets: Vec::new(),
            pinned_trnm_cometbft_trust_anchor_hashes: HashSet::new(),
            trnm_receipt_v2_max_body_bytes: DEFAULT_TRNM_RECEIPT_V2_MAX_BODY_BYTES,
            trnm_receipt_v2_max_in_flight: DEFAULT_TRNM_RECEIPT_V2_MAX_IN_FLIGHT,
        }
    }

    pub fn with_nakama_authorization_signer(
        mut self,
        issuer_key_id: impl Into<String>,
        seed: [u8; 32],
    ) -> Result<Self, String> {
        let issuer_key_id = issuer_key_id.into();
        validate_contract_text("issuer_key_id", &issuer_key_id)?;
        let signing_key = SigningKey::from_bytes(&seed);
        if signing_key.verifying_key() == self.nakama_control_signing_key.verifying_key()
            || self
                .trusted_nakama_research_authorities
                .values()
                .any(|key| key == &signing_key.verifying_key())
        {
            return Err(
                "Nakama authorization signer must differ from control and completion authorities"
                    .to_string(),
            );
        }
        self.nakama_authorization_issuer_key_id = issuer_key_id;
        self.nakama_authorization_signing_key = signing_key;
        Ok(self)
    }

    pub fn with_nakama_control_signer(
        mut self,
        issuer_key_id: impl Into<String>,
        seed: [u8; 32],
    ) -> Result<Self, String> {
        let issuer_key_id = issuer_key_id.into();
        validate_contract_text("control_issuer_key_id", &issuer_key_id)?;
        let signing_key = SigningKey::from_bytes(&seed);
        if signing_key.verifying_key() == self.nakama_authorization_signing_key.verifying_key()
            || self
                .trusted_nakama_research_authorities
                .values()
                .any(|key| key == &signing_key.verifying_key())
        {
            return Err(
                "Nakama control signer must differ from authorization and completion authorities"
                    .to_string(),
            );
        }
        self.nakama_control_issuer_key_id = issuer_key_id;
        self.nakama_control_signing_key = signing_key;
        Ok(self)
    }

    pub fn with_nakama_signers(
        mut self,
        authorization_issuer_key_id: impl Into<String>,
        authorization_seed: [u8; 32],
        control_issuer_key_id: impl Into<String>,
        control_seed: [u8; 32],
    ) -> Result<Self, String> {
        let authorization_issuer_key_id = authorization_issuer_key_id.into();
        let control_issuer_key_id = control_issuer_key_id.into();
        validate_contract_text("issuer_key_id", &authorization_issuer_key_id)?;
        validate_contract_text("control_issuer_key_id", &control_issuer_key_id)?;
        let authorization_key = SigningKey::from_bytes(&authorization_seed);
        let control_key = SigningKey::from_bytes(&control_seed);
        if authorization_key.verifying_key() == control_key.verifying_key()
            || self
                .trusted_nakama_research_authorities
                .values()
                .any(|key| {
                    key == &authorization_key.verifying_key() || key == &control_key.verifying_key()
                })
        {
            return Err(
                "Nakama authorization, control, and completion authorities must use distinct keys"
                    .to_string(),
            );
        }
        self.nakama_authorization_issuer_key_id = authorization_issuer_key_id;
        self.nakama_authorization_signing_key = authorization_key;
        self.nakama_control_issuer_key_id = control_issuer_key_id;
        self.nakama_control_signing_key = control_key;
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
        self.consumer_edge_verifying_keys.clear();
        self.consumer_edge_verifying_keys.insert(
            issuer_key_id,
            VerifyingKey::from_bytes(&public_key)
                .map_err(|_| "consumer Edge public key is not valid Ed25519".to_string())?,
        );
        Ok(self)
    }

    pub fn with_consumer_edge_verifying_key(
        mut self,
        issuer_key_id: impl Into<String>,
        public_key: [u8; 32],
    ) -> Result<Self, String> {
        let issuer_key_id = issuer_key_id.into();
        validate_contract_text("consumer_edge_issuer_key_id", &issuer_key_id)?;
        if self
            .consumer_edge_verifying_keys
            .contains_key(&issuer_key_id)
        {
            return Err("duplicate Consumer Edge issuer key ID".to_string());
        }
        self.consumer_edge_verifying_keys.insert(
            issuer_key_id,
            VerifyingKey::from_bytes(&public_key)
                .map_err(|_| "consumer Edge public key is not valid Ed25519".to_string())?,
        );
        Ok(self)
    }

    pub fn with_finality_mode(mut self, mode: FinalityMode) -> Self {
        self.finality_mode = mode;
        self
    }

    pub fn with_trnm_receipt_v2_ingress_limits(
        mut self,
        max_body_bytes: usize,
        max_in_flight: usize,
    ) -> Result<Self, String> {
        validate_trnm_receipt_v2_ingress_limits(max_body_bytes, max_in_flight)?;
        self.trnm_receipt_v2_max_body_bytes = max_body_bytes;
        self.trnm_receipt_v2_max_in_flight = max_in_flight;
        Ok(self)
    }

    pub fn with_pinned_trnm_cometbft_trust_anchor_hash(
        mut self,
        anchor_hash: impl Into<String>,
    ) -> Result<Self, String> {
        let anchor_hash = anchor_hash.into();
        validate_raw_sha256_hex("TRNM CometBFT trust anchor hash", &anchor_hash)?;
        if !self
            .pinned_trnm_cometbft_trust_anchor_hashes
            .insert(anchor_hash)
        {
            return Err("duplicate pinned TRNM CometBFT trust anchor hash".to_string());
        }
        Ok(self)
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
        if verifying_key == self.nakama_authorization_signing_key.verifying_key()
            || verifying_key == self.nakama_control_signing_key.verifying_key()
        {
            return Err(
                "Nakama completion authority must differ from authorization and control signers"
                    .to_string(),
            );
        }
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

    fn verify_nakama_research_completion_signature(
        &self,
        completion: &paper_raid_contracts::ResearchSessionCompletionV1,
        authority_public_key_base64: &str,
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
        let expected_public_key = BASE64.encode(verifying_key.to_bytes());
        if authority_public_key_base64 != expected_public_key {
            return Err(
                "Nakama evidence response authority key differs from the locally pinned key"
                    .to_string(),
            );
        }
        paper_raid_contracts::verify_research_session_completion(completion, verifying_key)
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
        let nakama_control_issuer_key_id = std::env::var("HEPTA_NAKAMA_CONTROL_ISSUER_KEY_ID")
            .map_err(|_| "HEPTA_NAKAMA_CONTROL_ISSUER_KEY_ID must be set".to_string())?;
        validate_contract_text(
            "HEPTA_NAKAMA_CONTROL_ISSUER_KEY_ID",
            &nakama_control_issuer_key_id,
        )?;
        let nakama_control_seed_base64 = std::env::var("HEPTA_NAKAMA_CONTROL_ED25519_SEED_BASE64")
            .map_err(|_| "HEPTA_NAKAMA_CONTROL_ED25519_SEED_BASE64 must be set".to_string())?;
        let nakama_control_seed = BASE64.decode(&nakama_control_seed_base64).map_err(|_| {
            "HEPTA_NAKAMA_CONTROL_ED25519_SEED_BASE64 must be canonical base64".to_string()
        })?;
        if BASE64.encode(&nakama_control_seed) != nakama_control_seed_base64 {
            return Err(
                "HEPTA_NAKAMA_CONTROL_ED25519_SEED_BASE64 must be canonical padded base64"
                    .to_string(),
            );
        }
        let nakama_control_seed: [u8; 32] = nakama_control_seed.try_into().map_err(|_| {
            "HEPTA_NAKAMA_CONTROL_ED25519_SEED_BASE64 must decode to exactly 32 bytes".to_string()
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
        let mut consumer_edge_keys =
            decode_public_key_ring_env("HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEYS_JSON")?;
        merge_legacy_public_key(
            &mut consumer_edge_keys,
            &consumer_edge_issuer_key_id,
            consumer_edge_public_key,
            "HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEYS_JSON",
        )?;
        let mut nakama_authority_keys =
            decode_public_key_ring_env("TRNM_NAKAMA_AUTHORITY_PUBLIC_KEYS_JSON")?;
        merge_legacy_public_key(
            &mut nakama_authority_keys,
            &nakama_authority_key_id,
            nakama_authority_public_key,
            "TRNM_NAKAMA_AUTHORITY_PUBLIC_KEYS_JSON",
        )?;
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
        let trnm_receipt_v2_max_body_bytes = parse_bounded_positive_decimal_env(
            TRNM_RECEIPT_V2_MAX_BODY_BYTES_ENV,
            DEFAULT_TRNM_RECEIPT_V2_MAX_BODY_BYTES,
            MAX_TRNM_RECEIPT_V2_DEPLOYMENT_BODY_BYTES,
        )?;
        let trnm_receipt_v2_max_in_flight = parse_bounded_positive_decimal_env(
            TRNM_RECEIPT_V2_MAX_IN_FLIGHT_ENV,
            DEFAULT_TRNM_RECEIPT_V2_MAX_IN_FLIGHT,
            MAX_TRNM_RECEIPT_V2_MAX_IN_FLIGHT,
        )?;
        let mut consumer_keys = consumer_edge_keys.into_iter();
        let (consumer_key_id, consumer_public_key) = consumer_keys
            .next()
            .ok_or_else(|| "Consumer Edge public key ring is empty".to_string())?;
        let mut security = Self::new(operator_token, nakama_token)
            .with_trnm_token(trnm_token)
            .with_consumer_edge_trust(
                consumer_edge_issuer,
                consumer_edge_audience,
                consumer_key_id,
                consumer_public_key,
            )?
            .with_nakama_signers(
                nakama_authorization_issuer_key_id,
                nakama_authorization_seed,
                nakama_control_issuer_key_id,
                nakama_control_seed,
            )?
            .with_trnm_receipt_v2_ingress_limits(
                trnm_receipt_v2_max_body_bytes,
                trnm_receipt_v2_max_in_flight,
            )?;
        for (key_id, public_key) in consumer_keys {
            security = security.with_consumer_edge_verifying_key(key_id, public_key)?;
        }
        for (key_id, public_key) in nakama_authority_keys {
            security = security.with_trusted_nakama_research_authority(key_id, public_key)?;
        }
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
        let pinned_anchor_hashes_json =
            std::env::var("HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON")
                .unwrap_or_else(|_| "[]".to_string());
        let pinned_anchor_hashes: Vec<String> = serde_json::from_str(&pinned_anchor_hashes_json)
            .map_err(|error| {
                format!("decode HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON: {error}")
            })?;
        for anchor_hash in pinned_anchor_hashes {
            security = security.with_pinned_trnm_cometbft_trust_anchor_hash(anchor_hash)?;
        }
        security.validate_finality_startup()?;
        Ok(security)
    }

    fn validate_finality_startup(&self) -> Result<(), String> {
        validate_trnm_receipt_v2_ingress_limits(
            self.trnm_receipt_v2_max_body_bytes,
            self.trnm_receipt_v2_max_in_flight,
        )?;
        if self.finality_mode == FinalityMode::Verified
            && self.pinned_trnm_cometbft_trust_anchor_hashes.is_empty()
        {
            return Err(
                "verified HEPTA_FINALITY_MODE requires HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON with at least one pinned trust-anchor hash"
                    .to_string(),
            );
        }
        Ok(())
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
            || self.nakama_control_issuer_key_id.trim().is_empty()
            || self.consumer_edge_issuer.trim().is_empty()
            || self.consumer_edge_audience.trim().is_empty()
            || self.consumer_edge_verifying_keys.is_empty()
        {
            errors.push("issuer_configuration_invalid");
        }
        if self.nakama_authorization_signing_key.verifying_key()
            == self.nakama_control_signing_key.verifying_key()
            || self
                .trusted_nakama_research_authorities
                .values()
                .any(|key| {
                    key == &self.nakama_authorization_signing_key.verifying_key()
                        || key == &self.nakama_control_signing_key.verifying_key()
                })
        {
            errors.push("nakama_authority_key_separation_invalid");
        }
        if self.trusted_nakama_research_authorities.is_empty() {
            errors.push("nakama_research_authority_missing");
        }
        if self.finality_mode == FinalityMode::Verified {
            if self.pinned_trnm_cometbft_trust_anchor_hashes.is_empty() {
                errors.push("trnm_finality_trust_missing");
            }
            if self
                .trusted_trnm_validator_sets
                .iter()
                .any(|validator_set| validator_set.validate().is_err())
            {
                errors.push("trnm_validator_set_invalid");
            }
        }
        if validate_trnm_receipt_v2_ingress_limits(
            self.trnm_receipt_v2_max_body_bytes,
            self.trnm_receipt_v2_max_in_flight,
        )
        .is_err()
        {
            errors.push("trnm_receipt_v2_ingress_configuration_invalid");
        }
        errors
    }
}

async fn apply_hepta_migrations(pool: &PgPool) -> Result<(), String> {
    for (name, migration) in [
        (
            "Hepta league",
            include_str!("../../../migrations/0031_add_hepta_research_league.sql"),
        ),
        (
            "Paper Raid",
            include_str!("../../../migrations/0032_add_hepta_paper_raid_v2.sql"),
        ),
        (
            "Paper collaboration",
            include_str!("../../../migrations/0033_add_hepta_paper_collaboration_kernel.sql"),
        ),
        (
            "Paper review",
            include_str!("../../../migrations/0034_add_hepta_paper_review_appeal.sql"),
        ),
        (
            "secure onboarding",
            include_str!("../../../migrations/0035_add_hepta_secure_onboarding.sql"),
        ),
        (
            "Nakama control",
            include_str!("../../../migrations/0036_add_hepta_nakama_research_control.sql"),
        ),
        (
            "Paper Chain finality V1",
            include_str!("../../../migrations/0037_add_hepta_paper_chain_finality_v1.sql"),
        ),
        (
            "Paper Chain finality V2",
            include_str!("../../../migrations/0038_add_hepta_paper_chain_finality_v2.sql"),
        ),
        (
            "Paper review assignments",
            include_str!("../../../migrations/0039_add_hepta_review_assignments.sql"),
        ),
        (
            "Paper evaluation draft quorum",
            include_str!("../../../migrations/0040_add_hepta_evaluation_draft_quorum.sql"),
        ),
        (
            "Agent capability disclosure",
            include_str!("../../../migrations/0041_add_hepta_agent_capability_disclosure.sql"),
        ),
        (
            "team proposal deadlines",
            include_str!("../../../migrations/0042_add_hepta_team_proposal_deadlines.sql"),
        ),
        (
            "ChallengeRuleset V1",
            include_str!("../../../migrations/0043_add_hepta_challenge_ruleset_v1.sql"),
        ),
    ] {
        sqlx::raw_sql(migration)
            .execute(pool)
            .await
            .map_err(|error| format!("apply {name} migration: {error}"))?;
    }
    Ok(())
}

async fn initialize_hepta_state(pool: &PgPool) -> Result<(), String> {
    sqlx::query(
        "insert into hepta_league_state (state_key, revision, state_json)
         values ('primary', 0, $1::jsonb)
         on conflict (state_key) do nothing",
    )
    .bind(
        serde_json::to_value(LeagueState::default())
            .map_err(|error| format!("serialize initial Hepta state: {error}"))?,
    )
    .execute(pool)
    .await
    .map_err(|error| format!("initialize Hepta state: {error}"))?;
    Ok(())
}

async fn verify_finality_v2_migration_catalog(pool: &PgPool) -> Result<(), String> {
    for table in FINALITY_V2_EVIDENCE_TABLES {
        let exists: bool = sqlx::query_scalar(
            "select exists(
                select 1
                from pg_class as relation
                join pg_namespace as namespace on namespace.oid=relation.relnamespace
                where namespace.nspname='public'
                  and relation.relname=$1
                  and relation.relkind in ('r','p')
             )",
        )
        .bind(table)
        .fetch_one(pool)
        .await
        .map_err(|error| format!("verify {table} migration table: {error}"))?;
        if !exists {
            return Err(format!(
                "Paper Chain finality V2 migration is missing public.{table}"
            ));
        }
    }

    for (table, trigger, function, trigger_type) in [
        (
            "hepta_trnm_cometbft_time_checkpoints_v1",
            "hepta_trnm_time_checkpoint_v1_progress_guard",
            "hepta_validate_paper_finality_v2_time_checkpoint",
            7_i16,
        ),
        (
            "hepta_trnm_cometbft_time_checkpoints_v1",
            "hepta_trnm_time_checkpoint_v1_immutable_guard",
            "hepta_reject_paper_finality_v2_evidence_mutation",
            27,
        ),
        (
            "hepta_trnm_cometbft_time_checkpoints_v1",
            "hepta_trnm_time_checkpoint_v1_truncate_guard",
            "hepta_reject_paper_finality_v2_truncate",
            34,
        ),
        (
            "hepta_paper_chain_finality_window_arms_v2",
            "hepta_paper_finality_v2_window_arm_guard",
            "hepta_paper_finality_v2_lock_window_arm",
            7,
        ),
        (
            "hepta_paper_chain_finality_window_arms_v2",
            "hepta_paper_finality_v2_window_arm_immutable_guard",
            "hepta_reject_paper_finality_v2_evidence_mutation",
            27,
        ),
        (
            "hepta_paper_chain_finality_window_arms_v2",
            "hepta_paper_finality_v2_window_arm_truncate_guard",
            "hepta_reject_paper_finality_v2_truncate",
            34,
        ),
        (
            "hepta_paper_chain_finality_preparations_v2",
            "hepta_paper_finality_v2_preparation_guard",
            "hepta_paper_finality_v2_lock_preparation",
            7,
        ),
        (
            "hepta_paper_chain_finality_preparations_v2",
            "hepta_paper_finality_v2_preparation_seal_guard",
            "hepta_paper_finality_v2_apply_seal",
            5,
        ),
        (
            "hepta_paper_chain_finality_preparations_v2",
            "hepta_paper_finality_v2_preparation_immutable_guard",
            "hepta_reject_paper_finality_v2_preparation_mutation",
            27,
        ),
        (
            "hepta_paper_chain_finality_preparations_v2",
            "hepta_paper_finality_v2_preparation_truncate_guard",
            "hepta_reject_paper_finality_v2_truncate",
            34,
        ),
        (
            "hepta_paper_projects",
            "hepta_paper_projects_finality_v2_source_guard",
            "hepta_guard_paper_finality_v2_anchor_mutation",
            27,
        ),
        (
            "hepta_paper_projects",
            "hepta_paper_projects_finality_v2_truncate_guard",
            "hepta_reject_paper_finality_v2_truncate",
            34,
        ),
        (
            "hepta_joint_paper_submissions",
            "hepta_joint_submissions_finality_v2_source_guard",
            "hepta_reject_paper_finality_v2_source_mutation",
            31,
        ),
        (
            "hepta_joint_paper_submissions",
            "hepta_joint_submissions_finality_v2_truncate_guard",
            "hepta_reject_paper_finality_v2_truncate",
            34,
        ),
        (
            "hepta_paper_evaluations",
            "hepta_paper_evaluations_finality_v2_source_guard",
            "hepta_reject_paper_finality_v2_source_mutation",
            31,
        ),
        (
            "hepta_paper_evaluations",
            "hepta_paper_evaluations_finality_v2_truncate_guard",
            "hepta_reject_paper_finality_v2_truncate",
            34,
        ),
        (
            "hepta_paper_reproductions",
            "hepta_paper_reproductions_finality_v2_source_guard",
            "hepta_reject_paper_finality_v2_source_mutation",
            31,
        ),
        (
            "hepta_paper_reproductions",
            "hepta_paper_reproductions_finality_v2_truncate_guard",
            "hepta_reject_paper_finality_v2_truncate",
            34,
        ),
        (
            "hepta_paper_appeals",
            "hepta_paper_appeals_finality_v2_source_guard",
            "hepta_reject_paper_finality_v2_source_mutation",
            31,
        ),
        (
            "hepta_paper_appeals",
            "hepta_paper_appeals_finality_v2_truncate_guard",
            "hepta_reject_paper_finality_v2_truncate",
            34,
        ),
        (
            "hepta_paper_appeal_resolutions",
            "hepta_paper_resolutions_finality_v2_source_guard",
            "hepta_reject_paper_finality_v2_source_mutation",
            31,
        ),
        (
            "hepta_paper_appeal_resolutions",
            "hepta_paper_resolutions_finality_v2_truncate_guard",
            "hepta_reject_paper_finality_v2_truncate",
            34,
        ),
        (
            "hepta_research_session_authorization_sets",
            "hepta_research_auth_sets_finality_v2_source_guard",
            "hepta_reject_paper_finality_v2_source_mutation",
            31,
        ),
        (
            "hepta_research_session_authorization_sets",
            "hepta_research_auth_sets_finality_v2_truncate_guard",
            "hepta_reject_paper_finality_v2_truncate",
            34,
        ),
        (
            "hepta_nakama_research_session_completions",
            "hepta_nakama_completions_finality_v2_source_guard",
            "hepta_reject_paper_finality_v2_source_mutation",
            31,
        ),
        (
            "hepta_nakama_research_session_completions",
            "hepta_nakama_completions_finality_v2_truncate_guard",
            "hepta_reject_paper_finality_v2_truncate",
            34,
        ),
    ] {
        let exact: bool = sqlx::query_scalar(
            "select exists(
                select 1
                from pg_trigger as trigger
                join pg_class as relation on relation.oid=trigger.tgrelid
                join pg_namespace as namespace on namespace.oid=relation.relnamespace
                join pg_proc as function on function.oid=trigger.tgfoid
                join pg_namespace as function_namespace
                  on function_namespace.oid=function.pronamespace
                where namespace.nspname='public'
                  and relation.relname=$1
                  and trigger.tgname=$2
                  and not trigger.tgisinternal
                  and function_namespace.nspname='public'
                  and function.proname=$3
                  and trigger.tgtype=$4
                  and trigger.tgenabled='A'
             )",
        )
        .bind(table)
        .bind(trigger)
        .bind(function)
        .bind(trigger_type)
        .fetch_one(pool)
        .await
        .map_err(|error| format!("verify public.{table}.{trigger}: {error}"))?;
        if !exact {
            return Err(format!(
                "Paper Chain finality V2 trigger public.{table}.{trigger} is missing, disabled, or miswired"
            ));
        }
    }

    let constraint_catalog = sqlx::query(
        "with external_constraint(relation_id, constraint_name) as (values
            (to_regclass('public.hepta_trnm_cometbft_trust_anchors'),
             'hepta_trnm_trust_anchor_chain_unique'),
            (to_regclass('public.hepta_joint_paper_submissions'),
             'hepta_joint_submissions_id_paper_unique'),
            (to_regclass('public.hepta_paper_evaluations'),
             'hepta_paper_evaluations_id_submission_paper_unique'),
            (to_regclass('public.hepta_paper_evaluations'),
             'hepta_paper_evaluations_submission_paper_fkey'),
            (to_regclass('public.hepta_paper_evaluations'),
             'hepta_paper_evaluations_supersedes_same_submission_fkey'),
            (to_regclass('public.hepta_paper_reproductions'),
             'hepta_paper_reproductions_id_evaluation_paper_unique'),
            (to_regclass('public.hepta_paper_reproductions'),
             'hepta_paper_reproductions_supersedes_same_evaluation_fkey'),
            (to_regclass('public.hepta_research_session_authorization_sets'),
             'hepta_research_auth_set_session_roster_paper_unique'),
            (to_regclass('public.hepta_research_session_authorization_sets'),
             'hepta_research_auth_set_identity_epoch_paper_unique'),
            (to_regclass('public.hepta_nakama_research_session_completions'),
             'hepta_nakama_completion_match_evidence_paper_unique'),
            (to_regclass('public.hepta_nakama_research_session_completions'),
             'hepta_nakama_completion_authorization_epoch_fkey'),
            (to_regclass('public.hepta_nakama_research_session_completions'),
             'hepta_nakama_completion_authorization_identity_fkey'),
            (to_regclass('public.hepta_paper_appeals'),
             'hepta_paper_appeals_id_evaluation_paper_unique'),
            (to_regclass('public.hepta_paper_appeals'),
             'hepta_paper_appeals_id_paper_unique'),
            (to_regclass('public.hepta_paper_appeal_resolutions'),
             'hepta_paper_resolutions_id_appeal_paper_unique'),
            (to_regclass('public.hepta_paper_appeal_resolutions'),
             'hepta_paper_resolutions_superseding_evaluation_paper_fkey'),
            (to_regclass('public.hepta_paper_projects'),
             'hepta_paper_projects_finality_v2_seal_coherent'),
            (to_regclass('public.hepta_paper_projects'),
             'hepta_paper_projects_finality_v2_seal_preparation_fkey')
        ), managed_constraint as (
            select
                namespace.nspname as schema_name,
                relation.relname as relation_name,
                constraint_row.oid,
                constraint_row.conname,
                constraint_row.contype,
                constraint_row.convalidated,
                constraint_row.condeferrable,
                constraint_row.condeferred
            from pg_constraint as constraint_row
            join pg_class as relation on relation.oid=constraint_row.conrelid
            join pg_namespace as namespace on namespace.oid=relation.relnamespace
            where constraint_row.conrelid in (
                to_regclass('public.hepta_trnm_cometbft_time_checkpoints_v1'),
                to_regclass('public.hepta_paper_chain_finality_window_arms_v2'),
                to_regclass('public.hepta_paper_chain_finality_preparations_v2')
            ) or (constraint_row.conrelid, constraint_row.conname) in (
                select relation_id, constraint_name from external_constraint
            )
        ), canonical_line as (
            select format(
                '%s.%s|%s|%s|%s|%s|%s|%s',
                schema_name,
                relation_name,
                conname,
                contype,
                convalidated,
                condeferrable,
                condeferred,
                replace(pg_get_constraintdef(oid, false), 'public.', '')
            ) as line
            from managed_constraint
        )
        select
            count(*)::bigint as constraint_count,
            encode(sha256(convert_to(
                string_agg(line, E'\\n' order by line),
                'UTF8'
            )), 'hex') as catalog_sha256
        from canonical_line",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("verify Paper Chain finality V2 constraints: {error}"))?;
    let constraint_count: i64 = constraint_catalog.get("constraint_count");
    let catalog_sha256: String = constraint_catalog.get("catalog_sha256");
    if constraint_count != 77
        || catalog_sha256 != "910d4454106f5722ad44c6c9095bf48d585dfaa9501fc40d9ef377fd57c3f3ba"
    {
        return Err(format!(
            "Paper Chain finality V2 constraint catalog mismatch: expected 77/910d4454106f5722ad44c6c9095bf48d585dfaa9501fc40d9ef377fd57c3f3ba, got {constraint_count}/{catalog_sha256}"
        ));
    }
    Ok(())
}

async fn quoted_identifier(pool: &PgPool, value: &str, label: &str) -> Result<String, String> {
    sqlx::query_scalar("select quote_ident($1)")
        .bind(value)
        .fetch_one(pool)
        .await
        .map_err(|error| format!("quote {label}: {error}"))
}

async fn current_login_role(pool: &PgPool, label: &str) -> Result<String, String> {
    let row = sqlx::query("select current_user as current_role, session_user as session_role")
        .fetch_one(pool)
        .await
        .map_err(|error| format!("inspect {label} PostgreSQL role: {error}"))?;
    let current_role: String = row.get("current_role");
    let session_role: String = row.get("session_role");
    if current_role != session_role {
        return Err(format!(
            "{label} database connection must log in directly as {current_role:?}; session role {session_role:?} could RESET ROLE"
        ));
    }
    Ok(current_role)
}

async fn verify_unprivileged_database_role(
    pool: &PgPool,
    role: &str,
    label: &str,
) -> Result<(), String> {
    let safe: Option<bool> = sqlx::query_scalar(
        "select
            role.rolcanlogin
            and not role.rolsuper
            and not role.rolinherit
            and not role.rolcreatedb
            and not role.rolcreaterole
            and not role.rolreplication
            and not role.rolbypassrls
            and not exists (
                select 1 from pg_auth_members as membership
                where membership.member=role.oid or membership.roleid=role.oid
            )
            and not exists (
                select 1
                from pg_class as relation
                join pg_namespace as namespace on namespace.oid=relation.relnamespace
                where namespace.nspname='public' and relation.relowner=role.oid
            )
            and not exists (
                select 1
                from pg_proc as function
                join pg_namespace as namespace on namespace.oid=function.pronamespace
                where namespace.nspname='public' and function.proowner=role.oid
            )
            and not exists (
                select 1 from pg_namespace as namespace
                where namespace.nspname='public' and namespace.nspowner=role.oid
            )
            and not exists (
                select 1 from pg_database as database
                where database.datname=current_database() and database.datdba=role.oid
            )
            and has_schema_privilege(role.rolname, 'public', 'USAGE')
            and not has_schema_privilege(role.rolname, 'public', 'CREATE')
            and has_database_privilege(role.rolname, current_database(), 'CONNECT')
            and not has_database_privilege(role.rolname, current_database(), 'CREATE')
            and not has_database_privilege(role.rolname, current_database(), 'TEMPORARY')
         from pg_roles as role
         where role.rolname=$1",
    )
    .bind(role)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("verify {label} database role attributes: {error}"))?;
    match safe {
        Some(true) => Ok(()),
        Some(false) => Err(format!(
            "{label} database role {role:?} is privileged, owns database objects, has role memberships, or has DDL authority"
        )),
        None => Err(format!("{label} database role {role:?} does not exist")),
    }
}

struct DatabaseTablePrivileges {
    table: String,
    table_select: bool,
    table_insert: bool,
    any_insert: bool,
    table_update: bool,
    any_update: bool,
    delete: bool,
    truncate: bool,
    any_references: bool,
    trigger: bool,
}

async fn public_table_privileges(
    pool: &PgPool,
    role: &str,
) -> Result<Vec<DatabaseTablePrivileges>, String> {
    let rows = sqlx::query(
        "select
            relation.relname,
            has_table_privilege($1, relation.oid, 'SELECT') as table_select,
            has_table_privilege($1, relation.oid, 'INSERT') as table_insert,
            has_any_column_privilege($1, relation.oid, 'INSERT') as any_insert,
            has_table_privilege($1, relation.oid, 'UPDATE') as table_update,
            has_any_column_privilege($1, relation.oid, 'UPDATE') as any_update,
            has_table_privilege($1, relation.oid, 'DELETE') as can_delete,
            has_table_privilege($1, relation.oid, 'TRUNCATE') as can_truncate,
            has_any_column_privilege($1, relation.oid, 'REFERENCES') as any_references,
            has_table_privilege($1, relation.oid, 'TRIGGER') as can_trigger
         from pg_class as relation
         join pg_namespace as namespace on namespace.oid=relation.relnamespace
         where namespace.nspname='public' and relation.relkind in ('r','p')
         order by relation.relname",
    )
    .bind(role)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("inspect {role:?} public-table privileges: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|row| DatabaseTablePrivileges {
            table: row.get("relname"),
            table_select: row.get("table_select"),
            table_insert: row.get("table_insert"),
            any_insert: row.get("any_insert"),
            table_update: row.get("table_update"),
            any_update: row.get("any_update"),
            delete: row.get("can_delete"),
            truncate: row.get("can_truncate"),
            any_references: row.get("any_references"),
            trigger: row.get("can_trigger"),
        })
        .collect())
}

async fn verify_runtime_role_privileges(pool: &PgPool, role: &str) -> Result<(), String> {
    let evidence = FINALITY_V2_EVIDENCE_TABLES
        .into_iter()
        .collect::<HashSet<_>>();
    for privileges in public_table_privileges(pool, role).await? {
        let is_evidence = evidence.contains(privileges.table.as_str());
        let valid_dml = if is_evidence {
            privileges.table_select
                && !privileges.any_insert
                && !privileges.any_update
                && !privileges.delete
        } else {
            privileges.table_select
                && privileges.table_insert
                && privileges.table_update
                && privileges.delete
        };
        if !valid_dml || privileges.truncate || privileges.any_references || privileges.trigger {
            return Err(format!(
                "runtime role {role:?} has an unsafe privilege set on public.{}",
                privileges.table
            ));
        }
    }

    let unsafe_sequences: i64 = sqlx::query_scalar(
        "select count(*)
         from pg_class as relation
         join pg_namespace as namespace on namespace.oid=relation.relnamespace
         where namespace.nspname='public'
           and relation.relkind='S'
           and (
               not has_sequence_privilege($1, relation.oid, 'USAGE')
               or not has_sequence_privilege($1, relation.oid, 'SELECT')
               or has_sequence_privilege($1, relation.oid, 'UPDATE')
           )",
    )
    .bind(role)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("verify runtime sequence privileges: {error}"))?;
    if unsafe_sequences != 0 {
        return Err(format!(
            "runtime role {role:?} violates the sequence read/use-only boundary on {unsafe_sequences} public sequences"
        ));
    }
    verify_definer_function_privileges(pool, role, false).await
}

async fn verify_finality_role_privileges(pool: &PgPool, role: &str) -> Result<(), String> {
    let evidence = FINALITY_V2_EVIDENCE_TABLES
        .into_iter()
        .collect::<HashSet<_>>();
    for privileges in public_table_privileges(pool, role).await? {
        let expected_insert = evidence.contains(privileges.table.as_str());
        let insert_matches = if expected_insert {
            privileges.table_insert
        } else {
            !privileges.any_insert
        };
        if !privileges.table_select
            || !insert_matches
            || privileges.any_update
            || privileges.delete
            || privileges.truncate
            || privileges.any_references
            || privileges.trigger
        {
            return Err(format!(
                "finality role {role:?} has an unsafe privilege set on public.{}",
                privileges.table
            ));
        }
    }

    let sequence_privileges: i64 = sqlx::query_scalar(
        "select count(*)
         from pg_class as relation
         join pg_namespace as namespace on namespace.oid=relation.relnamespace
         where namespace.nspname='public'
           and relation.relkind='S'
           and (
               has_sequence_privilege($1, relation.oid, 'USAGE')
               or has_sequence_privilege($1, relation.oid, 'SELECT')
               or has_sequence_privilege($1, relation.oid, 'UPDATE')
           )",
    )
    .bind(role)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("verify finality sequence privileges: {error}"))?;
    if sequence_privileges != 0 {
        return Err(format!(
            "finality role {role:?} unexpectedly has privileges on {sequence_privileges} public sequences"
        ));
    }
    verify_definer_function_privileges(pool, role, true).await
}

async fn verify_definer_function_privileges(
    pool: &PgPool,
    role: &str,
    finality_writer: bool,
) -> Result<(), String> {
    let mut allowed_oids = HashSet::new();
    for signature in FINALITY_WRITER_DEFINER_FUNCTIONS {
        let row = sqlx::query(
            "select
                function.oid::text as oid,
                function.prosecdef,
                coalesce(function.proconfig @> array['search_path=pg_catalog'], false)
                    as safe_search_path,
                has_function_privilege($1, function.oid, 'EXECUTE') as executable
             from pg_proc as function
             where function.oid=to_regprocedure($2)",
        )
        .bind(role)
        .bind(signature)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("verify finality definer function {signature}: {error}"))?
        .ok_or_else(|| format!("finality definer function {signature} is missing"))?;
        let oid: String = row.get("oid");
        let security_definer: bool = row.get("prosecdef");
        let safe_search_path: bool = row.get("safe_search_path");
        let executable: bool = row.get("executable");
        if !security_definer || !safe_search_path || executable != finality_writer {
            return Err(format!(
                "database role {role:?} has an unsafe capability boundary for {signature}"
            ));
        }
        allowed_oids.insert(oid);
    }

    let rows = sqlx::query(
        "select
            function.oid::text as oid,
            function.oid::regprocedure::text as signature,
            has_function_privilege($1, function.oid, 'EXECUTE') as executable
         from pg_proc as function
         join pg_namespace as namespace on namespace.oid=function.pronamespace
         where namespace.nspname='public' and function.prosecdef",
    )
    .bind(role)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("inspect {role:?} SECURITY DEFINER privileges: {error}"))?;
    for row in rows {
        let oid: String = row.get("oid");
        let signature: String = row.get("signature");
        let executable: bool = row.get("executable");
        if executable && (!finality_writer || !allowed_oids.contains(&oid)) {
            return Err(format!(
                "database role {role:?} can execute unauthorized SECURITY DEFINER function {signature}"
            ));
        }
    }
    Ok(())
}

async fn configure_database_roles(
    migration_pool: &PgPool,
    runtime_role: &str,
    finality_role: &str,
) -> Result<(), String> {
    let migration_role = current_login_role(migration_pool, "migration-owner").await?;
    if runtime_role == finality_role
        || runtime_role == migration_role
        || finality_role == migration_role
    {
        return Err(
            "migration-owner, runtime, and finality database roles must be distinct".to_string(),
        );
    }

    let quoted_runtime = quoted_identifier(migration_pool, runtime_role, "runtime role").await?;
    let quoted_finality = quoted_identifier(migration_pool, finality_role, "finality role").await?;
    let database: String = sqlx::query_scalar("select current_database()")
        .fetch_one(migration_pool)
        .await
        .map_err(|error| format!("inspect Hepta database name: {error}"))?;
    let quoted_database = quoted_identifier(migration_pool, &database, "Hepta database").await?;

    for role in [runtime_role, finality_role] {
        let exists: bool =
            sqlx::query_scalar("select exists(select 1 from pg_roles where rolname=$1)")
                .bind(role)
                .fetch_one(migration_pool)
                .await
                .map_err(|error| format!("inspect database role {role:?}: {error}"))?;
        if !exists {
            return Err(format!("database role {role:?} does not exist"));
        }
    }

    for statement in [
        format!("revoke create on database {quoted_database} from public"),
        format!("revoke temporary on database {quoted_database} from public"),
        "revoke create on schema public from public".to_string(),
        format!("revoke all privileges on database {quoted_database} from {quoted_runtime}"),
        format!("revoke all privileges on database {quoted_database} from {quoted_finality}"),
        format!("grant connect on database {quoted_database} to {quoted_runtime}"),
        format!("grant connect on database {quoted_database} to {quoted_finality}"),
        format!("revoke all on schema public from {quoted_runtime}"),
        format!("revoke all on schema public from {quoted_finality}"),
        format!("grant usage on schema public to {quoted_runtime}"),
        format!("grant usage on schema public to {quoted_finality}"),
        format!(
            "revoke all privileges on all tables in schema public from {quoted_runtime}"
        ),
        format!(
            "revoke all privileges on all tables in schema public from {quoted_finality}"
        ),
        format!(
            "grant select,insert,update,delete on all tables in schema public to {quoted_runtime}"
        ),
        format!(
            "revoke insert,update,delete on table public.hepta_trnm_cometbft_time_checkpoints_v1, public.hepta_paper_chain_finality_window_arms_v2, public.hepta_paper_chain_finality_preparations_v2 from {quoted_runtime}"
        ),
        format!("grant select on all tables in schema public to {quoted_finality}"),
        format!(
            "grant insert on table public.hepta_trnm_cometbft_time_checkpoints_v1, public.hepta_paper_chain_finality_window_arms_v2, public.hepta_paper_chain_finality_preparations_v2 to {quoted_finality}"
        ),
        format!("revoke all privileges on all sequences in schema public from {quoted_runtime}"),
        format!("revoke all privileges on all sequences in schema public from {quoted_finality}"),
        format!("grant usage,select on all sequences in schema public to {quoted_runtime}"),
        format!(
            "alter default privileges in schema public revoke all on tables from {quoted_runtime}"
        ),
        format!(
            "alter default privileges in schema public revoke all on tables from {quoted_finality}"
        ),
        format!(
            "alter default privileges in schema public revoke all on sequences from {quoted_runtime}"
        ),
        format!(
            "alter default privileges in schema public revoke all on sequences from {quoted_finality}"
        ),
        "alter default privileges in schema public revoke execute on functions from public"
            .to_string(),
        format!(
            "alter default privileges in schema public revoke all on functions from {quoted_runtime}"
        ),
        format!(
            "alter default privileges in schema public revoke all on functions from {quoted_finality}"
        ),
    ] {
        sqlx::raw_sql(&statement)
            .execute(migration_pool)
            .await
            .map_err(|error| format!("configure Hepta database roles: {error}"))?;
    }

    let definer_signatures = sqlx::query_scalar::<_, String>(
        "select function.oid::regprocedure::text
         from pg_proc as function
         join pg_namespace as namespace on namespace.oid=function.pronamespace
         where namespace.nspname='public' and function.prosecdef",
    )
    .fetch_all(migration_pool)
    .await
    .map_err(|error| format!("enumerate Hepta SECURITY DEFINER functions: {error}"))?;
    for signature in definer_signatures {
        sqlx::raw_sql(&format!(
            "revoke all on function {signature} from public, {quoted_runtime}, {quoted_finality}"
        ))
        .execute(migration_pool)
        .await
        .map_err(|error| format!("revoke definer function {signature}: {error}"))?;
    }
    for signature in FINALITY_WRITER_DEFINER_FUNCTIONS {
        sqlx::raw_sql(&format!(
            "grant execute on function {signature} to {quoted_finality}"
        ))
        .execute(migration_pool)
        .await
        .map_err(|error| format!("grant finality capability {signature}: {error}"))?;
    }

    verify_unprivileged_database_role(migration_pool, runtime_role, "runtime").await?;
    verify_unprivileged_database_role(migration_pool, finality_role, "finality").await?;
    verify_runtime_role_privileges(migration_pool, runtime_role).await?;
    verify_finality_role_privileges(migration_pool, finality_role).await?;
    Ok(())
}

impl AppState {
    pub fn new(security: SecurityConfig) -> Self {
        Self::from_pools(None, None, security)
    }

    fn from_pools(
        pool: Option<PgPool>,
        finality_pool: Option<PgPool>,
        security: SecurityConfig,
    ) -> Self {
        let paper_chain_verification_permits =
            Arc::new(Semaphore::new(security.trnm_receipt_v2_max_in_flight));
        Self {
            inner: Arc::new(RwLock::new(LeagueState::default())),
            paper_raid: Arc::new(RwLock::new(paper_raid_v2::PaperRaidMemory::default())),
            paper_chain_finality: Arc::new(RwLock::new(
                paper_chain_finality_v1::PaperChainFinalityMemory::default(),
            )),
            cometbft_local_verification_clock: Arc::new(std::time::SystemTime::now),
            paper_chain_verification_permits,
            pool,
            finality_pool,
            security: Arc::new(security),
            rate_limits: Arc::new(Mutex::new(HashMap::new())),
            nakama_control_http: None,
        }
    }

    pub async fn connect(database_url: &str, security: SecurityConfig) -> Result<Self, String> {
        let pool = PgPool::connect(database_url)
            .await
            .map_err(|error| format!("connect Hepta PostgreSQL: {error}"))?;
        apply_hepta_migrations(&pool).await?;
        initialize_hepta_state(&pool).await?;
        verify_finality_v2_migration_catalog(&pool).await?;
        Ok(Self::from_pools(Some(pool.clone()), Some(pool), security))
    }

    pub async fn migrate_and_configure_database_roles(
        migration_database_url: &str,
        runtime_role: &str,
        finality_role: &str,
    ) -> Result<(), String> {
        let migration_pool = PgPool::connect(migration_database_url)
            .await
            .map_err(|error| format!("connect Hepta migration-owner PostgreSQL: {error}"))?;
        let result = async {
            apply_hepta_migrations(&migration_pool).await?;
            initialize_hepta_state(&migration_pool).await?;
            verify_finality_v2_migration_catalog(&migration_pool).await?;
            configure_database_roles(&migration_pool, runtime_role, finality_role).await
        }
        .await;
        migration_pool.close().await;
        result
    }

    pub async fn connect_with_database_roles(
        runtime_database_url: &str,
        finality_database_url: &str,
        security: SecurityConfig,
    ) -> Result<Self, String> {
        let runtime_pool = PgPool::connect(runtime_database_url)
            .await
            .map_err(|error| format!("connect Hepta runtime PostgreSQL: {error}"))?;
        let finality_pool = match PgPool::connect(finality_database_url).await {
            Ok(pool) => pool,
            Err(error) => {
                runtime_pool.close().await;
                return Err(format!("connect Hepta finality PostgreSQL: {error}"));
            }
        };
        let result = async {
            let runtime_role = current_login_role(&runtime_pool, "runtime").await?;
            let finality_role = current_login_role(&finality_pool, "finality").await?;
            if runtime_role == finality_role {
                return Err(
                    "HEPTA_DATABASE_URL and HEPTA_FINALITY_DATABASE_URL must use distinct login roles"
                        .to_string(),
                );
            }
            let runtime_database: String = sqlx::query_scalar("select current_database()")
                .fetch_one(&runtime_pool)
                .await
                .map_err(|error| format!("inspect runtime database: {error}"))?;
            let finality_database: String = sqlx::query_scalar("select current_database()")
                .fetch_one(&finality_pool)
                .await
                .map_err(|error| format!("inspect finality database: {error}"))?;
            if runtime_database != finality_database {
                return Err(
                    "runtime and finality PostgreSQL roles must connect to the same database"
                        .to_string(),
                );
            }
            verify_finality_v2_migration_catalog(&runtime_pool).await?;
            verify_unprivileged_database_role(&runtime_pool, &runtime_role, "runtime").await?;
            verify_unprivileged_database_role(&finality_pool, &finality_role, "finality").await?;
            verify_runtime_role_privileges(&runtime_pool, &runtime_role).await?;
            verify_finality_role_privileges(&finality_pool, &finality_role).await?;
            Ok(())
        }
        .await;
        if let Err(error) = result {
            runtime_pool.close().await;
            finality_pool.close().await;
            return Err(error);
        }
        Ok(Self::from_pools(
            Some(runtime_pool),
            Some(finality_pool),
            security,
        ))
    }

    pub fn with_nakama_control_http(
        mut self,
        base_url: &str,
        runtime_http_key: &str,
    ) -> Result<Self, String> {
        self.nakama_control_http = Some(Arc::new(NakamaControlHttpClient::new(
            base_url,
            runtime_http_key,
        )?));
        Ok(self)
    }

    pub fn with_nakama_control_http_from_env(self) -> Result<Self, String> {
        let base_url = std::env::var("HEPTA_NAKAMA_BASE_URL")
            .map_err(|_| "HEPTA_NAKAMA_BASE_URL must be set".to_string())?;
        let runtime_http_key = std::env::var("HEPTA_NAKAMA_RUNTIME_HTTP_KEY")
            .map_err(|_| "HEPTA_NAKAMA_RUNTIME_HTTP_KEY must be set".to_string())?;
        self.with_nakama_control_http(&base_url, &runtime_http_key)
    }

    pub(crate) fn nakama_control_http_configured(&self) -> bool {
        self.nakama_control_http.is_some()
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
        self.pool.is_some() && self.finality_pool.is_some()
    }

    pub(crate) async fn probe_database_pools(&self) -> Result<(), &'static str> {
        let runtime_pool = self.pool.as_ref().ok_or("database_pool_missing")?;
        let finality_pool = self
            .finality_pool
            .as_ref()
            .ok_or("finality_database_pool_missing")?;
        for (pool, acquire_failure, probe_failure) in [
            (
                runtime_pool,
                "database_pool_acquire_failed",
                "database_probe_failed",
            ),
            (
                finality_pool,
                "finality_database_pool_acquire_failed",
                "finality_database_probe_failed",
            ),
        ] {
            let mut connection = pool.acquire().await.map_err(|_| acquire_failure)?;
            match sqlx::query_scalar::<_, i32>("select 1")
                .fetch_one(&mut *connection)
                .await
            {
                Ok(1) => {}
                Ok(_) => return Err("database_probe_unexpected_result"),
                Err(_) => return Err(probe_failure),
            }
        }
        Ok(())
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruleset: Option<ChallengeRulesetV1>,
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
    #[serde(default)]
    pub ruleset: Option<ChallengeRulesetV1>,
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

    fn bad_gateway(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code,
            message: message.into(),
        }
    }

    fn service_unavailable(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code,
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

    fn payload_too_large(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            code: "request_body_too_large",
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
        .route("/v1/hepta/challenges", post(create_challenge))
        .route("/v1/hepta/challenges/:challenge_id", get(get_challenge))
        .route(
            "/v1/hepta/operator/challenges",
            get(list_operator_challenges),
        )
        .route(
            "/v1/hepta/operator/challenges/:challenge_id",
            get(get_operator_challenge),
        )
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
        .merge(paper_chain_finality_v1::router())
        .merge(paper_chain_finality_v2::router())
        .merge(paper_raid_v2::router())
        .merge(workflows::router())
        .with_state(state)
        .layer(middleware::from_fn(trace_http_request))
}

async fn trace_http_request(request: Request, next: Next) -> Response {
    let request_id = Uuid::new_v4();
    let method: Method = request.method().clone();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or("unmatched")
        .to_string();
    let started_at = std::time::Instant::now();
    let response = next.run(request).await;
    tracing::info!(
        target: "hepta_http",
        request_id = %request_id,
        method = %method,
        route = %route,
        status = response.status().as_u16(),
        latency_ms = started_at.elapsed().as_millis() as u64,
        "request completed"
    );
    response
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
    if let Some(ruleset) = &request.ruleset {
        ruleset
            .validate()
            .map_err(|message| ApiError::bad_request("invalid_challenge_ruleset", message))?;
        let computed_hash = ruleset
            .canonical_hash()
            .map_err(|message| ApiError::bad_request("invalid_challenge_ruleset", message))?;
        if request.ruleset_hash != computed_hash {
            return Err(ApiError::bad_request(
                "challenge_ruleset_hash_mismatch",
                format!("ruleset_hash must equal the canonical typed ruleset hash {computed_hash}"),
            ));
        }
    }

    let challenge = ResearchChallenge {
        challenge_id: Uuid::new_v4(),
        title: request.title,
        description: request.description,
        ruleset_version: request.ruleset_version,
        ruleset_hash: request.ruleset_hash,
        dataset_manifest_hash: request.dataset_manifest_hash,
        evaluator_manifest_hash: request.evaluator_manifest_hash,
        ruleset: request.ruleset,
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
                    "ruleset_enforcement": if challenge.ruleset.is_some() { "authoritative_v1" } else { "legacy_unranked" },
                    "status": challenge.status,
                }),
            );
            Ok((StatusCode::CREATED, Json(challenge)))
        })
        .await
}

async fn list_operator_challenges(
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
) -> Result<Json<ResearchChallenge>, ApiError> {
    get_challenge_record(&state, challenge_id).await
}

async fn get_operator_challenge(
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
    get_challenge_record(&state, challenge_id).await
}

async fn get_challenge_record(
    state: &AppState,
    challenge_id: Uuid,
) -> Result<Json<ResearchChallenge>, ApiError> {
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

fn validate_raw_sha256_hex(field: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "{field} must contain exactly 64 lowercase hexadecimal characters"
        ));
    }
    Ok(())
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

fn decode_public_key_ring_env(field: &str) -> Result<HashMap<String, [u8; 32]>, String> {
    let raw = match std::env::var(field) {
        Ok(raw) if !raw.trim().is_empty() => raw,
        _ => return Ok(HashMap::new()),
    };
    let encoded: HashMap<String, String> =
        serde_json::from_str(&raw).map_err(|error| format!("decode {field}: {error}"))?;
    if encoded.is_empty() {
        return Err(format!("{field} must not be an empty JSON object"));
    }
    encoded
        .into_iter()
        .map(|(key_id, value)| {
            validate_contract_text(field, &key_id)?;
            let key = decode_canonical_base64_exact::<32>(field, &value)?;
            VerifyingKey::from_bytes(&key)
                .map_err(|_| format!("{field} key {key_id} is not valid Ed25519"))?;
            Ok((key_id, key))
        })
        .collect()
}

fn merge_legacy_public_key(
    keys: &mut HashMap<String, [u8; 32]>,
    key_id: &str,
    public_key: [u8; 32],
    ring_field: &str,
) -> Result<(), String> {
    if let Some(existing) = keys.get(key_id) {
        if existing != &public_key {
            return Err(format!(
                "legacy key {key_id} differs from the same key ID in {ring_field}"
            ));
        }
    } else {
        keys.insert(key_id.to_string(), public_key);
    }
    Ok(())
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

#[cfg(test)]
mod database_role_tests {
    use super::*;
    use sqlx::{Connection, PgConnection};

    const POSTGRES_TEST_LOCK: &str = "hepta-research-league-pg-tests";

    fn is_insufficient_privilege(error: &sqlx::Error) -> bool {
        error
            .as_database_error()
            .and_then(|database| database.code())
            .is_some_and(|code| code.as_ref() == "42501")
    }

    #[tokio::test]
    async fn one_shot_migration_hands_off_to_isolated_runtime_and_finality_pools() {
        let Ok(migration_database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
            eprintln!("HEPTA_TEST_DATABASE_URL unset; database-role PostgreSQL test skipped");
            return;
        };

        let result = exercise_separated_database_roles(&migration_database_url).await;
        assert!(result.is_ok(), "{result:?}");
    }

    async fn exercise_separated_database_roles(migration_database_url: &str) -> Result<(), String> {
        let mut migration_connection = PgConnection::connect(migration_database_url)
            .await
            .map_err(|error| format!("connect role-boundary migration owner: {error}"))?;
        sqlx::query("select pg_advisory_lock(hashtext($1))")
            .bind(POSTGRES_TEST_LOCK)
            .execute(&mut migration_connection)
            .await
            .map_err(|error| format!("lock role-boundary PostgreSQL test: {error}"))?;

        let migration_role: String = sqlx::query_scalar("select current_user")
            .fetch_one(&mut migration_connection)
            .await
            .map_err(|error| format!("inspect role-boundary migration owner: {error}"))?;
        let can_create_runtime_role: bool = sqlx::query_scalar(
            "select rolsuper or rolcreaterole from pg_roles where rolname=current_user",
        )
        .fetch_one(&mut migration_connection)
        .await
        .map_err(|error| format!("inspect role-boundary role-creation authority: {error}"))?;
        if !can_create_runtime_role {
            eprintln!(
                "HEPTA_TEST_DATABASE_URL role {migration_role:?} cannot create an isolated runtime role; separated database-role PostgreSQL test skipped"
            );
            sqlx::query("select pg_advisory_unlock(hashtext($1))")
                .bind(POSTGRES_TEST_LOCK)
                .execute(&mut migration_connection)
                .await
                .map_err(|error| {
                    format!("unlock skipped role-boundary PostgreSQL test: {error}")
                })?;
            return Ok(());
        }

        let suffix = Uuid::new_v4().simple().to_string();
        let runtime_role = format!("hepta_test_runtime_{suffix}");
        let runtime_password = format!("hepta-runtime-{suffix}");
        let finality_role = format!("hepta_test_finality_{suffix}");
        let finality_password = format!("hepta-finality-{suffix}");
        let quoted_runtime_role: String = sqlx::query_scalar("select quote_ident($1)")
            .bind(&runtime_role)
            .fetch_one(&mut migration_connection)
            .await
            .map_err(|error| format!("quote isolated runtime role: {error}"))?;
        let quoted_runtime_password: String = sqlx::query_scalar("select quote_literal($1)")
            .bind(&runtime_password)
            .fetch_one(&mut migration_connection)
            .await
            .map_err(|error| format!("quote isolated runtime password: {error}"))?;
        let quoted_finality_role: String = sqlx::query_scalar("select quote_ident($1)")
            .bind(&finality_role)
            .fetch_one(&mut migration_connection)
            .await
            .map_err(|error| format!("quote isolated finality role: {error}"))?;
        let quoted_finality_password: String = sqlx::query_scalar("select quote_literal($1)")
            .bind(&finality_password)
            .fetch_one(&mut migration_connection)
            .await
            .map_err(|error| format!("quote isolated finality password: {error}"))?;
        for (label, statement) in [
            (
                "runtime",
                format!(
                    "create role {quoted_runtime_role} login password {quoted_runtime_password} \
                     nosuperuser nocreatedb nocreaterole noinherit noreplication nobypassrls"
                ),
            ),
            (
                "finality",
                format!(
                    "create role {quoted_finality_role} login password {quoted_finality_password} \
                     nosuperuser nocreatedb nocreaterole noinherit noreplication nobypassrls"
                ),
            ),
        ] {
            sqlx::raw_sql(&statement)
                .execute(&mut migration_connection)
                .await
                .map_err(|error| format!("create isolated {label} role: {error}"))?;
        }

        let runtime_database_url =
            database_url_for_role(migration_database_url, &runtime_role, &runtime_password);
        let finality_database_url =
            database_url_for_role(migration_database_url, &finality_role, &finality_password);
        let role_boundary_result = match (runtime_database_url, finality_database_url) {
            (Ok(runtime_database_url), Ok(finality_database_url)) => {
                verify_separated_database_roles(
                    &runtime_database_url,
                    &finality_database_url,
                    migration_database_url,
                    &migration_role,
                    &runtime_role,
                    &finality_role,
                )
                .await
            }
            (Err(error), _) | (_, Err(error)) => Err(error),
        };

        let mut cleanup_errors = Vec::new();
        for (label, quoted_role) in [
            ("runtime", &quoted_runtime_role),
            ("finality", &quoted_finality_role),
        ] {
            if let Err(error) = sqlx::raw_sql(&format!("drop owned by {quoted_role}"))
                .execute(&mut migration_connection)
                .await
            {
                cleanup_errors.push(format!("drop isolated {label} grants: {error}"));
            }
            if let Err(error) = sqlx::raw_sql(&format!("drop role {quoted_role}"))
                .execute(&mut migration_connection)
                .await
            {
                cleanup_errors.push(format!("drop isolated {label} role: {error}"));
            }
        }
        if let Err(error) = sqlx::query("select pg_advisory_unlock(hashtext($1))")
            .bind(POSTGRES_TEST_LOCK)
            .execute(&mut migration_connection)
            .await
        {
            cleanup_errors.push(format!("unlock role-boundary PostgreSQL test: {error}"));
        }
        let cleanup_result = if cleanup_errors.is_empty() {
            Ok(())
        } else {
            Err(cleanup_errors.join("; "))
        };

        role_boundary_result.and(cleanup_result)
    }

    fn database_url_for_role(
        migration_database_url: &str,
        runtime_role: &str,
        runtime_password: &str,
    ) -> Result<String, String> {
        let mut runtime_database_url = reqwest::Url::parse(migration_database_url)
            .map_err(|error| format!("parse HEPTA_TEST_DATABASE_URL: {error}"))?;
        runtime_database_url
            .set_username(runtime_role)
            .map_err(|_| "set isolated runtime role in PostgreSQL URL".to_string())?;
        runtime_database_url
            .set_password(Some(runtime_password))
            .map_err(|_| "set isolated runtime password in PostgreSQL URL".to_string())?;
        Ok(runtime_database_url.to_string())
    }

    async fn verify_separated_database_roles(
        runtime_database_url: &str,
        finality_database_url: &str,
        migration_database_url: &str,
        migration_role: &str,
        runtime_role: &str,
        finality_role: &str,
    ) -> Result<(), String> {
        AppState::migrate_and_configure_database_roles(
            migration_database_url,
            runtime_role,
            finality_role,
        )
        .await?;
        crate::paper_raid_v2::endpoint_tests::reset_postgres(migration_database_url).await;
        let mut state = AppState::connect_with_database_roles(
            runtime_database_url,
            finality_database_url,
            crate::paper_raid_v2::endpoint_tests::paper_chain_finality_v2_security(),
        )
        .await?;
        let runtime_pool = state
            .pool
            .as_ref()
            .ok_or_else(|| "role-separated state did not retain a runtime pool".to_string())?;
        let finality_pool = state
            .finality_pool
            .as_ref()
            .ok_or_else(|| "role-separated state did not retain a finality pool".to_string())?;
        let verification_result = async {
            state.probe_database_pools().await.map_err(str::to_string)?;
            verify_runtime_role_pool(runtime_pool, migration_role, runtime_role).await?;
            verify_finality_role_pool(finality_pool, migration_role, finality_role).await?;
            let preparation = crate::paper_raid_v2::endpoint_tests::exercise_paper_chain_finality_v2_preparation(
                state.clone(),
            )
            .await;
            if preparation.status != PaperTrnmFinalityPreparationStatusV2::AwaitingChainVerifierUpgrade {
                return Err(format!(
                    "role-separated checkpoint/arm/preparation flow returned unexpected status {:?}",
                    preparation.status
                ));
            }
            Ok(())
        }
        .await;
        crate::paper_raid_v2::endpoint_tests::reset_postgres(migration_database_url).await;
        let runtime_pool = state
            .pool
            .take()
            .expect("role-separated state has a runtime pool");
        let finality_pool = state
            .finality_pool
            .take()
            .expect("role-separated state has a finality pool");
        runtime_pool.close().await;
        finality_pool.close().await;
        verification_result
    }

    async fn verify_runtime_role_pool(
        runtime_pool: &PgPool,
        migration_role: &str,
        runtime_role: &str,
    ) -> Result<(), String> {
        let retained_role: String = sqlx::query_scalar("select current_user")
            .fetch_one(runtime_pool)
            .await
            .map_err(|error| format!("inspect retained runtime pool: {error}"))?;
        if retained_role != runtime_role || retained_role == migration_role {
            return Err(format!(
                "owner-pool handoff retained role {retained_role:?}, expected isolated runtime role {runtime_role:?} distinct from {migration_role:?}"
            ));
        }

        let role_is_fail_closed: bool = sqlx::query_scalar(
            "select
                not role.rolsuper
                and not role.rolcreatedb
                and not role.rolcreaterole
                and not role.rolreplication
                and not role.rolbypassrls
                and not pg_has_role(role.rolname, $1, 'MEMBER')
                and not exists (
                    select 1
                    from pg_class as relation
                    join pg_namespace as namespace on namespace.oid=relation.relnamespace
                    where namespace.nspname='public'
                      and relation.relowner=role.oid
                )
                and has_schema_privilege(role.rolname, 'public', 'USAGE')
                and not has_schema_privilege(role.rolname, 'public', 'CREATE')
             from pg_roles as role
             where role.rolname=current_user",
        )
        .bind(migration_role)
        .fetch_one(runtime_pool)
        .await
        .map_err(|error| format!("verify isolated runtime role boundary: {error}"))?;
        if !role_is_fail_closed {
            return Err(
                "isolated runtime role attributes or schema boundary are unsafe".to_string(),
            );
        }

        let unsafe_table_privilege_count: i64 = sqlx::query_scalar(
            "select count(*)
             from pg_class as relation
             join pg_namespace as namespace on namespace.oid=relation.relnamespace
             where namespace.nspname='public'
               and relation.relkind in ('r','p')
               and (
                   not has_any_column_privilege(current_user, relation.oid, 'SELECT')
                   or (
                       relation.relname in (
                           'hepta_trnm_cometbft_time_checkpoints_v1',
                           'hepta_paper_chain_finality_window_arms_v2',
                           'hepta_paper_chain_finality_preparations_v2'
                       )
                       and (
                           has_any_column_privilege(current_user, relation.oid, 'INSERT')
                           or has_any_column_privilege(current_user, relation.oid, 'UPDATE')
                           or has_table_privilege(current_user, relation.oid, 'DELETE')
                       )
                   )
                   or (
                       relation.relname not in (
                           'hepta_trnm_cometbft_time_checkpoints_v1',
                           'hepta_paper_chain_finality_window_arms_v2',
                           'hepta_paper_chain_finality_preparations_v2'
                       )
                       and (
                           not has_any_column_privilege(current_user, relation.oid, 'INSERT')
                           or not has_any_column_privilege(current_user, relation.oid, 'UPDATE')
                           or not has_table_privilege(current_user, relation.oid, 'DELETE')
                       )
                   )
                   or has_table_privilege(current_user, relation.oid, 'TRUNCATE')
                   or has_any_column_privilege(current_user, relation.oid, 'REFERENCES')
                   or has_table_privilege(current_user, relation.oid, 'TRIGGER')
               )",
        )
        .fetch_one(runtime_pool)
        .await
        .map_err(|error| format!("verify isolated runtime table privileges: {error}"))?;
        if unsafe_table_privilege_count != 0 {
            return Err(format!(
                "{unsafe_table_privilege_count} public tables violate the runtime/evidence boundary"
            ));
        }

        let unsafe_sequence_privilege_count: i64 = sqlx::query_scalar(
            "select count(*)
             from pg_class as relation
             join pg_namespace as namespace on namespace.oid=relation.relnamespace
             where namespace.nspname='public'
               and relation.relkind='S'
               and (
                   not has_sequence_privilege(current_user, relation.oid, 'USAGE')
                   or not has_sequence_privilege(current_user, relation.oid, 'SELECT')
                   or has_sequence_privilege(current_user, relation.oid, 'UPDATE')
               )",
        )
        .fetch_one(runtime_pool)
        .await
        .map_err(|error| format!("verify isolated runtime sequence privileges: {error}"))?;
        if unsafe_sequence_privilege_count != 0 {
            return Err(format!(
                "{unsafe_sequence_privilege_count} public sequences violate the runtime read/use boundary"
            ));
        }

        let probe_key = format!("role-boundary-{runtime_role}");
        let mut transaction = runtime_pool
            .begin()
            .await
            .map_err(|error| format!("begin runtime DML probe: {error}"))?;
        let inserted = sqlx::query(
            "insert into hepta_league_state (state_key, revision, state_json)
             values ($1, 0, '{}'::jsonb)",
        )
        .bind(&probe_key)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("probe runtime INSERT grant: {error}"))?
        .rows_affected();
        let updated =
            sqlx::query("update hepta_league_state set revision=revision+1 where state_key=$1")
                .bind(&probe_key)
                .execute(&mut *transaction)
                .await
                .map_err(|error| format!("probe runtime UPDATE grant: {error}"))?
                .rows_affected();
        let deleted = sqlx::query("delete from hepta_league_state where state_key=$1")
            .bind(&probe_key)
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("probe runtime DELETE grant: {error}"))?
            .rows_affected();
        transaction
            .rollback()
            .await
            .map_err(|error| format!("rollback runtime DML probe: {error}"))?;
        if (inserted, updated, deleted) != (1, 1, 1) {
            return Err(format!(
                "runtime DML probe affected unexpected rows: insert={inserted}, update={updated}, delete={deleted}"
            ));
        }
        let evidence_error =
            sqlx::query("insert into hepta_trnm_cometbft_time_checkpoints_v1 default values")
                .execute(runtime_pool)
                .await
                .expect_err("runtime role must not insert finality evidence");
        if !is_insufficient_privilege(&evidence_error) {
            return Err(format!(
                "runtime finality-evidence denial returned unexpected error: {evidence_error}"
            ));
        }
        let definer_error =
            sqlx::query("select public.hepta_assert_paper_finality_v2_source_unsealed($1)")
                .bind(Uuid::new_v4())
                .execute(runtime_pool)
                .await
                .expect_err("runtime role must not execute the finality definer capability");
        if !is_insufficient_privilege(&definer_error) {
            return Err(format!(
                "runtime definer-capability denial returned unexpected error: {definer_error}"
            ));
        }
        Ok(())
    }

    async fn verify_finality_role_pool(
        finality_pool: &PgPool,
        migration_role: &str,
        finality_role: &str,
    ) -> Result<(), String> {
        let retained_role: String = sqlx::query_scalar("select current_user")
            .fetch_one(finality_pool)
            .await
            .map_err(|error| format!("inspect retained finality pool: {error}"))?;
        if retained_role != finality_role || retained_role == migration_role {
            return Err(format!(
                "owner-pool handoff retained finality role {retained_role:?}, expected {finality_role:?} distinct from {migration_role:?}"
            ));
        }

        let shared_idempotency_error = sqlx::query(
            "insert into hepta_paper_raid_idempotency (
                operation,idempotency_key,request_hash,aggregate_id,response_status,response_json
             ) values ($1,$2,$3,null,201,'{}'::jsonb)",
        )
        .bind(format!("role-boundary-{finality_role}"))
        .bind(format!("role-boundary-{}", Uuid::new_v4()))
        .bind("0".repeat(64))
        .execute(finality_pool)
        .await
        .expect_err("finality role must not insert shared Paper Raid idempotency rows");
        if !is_insufficient_privilege(&shared_idempotency_error) {
            return Err(format!(
                "finality shared-idempotency denial returned unexpected error: {shared_idempotency_error}"
            ));
        }

        let source_error = sqlx::query(
            "insert into hepta_league_state (state_key,revision,state_json)
             values ($1,0,'{}'::jsonb)",
        )
        .bind(format!("forbidden-{finality_role}"))
        .execute(finality_pool)
        .await
        .expect_err("finality role must not insert ordinary source rows");
        if !is_insufficient_privilege(&source_error) {
            return Err(format!(
                "finality source-write denial returned unexpected error: {source_error}"
            ));
        }
        let update_error = sqlx::query(
            "update hepta_paper_chain_finality_window_arms_v2
             set arm_id=arm_id where false",
        )
        .execute(finality_pool)
        .await
        .expect_err("finality role must not update immutable arms");
        if !is_insufficient_privilege(&update_error) {
            return Err(format!(
                "finality immutable-arm UPDATE denial returned unexpected error: {update_error}"
            ));
        }
        sqlx::query("select public.hepta_assert_paper_finality_v2_source_unsealed($1)")
            .bind(Uuid::new_v4())
            .execute(finality_pool)
            .await
            .map_err(|error| format!("execute finality definer capability: {error}"))?;
        Ok(())
    }
}
