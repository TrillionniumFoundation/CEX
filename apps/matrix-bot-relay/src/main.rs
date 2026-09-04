mod runtime_profile;
mod response_contract;

use anyhow::{anyhow, bail, Context, Result};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{header, uri::PathAndQuery, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use reqwest::{
    redirect::Policy, Client, Response as ReqwestResponse, StatusCode as ReqwestStatus, Url,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use shared_tracing::init_tracing;
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Row, Transaction};
use std::{env, sync::Arc};
use tokio::time::{sleep, timeout, Duration};
use tracing::{error, info, warn};
use uuid::Uuid;

const ADAPTER_DESTINATION: &str = "matrix-relay-adapter-v1";
const MATRIX_DESTINATION: &str = "matrix-homeserver-v1";
const MAX_DATABASE_CONNECTIONS: u32 = 8;
const PROFILE_ENV_NAME: &str = "MATRIX_RELAY_RUNTIME_PROFILE";

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    http: Client,
    config: Arc<RelayConfig>,
}

#[derive(Clone)]
struct RelayConfig {
    bind_addr: String,
    database_url: String,
    worker_id: String,
    claim_lease_seconds: i32,
    http_timeout_seconds: u64,
    poll_interval_ms: u64,
    ingress_token: String,
    ingress_max_bytes: usize,
    adapter_base_url: String,
    adapter_token: String,
    matrix_homeserver_base_url: String,
    matrix_access_token: String,
    max_response_bytes: usize,
    delivery_max_attempts: i32,
    production_like: bool,
}

#[derive(Debug)]
struct ClaimedDelivery {
    delivery_id: Uuid,
    source_event_id: String,
    destination: String,
    payload_sha256: String,
    payload: Value,
    attempt_count: i32,
    max_attempts: i32,
    lease_fence: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct InboundMatrixEvent {
    event_id: Option<String>,
    room_id: String,
    sender: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MatrixReplyEnvelope {
    room_id: String,
    projected_reply: Value,
}

#[derive(Debug)]
enum RemoteOutcome {
    Success(Value),
    Retryable(&'static str),
    Permanent(&'static str),
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let config = RelayConfig::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(MAX_DATABASE_CONNECTIONS)
        .connect(&config.database_url)
        .await
        .context("failed to connect to Matrix transport PostgreSQL")?;
    verify_schema(&pool).await?;

    let http = Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(config.http_timeout_seconds))
        .build()
        .context("failed to build Matrix relay HTTP client")?;
    let state = AppState {
        pool,
        http,
        config: Arc::new(config),
    };

    let worker_state = state.clone();
    tokio::spawn(async move {
        run_delivery_worker(worker_state).await;
    });

    let app = Router::new()
        .route("/health", get(health))
        .route(
            "/v1/inbound/matrix-event",
            post(compatibility_ingress),
        )
        .layer(DefaultBodyLimit::max(state.config.ingress_max_bytes))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&state.config.bind_addr)
        .await
        .context("failed to bind matrix-bot-relay")?;
    info!(
        bind = %state.config.bind_addr,
        worker = %state.config.worker_id,
        lease_seconds = state.config.claim_lease_seconds,
        production_like = state.config.production_like,
        "durable matrix-bot-relay started"
    );
    axum::serve(listener, app)
        .await
        .context("matrix-bot-relay server failed")?;
    Ok(())
}

async fn health(State(state): State<AppState>) -> Response {
    let result: std::result::Result<(i64, i64), sqlx::Error> = sqlx::query_as(
        "select \
             count(*) filter (where status in ('pending', 'claimed'))::bigint, \
             count(*) filter (where status = 'dead_letter')::bigint \
         from public.matrix_transport_outbox",
    )
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok((active, dead_letter)) => (
            StatusCode::OK,
            Json(json!({
                "status": "ready",
                "service": "matrix-bot-relay",
                "active_deliveries": active,
                "dead_letter_deliveries": dead_letter,
                "production_authorization": "not_granted"
            })),
        )
            .into_response(),
        Err(_) => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "matrix_transport_database_unavailable",
        ),
    }
}

async fn compatibility_ingress(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let supplied_token = headers
        .get("x-relay-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !constant_time_eq(
        supplied_token.as_bytes(),
        state.config.ingress_token.as_bytes(),
    ) {
        return api_error(StatusCode::UNAUTHORIZED, "relay_authentication_failed");
    }

    let payload_bytes = match serde_json::to_vec(&payload) {
        Ok(bytes) if bytes.len() <= state.config.ingress_max_bytes => bytes,
        _ => return api_error(StatusCode::PAYLOAD_TOO_LARGE, "relay_payload_too_large"),
    };
    let payload_sha256 = sha256_prefixed(&payload_bytes);
    if let Some(supplied_hash) = headers
        .get("x-cex-payload-sha256")
        .and_then(|value| value.to_str().ok())
    {
        if supplied_hash != payload_sha256.as_str() {
            return api_error(StatusCode::CONFLICT, "relay_payload_hash_mismatch");
        }
    }

    let event: InboundMatrixEvent = match serde_json::from_value(payload.clone()) {
        Ok(event) => event,
        Err(_) => return api_error(StatusCode::BAD_REQUEST, "invalid_matrix_event"),
    };
    let Some(event_id) = event.event_id.as_deref() else {
        return api_error(StatusCode::BAD_REQUEST, "matrix_event_id_required");
    };
    if validate_identifier(event_id, 512).is_err()
        || validate_identifier(&event.room_id, 512).is_err()
        || validate_identifier(&event.sender, 512).is_err()
    {
        return api_error(StatusCode::BAD_REQUEST, "invalid_matrix_identity");
    }

    let delivery_id = match headers
        .get("x-cex-delivery-id")
        .and_then(|value| value.to_str().ok())
    {
        Some(raw) => match Uuid::parse_str(raw) {
            Ok(value) => value,
            Err(_) => return api_error(StatusCode::BAD_REQUEST, "invalid_delivery_id"),
        },
        None => deterministic_uuid(
            "cex.matrix.relay.ingress.v1",
            &[event_id, ADAPTER_DESTINATION, &payload_sha256],
        ),
    };

    match admit_compatibility_event(
        &state,
        event_id,
        delivery_id,
        &payload_sha256,
        payload,
    )
    .await
    {
        Ok((event_disposition, delivery_disposition)) => (
            StatusCode::ACCEPTED,
            Json(json!({
                "accepted": true,
                "source_event_id": event_id,
                "delivery_id": delivery_id,
                "payload_sha256": payload_sha256,
                "event_disposition": event_disposition,
                "delivery_disposition": delivery_disposition,
                "production_authorization": "not_granted"
            })),
        )
            .into_response(),
        Err(err) => {
            let _ = err;
            warn!("matrix_event_admission_failed; inspect protected database evidence");
            api_error(StatusCode::CONFLICT, "matrix_event_admission_failed")
        }
    }
}

async fn admit_compatibility_event(
    state: &AppState,
    event_id: &str,
    delivery_id: Uuid,
    payload_sha256: &str,
    payload: Value,
) -> Result<(String, String)> {
    let mut tx: Transaction<'_, Postgres> = state.pool.begin().await?;
    let event_disposition: String = sqlx::query_scalar(
        "select public.cex_matrix_accept_source_event_v1($1, $2, $3, $4)",
    )
    .bind(event_id)
    .bind(payload_sha256)
    .bind("matrix-relay-http-v1")
    .bind(Option::<&str>::None)
    .fetch_one(&mut *tx)
    .await?;
    let delivery_disposition: String = sqlx::query_scalar(
        "select public.cex_matrix_enqueue_delivery_v1($1, $2, $3, $4, $5, $6)",
    )
    .bind(delivery_id)
    .bind(event_id)
    .bind(ADAPTER_DESTINATION)
    .bind(payload_sha256)
    .bind(sqlx::types::Json(payload))
    .bind(state.config.delivery_max_attempts)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((event_disposition, delivery_disposition))
}

async fn run_delivery_worker(state: AppState) {
    loop {
        match claim_delivery(&state).await {
            Ok(Some(delivery)) => {
                info!(
                    delivery_id = %delivery.delivery_id,
                    destination = %delivery.destination,
                    attempt_count = delivery.attempt_count,
                    max_attempts = delivery.max_attempts,
                    lease_fence = delivery.lease_fence,
                    "claimed durable Matrix delivery"
                );
                let delivery_id = delivery.delivery_id;
                let lease_fence = delivery.lease_fence;
                if let Err(_err) = process_claim(&state, delivery).await {
                    error!(
                        error_code = "matrix_delivery_processing_failed",
                        delivery_id = %delivery_id,
                        lease_fence,
                        "Matrix delivery processing failed"
                    );
                    let _ = finish_delivery(
                        &state.pool,
                        &state.config.worker_id,
                        delivery_id,
                        lease_fence,
                        "permanent_failure",
                        Some("relay_internal_unknown_outcome"),
                    )
                    .await;
                }
            }
            Ok(None) => {}
            Err(_err) => {
                error!("matrix_delivery_claim_failed");
            }
        }
        sleep(Duration::from_millis(state.config.poll_interval_ms)).await;
    }
}

async fn claim_delivery(state: &AppState) -> Result<Option<ClaimedDelivery>> {
    let row = sqlx::query(
        "select delivery_id, source_event_id, destination, payload_sha256, payload, \
                attempt_count, max_attempts, lease_fence \
         from public.cex_matrix_claim_delivery_v1($1, $2, 1)",
    )
    .bind(&state.config.worker_id)
    .bind(state.config.claim_lease_seconds)
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let payload: sqlx::types::Json<Value> = row.try_get("payload")?;
    Ok(Some(ClaimedDelivery {
        delivery_id: row.try_get("delivery_id")?,
        source_event_id: row.try_get("source_event_id")?,
        destination: row.try_get("destination")?,
        payload_sha256: row.try_get("payload_sha256")?,
        payload: payload.0,
        attempt_count: row.try_get("attempt_count")?,
        max_attempts: row.try_get("max_attempts")?,
        lease_fence: row.try_get("lease_fence")?,
    }))
}

async fn process_claim(state: &AppState, delivery: ClaimedDelivery) -> Result<()> {
    let canonical = serde_json::to_vec(&delivery.payload)?;
    if sha256_prefixed(&canonical) != delivery.payload_sha256 {
        poison_and_finish(state, &delivery, "delivery_payload_hash_mismatch").await?;
        return Ok(());
    }

    match delivery.destination.as_str() {
        ADAPTER_DESTINATION => process_adapter_delivery(state, &delivery).await,
        MATRIX_DESTINATION => process_matrix_delivery(state, &delivery).await,
        _ => {
            poison_and_finish(state, &delivery, "unknown_delivery_destination").await?;
            Ok(())
        }
    }
}

async fn process_adapter_delivery(state: &AppState, delivery: &ClaimedDelivery) -> Result<()> {
    let event: InboundMatrixEvent = match serde_json::from_value(delivery.payload.clone()) {
        Ok(event) => event,
        Err(_) => {
            poison_and_finish(state, delivery, "invalid_adapter_delivery_payload").await?;
            return Ok(());
        }
    };
    if event.event_id.as_deref() != Some(delivery.source_event_id.as_str()) {
        poison_and_finish(state, delivery, "adapter_source_identity_mismatch").await?;
        return Ok(());
    }

    match call_adapter(state, delivery).await? {
        RemoteOutcome::Success(upstream) => {
            complete_adapter_success(state, delivery, &event, upstream).await
        }
        RemoteOutcome::Retryable(code) => {
            finish_delivery(
                &state.pool,
                &state.config.worker_id,
                delivery.delivery_id,
                delivery.lease_fence,
                "retryable_failure",
                Some(code),
            )
            .await?;
            Ok(())
        }
        RemoteOutcome::Permanent(code) => {
            finish_delivery(
                &state.pool,
                &state.config.worker_id,
                delivery.delivery_id,
                delivery.lease_fence,
                "permanent_failure",
                Some(code),
            )
            .await?;
            Ok(())
        }
    }
}

async fn call_adapter(state: &AppState, delivery: &ClaimedDelivery) -> Result<RemoteOutcome> {
    let request = state
        .http
        .post(format!(
            "{}/v1/matrix/events",
            state.config.adapter_base_url.trim_end_matches('/')
        ))
        .header("x-entry-token", &state.config.adapter_token)
        .header("x-cex-delivery-id", delivery.delivery_id.to_string())
        .header("x-cex-payload-sha256", &delivery.payload_sha256)
        .header("x-idempotency-key", delivery.delivery_id.to_string())
        .header("idempotency-key", delivery.delivery_id.to_string())
        .json(&delivery.payload);

    let response = match timeout(
        Duration::from_secs(state.config.http_timeout_seconds),
        request.send(),
    )
    .await
    {
        Err(_) => return Ok(RemoteOutcome::Permanent("adapter_response_unknown_timeout")),
        Ok(Err(_)) => return Ok(RemoteOutcome::Permanent("adapter_response_unknown_network")),
        Ok(Ok(response)) => response,
    };
    let status = response.status();
    let body = match read_bounded_body(response, state.config.max_response_bytes).await {
        Ok(body) => body,
        Err(response_contract::BodyFailure::TooLarge) => {
            return Ok(RemoteOutcome::Permanent("adapter_unverified_oversized_response"));
        }
        Err(response_contract::BodyFailure::Interrupted) => {
            return Ok(RemoteOutcome::Permanent("adapter_response_unknown_interrupted"));
        }
    };

    if status.is_success() {
        return match serde_json::from_slice::<Value>(&body) {
            Ok(value) if value.is_object() => Ok(RemoteOutcome::Success(value)),
            _ => Ok(RemoteOutcome::Permanent("adapter_response_unknown_invalid_json")),
        };
    }
    if is_retryable_status(status) {
        Ok(RemoteOutcome::Permanent("adapter_response_unknown_status"))
    } else {
        Ok(RemoteOutcome::Permanent("adapter_permanent_status"))
    }
}

async fn complete_adapter_success(
    state: &AppState,
    delivery: &ClaimedDelivery,
    event: &InboundMatrixEvent,
    upstream: Value,
) -> Result<()> {
    let projected_reply = match response_contract::bound_reply(&upstream, &event.room_id) {
        Ok(reply) => reply,
        Err(code) => {
            finish_delivery(&state.pool, &state.config.worker_id, delivery.delivery_id,
                delivery.lease_fence, "permanent_failure", Some(code)).await?;
            return Ok(());
        }
    };
    let mut tx: Transaction<'_, Postgres> = state.pool.begin().await?;

    if let Some(projected_reply) = projected_reply {
        let room_id = event.room_id.as_str();
        validate_identifier(room_id, 512)?;
        let envelope = MatrixReplyEnvelope {
            room_id: room_id.to_string(),
            projected_reply,
        };
        let payload = serde_json::to_value(envelope)?;
        let payload_bytes = serde_json::to_vec(&payload)?;
        if payload_bytes.len() > 1_048_576 {
            bail!("projected Matrix reply exceeds durable payload limit");
        }
        let payload_sha256 = sha256_prefixed(&payload_bytes);
        let reply_delivery_id = deterministic_uuid(
            "cex.matrix.relay.reply.v1",
            &[
                &delivery.source_event_id,
                MATRIX_DESTINATION,
                room_id,
                &payload_sha256,
            ],
        );
        let _: String = sqlx::query_scalar(
            "select public.cex_matrix_enqueue_delivery_v1($1, $2, $3, $4, $5, $6)",
        )
        .bind(reply_delivery_id)
        .bind(&delivery.source_event_id)
        .bind(MATRIX_DESTINATION)
        .bind(&payload_sha256)
        .bind(sqlx::types::Json(payload))
        .bind(state.config.delivery_max_attempts)
        .fetch_one(&mut *tx)
        .await?;
    }

    let _: String = sqlx::query_scalar(
        "select public.cex_matrix_finish_delivery_v1($1, $2, $3, 'sent', null)",
    )
    .bind(delivery.delivery_id)
    .bind(&state.config.worker_id)
    .bind(delivery.lease_fence)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn process_matrix_delivery(state: &AppState, delivery: &ClaimedDelivery) -> Result<()> {
    let envelope: MatrixReplyEnvelope = match serde_json::from_value(delivery.payload.clone()) {
        Ok(envelope) => envelope,
        Err(_) => {
            poison_and_finish(state, delivery, "invalid_matrix_reply_payload").await?;
            return Ok(());
        }
    };
    if validate_identifier(&envelope.room_id, 512).is_err() {
        poison_and_finish(state, delivery, "invalid_matrix_reply_room").await?;
        return Ok(());
    }

    let outcome = call_matrix_homeserver(state, delivery, &envelope).await?;
    match outcome {
        RemoteOutcome::Success(receipt) => {
            let event_id = receipt.get("event_id").and_then(Value::as_str)
                .ok_or_else(|| anyhow!("matrix_receipt_contract_mismatch"))?;
            let mut tx = state.pool.begin().await?;
            let _: String = sqlx::query_scalar(
                "select public.cex_matrix_record_send_receipt_v1($1,$2,$3,$4,$5,$6)",
            )
            .bind(delivery.delivery_id)
            .bind(&state.config.worker_id)
            .bind(delivery.lease_fence)
            .bind(&delivery.payload_sha256)
            .bind(&envelope.room_id)
            .bind(event_id)
            .fetch_one(&mut *tx).await?;
            let _: String = sqlx::query_scalar(
                "select public.cex_matrix_finish_delivery_v1($1,$2,$3,'sent',null)",
            )
            .bind(delivery.delivery_id)
            .bind(&state.config.worker_id)
            .bind(delivery.lease_fence)
            .fetch_one(&mut *tx).await?;
            tx.commit().await?;
        }
        RemoteOutcome::Retryable(code) => {
            finish_delivery(
                &state.pool,
                &state.config.worker_id,
                delivery.delivery_id,
                delivery.lease_fence,
                "retryable_failure",
                Some(code),
            )
            .await?;
        }
        RemoteOutcome::Permanent(code) => {
            finish_delivery(
                &state.pool,
                &state.config.worker_id,
                delivery.delivery_id,
                delivery.lease_fence,
                "permanent_failure",
                Some(code),
            )
            .await?;
        }
    }
    Ok(())
}

async fn call_matrix_homeserver(
    state: &AppState,
    delivery: &ClaimedDelivery,
    envelope: &MatrixReplyEnvelope,
) -> Result<RemoteOutcome> {
    // Bind the credential scope before network I/O. A replacement access token
    // or homeserver must not silently turn a retry into a new Matrix operation.
    let credential_tag = sha256_prefixed(state.config.matrix_access_token.as_bytes());
    let binding: std::result::Result<String, sqlx::Error> = sqlx::query_scalar(
        "select public.cex_matrix_bind_send_attempt_v1($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(delivery.delivery_id).bind(&state.config.worker_id).bind(delivery.lease_fence)
    .bind(&delivery.payload_sha256).bind(&envelope.room_id)
    .bind(&state.config.matrix_homeserver_base_url).bind(&credential_tag)
    .fetch_one(&state.pool).await;
    if binding.is_err() {
        return Ok(RemoteOutcome::Permanent("matrix_send_binding_unverified"));
    }
    let send_url = matrix_send_url(
        &state.config.matrix_homeserver_base_url,
        &envelope.room_id,
        delivery.delivery_id,
    )?;
    let request = state
        .http
        .put(send_url)
        .header(
            header::AUTHORIZATION.as_str(),
            format!("Bearer {}", state.config.matrix_access_token),
        )
        .header("x-cex-delivery-id", delivery.delivery_id.to_string())
        .header("x-cex-payload-sha256", &delivery.payload_sha256)
        .header("x-idempotency-key", delivery.delivery_id.to_string())
        .json(&envelope.projected_reply);

    let response = match timeout(
        Duration::from_secs(state.config.http_timeout_seconds),
        request.send(),
    )
    .await
    {
        Err(_) => return Ok(RemoteOutcome::Retryable("matrix_response_unknown_timeout")),
        Ok(Err(_)) => return Ok(RemoteOutcome::Retryable("matrix_response_unknown_network")),
        Ok(Ok(response)) => response,
    };
    let status = response.status();
    let body = read_bounded_body(response, state.config.max_response_bytes).await;
    match response_contract::classify_matrix_response(status.as_u16(), body) {
        response_contract::MatrixDecision::Accepted(receipt) => Ok(RemoteOutcome::Success(receipt)),
        response_contract::MatrixDecision::Retry(code) => Ok(RemoteOutcome::Retryable(code)),
        response_contract::MatrixDecision::Hold(code) => Ok(RemoteOutcome::Permanent(code)),
    }
}

async fn finish_delivery(
    pool: &PgPool,
    owner: &str,
    delivery_id: Uuid,
    lease_fence: i64,
    outcome: &str,
    error_code: Option<&str>,
) -> Result<String> {
    let status: String = sqlx::query_scalar(
        "select public.cex_matrix_finish_delivery_v1($1, $2, $3, $4, $5)",
    )
    .bind(delivery_id)
    .bind(owner)
    .bind(lease_fence)
    .bind(outcome)
    .bind(error_code)
    .fetch_one(pool)
    .await?;
    Ok(status)
}

async fn poison_and_finish(
    state: &AppState,
    delivery: &ClaimedDelivery,
    failure_code: &'static str,
) -> Result<()> {
    let mut tx: Transaction<'_, Postgres> = state.pool.begin().await?;
    let source_hash: String = sqlx::query_scalar(
        "select source_event_sha256 from public.matrix_transport_inbox where source_event_id = $1",
    )
    .bind(&delivery.source_event_id)
    .fetch_one(&mut *tx)
    .await?;
    let partition_id: String = sqlx::query_scalar(
        "select partition_id from public.matrix_transport_inbox where source_event_id = $1",
    )
    .bind(&delivery.source_event_id)
    .fetch_one(&mut *tx)
    .await?;
    let _: i64 = sqlx::query_scalar(
        "select public.cex_matrix_record_poison_event_v1($1, $2, $3, $4)",
    )
    .bind(&delivery.source_event_id)
    .bind(&source_hash)
    .bind(&partition_id)
    .bind(failure_code)
    .fetch_one(&mut *tx)
    .await?;
    let _: String = sqlx::query_scalar(
        "select public.cex_matrix_finish_delivery_v1($1, $2, $3, 'permanent_failure', $4)",
    )
    .bind(delivery.delivery_id)
    .bind(&state.config.worker_id)
    .bind(delivery.lease_fence)
    .bind(failure_code)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn verify_schema(pool: &PgPool) -> Result<()> {
    let ready: bool = sqlx::query_scalar(
        "select to_regprocedure('public.cex_matrix_claim_delivery_v1(text,integer,integer)') is not null \
             and to_regprocedure('public.cex_matrix_enqueue_delivery_v1(uuid,text,text,text,jsonb,integer)') is not null \
             and to_regprocedure('public.cex_matrix_finish_delivery_v1(uuid,text,bigint,text,text)') is not null \
             and to_regprocedure('public.cex_matrix_record_send_receipt_v1(uuid,text,bigint,text,text,text)') is not null",
    )
    .fetch_one(pool)
    .await?;
    if !ready {
        bail!("Matrix transport schema/functions are not installed");
    }
    Ok(())
}

async fn read_bounded_body(
    mut response: ReqwestResponse,
    max_bytes: usize,
) -> std::result::Result<Vec<u8>, response_contract::BodyFailure> {
    if response.content_length().is_some_and(|length| length > max_bytes as u64) {
        return Err(response_contract::BodyFailure::TooLarge);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await
        .map_err(|_| response_contract::BodyFailure::Interrupted)? {
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err(response_contract::BodyFailure::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn matrix_send_url(base: &str, room_id: &str, delivery_id: Uuid) -> Result<Url> {
    let path = PathAndQuery::from_maybe_shared(format!(
        "/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
        encode_path_segment(room_id),
        delivery_id
    ))
    .context("failed to construct Matrix send path")?;
    let mut url = Url::parse(base).context("invalid MATRIX_HOMESERVER_BASE_URL")?;
    url.set_path(path.path());
    url.set_query(path.query());
    Ok(url)
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write;
            let _ = write!(&mut encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn is_retryable_status(status: ReqwestStatus) -> bool {
    status.is_server_error()
        || status == ReqwestStatus::TOO_MANY_REQUESTS
        || status == ReqwestStatus::REQUEST_TIMEOUT
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

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        difference |= (left_byte ^ right_byte) as usize;
    }
    difference == 0
}

fn validate_identifier(value: &str, max_bytes: usize) -> Result<()> {
    if value.trim().is_empty()
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
    {
        bail!("identifier is empty, too long, or contains control characters");
    }
    Ok(())
}

fn api_error(status: StatusCode, code: &'static str) -> Response {
    (status, Json(json!({"error_code": code}))).into_response()
}

impl RelayConfig {
    fn from_env() -> Result<Self> {
        let production_like = is_production_like()?;
        let bind_addr = env::var("MATRIX_BOT_RELAY_BIND")
            .or_else(|_| env::var("MATRIX_BOT_RELAY_BIND_ADDR"))
            .unwrap_or_else(|_| "127.0.0.1:8092".to_string());
        let database_url =
            required_env("MATRIX_TRANSPORT_DATABASE_URL", Some("DATABASE_URL"))?;
        let worker_id = required_env("MATRIX_RELAY_WORKER_ID", None)?;
        let claim_lease_seconds = parse_env_with_alias(
            "MATRIX_RELAY_CLAIM_LEASE_SECONDS",
            "MATRIX_RELAY_LEASE_SECONDS",
            60_i32,
        )?;
        let http_timeout_seconds =
            parse_env("MATRIX_RELAY_HTTP_TIMEOUT_SECONDS", 20_u64)?;
        let poll_interval_ms = parse_env("MATRIX_RELAY_POLL_INTERVAL_MS", 500_u64)?.max(50);
        let ingress_token = required_env("MATRIX_RELAY_INGRESS_TOKEN", None)?;
        let ingress_max_bytes =
            parse_env("MATRIX_RELAY_INGRESS_MAX_BYTES", 1_048_576_usize)?;
        let adapter_base_url = env::var("MATRIX_ADAPTER_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8091".to_string());
        let adapter_token = required_env(
            "MATRIX_ENTRY_ADAPTER_TOKEN",
            Some("MATRIX_ENTRY_INGRESS_TOKEN"),
        )?;
        let matrix_homeserver_base_url = env::var("MATRIX_HOMESERVER_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8008".to_string());
        let matrix_access_token = required_env("MATRIX_ACCESS_TOKEN", None)?;
        let max_response_bytes =
            parse_env("MATRIX_RELAY_MAX_RESPONSE_BYTES", 1_048_576_usize)?;
        let delivery_max_attempts =
            parse_env("MATRIX_RELAY_DELIVERY_MAX_ATTEMPTS", 8_i32)?;

        if !(5..=3_600).contains(&claim_lease_seconds) {
            bail!("MATRIX_RELAY_CLAIM_LEASE_SECONDS must be between 5 and 3600");
        }
        if http_timeout_seconds == 0
            || http_timeout_seconds.saturating_add(5) >= claim_lease_seconds as u64
        {
            bail!("MATRIX_RELAY_HTTP_TIMEOUT_SECONDS must be at least five seconds shorter than the claim lease");
        }
        if !(1..=1_048_576).contains(&ingress_max_bytes)
            || !(1..=4_194_304).contains(&max_response_bytes)
        {
            bail!("Matrix relay body limits are outside allowed bounds");
        }
        if !(1..=100).contains(&delivery_max_attempts) {
            bail!("MATRIX_RELAY_DELIVERY_MAX_ATTEMPTS must be between 1 and 100");
        }
        validate_identifier(&worker_id, 256)?;

        let adapter_url = Url::parse(&adapter_base_url)
            .context("MATRIX_ADAPTER_BASE_URL is not a valid URL")?;
        let homeserver_url = Url::parse(&matrix_homeserver_base_url)
            .context("MATRIX_HOMESERVER_BASE_URL is not a valid URL")?;
        for url in [&adapter_url, &homeserver_url] {
            if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none()
                || !url.username().is_empty() || url.password().is_some()
                || url.query().is_some() || url.fragment().is_some()
            {
                bail!("invalid_matrix_relay_endpoint_authority");
            }
        }
        if production_like
            && (adapter_url.scheme() != "https" || homeserver_url.scheme() != "https")
        {
            bail!("production-like Matrix relay downstream URLs must use https");
        }
        if production_like {
            if ingress_token.len() < 32
                || adapter_token.len() < 32
                || matrix_access_token.len() < 32
            {
                bail!("production-like Matrix relay credentials must be at least 32 bytes");
            }
            if ingress_token == adapter_token
                || ingress_token == matrix_access_token
                || adapter_token == matrix_access_token
            {
                bail!("Matrix relay credentials must be pairwise distinct");
            }
        }

        Ok(Self {
            bind_addr,
            database_url,
            worker_id,
            claim_lease_seconds,
            http_timeout_seconds,
            poll_interval_ms,
            ingress_token,
            ingress_max_bytes,
            adapter_base_url,
            adapter_token,
            matrix_homeserver_base_url,
            matrix_access_token,
            max_response_bytes,
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

fn parse_env_with_alias<T>(name: &str, alias: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(name).or_else(|_| env::var(alias)) {
        Ok(raw) => raw
            .parse::<T>()
            .map_err(|error| anyhow!("invalid {name}/{alias}: {error}")),
        Err(_) => Ok(default),
    }
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
    fn deterministic_ids_are_stable_and_content_bound() {
        let first = deterministic_uuid(
            "cex.matrix.relay.reply.v1",
            &["event-1", MATRIX_DESTINATION, "!room:a", "sha256:a"],
        );
        let replay = deterministic_uuid(
            "cex.matrix.relay.reply.v1",
            &["event-1", MATRIX_DESTINATION, "!room:a", "sha256:a"],
        );
        let changed = deterministic_uuid(
            "cex.matrix.relay.reply.v1",
            &["event-1", MATRIX_DESTINATION, "!room:a", "sha256:b"],
        );
        assert_eq!(first, replay);
        assert_ne!(first, changed);
    }

    #[test]
    fn matrix_transaction_url_uses_stable_delivery_id() {
        let delivery_id = Uuid::parse_str("00000000-0000-5000-8000-000000000001").unwrap();
        let url = matrix_send_url(
            "https://matrix.example",
            "!room:example",
            delivery_id,
        )
        .unwrap();
        assert!(url.as_str().contains("%21room%3Aexample"));
        assert!(url.as_str().ends_with(&delivery_id.to_string()));
    }

    #[test]
    fn secret_comparison_rejects_length_and_content_changes() {
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"diff"));
        assert!(!constant_time_eq(b"same", b"same-longer"));
    }

    #[test]
    fn retry_classification_is_bounded() {
        assert!(is_retryable_status(ReqwestStatus::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(ReqwestStatus::BAD_GATEWAY));
        assert!(!is_retryable_status(ReqwestStatus::BAD_REQUEST));
    }
}
