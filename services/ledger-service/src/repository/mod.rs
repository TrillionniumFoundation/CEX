mod native_economy;
pub mod postgres;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use term_exchange_protocol::{EconomicIntent, EconomicReceipt, WalletSnapshot};
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrnmPlayerIdentityRecord {
    pub player_id: String,
    pub account_id: Uuid,
    pub recovery_generation: i64,
    pub status: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrnmPlayerSessionRecord {
    pub session_id: Uuid,
    pub player_id: String,
    pub account_id: Uuid,
    pub device_id: String,
    pub recovery_generation: i64,
    pub issued_at_epoch: i64,
    pub expires_at_epoch: i64,
}

use crate::state::{AccountRecord, LedgerEntryRecord};

pub type LedgerRepositoryHandle = Arc<dyn LedgerRepository + Send + Sync>;

#[derive(Debug)]
pub enum LedgerActionError {
    RepositoryUnavailable(String),
    AccountNotFound,
    DuplicateIdempotencyKey,
    InsufficientAvailable { available: f64, requested: f64 },
    InsufficientReserved { reserved: f64, requested: f64 },
    IdentityRejected(String),
    Other(String),
}

// Session creation is an atomic persistence boundary and keeps all signed-session fields explicit.
#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait LedgerRepository {
    fn persistence_ready(&self) -> bool {
        false
    }

    async fn persistence_healthy(&self) -> bool {
        false
    }

    async fn create_account(&self, account: &AccountRecord) -> Result<(), String>;
    async fn get_account(&self, account_id: Uuid) -> Result<Option<AccountRecord>, String>;
    async fn append_entry(&self, entry: &LedgerEntryRecord) -> Result<(), String>;
    async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<LedgerEntryRecord>, String>;
    async fn reserve_credits(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError>;
    async fn consume_credits(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError>;
    async fn refund_credits(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError>;
    async fn grant_credits(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError>;

    async fn execute_trnm_economic_intent(
        &self,
        _intent: &EconomicIntent,
    ) -> Result<EconomicReceipt, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM native-economy repository is unavailable".to_string(),
        ))
    }

    async fn reconcile_trnm_wallet(
        &self,
        _actor_id: &str,
        _account_id: Uuid,
        _requested_cursor: u64,
    ) -> Result<WalletSnapshot, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM wallet reconciliation repository is unavailable".to_string(),
        ))
    }

    async fn register_trnm_player_identity(
        &self,
        _player_id: &str,
        _account_id: Uuid,
        _recovery_key: &str,
    ) -> Result<TrnmPlayerIdentityRecord, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM player identity repository is unavailable".to_string(),
        ))
    }

    async fn recover_trnm_player_identity(
        &self,
        _player_id: &str,
        _recovery_key: &str,
        _new_recovery_key: &str,
    ) -> Result<TrnmPlayerIdentityRecord, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM player identity repository is unavailable".to_string(),
        ))
    }

    async fn set_trnm_player_identity_status(
        &self,
        _player_id: &str,
        _status: &str,
    ) -> Result<TrnmPlayerIdentityRecord, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM player identity status repository is unavailable".to_string(),
        ))
    }

    async fn create_trnm_player_session(
        &self,
        _player_id: &str,
        _recovery_key: &str,
        _device_id: &str,
        _session_id: Uuid,
        _token_hash: &str,
        _issued_at_epoch: i64,
        _expires_at_epoch: i64,
    ) -> Result<TrnmPlayerSessionRecord, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM player session repository is unavailable".to_string(),
        ))
    }

    async fn authenticate_trnm_player_identity(
        &self,
        _player_id: &str,
        _recovery_key: &str,
    ) -> Result<TrnmPlayerIdentityRecord, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM player identity repository is unavailable".to_string(),
        ))
    }

    async fn verify_trnm_player_session(
        &self,
        _session_id: Uuid,
        _token_hash: &str,
        _actor_id: &str,
        _account_id: Uuid,
        _recovery_generation: i64,
    ) -> Result<TrnmPlayerSessionRecord, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM player session repository is unavailable".to_string(),
        ))
    }

    async fn revoke_trnm_player_session(
        &self,
        _session_id: Uuid,
        _reason: &str,
    ) -> Result<(), LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM player session repository is unavailable".to_string(),
        ))
    }

    async fn list_trnm_economic_receipts(&self) -> Result<Vec<EconomicReceipt>, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM receipt listing is unavailable".to_string(),
        ))
    }

    async fn maintain_trnm_native_economy(&self) -> Result<Value, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM economy maintenance is unavailable".to_string(),
        ))
    }
}
