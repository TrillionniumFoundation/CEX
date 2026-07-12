use crate::repository::LedgerRepositoryHandle;
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
    pub game_authority_token: Arc<String>,
    pub player_session_signing_secret: Arc<String>,
    pub require_player_session: bool,
    pub allow_system_economy_operations: bool,
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

        Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
            entries: Arc::new(RwLock::new(Vec::new())),
            idempotency_keys: Arc::new(RwLock::new(HashSet::new())),
            repository,
            fail_fast,
            admin_tokens: Arc::new(admin_tokens),
            entitlement_signing_secret: Arc::new("test-entitlement-secret".to_string()),
            entitlement_key_id: Arc::new("test-entitlement-key".to_string()),
            game_authority_token: Arc::new("test-game-authority-token".to_string()),
            player_session_signing_secret: Arc::new("test-player-session-secret".to_string()),
            require_player_session: false,
            allow_system_economy_operations: true,
        }
    }
}

fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(default)
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
