use std::{collections::BTreeMap, env};

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use paper_raid_bff::{config::MAX_ALPHA_IDENTITIES, db};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("paper-raid-accessctl: {error:#}");
        std::process::exit(2);
    }
}

async fn run() -> Result<()> {
    if env::var("PAPER_RAID_BFF_IDENTITY_MODE").as_deref() != Ok("invite_alpha") {
        bail!("PAPER_RAID_BFF_IDENTITY_MODE must be invite_alpha");
    }
    let database_url = required_env("PAPER_RAID_ACCESS_DATABASE_URL")?;
    let operator = required_env("PAPER_RAID_ACCESS_OPERATOR_SUBJECT")?;
    validate_identifier("operator subject", &operator)?;
    let retention_policy_id = required_env("PAPER_RAID_BFF_ACCESS_RETENTION_POLICY_ID")?;
    validate_identifier("retention policy id", &retention_policy_id)?;
    let (command, args) = parse_args()?;
    let pool = db::connect(&database_url)
        .await
        .context("connect access database")?;
    db::migrate(&pool)
        .await
        .context("migrate access database")?;

    let output = match command.as_str() {
        "batch-create" => batch_create(&pool, &operator, &args).await?,
        "batch-pause" => batch_state(&pool, &operator, &args, "paused").await?,
        "batch-resume" => batch_state(&pool, &operator, &args, "active").await?,
        "batch-revoke" => batch_state(&pool, &operator, &args, "revoked").await?,
        "invite-issue" => invite_issue(&pool, &operator, &args).await?,
        "invite-revoke" => invite_revoke(&pool, &operator, &args).await?,
        "credential-rotate" => credential_rotate(&pool, &operator, &args).await?,
        "account-suspend" => account_state(&pool, &operator, &args, "suspended").await?,
        "account-reactivate" => account_state(&pool, &operator, &args, "active").await?,
        "account-close" => account_state(&pool, &operator, &args, "closed").await?,
        "account-export" => account_export(&pool, &operator, &args).await?,
        "prune" => prune(&pool, &operator, &retention_policy_id, &args).await?,
        _ => bail!(
            "unsupported command; expected batch-create|batch-pause|batch-resume|batch-revoke|invite-issue|invite-revoke|credential-rotate|account-suspend|account-reactivate|account-close|account-export|prune"
        ),
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

async fn batch_create(pool: &PgPool, operator: &str, args: &Args) -> Result<Value> {
    args.only(&["label", "max-issued", "expires-at"])?;
    let label = args.required("label")?;
    validate_label(label)?;
    let max_issued = args
        .required("max-issued")?
        .parse::<i32>()
        .map_err(|_| anyhow!("max-issued must be an integer"))?;
    if !(1..=MAX_ALPHA_IDENTITIES as i32).contains(&max_issued) {
        bail!("max-issued must be between 1 and {MAX_ALPHA_IDENTITIES}");
    }
    let expires_at = parse_expiry(args.required("expires-at")?)?;
    if expires_at.is_some_and(|value| value <= Utc::now()) {
        bail!("batch expiry must be in the future");
    }
    let batch_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO paper_raid_bff_invite_batches \
         (batch_id, label, state, max_issued, expires_at) \
         VALUES ($1, $2, 'active', $3, $4)",
    )
    .bind(batch_id)
    .bind(label)
    .bind(max_issued)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        operator,
        None,
        "batch_create",
        json!({"batch_id": batch_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(
        json!({"schema":"paper-raid-bff.accessctl.result.v1","batch_id":batch_id,"state":"active","max_issued":max_issued,"expires_at":expires_at}),
    )
}

async fn batch_state(pool: &PgPool, operator: &str, args: &Args, state: &str) -> Result<Value> {
    args.only(&["batch-id"])?;
    let batch_id = parse_uuid(args.required("batch-id")?, "batch-id")?;
    let mut tx = pool.begin().await?;
    let update = if state == "active" {
        "UPDATE paper_raid_bff_invite_batches SET state = $1, updated_at = now() \
         WHERE batch_id = $2 AND state <> 'revoked' \
           AND (expires_at IS NULL OR expires_at > now())"
    } else {
        "UPDATE paper_raid_bff_invite_batches SET state = $1, updated_at = now() \
         WHERE batch_id = $2 AND state <> 'revoked'"
    };
    let result = sqlx::query(update)
        .bind(state)
        .bind(batch_id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() != 1 {
        bail!("batch not found or already revoked");
    }
    if state == "revoked" {
        sqlx::query(
            "UPDATE paper_raid_bff_invites SET state = 'revoked', revoked_at = now() \
             WHERE batch_id = $1 AND state = 'issued'",
        )
        .bind(batch_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE paper_raid_bff_accounts a SET state = 'closed', closed_at = now(), updated_at = now() \
             FROM paper_raid_bff_invites i \
             WHERE i.batch_id = $1 AND i.account_id = a.account_id AND a.state = 'invited'",
        )
        .bind(batch_id)
        .execute(&mut *tx)
        .await?;
    }
    audit(
        &mut tx,
        operator,
        None,
        &format!("batch_{state}"),
        json!({"batch_id": batch_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(json!({"schema":"paper-raid-bff.accessctl.result.v1","batch_id":batch_id,"state":state}))
}

async fn invite_issue(pool: &PgPool, operator: &str, args: &Args) -> Result<Value> {
    args.only(&[
        "batch-id",
        "subject",
        "display-name",
        "nakama-user-id",
        "player-id",
        "scopes",
        "author-roles",
        "expires-at",
        "credential-expires-at",
    ])?;
    let batch_id = parse_uuid(args.required("batch-id")?, "batch-id")?;
    let subject = args.required("subject")?;
    validate_identifier("subject", subject)?;
    let display_name = args.required("display-name")?;
    validate_display_name(display_name)?;
    let nakama_user_id = parse_uuid(args.required("nakama-user-id")?, "nakama-user-id")?;
    let player_id = parse_uuid(args.required("player-id")?, "player-id")?;
    let scopes = parse_set(
        args.required("scopes")?,
        &["author", "evaluator", "reviewer", "reproducer"],
        false,
    )?;
    let roles = parse_set(
        args.required("author-roles")?,
        &["captain", "evidence", "experiment"],
        true,
    )?;
    if scopes.contains(&"author".to_string()) != !roles.is_empty() {
        bail!("author scope requires at least one author role; non-authors require none");
    }
    if scopes.contains(&"author".to_string()) && scopes.len() != 1 {
        bail!("author invitations cannot also carry independent review scopes");
    }
    let expires_at = parse_expiry(args.required("expires-at")?)?;
    if expires_at.is_some_and(|value| value <= Utc::now()) {
        bail!("invite expiry must be in the future");
    }
    let credential_expires_at = parse_expiry(args.required("credential-expires-at")?)?;
    if credential_expires_at.is_some_and(|value| value <= Utc::now()) {
        bail!("credential expiry must be in the future");
    }
    let secret = random_secret();
    let secret_hash: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
    let account_id = Uuid::new_v4();
    let invite_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(742381917)")
        .execute(&mut *tx)
        .await?;
    let account_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM paper_raid_bff_accounts WHERE state <> 'closed'")
            .fetch_one(&mut *tx)
            .await?;
    if account_count >= MAX_ALPHA_IDENTITIES as i64 {
        bail!("invite alpha account cap reached");
    }
    let batch = sqlx::query(
        "SELECT state, expires_at, max_issued, issued_count \
         FROM paper_raid_bff_invite_batches \
         WHERE batch_id = $1 FOR UPDATE",
    )
    .bind(batch_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow!("batch not found"))?;
    let batch_state: String = batch.try_get("state")?;
    let batch_expires_at: Option<DateTime<Utc>> = batch.try_get("expires_at")?;
    let max_issued: i32 = batch.try_get("max_issued")?;
    let issued_count: i32 = batch.try_get("issued_count")?;
    if batch_state != "active" || batch_expires_at.is_some_and(|value| value <= Utc::now()) {
        bail!("batch is not active");
    }
    if issued_count >= max_issued {
        bail!("batch issue limit reached");
    }
    if let (Some(invite_expiry), Some(batch_expiry)) = (expires_at, batch_expires_at) {
        if invite_expiry > batch_expiry {
            bail!("invite expiry exceeds batch expiry");
        }
    }
    sqlx::query(
        "INSERT INTO paper_raid_bff_accounts \
         (account_id, subject_id, display_name, nakama_user_id, player_id, state) \
         VALUES ($1, $2, $3, $4, $5, 'invited')",
    )
    .bind(account_id)
    .bind(subject)
    .bind(display_name)
    .bind(nakama_user_id)
    .bind(player_id)
    .execute(&mut *tx)
    .await?;
    for scope in &scopes {
        sqlx::query("INSERT INTO paper_raid_bff_account_scopes(account_id, scope) VALUES ($1, $2)")
            .bind(account_id)
            .bind(scope)
            .execute(&mut *tx)
            .await?;
    }
    for role in &roles {
        sqlx::query(
            "INSERT INTO paper_raid_bff_account_author_roles(account_id, author_role) \
             VALUES ($1, $2)",
        )
        .bind(account_id)
        .bind(role)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO paper_raid_bff_invites \
         (invite_id, batch_id, account_id, secret_hash, state, expires_at, credential_expires_at) \
         VALUES ($1, $2, $3, $4, 'issued', $5, $6)",
    )
    .bind(invite_id)
    .bind(batch_id)
    .bind(account_id)
    .bind(secret_hash.as_slice())
    .bind(expires_at)
    .bind(credential_expires_at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE paper_raid_bff_invite_batches \
         SET issued_count = issued_count + 1, updated_at = now() \
         WHERE batch_id = $1 AND issued_count < max_issued",
    )
    .bind(batch_id)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        operator,
        Some(account_id),
        "invite_issue",
        json!({"invite_id":invite_id,"batch_id":batch_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(json!({
        "schema":"paper-raid-bff.accessctl.secret-result.v1",
        "invite_id":invite_id,
        "account_id":account_id,
        "subject_id":subject,
        "login_credential":secret,
        "secret_delivery":"displayed_once_not_stored",
        "invite_expires_at":expires_at,
        "credential_expires_at":credential_expires_at
    }))
}

async fn invite_revoke(pool: &PgPool, operator: &str, args: &Args) -> Result<Value> {
    args.only(&["invite-id"])?;
    let invite_id = parse_uuid(args.required("invite-id")?, "invite-id")?;
    let mut tx = pool.begin().await?;
    let account_id: Uuid = sqlx::query_scalar(
        "UPDATE paper_raid_bff_invites SET state = 'revoked', revoked_at = now() \
         WHERE invite_id = $1 AND state = 'issued' RETURNING account_id",
    )
    .bind(invite_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow!("issued invite not found"))?;
    sqlx::query(
        "UPDATE paper_raid_bff_accounts SET state = 'closed', closed_at = now(), updated_at = now() \
         WHERE account_id = $1 AND state = 'invited'",
    )
    .bind(account_id)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        operator,
        Some(account_id),
        "invite_revoke",
        json!({"invite_id":invite_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(
        json!({"schema":"paper-raid-bff.accessctl.result.v1","invite_id":invite_id,"state":"revoked"}),
    )
}

async fn credential_rotate(pool: &PgPool, operator: &str, args: &Args) -> Result<Value> {
    args.only(&["subject", "expires-at"])?;
    let subject = args.required("subject")?;
    validate_identifier("subject", subject)?;
    let expires_at = parse_expiry(args.required("expires-at")?)?;
    if expires_at.is_some_and(|value| value <= Utc::now()) {
        bail!("credential expiry must be in the future");
    }
    let secret = random_secret();
    let secret_hash: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
    let credential_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    let account_id: Uuid = sqlx::query_scalar(
        "SELECT account_id FROM paper_raid_bff_accounts \
         WHERE subject_id = $1 AND state = 'active' FOR UPDATE",
    )
    .bind(subject)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow!("active account not found"))?;
    sqlx::query(
        "UPDATE paper_raid_bff_login_credentials SET state = 'revoked', revoked_at = now() \
         WHERE account_id = $1 AND state = 'active'",
    )
    .bind(account_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO paper_raid_bff_login_credentials \
         (credential_id, account_id, secret_hash, state, expires_at) \
         VALUES ($1, $2, $3, 'active', $4)",
    )
    .bind(credential_id)
    .bind(account_id)
    .bind(secret_hash.as_slice())
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    revoke_sessions(&mut tx, subject).await?;
    audit(
        &mut tx,
        operator,
        Some(account_id),
        "credential_rotate",
        json!({"credential_id":credential_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(json!({
        "schema":"paper-raid-bff.accessctl.secret-result.v1",
        "subject_id":subject,
        "credential_id":credential_id,
        "login_credential":secret,
        "secret_delivery":"displayed_once_not_stored",
        "expires_at":expires_at
    }))
}

async fn account_state(pool: &PgPool, operator: &str, args: &Args, target: &str) -> Result<Value> {
    args.only(&["subject"])?;
    let subject = args.required("subject")?;
    validate_identifier("subject", subject)?;
    let mut tx = pool.begin().await?;
    let query = match target {
        "suspended" => {
            "UPDATE paper_raid_bff_accounts SET state = 'suspended', suspended_at = now(), updated_at = now() WHERE subject_id = $1 AND state = 'active' RETURNING account_id"
        }
        "active" => {
            "UPDATE paper_raid_bff_accounts SET state = 'active', suspended_at = NULL, updated_at = now() WHERE subject_id = $1 AND state = 'suspended' RETURNING account_id"
        }
        "closed" => {
            "UPDATE paper_raid_bff_accounts SET state = 'closed', closed_at = now(), updated_at = now() WHERE subject_id = $1 AND state <> 'closed' RETURNING account_id"
        }
        _ => bail!("invalid account target state"),
    };
    let account_id: Uuid = sqlx::query_scalar(query)
        .bind(subject)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| anyhow!("account is not in the required source state"))?;
    if target != "active" {
        revoke_sessions(&mut tx, subject).await?;
    }
    if target == "closed" {
        sqlx::query(
            "UPDATE paper_raid_bff_login_credentials SET state = 'revoked', revoked_at = now() \
             WHERE account_id = $1 AND state = 'active'",
        )
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE paper_raid_bff_invites SET state = 'revoked', revoked_at = now() \
             WHERE account_id = $1 AND state = 'issued'",
        )
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
    }
    audit(
        &mut tx,
        operator,
        Some(account_id),
        &format!("account_{target}"),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(json!({"schema":"paper-raid-bff.accessctl.result.v1","subject_id":subject,"state":target}))
}

async fn account_export(pool: &PgPool, operator: &str, args: &Args) -> Result<Value> {
    args.only(&["subject"])?;
    let subject = args.required("subject")?;
    validate_identifier("subject", subject)?;
    let account = sqlx::query(
        "SELECT account_id, subject_id, display_name, nakama_user_id, player_id, state, \
                created_at, activated_at, suspended_at, closed_at, updated_at \
         FROM paper_raid_bff_accounts WHERE subject_id = $1",
    )
    .bind(subject)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow!("account not found"))?;
    let account_id: Uuid = account.try_get("account_id")?;
    let scopes: Vec<String> = sqlx::query_scalar(
        "SELECT scope FROM paper_raid_bff_account_scopes WHERE account_id = $1 ORDER BY scope",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT author_role FROM paper_raid_bff_account_author_roles WHERE account_id = $1 ORDER BY author_role",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let credential_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM paper_raid_bff_login_credentials WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await?;
    let credential_rows = sqlx::query(
        "SELECT credential_id, state, expires_at, created_at, last_used_at, revoked_at \
         FROM paper_raid_bff_login_credentials WHERE account_id = $1 ORDER BY created_at",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let mut credentials = Vec::with_capacity(credential_rows.len());
    for row in credential_rows {
        credentials.push(json!({
            "credential_id":row.try_get::<Uuid,_>("credential_id")?,
            "state":row.try_get::<String,_>("state")?,
            "expires_at":row.try_get::<Option<DateTime<Utc>>,_>("expires_at")?,
            "created_at":row.try_get::<DateTime<Utc>,_>("created_at")?,
            "last_used_at":row.try_get::<Option<DateTime<Utc>>,_>("last_used_at")?,
            "revoked_at":row.try_get::<Option<DateTime<Utc>>,_>("revoked_at")?
        }));
    }
    let invite_rows = sqlx::query(
        "SELECT invite_id, batch_id, state, expires_at, credential_expires_at, \
                created_at, redeemed_at, revoked_at \
         FROM paper_raid_bff_invites WHERE account_id = $1 ORDER BY created_at",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let mut invites = Vec::with_capacity(invite_rows.len());
    for row in invite_rows {
        invites.push(json!({
            "invite_id":row.try_get::<Uuid,_>("invite_id")?,
            "batch_id":row.try_get::<Uuid,_>("batch_id")?,
            "state":row.try_get::<String,_>("state")?,
            "expires_at":row.try_get::<Option<DateTime<Utc>>,_>("expires_at")?,
            "credential_expires_at":row.try_get::<Option<DateTime<Utc>>,_>("credential_expires_at")?,
            "created_at":row.try_get::<DateTime<Utc>,_>("created_at")?,
            "redeemed_at":row.try_get::<Option<DateTime<Utc>>,_>("redeemed_at")?,
            "revoked_at":row.try_get::<Option<DateTime<Utc>>,_>("revoked_at")?
        }));
    }
    let audit_rows = sqlx::query(
        "SELECT action, outcome, metadata, occurred_at \
         FROM paper_raid_bff_access_audit WHERE account_id = $1 ORDER BY occurred_at",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let mut access_audit = Vec::with_capacity(audit_rows.len());
    for row in audit_rows {
        access_audit.push(json!({
            "action":row.try_get::<String,_>("action")?,
            "outcome":row.try_get::<String,_>("outcome")?,
            "metadata":row.try_get::<Value,_>("metadata")?,
            "occurred_at":row.try_get::<DateTime<Utc>,_>("occurred_at")?
        }));
    }
    let export = json!({
        "schema":"paper-raid-bff.account-export.v1",
        "account_id":account_id,
        "subject_id":account.try_get::<String,_>("subject_id")?,
        "display_name":account.try_get::<String,_>("display_name")?,
        "nakama_user_id":account.try_get::<Uuid,_>("nakama_user_id")?,
        "player_id":account.try_get::<Uuid,_>("player_id")?,
        "state":account.try_get::<String,_>("state")?,
        "created_at":account.try_get::<DateTime<Utc>,_>("created_at")?,
        "activated_at":account.try_get::<Option<DateTime<Utc>>,_>("activated_at")?,
        "suspended_at":account.try_get::<Option<DateTime<Utc>>,_>("suspended_at")?,
        "closed_at":account.try_get::<Option<DateTime<Utc>>,_>("closed_at")?,
        "updated_at":account.try_get::<DateTime<Utc>,_>("updated_at")?,
        "scopes":scopes,
        "author_roles":roles,
        "credential_count":credential_count,
        "credentials":credentials,
        "invites":invites,
        "access_audit":access_audit,
        "contains_secret_material":false
    });
    let mut tx = pool.begin().await?;
    audit(
        &mut tx,
        operator,
        Some(account_id),
        "account_export",
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(export)
}

async fn prune(
    pool: &PgPool,
    operator: &str,
    configured_policy_id: &str,
    args: &Args,
) -> Result<Value> {
    args.only(&["before", "policy-id"])?;
    let policy_id = args.required("policy-id")?;
    if policy_id != configured_policy_id {
        bail!("policy-id does not match PAPER_RAID_BFF_ACCESS_RETENTION_POLICY_ID");
    }
    let cutoff = parse_timestamp(args.required("before")?, "before")?;
    if cutoff >= Utc::now() {
        bail!("prune cutoff must be in the past");
    }
    let mut tx = pool.begin().await?;
    let sessions = sqlx::query(
        "DELETE FROM paper_raid_bff_sessions \
         WHERE expires_at < $1 OR (revoked_at IS NOT NULL AND revoked_at < $1)",
    )
    .bind(cutoff)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let assertions = sqlx::query("DELETE FROM paper_raid_bff_assertions WHERE expires_at < $1")
        .bind(cutoff)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let idempotency = sqlx::query(
        "DELETE FROM paper_raid_bff_idempotency \
         WHERE state = 'completed' AND completed_at < $1",
    )
    .bind(cutoff)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let read_cache = sqlx::query("DELETE FROM paper_raid_bff_read_cache WHERE expires_at < $1")
        .bind(cutoff)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let product_events =
        sqlx::query("DELETE FROM paper_raid_bff_product_events WHERE occurred_at < $1")
            .bind(cutoff)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    let quotas =
        sqlx::query("DELETE FROM paper_raid_bff_quota_windows WHERE window_started_at < $1")
            .bind(cutoff)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    // Signed Agent response bytes are a <=60s replay cache, not a retention-
    // policy data set. Never preserve an expired row until an older cutoff.
    let agent_request_uses =
        sqlx::query("DELETE FROM paper_raid_bff_agent_request_uses WHERE expires_at <= now()")
            .execute(&mut *tx)
            .await?
            .rows_affected();
    let revoked_credentials = sqlx::query(
        "DELETE FROM paper_raid_bff_login_credentials \
         WHERE state = 'revoked' AND revoked_at < $1",
    )
    .bind(cutoff)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let terminal_invites = sqlx::query(
        "DELETE FROM paper_raid_bff_invites \
         WHERE (state = 'revoked' AND revoked_at < $1) \
            OR (state = 'redeemed' AND redeemed_at < $1)",
    )
    .bind(cutoff)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let deleted_counts = json!({
        "sessions":sessions,
        "assertions":assertions,
        "idempotency":idempotency,
        "read_cache":read_cache,
        "product_events":product_events,
        "access_audit":0,
        "quota_windows":quotas,
        "agent_request_uses":agent_request_uses,
        "revoked_credentials":revoked_credentials,
        "terminal_invites":terminal_invites
    });
    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO paper_raid_bff_retention_runs \
         (run_id, policy_id, cutoff, operator_subject, deleted_counts) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(run_id)
    .bind(policy_id)
    .bind(cutoff)
    .bind(operator)
    .bind(deleted_counts.clone())
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        operator,
        None,
        "retention_prune",
        json!({"run_id":run_id,"policy_id":policy_id,"cutoff":cutoff}),
    )
    .await?;
    tx.commit().await?;
    Ok(
        json!({"schema":"paper-raid-bff.retention-run.v1","run_id":run_id,"policy_id":policy_id,"cutoff":cutoff,"deleted_counts":deleted_counts}),
    )
}

async fn revoke_sessions(tx: &mut Transaction<'_, Postgres>, subject: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO paper_raid_bff_session_generation(subject_id, generation) VALUES ($1, 1) \
         ON CONFLICT (subject_id) DO UPDATE SET generation = paper_raid_bff_session_generation.generation + 1, updated_at = now()",
    )
    .bind(subject)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE paper_raid_bff_sessions SET revoked_at = now() \
         WHERE subject_id = $1 AND revoked_at IS NULL",
    )
    .bind(subject)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    operator: &str,
    account_id: Option<Uuid>,
    action: &str,
    metadata: Value,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO paper_raid_bff_access_audit \
         (audit_id, operator_subject, account_id, action, outcome, metadata) \
         VALUES ($1, $2, $3, $4, 'succeeded', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(operator)
    .bind(account_id)
    .bind(action)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn random_secret() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn parse_expiry(value: &str) -> Result<Option<DateTime<Utc>>> {
    if value == "never" {
        return Ok(None);
    }
    Ok(Some(parse_timestamp(value, "expires-at")?))
}

fn parse_timestamp(value: &str, name: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| anyhow!("{name} must be RFC3339"))
}

fn parse_uuid(value: &str, name: &str) -> Result<Uuid> {
    Uuid::parse_str(value).map_err(|_| anyhow!("{name} must be a UUID"))
}

fn parse_set(value: &str, allowed: &[&str], empty_allowed: bool) -> Result<Vec<String>> {
    if value == "none" && empty_allowed {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    for item in value.split(',') {
        if !allowed.contains(&item) || values.iter().any(|existing| existing == item) {
            bail!("invalid or duplicate set value: {item}");
        }
        values.push(item.to_string());
    }
    if values.is_empty() {
        bail!("set must not be empty");
    }
    Ok(values)
}

fn validate_identifier(name: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
        })
    {
        bail!("{name} must be an opaque ASCII identifier");
    }
    Ok(())
}

fn validate_display_name(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 80 || value.as_bytes().contains(&0) {
        bail!("display-name must contain 1..80 characters");
    }
    Ok(())
}

fn validate_label(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 80 || value.as_bytes().contains(&0) {
        bail!("label must contain 1..80 characters");
    }
    Ok(())
}

fn required_env(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("{name} is required"))
}

type Args = ParsedArgs;

struct ParsedArgs(BTreeMap<String, String>);

impl ParsedArgs {
    fn required(&self, name: &str) -> Result<&str> {
        self.0
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| anyhow!("--{name} is required"))
    }

    fn only(&self, allowed: &[&str]) -> Result<()> {
        if let Some(name) = self.0.keys().find(|name| !allowed.contains(&name.as_str())) {
            bail!("unknown option --{name}");
        }
        Ok(())
    }
}

fn parse_args() -> Result<(String, ParsedArgs)> {
    let mut raw = env::args().skip(1);
    let command = raw.next().ok_or_else(|| anyhow!("command is required"))?;
    let mut parsed = BTreeMap::new();
    while let Some(name) = raw.next() {
        let Some(name) = name.strip_prefix("--") else {
            bail!("arguments must be --name value pairs");
        };
        let value = raw
            .next()
            .ok_or_else(|| anyhow!("--{name} requires a value"))?;
        if value.starts_with("--") || parsed.insert(name.to_string(), value).is_some() {
            bail!("invalid or duplicate --{name}");
        }
    }
    Ok((command, ParsedArgs(parsed)))
}
