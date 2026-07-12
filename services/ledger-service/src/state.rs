use crate::repository::LedgerRepositoryHandle;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use shared_config::{build_admin_principal_map, load_ledger_scoped_admin_tokens, AdminPrincipal};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecord {
    pub account_id: Uuid,
    pub org_id: String,
    pub account_type: String,
    pub currency_unit: String,
    pub balance: f64,
    pub reserved: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntryRecord {
    pub entry_id: Uuid,
    pub account_id: Uuid,
    pub action: String,
    pub amount: f64,
    pub reference_id: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitlementIssuerKey {
    pub issuer: String,
    pub public_key_base64: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntitlementIssuerRegistry {
    keys: HashMap<String, EntitlementIssuerKey>,
}

#[derive(Clone)]
pub struct AppState {
    pub accounts: Arc<RwLock<HashMap<Uuid, AccountRecord>>>,
    pub entries: Arc<RwLock<Vec<LedgerEntryRecord>>>,
    pub idempotency_keys: Arc<RwLock<HashSet<String>>>,
    pub repository: LedgerRepositoryHandle,
    pub fail_fast: bool,
    pub admin_tokens: Arc<HashMap<String, AdminPrincipal>>,
    pub entitlement_signing_secret: Arc<String>,
    pub entitlement_key_id: Arc<String>,
    pub entitlement_issuer_keys: Arc<HashMap<String, EntitlementIssuerKey>>,
    pub game_authority_token: Arc<String>,
    pub player_session_signing_secret: Arc<String>,
    pub require_player_session: bool,
    pub allow_system_economy_operations: bool,
    pub product_org_id: Uuid,
}

impl AppState {
    pub fn new(repository: LedgerRepositoryHandle) -> Self {
        let fail_fast = std::env::var("LEDGER_FAIL_FAST")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
            entries: Arc::new(RwLock::new(Vec::new())),
            idempotency_keys: Arc::new(RwLock::new(HashSet::new())),
            repository,
            fail_fast,
            admin_tokens: Arc::new(load_admin_tokens()),
            entitlement_signing_secret: Arc::new(required_secret(
                "TRNM_VALUE_ENTITLEMENT_SIGNING_SECRET",
                fail_fast,
                "local-development-entitlement-secret-change-me",
            )),
            entitlement_key_id: Arc::new(
                std::env::var("TRNM_VALUE_ENTITLEMENT_KEY_ID")
                    .unwrap_or_else(|_| "trnm-local-entitlement-v1".to_string()),
            ),
            entitlement_issuer_keys: Arc::new(load_entitlement_issuer_registry(fail_fast)),
            game_authority_token: Arc::new(required_secret(
                "TRNM_GAME_AUTHORITY_TOKEN",
                fail_fast,
                "local-development-game-authority-token",
            )),
            player_session_signing_secret: Arc::new(required_secret(
                "TRNM_PLAYER_SESSION_SIGNING_SECRET",
                fail_fast,
                "local-development-player-session-secret-change-me",
            )),
            require_player_session: env_flag("TRNM_REQUIRE_PLAYER_SESSION", fail_fast),
            allow_system_economy_operations: env_flag(
                "TRNM_ALLOW_SYSTEM_ECONOMY_OPERATIONS",
                false,
            ),
            product_org_id: required_uuid(
                "TRNM_PRODUCT_ORG_ID",
                "00000000-0000-0000-0000-00000000ce01",
            ),
        }
    }

    pub fn new_for_tests(
        repository: LedgerRepositoryHandle,
        fail_fast: bool,
        admin_token: Option<String>,
        scopes: Vec<String>,
        org_ids: Vec<String>,
    ) -> Self {
        let admin_tokens = admin_token
            .into_iter()
            .map(|token| {
                (
                    token,
                    AdminPrincipal {
                        actor_id: "test-ledger-admin".to_string(),
                        actor_label: Some("Test Ledger Admin".to_string()),
                        scopes: scopes.clone(),
                        org_ids: org_ids.clone(),
                    },
                )
            })
            .collect();

        let test_signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let entitlement_issuer_keys = HashMap::from([(
            "test-online-ed25519-v1".to_string(),
            EntitlementIssuerKey {
                issuer: "trnm-online-game-server".to_string(),
                public_key_base64: STANDARD.encode(test_signing_key.verifying_key().to_bytes()),
                status: "active".to_string(),
            },
        )]);
        Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
            entries: Arc::new(RwLock::new(Vec::new())),
            idempotency_keys: Arc::new(RwLock::new(HashSet::new())),
            repository,
            fail_fast,
            admin_tokens: Arc::new(admin_tokens),
            entitlement_signing_secret: Arc::new("test-entitlement-secret".to_string()),
            entitlement_key_id: Arc::new("test-entitlement-key".to_string()),
            entitlement_issuer_keys: Arc::new(entitlement_issuer_keys),
            game_authority_token: Arc::new("test-game-authority-token".to_string()),
            player_session_signing_secret: Arc::new("test-player-session-secret".to_string()),
            require_player_session: false,
            allow_system_economy_operations: true,
            product_org_id: Uuid::parse_str("00000000-0000-0000-0000-00000000ce01")
                .expect("test product org UUID"),
        }
    }
}

fn load_entitlement_issuer_registry(fail_fast: bool) -> HashMap<String, EntitlementIssuerKey> {
    let path = std::env::var("TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let Some(path) = path else {
        if fail_fast {
            panic!("TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH is required");
        }
        return HashMap::new();
    };
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read entitlement issuer registry {path}: {error}"));
    let registry: EntitlementIssuerRegistry = serde_json::from_str(&content)
        .unwrap_or_else(|error| panic!("decode entitlement issuer registry {path}: {error}"));
    for (key_id, key) in &registry.keys {
        let decoded = STANDARD
            .decode(&key.public_key_base64)
            .unwrap_or_else(|error| panic!("decode entitlement public key {key_id}: {error}"));
        assert_eq!(
            decoded.len(),
            32,
            "entitlement public key {key_id} must be 32 bytes"
        );
        assert!(
            matches!(key.status.as_str(), "active" | "revoked"),
            "entitlement issuer key {key_id} status must be active or revoked"
        );
        assert!(
            !key.issuer.trim().is_empty(),
            "entitlement issuer is required"
        );
    }
    if fail_fast && !registry.keys.values().any(|key| key.status == "active") {
        panic!("entitlement issuer registry requires at least one active key");
    }
    registry.keys
}

fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(default)
}

fn required_uuid(name: &str, default: &str) -> Uuid {
    let value = std::env::var(name).unwrap_or_else(|_| default.to_string());
    Uuid::parse_str(&value).unwrap_or_else(|_| panic!("{name} must be a UUID"))
}

fn required_secret(name: &str, fail_fast: bool, development_fallback: &str) -> String {
    match std::env::var(name).ok().filter(|value| value.len() >= 24) {
        Some(value) => value,
        None if fail_fast => panic!("{name} must be configured with at least 24 characters"),
        None => development_fallback.to_string(),
    }
}

fn load_admin_tokens() -> HashMap<String, AdminPrincipal> {
    build_admin_principal_map(load_ledger_scoped_admin_tokens())
}
