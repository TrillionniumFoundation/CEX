mod filter_definition;
mod runtime_profile;
mod stream_scope;
mod sync_recovery;
mod wire_response;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::{redirect::Policy, Client, Response, Url};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use shared_tracing::init_tracing;
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Transaction};
use std::{collections::BTreeMap, env};
use tokio::time::{sleep, timeout, Duration, Instant};
use tracing::{error, info, warn};
use uuid::Uuid;

const RELAY_DESTINATION: &str = "matrix-relay-adapter-v1";
const MAX_DATABASE_CONNECTIONS: u32 = 4;
const PROFILE_ENV_NAME: &str = "MATRIX_POLL_RUNTIME_PROFILE";
const GAP_PAGE_LIMIT: usize = 100;
const GAP_PAGE_BUDGET: usize = 100;
const GAP_EVENT_BUDGET: usize = 10_000;
const GAP_BYTE_BUDGET: usize = 33_554_432;
const GAP_DEADLINE_SECONDS: u64 = 120;

#[derive(Clone)]
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
    sync_filter_definition_sha256: Option<String>,
    resolved_filter: Option<filter_definition::FilterDefinition>,
    sync_max_bytes: usize,
    delivery_max_attempts: i32,
    production_like: bool,
    bootstrap_start_now: bool,
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
    #[serde(default)]
    limited: bool,
    #[serde(default, deserialize_with = "wire_response::optional_string")]
    prev_batch: Option<String>,
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
        room_id: String,
        source_payload: Value,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let mut config = PollerConfig::from_env()?;
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
        partition = %config.partition_id,
        worker = %config.worker_id,
        lease_seconds = config.cursor_lease_seconds,
        sync_timeout_ms = config.sync_timeout_ms,
        sync_max_bytes = config.sync_max_bytes,
        production_like = config.production_like,
        "starting durable matrix-bot-poller"
    );

    verify_homeserver_account(&http, &config).await?;
    config.resolved_filter = filter_definition::resolve(
        &http, &config.homeserver_base_url, &config.bot_user_id,
        &config.matrix_access_token, config.sync_filter.as_deref(),
        config.sync_filter_definition_sha256.as_deref(),
    ).await?;
    run(config, pool, http).await
}

async fn verify_homeserver_account(http: &Client, config: &PollerConfig) -> Result<()> {
    // This bounded read verifies the token's account; no token is persisted.
    stream_scope::describe(&config.homeserver_base_url, &config.bot_user_id,
        config.sync_filter.as_deref()).map_err(|code| anyhow!(code))?;
    let url = stream_scope::whoami_url(&config.homeserver_base_url)
        .map_err(|code| anyhow!(code))?;
    timeout(Duration::from_secs(10), async {
        let response = http.get(url).bearer_auth(&config.matrix_access_token).send().await
            .map_err(|_| anyhow!("matrix_stream_account_transport_failure"))?;
        if response.status() != reqwest::StatusCode::OK {
            bail!("matrix_stream_account_http_rejected");
        }
        let bytes = read_bounded_body(response, 4096).await?;
        let body: Value = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow!("matrix_stream_account_invalid_json"))?;
        stream_scope::verify_account(&body, &config.bot_user_id).map_err(|code| anyhow!(code))
    }).await.map_err(|_| anyhow!("matrix_stream_account_timeout"))?
}

async fn bind_stream_scope(
    tx: &mut Transaction<'_, Postgres>,
    config: &PollerConfig,
    lease: &CursorLease,
) -> Result<()> {
    let scope = stream_scope::describe(&config.homeserver_base_url, &config.bot_user_id,
        config.sync_filter.as_deref()).map_err(|code| anyhow!(code))?;
    let disposition: String = sqlx::query_scalar(
        "select public.cex_matrix_bind_stream_scope_v1($1,$2,$3,$4,$5)",
    )
    .bind(&config.partition_id).bind(&config.worker_id)
    .bind(lease.lease_fence).bind(lease.cursor_revision).bind(sqlx::types::Json(scope))
    .fetch_one(&mut **tx).await.map_err(|error| {
        let code = error.as_database_error().map(|db| db.message());
        anyhow!(match code {
            Some("matrix_stream_scope_mismatch") => "matrix_stream_scope_mismatch",
            Some("matrix_stream_scope_legacy_review_required") => "matrix_stream_scope_legacy_review_required",
            Some("matrix_cursor_lease_or_revision_mismatch") => "matrix_cursor_lease_or_revision_mismatch",
            _ => "matrix_stream_binding_unverified",
        })
    })?;
    if !matches!(disposition.as_str(), "bound" | "replay") {
        bail!("matrix_stream_binding_unverified");
    }
    // Scope and definition pin are committed together before a cursor is sent.
    // The same checks repeat inside the ordinary admission transaction.
    filter_definition::effective_filter(config.sync_filter.as_deref(), config.resolved_filter.as_ref())
        .map_err(|code| anyhow!(code))?;
    if let Some(definition) = config.resolved_filter.as_ref() {
        let filter_id = config.sync_filter.as_deref()
            .ok_or_else(|| anyhow!("matrix_filter_definition_unresolved"))?;
        let result: String = sqlx::query_scalar(
            "select public.cex_matrix_bind_filter_definition_v1($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(&config.partition_id).bind(&config.worker_id)
        .bind(lease.lease_fence).bind(lease.cursor_revision).bind(filter_id)
        .bind(definition.bytes()).bind(definition.sha256())
        .fetch_one(&mut **tx).await.map_err(|error| {
            let message = error.as_database_error().map(|db| db.message());
            anyhow!(match message {
                Some("matrix_filter_definition_mismatch") => "matrix_filter_definition_mismatch",
                Some("matrix_filter_definition_legacy_review_required") => "matrix_filter_definition_legacy_review_required",
                Some("matrix_filter_definition_scope_mismatch") => "matrix_filter_definition_scope_mismatch",
                Some("matrix_cursor_lease_or_revision_mismatch") => "matrix_cursor_lease_or_revision_mismatch",
                _ => "matrix_filter_definition_binding_unverified",
            })
        })?;
        if !matches!(result.as_str(), "bound" | "replay") {
            bail!("matrix_filter_definition_binding_unverified");
        }
    }
    Ok(())
}

async fn run(config: PollerConfig, pool: PgPool, http: Client) -> Result<()> {
    loop {
        match acquire_cursor_lease(&pool, &config).await {
            Ok(Some(lease)) => {
                if let Err(error) = poll_once(&pool, &http, &config, lease).await {
                    // Only emit closed-set machine codes, never SQL diagnostics,
                    // endpoint strings, opaque cursors or message bodies.
                    let code = match error.to_string().as_str() {
                        "matrix_stream_scope_mismatch" => "matrix_stream_scope_mismatch",
                        "matrix_stream_scope_legacy_review_required" => "matrix_stream_scope_legacy_review_required",
                        "matrix_stream_binding_unverified" => "matrix_stream_binding_unverified",
                        "matrix_gap_filter_id_requires_resolution" => "matrix_gap_filter_id_requires_resolution",
                        "matrix_gap_room_excluded_by_filter" => "matrix_gap_room_excluded_by_filter",
                        "matrix_sync_filter_unsupported" => "matrix_sync_filter_unsupported",
                        "matrix_filter_definition_mismatch" => "matrix_filter_definition_mismatch",
                        "matrix_filter_definition_legacy_review_required" => "matrix_filter_definition_legacy_review_required",
                        "matrix_filter_definition_scope_mismatch" => "matrix_filter_definition_scope_mismatch",
                        "matrix_filter_definition_binding_unverified" => "matrix_filter_definition_binding_unverified",
                        _ => "matrix_sync_not_accepted",
                    };
                    warn!(error_code = code, "Matrix sync held at last committed cursor; inspect protected recovery evidence");
                }
            }
            Ok(None) => {
                info!(partition = %config.partition_id, "Matrix cursor lease is owned by another worker");
            }
            Err(_err) => {
                error!("matrix_cursor_lease_acquisition_failed");
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
    // Commit/verify the stream scope before any cursor-bearing HTTP request.
    let mut scope_tx = pool.begin().await?;
    bind_stream_scope(&mut scope_tx, config, &lease).await?;
    scope_tx.commit().await?;
    let sync_url = build_sync_url(config, lease.opaque_cursor.as_deref())?;
    let request = http
        .get(sync_url)
        .bearer_auth(&config.matrix_access_token);
    let response = timeout(
        Duration::from_millis(config.sync_timeout_ms.saturating_add(10_000)),
        request.send(),
    )
    .await
    .map_err(|_| anyhow!("matrix_sync_timeout"))?
    .map_err(|_| anyhow!("matrix_sync_transport_failure"))?;

    if response.status() != reqwest::StatusCode::OK {
        bail!("matrix_sync_http_rejected");
    }

    let bytes = read_bounded_body(response, config.sync_max_bytes).await?;
    let mut body: SyncResponse = wire_response::decode(&bytes)
        .map_err(|code| anyhow!(code))?;
    sync_recovery::validate_token(&body.next_batch)?;

    if lease.opaque_cursor.is_none() {
        if !config.bootstrap_start_now {
            bail!("matrix_initial_cursor_required");
        }
        // Explicitly establish the first observation boundary. Historical
        // messages are never interpreted as fresh commands on first startup.
        persist_batch(pool, config, &lease, &body.next_batch, Vec::new()).await?;
        info!("Matrix initial start-now boundary committed; no historical commands admitted");
        return Ok(());
    }
    timeout(Duration::from_secs(GAP_DEADLINE_SECONDS),
        recover_limited_timelines(pool, http, config, &lease, &mut body))
        .await.map_err(|_| anyhow!("matrix_gap_recovery_deadline"))??;
    let admissions = prepare_admissions(&body, config)?;
    persist_poison_observations(pool, config, &lease, &admissions).await?;
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
        if let Some(filter) = filter_definition::effective_filter(
            config.sync_filter.as_deref(), config.resolved_filter.as_ref(),
        ).map_err(|code| anyhow!(code))? {
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
            if event_type.is_some_and(|kind| !kind.is_empty() && kind != "m.room.message") {
                continue;
            }

            let mut source_payload = raw_event.clone();
            if let Some(object) = source_payload.as_object_mut() {
                object.remove("unsigned");
                object.remove("room_id");
            }
            let raw_bytes = serde_json::to_vec(&source_payload)
                .context("failed to serialize Matrix source event")?;
            let raw_hash = sha256_prefixed(&raw_bytes);
            let raw_event_id = raw_event.get("event_id").and_then(Value::as_str);
            let poison_source_id = raw_event_id
                .filter(|value| validate_identifier("event_id", value, 512).is_ok())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("matrix-poison:{}", &raw_hash[7..]));

            // An absent, empty or non-string type is not a known ignored
            // event. Retain its bytes and hold the same cursor for quarantine.
            if event_type != Some("m.room.message") {
                admissions.push(Admission::Poison {
                    source_event_id: poison_source_id,
                    source_event_sha256: raw_hash,
                    failure_code: "invalid_event_type",
                    room_id: room_id.clone(),
                    source_payload: source_payload.clone(),
                });
                continue;
            }

            let Some(event_id) = raw_event_id else {
                admissions.push(Admission::Poison {
                    source_event_id: poison_source_id,
                    source_event_sha256: raw_hash,
                    failure_code: "missing_event_id",
                    room_id: room_id.clone(),
                    source_payload: source_payload.clone(),
                });
                continue;
            };
            if validate_identifier("event_id", event_id, 512).is_err() {
                admissions.push(Admission::Poison {
                    source_event_id: poison_source_id,
                    source_event_sha256: raw_hash,
                    failure_code: "invalid_event_id",
                    room_id: room_id.clone(),
                    source_payload: source_payload.clone(),
                });
                continue;
            }

            let Some(sender) = raw_event.get("sender").and_then(Value::as_str) else {
                admissions.push(Admission::Poison {
                    source_event_id: event_id.to_string(),
                    source_event_sha256: raw_hash,
                    failure_code: "missing_sender",
                    room_id: room_id.clone(),
                    source_payload: source_payload.clone(),
                });
                continue;
            };
            if sender == config.bot_user_id {
                continue;
            }
            if validate_identifier("sender", sender, 512).is_err()
                || validate_identifier("room_id", room_id, 512).is_err()
            {
                admissions.push(Admission::Poison {
                    source_event_id: event_id.to_string(),
                    source_event_sha256: raw_hash,
                    failure_code: "invalid_matrix_identity",
                    room_id: room_id.clone(),
                    source_payload: source_payload.clone(),
                });
                continue;
            }

            let content = raw_event
                .get("content")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if content.get("m.relates_to")
                .and_then(|relation| relation.get("rel_type"))
                .and_then(Value::as_str) == Some("m.replace")
            {
                // Edits cannot create a second privileged invocation.
                continue;
            }
            if content.get("msgtype").and_then(Value::as_str).is_some_and(|kind| {
                matches!(kind, "m.image" | "m.video" | "m.audio" | "m.file" | "m.location" | "m.notice" | "m.emote")
            }) {
                continue;
            }
            if content.get("msgtype").and_then(Value::as_str) != Some("m.text") {
                admissions.push(Admission::Poison {
                    source_event_id: event_id.to_string(),
                    source_event_sha256: raw_hash,
                    failure_code: "unsupported_message_type",
                    room_id: room_id.clone(),
                    source_payload: source_payload.clone(),
                });
                continue;
            }
            let Some(text) = content.get("body").and_then(Value::as_str) else {
                admissions.push(Admission::Poison {
                    source_event_id: event_id.to_string(),
                    source_event_sha256: raw_hash,
                    failure_code: "missing_message_body",
                    room_id: room_id.clone(),
                    source_payload: source_payload.clone(),
                });
                continue;
            };
            if text.trim().is_empty() || text.len() > 65_536 {
                admissions.push(Admission::Poison {
                    source_event_id: event_id.to_string(),
                    source_event_sha256: raw_hash,
                    failure_code: "invalid_message_body",
                    room_id: room_id.clone(),
                    source_payload: source_payload.clone(),
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
                    room_id: room_id.clone(),
                    source_payload: source_payload.clone(),
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
    bind_stream_scope(&mut tx, config, lease).await?;

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

    let blocked: bool = sqlx::query_scalar(
        "select exists (select 1 from public.matrix_transport_poison_events \
         where partition_id = $1 and acknowledged_at is null)",
    )
    .bind(&config.partition_id)
    .fetch_one(&mut *tx)
    .await?;
    if blocked {
        bail!("matrix_poison_requires_operator_quarantine");
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
            Admission::Poison { .. } => {
                // The observation transaction persisted the bytes. Only a
                // separate operator acknowledgement permits quarantine; this
                // does not declare the malformed message reprocessed.
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

async fn renew_cursor_lease(pool: &PgPool, config: &PollerConfig, lease: &CursorLease) -> Result<()> {
    let renewed: bool = sqlx::query_scalar(
        "select public.cex_matrix_renew_cursor_lease_v1($1,$2,$3,$4,$5)",
    )
    .bind(&config.partition_id).bind(&config.worker_id).bind(lease.lease_fence)
    .bind(lease.cursor_revision).bind(config.cursor_lease_seconds)
    .fetch_one(pool).await?;
    if !renewed {
        bail!("matrix_cursor_lease_or_revision_mismatch");
    }
    Ok(())
}

async fn recover_limited_timelines(
    pool: &PgPool,
    http: &Client,
    config: &PollerConfig,
    lease: &CursorLease,
    body: &mut SyncResponse,
) -> Result<()> {
    let stop = lease.opaque_cursor.as_deref().ok_or_else(|| anyhow!("matrix_initial_cursor_required"))?;
    let deadline = Instant::now() + Duration::from_secs(GAP_DEADLINE_SECONDS);
    let mut page_count = 0_usize;
    let mut event_count: usize = body.rooms.join.values().map(|room| room.timeline.events.len()).sum();
    let mut byte_count = 0_usize;
    if event_count > GAP_EVENT_BUDGET {
        bail!("matrix_sync_event_budget_exceeded");
    }
    for (room_id, room) in &mut body.rooms.join {
        if !room.timeline.limited {
            continue;
        }
        let from = room.timeline.prev_batch.as_deref().ok_or_else(|| anyhow!("matrix_gap_boundary_missing"))?;
        let mut pager = sync_recovery::GapPager::new(from, stop)?;
        let message_filter = if pager.complete() {
            None
        } else {
            let effective = filter_definition::effective_filter(
                config.sync_filter.as_deref(), config.resolved_filter.as_ref(),
            ).map_err(|code| anyhow!(code))?;
            stream_scope::backfill_filter(effective, room_id)
                .map_err(|code| anyhow!(code))?
        };
        let mut backwards = Vec::new();
        while !pager.complete() {
            if page_count >= GAP_PAGE_BUDGET || Instant::now() >= deadline {
                bail!("matrix_gap_recovery_budget_exceeded");
            }
            // Standalone statement commits before the HTTP request. Never
            // renew an expired/stolen fence or hold a SQL transaction over I/O.
            renew_cursor_lease(pool, config, lease).await?;
            let url = sync_recovery::messages_url(
                &config.homeserver_base_url, room_id, pager.cursor(), stop, GAP_PAGE_LIMIT,
                message_filter.as_deref(),
            )?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            let bytes = timeout(remaining, async {
                let response = http.get(url).bearer_auth(&config.matrix_access_token).send().await
                    .map_err(|_| anyhow!("matrix_gap_transport_failure"))?;
                if response.status() != reqwest::StatusCode::OK {
                    bail!("matrix_gap_http_rejected");
                }
                read_bounded_body(response, config.sync_max_bytes).await
            }).await.map_err(|_| anyhow!("matrix_gap_recovery_deadline"))??;
            byte_count = byte_count.checked_add(bytes.len()).ok_or_else(|| anyhow!("matrix_gap_byte_budget_exceeded"))?;
            if byte_count > GAP_BYTE_BUDGET {
                bail!("matrix_gap_byte_budget_exceeded");
            }
            let page: sync_recovery::MessagePage = wire_response::decode(&bytes)
                .map_err(|code| anyhow!(code))?;
            pager.accept(&page, GAP_PAGE_LIMIT)?;
            page_count += 1;
            event_count = event_count.checked_add(page.chunk.len()).ok_or_else(|| anyhow!("matrix_gap_event_budget_exceeded"))?;
            if event_count > GAP_EVENT_BUDGET {
                bail!("matrix_gap_event_budget_exceeded");
            }
            for event in &page.chunk {
                if event.get("room_id").is_some_and(|room| room.as_str() != Some(room_id.as_str())) {
                    bail!("matrix_gap_room_identity_mismatch");
                }
            }
            backwards.extend(page.chunk);
        }
        backwards.reverse();
        backwards.append(&mut room.timeline.events);
        room.timeline.events = backwards;
        room.timeline.limited = false;
    }
    renew_cursor_lease(pool, config, lease).await?;
    Ok(())
}

async fn persist_poison_observations(
    pool: &PgPool,
    config: &PollerConfig,
    lease: &CursorLease,
    admissions: &[Admission],
) -> Result<()> {
    if !admissions.iter().any(|item| matches!(item, Admission::Poison { .. })) {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    let owned: bool = sqlx::query_scalar(
        "select public.cex_matrix_renew_cursor_lease_v1($1,$2,$3,$4,$5)",
    )
    .bind(&config.partition_id).bind(&config.worker_id).bind(lease.lease_fence)
    .bind(lease.cursor_revision).bind(config.cursor_lease_seconds)
    .fetch_one(&mut *tx).await?;
    if !owned {
        bail!("matrix_cursor_lease_or_revision_mismatch");
    }
    for item in admissions {
        if let Admission::Poison { source_event_id, source_event_sha256, failure_code, room_id, source_payload } = item {
            let _: i64 = sqlx::query_scalar(
                "select public.cex_matrix_record_poison_event_v1($1,$2,$3,$4)",
            )
            .bind(source_event_id).bind(source_event_sha256).bind(&config.partition_id)
            .bind(*failure_code).fetch_one(&mut *tx).await?;
            let _: String = sqlx::query_scalar(
                "select public.cex_matrix_store_poison_payload_v1($1,$2,$3,$4,$5)",
            )
            .bind(source_event_id).bind(source_event_sha256).bind(&config.partition_id)
            .bind(room_id).bind(sqlx::types::Json(source_payload.clone()))
            .fetch_one(&mut *tx).await?;
        }
    }
    // Poison evidence survives even though the subsequent admission/cursor
    // transaction is rejected. It is never silently rolled back with that batch.
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
             and to_regprocedure('public.cex_matrix_advance_cursor_v1(text,text,bigint,bigint,text)') is not null \
             and to_regprocedure('public.cex_matrix_renew_cursor_lease_v1(text,text,bigint,bigint,integer)') is not null \
             and to_regclass('public.matrix_transport_poison_payloads') is not null \
             and to_regclass('public.matrix_transport_cursor_history') is not null \
             and to_regprocedure('public.cex_matrix_bind_stream_scope_v1(text,text,bigint,bigint,jsonb)') is not null \
             and to_regprocedure('public.cex_matrix_bind_filter_definition_v1(text,text,bigint,bigint,text,text,text)') is not null",
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
    while let Some(chunk) = response.chunk().await
        .map_err(|_| anyhow!("matrix_sync_body_interrupted"))? {
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
        let production_like = is_production_like()?;
        let bootstrap_start_now = match env::var("MATRIX_POLL_BOOTSTRAP_MODE") {
            Ok(value) if value == "start_now" => true,
            Ok(value) if value == "require_cursor" => false,
            Err(env::VarError::NotPresent) => false,
            _ => bail!("invalid_matrix_bootstrap_mode"),
        };
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
        let sync_filter = stream_scope::configured_filter(env::var("MATRIX_SYNC_FILTER"))
            .map_err(|code| anyhow!(code))?;
        stream_scope::describe(&homeserver_base_url, &bot_user_id, sync_filter.as_deref())
            .map_err(|code| anyhow!(code))?;
        let sync_filter_definition_sha256 = filter_definition::configured_digest(
            sync_filter.as_deref(), env::var("MATRIX_SYNC_FILTER_DEFINITION_SHA256"),
        ).map_err(|code| anyhow!(code))?;

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
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none()
            || !url.username().is_empty() || url.password().is_some()
            || url.query().is_some() || url.fragment().is_some()
        {
            bail!("invalid_matrix_homeserver_authority");
        }
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
            sync_filter_definition_sha256,
            resolved_filter: None,
            sync_max_bytes,
            delivery_max_attempts,
            production_like,
            bootstrap_start_now,
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

fn is_production_like() -> Result<bool> {
    let mut values = Vec::new();
    for name in [PROFILE_ENV_NAME, "CEX_RUNTIME_PROFILE", "APP_ENV"] {
        match env::var(name) {
            Ok(value) => values.push(Some(value)),
            Err(env::VarError::NotPresent) => values.push(None),
            Err(env::VarError::NotUnicode(_)) => bail!("non_unicode_matrix_runtime_profile"),
        }
    }
    let selected = runtime_profile::resolve_profiles(&values).map_err(|code| anyhow!(code))?;
    Ok(selected.legacy_value() != "local_dev")
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

    fn test_config() -> PollerConfig {
        PollerConfig {
            homeserver_base_url: "https://matrix.example".into(),
            matrix_access_token: "unit-test-not-a-secret".into(),
            bot_user_id: "@bot:example".into(),
            database_url: "postgres://unit/test".into(),
            partition_id: "test-partition".into(),
            worker_id: "test-worker".into(),
            cursor_lease_seconds: 60,
            poll_interval_ms: 100,
            sync_timeout_ms: 1000,
            sync_filter: None,
            sync_filter_definition_sha256: None,
            resolved_filter: None,
            sync_max_bytes: 4096,
            delivery_max_attempts: 3,
            production_like: false,
            bootstrap_start_now: false,
        }
    }

    fn test_batch(event: Value) -> SyncResponse {
        serde_json::from_value(json!({
            "next_batch": "next",
            "rooms": {"join": {"!room:example": {"timeline": {"events": [event]}}}}
        })).unwrap()
    }

    #[test]
    fn text_admission_remains_stable_across_unsigned_observations() {
        let mut event = json!({"type":"m.room.message", "event_id":"$one",
            "sender":"@human:example", "origin_server_ts":1,
            "content":{"msgtype":"m.text","body":"hello"}});
        let first = prepare_admissions(&test_batch(event.clone()), &test_config()).unwrap();
        event["unsigned"] = json!({"age": 999});
        let second = prepare_admissions(&test_batch(event), &test_config()).unwrap();
        match (&first[0], &second[0]) {
            (Admission::Delivery { delivery_id: a, payload_sha256: ah, .. },
             Admission::Delivery { delivery_id: b, payload_sha256: bh, .. }) => {
                assert_eq!(a, b);
                assert_eq!(ah, bh);
            }
            _ => panic!("valid text must be an immutable delivery"),
        }
    }

    #[test]
    fn malformed_text_retains_recoverable_poison_bytes() {
        let event = json!({"type":"m.room.message", "event_id":"$one",
            "sender":"@human:example", "unsigned":{"age":10},
            "content":{"msgtype":"m.text"}});
        let admissions = prepare_admissions(&test_batch(event), &test_config()).unwrap();
        match &admissions[0] {
            Admission::Poison { failure_code, source_payload, room_id, .. } => {
                assert_eq!(*failure_code, "missing_message_body");
                assert_eq!(room_id, "!room:example");
                assert!(source_payload.get("unsigned").is_none());
                assert_eq!(source_payload["event_id"], "$one");
            }
            _ => panic!("malformed text must not become a business command"),
        }
    }

    #[test]
    fn edits_are_not_new_commands() {
        let event = json!({"type":"m.room.message", "event_id":"$edit",
            "sender":"@human:example", "content":{"msgtype":"m.text","body":"replacement",
            "m.relates_to":{"rel_type":"m.replace","event_id":"$one"}}});
        assert!(prepare_admissions(&test_batch(event), &test_config()).unwrap().is_empty());
    }

    #[test]
    fn known_nontext_and_bot_echoes_are_ignored() {
        for (sender, kind) in [("@human:example", "m.image"), ("@bot:example", "m.text")] {
            let event = json!({"type":"m.room.message", "event_id":"$one",
                "sender":sender, "content":{"msgtype":kind,"body":"ignored"}});
            assert!(prepare_admissions(&test_batch(event), &test_config()).unwrap().is_empty());
        }
    }

    #[test]
    fn sync_url_uses_the_verified_definition_not_the_remote_id() {
        let mut config = test_config();
        config.sync_filter = Some("0".into());
        assert!(build_sync_url(&config, Some("cursor")).is_err());
        let bytes = br#"{"room":{"timeline":{"not_senders":["@excluded:example"]}}}"#;
        let digest = sha256_prefixed(bytes);
        config.resolved_filter = Some(filter_definition::verify_definition(bytes, &digest).unwrap());
        let url = build_sync_url(&config, Some("cursor")).unwrap();
        let actual = url.query_pairs().find(|(key, _)| key == "filter").unwrap().1;
        assert_eq!(actual, std::str::from_utf8(bytes).unwrap());
        assert_eq!(url.query_pairs().filter(|(key, _)| key == "filter").count(), 1);
        assert_eq!(url.query_pairs().find(|(key, _)| key == "since").unwrap().1, "cursor");
    }

    #[test]
    fn malformed_event_type_is_quarantined_not_silently_skipped() {
        for event in [
            Value::Null,
            json!([]),
            json!({"event_id":"$one","content":{"body":"command"}}),
            json!({"event_id":"$one","type":null}),
            json!({"event_id":"$one","type":42}),
            json!({"event_id":"$one","type":""}),
        ] {
            let admissions = prepare_admissions(&test_batch(event.clone()), &test_config()).unwrap();
            assert_eq!(admissions.len(), 1);
            match &admissions[0] {
                Admission::Poison { failure_code, source_payload, source_event_id, .. } => {
                    assert_eq!(*failure_code, "invalid_event_type");
                    assert_eq!(source_payload, &event);
                    assert!(!source_event_id.is_empty());
                }
                _ => panic!("malformed routing data must not become a command or disappear"),
            }
        }
    }

    #[test]
    fn well_formed_unsupported_event_types_still_do_not_become_commands() {
        for kind in ["m.room.member", "m.room.encrypted", "org.example.custom"] {
            let event = json!({"type":kind,"event_id":"$one","content":{}});
            assert!(prepare_admissions(&test_batch(event), &test_config()).unwrap().is_empty());
        }
    }

    #[test]
    fn ambiguous_sync_and_error_envelopes_are_not_empty_successes() {
        for bytes in [
            br#"{"next_batch":"new","errcode":null}"#.as_slice(),
            br#"{"next_batch":"new","rooms":{"join":{"!r:e":{},"!r:e":{}}}}"#.as_slice(),
            br#"{"next_batch":"new","rooms":{"join":{"!r:e":{"timeline":{"prev_batch":null}}}}}"#.as_slice(),
        ] {
            assert!(wire_response::decode::<SyncResponse>(bytes).is_err());
        }
        assert!(wire_response::decode::<SyncResponse>(br#"{"next_batch":"new"}"#).is_ok());
    }

    #[test]
    fn healthy_wire_decode_does_not_change_delivery_identity() {
        let event = json!({"type":"m.room.message","event_id":"$one",
            "sender":"@human:example","origin_server_ts":1,
            "content":{"msgtype":"m.text","body":"hello"}});
        let direct = prepare_admissions(&test_batch(event.clone()), &test_config()).unwrap();
        let wire = serde_json::to_vec(&json!({"next_batch":"next","rooms":{"join":{
            "!room:example":{"timeline":{"events":[event]}}
        }}})).unwrap();
        let decoded: SyncResponse = wire_response::decode(&wire).unwrap();
        let checked = prepare_admissions(&decoded, &test_config()).unwrap();
        match (&direct[0], &checked[0]) {
            (Admission::Delivery { delivery_id: a, payload_sha256: ah, .. },
             Admission::Delivery { delivery_id: b, payload_sha256: bh, .. }) => {
                assert_eq!(a, b);
                assert_eq!(ah, bh);
            }
            _ => panic!("healthy wire decoding must preserve normal delivery identity"),
        }
    }

}
