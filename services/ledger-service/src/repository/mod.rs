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

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct PostgresOperationalReadiness {
    pub query_healthy: bool,
    pub pool_saturation_healthy: bool,
    pub pool_max_connections: u32,
    pub pool_size: u32,
    pub pool_idle_connections: usize,
    pub archive_mode_on: bool,
    pub archive_command_configured: bool,
    pub archiver_recovered: bool,
    pub archived_count: i64,
    pub failed_count: i64,
    pub last_archived_wal: Option<String>,
    pub last_failed_wal: Option<String>,
}

impl PostgresOperationalReadiness {
    pub fn ready(&self) -> bool {
        self.query_healthy
            && self.pool_saturation_healthy
            && self.archive_mode_on
            && self.archive_command_configured
            && self.archiver_recovered
    }
}

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

    async fn postgres_operational_readiness(&self) -> PostgresOperationalReadiness {
        PostgresOperationalReadiness::default()
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

    async fn register_trnm_product_player(
        &self,
        _player_id: &str,
        _recovery_key: &str,
        _org_id: Uuid,
        _invite_code: &str,
    ) -> Result<TrnmPlayerIdentityRecord, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM product registration repository is unavailable".to_string(),
        ))
    }

    async fn issue_trnm_product_registration_invite(
        &self,
        _lifetime_seconds: i64,
        _max_uses: i32,
    ) -> Result<Value, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM product invite repository is unavailable".to_string(),
        ))
    }

    async fn submit_trnm_identity_appeal(
        &self,
        _player_id: &str,
        _recovery_key: &str,
        _message: &str,
    ) -> Result<Value, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM identity appeal repository is unavailable".to_string(),
        ))
    }

    async fn resolve_trnm_identity_appeal(
        &self,
        _appeal_id: Uuid,
        _decision: &str,
        _resolution: &str,
    ) -> Result<Value, LedgerActionError> {
        Err(LedgerActionError::RepositoryUnavailable(
            "TRNM identity appeal resolution repository is unavailable".to_string(),
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

#[cfg(test)]
mod tests {
    use super::PostgresOperationalReadiness;

    fn green_readiness() -> PostgresOperationalReadiness {
        PostgresOperationalReadiness {
            query_healthy: true,
            pool_saturation_healthy: true,
            pool_max_connections: 8,
            pool_size: 4,
            pool_idle_connections: 1,
            archive_mode_on: true,
            archive_command_configured: true,
            archiver_recovered: true,
            archived_count: 10,
            failed_count: 1,
            last_archived_wal: Some("00000001000000000000000A".to_string()),
            last_failed_wal: Some("000000010000000000000009".to_string()),
        }
    }

    #[test]
    fn postgres_operational_readiness_fails_closed_on_each_required_signal() {
        assert!(green_readiness().ready());

        let mut status = green_readiness();
        status.query_healthy = false;
        assert!(!status.ready());

        let mut status = green_readiness();
        status.pool_saturation_healthy = false;
        assert!(!status.ready());

        let mut status = green_readiness();
        status.archive_mode_on = false;
        assert!(!status.ready());

        let mut status = green_readiness();
        status.archive_command_configured = false;
        assert!(!status.ready());

        let mut status = green_readiness();
        status.archiver_recovered = false;
        assert!(!status.ready());
    }
}
