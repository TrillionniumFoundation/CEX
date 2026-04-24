use axum::{
    extract::State,
    http::StatusCode as HttpStatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use reqwest::{Client, StatusCode as ReqStatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shared_tracing::init_tracing;
use std::{
    collections::VecDeque,
    env, fs,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::Mutex,
    time::{sleep, Duration},
};
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Debug)]
struct RelayMetrics {
    inbound_events: AtomicU64,
    self_events: AtomicU64,
    duplicate_events: AtomicU64,
    projected_reply_missing: AtomicU64,
    adapter_requests: AtomicU64,
    adapter_failures: AtomicU64,
    matrix_send_attempts: AtomicU64,
    matrix_send_successes: AtomicU64,
    matrix_send_failures: AtomicU64,
    matrix_send_queue_enqueued: AtomicU64,
    matrix_send_queue_attempts: AtomicU64,
    matrix_send_queue_success: AtomicU64,
    matrix_send_queue_failures: AtomicU64,
    matrix_send_queue_requeues: AtomicU64,
    matrix_send_queue_drops: AtomicU64,
    queue_depth_peak: AtomicU64,
    queue_depth_observations: AtomicU64,
    queue_depth_sum: AtomicU64,
}

impl RelayMetrics {
    fn new() -> Self {
        Self {
            inbound_events: AtomicU64::new(0),
            self_events: AtomicU64::new(0),
            duplicate_events: AtomicU64::new(0),
            projected_reply_missing: AtomicU64::new(0),
            adapter_requests: AtomicU64::new(0),
            adapter_failures: AtomicU64::new(0),
            matrix_send_attempts: AtomicU64::new(0),
            matrix_send_successes: AtomicU64::new(0),
            matrix_send_failures: AtomicU64::new(0),
            matrix_send_queue_enqueued: AtomicU64::new(0),
            matrix_send_queue_attempts: AtomicU64::new(0),
            matrix_send_queue_success: AtomicU64::new(0),
            matrix_send_queue_failures: AtomicU64::new(0),
            matrix_send_queue_requeues: AtomicU64::new(0),
            matrix_send_queue_drops: AtomicU64::new(0),
            queue_depth_peak: AtomicU64::new(0),
            queue_depth_observations: AtomicU64::new(0),
            queue_depth_sum: AtomicU64::new(0),
        }
    }

    fn inc_inbound_events(&self) {
        self.inbound_events.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_self_events(&self) {
        self.self_events.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_duplicate_events(&self) {
        self.duplicate_events.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_projected_reply_missing(&self) {
        self.projected_reply_missing.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_adapter_request(&self) {
        self.adapter_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_adapter_failure(&self) {
        self.adapter_failures.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_matrix_send_attempt(&self) {
        self.matrix_send_attempts.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_matrix_send_success(&self) {
        self.matrix_send_successes.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_matrix_send_failure(&self) {
        self.matrix_send_failures.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_queue_enqueued(&self) {
        self.matrix_send_queue_enqueued
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_queue_attempt(&self) {
        self.matrix_send_queue_attempts
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_queue_success(&self) {
        self.matrix_send_queue_success
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_queue_failure(&self) {
        self.matrix_send_queue_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_queue_requeue(&self) {
        self.matrix_send_queue_requeues
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_queue_drop(&self) {
        self.matrix_send_queue_drops.fetch_add(1, Ordering::Relaxed);
    }

    fn record_queue_depth(&self, depth: usize) {
        let depth_u64 = depth as u64;
        self.queue_depth_observations
            .fetch_add(1, Ordering::Relaxed);
        self.queue_depth_sum.fetch_add(depth_u64, Ordering::Relaxed);

        loop {
            let current_peak = self.queue_depth_peak.load(Ordering::Acquire);
            if depth_u64 <= current_peak {
                return;
            }

            if self
                .queue_depth_peak
                .compare_exchange(current_peak, depth_u64, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
        }
    }

    fn snapshot(&self, queue_len: usize) -> Value {
        let inbound = self.inbound_events.load(Ordering::Relaxed);
        let duplicates = self.duplicate_events.load(Ordering::Relaxed);
        let duplicate_rate = if inbound > 0 {
            (duplicates as f64 / inbound as f64) * 100.0
        } else {
            0.0
        };

        let queue_observations = self.queue_depth_observations.load(Ordering::Relaxed);
        let queue_depth_sum = self.queue_depth_sum.load(Ordering::Relaxed);
        let avg_queue_depth = if queue_observations > 0 {
            queue_depth_sum as f64 / queue_observations as f64
        } else {
            0.0
        };

        let queue_attempts = self.matrix_send_queue_attempts.load(Ordering::Relaxed);
        let queue_success = self.matrix_send_queue_success.load(Ordering::Relaxed);
        let queue_requeues = self.matrix_send_queue_requeues.load(Ordering::Relaxed);
        let queue_failures = self.matrix_send_queue_failures.load(Ordering::Relaxed);
        let queue_success_rate = if queue_attempts > 0 {
            (queue_success as f64 / queue_attempts as f64) * 100.0
        } else {
            0.0
        };

        let direct_send_attempts = self
            .matrix_send_attempts
            .load(Ordering::Relaxed)
            .saturating_sub(queue_attempts);
        let direct_send_success = self
            .matrix_send_successes
            .load(Ordering::Relaxed)
            .saturating_sub(queue_success);
        let direct_send_rate = if direct_send_attempts > 0 {
            (direct_send_success as f64 / direct_send_attempts as f64) * 100.0
        } else {
            0.0
        };

        let total_send_attempts = self.matrix_send_attempts.load(Ordering::Relaxed);
        let total_send_success = self.matrix_send_successes.load(Ordering::Relaxed);
        let total_send_failure = self.matrix_send_failures.load(Ordering::Relaxed);
        let total_send_success_rate = if total_send_attempts > 0 {
            (total_send_success as f64 / total_send_attempts as f64) * 100.0
        } else {
            0.0
        };

        json!({
            "inbound_events_total": inbound,
            "self_events_total": self.self_events.load(Ordering::Relaxed),
            "duplicate_events_total": duplicates,
            "duplicate_event_rate_pct": duplicate_rate,
            "projected_reply_missing_total": self.projected_reply_missing.load(Ordering::Relaxed),
            "adapter_requests_total": self.adapter_requests.load(Ordering::Relaxed),
            "adapter_failures_total": self.adapter_failures.load(Ordering::Relaxed),
            "matrix_send_attempts_total": total_send_attempts,
            "matrix_send_successes_total": total_send_success,
            "matrix_send_failures_total": total_send_failure,
            "matrix_send_success_rate_pct": total_send_success_rate,
            "direct_send_attempts_total": direct_send_attempts,
            "direct_send_successes_total": direct_send_success,
            "direct_send_failures_total": total_send_failure.saturating_sub(queue_failures),
            "direct_send_success_rate_pct": direct_send_rate,
            "queue_send_attempts_total": queue_attempts,
            "queue_send_success_total": queue_success,
            "queue_send_failures_total": queue_failures,
            "queue_send_requeues_total": queue_requeues,
            "queue_send_drops_total": self.matrix_send_queue_drops.load(Ordering::Relaxed),
            "queue_send_success_rate_pct": queue_success_rate,
            "queue_len_current": queue_len,
            "queue_len_peak": self.queue_depth_peak.load(Ordering::Relaxed),
            "queue_len_avg": avg_queue_depth,
            "queue_depth_observations": queue_observations,
            "queue_depth_sum": queue_depth_sum,
            "enqueued_total": self.matrix_send_queue_enqueued.load(Ordering::Relaxed),
        })
    }
}

#[derive(Clone)]
struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    http: Client,
    config: RelayConfig,
    recent_event_ids: Mutex<VecDeque<String>>,
    send_queue: Mutex<VecDeque<QueuedMatrixSend>>,
    metrics: RelayMetrics,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RelayConfig {
    bind_addr: String,
    matrix_adapter_base_url: String,
    matrix_adapter_ingress_token: Option<String>,
    matrix_homeserver_base_url: String,
    matrix_access_token: String,
    matrix_bot_user_id: String,
    max_recent_event_ids: usize,
    matrix_send_max_attempts: usize,
    matrix_send_initial_delay_ms: u64,
    matrix_send_max_delay_ms: u64,
    matrix_send_queue_enabled: bool,
    matrix_send_queue_path: String,
    matrix_send_queue_poll_interval_ms: u64,
    matrix_send_queue_max_size: usize,
}

impl RelayConfig {
    fn from_env() -> Self {
        Self {
            bind_addr: env::var("MATRIX_BOT_RELAY_BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8092".to_string()),
            matrix_adapter_base_url: env::var("MATRIX_ADAPTER_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8091".to_string()),
            matrix_adapter_ingress_token: env::var("MATRIX_ENTRY_INGRESS_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            matrix_homeserver_base_url: env::var("MATRIX_HOMESERVER_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8008".to_string()),
            matrix_access_token: env::var("MATRIX_ACCESS_TOKEN").unwrap_or_default(),
            matrix_bot_user_id: env::var("MATRIX_BOT_USER_ID")
                .unwrap_or_else(|_| "@cex-bot:local.dev".to_string()),
            max_recent_event_ids: env::var("MATRIX_RELAY_MAX_RECENT_EVENT_IDS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1000),
            matrix_send_max_attempts: env::var("MATRIX_RELAY_SEND_MAX_ATTEMPTS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(3),
            matrix_send_initial_delay_ms: env::var("MATRIX_RELAY_SEND_INITIAL_DELAY_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(200),
            matrix_send_max_delay_ms: env::var("MATRIX_RELAY_SEND_MAX_DELAY_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(2500),
            matrix_send_queue_enabled: env::var("MATRIX_RELAY_QUEUE_ENABLED")
                .ok()
                .and_then(|v| v.parse::<bool>().ok())
                .unwrap_or(true),
            matrix_send_queue_path: env::var("MATRIX_RELAY_QUEUE_PATH")
                .unwrap_or_else(|_| "/tmp/matrix-bot-relay-queue.json".to_string()),
            matrix_send_queue_poll_interval_ms: env::var("MATRIX_RELAY_QUEUE_POLL_INTERVAL_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(4000),
            matrix_send_queue_max_size: env::var("MATRIX_RELAY_QUEUE_MAX_SIZE")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(2000),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct QueuedMatrixSend {
    queue_id: String,
    room_id: String,
    event_id: Option<String>,
    projected_reply: Value,
    attempts: usize,
    max_attempts: usize,
    created_at_ms: i64,
    next_retry_at_ms: i64,
    last_error: Option<String>,
}

impl AppState {
    async fn from_env() -> Self {
        let config = RelayConfig::from_env();
        let send_queue = load_send_queue(&config.matrix_send_queue_path).await;
        Self::new(config, send_queue)
    }

    fn new(config: RelayConfig, send_queue: VecDeque<QueuedMatrixSend>) -> Self {
        let metrics = RelayMetrics::new();
        metrics.record_queue_depth(send_queue.len());

        Self {
            inner: Arc::new(AppStateInner {
                http: Client::new(),
                config,
                recent_event_ids: Mutex::new(VecDeque::new()),
                send_queue: Mutex::new(send_queue),
                metrics,
            }),
        }
    }

    fn config(&self) -> &RelayConfig {
        &self.inner.config
    }

    fn metrics(&self) -> &RelayMetrics {
        &self.inner.metrics
    }
}

#[derive(Debug, Deserialize)]
struct InboundMatrixEvent {
    event_id: Option<String>,
    event_type: Option<String>,
    room_id: String,
    sender: String,
    text: Option<String>,
    content: Option<Value>,
    timestamp_ms: Option<i64>,
    metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
struct RelayResponse {
    accepted: bool,
    room_id: String,
    sender: String,
    upstream: Option<Value>,
    sent_to_matrix: bool,
    send_result: Option<Value>,
}

#[derive(Debug)]
struct MatrixSendFailure {
    kind: &'static str,
    message: String,
    status: Option<u16>,
    details: Option<Value>,
    attempts: usize,
    retryable: bool,
    retry_after_ms: Option<u64>,
}

#[tokio::main]
async fn main() {
    init_tracing();

    let state = AppState::from_env().await;

    if state.config().matrix_send_queue_enabled {
        let queue_worker_state = state.clone();
        tokio::spawn(async move {
            run_send_queue_worker(queue_worker_state).await;
        });
    }

    let app = Router::new()
        .route("/health", get(health))
        .route(
            "/v1/inbound/matrix-event",
            post(handle_inbound_matrix_event),
        )
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&state.config().bind_addr)
        .await
        .unwrap();
    info!(bind = %state.config().bind_addr, "matrix-bot-relay started");
    axum::serve(listener, app).await.unwrap();
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    let queue_len = { state.inner.send_queue.lock().await.len() };

    let metrics = state.metrics().snapshot(queue_len);

    Json(json!({
        "status": "ok",
        "service": "matrix-bot-relay",
        "matrix_adapter_base_url": state.config().matrix_adapter_base_url,
        "matrix_homeserver_base_url": state.config().matrix_homeserver_base_url,
        "matrix_bot_user_id": state.config().matrix_bot_user_id,
        "has_access_token": !state.config().matrix_access_token.is_empty(),
        "max_recent_event_ids": state.config().max_recent_event_ids,
        "matrix_send_max_attempts": state.config().matrix_send_max_attempts,
        "matrix_send_initial_delay_ms": state.config().matrix_send_initial_delay_ms,
        "matrix_send_max_delay_ms": state.config().matrix_send_max_delay_ms,
        "matrix_send_queue_enabled": state.config().matrix_send_queue_enabled,
        "matrix_send_queue_path": state.config().matrix_send_queue_path,
        "matrix_send_queue_poll_interval_ms": state.config().matrix_send_queue_poll_interval_ms,
        "matrix_send_queue_len": queue_len,
        "observability": metrics,
    }))
}

async fn handle_inbound_matrix_event(
    State(state): State<AppState>,
    Json(event): Json<InboundMatrixEvent>,
) -> Response {
    state.inner.metrics.inc_inbound_events();

    if event.sender == state.config().matrix_bot_user_id {
        state.inner.metrics.inc_self_events();
        return (
            HttpStatusCode::OK,
            Json(RelayResponse {
                accepted: false,
                room_id: event.room_id,
                sender: event.sender,
                upstream: None,
                sent_to_matrix: false,
                send_result: Some(json!({ "reason": "ignored_self_event" })),
            }),
        )
            .into_response();
    }

    if let Some(event_id) = event.event_id.as_deref() {
        let mut ids = state.inner.recent_event_ids.lock().await;
        if ids.contains(&event_id.to_string()) {
            state.inner.metrics.inc_duplicate_events();
            return (
                HttpStatusCode::OK,
                Json(RelayResponse {
                    accepted: false,
                    room_id: event.room_id,
                    sender: event.sender,
                    upstream: None,
                    sent_to_matrix: false,
                    send_result: Some(json!({ "reason": "duplicate_event", "event_id": event_id })),
                }),
            )
                .into_response();
        }

        ids.push_front(event_id.to_string());
        while ids.len() > state.config().max_recent_event_ids {
            ids.pop_back();
        }
    }

    state.inner.metrics.inc_adapter_request();

    let mut adapter_request = state
        .inner
        .http
        .post(format!(
            "{}/v1/matrix/events",
            state.config().matrix_adapter_base_url.trim_end_matches('/')
        ))
        .json(&json!({
            "event_id": event.event_id,
            "event_type": event.event_type,
            "room_id": event.room_id,
            "sender": event.sender,
            "text": event.text,
            "content": event.content,
            "timestamp_ms": event.timestamp_ms,
            "metadata": event.metadata,
        }));

    if let Some(token) = &state.config().matrix_adapter_ingress_token {
        adapter_request = adapter_request.header("x-entry-token", token);
    }

    let adapter_response = match adapter_request.send().await {
        Ok(response) => response,
        Err(err) => {
            state.inner.metrics.inc_adapter_failure();
            return error_response(
                HttpStatusCode::BAD_GATEWAY,
                "adapter_unreachable",
                format!("failed to reach matrix-entry-adapter: {err}"),
                Some(json!({ "retryable": true })),
            );
        }
    };

    let status = HttpStatusCode::from_u16(adapter_response.status().as_u16())
        .unwrap_or(HttpStatusCode::BAD_GATEWAY);
    let upstream = match adapter_response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            state.inner.metrics.inc_adapter_failure();
            return error_response(
                HttpStatusCode::BAD_GATEWAY,
                "adapter_invalid_response",
                format!("matrix-entry-adapter returned non-json response: {err}"),
                None,
            );
        }
    };

    if !status.is_success() {
        state.inner.metrics.inc_adapter_failure();
        return (
            status,
            Json(json!({
                "error_code": "adapter_error",
                "error": "matrix-entry-adapter request failed",
                "details": upstream,
            })),
        )
            .into_response();
    }

    let projected_reply = upstream.get("projected_reply").cloned();
    if projected_reply.is_none() {
        state.inner.metrics.inc_projected_reply_missing();
        return (
            HttpStatusCode::OK,
            Json(RelayResponse {
                accepted: true,
                room_id: upstream
                    .get("room_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                sender: upstream
                    .get("sender")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                upstream: Some(upstream),
                sent_to_matrix: false,
                send_result: Some(json!({ "reason": "no_projected_reply" })),
            }),
        )
            .into_response();
    }

    if state.config().matrix_access_token.is_empty() {
        return (
            HttpStatusCode::ACCEPTED,
            Json(RelayResponse {
                accepted: true,
                room_id: upstream
                    .get("room_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                sender: upstream
                    .get("sender")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                upstream: Some(upstream),
                sent_to_matrix: false,
                send_result: Some(json!({
                    "reason": "matrix_access_token_missing",
                    "projected_reply": projected_reply,
                })),
            }),
        )
            .into_response();
    }

    let event_id = event.event_id.clone();
    let room_id = upstream
        .get("room_id")
        .and_then(Value::as_str)
        .unwrap_or(&event.room_id);
    let projected_reply = projected_reply.unwrap_or_else(|| json!({}));

    let send_result = match send_matrix_reply(
        &state,
        room_id,
        &projected_reply,
        event_id.as_deref(),
        state.config().matrix_send_max_attempts,
    )
    .await
    {
        Ok(value) => (
            HttpStatusCode::OK,
            Json(RelayResponse {
                accepted: true,
                room_id: room_id.to_string(),
                sender: upstream
                    .get("sender")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                upstream: Some(upstream),
                sent_to_matrix: true,
                send_result: Some(value),
            }),
        )
            .into_response(),
        Err(failure) => {
            if should_queue_failure(&failure, &state).await {
                let queue_item = make_queue_item(
                    room_id,
                    event_id.clone(),
                    projected_reply,
                    &failure,
                    state.config().matrix_send_initial_delay_ms,
                    state.config().matrix_send_max_delay_ms,
                );
                let queued = enqueue_send(state.clone(), queue_item).await;

                if queued {
                    warn!(
                        room_id = room_id,
                        event_id = event_id.as_deref().unwrap_or("<missing>"),
                        status_code = failure.status,
                        kind = failure.kind,
                        attempts = failure.attempts,
                        "matrix send failed and queued for retry"
                    );
                } else {
                    state.inner.metrics.inc_queue_drop();
                    warn!(
                        room_id = room_id,
                        event_id = event_id.as_deref().unwrap_or("<missing>"),
                        status_code = failure.status,
                        kind = failure.kind,
                        attempts = failure.attempts,
                        "matrix send failed but queue rejected duplicate or saturated event"
                    );
                }

                return (
                    HttpStatusCode::ACCEPTED,
                    Json(RelayResponse {
                        accepted: false,
                        room_id: room_id.to_string(),
                        sender: upstream
                            .get("sender")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        upstream: Some(upstream),
                        sent_to_matrix: false,
                        send_result: Some(json!({
                            "reason": "queued_for_retry",
                            "attempts": failure.attempts,
                            "max_attempts": state.config().matrix_send_max_attempts,
                            "retry_after_ms": compute_next_retry_delay(
                                failure.attempts,
                                failure.retry_after_ms,
                                state.config().matrix_send_initial_delay_ms,
                                state.config().matrix_send_max_delay_ms,
                            )
                        })),
                    }),
                )
                    .into_response();
            }

            let status = failure
                .status
                .and_then(|code| HttpStatusCode::from_u16(code).ok())
                .unwrap_or(HttpStatusCode::BAD_GATEWAY);

            error!(
                room_id = room_id,
                event_id = event_id.as_deref().unwrap_or("<missing>"),
                status_code = failure.status,
                kind = failure.kind,
                "matrix send failed and was not queued"
            );

            error_response(
                status,
                failure.kind,
                failure.message,
                Some(json!({
                    "attempts": failure.attempts,
                    "status_code": failure.status,
                    "details": failure.details,
                    "retryable": failure.retryable,
                })),
            )
        }
    };

    send_result
}

fn is_retryable_status(status: ReqStatusCode) -> bool {
    status.is_server_error()
        || status == ReqStatusCode::TOO_MANY_REQUESTS
        || status == ReqStatusCode::REQUEST_TIMEOUT
}

fn parse_retry_after_ms(header_value: Option<&str>) -> Option<u64> {
    header_value
        .and_then(|raw| raw.parse::<u64>().ok())
        .map(|v| v.saturating_mul(1000))
}

async fn should_queue_failure(failure: &MatrixSendFailure, state: &AppState) -> bool {
    if !state.config().matrix_send_queue_enabled {
        return false;
    }

    if !failure.retryable {
        return false;
    }

    failure.attempts < state.config().matrix_send_max_attempts
}

fn make_queue_item(
    room_id: &str,
    event_id: Option<String>,
    projected_reply: Value,
    failure: &MatrixSendFailure,
    initial_delay_ms: u64,
    max_delay_ms: u64,
) -> QueuedMatrixSend {
    let now_ms = now_millis_i64();
    let delay_ms = compute_next_retry_delay(
        failure.attempts,
        failure.retry_after_ms,
        initial_delay_ms,
        max_delay_ms,
    );

    QueuedMatrixSend {
        queue_id: Uuid::new_v4().to_string(),
        room_id: room_id.to_string(),
        event_id,
        projected_reply,
        attempts: failure.attempts,
        max_attempts: 0,
        created_at_ms: now_ms,
        next_retry_at_ms: now_ms + delay_ms as i64,
        last_error: Some(failure.message.clone()),
    }
}

fn compute_next_retry_delay(
    attempt: usize,
    retry_after_ms: Option<u64>,
    initial_ms: u64,
    max_ms: u64,
) -> u64 {
    if let Some(delay) = retry_after_ms {
        return delay;
    }

    let exponent = attempt.saturating_sub(1) as u32;
    let mut delay = initial_ms.saturating_mul(2_u64.pow(exponent.min(31)));
    if delay > max_ms {
        delay = max_ms;
    }
    delay
}

fn now_millis_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

async fn send_matrix_reply(
    state: &AppState,
    room_id: &str,
    projected_reply: &Value,
    event_id: Option<&str>,
    max_attempts: usize,
) -> Result<Value, MatrixSendFailure> {
    let mut last_error = MatrixSendFailure {
        kind: "matrix_send_unset",
        message: "initializing".to_string(),
        status: None,
        details: None,
        attempts: 0,
        retryable: true,
        retry_after_ms: None,
    };

    let mut delay_ms = state.config().matrix_send_initial_delay_ms;
    let max_delay_ms = state.config().matrix_send_max_delay_ms.max(delay_ms);

    for attempt in 1..=max_attempts {
        let result = send_matrix_reply_once(state, room_id, projected_reply, event_id).await;

        match result {
            Ok(value) => return Ok(value),
            Err(mut failure) => {
                failure.attempts = attempt;
                last_error = failure;

                if !last_error.retryable || attempt >= max_attempts {
                    last_error.kind = if attempt >= max_attempts && last_error.retryable {
                        "matrix_send_http_retry_exhausted"
                    } else {
                        last_error.kind
                    };
                    break;
                }

                warn!(
                    room_id = %room_id,
                    event_id = event_id.unwrap_or("<missing>"),
                    attempt = attempt,
                    error = %last_error.message,
                    "matrix send attempt failed, retrying"
                );
                if let Some(wait_ms) = last_error.retry_after_ms {
                    sleep(Duration::from_millis(wait_ms)).await;
                } else {
                    sleep(Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms.saturating_mul(2)).min(max_delay_ms);
                }
            }
        }
    }

    Err(last_error)
}

async fn send_matrix_reply_once(
    state: &AppState,
    room_id: &str,
    projected_reply: &Value,
    _event_id: Option<&str>,
) -> Result<Value, MatrixSendFailure> {
    let config = state.config().clone();
    let txn_id = format!(
        "cexbot-{}-{}",
        Utc::now().timestamp_millis(),
        Uuid::new_v4()
    );
    let send_url = format!(
        "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
        config.matrix_homeserver_base_url.trim_end_matches('/'),
        encode_path(room_id),
        txn_id,
    );

    state.inner.metrics.inc_matrix_send_attempt();

    let result = state
        .inner
        .http
        .put(send_url)
        .bearer_auth(&config.matrix_access_token)
        .json(projected_reply)
        .send()
        .await;

    match result {
        Ok(resp) => {
            let status = resp.status();
            let retry_after_ms = resp
                .headers()
                .get("retry-after")
                .and_then(|h| h.to_str().ok())
                .and_then(|raw| parse_retry_after_ms(Some(raw)));
            let text = resp.text().await.unwrap_or_else(|_| "".to_string());
            let details = serde_json::from_str::<Value>(&text).ok().or_else(|| {
                if text.is_empty() {
                    None
                } else {
                    Some(json!({ "raw_body": text }))
                }
            });

            if status.is_success() {
                state.inner.metrics.inc_matrix_send_success();
                return Ok(details.unwrap_or_else(|| json!({ "status": status.as_u16() })));
            }

            state.inner.metrics.inc_matrix_send_failure();
            let retryable = is_retryable_status(status);
            let kind = if retryable {
                "matrix_send_http_retryable_error"
            } else {
                "matrix_send_http_error"
            };

            Err(MatrixSendFailure {
                kind,
                message: format!("matrix send returned status {status}"),
                status: Some(status.as_u16()),
                details,
                attempts: 0,
                retryable,
                retry_after_ms,
            })
        }
        Err(err) => {
            state.inner.metrics.inc_matrix_send_failure();

            Err(MatrixSendFailure {
                kind: "matrix_send_network_error",
                message: format!("failed to send reply to matrix homeserver: {err}"),
                status: None,
                details: Some(json!({ "error": err.to_string() })),
                attempts: 0,
                retryable: true,
                retry_after_ms: None,
            })
        }
    }
}

async fn enqueue_send(state: AppState, mut item: QueuedMatrixSend) -> bool {
    if !state.config().matrix_send_queue_enabled {
        return false;
    }

    let mut queue = state.inner.send_queue.lock().await;

    if queue.iter().any(|entry| {
        entry.queue_id == item.queue_id
            || (entry.room_id == item.room_id
                && entry.event_id.is_some()
                && entry.event_id == item.event_id
                && entry.projected_reply == item.projected_reply)
    }) {
        return false;
    }

    if let Some(event_id) = item.event_id.as_ref() {
        if queue
            .iter()
            .filter(|q| q.event_id.as_ref() == Some(event_id))
            .count()
            >= 1
        {
            return false;
        }
    }

    if item.max_attempts == 0 {
        item.max_attempts = state.config().matrix_send_max_attempts;
    }

    queue.push_back(item);

    let mut dropped_count = 0usize;
    while queue.len() > state.config().matrix_send_queue_max_size {
        queue.pop_front();
        dropped_count = dropped_count.saturating_add(1);
    }

    state.inner.metrics.inc_queue_enqueued();

    for _ in 0..dropped_count {
        state.inner.metrics.inc_queue_drop();
    }

    let queue_len = queue.len();
    state.inner.metrics.record_queue_depth(queue_len);

    if let Err(err) = persist_send_queue(&state.config().matrix_send_queue_path, &queue).await {
        warn!(
            path = %state.config().matrix_send_queue_path,
            error = %err,
            "failed to persist relay send queue"
        );
    }

    true
}

async fn run_send_queue_worker(state: AppState) {
    let poll_interval_ms = state.config().matrix_send_queue_poll_interval_ms;
    info!(
        interval_ms = poll_interval_ms,
        "starting matrix relay send queue worker"
    );

    loop {
        process_due_queue_items(state.clone()).await;
        sleep(Duration::from_millis(poll_interval_ms)).await;
    }
}

async fn process_due_queue_items(state: AppState) {
    let now_ms = now_millis_i64();
    let mut due: Vec<QueuedMatrixSend> = Vec::new();

    {
        let mut queue = state.inner.send_queue.lock().await;
        let mut remaining = VecDeque::new();
        while let Some(item) = queue.pop_front() {
            if item.next_retry_at_ms <= now_ms {
                due.push(item);
            } else {
                remaining.push_back(item);
            }
        }

        *queue = remaining;
        state.inner.metrics.record_queue_depth(queue.len());

        if let Err(err) = persist_send_queue(&state.config().matrix_send_queue_path, &queue).await {
            warn!(
                path = %state.config().matrix_send_queue_path,
                error = %err,
                "failed to persist queue state"
            );
        }
    }

    for mut item in due {
        state.inner.metrics.inc_queue_attempt();

        match send_matrix_reply_once(
            &state,
            &item.room_id,
            &item.projected_reply,
            item.event_id.as_deref(),
        )
        .await
        {
            Ok(_) => {
                state.inner.metrics.inc_queue_success();
                info!(
                    queue_id = %item.queue_id,
                    room_id = %item.room_id,
                    event_id = item.event_id.unwrap_or_else(|| "<missing>".to_string()),
                    attempts = item.attempts,
                    "queued matrix reply sent successfully"
                );
            }
            Err(failure) => {
                let next_attempt = item.attempts.saturating_add(1);
                if !failure.retryable || next_attempt > item.max_attempts {
                    state.inner.metrics.inc_queue_drop();
                    state.inner.metrics.inc_queue_failure();
                    warn!(
                        queue_id = %item.queue_id,
                        room_id = %item.room_id,
                        error = %failure.message,
                        attempts = item.attempts,
                        "dropping queue item after exhausting attempts"
                    );
                    continue;
                }

                let wait_ms = compute_next_retry_delay(
                    next_attempt,
                    failure.retry_after_ms,
                    state.config().matrix_send_initial_delay_ms,
                    state.config().matrix_send_max_delay_ms,
                );

                item.attempts = next_attempt;
                item.next_retry_at_ms = now_millis_i64() + wait_ms as i64;
                item.last_error = Some(failure.message);

                if enqueue_send(state.clone(), item).await {
                    state.inner.metrics.inc_queue_requeue();
                }
            }
        }
    }
}

async fn persist_send_queue(path: &str, queue: &VecDeque<QueuedMatrixSend>) -> Result<(), String> {
    if path.is_empty() {
        return Ok(());
    }

    if let Some(parent) = Path::new(path).parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            return Err(format!("failed to create queue parent dir: {err}"));
        }
    }

    let encoded = serde_json::to_string_pretty(queue).map_err(|err| err.to_string())?;
    fs::write(path, encoded).map_err(|err| err.to_string())
}

async fn load_send_queue(path: &str) -> VecDeque<QueuedMatrixSend> {
    if path.is_empty() || !Path::new(path).exists() {
        return VecDeque::new();
    }

    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) => {
            warn!(path = %path, error = %err, "failed to read relay send queue, initializing empty");
            return VecDeque::new();
        }
    };

    match serde_json::from_str::<Vec<QueuedMatrixSend>>(&raw) {
        Ok(items) => VecDeque::from(items),
        Err(err) => {
            warn!(path = %path, error = %err, "failed to parse relay send queue, initializing empty");
            VecDeque::new()
        }
    }
}

fn encode_path(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn error_response(
    status: HttpStatusCode,
    code: &str,
    message: String,
    details: Option<Value>,
) -> Response {
    (
        status,
        Json(json!({
            "error_code": code,
            "error": message,
            "details": details,
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::encode_path;

    #[test]
    fn room_id_is_percent_encoded() {
        assert_eq!(encode_path("!room:local.dev"), "%21room%3Alocal.dev");
    }
}
