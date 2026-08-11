use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    config::{
        AlphaAuthorRole, AlphaIdentity, AlphaIdentityScope, DurableQuotaConfig,
        MAX_ALPHA_IDENTITIES,
    },
    error::AppError,
};

const GLOBAL_LOGIN_PRINCIPAL: &[u8] = b"paper-raid-bff/login/global/v1";
const LOGIN_BUCKET_DOMAIN: &[u8] = b"paper-raid-bff/login/preauth-bucket/v1\0";

#[derive(Clone)]
pub struct AccessDirectory {
    pool: PgPool,
    quota: DurableQuotaConfig,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AccessDirectoryStatus {
    pub directory_reachable: bool,
    pub directory_within_capacity: bool,
    pub audit_append_only: bool,
    pub topology_ready: bool,
    pub provisioned_accounts: i64,
    pub active_accounts: i64,
    pub authors: i64,
    pub captains: i64,
    pub evidence_authors: i64,
    pub experiment_authors: i64,
    pub evaluators: i64,
    pub reviewers: i64,
    pub reproducers: i64,
}

pub struct InviteAuthentication {
    pub identity: AlphaIdentity,
    pub expected_session_generation: i64,
    pub redeemed_invite: bool,
}

impl AccessDirectory {
    pub fn new(pool: PgPool, quota: DurableQuotaConfig) -> Self {
        Self { pool, quota }
    }

    /// Authenticates an active credential or atomically converts a still-valid
    /// one-time invitation into the account's first active credential. The
    /// caller gets the same Unauthorized response for every credential/account
    /// denial, and cleartext key material is never persisted.
    pub async fn authenticate_or_redeem(
        &self,
        candidate: &str,
    ) -> Result<InviteAuthentication, AppError> {
        let candidate_hash: [u8; 32] = Sha256::digest(candidate.as_bytes()).into();
        let now = Utc::now();
        let mut tx = self.pool.begin().await?;

        let global_principal: [u8; 32] = Sha256::digest(GLOBAL_LOGIN_PRINCIPAL).into();
        let global_retry = bump_quota(
            &mut tx,
            "login_global",
            &global_principal,
            self.quota.login_window,
            self.quota.login_global_limit,
            now,
        )
        .await?;
        let key_principal = login_bucket_principal(&candidate_hash);
        let key_retry = bump_quota(
            &mut tx,
            "login_bucket",
            &key_principal,
            self.quota.login_window,
            self.quota.login_bucket_limit,
            now,
        )
        .await?;
        if let Some(retry_after_secs) = global_retry.or(key_retry) {
            tx.commit().await?;
            return Err(AppError::RateLimited { retry_after_secs });
        }

        let credential = sqlx::query(
            "SELECT c.credential_id, c.account_id, c.state AS credential_state, \
                    c.expires_at AS credential_expires_at, a.state AS account_state \
             FROM paper_raid_bff_login_credentials c \
             JOIN paper_raid_bff_accounts a ON a.account_id = c.account_id \
             WHERE c.secret_hash = $1 \
             FOR UPDATE OF c, a",
        )
        .bind(candidate_hash.as_slice())
        .fetch_optional(&mut *tx)
        .await?;

        // Always perform both fixed-shape lookups after quota admission. The
        // pre-authentication bucket above is derived without consulting these
        // rows, so response/rate-limit behavior cannot reveal key existence.
        let invitation = sqlx::query(
            "SELECT i.invite_id, i.account_id, i.state AS invite_state, \
                    i.expires_at AS invite_expires_at, \
                    i.credential_expires_at, b.state AS batch_state, \
                    b.expires_at AS batch_expires_at, a.state AS account_state \
             FROM paper_raid_bff_invites i \
             JOIN paper_raid_bff_invite_batches b ON b.batch_id = i.batch_id \
             JOIN paper_raid_bff_accounts a ON a.account_id = i.account_id \
             WHERE i.secret_hash = $1 \
             FOR UPDATE OF i, b, a",
        )
        .bind(candidate_hash.as_slice())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = credential {
            let credential_id: Uuid = row.try_get("credential_id")?;
            let account_id: Uuid = row.try_get("account_id")?;
            let credential_state: String = row.try_get("credential_state")?;
            let account_state: String = row.try_get("account_state")?;
            let credential_expires_at: Option<DateTime<Utc>> =
                row.try_get("credential_expires_at")?;
            if credential_state != "active"
                || account_state != "active"
                || credential_expires_at.is_some_and(|expires_at| expires_at <= now)
            {
                tx.commit().await?;
                return Err(AppError::Unauthorized);
            }
            sqlx::query(
                "UPDATE paper_raid_bff_login_credentials SET last_used_at = $1 \
                 WHERE credential_id = $2 AND state = 'active'",
            )
            .bind(now)
            .bind(credential_id)
            .execute(&mut *tx)
            .await?;
            let identity = identity_for_account_tx(&mut tx, account_id, true).await?;
            let expected_session_generation =
                session_generation_tx(&mut tx, &identity.subject_id).await?;
            audit(&mut tx, None, Some(account_id), "login", "succeeded").await?;
            tx.commit().await?;
            return Ok(InviteAuthentication {
                identity,
                expected_session_generation,
                redeemed_invite: false,
            });
        }

        let Some(row) = invitation else {
            tx.commit().await?;
            return Err(AppError::Unauthorized);
        };
        let invite_id: Uuid = row.try_get("invite_id")?;
        let account_id: Uuid = row.try_get("account_id")?;
        let invite_state: String = row.try_get("invite_state")?;
        let account_state: String = row.try_get("account_state")?;
        let batch_state: String = row.try_get("batch_state")?;
        let invite_expires_at: Option<DateTime<Utc>> = row.try_get("invite_expires_at")?;
        let batch_expires_at: Option<DateTime<Utc>> = row.try_get("batch_expires_at")?;
        let credential_expires_at: Option<DateTime<Utc>> = row.try_get("credential_expires_at")?;
        if candidate.len() < 32
            || invite_state != "issued"
            || batch_state != "active"
            || account_state != "invited"
            || invite_expires_at.is_some_and(|expires_at| expires_at <= now)
            || batch_expires_at.is_some_and(|expires_at| expires_at <= now)
            || credential_expires_at.is_some_and(|expires_at| expires_at <= now)
        {
            tx.commit().await?;
            return Err(AppError::Unauthorized);
        }

        let credential_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO paper_raid_bff_login_credentials \
             (credential_id, account_id, secret_hash, state, expires_at, last_used_at) \
             VALUES ($1, $2, $3, 'active', $4, $5)",
        )
        .bind(credential_id)
        .bind(account_id)
        .bind(candidate_hash.as_slice())
        .bind(credential_expires_at)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE paper_raid_bff_invites SET state = 'redeemed', redeemed_at = $1 \
             WHERE invite_id = $2 AND state = 'issued'",
        )
        .bind(now)
        .bind(invite_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE paper_raid_bff_accounts \
             SET state = 'active', activated_at = $1, updated_at = $1 \
             WHERE account_id = $2 AND state = 'invited'",
        )
        .bind(now)
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
        let identity = identity_for_account_tx(&mut tx, account_id, true).await?;
        audit(
            &mut tx,
            None,
            Some(account_id),
            "invite_redeem",
            "succeeded",
        )
        .await?;
        let expected_session_generation =
            session_generation_tx(&mut tx, &identity.subject_id).await?;
        tx.commit().await?;
        Ok(InviteAuthentication {
            identity,
            expected_session_generation,
            redeemed_invite: true,
        })
    }

    pub async fn identity_for_subject(
        &self,
        subject_id: &str,
    ) -> Result<Option<AlphaIdentity>, AppError> {
        let row = sqlx::query(
            "SELECT account_id FROM paper_raid_bff_accounts \
             WHERE subject_id = $1 AND state = 'active' \
               AND EXISTS ( \
                   SELECT 1 FROM paper_raid_bff_login_credentials c \
                   WHERE c.account_id = paper_raid_bff_accounts.account_id \
                     AND c.state = 'active' \
                     AND (c.expires_at IS NULL OR c.expires_at > now()) \
               )",
        )
        .bind(subject_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let account_id: Uuid = row.try_get("account_id")?;
        let mut tx = self.pool.begin().await?;
        let identity = identity_for_account_tx(&mut tx, account_id, true).await?;
        tx.commit().await?;
        Ok(Some(identity))
    }

    pub async fn status(&self) -> AccessDirectoryStatus {
        let row = sqlx::query(
            "WITH eligible AS ( \
                SELECT a.account_id, a.state FROM paper_raid_bff_accounts a \
                WHERE (a.state = 'active' AND EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_login_credentials c \
                    WHERE c.account_id = a.account_id AND c.state = 'active' \
                      AND (c.expires_at IS NULL OR c.expires_at > now()) \
                )) OR (a.state = 'invited' AND EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_invites i \
                    JOIN paper_raid_bff_invite_batches b ON b.batch_id = i.batch_id \
                    WHERE i.account_id = a.account_id AND i.state = 'issued' \
                      AND b.state = 'active' \
                      AND (i.expires_at IS NULL OR i.expires_at > now()) \
                      AND (i.credential_expires_at IS NULL OR i.credential_expires_at > now()) \
                      AND (b.expires_at IS NULL OR b.expires_at > now()) \
                )) \
             ) \
             SELECT \
                (SELECT count(*) FROM paper_raid_bff_accounts WHERE state <> 'closed') AS directory_accounts, \
                (SELECT count(*) FROM eligible) AS provisioned_accounts, \
                (SELECT count(*) FROM eligible WHERE state = 'active') AS active_accounts, \
                (SELECT count(*) FROM eligible e WHERE EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_scopes s \
                    WHERE s.account_id = e.account_id AND s.scope = 'author' \
                )) AS authors, \
                (SELECT count(*) FROM eligible e WHERE EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_scopes s \
                    WHERE s.account_id = e.account_id AND s.scope = 'author' \
                ) AND EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_author_roles r \
                    WHERE r.account_id = e.account_id AND r.author_role = 'captain' \
                )) AS captains, \
                (SELECT count(*) FROM eligible e WHERE EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_scopes s \
                    WHERE s.account_id = e.account_id AND s.scope = 'author' \
                ) AND EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_author_roles r \
                    WHERE r.account_id = e.account_id AND r.author_role = 'evidence' \
                )) AS evidence_authors, \
                (SELECT count(*) FROM eligible e WHERE EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_scopes s \
                    WHERE s.account_id = e.account_id AND s.scope = 'author' \
                ) AND EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_author_roles r \
                    WHERE r.account_id = e.account_id AND r.author_role = 'experiment' \
                )) AS experiment_authors, \
                (SELECT count(*) FROM eligible e WHERE NOT EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_scopes s \
                    WHERE s.account_id = e.account_id AND s.scope = 'author' \
                ) AND EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_scopes s \
                    WHERE s.account_id = e.account_id AND s.scope = 'evaluator' \
                )) AS evaluators, \
                (SELECT count(*) FROM eligible e WHERE NOT EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_scopes s \
                    WHERE s.account_id = e.account_id AND s.scope = 'author' \
                ) AND EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_scopes s \
                    WHERE s.account_id = e.account_id AND s.scope = 'reviewer' \
                )) AS reviewers, \
                (SELECT count(*) FROM eligible e WHERE NOT EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_scopes s \
                    WHERE s.account_id = e.account_id AND s.scope = 'author' \
                ) AND EXISTS ( \
                    SELECT 1 FROM paper_raid_bff_account_scopes s \
                    WHERE s.account_id = e.account_id AND s.scope = 'reproducer' \
                )) AS reproducers, \
                EXISTS ( \
                    SELECT 1 FROM eligible c, eligible v, eligible x \
                    WHERE c.account_id <> v.account_id \
                      AND c.account_id <> x.account_id \
                      AND v.account_id <> x.account_id \
                      AND EXISTS (SELECT 1 FROM paper_raid_bff_account_scopes s WHERE s.account_id = c.account_id AND s.scope = 'author') \
                      AND EXISTS (SELECT 1 FROM paper_raid_bff_account_scopes s WHERE s.account_id = v.account_id AND s.scope = 'author') \
                      AND EXISTS (SELECT 1 FROM paper_raid_bff_account_scopes s WHERE s.account_id = x.account_id AND s.scope = 'author') \
                      AND EXISTS (SELECT 1 FROM paper_raid_bff_account_author_roles r WHERE r.account_id = c.account_id AND r.author_role = 'captain') \
                      AND EXISTS (SELECT 1 FROM paper_raid_bff_account_author_roles r WHERE r.account_id = v.account_id AND r.author_role = 'evidence') \
                      AND EXISTS (SELECT 1 FROM paper_raid_bff_account_author_roles r WHERE r.account_id = x.account_id AND r.author_role = 'experiment') \
                ) AS author_topology_ready, \
                EXISTS ( \
                    SELECT 1 FROM eligible e, eligible r1, eligible r2, eligible p \
                    WHERE e.account_id <> r1.account_id AND e.account_id <> r2.account_id \
                      AND e.account_id <> p.account_id AND r1.account_id <> r2.account_id \
                      AND r1.account_id <> p.account_id AND r2.account_id <> p.account_id \
                      AND NOT EXISTS (SELECT 1 FROM paper_raid_bff_account_scopes s WHERE s.account_id = e.account_id AND s.scope = 'author') \
                      AND NOT EXISTS (SELECT 1 FROM paper_raid_bff_account_scopes s WHERE s.account_id = r1.account_id AND s.scope = 'author') \
                      AND NOT EXISTS (SELECT 1 FROM paper_raid_bff_account_scopes s WHERE s.account_id = r2.account_id AND s.scope = 'author') \
                      AND NOT EXISTS (SELECT 1 FROM paper_raid_bff_account_scopes s WHERE s.account_id = p.account_id AND s.scope = 'author') \
                      AND EXISTS (SELECT 1 FROM paper_raid_bff_account_scopes s WHERE s.account_id = e.account_id AND s.scope = 'evaluator') \
                      AND EXISTS (SELECT 1 FROM paper_raid_bff_account_scopes s WHERE s.account_id = r1.account_id AND s.scope = 'reviewer') \
                      AND EXISTS (SELECT 1 FROM paper_raid_bff_account_scopes s WHERE s.account_id = r2.account_id AND s.scope = 'reviewer') \
                      AND EXISTS (SELECT 1 FROM paper_raid_bff_account_scopes s WHERE s.account_id = p.account_id AND s.scope = 'reproducer') \
                ) AS review_topology_ready, \
                EXISTS ( \
                    SELECT 1 FROM pg_trigger \
                    WHERE tgname = 'paper_raid_bff_access_audit_append_only' \
                      AND tgrelid = 'paper_raid_bff_access_audit'::regclass \
                      AND NOT tgisinternal \
                ) AS audit_append_only",
        )
        .fetch_one(&self.pool)
        .await;
        let Ok(row) = row else {
            return AccessDirectoryStatus::default();
        };
        let directory_accounts = row
            .try_get::<i64, _>("directory_accounts")
            .unwrap_or(i64::MAX);
        let provisioned_accounts = row
            .try_get::<i64, _>("provisioned_accounts")
            .unwrap_or(i64::MAX);
        let active_accounts = row.try_get::<i64, _>("active_accounts").unwrap_or(0);
        let authors = row.try_get::<i64, _>("authors").unwrap_or(0);
        let captains = row.try_get::<i64, _>("captains").unwrap_or(0);
        let evidence_authors = row.try_get::<i64, _>("evidence_authors").unwrap_or(0);
        let experiment_authors = row.try_get::<i64, _>("experiment_authors").unwrap_or(0);
        let evaluators = row.try_get::<i64, _>("evaluators").unwrap_or(0);
        let reviewers = row.try_get::<i64, _>("reviewers").unwrap_or(0);
        let reproducers = row.try_get::<i64, _>("reproducers").unwrap_or(0);
        let directory_reachable = true;
        let directory_within_capacity = directory_accounts <= MAX_ALPHA_IDENTITIES as i64;
        let audit_append_only = row.try_get::<bool, _>("audit_append_only").unwrap_or(false);
        let topology_ready = directory_within_capacity
            && audit_append_only
            && row
                .try_get::<bool, _>("author_topology_ready")
                .unwrap_or(false)
            && row
                .try_get::<bool, _>("review_topology_ready")
                .unwrap_or(false);
        AccessDirectoryStatus {
            directory_reachable,
            directory_within_capacity,
            audit_append_only,
            topology_ready,
            provisioned_accounts,
            active_accounts,
            authors,
            captains,
            evidence_authors,
            experiment_authors,
            evaluators,
            reviewers,
            reproducers,
        }
    }
}

fn login_bucket_principal(candidate_hash: &[u8; 32]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(LOGIN_BUCKET_DOMAIN);
    digest.update([candidate_hash[0]]);
    digest.finalize().into()
}

pub async fn enforce_mutation_quota(
    pool: &PgPool,
    quota: DurableQuotaConfig,
    subject_id: &str,
) -> Result<(), AppError> {
    let principal_hash: [u8; 32] = Sha256::digest(subject_id.as_bytes()).into();
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    let retry_after = bump_quota(
        &mut tx,
        "authenticated_mutation",
        &principal_hash,
        quota.mutation_window,
        quota.mutation_account_limit,
        now,
    )
    .await?;
    tx.commit().await?;
    if let Some(retry_after_secs) = retry_after {
        return Err(AppError::RateLimited { retry_after_secs });
    }
    Ok(())
}

pub(crate) async fn bump_quota(
    tx: &mut Transaction<'_, Postgres>,
    quota_kind: &str,
    principal_hash: &[u8],
    window: Duration,
    limit: u64,
    now: DateTime<Utc>,
) -> Result<Option<u64>, sqlx::Error> {
    let window_seconds = i64::try_from(window.as_secs()).unwrap_or(i64::MAX);
    let start_seconds = now.timestamp().div_euclid(window_seconds) * window_seconds;
    let window_started_at = Utc.timestamp_opt(start_seconds, 0).single().unwrap_or(now);
    let count: i64 = sqlx::query_scalar(
        "INSERT INTO paper_raid_bff_quota_windows \
         (quota_kind, principal_hash, window_started_at, request_count) \
         VALUES ($1, $2, $3, 1) \
         ON CONFLICT (quota_kind, principal_hash, window_started_at) \
         DO UPDATE SET request_count = LEAST( \
                           paper_raid_bff_quota_windows.request_count + 1, $4 \
                       ), \
                       updated_at = now() \
         RETURNING request_count",
    )
    .bind(quota_kind)
    .bind(principal_hash)
    .bind(window_started_at)
    .bind(
        i64::try_from(limit)
            .unwrap_or(i64::MAX - 1)
            .saturating_add(1),
    )
    .fetch_one(&mut **tx)
    .await?;
    if count <= i64::try_from(limit).unwrap_or(i64::MAX) {
        return Ok(None);
    }
    let window_end = window_started_at + chrono::Duration::seconds(window_seconds);
    Ok(Some(
        u64::try_from((window_end - now).num_seconds())
            .unwrap_or(1)
            .max(1),
    ))
}

async fn identity_for_account_tx(
    tx: &mut Transaction<'_, Postgres>,
    account_id: Uuid,
    require_active: bool,
) -> Result<AlphaIdentity, AppError> {
    let row = sqlx::query(
        "SELECT subject_id, display_name, nakama_user_id, player_id, state \
         FROM paper_raid_bff_accounts WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(AppError::Unauthorized)?;
    let state: String = row.try_get("state")?;
    if require_active && state != "active" {
        return Err(AppError::Unauthorized);
    }
    let scope_values: Vec<String> = sqlx::query_scalar(
        "SELECT scope FROM paper_raid_bff_account_scopes \
         WHERE account_id = $1 ORDER BY scope",
    )
    .bind(account_id)
    .fetch_all(&mut **tx)
    .await?;
    let role_values: Vec<String> = sqlx::query_scalar(
        "SELECT author_role FROM paper_raid_bff_account_author_roles \
         WHERE account_id = $1 ORDER BY author_role",
    )
    .bind(account_id)
    .fetch_all(&mut **tx)
    .await?;
    let scopes = scope_values
        .iter()
        .map(|value| parse_scope(value))
        .collect::<Result<Vec<_>, _>>()?;
    let roles = role_values
        .iter()
        .map(|value| parse_role(value))
        .collect::<Result<Vec<_>, _>>()?;
    AlphaIdentity::from_access_directory(
        row.try_get("subject_id")?,
        row.try_get("display_name")?,
        row.try_get("nakama_user_id")?,
        row.try_get("player_id")?,
        scopes,
        roles,
    )
    .map_err(|error| {
        tracing::error!(%error, %account_id, "invalid fail-closed invite account directory row");
        AppError::Internal
    })
}

fn parse_scope(value: &str) -> Result<AlphaIdentityScope, AppError> {
    match value {
        "author" => Ok(AlphaIdentityScope::Author),
        "evaluator" => Ok(AlphaIdentityScope::Evaluator),
        "reviewer" => Ok(AlphaIdentityScope::Reviewer),
        "reproducer" => Ok(AlphaIdentityScope::Reproducer),
        _ => Err(AppError::Internal),
    }
}

fn parse_role(value: &str) -> Result<AlphaAuthorRole, AppError> {
    match value {
        "captain" => Ok(AlphaAuthorRole::Captain),
        "evidence" => Ok(AlphaAuthorRole::Evidence),
        "experiment" => Ok(AlphaAuthorRole::Experiment),
        _ => Err(AppError::Internal),
    }
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    operator_subject: Option<&str>,
    account_id: Option<Uuid>,
    action: &str,
    outcome: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO paper_raid_bff_access_audit \
         (audit_id, operator_subject, account_id, action, outcome, metadata) \
         VALUES ($1, $2, $3, $4, $5, '{}'::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(operator_subject)
    .bind(account_id)
    .bind(action)
    .bind(outcome)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn session_generation_tx(
    tx: &mut Transaction<'_, Postgres>,
    subject_id: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query(
        "INSERT INTO paper_raid_bff_session_generation(subject_id, generation) \
         VALUES ($1, 0) ON CONFLICT (subject_id) DO NOTHING",
    )
    .bind(subject_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query_scalar(
        "SELECT generation FROM paper_raid_bff_session_generation \
         WHERE subject_id = $1 FOR UPDATE",
    )
    .bind(subject_id)
    .fetch_one(&mut **tx)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn directory_values_are_deny_unknown() {
        assert!(matches!(
            parse_scope("author"),
            Ok(AlphaIdentityScope::Author)
        ));
        assert!(parse_scope("admin").is_err());
        assert!(matches!(
            parse_role("experiment"),
            Ok(AlphaAuthorRole::Experiment)
        ));
        assert!(parse_role("operator").is_err());
    }

    #[test]
    fn secret_hashes_are_fixed_length_and_domain_values_differ() {
        let one: [u8; 32] = Sha256::digest(b"one-secret-with-at-least-32-bytes-000").into();
        let two: [u8; 32] = Sha256::digest(b"two-secret-with-at-least-32-bytes-000").into();
        assert_eq!(one.len(), 32);
        assert_ne!(one, two);
        assert_ne!(one.as_slice(), b"one-secret-with-at-least-32-bytes-000");
    }

    #[test]
    fn preauthentication_quota_has_at_most_256_existence_independent_buckets() {
        let buckets = (0_u16..=255)
            .map(|prefix| {
                let mut candidate = [0_u8; 32];
                candidate[0] = prefix as u8;
                login_bucket_principal(&candidate)
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(buckets.len(), 256);
        let mut known_shaped = [1_u8; 32];
        let mut unknown_shaped = [2_u8; 32];
        known_shaped[0] = 73;
        unknown_shaped[0] = 73;
        assert_eq!(
            login_bucket_principal(&known_shaped),
            login_bucket_principal(&unknown_shaped)
        );
        unknown_shaped[0] = 74;
        assert_ne!(
            login_bucket_principal(&unknown_shaped),
            login_bucket_principal(&known_shaped)
        );
    }

    #[tokio::test]
    async fn real_postgres_invite_redeems_once_and_unknown_keys_share_one_bucket() {
        let Ok(database_url) = std::env::var("PAPER_RAID_BFF_TEST_DATABASE_URL") else {
            eprintln!("PAPER_RAID_BFF_TEST_DATABASE_URL is unset; invite directory gate skipped");
            return;
        };
        let pool = db::connect(&database_url)
            .await
            .expect("connect PostgreSQL");
        db::migrate(&pool).await.expect("migrate PostgreSQL");
        let suffix = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let batch_id = Uuid::new_v4();
        let invite_id = Uuid::new_v4();
        let subject = format!("invite-test-{suffix}");
        let secret = format!("invite-secret-{suffix}-with-adequate-entropy");
        let secret_hash: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
        sqlx::query(
            "INSERT INTO paper_raid_bff_invite_batches \
             (batch_id, label, state, max_issued, issued_count, expires_at) \
             VALUES ($1, $2, 'active', 1, 1, now() + interval '1 hour')",
        )
        .bind(batch_id)
        .bind(format!("test-{suffix}"))
        .execute(&pool)
        .await
        .expect("insert batch");
        sqlx::query(
            "INSERT INTO paper_raid_bff_accounts \
             (account_id, subject_id, display_name, nakama_user_id, player_id, state) \
             VALUES ($1, $2, 'Invite Test', $3, $4, 'invited')",
        )
        .bind(account_id)
        .bind(&subject)
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .expect("insert account");
        sqlx::query(
            "INSERT INTO paper_raid_bff_account_scopes(account_id, scope) VALUES ($1, 'author')",
        )
        .bind(account_id)
        .execute(&pool)
        .await
        .expect("insert scope");
        sqlx::query(
            "INSERT INTO paper_raid_bff_account_author_roles(account_id, author_role) \
             VALUES ($1, 'captain')",
        )
        .bind(account_id)
        .execute(&pool)
        .await
        .expect("insert role");
        sqlx::query(
            "INSERT INTO paper_raid_bff_invites \
             (invite_id, batch_id, account_id, secret_hash, state, expires_at, credential_expires_at) \
             VALUES ($1, $2, $3, $4, 'issued', now() + interval '30 minutes', now() + interval '1 hour')",
        )
        .bind(invite_id)
        .bind(batch_id)
        .bind(account_id)
        .bind(secret_hash.as_slice())
        .execute(&pool)
        .await
        .expect("insert invite");
        let directory = AccessDirectory::new(
            pool.clone(),
            DurableQuotaConfig {
                login_window: Duration::from_secs(3600),
                login_global_limit: 100_000,
                login_bucket_limit: 100,
                mutation_window: Duration::from_secs(60),
                mutation_account_limit: 100,
            },
        );
        let first = directory
            .authenticate_or_redeem(&secret)
            .await
            .expect("redeem invite");
        assert_eq!(first.identity.subject_id, subject);
        let second = directory
            .authenticate_or_redeem(&secret)
            .await
            .expect("credential login");
        assert_eq!(second.identity.player_id, first.identity.player_id);
        assert_eq!(second.expected_session_generation, 0);
        let audit_id: Uuid = sqlx::query_scalar(
            "SELECT audit_id FROM paper_raid_bff_access_audit \
             WHERE account_id = $1 ORDER BY occurred_at LIMIT 1",
        )
        .bind(account_id)
        .fetch_one(&pool)
        .await
        .expect("access audit");
        assert!(sqlx::query(
            "UPDATE paper_raid_bff_access_audit SET outcome = 'denied' WHERE audit_id = $1",
        )
        .bind(audit_id)
        .execute(&pool)
        .await
        .is_err());

        assert!(matches!(
            directory
                .authenticate_or_redeem(&format!("unknown-one-{suffix}"))
                .await,
            Err(AppError::Unauthorized)
        ));
        assert!(matches!(
            directory
                .authenticate_or_redeem(&format!("unknown-two-{suffix}"))
                .await,
            Err(AppError::Unauthorized)
        ));
        let bucket_count: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT principal_hash) FROM paper_raid_bff_quota_windows \
             WHERE quota_kind = 'login_bucket'",
        )
        .fetch_one(&pool)
        .await
        .expect("pre-authentication bucket count");
        assert!(bucket_count <= 256);

        sqlx::query(
            "UPDATE paper_raid_bff_accounts \
             SET state = 'suspended', suspended_at = now(), updated_at = now() \
             WHERE account_id = $1",
        )
        .bind(account_id)
        .execute(&pool)
        .await
        .expect("suspend account");
        assert!(directory
            .identity_for_subject(&subject)
            .await
            .expect("directory read")
            .is_none());
        assert!(matches!(
            directory.authenticate_or_redeem(&secret).await,
            Err(AppError::Unauthorized)
        ));
    }
}
