mod native_economy;
pub mod postgres;

use async_trait::async_trait;
use std::sync::Arc;
use term_exchange_protocol::{EconomicIntent, EconomicReceipt, WalletSnapshot};
use uuid::Uuid;

use crate::state::{AccountRecord, LedgerEntryRecord};

pub type LedgerRepositoryHandle = Arc<dyn LedgerRepository + Send + Sync>;

#[derive(Debug)]
pub enum LedgerActionError {
    RepositoryUnavailable(String),
    AccountNotFound,
    DuplicateIdempotencyKey,
    InsufficientAvailable { available: f64, requested: f64 },
    InsufficientReserved { reserved: f64, requested: f64 },
    Other(String),
}

#[async_trait]
pub trait LedgerRepository {
    fn persistence_ready(&self) -> bool {
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
}
