use anyhow::{anyhow, bail, Context, Result};
use reqwest::{redirect::Policy, Client, Response, Url};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use shared_tracing::init_tracing;
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Transaction};
use std::{collections::BTreeMap, env};
use tokio::time::{sleep, timeout, Duration};
use tracing::{error, info, warn};
use uuid::Uuid;

const RELAY_DESTINATION: &str = "matrix-relay-adapter-v1";
const MAX_DATABASE_CONNECTIONS: u32 = 4;

#[derive(Debug, Clone)]
struct PollerConfig {
    homeserver_base_url: String,
    matrix_access_token: String,
    bot_user_id: String,
    database_url: String,
    partition_id: String,
    worker_id: String,
    cursor_lease_seconds: i32,
    poll_interval_ms: u64,
    sync_timeout_ms: u64,
    sync_filter: Option<String>,
    sync_max_bytes: usize,
    delivery_max_attempts: i32,
    production_like: bool,
}

#[derive(Debug)]
struct CursorLease {
    opaque_cursor: Option<String>,
    cursor_revision: i64,
    lease_fence: i64,
}

#[derive(Debug, Deserialize)]
struct SyncResponse {
    next_batch: String,
    #[serde(default)]
    rooms: RoomsState,
}

#[derive(Debug, Deserialize, Default)]
struct RoomsState {
    #[serde(default)]
    join: BTreeMap<String, JoinRoomState>,
}

#[derive(Debug, Deserialize)]
struct JoinRoomState {
    #[serde(default)]
    timeline: TimelineState,
}

#[derive(Debug, Deserialize, Default)]
struct TimelineState {
    #[serde(default)]
    events: Vec<Value>,
}

#[derive(Debug)]
enum Admission {
    Delivery {
        source_event_id: String,
        source_event_sha256: String,
        delivery_id: Uuid,
        payload_sha256: String,
        payload: Value,
    },
    Poison {
        source_event_id: String,
        source_event_sha256: String,
        failure_code: &'static str,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let config = PollerConfig::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(MAX_DATABASE_CONNECTIONS)
        .connect(&config.database_url)
        .await
        .context("failed to connect to Matrix transport PostgreSQL")?;
    verify_schema(&pool).await?;

    let http_timeout_ms = config
        .sync_timeout_ms
        .saturating_add(10_000)
        .min(
            (config.cursor_lease_seconds as u64)
                .saturating_mul(1_000)
                .saturating_sub(5_000),
        );
    let http = Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_millis(http_timeout_ms))
        .build()
        .context("failed to build Matrix HTTP client")?;

    info!(
        homeserver = %config.homeserver_base_url,
        partition = %config.partition_id,
        worker = %config.worker_id,
        lease_seconds = config.cursor_lease_seconds,
        sync_timeout_ms = config.sync_timeout_ms,
        sync_max_bytes = config.sync_max_bytes,
        production_like = config.production_like,
        "starting durable matrix-bot-poller"
    );

    run(config, pool, http).await
}

async fn run(config: PollerConfig, pool: PgPool, http: Client) -> Result<()> {
    loop {
        match acquire_cursor_lease(&pool, &config).await {
            Ok(Some(lease)) => {
                if let Err(err) = poll_once(&pool, &http, &config, lease).await {
                    warn!(error = %err, "Matrix poll iteration failed; durable cursor was not advanced unless admission committed");
                }
            }
            Ok(None) => {
                info!(partition = %config.partition_id, "Matrix cursor lease is owned by another worker");
            }
            Err(err) => {
                error!(error = %err, "failed to acquire Matrix cursor lease");
            }
        }

        sleep(Duration::from_millis(config.poll_interval_ms)).await;
    }
}

async fn poll_once(
    pool: &PgPool,
    http: &Client,
    config: &PollerConfig,
    lease: CursorLease,
) -> Result<()> {
    let sync_url = build_sync_url(config, lease.opaque_cursor.as_deref())?;
    let request = http
        .get(sync_url)
        .bearer_auth(&config.matrix_access_token);
    let response = timeout(
        Duration::from_millis(config.sync_timeout_ms.saturating_add(10_000)),
        request.send(),
    )
    .await
    .context("Matrix sync request timed out")??;

    if !response.status().is_success() {
        bail!("Matrix sync returned status {}", response.status());
    }

    let bytes = read_bounded_body(response, config.sync_max_bytes).await?;
    let body: SyncResponse = serde_json::from_slice(&bytes)
        .context("Matrix sync response was not valid bounded JSON")?;
    if body.next_batch.is_empty() || body.next_batch.len() > 8_192 {
        bail!("Matrix sync next_batch is absent or exceeds the durable cursor bound");
    }

    let admissions = prepare_admissions(&body, config)?;
    persist_batch(pool, config, &lease, &body.next_batch, admissions).await?;

    info!(
        partition = %config.partition_id,
        cursor_revision = lease.cursor_revision + 1,
        "durably admitted Matrix sync batch"
    );
    Ok(())
}

fn build_sync_url(config: &PollerConfig, cursor: Option<&str>) -> Result<Url> {
    let mut url = Url::parse(&format!(
        "{}/_matrix/client/v3/sync",
        config.homeserver_base_url.trim_end_matches('/')
    ))
    .context("invalid MATRIX_POLL_HOMESERVER URL")?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("timeout", &config.sync_timeout_ms.to_string());
        query.append_pair("set_presence", "offline");
        if let Some(cursor) = cursor {
            query.append_pair("since", cursor);
        }
        if let Some(filter) = config.sync_filter.as_deref() {
            query.append_pair("filter", filter);
        }
    }
    Ok(url)
}

fn prepare_admissions(body: &SyncResponse, config: &PollerConfig) -> Result<Vec<Admission>> {
    let mut admissions = Vec::new();

    for (room_id, room) in &body.rooms.join {
        for raw_event in &room.timeline.events {
            let event_type = raw_event.get("type").and_then(Value::as_str);
            if event_type != Some("m.room.message") {
                continue;
            }

            let raw_bytes = serde_json::to_vec(raw_event)
                .context("failed to serialize Matrix source event")?;
            let raw_hash = sha256_prefixed(&raw_bytes);
            let raw_event_id = raw_event.get("event_id").and_then(Value::as_str);
            let poison_source_id = raw_event_id
                .filter(|value| !value.is_empty() && value.len() <= 512)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("matrix-poison:{}", &raw_hash[7..]));

            let Some(event_id) = raw_event_id else {
                admissions.push(Admission::Poison {
                    source_event_id: poison_source_id,
                    source_event_sha256: raw_hash,
                    failure_code: "missing_event_id",
                });
                continue;
            };
            if event_id.is_empty() || event_id.len() > 512 {
                admissions.push(Admission::Poison {
                    source_event_id: poison_source_id,
                    source_event_sha256: raw_hash,
                    failure_code: "invalid_event_id",
                });
                continue;
            }

            let Some(sender) = raw_event.get("sender").and_then(Value::as_str) else {
                admissions.push(Admission::Poison {
                    source_event_id: event_id.to_string(),
                    source_event_sha256: raw_hash,
                    failure_code: "missing_sender",
                });
                continue;
            };
            if sender == config.bot_user_id {
                continue;
            }
            if sender.is_empty()
                || sender.len() > 512
                || room_id.is_empty()
                || room_id.len() > 512
            {
                admissions.push(Admission::Poison {
                    source_event_id: event_id.to_string(),
                    source_event_sha256: raw_hash,
                    failure_code: "invalid_matrix_identity",
                });
                continue;
            }

            let content = raw_event
                .get("content")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if content.get("msgtype").and_then(Value::as_str) != Some("m.text") {
                admissions.push(Admission::Poison {
                    source_event_id: event_id.to_string(),
                    source_event_sha256: raw_hash,
                    failure_code: "unsupported_message_type",
                });
                continue;
            }
            let Some(text) = content.get("body").and_then(Value::as_str) else {
                admissions.push(Admission::Poison {
                    source_event_id: event_id.to_string(),
                    source_event_sha256: raw_hash,
                    failure_code: "missing_message_body",
                });
                continue;
            };
            if text.trim().is_empty() || text.len() > 65_536 {
                admissions.push(Admission::Poison {
                    source_event_id: event_id.to_string(),
                    source_event_sha256: raw_hash,
                    failure_code: "invalid_message_body",
                });
                continue;
            }

            let payload = json!({
                "event_id": event_id,
                "event_type": "m.room.message",
                "room_id": room_id,
                "sender": sender,
                "text": text,
                "content": content,
                "timestamp_ms": raw_event.get("origin_server_ts").and_then(Value::as_i64),
                "metadata": {
                    "source": "matrix-client-v3-sync",
                    "partition_id": config.partition_id.as_str(),
                }
            });
            let payload_bytes = serde_json::to_vec(&payload)
                .context("failed to serialize normalized Matrix delivery")?;
            if payload_bytes.len() > 1_048_576 {
                admissions.push(Admission::Poison {
                    source_event_id: event_id.to_string(),
                    source_event_sha256: raw_hash,
                    failure_code: "normalized_payload_too_large",
                });
                continue;
            }
            let payload_sha256 = sha256_prefixed(&payload_bytes);
            let delivery_id = deterministic_uuid(
                "cex.matrix.poller.delivery.v1",
                &[event_id, RELAY_DESTINATION, &payload_sha256],
            );
            admissions.push(Admission::Delivery {
                source_event_id: event_id.to_string(),
                source_event_sha256: payload_sha256.clone(),
                delivery_id,
                payload_sha256,
                payload,
            });
        }
    }

    Ok(admissions)
}

async fn persist_batch(
    pool: &PgPool,
    config: &PollerConfig,
    lease: &CursorLease,
    next_cursor: &str,
    admissions: Vec<Admission>,
) -> Result<()> {
    let mut tx: Transaction<'_, Postgres> = pool.begin().await?;

    let locked: Option<(String, i64, i64)> = sqlx::query_as(
        "select lease_owner, lease_fence, cursor_revision \
         from public.matrix_transport_cursors \
         where partition_id = $1 and lease_expires_at > clock_timestamp() \
         for update",
    )
    .bind(&config.partition_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((owner, fence, revision)) = locked else {
        bail!("matrix_cursor_lease_or_revision_mismatch");
    };
    if owner != config.worker_id
        || fence != lease.lease_fence
        || revision != lease.cursor_revision
    {
        bail!("matrix_cursor_lease_or_revision_mismatch");
    }

    for admission in admissions {
        match admission {
            Admission::Delivery {
                source_event_id,
                source_event_sha256,
                delivery_id,
                payload_sha256,
                payload,
            } => {
                let _: String = sqlx::query_scalar(
                    "select public.cex_matrix_accept_source_event_v1($1, $2, $3, $4)",
                )
                .bind(&source_event_id)
                .bind(&source_event_sha256)
                .bind(&config.partition_id)
                .bind(lease.opaque_cursor.as_deref())
                .fetch_one(&mut *tx)
                .await?;

                let _: String = sqlx::query_scalar(
                    "select public.cex_matrix_enqueue_delivery_v1($1, $2, $3, $4, $5, $6)",
                )
                .bind(delivery_id)
                .bind(&source_event_id)
                .bind(RELAY_DESTINATION)
                .bind(&payload_sha256)
                .bind(sqlx::types::Json(payload))
                .bind(config.delivery_max_attempts)
                .fetch_one(&mut *tx)
                .await?;
            }
            Admission::Poison {
                source_event_id,
                source_event_sha256,
                failure_code,
            } => {
                let _: i64 = sqlx::query_scalar(
                    "select public.cex_matrix_record_poison_event_v1($1, $2, $3, $4)",
                )
                .bind(&source_event_id)
                .bind(&source_event_sha256)
                .bind(&config.partition_id)
                .bind(failure_code)
                .fetch_one(&mut *tx)
                .await?;
            }
        }
    }

    let advanced: bool = sqlx::query_scalar(
        "select public.cex_matrix_advance_cursor_v1($1, $2, $3, $4, $5)",
    )
    .bind(&config.partition_id)
    .bind(&config.worker_id)
    .bind(lease.lease_fence)
    .bind(lease.cursor_revision)
    .bind(next_cursor)
    .fetch_one(&mut *tx)
    .await?;
    if !advanced {
        bail!("matrix_cursor_lease_or_revision_mismatch");
    }

    tx.commit().await?;
    Ok(())
}

async fn acquire_cursor_lease(
    pool: &PgPool,
    config: &PollerConfig,
) -> Result<Option<CursorLease>> {
    let row: Option<(Option<String>, i64, i64)> = sqlx::query_as(
        "select opaque_cursor, cursor_revision, lease_fence \
         from public.cex_matrix_acquire_cursor_lease_v1($1, $2, $3)",
    )
    .bind(&config.partition_id)
    .bind(&config.worker_id)
    .bind(config.cursor_lease_seconds)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(opaque_cursor, cursor_revision, lease_fence)| CursorLease {
            opaque_cursor,
            cursor_revision,
            lease_fence,
        },
    ))
}

async fn verify_schema(pool: &PgPool) -> Result<()> {
    let ready: bool = sqlx::query_scalar(
        "select to_regprocedure('public.cex_matrix_acquire_cursor_lease_v1(text,text,integer)') is not null \
             and to_regprocedure('public.cex_matrix_enqueue_delivery_v1(uuid,text,text,text,jsonb,integer)') is not null \
             and to_regprocedure('public.cex_matrix_advance_cursor_v1(text,text,bigint,bigint,text)') is not null",
    )
    .fetch_one(pool)
    .await?;
    if !ready {
        bail!("Matrix transport schema/functions are not installed");
    }
    Ok(())
}

async fn read_bounded_body(mut response: Response, max_bytes: usize) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        bail!("Matrix sync response exceeds MATRIX_POLL_SYNC_MAX_BYTES");
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            bail!("Matrix sync response exceeds MATRIX_POLL_SYNC_MAX_BYTES");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn sha256_prefixed(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn deterministic_uuid(domain: &str, parts: &[&str]) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    for part in parts {
        hasher.update([0]);
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

impl PollerConfig {
    fn from_env() -> Result<Self> {
        let production_like = is_production_like();
        let homeserver_base_url = env::var("MATRIX_POLL_HOMESERVER")
            .unwrap_or_else(|_| "http://127.0.0.1:8008".to_string());
        let matrix_access_token = required_env("MATRIX_ACCESS_TOKEN", None)?;
        let bot_user_id = required_env("MATRIX_BOT_USER_ID", None)?;
        let database_url =
            required_env("MATRIX_TRANSPORT_DATABASE_URL", Some("DATABASE_URL"))?;
        let partition_id = required_env("MATRIX_POLL_PARTITION_ID", None)?;
        let worker_id = required_env("MATRIX_POLL_WORKER_ID", None)?;
        let cursor_lease_seconds =
            parse_env("MATRIX_POLL_CURSOR_LEASE_SECONDS", 60_i32)?;
        let poll_interval_ms = parse_env("MATRIX_POLL_INTERVAL_MS", 3_000_u64)?;
        let sync_timeout_ms = parse_env("MATRIX_POLL_SYNC_TIMEOUT_MS", 30_000_u64)?;
        let sync_max_bytes = parse_env("MATRIX_POLL_SYNC_MAX_BYTES", 2_097_152_usize)?;
        let delivery_max_attempts =
            parse_env("MATRIX_POLL_DELIVERY_MAX_ATTEMPTS", 8_i32)?;
        let sync_filter = env::var("MATRIX_SYNC_FILTER")
            .ok()
            .filter(|value| !value.trim().is_empty() && value != "0");

        if !(5..=3_600).contains(&cursor_lease_seconds) {
            bail!("MATRIX_POLL_CURSOR_LEASE_SECONDS must be between 5 and 3600");
        }
        if sync_timeout_ms < 1_000
            || sync_timeout_ms.saturating_add(10_000)
                >= (cursor_lease_seconds as u64).saturating_mul(1_000)
        {
            bail!("Matrix cursor lease must exceed sync timeout by more than ten seconds");
        }
        if !(1..=4_194_304).contains(&sync_max_bytes) {
            bail!("MATRIX_POLL_SYNC_MAX_BYTES must be between 1 and 4194304");
        }
        if !(1..=100).contains(&delivery_max_attempts) {
            bail!("MATRIX_POLL_DELIVERY_MAX_ATTEMPTS must be between 1 and 100");
        }
        validate_identifier("MATRIX_POLL_PARTITION_ID", &partition_id, 256)?;
        validate_identifier("MATRIX_POLL_WORKER_ID", &worker_id, 256)?;
        validate_identifier("MATRIX_BOT_USER_ID", &bot_user_id, 512)?;

        let url = Url::parse(&homeserver_base_url)
            .context("MATRIX_POLL_HOMESERVER is not a valid URL")?;
        if production_like && url.scheme() != "https" {
            bail!("production-like MATRIX_POLL_HOMESERVER must use https");
        }

        Ok(Self {
            homeserver_base_url,
            matrix_access_token,
            bot_user_id,
            database_url,
            partition_id,
            worker_id,
            cursor_lease_seconds,
            poll_interval_ms: poll_interval_ms.max(100),
            sync_timeout_ms,
            sync_filter,
            sync_max_bytes,
            delivery_max_attempts,
            production_like,
        })
    }
}

fn required_env(primary: &str, fallback: Option<&str>) -> Result<String> {
    let value = env::var(primary)
        .ok()
        .or_else(|| fallback.and_then(|name| env::var(name).ok()))
        .unwrap_or_default();
    if value.trim().is_empty() {
        let alternate = fallback
            .map(|name| format!(" or {name}"))
            .unwrap_or_default();
        return Err(anyhow!("{primary}{alternate} is required"));
    }
    Ok(value)
}

fn parse_env<T>(name: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Ok(raw) => raw
            .parse::<T>()
            .map_err(|error| anyhow!("invalid {name}: {error}")),
        Err(_) => Ok(default),
    }
}

fn validate_identifier(name: &str, value: &str, max_bytes: usize) -> Result<()> {
    if value.trim().is_empty()
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
    {
        bail!("{name} is empty, too long, or contains control characters");
    }
    Ok(())
}

fn is_production_like() -> bool {
    env::var("CEX_RUNTIME_PROFILE")
        .or_else(|_| env::var("APP_ENV"))
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "production" | "prod" | "staging" | "stage" | "preprod"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_delivery_identity_is_stable_and_separated() {
        let first = deterministic_uuid(
            "cex.matrix.poller.delivery.v1",
            &["event-1", RELAY_DESTINATION, "sha256:abc"],
        );
        let replay = deterministic_uuid(
            "cex.matrix.poller.delivery.v1",
            &["event-1", RELAY_DESTINATION, "sha256:abc"],
        );
        let changed = deterministic_uuid(
            "cex.matrix.poller.delivery.v1",
            &["event-1", RELAY_DESTINATION, "sha256:def"],
        );
        assert_eq!(first, replay);
        assert_ne!(first, changed);
    }

    #[test]
    fn canonical_hash_changes_with_payload() {
        let first = sha256_prefixed(br#"{"event_id":"one"}"#);
        let second = sha256_prefixed(br#"{"event_id":"two"}"#);
        assert!(first.starts_with("sha256:"));
        assert_eq!(first.len(), 71);
        assert_ne!(first, second);
    }
}
