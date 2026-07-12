use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use term_exchange_protocol::{
    EconomicIntent, EconomicIntentKind, EconomicReceipt, ReceiptProgressionClass, ReceiptStatus,
    SettlementBackendKind, WalletSnapshot, CEX_SETTLEMENT_BACKEND_ID,
};
use uuid::Uuid;

use super::{postgres::PostgresLedgerRepository, LedgerActionError};
use crate::state::AccountRecord;

impl PostgresLedgerRepository {
    pub(super) async fn execute_trnm_native_intent(
        &self,
        intent: &EconomicIntent,
    ) -> Result<EconomicReceipt, LedgerActionError> {
        intent
            .validate()
            .map_err(|error| LedgerActionError::Other(format!("invalid TRNM intent: {error}")))?;
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let payload = serde_json::to_value(intent)
            .map_err(|error| LedgerActionError::Other(error.to_string()))?;
        let payload_bytes = serde_json::to_vec(&payload)
            .map_err(|error| LedgerActionError::Other(error.to_string()))?;
        let payload_hash = format!("{:x}", Sha256::digest(payload_bytes));
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| db_error("begin TRNM native-economy transaction", error))?;

        if let Some((stored_intent_id, stored_hash, stored_receipt)) =
            sqlx::query_as::<_, (String, String, Option<Value>)>(
                "select i.intent_id, i.payload_hash, r.receipt_json
             from trnm_economic_intents i
             left join trnm_economic_receipts r on r.intent_id = i.intent_id
             where i.intent_id = $1 or (i.idempotency_scope = $2 and i.idempotency_key = $3)
             order by (i.intent_id = $1) desc
             limit 1
             for update of i",
            )
            .bind(&intent.intent_id)
            .bind(&intent.idempotency_key.scope)
            .bind(&intent.idempotency_key.key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| db_error("load TRNM idempotency record", error))?
        {
            if stored_intent_id != intent.intent_id || stored_hash != payload_hash {
                return Err(LedgerActionError::Other(
                    "TRNM idempotency key is already bound to a different payload".to_string(),
                ));
            }
            if let Some(value) = stored_receipt {
                let receipt: EconomicReceipt = serde_json::from_value(value).map_err(|error| {
                    LedgerActionError::Other(format!("decode stored TRNM receipt failed: {error}"))
                })?;
                if receipt.progression_class != ReceiptProgressionClass::RecoverableHold {
                    tx.commit()
                        .await
                        .map_err(|error| db_error("commit TRNM receipt replay", error))?;
                    return Ok(receipt);
                }
            }
        } else {
            sqlx::query(
                "insert into trnm_economic_intents (
                     intent_id, protocol_version, idempotency_scope, idempotency_key,
                     payload_hash, intent_json, status
                 ) values ($1, $2, $3, $4, $5, $6, 'processing')",
            )
            .bind(&intent.intent_id)
            .bind(&intent.protocol_version)
            .bind(&intent.idempotency_key.scope)
            .bind(&intent.idempotency_key.key)
            .bind(&payload_hash)
            .bind(&payload)
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error("insert TRNM economic intent", error))?;
        }

        let account_id = intent_account_id(intent)?;
        let amount = intent.amount_credits.unwrap_or_default();
        let mut receipt = EconomicReceipt::from_intent(
            format!("cex-native-receipt:{}", intent.intent_id),
            intent,
            CEX_SETTLEMENT_BACKEND_ID,
            SettlementBackendKind::Cex,
            ReceiptStatus::FailedLedger,
            Utc::now().timestamp(),
        );
        receipt.settlement_reference = Some(intent.idempotency_key.key.clone());
        receipt.evidence = json!({
            "authority": "cex-ledger-postgres",
            "atomic_intent_receipt": true,
            "payload_hash": payload_hash,
        });

        let result = match intent.kind {
            EconomicIntentKind::ReleaseReward | EconomicIntentKind::CompleteContract => {
                if amount <= 0 {
                    receipt.status = ReceiptStatus::SkippedZeroReward;
                    receipt.progression_class = receipt.status.progression_class();
                    Ok(())
                } else {
                    let mut account = load_account_for_update(&mut tx, account_id).await?;
                    account.balance += amount as f64;
                    update_account(&mut tx, &account).await?;
                    let entry_id = append_native_entry(
                        &mut tx,
                        account_id,
                        "credit",
                        amount,
                        if matches!(intent.kind, EconomicIntentKind::CompleteContract) {
                            "complete_contract"
                        } else {
                            "release_reward"
                        },
                        &intent.idempotency_key.key,
                    )
                    .await?;
                    receipt.status = ReceiptStatus::ApprovedRelease;
                    receipt.progression_class = receipt.status.progression_class();
                    receipt.ledger_entry_id = Some(entry_id.to_string());
                    Ok(())
                }
            }
            EconomicIntentKind::Reserve => {
                if amount <= 0 {
                    receipt.status = ReceiptStatus::SkippedZeroPrice;
                    receipt.progression_class = receipt.status.progression_class();
                    Ok(())
                } else {
                    let mut account = load_account_for_update(&mut tx, account_id).await?;
                    let available = account.balance - account.reserved;
                    if available + 1e-9 < amount as f64 {
                        receipt.status = ReceiptStatus::FailedBadResponse;
                        receipt.progression_class = receipt.status.progression_class();
                        receipt.reason = Some(format!(
                            "insufficient available wallet credits: available={available:.0}, requested={amount}"
                        ));
                        Ok(())
                    } else {
                        account.reserved += amount as f64;
                        update_account(&mut tx, &account).await?;
                        let entry_id = append_native_entry(
                            &mut tx,
                            account_id,
                            "debit",
                            amount,
                            "reserve",
                            &intent.idempotency_key.key,
                        )
                        .await?;
                        receipt.status = ReceiptStatus::Reserved;
                        receipt.progression_class = receipt.status.progression_class();
                        receipt.ledger_entry_id = Some(entry_id.to_string());
                        Ok(())
                    }
                }
            }
            EconomicIntentKind::Settle => {
                open_escrow(&mut tx, intent, account_id, amount, &mut receipt).await
            }
            EconomicIntentKind::Consume => {
                commit_escrow(&mut tx, intent, account_id, &mut receipt).await
            }
            EconomicIntentKind::Refund => {
                refund_or_cancel_escrow(&mut tx, intent, account_id, amount, &mut receipt).await
            }
            EconomicIntentKind::Chargeback => reverse_escrow(&mut tx, intent, &mut receipt).await,
            EconomicIntentKind::Quote | EconomicIntentKind::VerifyReceipt => {
                receipt.status = ReceiptStatus::FailedBadResponse;
                receipt.progression_class = receipt.status.progression_class();
                receipt.reason = Some("quote/verify are read-model operations".to_string());
                Ok(())
            }
        };
        result?;
        persist_native_receipt(&mut tx, intent, &receipt).await?;
        tx.commit()
            .await
            .map_err(|error| db_error("commit TRNM native-economy transaction", error))?;
        Ok(receipt)
    }

    pub(super) async fn reconcile_trnm_native_wallet(
        &self,
        actor_id: &str,
        account_id: Uuid,
        requested_cursor: u64,
    ) -> Result<WalletSnapshot, LedgerActionError> {
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| db_error("begin TRNM wallet reconciliation", error))?;
        let account = load_account_for_update(&mut tx, account_id).await?;
        let cursor = sqlx::query_scalar::<_, i64>(
            "insert into trnm_economy_reconciliation_cursors (actor_id, account_id, cursor)
             values ($1, $2, $3)
             on conflict (actor_id, account_id) do update set
                 cursor = greatest(trnm_economy_reconciliation_cursors.cursor, excluded.cursor),
                 updated_at = now()
             returning cursor",
        )
        .bind(actor_id)
        .bind(account_id)
        .bind(i64::try_from(requested_cursor).unwrap_or(i64::MAX))
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| db_error("persist TRNM reconciliation cursor", error))?;
        tx.commit()
            .await
            .map_err(|error| db_error("commit TRNM wallet reconciliation", error))?;
        Ok(WalletSnapshot {
            account_id: account_id.to_string(),
            available_credits: (account.balance - account.reserved).round() as i64,
            reserved_credits: account.reserved.round() as i64,
            observed_at_cursor: u64::try_from(cursor).unwrap_or_default(),
        })
    }
}

fn intent_account_id(intent: &EconomicIntent) -> Result<Uuid, LedgerActionError> {
    let raw = intent
        .actors
        .first()
        .and_then(|actor| actor.account_id.as_deref())
        .ok_or(LedgerActionError::AccountNotFound)?;
    Uuid::parse_str(raw)
        .map_err(|_| LedgerActionError::Other("TRNM intent account_id must be a UUID".to_string()))
}

fn metadata_string<'a>(intent: &'a EconomicIntent, key: &str) -> Option<&'a str> {
    intent.metadata.get(key).and_then(Value::as_str)
}

fn escrow_metadata(
    intent: &EconomicIntent,
) -> Result<(String, Uuid, Uuid, String, i64), LedgerActionError> {
    let purchase_id = metadata_string(intent, "purchase_id")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| LedgerActionError::Other("escrow purchase_id is required".to_string()))?;
    let buyer = metadata_string(intent, "buyer_account_id")
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| {
            LedgerActionError::Other("escrow buyer_account_id is required".to_string())
        })?;
    let seller = metadata_string(intent, "seller_account_id")
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| {
            LedgerActionError::Other("escrow seller_account_id is required".to_string())
        })?;
    let asset_id = intent
        .assets
        .first()
        .map(|asset| asset.asset_id.clone())
        .ok_or_else(|| LedgerActionError::Other("escrow asset is required".to_string()))?;
    let quantity = intent
        .assets
        .first()
        .map(|asset| asset.quantity)
        .unwrap_or_default();
    Ok((purchase_id.to_string(), buyer, seller, asset_id, quantity))
}

async fn load_account_for_update(
    tx: &mut Transaction<'_, Postgres>,
    account_id: Uuid,
) -> Result<AccountRecord, LedgerActionError> {
    let row = sqlx::query(
        "select account_id, org_id::text as org_id, account_type, currency_unit,
                balance::float8 as balance, reserved::float8 as reserved
         from accounts where account_id = $1 for update",
    )
    .bind(account_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| db_error("load native-economy account", error))?
    .ok_or(LedgerActionError::AccountNotFound)?;
    Ok(AccountRecord {
        account_id: row.try_get("account_id").map_err(row_error)?,
        org_id: row.try_get("org_id").map_err(row_error)?,
        account_type: row.try_get("account_type").map_err(row_error)?,
        currency_unit: row.try_get("currency_unit").map_err(row_error)?,
        balance: row.try_get("balance").map_err(row_error)?,
        reserved: row.try_get("reserved").map_err(row_error)?,
    })
}

async fn update_account(
    tx: &mut Transaction<'_, Postgres>,
    account: &AccountRecord,
) -> Result<(), LedgerActionError> {
    sqlx::query("update accounts set balance = $2, reserved = $3 where account_id = $1")
        .bind(account.account_id)
        .bind(account.balance)
        .bind(account.reserved)
        .execute(&mut **tx)
        .await
        .map_err(|error| db_error("update native-economy account", error))?;
    Ok(())
}

async fn append_native_entry(
    tx: &mut Transaction<'_, Postgres>,
    account_id: Uuid,
    direction: &str,
    amount: i64,
    reason: &str,
    idempotency_key: &str,
) -> Result<Uuid, LedgerActionError> {
    let entry_id = Uuid::new_v4();
    sqlx::query(
        "insert into ledger_entries (
             entry_id, account_id, direction, amount, reason, reference_type, idempotency_key
         ) values ($1, $2, $3, $4, $5, 'trnm_native_economy', $6)",
    )
    .bind(entry_id)
    .bind(account_id)
    .bind(direction)
    .bind(amount as f64)
    .bind(reason)
    .bind(idempotency_key)
    .execute(&mut **tx)
    .await
    .map_err(|error| db_error("append native-economy ledger entry", error))?;
    Ok(entry_id)
}

async fn open_escrow(
    tx: &mut Transaction<'_, Postgres>,
    intent: &EconomicIntent,
    actor_account_id: Uuid,
    amount: i64,
    receipt: &mut EconomicReceipt,
) -> Result<(), LedgerActionError> {
    let (purchase_id, buyer, seller, asset_id, quantity) = escrow_metadata(intent)?;
    if actor_account_id != buyer || buyer == seller || amount <= 0 || quantity <= 0 {
        receipt.status = ReceiptStatus::FailedBadResponse;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some("invalid escrow participants or amount".to_string());
        return Ok(());
    }
    let mut buyer_account = load_account_for_update(tx, buyer).await?;
    if buyer_account.reserved + 1e-9 < amount as f64 {
        receipt.status = ReceiptStatus::FailedLedger;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some("buyer reservation is not available for escrow".to_string());
        return Ok(());
    }
    buyer_account.reserved -= amount as f64;
    buyer_account.balance -= amount as f64;
    update_account(tx, &buyer_account).await?;
    let entry_id = append_native_entry(
        tx,
        buyer,
        "debit",
        amount,
        "escrow_hold",
        &intent.idempotency_key.key,
    )
    .await?;
    sqlx::query(
        "insert into trnm_escrow_trades (
             purchase_id, buyer_account_id, seller_account_id, asset_id, quantity,
             amount, status, reserve_intent_id, settle_intent_id
         ) values ($1, $2, $3, $4, $5, $6, 'held', $7, $8)",
    )
    .bind(&purchase_id)
    .bind(buyer)
    .bind(seller)
    .bind(&asset_id)
    .bind(quantity)
    .bind(amount as f64)
    .bind(metadata_string(intent, "reserve_intent_id").unwrap_or("missing-reserve-intent"))
    .bind(&intent.intent_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| db_error("open TRNM escrow", error))?;
    receipt.status = ReceiptStatus::Settled;
    receipt.progression_class = receipt.status.progression_class();
    receipt.ledger_entry_id = Some(entry_id.to_string());
    receipt.evidence["escrow_status"] = json!("held");
    receipt.evidence["purchase_id"] = json!(purchase_id);
    Ok(())
}

async fn commit_escrow(
    tx: &mut Transaction<'_, Postgres>,
    intent: &EconomicIntent,
    actor_account_id: Uuid,
    receipt: &mut EconomicReceipt,
) -> Result<(), LedgerActionError> {
    let (purchase_id, buyer, seller, _, _) = escrow_metadata(intent)?;
    if actor_account_id != buyer {
        receipt.status = ReceiptStatus::FailedBadResponse;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some("only the bound buyer can consume escrow".to_string());
        return Ok(());
    }
    let row = sqlx::query(
        "select amount::float8 as amount, status from trnm_escrow_trades
         where purchase_id = $1 and buyer_account_id = $2 and seller_account_id = $3
         for update",
    )
    .bind(&purchase_id)
    .bind(buyer)
    .bind(seller)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| db_error("load TRNM escrow for commit", error))?;
    let Some(row) = row else {
        receipt.status = ReceiptStatus::FailedBadResponse;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some("escrow trade does not exist".to_string());
        return Ok(());
    };
    let status: String = row.try_get("status").map_err(row_error)?;
    let amount = row.try_get::<f64, _>("amount").map_err(row_error)?.round() as i64;
    if status == "committed" {
        receipt.status = ReceiptStatus::Consumed;
        receipt.progression_class = receipt.status.progression_class();
        receipt.evidence["escrow_status"] = json!("committed");
        return Ok(());
    }
    if status != "held" {
        receipt.status = ReceiptStatus::FailedBadResponse;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some(format!("escrow cannot commit from status {status}"));
        return Ok(());
    }
    let mut seller_account = load_account_for_update(tx, seller).await?;
    seller_account.balance += amount as f64;
    update_account(tx, &seller_account).await?;
    let entry_id = append_native_entry(
        tx,
        seller,
        "credit",
        amount,
        "escrow_commit",
        &intent.idempotency_key.key,
    )
    .await?;
    sqlx::query(
        "update trnm_escrow_trades set status = 'committed', consume_intent_id = $2,
             updated_at = now() where purchase_id = $1",
    )
    .bind(&purchase_id)
    .bind(&intent.intent_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| db_error("commit TRNM escrow", error))?;
    receipt.status = ReceiptStatus::Consumed;
    receipt.progression_class = receipt.status.progression_class();
    receipt.ledger_entry_id = Some(entry_id.to_string());
    receipt.evidence["escrow_status"] = json!("committed");
    receipt.evidence["purchase_id"] = json!(purchase_id);
    Ok(())
}

async fn refund_or_cancel_escrow(
    tx: &mut Transaction<'_, Postgres>,
    intent: &EconomicIntent,
    actor_account_id: Uuid,
    amount: i64,
    receipt: &mut EconomicReceipt,
) -> Result<(), LedgerActionError> {
    let Some(purchase_id) = metadata_string(intent, "purchase_id") else {
        let mut account = load_account_for_update(tx, actor_account_id).await?;
        if amount <= 0 || account.reserved + 1e-9 < amount as f64 {
            receipt.status = ReceiptStatus::RejectedRefundFailed;
            receipt.progression_class = receipt.status.progression_class();
            receipt.reason = Some("reserved wallet credits are unavailable for refund".to_string());
            return Ok(());
        }
        account.reserved -= amount as f64;
        update_account(tx, &account).await?;
        let entry_id = append_native_entry(
            tx,
            actor_account_id,
            "credit",
            amount,
            "refund_reservation",
            &intent.idempotency_key.key,
        )
        .await?;
        receipt.status = ReceiptStatus::Refunded;
        receipt.progression_class = receipt.status.progression_class();
        receipt.ledger_entry_id = Some(entry_id.to_string());
        return Ok(());
    };
    let row = sqlx::query(
        "select buyer_account_id, amount::float8 as amount, status
         from trnm_escrow_trades where purchase_id = $1 for update",
    )
    .bind(purchase_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| db_error("load TRNM escrow for refund", error))?;
    let Some(row) = row else {
        receipt.status = ReceiptStatus::CancelledRefundFailed;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some("escrow trade does not exist".to_string());
        return Ok(());
    };
    let buyer: Uuid = row.try_get("buyer_account_id").map_err(row_error)?;
    let escrow_amount = row.try_get::<f64, _>("amount").map_err(row_error)?.round() as i64;
    let status: String = row.try_get("status").map_err(row_error)?;
    if status == "refunded" {
        receipt.status = ReceiptStatus::Refunded;
        receipt.progression_class = receipt.status.progression_class();
        receipt.evidence["escrow_status"] = json!("refunded");
        return Ok(());
    }
    if status != "held" {
        receipt.status = ReceiptStatus::CancelledRefundFailed;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some(format!("escrow cannot refund from status {status}"));
        return Ok(());
    }
    let mut buyer_account = load_account_for_update(tx, buyer).await?;
    buyer_account.balance += escrow_amount as f64;
    update_account(tx, &buyer_account).await?;
    let entry_id = append_native_entry(
        tx,
        buyer,
        "credit",
        escrow_amount,
        "escrow_refund",
        &intent.idempotency_key.key,
    )
    .await?;
    sqlx::query(
        "update trnm_escrow_trades set status = 'refunded', reversal_intent_id = $2,
             updated_at = now() where purchase_id = $1",
    )
    .bind(purchase_id)
    .bind(&intent.intent_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| db_error("refund TRNM escrow", error))?;
    receipt.status = ReceiptStatus::Refunded;
    receipt.progression_class = receipt.status.progression_class();
    receipt.ledger_entry_id = Some(entry_id.to_string());
    receipt.evidence["escrow_status"] = json!("refunded");
    receipt.evidence["purchase_id"] = json!(purchase_id);
    Ok(())
}

async fn reverse_escrow(
    tx: &mut Transaction<'_, Postgres>,
    intent: &EconomicIntent,
    receipt: &mut EconomicReceipt,
) -> Result<(), LedgerActionError> {
    let purchase_id = metadata_string(intent, "purchase_id").ok_or_else(|| {
        LedgerActionError::Other("chargeback purchase_id is required".to_string())
    })?;
    let row = sqlx::query(
        "select buyer_account_id, seller_account_id, amount::float8 as amount, status
         from trnm_escrow_trades where purchase_id = $1 for update",
    )
    .bind(purchase_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| db_error("load TRNM escrow for chargeback", error))?;
    let Some(row) = row else {
        receipt.status = ReceiptStatus::SellerChargebackFailed;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some("escrow trade does not exist".to_string());
        return Ok(());
    };
    let buyer: Uuid = row.try_get("buyer_account_id").map_err(row_error)?;
    let seller: Uuid = row.try_get("seller_account_id").map_err(row_error)?;
    let amount = row.try_get::<f64, _>("amount").map_err(row_error)?.round() as i64;
    let status: String = row.try_get("status").map_err(row_error)?;
    if status == "reversed" {
        receipt.status = ReceiptStatus::SellerChargebackConsumed;
        receipt.progression_class = receipt.status.progression_class();
        receipt.evidence["escrow_status"] = json!("reversed");
        return Ok(());
    }
    if status != "committed" {
        receipt.status = ReceiptStatus::SellerChargebackFailed;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some(format!("escrow cannot reverse from status {status}"));
        return Ok(());
    }
    let mut locked = Vec::new();
    for id in if buyer < seller {
        [buyer, seller]
    } else {
        [seller, buyer]
    } {
        locked.push(load_account_for_update(tx, id).await?);
    }
    let seller_index = locked
        .iter()
        .position(|account| account.account_id == seller)
        .expect("seller was locked");
    let buyer_index = 1 - seller_index;
    let seller_available = locked[seller_index].balance - locked[seller_index].reserved;
    if seller_available + 1e-9 < amount as f64 {
        receipt.status = ReceiptStatus::SellerChargebackReserveFailed;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some(format!(
            "seller funds unavailable for chargeback: available={seller_available:.0}, required={amount}"
        ));
        receipt.evidence["compensation_lane"] = json!("retry_required");
        return Ok(());
    }
    locked[seller_index].balance -= amount as f64;
    locked[buyer_index].balance += amount as f64;
    update_account(tx, &locked[seller_index]).await?;
    update_account(tx, &locked[buyer_index]).await?;
    let seller_entry = append_native_entry(
        tx,
        seller,
        "debit",
        amount,
        "escrow_chargeback",
        &format!("{}:seller", intent.idempotency_key.key),
    )
    .await?;
    append_native_entry(
        tx,
        buyer,
        "credit",
        amount,
        "escrow_chargeback_refund",
        &format!("{}:buyer", intent.idempotency_key.key),
    )
    .await?;
    sqlx::query(
        "update trnm_escrow_trades set status = 'reversed', reversal_intent_id = $2,
             updated_at = now() where purchase_id = $1",
    )
    .bind(purchase_id)
    .bind(&intent.intent_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| db_error("reverse TRNM escrow", error))?;
    receipt.status = ReceiptStatus::SellerChargebackConsumed;
    receipt.progression_class = receipt.status.progression_class();
    receipt.ledger_entry_id = Some(seller_entry.to_string());
    receipt.evidence["escrow_status"] = json!("reversed");
    receipt.evidence["compensation_lane"] = json!("completed");
    Ok(())
}

async fn persist_native_receipt(
    tx: &mut Transaction<'_, Postgres>,
    intent: &EconomicIntent,
    receipt: &EconomicReceipt,
) -> Result<(), LedgerActionError> {
    let receipt_json = serde_json::to_value(receipt)
        .map_err(|error| LedgerActionError::Other(error.to_string()))?;
    let status = serde_json::to_value(&receipt.status)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    let progression = serde_json::to_value(receipt.progression_class)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    sqlx::query(
        "insert into trnm_economic_receipts (
             receipt_id, intent_id, protocol_version, idempotency_scope, idempotency_key,
             progression_class, status, receipt_json, finalized_at
         ) values ($1, $2, $3, $4, $5, $6, $7, $8, to_timestamp($9::double precision))
         on conflict (intent_id) do update set
             receipt_id = excluded.receipt_id,
             progression_class = excluded.progression_class,
             status = excluded.status,
             receipt_json = excluded.receipt_json,
             finalized_at = excluded.finalized_at,
             updated_at = now()",
    )
    .bind(&receipt.receipt_id)
    .bind(&intent.intent_id)
    .bind(&receipt.protocol_version)
    .bind(&intent.idempotency_key.scope)
    .bind(&intent.idempotency_key.key)
    .bind(&progression)
    .bind(&status)
    .bind(&receipt_json)
    .bind(receipt.finalized_at_epoch as f64)
    .execute(&mut **tx)
    .await
    .map_err(|error| db_error("persist TRNM economic receipt", error))?;
    sqlx::query(
        "update trnm_economic_intents set status = $2, updated_at = now() where intent_id = $1",
    )
    .bind(&intent.intent_id)
    .bind(&status)
    .execute(&mut **tx)
    .await
    .map_err(|error| db_error("finalize TRNM economic intent", error))?;
    Ok(())
}

fn db_error(context: &str, error: sqlx::Error) -> LedgerActionError {
    LedgerActionError::Other(format!("{context} failed: {error}"))
}

fn row_error(error: sqlx::Error) -> LedgerActionError {
    LedgerActionError::Other(error.to_string())
}
