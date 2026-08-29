use crate::contract::{
    stable_receipt_id, stable_settlement_reference, EconomicIntent, EconomicReceipt, ReceiptStatus,
    SettlementBackendKind, SETTLEMENT_BACKEND_ID, SETTLEMENT_RECEIPT_LOOKUP_CONTRACT,
    TERM_EXCHANGE_PROTOCOL_VERSION,
};
use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgRow, Executor, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

const MIGRATION: &str = include_str!("../migrations/settlement_v1.sql");
const INTENT_LOCK_SALT: i64 = 0x5452_4e4d_4345_5853;

#[derive(Clone, Debug)]
pub enum SettlementPlan {
    ReleaseReward {
        account_id: Uuid,
        amount_credits: i64,
        budget_day: i32,
    },
    CompleteContract,
}

#[derive(Clone)]
pub struct SettlementRepository {
    pool: PgPool,
}

#[derive(Debug)]
pub enum RepositoryError {
    Conflict,
    AccountNotFound,
    AccountInactive,
    AccountCurrencyMismatch,
    DailyRewardLimit,
    StoredReceiptCorrupt(String),
    Database(String),
}

#[derive(Debug)]
struct StoredSettlementReceipt {
    intent_hash: String,
    intent_bytes: Vec<u8>,
    receipt: EconomicReceipt,
}

impl SettlementRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn apply_migration(&self) -> Result<(), RepositoryError> {
        self.pool.execute(MIGRATION).await.map_err(database_error)?;
        Ok(())
    }

    pub async fn schema_ready(&self) -> bool {
        let result = sqlx::query_scalar::<_, bool>(
            "select
                pg_catalog.to_regclass('public.trnm_economy_settlement_receipts_v1') is not null
                and pg_catalog.to_regclass('public.trnm_economy_reward_budget_v1') is not null
                and pg_catalog.to_regprocedure(
                    'public.cex_apply_ledger_effect_v1(uuid,uuid,uuid,text,bigint,smallint,text,uuid,text,text,text,text,text)'
                ) is not null
                and exists (
                    select 1
                      from pg_catalog.pg_attribute
                     where attrelid = pg_catalog.to_regclass('public.accounts')
                       and attname in ('currency_scale', 'balance_minor', 'reserved_minor')
                       and not attisdropped
                     having count(*) = 3
                )
                and exists (
                    select 1
                      from pg_catalog.pg_attribute
                     where attrelid = pg_catalog.to_regclass('public.ledger_entries')
                       and attname in (
                           'currency_scale', 'amount_minor', 'trace_id',
                           'operation_id', 'operation_kind', 'idempotency_scope',
                           'source_service', 'source_principal', 'schema_version',
                           'provenance_mode', 'request_fingerprint'
                       )
                       and not attisdropped
                     having count(*) = 11
                )",
        )
        .fetch_one(&self.pool)
        .await;
        matches!(result, Ok(true))
    }

    pub async fn postgres_ready(&self) -> bool {
        sqlx::query_scalar::<_, i32>("select 1")
            .fetch_one(&self.pool)
            .await
            .is_ok()
    }

    pub async fn lookup(
        &self,
        intent_id: &str,
    ) -> Result<Option<(String, EconomicReceipt)>, RepositoryError> {
        let row = sqlx::query(
            "select intent_id, intent_hash, intent_bytes, intent_json, receipt_json
               from public.trnm_economy_settlement_receipts_v1
              where intent_id = $1",
        )
        .bind(intent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;

        row.map(decode_stored_receipt)
            .transpose()
            .map(|stored| stored.map(|value| (value.intent_hash, value.receipt)))
    }

    pub async fn submit(
        &self,
        intent: &EconomicIntent,
        intent_hash: &str,
        authority_id: &str,
        plan: SettlementPlan,
    ) -> Result<EconomicReceipt, RepositoryError> {
        let intent_bytes = serde_json::to_vec(intent).map_err(|error| {
            RepositoryError::StoredReceiptCorrupt(format!("encode exact durable intent: {error}"))
        })?;
        let computed_hash = format!("{:x}", Sha256::digest(&intent_bytes));
        if computed_hash != intent_hash {
            return Err(RepositoryError::Conflict);
        }
        let intent_json =
            serde_json::from_slice::<serde_json::Value>(&intent_bytes).map_err(|error| {
                RepositoryError::StoredReceiptCorrupt(format!(
                    "decode exact durable intent projection: {error}"
                ))
            })?;

        let mut transaction = self.pool.begin().await.map_err(database_error)?;
        acquire_intent_lock(&mut transaction, &intent.intent_id).await?;

        if let Some(existing) = load_existing(&mut transaction, &intent.intent_id).await? {
            if existing.intent_hash != intent_hash || existing.intent_bytes != intent_bytes {
                return Err(RepositoryError::Conflict);
            }
            transaction.commit().await.map_err(database_error)?;
            return Ok(existing.receipt);
        }

        let (status, account_id, ledger_entry_id) = match plan {
            SettlementPlan::ReleaseReward {
                account_id,
                amount_credits,
                budget_day,
            } => {
                let ledger_entry_id = apply_release_reward(
                    &mut transaction,
                    account_id,
                    amount_credits,
                    budget_day,
                    intent,
                    intent_hash,
                    authority_id,
                )
                .await?;
                (
                    ReceiptStatus::ApprovedRelease,
                    Some(account_id),
                    Some(ledger_entry_id),
                )
            }
            SettlementPlan::CompleteContract => (ReceiptStatus::Settled, None, None),
        };

        let receipt = EconomicReceipt {
            protocol_version: TERM_EXCHANGE_PROTOCOL_VERSION.to_string(),
            receipt_id: stable_receipt_id(intent_hash),
            intent_id: intent.intent_id.clone(),
            term_id: intent.term_id.clone(),
            backend_id: SETTLEMENT_BACKEND_ID.to_string(),
            backend_kind: SettlementBackendKind::Cex,
            status,
            progression_class: status.progression_class(),
            settlement_reference: Some(stable_settlement_reference(intent_hash)),
            ledger_entry_id: ledger_entry_id.map(|entry_id| entry_id.to_string()),
            reason: None,
            evidence: json!({
                "authority_id": authority_id,
                "contract_version": SETTLEMENT_RECEIPT_LOOKUP_CONTRACT,
                "intent_hash": intent_hash,
            }),
            finalized_at_epoch: Utc::now().timestamp(),
        };

        let receipt_json = serde_json::to_value(&receipt).map_err(|error| {
            RepositoryError::StoredReceiptCorrupt(format!("encode durable receipt: {error}"))
        })?;

        sqlx::query(
            "insert into public.trnm_economy_settlement_receipts_v1 (
                intent_id, intent_hash, intent_bytes, intent_json, receipt_id, receipt_json,
                authority_id, account_id, ledger_entry_id
             ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(&intent.intent_id)
        .bind(intent_hash)
        .bind(intent_bytes)
        .bind(intent_json)
        .bind(&receipt.receipt_id)
        .bind(receipt_json)
        .bind(authority_id)
        .bind(account_id)
        .bind(ledger_entry_id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;

        transaction.commit().await.map_err(database_error)?;
        Ok(receipt)
    }
}

async fn acquire_intent_lock(
    transaction: &mut Transaction<'_, Postgres>,
    intent_id: &str,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "select pg_catalog.pg_advisory_xact_lock(
            pg_catalog.hashtextextended($1, $2)
         )",
    )
    .bind(intent_id)
    .bind(INTENT_LOCK_SALT)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

async fn load_existing(
    transaction: &mut Transaction<'_, Postgres>,
    intent_id: &str,
) -> Result<Option<StoredSettlementReceipt>, RepositoryError> {
    let row = sqlx::query(
        "select intent_id, intent_hash, intent_bytes, intent_json, receipt_json
           from public.trnm_economy_settlement_receipts_v1
          where intent_id = $1
          for update",
    )
    .bind(intent_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(database_error)?;

    row.map(decode_stored_receipt).transpose()
}

fn decode_stored_receipt(row: PgRow) -> Result<StoredSettlementReceipt, RepositoryError> {
    let intent_id = row
        .try_get::<String, _>("intent_id")
        .map_err(|error| RepositoryError::StoredReceiptCorrupt(error.to_string()))?;
    let intent_hash = row
        .try_get::<String, _>("intent_hash")
        .map_err(|error| RepositoryError::StoredReceiptCorrupt(error.to_string()))?;
    let intent_bytes = row
        .try_get::<Vec<u8>, _>("intent_bytes")
        .map_err(|error| RepositoryError::StoredReceiptCorrupt(error.to_string()))?;
    let intent_json = row
        .try_get::<serde_json::Value, _>("intent_json")
        .map_err(|error| RepositoryError::StoredReceiptCorrupt(error.to_string()))?;
    let receipt_json = row
        .try_get::<serde_json::Value, _>("receipt_json")
        .map_err(|error| RepositoryError::StoredReceiptCorrupt(error.to_string()))?;

    let computed_hash = format!("{:x}", Sha256::digest(&intent_bytes));
    if computed_hash != intent_hash {
        return Err(RepositoryError::StoredReceiptCorrupt(
            "stored intent bytes do not match intent_hash".to_string(),
        ));
    }

    let decoded_intent_value =
        serde_json::from_slice::<serde_json::Value>(&intent_bytes).map_err(|error| {
            RepositoryError::StoredReceiptCorrupt(format!(
                "decode stored exact intent bytes: {error}"
            ))
        })?;
    if decoded_intent_value != intent_json {
        return Err(RepositoryError::StoredReceiptCorrupt(
            "stored intent bytes and JSON projection diverge".to_string(),
        ));
    }
    let intent =
        serde_json::from_value::<EconomicIntent>(decoded_intent_value).map_err(|error| {
            RepositoryError::StoredReceiptCorrupt(format!("decode stored economic intent: {error}"))
        })?;
    if intent.intent_id != intent_id {
        return Err(RepositoryError::StoredReceiptCorrupt(
            "stored intent identity diverges from primary key".to_string(),
        ));
    }

    let receipt = serde_json::from_value::<EconomicReceipt>(receipt_json).map_err(|error| {
        RepositoryError::StoredReceiptCorrupt(format!("decode stored receipt: {error}"))
    })?;
    if receipt.intent_id != intent_id
        || receipt.term_id != intent.term_id
        || receipt.receipt_id != stable_receipt_id(&intent_hash)
        || receipt
            .evidence
            .get("intent_hash")
            .and_then(serde_json::Value::as_str)
            != Some(intent_hash.as_str())
    {
        return Err(RepositoryError::StoredReceiptCorrupt(
            "stored receipt is not bound to exact durable intent bytes".to_string(),
        ));
    }

    Ok(StoredSettlementReceipt {
        intent_hash,
        intent_bytes,
        receipt,
    })
}

async fn apply_release_reward(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: Uuid,
    amount_credits: i64,
    budget_day: i32,
    intent: &EconomicIntent,
    intent_hash: &str,
    authority_id: &str,
) -> Result<Uuid, RepositoryError> {
    if amount_credits <= 0 {
        return Err(RepositoryError::Database(
            "reward amount must be positive".to_string(),
        ));
    }
    // Read policy/scale before touching the budget.  The exact Ledger v2
    // function below acquires the authoritative account row lock after its
    // operation/idempotency locks; taking an account lock here would invert
    // that order and permit a replay deadlock with another Ledger caller.
    let account = sqlx::query(
        "select status, currency_unit, currency_scale
           from public.accounts
          where account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(database_error)?
    .ok_or(RepositoryError::AccountNotFound)?;

    let status = account
        .try_get::<String, _>("status")
        .map_err(|error| RepositoryError::Database(error.to_string()))?;
    let currency = account
        .try_get::<String, _>("currency_unit")
        .map_err(|error| RepositoryError::Database(error.to_string()))?;
    if status != "active" {
        return Err(RepositoryError::AccountInactive);
    }
    if currency != "wallet_credits" {
        return Err(RepositoryError::AccountCurrencyMismatch);
    }
    let currency_scale = account
        .try_get::<i16, _>("currency_scale")
        .map_err(|error| RepositoryError::Database(error.to_string()))?;
    if !(0..=6).contains(&currency_scale) {
        return Err(RepositoryError::Database(format!(
            "wallet account has unsupported currency_scale {currency_scale}"
        )));
    }

    sqlx::query(
        "insert into public.trnm_economy_reward_budget_v1 (
            account_id, budget_day, amount_credits
         ) values ($1, $2, 0)
         on conflict (account_id, budget_day) do nothing",
    )
    .bind(account_id)
    .bind(budget_day)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;

    let current = sqlx::query_scalar::<_, i64>(
        "select amount_credits
           from public.trnm_economy_reward_budget_v1
          where account_id = $1 and budget_day = $2
          for update",
    )
    .bind(account_id)
    .bind(budget_day)
    .fetch_one(&mut **transaction)
    .await
    .map_err(database_error)?;

    if current.saturating_add(amount_credits) > crate::contract::BATTLE_WALLET_REWARD_DAILY_CAP {
        return Err(RepositoryError::DailyRewardLimit);
    }

    sqlx::query(
        "update public.trnm_economy_reward_budget_v1
            set amount_credits = amount_credits + $3,
                updated_at = pg_catalog.clock_timestamp()
          where account_id = $1 and budget_day = $2",
    )
    .bind(account_id)
    .bind(budget_day)
    .bind(amount_credits)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;

    // All value mutation is delegated to the exact Ledger v2 command.  The
    // function owns the account lock, integer-unit arithmetic, immutable
    // operation identity and append-only ledger row.  Keeping this call in
    // the same transaction as the reward budget and receipt prevents a
    // response-loss retry from creating a second effect.
    let scale_factor = 10_i64
        .checked_pow(currency_scale as u32)
        .ok_or_else(|| RepositoryError::Database("currency scale overflow".to_string()))?;
    let amount_minor = amount_credits.checked_mul(scale_factor).ok_or_else(|| {
        RepositoryError::Database("reward amount exceeds minor-unit range".to_string())
    })?;
    let trace_id = deterministic_uuid(&format!("trnm-economy-trace:{intent_hash}"));
    let operation_id = deterministic_uuid(&format!("trnm-economy-operation:{intent_hash}"));
    let reference_id = deterministic_uuid(&format!("trnm-economy-reference:{intent_hash}"));

    sqlx::query_scalar::<_, Uuid>(
        "select (
            public.cex_apply_ledger_effect_v1(
                $1, $2, $3, 'grant', $4, $5,
                'trnm_economy_intent', $6, $7, $8,
                'trnm-economy-service', $9, 'explicit'
            ) #>> '{effect,entry_id}'
         )::uuid",
    )
    .bind(account_id)
    .bind(trace_id)
    .bind(operation_id)
    .bind(amount_minor)
    .bind(currency_scale)
    .bind(reference_id)
    .bind(&intent.idempotency_key.scope)
    .bind(&intent.idempotency_key.key)
    .bind(authority_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(database_error)
}

/// Match the database `cex_deterministic_uuid_v1` shaping exactly.  Stable
/// operation/trace/reference IDs are derived from the immutable intent hash,
/// so retries can only replay the same Ledger v2 effect.
fn deterministic_uuid(material: &str) -> Uuid {
    let digest = Sha256::digest(material.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // PostgreSQL's helper overlays hexadecimal positions 13 and 17 with 4/8.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x0f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn database_error(error: sqlx::Error) -> RepositoryError {
    RepositoryError::Database(error.to_string())
}
