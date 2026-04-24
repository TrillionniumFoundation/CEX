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
        }
    }
}

fn load_admin_tokens() -> HashMap<String, AdminPrincipal> {
    build_admin_principal_map(load_ledger_scoped_admin_tokens())
}
