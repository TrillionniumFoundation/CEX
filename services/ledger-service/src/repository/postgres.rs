use async_trait::async_trait;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    repository::{LedgerActionError, LedgerRepository, LedgerRepositoryHandle},
    state::{AccountRecord, LedgerEntryRecord},
};

pub struct PostgresLedgerRepository {
    pub database_url: Option<String>,
    pub pool: Option<PgPool>,
}

impl PostgresLedgerRepository {
    pub fn new_placeholder() -> LedgerRepositoryHandle {
        Arc::new(Self {
            database_url: std::env::var("DATABASE_URL").ok(),
            pool: None,
        })
    }

    pub async fn connect_from_env() -> Result<Self, String> {
        let database_url =
            std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is not set".to_string())?;

        let pool = PgPool::connect(&database_url)
            .await
            .map_err(|e| format!("failed to connect postgres: {e}"))?;

        Ok(Self {
            database_url: Some(database_url),
            pool: Some(pool),
        })
    }

    fn pool_or_unavailable(&self) -> Result<&PgPool, LedgerActionError> {
        self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })
    }

    async fn load_account_for_update(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        account_id: Uuid,
    ) -> Result<AccountRecord, LedgerActionError> {
        let row = sqlx::query(
            "select account_id, org_id::text as org_id, account_type, currency_unit, balance::float8 as balance, reserved::float8 as reserved from accounts where account_id = $1 for update"
        )
        .bind(account_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| LedgerActionError::Other(format!("load account for update failed: {e}")))?;

        match row {
            Some(row) => Ok(AccountRecord {
                account_id: row
                    .try_get("account_id")
                    .map_err(|e| LedgerActionError::Other(e.to_string()))?,
                org_id: row
                    .try_get("org_id")
                    .map_err(|e| LedgerActionError::Other(e.to_string()))?,
                account_type: row
                    .try_get("account_type")
                    .map_err(|e| LedgerActionError::Other(e.to_string()))?,
                currency_unit: row
                    .try_get("currency_unit")
                    .map_err(|e| LedgerActionError::Other(e.to_string()))?,
                balance: row
                    .try_get("balance")
                    .map_err(|e| LedgerActionError::Other(e.to_string()))?,
                reserved: row
                    .try_get("reserved")
                    .map_err(|e| LedgerActionError::Other(e.to_string()))?,
            }),
            None => Err(LedgerActionError::AccountNotFound),
        }
    }

    async fn check_duplicate_idempotency(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        idempotency_key: Option<&str>,
    ) -> Result<(), LedgerActionError> {
        let Some(key) = idempotency_key else {
            return Ok(());
        };

        let row = sqlx::query("select entry_id from ledger_entries where idempotency_key = $1")
            .bind(key)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|e| LedgerActionError::Other(format!("idempotency check failed: {e}")))?;

        if row.is_some() {
            return Err(LedgerActionError::DuplicateIdempotencyKey);
        }

        Ok(())
    }

    async fn update_account_summary(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        account: &AccountRecord,
    ) -> Result<(), LedgerActionError> {
        sqlx::query("update accounts set balance = $2, reserved = $3 where account_id = $1")
            .bind(account.account_id)
            .bind(account.balance)
            .bind(account.reserved)
            .execute(&mut **tx)
            .await
            .map_err(|e| LedgerActionError::Other(format!("update account summary failed: {e}")))?;

        Ok(())
    }

    async fn append_entry_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        entry: &LedgerEntryRecord,
    ) -> Result<(), LedgerActionError> {
        let direction = if entry.action == "refund" || entry.action == "grant" {
            "credit"
        } else {
            "debit"
        };
        let reason = entry.action.clone();
        let reference_id = entry
            .reference_id
            .as_deref()
            .and_then(|raw| Uuid::parse_str(raw).ok());

        sqlx::query(
            "insert into ledger_entries (entry_id, account_id, direction, amount, reason, reference_type, reference_id, idempotency_key) values ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(entry.entry_id)
        .bind(entry.account_id)
        .bind(direction)
        .bind(entry.amount)
        .bind(reason)
        .bind("invocation")
        .bind(reference_id)
        .bind(&entry.idempotency_key)
        .execute(&mut **tx)
        .await
        .map_err(|e| LedgerActionError::Other(format!("append entry in tx failed: {e}")))?;

        Ok(())
    }

    pub async fn reserve_transaction_skeleton(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError> {
        let pool = self.pool_or_unavailable()?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| LedgerActionError::Other(format!("begin tx failed: {e}")))?;

        Self::check_duplicate_idempotency(&mut tx, entry.idempotency_key.as_deref()).await?;

        let mut account = Self::load_account_for_update(&mut tx, entry.account_id).await?;
        let available = account.balance - account.reserved;
        if available + 1e-9 < entry.amount {
            return Err(LedgerActionError::InsufficientAvailable {
                available,
                requested: entry.amount,
            });
        }

        account.reserved += entry.amount;
        Self::update_account_summary(&mut tx, &account).await?;
        Self::append_entry_in_tx(&mut tx, entry).await?;

        tx.commit()
            .await
            .map_err(|e| LedgerActionError::Other(format!("commit tx failed: {e}")))?;

        Ok(account)
    }

    pub async fn consume_transaction_skeleton(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError> {
        let pool = self.pool_or_unavailable()?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| LedgerActionError::Other(format!("begin tx failed: {e}")))?;

        Self::check_duplicate_idempotency(&mut tx, entry.idempotency_key.as_deref()).await?;

        let mut account = Self::load_account_for_update(&mut tx, entry.account_id).await?;
        if account.reserved + 1e-9 < entry.amount {
            return Err(LedgerActionError::InsufficientReserved {
                reserved: account.reserved,
                requested: entry.amount,
            });
        }

        account.reserved -= entry.amount;
        account.balance -= entry.amount;
        Self::update_account_summary(&mut tx, &account).await?;
        Self::append_entry_in_tx(&mut tx, entry).await?;

        tx.commit()
            .await
            .map_err(|e| LedgerActionError::Other(format!("commit tx failed: {e}")))?;

        Ok(account)
    }

    pub async fn refund_transaction_skeleton(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError> {
        let pool = self.pool_or_unavailable()?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| LedgerActionError::Other(format!("begin tx failed: {e}")))?;

        Self::check_duplicate_idempotency(&mut tx, entry.idempotency_key.as_deref()).await?;

        let mut account = Self::load_account_for_update(&mut tx, entry.account_id).await?;
        if account.reserved + 1e-9 < entry.amount {
            return Err(LedgerActionError::InsufficientReserved {
                reserved: account.reserved,
                requested: entry.amount,
            });
        }
        account.reserved -= entry.amount;
        Self::update_account_summary(&mut tx, &account).await?;
        Self::append_entry_in_tx(&mut tx, entry).await?;

        tx.commit()
            .await
            .map_err(|e| LedgerActionError::Other(format!("commit tx failed: {e}")))?;

        Ok(account)
    }

    pub async fn grant_transaction_skeleton(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError> {
        let pool = self.pool_or_unavailable()?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| LedgerActionError::Other(format!("begin tx failed: {e}")))?;

        Self::check_duplicate_idempotency(&mut tx, entry.idempotency_key.as_deref()).await?;

        let mut account = Self::load_account_for_update(&mut tx, entry.account_id).await?;
        account.balance += entry.amount;
        Self::update_account_summary(&mut tx, &account).await?;
        Self::append_entry_in_tx(&mut tx, entry).await?;

        tx.commit()
            .await
            .map_err(|e| LedgerActionError::Other(format!("commit tx failed: {e}")))?;

        Ok(account)
    }
}

#[async_trait]
impl LedgerRepository for PostgresLedgerRepository {
    async fn create_account(&self, account: &AccountRecord) -> Result<(), String> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| "postgres pool not initialized".to_string())?;

        sqlx::query(
            "insert into accounts (account_id, org_id, account_type, currency_unit, balance, reserved, status) values ($1, $2::uuid, $3, $4, $5, $6, 'active')"
        )
        .bind(account.account_id)
        .bind(&account.org_id)
        .bind(&account.account_type)
        .bind(&account.currency_unit)
        .bind(account.balance)
        .bind(account.reserved)
        .execute(pool)
        .await
        .map_err(|e| format!("create_account failed: {e}"))?;

        Ok(())
    }

    async fn get_account(&self, account_id: Uuid) -> Result<Option<AccountRecord>, String> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| "postgres pool not initialized".to_string())?;

        let row = sqlx::query(
            "select account_id, org_id::text as org_id, account_type, currency_unit, balance::float8 as balance, reserved::float8 as reserved from accounts where account_id = $1"
        )
        .bind(account_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("get_account failed: {e}"))?;

        match row {
            Some(row) => Ok(Some(AccountRecord {
                account_id: row.try_get("account_id").map_err(|e| e.to_string())?,
                org_id: row.try_get("org_id").map_err(|e| e.to_string())?,
                account_type: row.try_get("account_type").map_err(|e| e.to_string())?,
                currency_unit: row.try_get("currency_unit").map_err(|e| e.to_string())?,
                balance: row.try_get("balance").map_err(|e| e.to_string())?,
                reserved: row.try_get("reserved").map_err(|e| e.to_string())?,
            })),
            None => Ok(None),
        }
    }

    async fn append_entry(&self, entry: &LedgerEntryRecord) -> Result<(), String> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| "postgres pool not initialized".to_string())?;

        let direction = if entry.action == "refund" {
            "credit"
        } else {
            "debit"
        };
        let reason = entry.action.clone();
        let reference_id = entry
            .reference_id
            .as_deref()
            .and_then(|raw| Uuid::parse_str(raw).ok());

        sqlx::query(
            "insert into ledger_entries (entry_id, account_id, direction, amount, reason, reference_type, reference_id, idempotency_key) values ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(entry.entry_id)
        .bind(entry.account_id)
        .bind(direction)
        .bind(entry.amount)
        .bind(reason)
        .bind("invocation")
        .bind(reference_id)
        .bind(&entry.idempotency_key)
        .execute(pool)
        .await
        .map_err(|e| format!("append_entry failed: {e}"))?;

        Ok(())
    }

    async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<LedgerEntryRecord>, String> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| "postgres pool not initialized".to_string())?;

        let row = sqlx::query(
            "select entry_id, account_id, reason, amount::float8 as amount, reference_id::text as reference_id, idempotency_key from ledger_entries where idempotency_key = $1"
        )
        .bind(idempotency_key)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("find_by_idempotency_key failed: {e}"))?;

        match row {
            Some(row) => Ok(Some(LedgerEntryRecord {
                entry_id: row.try_get("entry_id").map_err(|e| e.to_string())?,
                account_id: row.try_get("account_id").map_err(|e| e.to_string())?,
                action: row.try_get("reason").map_err(|e| e.to_string())?,
                amount: row.try_get("amount").map_err(|e| e.to_string())?,
                reference_id: row.try_get("reference_id").map_err(|e| e.to_string())?,
                idempotency_key: row.try_get("idempotency_key").map_err(|e| e.to_string())?,
            })),
            None => Ok(None),
        }
    }

    async fn reserve_credits(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError> {
        self.reserve_transaction_skeleton(entry).await
    }

    async fn consume_credits(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError> {
        self.consume_transaction_skeleton(entry).await
    }

    async fn refund_credits(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError> {
        self.refund_transaction_skeleton(entry).await
    }

    async fn grant_credits(
        &self,
        entry: &LedgerEntryRecord,
    ) -> Result<AccountRecord, LedgerActionError> {
        self.grant_transaction_skeleton(entry).await
    }
}
