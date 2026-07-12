use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use term_exchange_protocol::{
    EconomicIntent, EconomicIntentKind, EconomicReceipt, ReceiptProgressionClass, ReceiptStatus,
    ServerSignedValueEntitlementV1, ServerSignedValueEntitlementV2, SettlementBackendKind,
    ValueEntitlementSource, WalletSnapshot, BATTLE_WALLET_REWARD_DAILY_CAP,
    CEX_SETTLEMENT_BACKEND_ID, SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY,
    SERVER_SIGNED_VALUE_ENTITLEMENT_V2_CONTRACT,
};
use uuid::Uuid;

use super::{
    postgres::PostgresLedgerRepository, LedgerActionError, TrnmPlayerIdentityRecord,
    TrnmPlayerSessionRecord,
};
use crate::state::AccountRecord;

const DEFAULT_SELLER_REVERSIBLE_WINDOW_SECONDS: i64 = 86_400;
const MAX_SELLER_REVERSIBLE_WINDOW_SECONDS: i64 = 30 * 86_400;

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
        let advisory_key = format!(
            "{}:{}",
            intent.idempotency_key.scope, intent.idempotency_key.key
        );
        sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&advisory_key)
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error("lock TRNM idempotency key", error))?;

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
            EconomicIntentKind::CompleteContract => {
                if amount != 0 {
                    return Err(LedgerActionError::IdentityRejected(
                        "CompleteContract is audit-only and must have amount_credits=0".to_string(),
                    ));
                }
                receipt.status = ReceiptStatus::SkippedZeroReward;
                receipt.progression_class = receipt.status.progression_class();
                Ok(())
            }
            EconomicIntentKind::ReleaseReward => {
                if amount <= 0 {
                    receipt.status = ReceiptStatus::SkippedZeroReward;
                    receipt.progression_class = receipt.status.progression_class();
                    Ok(())
                } else {
                    consume_value_entitlement(&mut tx, intent, account_id, amount).await?;
                    let mut account = load_account_for_update(&mut tx, account_id).await?;
                    account.balance += amount as f64;
                    update_account(&mut tx, &account).await?;
                    let entry_id = append_native_entry(
                        &mut tx,
                        account_id,
                        "credit",
                        amount,
                        "release_reward",
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
        release_matured_seller_holds(&mut tx, account_id).await?;
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

    pub(super) async fn register_trnm_native_player_identity(
        &self,
        player_id: &str,
        account_id: Uuid,
        recovery_key: &str,
    ) -> Result<TrnmPlayerIdentityRecord, LedgerActionError> {
        validate_identity_inputs(player_id, recovery_key)?;
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| db_error("begin TRNM identity registration", error))?;
        let _ = load_account_for_update(&mut tx, account_id).await?;
        let recovery_key_hash = recovery_key_hash(recovery_key)?;
        sqlx::query(
            "insert into trnm_player_identities (
                 player_id, account_id, recovery_key_hash, recovery_generation, status
             ) values ($1, $2, $3, 1, 'active')",
        )
        .bind(player_id)
        .bind(account_id)
        .bind(&recovery_key_hash)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("register TRNM player identity", error))?;
        append_identity_audit(&mut tx, player_id, 1, "registered", &recovery_key_hash).await?;
        tx.commit()
            .await
            .map_err(|error| db_error("commit TRNM identity registration", error))?;
        Ok(TrnmPlayerIdentityRecord {
            player_id: player_id.to_string(),
            account_id,
            recovery_generation: 1,
            status: "active".to_string(),
        })
    }

    pub(super) async fn issue_trnm_native_product_registration_invite(
        &self,
        lifetime_seconds: i64,
        max_uses: i32,
    ) -> Result<Value, LedgerActionError> {
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let lifetime_seconds = lifetime_seconds.clamp(60, 604_800);
        let max_uses = max_uses.clamp(1, 100);
        let invite_id = Uuid::new_v4();
        let invite_code = format!("trnm-register-{}-{}", Uuid::new_v4(), Uuid::new_v4());
        let invite_code_hash = format!("{:x}", Sha256::digest(invite_code.as_bytes()));
        let expires_at_epoch = Utc::now().timestamp().saturating_add(lifetime_seconds);
        sqlx::query(
            "insert into trnm_product_registration_invites (
                invite_id, invite_code_hash, max_uses, expires_at
             ) values ($1, $2, $3, to_timestamp($4))",
        )
        .bind(invite_id)
        .bind(invite_code_hash)
        .bind(max_uses)
        .bind(expires_at_epoch)
        .execute(pool)
        .await
        .map_err(|error| db_error("issue TRNM product registration invite", error))?;
        Ok(json!({
            "invite_id": invite_id,
            "invite_code": invite_code,
            "max_uses": max_uses,
            "expires_at_epoch": expires_at_epoch,
        }))
    }

    pub(super) async fn register_trnm_native_product_player(
        &self,
        player_id: &str,
        recovery_key: &str,
        org_id: Uuid,
        invite_code: &str,
    ) -> Result<TrnmPlayerIdentityRecord, LedgerActionError> {
        validate_identity_inputs(player_id, recovery_key)?;
        if !player_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        {
            return Err(LedgerActionError::Other(
                "player_id must use portable ASCII letters, digits, '-' or '_'".to_string(),
            ));
        }
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| db_error("begin TRNM product registration", error))?;
        let org_exists: bool = sqlx::query_scalar(
            "select exists(select 1 from organizations where org_id = $1 and status = 'active')",
        )
        .bind(org_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| db_error("validate TRNM product organization", error))?;
        if !org_exists {
            return Err(LedgerActionError::IdentityRejected(
                "TRNM product registration organization is unavailable".to_string(),
            ));
        }
        let invite_code_hash = format!("{:x}", Sha256::digest(invite_code.as_bytes()));
        let consumed = sqlx::query(
            "update trnm_product_registration_invites set used_count = used_count + 1
             where invite_code_hash = $1 and revoked_at is null and expires_at > now()
               and used_count < max_uses
             returning invite_id",
        )
        .bind(invite_code_hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| db_error("consume TRNM product registration invite", error))?;
        if consumed.is_none() {
            return Err(LedgerActionError::IdentityRejected(
                "closed-alpha registration invite is invalid, expired, or consumed".to_string(),
            ));
        }
        let account_id = Uuid::new_v4();
        sqlx::query(
            "insert into accounts (account_id, org_id, account_type, currency_unit, status)
             values ($1, $2, 'trnm-online-player', 'credit', 'active')",
        )
        .bind(account_id)
        .bind(org_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("create TRNM product account", error))?;
        let recovery_key_hash = recovery_key_hash(recovery_key)?;
        sqlx::query(
            "insert into trnm_player_identities (
                 player_id, account_id, recovery_key_hash, recovery_generation, status
             ) values ($1, $2, $3, 1, 'active')",
        )
        .bind(player_id)
        .bind(account_id)
        .bind(&recovery_key_hash)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("register TRNM product identity", error))?;
        append_identity_audit(&mut tx, player_id, 1, "registered", &recovery_key_hash).await?;
        tx.commit()
            .await
            .map_err(|error| db_error("commit TRNM product registration", error))?;
        Ok(TrnmPlayerIdentityRecord {
            player_id: player_id.to_string(),
            account_id,
            recovery_generation: 1,
            status: "active".to_string(),
        })
    }

    pub(super) async fn submit_trnm_native_identity_appeal(
        &self,
        player_id: &str,
        recovery_key: &str,
        message: &str,
    ) -> Result<Value, LedgerActionError> {
        validate_identity_inputs(player_id, recovery_key)?;
        if !(10..=2000).contains(&message.trim().len()) {
            return Err(LedgerActionError::Other(
                "appeal message must contain 10..2000 characters".to_string(),
            ));
        }
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| db_error("begin TRNM identity appeal", error))?;
        let row = sqlx::query(
            "select recovery_key_hash, status from trnm_player_identities
             where player_id = $1 for update",
        )
        .bind(player_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| db_error("load appealed TRNM identity", error))?
        .ok_or(LedgerActionError::AccountNotFound)?;
        let stored_hash: String = row.try_get("recovery_key_hash").map_err(row_error)?;
        let status: String = row.try_get("status").map_err(row_error)?;
        if !recovery_key_matches(&stored_hash, recovery_key) {
            return Err(LedgerActionError::IdentityRejected(
                "TRNM recovery credential is invalid".to_string(),
            ));
        }
        if status != "suspended" {
            return Err(LedgerActionError::IdentityRejected(
                "only a suspended identity may submit an appeal".to_string(),
            ));
        }
        let appeal_id = Uuid::new_v4();
        sqlx::query(
            "insert into trnm_identity_appeals (appeal_id, player_id, message)
             values ($1, $2, $3)",
        )
        .bind(appeal_id)
        .bind(player_id)
        .bind(message.trim())
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("submit TRNM identity appeal", error))?;
        tx.commit()
            .await
            .map_err(|error| db_error("commit TRNM identity appeal", error))?;
        Ok(json!({
            "appeal_id": appeal_id,
            "player_id": player_id,
            "status": "pending",
        }))
    }

    pub(super) async fn resolve_trnm_native_identity_appeal(
        &self,
        appeal_id: Uuid,
        decision: &str,
        resolution: &str,
    ) -> Result<Value, LedgerActionError> {
        if !matches!(decision, "approved" | "rejected") {
            return Err(LedgerActionError::Other(
                "appeal decision must be approved or rejected".to_string(),
            ));
        }
        if !(10..=2000).contains(&resolution.trim().len()) {
            return Err(LedgerActionError::Other(
                "appeal resolution must contain 10..2000 characters".to_string(),
            ));
        }
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| db_error("begin TRNM identity appeal resolution", error))?;
        let row = sqlx::query(
            "update trnm_identity_appeals set status = $2, resolution = $3,
                 resolved_at = now()
             where appeal_id = $1 and status = 'pending'
             returning player_id",
        )
        .bind(appeal_id)
        .bind(decision)
        .bind(resolution.trim())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| db_error("resolve TRNM identity appeal", error))?
        .ok_or_else(|| {
            LedgerActionError::IdentityRejected(
                "identity appeal is already resolved or unknown".to_string(),
            )
        })?;
        let player_id: String = row.try_get("player_id").map_err(row_error)?;
        if decision == "approved" {
            let identity = sqlx::query(
                "update trnm_player_identities set status = 'active', updated_at = now()
                 where player_id = $1 and status = 'suspended'
                 returning recovery_generation, recovery_key_hash",
            )
            .bind(&player_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| db_error("reactivate appealed TRNM identity", error))?
            .ok_or_else(|| {
                LedgerActionError::IdentityRejected(
                    "appealed identity is no longer suspended".to_string(),
                )
            })?;
            append_identity_audit(
                &mut tx,
                &player_id,
                identity.try_get("recovery_generation").map_err(row_error)?,
                "reactivated",
                &identity
                    .try_get::<String, _>("recovery_key_hash")
                    .map_err(row_error)?,
            )
            .await?;
        }
        tx.commit()
            .await
            .map_err(|error| db_error("commit TRNM identity appeal resolution", error))?;
        Ok(json!({
            "appeal_id": appeal_id,
            "player_id": player_id,
            "status": decision,
        }))
    }

    pub(super) async fn recover_trnm_native_player_identity(
        &self,
        player_id: &str,
        recovery_key: &str,
        new_recovery_key: &str,
    ) -> Result<TrnmPlayerIdentityRecord, LedgerActionError> {
        validate_identity_inputs(player_id, recovery_key)?;
        validate_identity_inputs(player_id, new_recovery_key)?;
        if recovery_key == new_recovery_key {
            return Err(LedgerActionError::Other(
                "new recovery key must differ from the current key".to_string(),
            ));
        }
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| db_error("begin TRNM identity recovery", error))?;
        let row = sqlx::query(
            "select account_id, recovery_key_hash, recovery_generation, status
             from trnm_player_identities where player_id = $1 for update",
        )
        .bind(player_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| db_error("load TRNM player identity", error))?
        .ok_or(LedgerActionError::AccountNotFound)?;
        let status: String = row.try_get("status").map_err(row_error)?;
        if status != "active" {
            return Err(LedgerActionError::IdentityRejected(
                "TRNM player identity is not active".to_string(),
            ));
        }
        let stored_hash: String = row.try_get("recovery_key_hash").map_err(row_error)?;
        if !recovery_key_matches(&stored_hash, recovery_key) {
            return Err(LedgerActionError::IdentityRejected(
                "TRNM recovery credential is invalid".to_string(),
            ));
        }
        let account_id: Uuid = row.try_get("account_id").map_err(row_error)?;
        let generation = row
            .try_get::<i64, _>("recovery_generation")
            .map_err(row_error)?
            .saturating_add(1);
        let new_hash = recovery_key_hash(new_recovery_key)?;
        sqlx::query(
            "update trnm_player_identities set recovery_key_hash = $2,
                 recovery_generation = $3, recovered_at = now(), updated_at = now()
             where player_id = $1",
        )
        .bind(player_id)
        .bind(&new_hash)
        .bind(generation)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("rotate TRNM recovery credential", error))?;
        sqlx::query(
            "update trnm_player_sessions
             set revoked_at = now(), revoke_reason = 'identity_recovered'
             where player_id = $1 and revoked_at is null",
        )
        .bind(player_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("revoke sessions after identity recovery", error))?;
        append_identity_audit(&mut tx, player_id, generation, "recovered", &new_hash).await?;
        tx.commit()
            .await
            .map_err(|error| db_error("commit TRNM identity recovery", error))?;
        Ok(TrnmPlayerIdentityRecord {
            player_id: player_id.to_string(),
            account_id,
            recovery_generation: generation,
            status,
        })
    }

    pub(super) async fn set_trnm_native_player_identity_status(
        &self,
        player_id: &str,
        status: &str,
    ) -> Result<TrnmPlayerIdentityRecord, LedgerActionError> {
        if !matches!(status, "active" | "suspended" | "closed") {
            return Err(LedgerActionError::Other(
                "identity status must be active, suspended, or closed".to_string(),
            ));
        }
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| db_error("begin TRNM identity status update", error))?;
        let row = sqlx::query(
            "update trnm_player_identities set status = $2, updated_at = now()
             where player_id = $1
             returning account_id, recovery_key_hash, recovery_generation, status",
        )
        .bind(player_id)
        .bind(status)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| db_error("update TRNM identity status", error))?
        .ok_or(LedgerActionError::AccountNotFound)?;
        if status != "active" {
            sqlx::query(
                "update trnm_player_sessions set revoked_at = now(), revoke_reason = $2
                 where player_id = $1 and revoked_at is null",
            )
            .bind(player_id)
            .bind(format!("identity_{status}"))
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error("revoke sessions after identity status change", error))?;
        }
        let generation: i64 = row.try_get("recovery_generation").map_err(row_error)?;
        let recovery_hash: String = row.try_get("recovery_key_hash").map_err(row_error)?;
        let event_kind = if status == "active" {
            "reactivated"
        } else {
            status
        };
        append_identity_audit(&mut tx, player_id, generation, event_kind, &recovery_hash).await?;
        tx.commit()
            .await
            .map_err(|error| db_error("commit TRNM identity status update", error))?;
        Ok(TrnmPlayerIdentityRecord {
            player_id: player_id.to_string(),
            account_id: row.try_get("account_id").map_err(row_error)?,
            recovery_generation: generation,
            status: row.try_get("status").map_err(row_error)?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn create_trnm_native_player_session(
        &self,
        player_id: &str,
        recovery_key: &str,
        device_id: &str,
        session_id: Uuid,
        token_hash: &str,
        issued_at_epoch: i64,
        expires_at_epoch: i64,
    ) -> Result<TrnmPlayerSessionRecord, LedgerActionError> {
        validate_identity_inputs(player_id, recovery_key)?;
        if device_id.trim().len() < 3 || device_id.len() > 128 {
            return Err(LedgerActionError::Other(
                "device_id must contain 3..128 characters".to_string(),
            ));
        }
        let issued_at = Utc
            .timestamp_opt(issued_at_epoch, 0)
            .single()
            .ok_or_else(|| LedgerActionError::Other("invalid session issued_at".to_string()))?;
        let expires_at = Utc
            .timestamp_opt(expires_at_epoch, 0)
            .single()
            .ok_or_else(|| LedgerActionError::Other("invalid session expires_at".to_string()))?;
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| db_error("begin TRNM player session", error))?;
        let row = sqlx::query(
            "select account_id, recovery_key_hash, recovery_generation, status
             from trnm_player_identities where player_id = $1 for update",
        )
        .bind(player_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| db_error("load TRNM identity for session", error))?
        .ok_or(LedgerActionError::AccountNotFound)?;
        let status: String = row.try_get("status").map_err(row_error)?;
        if status != "active" {
            return Err(LedgerActionError::IdentityRejected(
                "TRNM player identity is not active".to_string(),
            ));
        }
        let stored_hash: String = row.try_get("recovery_key_hash").map_err(row_error)?;
        if !recovery_key_matches(&stored_hash, recovery_key) {
            return Err(LedgerActionError::IdentityRejected(
                "TRNM recovery credential is invalid".to_string(),
            ));
        }
        let account_id: Uuid = row.try_get("account_id").map_err(row_error)?;
        let recovery_generation: i64 = row.try_get("recovery_generation").map_err(row_error)?;
        sqlx::query(
            "insert into trnm_player_sessions (
                 session_id, player_id, account_id, device_id, recovery_generation,
                 token_hash, issued_at, expires_at
             ) values ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(session_id)
        .bind(player_id)
        .bind(account_id)
        .bind(device_id)
        .bind(recovery_generation)
        .bind(token_hash)
        .bind(issued_at)
        .bind(expires_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error("persist TRNM player session", error))?;
        tx.commit()
            .await
            .map_err(|error| db_error("commit TRNM player session", error))?;
        Ok(TrnmPlayerSessionRecord {
            session_id,
            player_id: player_id.to_string(),
            account_id,
            device_id: device_id.to_string(),
            recovery_generation,
            issued_at_epoch,
            expires_at_epoch,
        })
    }

    pub(super) async fn authenticate_trnm_native_player_identity(
        &self,
        player_id: &str,
        recovery_key: &str,
    ) -> Result<TrnmPlayerIdentityRecord, LedgerActionError> {
        validate_identity_inputs(player_id, recovery_key)?;
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let locked: bool = sqlx::query_scalar(
            "select coalesce(locked_until > now(), false)
             from trnm_product_login_attempts where player_id = $1",
        )
        .bind(player_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| db_error("check TRNM product login rate limit", error))?
        .unwrap_or(false);
        if locked {
            return Err(LedgerActionError::IdentityRejected(
                "TRNM product login is temporarily locked after repeated failures".to_string(),
            ));
        }
        let row = sqlx::query(
            "select account_id, recovery_key_hash, recovery_generation, status
             from trnm_player_identities where player_id = $1",
        )
        .bind(player_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| db_error("authenticate TRNM identity", error))?;
        let valid_credential = row
            .as_ref()
            .and_then(|row| row.try_get::<String, _>("recovery_key_hash").ok())
            .is_some_and(|stored_hash| recovery_key_matches(&stored_hash, recovery_key));
        if !valid_credential {
            sqlx::query(
                "insert into trnm_product_login_attempts (
                    player_id, attempt_count, window_started_at, locked_until, updated_at
                 ) values ($1, 1, now(), null, now())
                 on conflict (player_id) do update set
                    attempt_count = case
                        when trnm_product_login_attempts.window_started_at < now() - interval '15 minutes'
                        then 1 else trnm_product_login_attempts.attempt_count + 1 end,
                    window_started_at = case
                        when trnm_product_login_attempts.window_started_at < now() - interval '15 minutes'
                        then now() else trnm_product_login_attempts.window_started_at end,
                    locked_until = case
                        when (case
                            when trnm_product_login_attempts.window_started_at < now() - interval '15 minutes'
                            then 1 else trnm_product_login_attempts.attempt_count + 1 end) >= 5
                        then now() + interval '5 minutes' else null end,
                    updated_at = now()",
            )
            .bind(player_id)
            .execute(pool)
            .await
            .map_err(|error| db_error("record TRNM product login failure", error))?;
            return Err(LedgerActionError::IdentityRejected(
                "TRNM product login credential is invalid".to_string(),
            ));
        }
        let row = row.expect("valid credential requires an identity row");
        let status: String = row.try_get("status").map_err(row_error)?;
        if status != "active" {
            return Err(LedgerActionError::IdentityRejected(
                "TRNM player identity is not active".to_string(),
            ));
        }
        sqlx::query("delete from trnm_product_login_attempts where player_id = $1")
            .bind(player_id)
            .execute(pool)
            .await
            .map_err(|error| db_error("clear TRNM product login failures", error))?;
        Ok(TrnmPlayerIdentityRecord {
            player_id: player_id.to_string(),
            account_id: row.try_get("account_id").map_err(row_error)?,
            recovery_generation: row.try_get("recovery_generation").map_err(row_error)?,
            status,
        })
    }

    pub(super) async fn verify_trnm_native_player_session(
        &self,
        session_id: Uuid,
        token_hash: &str,
        actor_id: &str,
        account_id: Uuid,
        recovery_generation: i64,
    ) -> Result<TrnmPlayerSessionRecord, LedgerActionError> {
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let row = sqlx::query(
            "update trnm_player_sessions s set last_used_at = now()
             from trnm_player_identities i
             where s.session_id = $1 and s.player_id = $2 and s.account_id = $3
               and s.recovery_generation = $4 and s.token_hash = $5
               and s.revoked_at is null and s.expires_at > now()
               and i.player_id = s.player_id and i.account_id = s.account_id
               and i.status = 'active' and i.recovery_generation = s.recovery_generation
             returning s.player_id, s.account_id, s.device_id, s.recovery_generation,
                       extract(epoch from s.issued_at)::bigint as issued_at_epoch,
                       extract(epoch from s.expires_at)::bigint as expires_at_epoch",
        )
        .bind(session_id)
        .bind(actor_id)
        .bind(account_id)
        .bind(recovery_generation)
        .bind(token_hash)
        .fetch_optional(pool)
        .await
        .map_err(|error| db_error("verify TRNM player session", error))?
        .ok_or_else(|| {
            LedgerActionError::IdentityRejected(
                "player session is expired, revoked, or does not own this account".to_string(),
            )
        })?;
        Ok(TrnmPlayerSessionRecord {
            session_id,
            player_id: row.try_get("player_id").map_err(row_error)?,
            account_id: row.try_get("account_id").map_err(row_error)?,
            device_id: row.try_get("device_id").map_err(row_error)?,
            recovery_generation: row.try_get("recovery_generation").map_err(row_error)?,
            issued_at_epoch: row.try_get("issued_at_epoch").map_err(row_error)?,
            expires_at_epoch: row.try_get("expires_at_epoch").map_err(row_error)?,
        })
    }

    pub(super) async fn revoke_trnm_native_player_session(
        &self,
        session_id: Uuid,
        reason: &str,
    ) -> Result<(), LedgerActionError> {
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let result = sqlx::query(
            "update trnm_player_sessions set revoked_at = now(), revoke_reason = $2
             where session_id = $1 and revoked_at is null",
        )
        .bind(session_id)
        .bind(reason)
        .execute(pool)
        .await
        .map_err(|error| db_error("revoke TRNM player session", error))?;
        if result.rows_affected() == 0 {
            return Err(LedgerActionError::IdentityRejected(
                "player session is already revoked or unknown".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) async fn list_trnm_native_receipts(
        &self,
    ) -> Result<Vec<EconomicReceipt>, LedgerActionError> {
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let values = sqlx::query_scalar::<_, Value>(
            "select receipt_json from trnm_economic_receipts order by finalized_at, receipt_id",
        )
        .fetch_all(pool)
        .await
        .map_err(|error| db_error("list TRNM economic receipts", error))?;
        values
            .into_iter()
            .map(|value| {
                serde_json::from_value(value).map_err(|error| {
                    LedgerActionError::Other(format!("decode TRNM receipt failed: {error}"))
                })
            })
            .collect()
    }

    pub(super) async fn run_trnm_native_maintenance(&self) -> Result<Value, LedgerActionError> {
        let pool = self.pool.as_ref().ok_or_else(|| {
            LedgerActionError::RepositoryUnavailable("postgres pool not initialized".to_string())
        })?;
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| db_error("begin TRNM economy maintenance", error))?;
        let sellers = sqlx::query_scalar::<_, Uuid>(
            "select distinct seller_account_id from trnm_escrow_trades
             where status = 'committed' and seller_hold_released = false
               and reversible_until <= now()",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| db_error("find matured seller holds", error))?;
        for seller in &sellers {
            release_matured_seller_holds(&mut tx, *seller).await?;
        }
        let receipt_count =
            sqlx::query_scalar::<_, i64>("select count(*)::bigint from trnm_economic_receipts")
                .fetch_one(&mut *tx)
                .await
                .map_err(|error| db_error("count TRNM receipts", error))?;
        let overdue_holds = sqlx::query_scalar::<_, i64>(
            "select count(*)::bigint from trnm_escrow_trades
             where status = 'committed' and seller_hold_released = false
               and reversible_until <= now()",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| db_error("count overdue seller holds", error))?;
        tx.commit()
            .await
            .map_err(|error| db_error("commit TRNM economy maintenance", error))?;
        Ok(json!({
            "released_seller_accounts": sellers.len(),
            "overdue_seller_holds": overdue_holds,
            "authoritative_receipts": receipt_count,
            "alert": overdue_holds > 0,
        }))
    }
}

fn validate_identity_inputs(player_id: &str, recovery_key: &str) -> Result<(), LedgerActionError> {
    if player_id.trim().len() < 3 || player_id.len() > 128 {
        return Err(LedgerActionError::Other(
            "player_id must contain 3..128 characters".to_string(),
        ));
    }
    if recovery_key.len() < 24 || recovery_key.len() > 512 {
        return Err(LedgerActionError::Other(
            "recovery key must contain 24..512 characters".to_string(),
        ));
    }
    Ok(())
}

fn recovery_key_hash(recovery_key: &str) -> Result<String, LedgerActionError> {
    Argon2::default()
        .hash_password(recovery_key.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|hash| hash.to_string())
        .map_err(|error| LedgerActionError::Other(format!("hash TRNM credential: {error}")))
}

fn recovery_key_matches(stored_hash: &str, recovery_key: &str) -> bool {
    if stored_hash.starts_with("$argon2") {
        return PasswordHash::new(stored_hash).is_ok_and(|parsed| {
            Argon2::default()
                .verify_password(recovery_key.as_bytes(), &parsed)
                .is_ok()
        });
    }
    stored_hash
        == format!(
            "{:x}",
            Sha256::digest(format!("trnm-player-recovery-v1:{recovery_key}").as_bytes())
        )
}

async fn append_identity_audit(
    tx: &mut Transaction<'_, Postgres>,
    player_id: &str,
    generation: i64,
    event_kind: &str,
    recovery_key_hash: &str,
) -> Result<(), LedgerActionError> {
    let audit_id = Uuid::new_v4();
    let event_hash = format!(
        "{:x}",
        Sha256::digest(
            format!("{player_id}:{generation}:{event_kind}:{recovery_key_hash}:{audit_id}")
                .as_bytes()
        )
    );
    sqlx::query(
        "insert into trnm_identity_recovery_audit (
             audit_id, player_id, recovery_generation, event_kind, event_hash
         ) values ($1, $2, $3, $4, $5)",
    )
    .bind(audit_id)
    .bind(player_id)
    .bind(generation)
    .bind(event_kind)
    .bind(event_hash)
    .execute(&mut **tx)
    .await
    .map_err(|error| db_error("append TRNM identity recovery audit", error))?;
    Ok(())
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

async fn consume_value_entitlement(
    tx: &mut Transaction<'_, Postgres>,
    intent: &EconomicIntent,
    account_id: Uuid,
    amount: i64,
) -> Result<(), LedgerActionError> {
    let entitlement_json = intent
        .metadata
        .get(SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY)
        .cloned()
        .ok_or_else(|| {
            LedgerActionError::IdentityRejected(
                "server-signed value entitlement is required".to_string(),
            )
        })?;
    let entitlement: ServerSignedValueEntitlementV1 = if entitlement_json
        .get("contract_version")
        .and_then(Value::as_str)
        == Some(SERVER_SIGNED_VALUE_ENTITLEMENT_V2_CONTRACT)
    {
        let value: ServerSignedValueEntitlementV2 =
            serde_json::from_value(entitlement_json.clone()).map_err(|error| {
                LedgerActionError::IdentityRejected(format!(
                    "decode v2 value entitlement failed: {error}"
                ))
            })?;
        ServerSignedValueEntitlementV1 {
            contract_version: value.contract_version,
            entitlement_id: value.entitlement_id,
            issuer: value.issuer,
            key_id: value.key_id,
            actor_id: value.actor_id,
            account_id: value.account_id,
            source: value.source,
            source_id: value.source_id,
            intent_id: value.intent_id,
            amount_credits: value.amount_credits,
            currency: value.currency,
            budget_day: value.budget_day,
            issued_at_epoch: value.issued_at_epoch,
            expires_at_epoch: value.expires_at_epoch,
            signature: value.signature,
        }
    } else {
        serde_json::from_value(entitlement_json.clone()).map_err(|error| {
            LedgerActionError::IdentityRejected(format!("decode value entitlement failed: {error}"))
        })?
    };
    let entitlement_account = Uuid::parse_str(&entitlement.account_id).map_err(|_| {
        LedgerActionError::IdentityRejected("entitlement account_id is invalid".to_string())
    })?;
    if entitlement.intent_id != intent.intent_id
        || entitlement_account != account_id
        || entitlement.amount_credits != amount
        || !matches!(entitlement.source, ValueEntitlementSource::Battle)
    {
        return Err(LedgerActionError::IdentityRejected(
            "value entitlement does not bind this reward intent".to_string(),
        ));
    }
    let issued_at = Utc
        .timestamp_opt(entitlement.issued_at_epoch, 0)
        .single()
        .ok_or_else(|| {
            LedgerActionError::IdentityRejected("invalid entitlement issue time".to_string())
        })?;
    let expires_at = Utc
        .timestamp_opt(entitlement.expires_at_epoch, 0)
        .single()
        .ok_or_else(|| {
            LedgerActionError::IdentityRejected("invalid entitlement expiry".to_string())
        })?;
    sqlx::query(
        "insert into trnm_value_entitlements (
             entitlement_id, intent_id, issuer, key_id, actor_id, account_id,
             source_kind, source_id, amount_credits, currency, budget_day,
             issued_at, expires_at, entitlement_json
         ) values ($1, $2, $3, $4, $5, $6, 'battle', $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(&entitlement.entitlement_id)
    .bind(&entitlement.intent_id)
    .bind(&entitlement.issuer)
    .bind(&entitlement.key_id)
    .bind(&entitlement.actor_id)
    .bind(account_id)
    .bind(&entitlement.source_id)
    .bind(entitlement.amount_credits)
    .bind(&entitlement.currency)
    .bind(i32::try_from(entitlement.budget_day).unwrap_or(i32::MAX))
    .bind(issued_at)
    .bind(expires_at)
    .bind(entitlement_json)
    .execute(&mut **tx)
    .await
    .map_err(|error| {
        LedgerActionError::IdentityRejected(format!(
            "value entitlement is duplicated or invalid: {error}"
        ))
    })?;
    let updated = sqlx::query_scalar::<_, i64>(
        "insert into trnm_wallet_reward_daily_budget (account_id, budget_day, issued_credits)
         values ($1, $2, $3)
         on conflict (account_id, budget_day) do update set
             issued_credits = trnm_wallet_reward_daily_budget.issued_credits + excluded.issued_credits,
             updated_at = now()
         where trnm_wallet_reward_daily_budget.issued_credits + excluded.issued_credits <= $4
         returning issued_credits",
    )
    .bind(account_id)
    .bind(i32::try_from(entitlement.budget_day).unwrap_or(i32::MAX))
    .bind(amount)
    .bind(BATTLE_WALLET_REWARD_DAILY_CAP)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| db_error("enforce TRNM wallet reward daily budget", error))?;
    if updated.is_none() {
        return Err(LedgerActionError::IdentityRejected(format!(
            "daily wallet reward budget exceeds {BATTLE_WALLET_REWARD_DAILY_CAP} credits"
        )));
    }
    Ok(())
}

fn seller_reversible_window_seconds(intent: &EconomicIntent) -> i64 {
    intent
        .metadata
        .get("seller_reversible_window_seconds")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_SELLER_REVERSIBLE_WINDOW_SECONDS)
        .clamp(60, MAX_SELLER_REVERSIBLE_WINDOW_SECONDS)
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
        "select amount::float8 as amount, status, seller_hold_amount::float8 as seller_hold_amount,
                seller_hold_released
         from trnm_escrow_trades
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
        receipt.evidence["seller_payout_reserved"] = json!(!row
            .try_get::<bool, _>("seller_hold_released")
            .map_err(row_error)?);
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
    seller_account.reserved += amount as f64;
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
             seller_hold_amount = $3, seller_hold_released = false,
             reversible_until = now() + make_interval(secs => $4),
             updated_at = now() where purchase_id = $1",
    )
    .bind(&purchase_id)
    .bind(&intent.intent_id)
    .bind(amount as f64)
    .bind(seller_reversible_window_seconds(intent) as f64)
    .execute(&mut **tx)
    .await
    .map_err(|error| db_error("commit TRNM escrow", error))?;
    receipt.status = ReceiptStatus::Consumed;
    receipt.progression_class = receipt.status.progression_class();
    receipt.ledger_entry_id = Some(entry_id.to_string());
    receipt.evidence["escrow_status"] = json!("committed");
    receipt.evidence["seller_payout_reserved"] = json!(true);
    receipt.evidence["seller_reversible_window_seconds"] =
        json!(seller_reversible_window_seconds(intent));
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
        "select buyer_account_id, seller_account_id, amount::float8 as amount, status,
                seller_hold_amount::float8 as seller_hold_amount, seller_hold_released
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
    let seller_hold_amount = row
        .try_get::<f64, _>("seller_hold_amount")
        .map_err(row_error)?
        .round() as i64;
    let seller_hold_released: bool = row.try_get("seller_hold_released").map_err(row_error)?;
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
    let reserved_reversal = !seller_hold_released && seller_hold_amount >= amount;
    if reserved_reversal && locked[seller_index].reserved + 1e-9 < amount as f64 {
        receipt.status = ReceiptStatus::SellerChargebackReserveFailed;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some("seller payout hold is missing from reserved balance".to_string());
        receipt.evidence["compensation_lane"] = json!("operator_reconciliation_required");
        return Ok(());
    }
    if !reserved_reversal && seller_available + 1e-9 < amount as f64 {
        receipt.status = ReceiptStatus::SellerChargebackReserveFailed;
        receipt.progression_class = receipt.status.progression_class();
        receipt.reason = Some(format!(
            "seller funds unavailable for chargeback: available={seller_available:.0}, required={amount}"
        ));
        receipt.evidence["compensation_lane"] = json!("retry_required");
        return Ok(());
    }
    if reserved_reversal {
        locked[seller_index].reserved -= amount as f64;
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
             seller_hold_amount = 0, seller_hold_released = true,
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
    receipt.evidence["seller_payout_hold_consumed"] = json!(reserved_reversal);
    Ok(())
}

async fn release_matured_seller_holds(
    tx: &mut Transaction<'_, Postgres>,
    seller_account_id: Uuid,
) -> Result<(), LedgerActionError> {
    let rows = sqlx::query(
        "select purchase_id, seller_hold_amount::float8 as seller_hold_amount
         from trnm_escrow_trades
         where seller_account_id = $1 and status = 'committed'
           and seller_hold_released = false and reversible_until <= now()
         order by purchase_id for update",
    )
    .bind(seller_account_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| db_error("load matured TRNM seller payout holds", error))?;
    if rows.is_empty() {
        return Ok(());
    }
    let total = rows
        .iter()
        .map(|row| row.try_get::<f64, _>("seller_hold_amount"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(row_error)?
        .into_iter()
        .sum::<f64>();
    let mut seller = load_account_for_update(tx, seller_account_id).await?;
    if seller.reserved + 1e-9 < total {
        return Err(LedgerActionError::Other(
            "seller reserved balance is below matured payout holds".to_string(),
        ));
    }
    seller.reserved -= total;
    update_account(tx, &seller).await?;
    sqlx::query(
        "update trnm_escrow_trades set seller_hold_released = true, updated_at = now()
         where seller_account_id = $1 and status = 'committed'
           and seller_hold_released = false and reversible_until <= now()",
    )
    .bind(seller_account_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| db_error("release matured TRNM seller payout holds", error))?;
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

#[cfg(test)]
mod credential_tests {
    use super::*;

    #[test]
    fn product_credentials_use_argon2id_and_accept_legacy_high_entropy_hashes() {
        let credential = "test-product-credential-012345678901234567890123";
        let encoded = recovery_key_hash(credential).expect("argon2 credential hash");
        assert!(encoded.starts_with("$argon2id$"));
        assert!(recovery_key_matches(&encoded, credential));
        assert!(!recovery_key_matches(&encoded, "wrong-credential"));

        let legacy = format!(
            "{:x}",
            Sha256::digest(format!("trnm-player-recovery-v1:{credential}").as_bytes())
        );
        assert!(recovery_key_matches(&legacy, credential));
    }
}
