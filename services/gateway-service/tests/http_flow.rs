use axum::{
    body::{to_bytes, Body},
    extract::{Path, State},
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use gateway_service::{
    build_router,
    infrastructure::{
        clients::ServiceClients,
        state::{AppState, GatewayRuntimeMetrics},
    },
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{atomic::Ordering, Arc},
};
use tokio::{
    net::TcpListener,
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use tower::util::ServiceExt;
use uuid::Uuid;

#[derive(Clone)]
struct MockConfig {
    auth_status: u16,
    auth_body: Value,
    capability_status: u16,
    capability_body: Value,
    account_status: u16,
    account_body: Value,
    reserve_status: u16,
    refund_status: u16,
    execution_status: u16,
    execution_body: Value,
    execution_start_status: u16,
    execution_start_body: Value,
    audit_status: u16,
}

impl MockConfig {
    fn queued() -> Self {
        Self {
            auth_status: 200,
            auth_body: json!({
                "org_id": "00000000-0000-0000-0000-00000000ce01",
                "actor_id": "local-dev-actor",
                "actor_label": "Local Dev"
            }),
            capability_status: 200,
            capability_body: json!({
                "capability_id": "summarize",
                "kind": "model",
                "provider": "demo",
                "provider_ref": "demo/summarize-v1",
                "display_name": "Summarize",
                "version": "v1",
                "description": "mock capability",
                "enabled": true
            }),
            account_status: 200,
            account_body: json!({
                "account_id": Uuid::from_u128(0x11111111111111111111111111111111_u128),
                "org_id": "00000000-0000-0000-0000-00000000ce01",
                "account_type": "org_wallet",
                "currency_unit": "credit",
                "balance": 100.0,
                "reserved": 0.0
            }),
            reserve_status: 200,
            refund_status: 200,
            execution_status: 200,
            execution_body: json!({
                "execution_id": Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa_u128),
                "status": "Queued",
                "approval_required": false,
                "policy_reason": null,
                "dispatch_mode": "manual",
                "attempt_count": 0,
                "max_attempts": 1
            }),
            execution_start_status: 200,
            execution_start_body: json!({
                "execution_id": Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa_u128),
                "status": "Succeeded",
                "approval_required": false,
                "policy_reason": null,
                "dispatch_mode": "manual",
                "attempt_count": 0,
                "max_attempts": 1
            }),
            audit_status: 200,
        }
    }

    fn awaiting_approval() -> Self {
        Self {
            auth_status: 200,
            auth_body: json!({
                "org_id": "00000000-0000-0000-0000-00000000ce01",
                "actor_id": "local-dev-actor",
                "actor_label": "Local Dev"
            }),
            capability_status: 200,
            capability_body: json!({
                "capability_id": "publish",
                "kind": "workflow",
                "provider": "demo",
                "provider_ref": "demo/publish-v1",
                "display_name": "Publish",
                "version": "v1",
                "description": "mock capability",
                "enabled": true
            }),
            account_status: 200,
            account_body: json!({
                "account_id": Uuid::from_u128(0x22222222222222222222222222222222_u128),
                "org_id": "00000000-0000-0000-0000-00000000ce01",
                "account_type": "org_wallet",
                "currency_unit": "credit",
                "balance": 100.0,
                "reserved": 0.0
            }),
            reserve_status: 200,
            refund_status: 200,
            execution_status: 200,
            execution_body: json!({
                "execution_id": Uuid::from_u128(0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb_u128),
                "status": "AwaitingApproval",
                "approval_required": true,
                "policy_reason": "prompt matched sensitive keyword 'publish'",
                "dispatch_mode": "manual",
                "attempt_count": 0,
                "max_attempts": 1
            }),
            execution_start_status: 200,
            execution_start_body: json!({
                "execution_id": Uuid::from_u128(0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb_u128),
                "status": "AwaitingApproval",
                "approval_required": true,
                "policy_reason": "prompt matched sensitive keyword 'publish'",
                "dispatch_mode": "manual",
                "attempt_count": 0,
                "max_attempts": 1
            }),
            audit_status: 200,
        }
    }

    fn reserve_failure() -> Self {
        Self {
            auth_status: 200,
            auth_body: json!({
                "org_id": "00000000-0000-0000-0000-00000000ce01",
                "actor_id": "local-dev-actor",
                "actor_label": "Local Dev"
            }),
            capability_status: 200,
            capability_body: json!({
                "capability_id": "summarize",
                "kind": "model",
                "provider": "demo",
                "provider_ref": "demo/summarize-v1",
                "display_name": "Summarize",
                "version": "v1",
                "description": "mock capability",
                "enabled": true
            }),
            account_status: 200,
            account_body: json!({
                "account_id": Uuid::from_u128(0x33333333333333333333333333333333_u128),
                "org_id": "00000000-0000-0000-0000-00000000ce01",
                "account_type": "org_wallet",
                "currency_unit": "credit",
                "balance": 100.0,
                "reserved": 0.0
            }),
            reserve_status: 502,
            refund_status: 200,
            execution_status: 200,
            execution_body: json!({
                "execution_id": Uuid::from_u128(0xcccccccccccccccccccccccccccccccc_u128),
                "status": "Queued",
                "approval_required": false,
                "policy_reason": null,
                "dispatch_mode": "manual",
                "attempt_count": 0,
                "max_attempts": 1
            }),
            execution_start_status: 200,
            execution_start_body: json!({
                "execution_id": Uuid::from_u128(0xcccccccccccccccccccccccccccccccc_u128),
                "status": "Succeeded",
                "approval_required": false,
                "policy_reason": null,
                "dispatch_mode": "manual",
                "attempt_count": 0,
                "max_attempts": 1
            }),
            audit_status: 200,
        }
    }

    fn execution_failure_with_refund() -> Self {
        Self {
            auth_status: 200,
            auth_body: json!({
                "org_id": "00000000-0000-0000-0000-00000000ce01",
                "actor_id": "local-dev-actor",
                "actor_label": "Local Dev"
            }),
            capability_status: 200,
            capability_body: json!({
                "capability_id": "summarize",
                "kind": "model",
                "provider": "demo",
                "provider_ref": "demo/summarize-v1",
                "display_name": "Summarize",
                "version": "v1",
                "description": "mock capability",
                "enabled": true
            }),
            account_status: 200,
            account_body: json!({
                "account_id": Uuid::from_u128(0x44444444444444444444444444444444_u128),
                "org_id": "00000000-0000-0000-0000-00000000ce01",
                "account_type": "org_wallet",
                "currency_unit": "credit",
                "balance": 100.0,
                "reserved": 0.0
            }),
            reserve_status: 200,
            refund_status: 200,
            execution_status: 502,
            execution_body: json!({ "error": "execution unavailable" }),
            execution_start_status: 502,
            execution_start_body: json!({ "error": "execution unavailable" }),
            audit_status: 200,
        }
    }

    fn auth_failure() -> Self {
        Self {
            auth_status: 401,
            auth_body: json!({
                "error": "revoked api key"
            }),
            capability_status: 200,
            capability_body: json!({
                "capability_id": "summarize",
                "kind": "model",
                "provider": "demo",
                "provider_ref": "demo/summarize-v1",
                "display_name": "Summarize",
                "version": "v1",
                "description": "mock capability",
                "enabled": true
            }),
            account_status: 200,
            account_body: json!({
                "account_id": Uuid::from_u128(0x66666666666666666666666666666666_u128),
                "org_id": "00000000-0000-0000-0000-00000000ce01",
                "account_type": "org_wallet",
                "currency_unit": "credit",
                "balance": 100.0,
                "reserved": 0.0
            }),
            reserve_status: 200,
            refund_status: 200,
            execution_status: 200,
            execution_body: json!({
                "execution_id": Uuid::from_u128(0x66666666666666666666666666666666_u128),
                "status": "Queued",
                "approval_required": false,
                "policy_reason": null,
                "dispatch_mode": "manual",
                "attempt_count": 0,
                "max_attempts": 1
            }),
            execution_start_status: 200,
            execution_start_body: json!({
                "execution_id": Uuid::from_u128(0x66666666666666666666666666666666_u128),
                "status": "Succeeded",
                "approval_required": false,
                "policy_reason": null,
                "dispatch_mode": "manual",
                "attempt_count": 0,
                "max_attempts": 1
            }),
            audit_status: 200,
        }
    }
}

#[derive(Clone)]
struct MockState {
    config: Arc<MockConfig>,
    calls: Arc<Mutex<Vec<String>>>,
}

struct MockServer {
    base_url: String,
    calls: Arc<Mutex<Vec<String>>>,
    _handle: JoinHandle<()>,
}

impl MockServer {
    async fn calls(&self) -> Vec<String> {
        self.calls.lock().await.clone()
    }
}

async fn auth_resolve(
    State(state): State<MockState>,
    Json(_body): Json<Value>,
) -> impl IntoResponse {
    state
        .calls
        .lock()
        .await
        .push("identity.resolve".to_string());
    let status = StatusCode::from_u16(state.config.auth_status).expect("valid auth status");
    (status, Json(state.config.auth_body.clone())).into_response()
}

async fn get_capability(
    Path(_id): Path<String>,
    State(state): State<MockState>,
) -> impl IntoResponse {
    state.calls.lock().await.push("capability.get".to_string());
    let status =
        StatusCode::from_u16(state.config.capability_status).expect("valid capability status");
    (status, Json(state.config.capability_body.clone())).into_response()
}

async fn get_account(Path(_id): Path<Uuid>, State(state): State<MockState>) -> impl IntoResponse {
    state
        .calls
        .lock()
        .await
        .push("ledger.get_account".to_string());
    let status = StatusCode::from_u16(state.config.account_status).expect("valid account status");
    (status, Json(state.config.account_body.clone())).into_response()
}

async fn ledger_reserve(
    State(state): State<MockState>,
    Json(_body): Json<Value>,
) -> impl IntoResponse {
    state.calls.lock().await.push("ledger.reserve".to_string());
    let status = StatusCode::from_u16(state.config.reserve_status).expect("valid reserve status");
    (status, Json(json!({ "ok": status.is_success() }))).into_response()
}

async fn ledger_refund(
    State(state): State<MockState>,
    Json(_body): Json<Value>,
) -> impl IntoResponse {
    state.calls.lock().await.push("ledger.refund".to_string());
    let status = StatusCode::from_u16(state.config.refund_status).expect("valid refund status");
    (status, Json(json!({ "ok": status.is_success() }))).into_response()
}

async fn create_execution(
    State(state): State<MockState>,
    Json(_body): Json<Value>,
) -> impl IntoResponse {
    state
        .calls
        .lock()
        .await
        .push("execution.create".to_string());
    let status =
        StatusCode::from_u16(state.config.execution_status).expect("valid execution status");
    (status, Json(state.config.execution_body.clone())).into_response()
}

async fn get_execution(Path(_id): Path<Uuid>, State(state): State<MockState>) -> impl IntoResponse {
    state.calls.lock().await.push("execution.get".to_string());
    let status =
        StatusCode::from_u16(state.config.execution_status).expect("valid execution status");
    (status, Json(state.config.execution_body.clone())).into_response()
}

async fn start_execution(
    Path(_id): Path<Uuid>,
    State(state): State<MockState>,
    Json(_body): Json<Value>,
) -> impl IntoResponse {
    state.calls.lock().await.push("execution.start".to_string());
    let status = StatusCode::from_u16(state.config.execution_start_status)
        .expect("valid execution start status");
    (status, Json(state.config.execution_start_body.clone())).into_response()
}

async fn audit_event(
    State(state): State<MockState>,
    Json(_body): Json<Value>,
) -> impl IntoResponse {
    state.calls.lock().await.push("audit.emit".to_string());
    let status = StatusCode::from_u16(state.config.audit_status).expect("valid audit status");
    (status, Json(json!({ "ok": status.is_success() }))).into_response()
}

async fn start_mock_server(config: MockConfig) -> MockServer {
    let state = MockState {
        config: Arc::new(config),
        calls: Arc::new(Mutex::new(Vec::new())),
    };

    let app = Router::new()
        .route("/v1/auth/resolve", post(auth_resolve))
        .route("/v1/capabilities/:id", get(get_capability))
        .route("/v1/accounts/:id", get(get_account))
        .route("/v1/ledger/reserve", post(ledger_reserve))
        .route("/v1/ledger/refund", post(ledger_refund))
        .route("/v1/executions", post(create_execution))
        .route("/v1/executions/:id", get(get_execution))
        .route("/v1/executions/:id/start", post(start_execution))
        .route("/v1/audit/events", post(audit_event))
        .with_state(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock upstream server");
    let addr = listener.local_addr().expect("mock upstream local addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("run mock upstream server");
    });

    MockServer {
        base_url: format!("http://{addr}"),
        calls: state.calls,
        _handle: handle,
    }
}

fn test_state(base_url: String) -> AppState {
    test_state_with_legacy_reserve_break_glass(base_url, false)
}

fn legacy_test_state(base_url: String) -> AppState {
    test_state_with_legacy_reserve_break_glass(base_url, true)
}

fn test_state_with_legacy_reserve_break_glass(
    base_url: String,
    legacy_reserve_break_glass: bool,
) -> AppState {
    AppState {
        invocations: Arc::new(RwLock::new(HashMap::new())),
        clients: Arc::new(ServiceClients::new(
            base_url.clone(),
            base_url.clone(),
            base_url.clone(),
            base_url.clone(),
            base_url,
        )),
        pool: None,
        fail_fast: false,
        legacy_reserve_break_glass,
        metrics: Arc::new(GatewayRuntimeMetrics::default()),
        alert_gateway_upstream_failure_threshold: 1,
    }
}

async fn send_json_with_auth(
    app: Router,
    method: &str,
    uri: &str,
    auth_header: Option<(&str, &str)>,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");

    if let Some((header, value)) = auth_header {
        request = request.header(header, value);
    }

    let request = request
        .body(Body::from(
            serde_json::to_vec(&body).expect("serialize request body"),
        ))
        .expect("build request");

    let response = app.oneshot(request).await.expect("router response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let json: Value = serde_json::from_slice(&bytes).expect("decode response json");
    (status, json)
}

async fn send_json(app: Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send_json_with_auth(app, method, uri, Some(("x-api-key", "local-dev-key")), body).await
}

async fn get_json_with_auth(
    app: Router,
    uri: &str,
    auth_header: Option<(&str, &str)>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method("GET").uri(uri);

    if let Some((header, value)) = auth_header {
        request = request.header(header, value);
    }

    let request = request.body(Body::empty()).expect("build get request");

    let response = app.oneshot(request).await.expect("router response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let json: Value = serde_json::from_slice(&bytes).expect("decode response json");
    (status, json)
}

async fn get_json(app: Router, uri: &str) -> (StatusCode, Value) {
    get_json_with_auth(app, uri, Some(("x-api-key", "local-dev-key"))).await
}

async fn get_text(app: Router, uri: &str) -> (StatusCode, String, String) {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build get request");

    let response = app.oneshot(request).await.expect("router response");
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let text = String::from_utf8(bytes.to_vec()).expect("decode text body");
    (status, content_type, text)
}

#[tokio::test]
async fn metrics_endpoint_exports_gateway_counters_and_signals() {
    let upstream = start_mock_server(MockConfig::queued()).await;
    let state = test_state(upstream.base_url.clone());
    state
        .metrics
        .invocation_create_requests
        .store(2, Ordering::Relaxed);
    state
        .metrics
        .invocation_create_upstream_failures
        .store(1, Ordering::Relaxed);
    let app = build_router(state);

    let (status, content_type, body) = get_text(app, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/plain; version=0.0.4"));
    assert!(
        body.contains("cex_gateway_runtime_counter_total{name=\"invocation_create_requests\"} 2\n")
    );
    assert!(body.contains(
        "cex_gateway_operator_signal_active{name=\"invocation_create_upstream_failures\"} 1\n"
    ));
    assert!(body.contains(
        "cex_gateway_operator_signal_threshold{name=\"invocation_create_upstream_failures\"} 1\n"
    ));
}

#[tokio::test]
async fn missing_api_key_returns_401_without_upstream_calls() {
    let upstream = start_mock_server(MockConfig::queued()).await;
    let app = build_router(test_state(upstream.base_url.clone()));

    let request_body = json!({
        "prompt": "summarize logs"
    });

    let (status, created) =
        send_json_with_auth(app, "POST", "/v1/invocations", None, request_body).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(created["error"], "missing api key");

    let calls = upstream.calls().await;
    assert!(calls.is_empty());
}

#[tokio::test]
async fn legacy_reserve_fails_closed_before_auth_or_upstream_calls() {
    let upstream = start_mock_server(MockConfig::queued()).await;
    let state = test_state_with_legacy_reserve_break_glass(upstream.base_url.clone(), false);
    let invocations = state.invocations.clone();
    let app = build_router(state);

    let request_body = json!({
        "account_id": Uuid::from_u128(0x11111111111111111111111111111111_u128),
        "prompt": "legacy reserve must be migrated",
        "reserve_amount": 5.0
    });

    let (status, response) = send_json(app, "POST", "/v1/invocations", request_body).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(response["error"], "legacy_reserve_requires_exact_ingress");
    assert_eq!(
        response["migration"]["exact_reserve"],
        "POST /v2/invocations/:invocation_id/exact-reserve"
    );
    assert!(invocations.read().await.is_empty());
    assert!(upstream.calls().await.is_empty());
}

#[tokio::test]
async fn get_invocation_requires_api_key() {
    let upstream = start_mock_server(MockConfig::queued()).await;
    let invocation_id = Uuid::new_v4();
    let state = test_state(upstream.base_url.clone());
    state.invocations.write().await.insert(
        invocation_id,
        gateway_service::domain::invocation::InvocationRecord {
            invocation_id,
            trace: shared_types::TraceContext {
                trace_id: Uuid::new_v4(),
                created_at: chrono::Utc::now(),
            },
            status: shared_types::ExecutionStatus::Queued,
            request: gateway_service::domain::invocation::InvocationRequest {
                org_id: "00000000-0000-0000-0000-00000000ce01".to_string(),
                actor_id: Some("local-dev-actor".to_string()),
                capability_id: Some("summarize".to_string()),
                capability_provider: Some("demo".to_string()),
                capability_provider_ref: Some("demo/summarize-v1".to_string()),
                account_id: None,
                prompt: "summarize logs".to_string(),
                reserve_amount: None,
            },
            ledger_reserved: false,
            ledger_refunded: false,
            approval_required: false,
            execution_id: None,
            execution: None,
            policy_reason: None,
            failure_reason: None,
        },
    );
    let app = build_router(state);

    let (status, fetched) =
        get_json_with_auth(app, &format!("/v1/invocations/{invocation_id}"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(fetched["error"], "missing api key");

    let calls = upstream.calls().await;
    assert!(calls.is_empty());
}

#[tokio::test]
async fn get_invocation_rejects_api_key_for_other_org() {
    let upstream = start_mock_server(MockConfig::queued()).await;
    let state = test_state(upstream.base_url.clone());
    let invocation_id = Uuid::new_v4();
    state.invocations.write().await.insert(
        invocation_id,
        gateway_service::domain::invocation::InvocationRecord {
            invocation_id,
            trace: shared_types::TraceContext {
                trace_id: Uuid::new_v4(),
                created_at: chrono::Utc::now(),
            },
            status: shared_types::ExecutionStatus::Queued,
            request: gateway_service::domain::invocation::InvocationRequest {
                org_id: "00000000-0000-0000-0000-00000000ce99".to_string(),
                actor_id: Some("other-actor".to_string()),
                capability_id: Some("summarize".to_string()),
                capability_provider: Some("demo".to_string()),
                capability_provider_ref: Some("demo/summarize-v1".to_string()),
                account_id: None,
                prompt: "summarize logs".to_string(),
                reserve_amount: None,
            },
            ledger_reserved: false,
            ledger_refunded: false,
            approval_required: false,
            execution_id: None,
            execution: None,
            policy_reason: None,
            failure_reason: None,
        },
    );
    let app = build_router(state);

    let (status, fetched) = get_json(app, &format!("/v1/invocations/{invocation_id}")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(fetched["error"], "api key not authorized for org");
    assert_eq!(fetched["message"], "00000000-0000-0000-0000-00000000ce99");

    let calls = upstream.calls().await;
    assert_eq!(calls, vec!["identity.resolve".to_string()]);
}

#[tokio::test]
async fn get_invocation_returns_cached_execution_snapshot_when_execution_lookup_fails() {
    let mut config = MockConfig::queued();
    config.execution_status = 503;
    config.execution_body = json!({ "error": "execution unavailable" });

    let upstream = start_mock_server(config).await;
    let state = test_state(upstream.base_url.clone());
    let invocation_id = Uuid::new_v4();
    let execution_id = Uuid::from_u128(0x77777777777777777777777777777777_u128);
    state.invocations.write().await.insert(
        invocation_id,
        gateway_service::domain::invocation::InvocationRecord {
            invocation_id,
            trace: shared_types::TraceContext {
                trace_id: Uuid::new_v4(),
                created_at: chrono::Utc::now(),
            },
            status: shared_types::ExecutionStatus::Queued,
            request: gateway_service::domain::invocation::InvocationRequest {
                org_id: "00000000-0000-0000-0000-00000000ce01".to_string(),
                actor_id: Some("local-dev-actor".to_string()),
                capability_id: Some("summarize".to_string()),
                capability_provider: Some("openai".to_string()),
                capability_provider_ref: Some("gpt-4.1-mini".to_string()),
                account_id: None,
                prompt: "summarize logs".to_string(),
                reserve_amount: None,
            },
            ledger_reserved: false,
            ledger_refunded: false,
            approval_required: false,
            execution_id: Some(execution_id),
            execution: Some(
                gateway_service::domain::invocation::InvocationExecutionState {
                    dispatch_mode: shared_types::ExecutionDispatchMode::QueuedWorker,
                    attempt_count: 2,
                    max_attempts: 3,
                    attempts_remaining: 1,
                    retry_budget_exhausted: false,
                },
            ),
            policy_reason: None,
            failure_reason: None,
        },
    );
    let app = build_router(state);

    let (status, fetched) = get_json(app, &format!("/v1/invocations/{invocation_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["execution"]["dispatch_mode"], "queued_worker");
    assert_eq!(fetched["execution"]["attempt_count"], 2);
    assert_eq!(fetched["execution"]["max_attempts"], 3);
    assert_eq!(fetched["execution"]["attempts_remaining"], 1);
    assert_eq!(fetched["execution"]["retry_budget_exhausted"], false);

    let calls = upstream.calls().await;
    assert!(calls.contains(&"identity.resolve".to_string()));
    assert!(calls.contains(&"execution.get".to_string()));
}

#[tokio::test]
async fn bearer_authorization_header_is_accepted() {
    let upstream = start_mock_server(MockConfig::queued()).await;
    let app = build_router(legacy_test_state(upstream.base_url.clone()));

    let request_body = json!({
        "capability_id": "summarize",
        "account_id": Uuid::from_u128(0x11111111111111111111111111111111_u128),
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });

    let (status, created) = send_json_with_auth(
        app,
        "POST",
        "/v1/invocations",
        Some(("authorization", "Bearer local-dev-key")),
        request_body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["status"], "Queued");
}

#[tokio::test]
async fn invalid_capability_id_returns_400_before_ledger() {
    let mut config = MockConfig::queued();
    config.capability_status = 404;
    config.capability_body = json!({ "error": "capability not found" });

    let upstream = start_mock_server(config).await;
    let app = build_router(legacy_test_state(upstream.base_url.clone()));

    let request_body = json!({
        "capability_id": "missing.capability",
        "account_id": Uuid::from_u128(0x66666666666666666666666666666666_u128),
        "prompt": "should fail capability",
        "reserve_amount": 5.0
    });

    let (status, created) = send_json(app, "POST", "/v1/invocations", request_body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(created["error"], "invalid capability id");
    assert_eq!(created["message"], "missing.capability");

    let calls = upstream.calls().await;
    assert!(calls.contains(&"identity.resolve".to_string()));
    assert!(calls.contains(&"capability.get".to_string()));
    assert!(!calls.contains(&"ledger.get_account".to_string()));
    assert!(!calls.contains(&"ledger.reserve".to_string()));
    assert!(!calls.contains(&"execution.create".to_string()));
}

#[tokio::test]
async fn auth_resolution_failure_returns_401_and_stops_before_ledger() {
    let upstream = start_mock_server(MockConfig::auth_failure()).await;
    let app = build_router(legacy_test_state(upstream.base_url.clone()));

    let request_body = json!({
        "account_id": Uuid::from_u128(0x66666666666666666666666666666666_u128),
        "prompt": "should fail auth",
        "reserve_amount": 5.0
    });

    let (status, created) = send_json(app, "POST", "/v1/invocations", request_body).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(created["error"], "auth resolution failed");
    assert!(created["message"]
        .as_str()
        .expect("auth failure message")
        .contains("revoked api key"));

    let calls = upstream.calls().await;
    assert!(calls.contains(&"identity.resolve".to_string()));
    assert!(!calls.contains(&"ledger.get_account".to_string()));
    assert!(!calls.contains(&"ledger.reserve".to_string()));
    assert!(!calls.contains(&"execution.create".to_string()));
}

#[tokio::test]
async fn create_invocation_happy_path_reserves_and_queues_execution() {
    let upstream = start_mock_server(MockConfig::queued()).await;
    let app = build_router(legacy_test_state(upstream.base_url.clone()));

    let request_body = json!({
        "capability_id": "summarize",
        "account_id": Uuid::from_u128(0x11111111111111111111111111111111_u128),
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });

    let (status, created) = send_json(app.clone(), "POST", "/v1/invocations", request_body).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["status"], "Queued");
    assert_eq!(created["ledger_reserved"], true);
    assert_eq!(created["approval_required"], false);
    assert!(created["execution_id"].is_string());
    assert_eq!(created["execution"]["dispatch_mode"], "manual");
    assert_eq!(created["execution"]["attempt_count"], 0);
    assert_eq!(created["execution"]["max_attempts"], 1);
    assert_eq!(created["execution"]["attempts_remaining"], 1);
    assert_eq!(created["execution"]["retry_budget_exhausted"], false);

    let invocation_id = created["invocation_id"].as_str().expect("invocation id");
    let (get_status, fetched) = get_json(app, &format!("/v1/invocations/{invocation_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Queued");
    assert_eq!(fetched["ledger_reserved"], true);
    assert_eq!(fetched["execution"]["dispatch_mode"], "manual");
    assert_eq!(fetched["execution"]["attempt_count"], 0);
    assert_eq!(fetched["execution"]["max_attempts"], 1);
    assert_eq!(fetched["execution"]["attempts_remaining"], 1);
    assert_eq!(fetched["execution"]["retry_budget_exhausted"], false);

    let calls = upstream.calls().await;
    assert!(calls.contains(&"capability.get".to_string()));
    assert!(calls.contains(&"ledger.get_account".to_string()));
    assert!(calls.contains(&"ledger.reserve".to_string()));
    assert!(calls.contains(&"execution.create".to_string()));
    assert!(calls.contains(&"execution.get".to_string()));
    assert!(
        calls
            .iter()
            .filter(|call| call.as_str() == "audit.emit")
            .count()
            >= 3
    );
}

#[tokio::test]
async fn create_invocation_immediate_dispatch_auto_starts_provider_capability_and_returns_succeeded(
) {
    let mut config = MockConfig::queued();
    config.capability_body = json!({
        "capability_id": "summarize",
        "kind": "model",
        "provider": "ollama",
        "provider_ref": "qwen2.5:3b",
        "display_name": "Summarize",
        "version": "v1",
        "description": "mock capability",
        "enabled": true
    });
    config.execution_body = json!({
        "execution_id": Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa_u128),
        "status": "Queued",
        "approval_required": false,
        "policy_reason": null,
        "dispatch_mode": "immediate",
        "attempt_count": 0,
        "max_attempts": 1
    });

    let upstream = start_mock_server(config).await;
    let app = build_router(legacy_test_state(upstream.base_url.clone()));

    let request_body = json!({
        "capability_id": "summarize",
        "account_id": Uuid::from_u128(0x11111111111111111111111111111111_u128),
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });

    let (status, created) = send_json(app.clone(), "POST", "/v1/invocations", request_body).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["status"], "Succeeded");
    assert_eq!(created["ledger_reserved"], false);
    assert_eq!(created["ledger_refunded"], false);
    assert_eq!(created["execution"]["dispatch_mode"], "manual");
    assert_eq!(created["execution"]["attempt_count"], 0);
    assert_eq!(created["execution"]["max_attempts"], 1);

    let invocation_id = created["invocation_id"].as_str().expect("invocation id");
    let (get_status, fetched) = get_json(app, &format!("/v1/invocations/{invocation_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Succeeded");
    assert_eq!(fetched["ledger_reserved"], false);
    assert_eq!(fetched["execution"]["dispatch_mode"], "immediate");
    assert_eq!(fetched["execution"]["attempt_count"], 0);
    assert_eq!(fetched["execution"]["max_attempts"], 1);

    let calls = upstream.calls().await;
    assert!(calls.contains(&"capability.get".to_string()));
    assert!(calls.contains(&"execution.create".to_string()));
    assert!(calls.contains(&"execution.start".to_string()));
    assert!(calls.contains(&"execution.get".to_string()));
}

#[tokio::test]
async fn create_invocation_queued_worker_dispatch_does_not_auto_start_execution() {
    let mut config = MockConfig::queued();
    config.capability_body = json!({
        "capability_id": "summarize",
        "kind": "model",
        "provider": "openai",
        "provider_ref": "gpt-4.1-mini",
        "display_name": "Summarize",
        "version": "v1",
        "description": "mock capability",
        "enabled": true
    });
    config.execution_body = json!({
        "execution_id": Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa_u128),
        "status": "Queued",
        "approval_required": false,
        "policy_reason": null,
        "dispatch_mode": "queued_worker",
        "attempt_count": 0,
        "max_attempts": 3
    });

    let upstream = start_mock_server(config).await;
    let app = build_router(legacy_test_state(upstream.base_url.clone()));

    let request_body = json!({
        "capability_id": "summarize",
        "account_id": Uuid::from_u128(0x11111111111111111111111111111111_u128),
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });

    let (status, created) = send_json(app.clone(), "POST", "/v1/invocations", request_body).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["status"], "Queued");
    assert_eq!(created["ledger_reserved"], true);
    assert_eq!(created["ledger_refunded"], false);
    assert_eq!(created["execution"]["dispatch_mode"], "queued_worker");
    assert_eq!(created["execution"]["attempt_count"], 0);
    assert_eq!(created["execution"]["max_attempts"], 3);
    assert_eq!(created["execution"]["attempts_remaining"], 3);

    let invocation_id = created["invocation_id"].as_str().expect("invocation id");
    let (get_status, fetched) = get_json(app, &format!("/v1/invocations/{invocation_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Queued");
    assert_eq!(fetched["ledger_reserved"], true);
    assert_eq!(fetched["execution"]["dispatch_mode"], "queued_worker");
    assert_eq!(fetched["execution"]["attempt_count"], 0);
    assert_eq!(fetched["execution"]["max_attempts"], 3);
    assert_eq!(fetched["execution"]["attempts_remaining"], 3);

    let calls = upstream.calls().await;
    assert!(calls.contains(&"capability.get".to_string()));
    assert!(calls.contains(&"execution.create".to_string()));
    assert!(calls.contains(&"execution.get".to_string()));
    assert!(!calls.contains(&"execution.start".to_string()));
}

#[tokio::test]
async fn create_invocation_auto_start_failure_returns_refunded_when_provider_execution_refunds() {
    let mut config = MockConfig::queued();
    config.capability_body = json!({
        "capability_id": "summarize",
        "kind": "model",
        "provider": "ollama",
        "provider_ref": "qwen2.5:3b",
        "display_name": "Summarize",
        "version": "v1",
        "description": "mock capability",
        "enabled": true
    });
    config.execution_body = json!({
        "execution_id": Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa_u128),
        "status": "Queued",
        "approval_required": false,
        "policy_reason": null,
        "dispatch_mode": "immediate"
    });
    config.execution_start_body = json!({
        "execution_id": Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa_u128),
        "status": "Refunded",
        "approval_required": false,
        "policy_reason": null,
        "dispatch_mode": "immediate",
        "attempt_count": 0,
        "max_attempts": 1
    });

    let upstream = start_mock_server(config).await;
    let app = build_router(legacy_test_state(upstream.base_url.clone()));

    let request_body = json!({
        "capability_id": "summarize",
        "account_id": Uuid::from_u128(0x11111111111111111111111111111111_u128),
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });

    let (status, created) = send_json(app.clone(), "POST", "/v1/invocations", request_body).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(created["status"], "Refunded");
    assert_eq!(created["ledger_reserved"], false);
    assert_eq!(created["ledger_refunded"], true);
    assert_eq!(created["execution"]["dispatch_mode"], "immediate");

    let invocation_id = created["invocation_id"].as_str().expect("invocation id");
    let (get_status, fetched) = get_json(app, &format!("/v1/invocations/{invocation_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Refunded");
    assert_eq!(fetched["ledger_refunded"], true);
    assert_eq!(fetched["execution"]["dispatch_mode"], "immediate");

    let calls = upstream.calls().await;
    assert!(calls.contains(&"capability.get".to_string()));
    assert!(calls.contains(&"execution.create".to_string()));
    assert!(calls.contains(&"execution.start".to_string()));
    assert!(calls.contains(&"execution.get".to_string()));
}

#[tokio::test]
async fn create_invocation_returns_accepted_when_execution_needs_approval() {
    let upstream = start_mock_server(MockConfig::awaiting_approval()).await;
    let app = build_router(legacy_test_state(upstream.base_url.clone()));

    let request_body = json!({
        "capability_id": "publish",
        "account_id": Uuid::from_u128(0x22222222222222222222222222222222_u128),
        "prompt": "publish deploy package",
        "reserve_amount": 25.0
    });

    let (status, created) = send_json(app, "POST", "/v1/invocations", request_body).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(created["status"], "AwaitingApproval");
    assert_eq!(created["approval_required"], true);
    assert!(created["policy_reason"]
        .as_str()
        .expect("policy reason")
        .contains("publish"));

    let calls = upstream.calls().await;
    assert!(calls.contains(&"capability.get".to_string()));
    assert!(calls.contains(&"ledger.get_account".to_string()));
    assert!(calls.contains(&"ledger.reserve".to_string()));
    assert!(calls.contains(&"execution.create".to_string()));
    assert!(!calls.contains(&"ledger.refund".to_string()));
}

#[tokio::test]
async fn reserve_failure_blocks_execution_creation() {
    let upstream = start_mock_server(MockConfig::reserve_failure()).await;
    let app = build_router(legacy_test_state(upstream.base_url.clone()));

    let request_body = json!({
        "capability_id": "summarize",
        "account_id": Uuid::from_u128(0x33333333333333333333333333333333_u128),
        "prompt": "summarize logs",
        "reserve_amount": 7.0
    });

    let (status, created) = send_json(app, "POST", "/v1/invocations", request_body).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(created["status"], "Failed");
    assert_eq!(created["ledger_reserved"], false);
    assert!(created["failure_reason"]
        .as_str()
        .expect("failure reason")
        .contains("ledger reserve failed"));

    let calls = upstream.calls().await;
    assert!(calls.contains(&"capability.get".to_string()));
    assert!(calls.contains(&"ledger.get_account".to_string()));
    assert!(calls.contains(&"ledger.reserve".to_string()));
    assert!(!calls.contains(&"execution.create".to_string()));
}

#[tokio::test]
async fn execution_failure_triggers_refund_and_persists_refunded_status() {
    let upstream = start_mock_server(MockConfig::execution_failure_with_refund()).await;
    let app = build_router(legacy_test_state(upstream.base_url.clone()));

    let request_body = json!({
        "capability_id": "summarize",
        "account_id": Uuid::from_u128(0x44444444444444444444444444444444_u128),
        "prompt": "summarize logs",
        "reserve_amount": 9.0
    });

    let (status, created) = send_json(app.clone(), "POST", "/v1/invocations", request_body).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(created["status"], "Refunded");
    assert_eq!(created["ledger_reserved"], true);
    assert_eq!(created["ledger_refunded"], true);
    assert!(created["failure_reason"]
        .as_str()
        .expect("failure reason")
        .contains("reserve refunded"));

    let invocation_id = created["invocation_id"].as_str().expect("invocation id");
    let (get_status, fetched) = get_json(app, &format!("/v1/invocations/{invocation_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Refunded");
    assert_eq!(fetched["ledger_refunded"], true);

    let calls = upstream.calls().await;
    assert!(calls.contains(&"capability.get".to_string()));
    assert!(calls.contains(&"ledger.get_account".to_string()));
    assert!(calls.contains(&"ledger.reserve".to_string()));
    assert!(calls.contains(&"execution.create".to_string()));
    assert!(calls.contains(&"ledger.refund".to_string()));
}

#[tokio::test]
async fn account_org_mismatch_blocks_invocation_before_reserve() {
    let mut config = MockConfig::queued();
    config.account_body = json!({
        "account_id": Uuid::from_u128(0x55555555555555555555555555555555_u128),
        "org_id": "00000000-0000-0000-0000-00000000ce99",
        "account_type": "org_wallet",
        "currency_unit": "credit",
        "balance": 100.0,
        "reserved": 0.0
    });

    let upstream = start_mock_server(config).await;
    let app = build_router(legacy_test_state(upstream.base_url.clone()));

    let request_body = json!({
        "capability_id": "summarize",
        "account_id": Uuid::from_u128(0x55555555555555555555555555555555_u128),
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });

    let (status, created) = send_json(app, "POST", "/v1/invocations", request_body).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(created["status"], "Failed");
    assert_eq!(created["ledger_reserved"], false);
    assert!(created["failure_reason"]
        .as_str()
        .expect("failure reason")
        .contains("account org mismatch"));

    let calls = upstream.calls().await;
    assert!(calls.contains(&"capability.get".to_string()));
    assert!(calls.contains(&"ledger.get_account".to_string()));
    assert!(!calls.contains(&"ledger.reserve".to_string()));
    assert!(!calls.contains(&"execution.create".to_string()));
}
