use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Utc};
use execution_service::{build_router, state::AppState};
use serde_json::{json, Value};
use shared_types::{ExecutionDispatchMode, ExecutionStatus};
use std::{
    env, fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{net::TcpListener, task::JoinHandle};
use tower::util::ServiceExt;

fn test_state() -> AppState {
    AppState::new_for_tests(
        false,
        Some("local-dev-admin-token".to_string()),
        vec![
            "executions:manage".to_string(),
            "executions:read".to_string(),
        ],
        Vec::new(),
    )
}

struct MockProviderServer {
    base_url: String,
    _handle: JoinHandle<()>,
}

struct MockOpenClawCliScript {
    command: String,
    capture_path: PathBuf,
}

async fn mock_ollama_generate(Json(_body): Json<Value>) -> Json<Value> {
    Json(json!({
        "model": "test-model",
        "response": "hello from ollama",
        "done": true,
        "total_duration": 123,
        "eval_count": 7,
        "prompt_eval_count": 3
    }))
}

async fn start_mock_provider_server() -> MockProviderServer {
    let app = Router::new().route("/api/generate", post(mock_ollama_generate));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock provider");
    let addr = listener.local_addr().expect("provider local addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("run mock provider");
    });

    MockProviderServer {
        base_url: format!("http://{addr}"),
        _handle: handle,
    }
}

fn create_mock_openclaw_cli_script() -> MockOpenClawCliScript {
    let base = env::temp_dir().join(format!(
        "cex-openclaw-mock-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&base).expect("create mock openclaw dir");

    #[cfg(windows)]
    let script_path = base.join("openclaw.cmd");
    #[cfg(not(windows))]
    let script_path = base.join("openclaw.sh");

    let capture_path = base.join("env-capture.json");
    let capture_path_string = path_to_command_string(capture_path.clone());

    #[cfg(windows)]
    let script_body = format!(
        r#"@echo off
set CAPTURE_PATH={capture_path}
if "%1"=="infer" goto ok
exit /b 1
:ok
> "%CAPTURE_PATH%" (
  echo {{"OPENCLAW_CONFIG_PATH":"%OPENCLAW_CONFIG_PATH%","OPENCLAW_STATE_DIR":"%OPENCLAW_STATE_DIR%","OPENCLAW_AGENT_DIR":"%OPENCLAW_AGENT_DIR%"}}
)
echo {{"ok":true,"capability":"model.run","transport":"local","provider":"openai-codex","model":"gpt-5.4","attempts":[],"outputs":[{{"text":"hello from openclaw bridge","mediaUrl":null}}]}}
"#,
        capture_path = capture_path_string
    );

    #[cfg(not(windows))]
    let script_body = format!(
        r#"#!/usr/bin/env sh
printf '%s\n' "{{\"OPENCLAW_CONFIG_PATH\":\"${{OPENCLAW_CONFIG_PATH:-}}\",\"OPENCLAW_STATE_DIR\":\"${{OPENCLAW_STATE_DIR:-}}\",\"OPENCLAW_AGENT_DIR\":\"${{OPENCLAW_AGENT_DIR:-}}\"}}" > '{capture_path}'
printf '%s\n' '{{"ok":true,"capability":"model.run","transport":"local","provider":"openai-codex","model":"gpt-5.4","attempts":[],"outputs":[{{"text":"hello from openclaw bridge","mediaUrl":null}}]}}'
"#,
        capture_path = capture_path_string
    );

    fs::write(&script_path, script_body).expect("write mock openclaw script");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path)
            .expect("mock openclaw metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod mock openclaw script");
    }

    MockOpenClawCliScript {
        command: path_to_command_string(script_path),
        capture_path,
    }
}

fn path_to_command_string(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}

async fn send_json_with_headers(
    app: axum::Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");

    for (name, value) in headers {
        request = request.header(*name, *value);
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
        .expect("read body bytes");
    let json: Value = serde_json::from_slice(&bytes).expect("decode json body");
    (status, json)
}

async fn send_json(app: axum::Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send_json_with_headers(
        app,
        method,
        uri,
        &[("x-admin-token", "local-dev-admin-token")],
        body,
    )
    .await
}

async fn get_json_with_headers(
    app: axum::Router,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut request = Request::builder().method("GET").uri(uri);

    for (name, value) in headers {
        request = request.header(*name, *value);
    }

    let request = request.body(Body::empty()).expect("build get request");

    let response = app.oneshot(request).await.expect("router response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body bytes");
    let json: Value = serde_json::from_slice(&bytes).expect("decode json body");
    (status, json)
}

async fn get_json(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    get_json_with_headers(app, uri, &[("x-admin-token", "local-dev-admin-token")]).await
}

#[tokio::test]
async fn create_execution_exposes_auto_approved_http_flow() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000101",
        "trace_id": "00000000-0000-0000-0000-000000000201",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });

    let (status, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["status"], "Queued");
    assert_eq!(created["approval_required"], false);
    assert_eq!(created["dispatch_mode"], "manual");

    let execution_id = created["execution_id"].as_str().expect("execution id");
    let (get_status, fetched) = get_json(app, &format!("/v1/executions/{execution_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["execution_id"], created["execution_id"]);
    assert_eq!(fetched["status"], "Queued");
}

#[tokio::test]
async fn lifecycle_http_endpoints_support_replay_and_reject_invalid_transitions() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000102",
        "trace_id": "00000000-0000-0000-0000-000000000202",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "prompt": "normal execution",
        "reserve_amount": 5.0
    });
    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let dispatch_body = json!({ "dispatched_by": "tester", "note": "dispatch" });
    let start_body = json!({ "started_by": "tester", "note": "start" });
    let succeed_body = json!({ "settled_by": "tester", "note": "success" });
    let fail_body = json!({ "failed_by": "tester", "reason": "should conflict" });

    let (dispatch_status_1, dispatch_1) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/dispatch"),
        dispatch_body.clone(),
    )
    .await;
    let (dispatch_status_2, dispatch_2) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/dispatch"),
        dispatch_body,
    )
    .await;
    assert_eq!(dispatch_status_1, StatusCode::OK);
    assert_eq!(dispatch_status_2, StatusCode::OK);
    assert_eq!(dispatch_1["status"], "Dispatching");
    assert_eq!(dispatch_2["status"], "Dispatching");

    let (start_status_1, start_1) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/start"),
        start_body.clone(),
    )
    .await;
    let (start_status_2, start_2) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/start"),
        start_body,
    )
    .await;
    assert_eq!(start_status_1, StatusCode::OK);
    assert_eq!(start_status_2, StatusCode::OK);
    assert_eq!(start_1["status"], "Running");
    assert_eq!(start_2["status"], "Running");

    let (succeed_status_1, succeed_1) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/succeed"),
        succeed_body.clone(),
    )
    .await;
    let (succeed_status_2, succeed_2) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/succeed"),
        succeed_body,
    )
    .await;
    let (fail_status, fail_body_json) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/fail"),
        fail_body,
    )
    .await;
    assert_eq!(succeed_status_1, StatusCode::OK);
    assert_eq!(succeed_status_2, StatusCode::OK);
    assert_eq!(succeed_1["status"], "Succeeded");
    assert_eq!(succeed_2["status"], "Succeeded");
    assert_eq!(fail_status, StatusCode::CONFLICT);
    assert_eq!(fail_body_json["status"], "Succeeded");
}

#[tokio::test]
async fn create_execution_ollama_provider_sets_immediate_dispatch_mode() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000105",
        "trace_id": "00000000-0000-0000-0000-000000000205",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.ollama.test",
        "capability_provider": "ollama",
        "capability_provider_ref": "qwen-test",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });

    let (status, created) = send_json(app, "POST", "/v1/executions", create_body).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["status"], "Queued");
    assert_eq!(created["dispatch_mode"], "immediate");
    assert_eq!(created["provider_target"], "ollama://qwen-test");
}

#[tokio::test]
async fn create_execution_uses_configured_max_attempts_by_dispatch_mode() {
    let app = build_router(AppState::new_for_tests_with_attempt_limits(
        false,
        Some("local-dev-admin-token".to_string()),
        vec![
            "executions:manage".to_string(),
            "executions:read".to_string(),
        ],
        Vec::new(),
        2,
        7,
    ));

    let manual_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000106",
        "trace_id": "00000000-0000-0000-0000-000000000206",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "prompt": "manual attempt budget",
        "reserve_amount": 1.0
    });
    let queued_worker_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000107",
        "trace_id": "00000000-0000-0000-0000-000000000207",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "queued worker attempt budget",
        "reserve_amount": 1.0
    });

    let (manual_status, manual_created) =
        send_json(app.clone(), "POST", "/v1/executions", manual_body).await;
    assert_eq!(manual_status, StatusCode::CREATED);
    assert_eq!(manual_created["dispatch_mode"], "manual");
    assert_eq!(manual_created["max_attempts"], 2);

    let (queued_status, queued_created) =
        send_json(app, "POST", "/v1/executions", queued_worker_body).await;
    assert_eq!(queued_status, StatusCode::CREATED);
    assert_eq!(queued_created["dispatch_mode"], "queued_worker");
    assert_eq!(queued_created["max_attempts"], 7);
}

#[tokio::test]
async fn create_execution_unsupported_provider_sets_queued_worker_dispatch_mode() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000107",
        "trace_id": "00000000-0000-0000-0000-000000000207",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });

    let (status, created) = send_json(app, "POST", "/v1/executions", create_body).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["status"], "Queued");
    assert_eq!(created["dispatch_mode"], "queued_worker");
    assert_eq!(created["provider_target"], "openai://gpt-4.1-mini");
    assert_eq!(created["attempt_count"], 0);
    assert_eq!(created["max_attempts"], 3);
}

#[tokio::test]
async fn requeue_execution_rejects_exhausted_retry_budget() {
    let state = test_state();
    let app = build_router(state.clone());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000150",
        "trace_id": "00000000-0000-0000-0000-000000000250",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "budget exhausted requeue",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"]
        .as_str()
        .expect("execution id")
        .parse()
        .expect("uuid");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    {
        let mut map = state.executions.write().await;
        let record = map.get_mut(&execution_id).expect("execution record");
        record.max_attempts = 1;
    }

    let (requeue_status, body) = send_json(
        app,
        "POST",
        &format!("/v1/executions/{execution_id}/requeue"),
        json!({ "worker_id": "worker-1", "note": "try requeue" }),
    )
    .await;
    assert_eq!(requeue_status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "execution retry budget exhausted");
    assert_eq!(body["status"], "Dispatching");
}

#[tokio::test]
async fn reclaim_expired_executions_skips_budget_exhausted_leases() {
    let state = test_state();
    let app = build_router(state.clone());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000151",
        "trace_id": "00000000-0000-0000-0000-000000000251",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "budget exhausted reclaim",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"]
        .as_str()
        .expect("execution id")
        .parse()
        .expect("uuid");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    {
        let mut map = state.executions.write().await;
        let record = map.get_mut(&execution_id).expect("execution record");
        record.max_attempts = 1;
        record.lease_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
    }

    let (reclaim_status, body) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/reclaim-expired",
        json!({ "reclaimed_by": "worker-sweeper", "limit": 10, "note": "skip exhausted" }),
    )
    .await;
    assert_eq!(reclaim_status, StatusCode::OK);
    let items = body["items"].as_array().expect("reclaim items");
    assert!(items.is_empty());

    let (get_status, fetched) = get_json(
        app,
        &format!(
            "/v1/executions/{}",
            created["execution_id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Dispatching");
    assert_eq!(fetched["attempt_count"], 1);
    assert_eq!(fetched["max_attempts"], 1);
}

#[tokio::test]
async fn claim_execution_batch_can_target_only_expired_leases() {
    let state = test_state();
    let app = build_router(state.clone());

    let expired_a = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000134",
        "trace_id": "00000000-0000-0000-0000-000000000234",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "expired batch a",
        "reserve_amount": 1.0
    });
    let expired_b = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000135",
        "trace_id": "00000000-0000-0000-0000-000000000235",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "expired batch b",
        "reserve_amount": 1.0
    });
    let queued_c = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000136",
        "trace_id": "00000000-0000-0000-0000-000000000236",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "still queued",
        "reserve_amount": 1.0
    });

    let (_, created_a) = send_json(app.clone(), "POST", "/v1/executions", expired_a).await;
    let (_, created_b) = send_json(app.clone(), "POST", "/v1/executions", expired_b).await;
    let (_, created_c) = send_json(app.clone(), "POST", "/v1/executions", queued_c).await;

    let (initial_claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-batch",
        json!({ "claimed_by": "worker-1", "limit": 2, "note": "initial claim" }),
    )
    .await;
    assert_eq!(initial_claim_status, StatusCode::OK);

    {
        let mut map = state.executions.write().await;
        for execution_id in [
            created_a["execution_id"].as_str().unwrap(),
            created_b["execution_id"].as_str().unwrap(),
        ] {
            let record = map
                .values_mut()
                .find(|record| record.execution_id.to_string() == execution_id)
                .expect("claimed execution record");
            record.lease_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
        }
    }

    let (status, body) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-batch",
        json!({
            "claimed_by": "worker-2",
            "limit": 10,
            "lease_expired_only": true,
            "note": "reclaim expired only"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().expect("claim batch items");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["execution_id"], created_a["execution_id"]);
    assert_eq!(items[1]["execution_id"], created_b["execution_id"]);
    assert!(items.iter().all(|item| item["worker_id"] == "worker-2"));

    let (queue_status, queue) = get_json(app, "/v1/executions/worker-queue").await;
    assert_eq!(queue_status, StatusCode::OK);
    let queue_items = queue.as_array().expect("worker queue array");
    let queued_item = queue_items
        .iter()
        .find(|item| item["execution_id"] == created_c["execution_id"])
        .expect("queued execution still present");
    assert_eq!(queued_item["claimable"], true);
}

#[tokio::test]
async fn claim_execution_batch_claims_oldest_visible_queued_worker_items_up_to_limit() {
    let app = build_router(test_state());

    let worker_a = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000122",
        "trace_id": "00000000-0000-0000-0000-000000000222",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "batch a",
        "reserve_amount": 1.0
    });
    let worker_b = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000123",
        "trace_id": "00000000-0000-0000-0000-000000000223",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "batch b",
        "reserve_amount": 1.0
    });
    let worker_c = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000124",
        "trace_id": "00000000-0000-0000-0000-000000000224",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "batch c",
        "reserve_amount": 1.0
    });

    let (_, created_a) = send_json(app.clone(), "POST", "/v1/executions", worker_a).await;
    let (_, created_b) = send_json(app.clone(), "POST", "/v1/executions", worker_b).await;
    let (_, created_c) = send_json(app.clone(), "POST", "/v1/executions", worker_c).await;

    let (status, body) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-batch",
        json!({ "claimed_by": "worker-batch", "limit": 2, "note": "claim batch" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().expect("claim batch items");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["execution_id"], created_a["execution_id"]);
    assert_eq!(items[1]["execution_id"], created_b["execution_id"]);
    assert_eq!(items[0]["worker_id"], "worker-batch");
    assert_eq!(items[1]["worker_id"], "worker-batch");

    let (queue_status, queue) = get_json(app, "/v1/executions/worker-queue").await;
    assert_eq!(queue_status, StatusCode::OK);
    let queue_items = queue.as_array().expect("worker queue array");
    assert_eq!(queue_items.len(), 3);
    assert_eq!(queue_items[0]["execution_id"], created_a["execution_id"]);
    assert_eq!(queue_items[0]["claimable"], false);
    assert_eq!(queue_items[1]["execution_id"], created_b["execution_id"]);
    assert_eq!(queue_items[1]["claimable"], false);
    assert_eq!(queue_items[2]["execution_id"], created_c["execution_id"]);
    assert_eq!(queue_items[2]["claimable"], true);
}

#[tokio::test]
async fn worker_queue_summary_reports_visible_queue_counts() {
    let state = test_state();
    let app = build_router(state.clone());

    let queued_worker_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000130",
        "trace_id": "00000000-0000-0000-0000-000000000230",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "summary a",
        "reserve_amount": 1.0
    });
    let queued_worker_body_2 = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000131",
        "trace_id": "00000000-0000-0000-0000-000000000231",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "summary b",
        "reserve_amount": 1.0
    });
    let queued_worker_body_3 = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000132",
        "trace_id": "00000000-0000-0000-0000-000000000232",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "summary c",
        "reserve_amount": 1.0
    });
    let manual_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000133",
        "trace_id": "00000000-0000-0000-0000-000000000233",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "prompt": "manual",
        "reserve_amount": 5.0
    });

    let (_, created_1) = send_json(app.clone(), "POST", "/v1/executions", queued_worker_body).await;
    let (_, created_2) =
        send_json(app.clone(), "POST", "/v1/executions", queued_worker_body_2).await;
    let _ = send_json(app.clone(), "POST", "/v1/executions", queued_worker_body_3).await;
    let _ = send_json(app.clone(), "POST", "/v1/executions", manual_body).await;

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-batch",
        json!({ "claimed_by": "worker-1", "limit": 2, "note": "claim batch" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    {
        let mut map = state.executions.write().await;
        let record_1 = map
            .values_mut()
            .find(|record| {
                record.execution_id.to_string() == created_1["execution_id"].as_str().unwrap()
            })
            .expect("first execution record");
        record_1.lease_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));

        let record_2 = map
            .values_mut()
            .find(|record| {
                record.execution_id.to_string() == created_2["execution_id"].as_str().unwrap()
            })
            .expect("second execution record");
        record_2.lease_expires_at = Some(Utc::now() + chrono::Duration::seconds(300));
    }

    let (status, summary) = get_json(app, "/v1/executions/worker-queue/summary").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(summary["total"], 3);
    assert_eq!(summary["queued"], 1);
    assert_eq!(summary["dispatching"], 2);
    assert_eq!(summary["claimable"], 2);
    assert_eq!(summary["lease_expired"], 1);
    assert_eq!(summary["claimed_active"], 1);
    assert_eq!(summary["retryable"], 3);
    assert_eq!(summary["retry_budget_exhausted"], 0);
    assert_eq!(summary["active_workers"], 1);
}

#[tokio::test]
async fn worker_queue_supports_lease_expired_only_filter() {
    let state = test_state();
    let app = build_router(state.clone());

    let queued_worker_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000128",
        "trace_id": "00000000-0000-0000-0000-000000000228",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "expired a",
        "reserve_amount": 1.0
    });
    let queued_worker_body_2 = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000129",
        "trace_id": "00000000-0000-0000-0000-000000000229",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "expired b",
        "reserve_amount": 1.0
    });

    let (_, created_1) = send_json(app.clone(), "POST", "/v1/executions", queued_worker_body).await;
    let (_, created_2) =
        send_json(app.clone(), "POST", "/v1/executions", queued_worker_body_2).await;

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-batch",
        json!({ "claimed_by": "worker-1", "limit": 2, "note": "claim batch" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    {
        let mut map = state.executions.write().await;
        let record_1 = map
            .values_mut()
            .find(|record| {
                record.execution_id.to_string() == created_1["execution_id"].as_str().unwrap()
            })
            .expect("first execution record");
        record_1.lease_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));

        let record_2 = map
            .values_mut()
            .find(|record| {
                record.execution_id.to_string() == created_2["execution_id"].as_str().unwrap()
            })
            .expect("second execution record");
        record_2.lease_expires_at = Some(Utc::now() + chrono::Duration::seconds(300));
    }

    let (status, queue) =
        get_json(app, "/v1/executions/worker-queue?lease_expired_only=true").await;
    assert_eq!(status, StatusCode::OK);
    let items = queue.as_array().expect("worker queue array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["execution_id"], created_1["execution_id"]);
    assert_eq!(items[0]["claimable"], true);
}

#[tokio::test]
async fn worker_queue_supports_limit_and_claimable_only_filters() {
    let state = test_state();
    let app = build_router(state.clone());

    let queued_worker_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000125",
        "trace_id": "00000000-0000-0000-0000-000000000225",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "limit a",
        "reserve_amount": 1.0
    });
    let queued_worker_body_2 = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000126",
        "trace_id": "00000000-0000-0000-0000-000000000226",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "limit b",
        "reserve_amount": 1.0
    });
    let queued_worker_body_3 = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000127",
        "trace_id": "00000000-0000-0000-0000-000000000227",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "limit c",
        "reserve_amount": 1.0
    });

    let _ = send_json(app.clone(), "POST", "/v1/executions", queued_worker_body).await;
    let _ = send_json(app.clone(), "POST", "/v1/executions", queued_worker_body_2).await;
    let (_, created_3) =
        send_json(app.clone(), "POST", "/v1/executions", queued_worker_body_3).await;

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-batch",
        json!({ "claimed_by": "worker-1", "limit": 2, "note": "claim batch" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    let (status, queue) = get_json(
        app.clone(),
        "/v1/executions/worker-queue?claimable_only=true&limit=1",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = queue.as_array().expect("worker queue array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["execution_id"], created_3["execution_id"]);
    assert_eq!(items[0]["claimable"], true);

    let (worker_status, worker_queue) =
        get_json(app, "/v1/executions/worker-queue?worker_id=worker-1").await;
    assert_eq!(worker_status, StatusCode::OK);
    let worker_items = worker_queue.as_array().expect("worker queue array");
    assert_eq!(worker_items.len(), 2);
    assert!(worker_items
        .iter()
        .all(|item| item["worker_id"] == "worker-1"));
    assert!(worker_items.iter().all(|item| item["claimable"] == false));
}

#[tokio::test]
async fn worker_queue_lists_queued_worker_items_with_claimable_flags() {
    let state = test_state();
    let app = build_router(state.clone());

    let queued_worker_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000117",
        "trace_id": "00000000-0000-0000-0000-000000000217",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });
    let queued_worker_body_2 = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000118",
        "trace_id": "00000000-0000-0000-0000-000000000218",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "say hi again",
        "reserve_amount": 1.0
    });
    let manual_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000119",
        "trace_id": "00000000-0000-0000-0000-000000000219",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });

    let (_, first_created) =
        send_json(app.clone(), "POST", "/v1/executions", queued_worker_body).await;
    let first_execution_id = first_created["execution_id"]
        .as_str()
        .expect("execution id");
    let _ = send_json(app.clone(), "POST", "/v1/executions", queued_worker_body_2).await;
    let _ = send_json(app.clone(), "POST", "/v1/executions", manual_body).await;

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    let (status, queue) = get_json(app, "/v1/executions/worker-queue").await;
    assert_eq!(status, StatusCode::OK);
    let items = queue.as_array().expect("worker queue array");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["execution_id"], first_execution_id);
    assert_eq!(items[0]["status"], "Dispatching");
    assert_eq!(items[0]["claimable"], false);
    assert_eq!(items[1]["status"], "Queued");
    assert_eq!(items[1]["claimable"], true);
}

#[tokio::test]
async fn worker_queue_filters_other_orgs_for_scoped_admin() {
    let app = build_router(AppState::new_for_tests(
        false,
        Some("exec-admin".to_string()),
        vec![
            "executions:manage".to_string(),
            "executions:read".to_string(),
        ],
        vec!["00000000-0000-0000-0000-00000000ce02".to_string()],
    ));

    let _ = send_json_with_headers(
        app.clone(),
        "POST",
        "/v1/executions",
        &[("x-admin-token", "exec-admin")],
        json!({
            "invocation_id": "00000000-0000-0000-0000-000000000120",
            "trace_id": "00000000-0000-0000-0000-000000000220",
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "capability_id": "cap.openai.test",
            "capability_provider": "openai",
            "capability_provider_ref": "gpt-4.1-mini",
            "prompt": "say hi",
            "reserve_amount": 1.0
        }),
    )
    .await;
    let _ = send_json_with_headers(
        app.clone(),
        "POST",
        "/v1/executions",
        &[("x-admin-token", "exec-admin")],
        json!({
            "invocation_id": "00000000-0000-0000-0000-000000000121",
            "trace_id": "00000000-0000-0000-0000-000000000221",
            "org_id": "00000000-0000-0000-0000-00000000ce02",
            "capability_id": "cap.openai.test",
            "capability_provider": "openai",
            "capability_provider_ref": "gpt-4.1-mini",
            "prompt": "say hi 2",
            "reserve_amount": 1.0
        }),
    )
    .await;

    let (status, queue) = get_json_with_headers(
        app,
        "/v1/executions/worker-queue",
        &[("x-admin-token", "exec-admin")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = queue.as_array().expect("worker queue array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["org_id"], "00000000-0000-0000-0000-00000000ce02");
}

#[tokio::test]
async fn claim_next_execution_picks_oldest_queued_worker_and_marks_dispatching() {
    let app = build_router(test_state());

    let manual_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000110",
        "trace_id": "00000000-0000-0000-0000-000000000210",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });
    let queued_worker_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000111",
        "trace_id": "00000000-0000-0000-0000-000000000211",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });

    let _ = send_json(app.clone(), "POST", "/v1/executions", manual_body).await;
    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", queued_worker_body).await;
    let queued_worker_execution_id = created["execution_id"].as_str().expect("execution id");

    let (claim_status, claimed) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);
    assert_eq!(claimed["execution_id"], queued_worker_execution_id);
    assert_eq!(claimed["status"], "Dispatching");
    assert_eq!(claimed["dispatch_mode"], "queued_worker");
    assert_eq!(claimed["worker_id"], "worker-1");
    assert!(claimed["lease_expires_at"].as_str().is_some());

    let (get_status, fetched) =
        get_json(app, &format!("/v1/executions/{queued_worker_execution_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Dispatching");
    assert_eq!(fetched["dispatch_mode"], "queued_worker");
}

#[tokio::test]
async fn claim_next_execution_can_target_only_expired_leases() {
    let state = test_state();
    let app = build_router(state.clone());

    let queued_worker_a = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000137",
        "trace_id": "00000000-0000-0000-0000-000000000237",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "expired next a",
        "reserve_amount": 1.0
    });
    let queued_worker_b = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000138",
        "trace_id": "00000000-0000-0000-0000-000000000238",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "expired next b",
        "reserve_amount": 1.0
    });

    let (_, created_a) = send_json(app.clone(), "POST", "/v1/executions", queued_worker_a).await;
    let (_, created_b) = send_json(app.clone(), "POST", "/v1/executions", queued_worker_b).await;
    let execution_id_a = created_a["execution_id"]
        .as_str()
        .expect("execution id a")
        .parse()
        .expect("uuid a");

    let (initial_claim_status, initial_claimed) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "initial claim" }),
    )
    .await;
    assert_eq!(initial_claim_status, StatusCode::OK);
    assert_eq!(initial_claimed["execution_id"], created_a["execution_id"]);

    {
        let mut map = state.executions.write().await;
        let record = map.get_mut(&execution_id_a).expect("execution record a");
        record.lease_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
    }

    let (reclaim_status, reclaimed) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({
            "claimed_by": "worker-2",
            "lease_expired_only": true,
            "note": "expired only reclaim"
        }),
    )
    .await;
    assert_eq!(reclaim_status, StatusCode::OK);
    assert_eq!(reclaimed["execution_id"], created_a["execution_id"]);
    assert_eq!(reclaimed["worker_id"], "worker-2");

    let (queue_status, queue) = get_json(app, "/v1/executions/worker-queue").await;
    assert_eq!(queue_status, StatusCode::OK);
    let items = queue.as_array().expect("worker queue array");
    let queued_item = items
        .iter()
        .find(|item| item["execution_id"] == created_b["execution_id"])
        .expect("queued execution still present");
    assert_eq!(queued_item["claimable"], true);
}

#[tokio::test]
async fn claim_next_execution_with_expired_only_returns_not_found_without_expired_leases() {
    let app = build_router(test_state());

    let queued_worker = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000139",
        "trace_id": "00000000-0000-0000-0000-000000000239",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "no expired yet",
        "reserve_amount": 1.0
    });

    let _ = send_json(app.clone(), "POST", "/v1/executions", queued_worker).await;

    let (claim_status, body) = send_json(
        app,
        "POST",
        "/v1/executions/claim-next",
        json!({
            "claimed_by": "worker-1",
            "lease_expired_only": true,
            "note": "should not claim queued"
        }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "no queued worker execution available");
}

#[tokio::test]
async fn reclaim_expired_executions_returns_expired_dispatching_items_to_queue_up_to_limit() {
    let state = test_state();
    let app = build_router(state.clone());

    let body_a = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000142",
        "trace_id": "00000000-0000-0000-0000-000000000242",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "reclaim a",
        "reserve_amount": 1.0
    });
    let body_b = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000143",
        "trace_id": "00000000-0000-0000-0000-000000000243",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "reclaim b",
        "reserve_amount": 1.0
    });
    let body_c = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000144",
        "trace_id": "00000000-0000-0000-0000-000000000244",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "reclaim c",
        "reserve_amount": 1.0
    });

    let (_, created_a) = send_json(app.clone(), "POST", "/v1/executions", body_a).await;
    let (_, created_b) = send_json(app.clone(), "POST", "/v1/executions", body_b).await;
    let (_, created_c) = send_json(app.clone(), "POST", "/v1/executions", body_c).await;

    let execution_id_a = created_a["execution_id"]
        .as_str()
        .expect("execution id a")
        .parse()
        .expect("uuid a");
    let execution_id_b = created_b["execution_id"]
        .as_str()
        .expect("execution id b")
        .parse()
        .expect("uuid b");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-batch",
        json!({ "claimed_by": "worker-1", "limit": 2, "note": "initial batch claim" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    {
        let mut map = state.executions.write().await;
        map.get_mut(&execution_id_a)
            .expect("record a")
            .lease_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
        map.get_mut(&execution_id_b)
            .expect("record b")
            .lease_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
    }

    let (reclaim_status, body) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/reclaim-expired",
        json!({ "reclaimed_by": "worker-sweeper", "limit": 1, "note": "reclaim expired leases" }),
    )
    .await;
    assert_eq!(reclaim_status, StatusCode::OK);
    let items = body["items"].as_array().expect("reclaim items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["execution_id"], created_a["execution_id"]);
    assert_eq!(items[0]["status"], "Queued");
    assert!(items[0]["worker_id"].is_null());
    assert!(items[0]["lease_expires_at"].is_null());

    let (get_status_a, fetched_a) = get_json(
        app.clone(),
        &format!(
            "/v1/executions/{}",
            created_a["execution_id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(get_status_a, StatusCode::OK);
    assert_eq!(fetched_a["status"], "Queued");
    assert!(fetched_a["worker_id"].is_null());

    let (get_status_b, fetched_b) = get_json(
        app.clone(),
        &format!(
            "/v1/executions/{}",
            created_b["execution_id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(get_status_b, StatusCode::OK);
    assert_eq!(fetched_b["status"], "Dispatching");
    assert_eq!(fetched_b["worker_id"], "worker-1");

    let (queue_status, queue) =
        get_json(app, "/v1/executions/worker-queue?claimable_only=true").await;
    assert_eq!(queue_status, StatusCode::OK);
    let queue_items = queue.as_array().expect("worker queue array");
    assert!(queue_items
        .iter()
        .any(|item| item["execution_id"] == created_a["execution_id"]));
    assert!(queue_items
        .iter()
        .any(|item| item["execution_id"] == created_b["execution_id"]));
    assert!(queue_items
        .iter()
        .any(|item| item["execution_id"] == created_c["execution_id"]));
}

#[tokio::test]
async fn reclaim_expired_executions_returns_empty_when_no_expired_leases_exist() {
    let app = build_router(test_state());

    let queued_worker = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000145",
        "trace_id": "00000000-0000-0000-0000-000000000245",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "nothing expired",
        "reserve_amount": 1.0
    });

    let _ = send_json(app.clone(), "POST", "/v1/executions", queued_worker).await;

    let (reclaim_status, body) = send_json(
        app,
        "POST",
        "/v1/executions/reclaim-expired",
        json!({ "reclaimed_by": "worker-sweeper", "limit": 10, "note": "no expired leases yet" }),
    )
    .await;
    assert_eq!(reclaim_status, StatusCode::OK);
    let items = body["items"].as_array().expect("reclaim items");
    assert!(items.is_empty());
}

#[tokio::test]
async fn timeout_expired_executions_times_out_expired_dispatching_items_up_to_limit() {
    let state = test_state();
    let app = build_router(state.clone());

    let body_a = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000146",
        "trace_id": "00000000-0000-0000-0000-000000000246",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "timeout a",
        "reserve_amount": 1.0
    });
    let body_b = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000147",
        "trace_id": "00000000-0000-0000-0000-000000000247",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "timeout b",
        "reserve_amount": 1.0
    });
    let body_c = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000148",
        "trace_id": "00000000-0000-0000-0000-000000000248",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "timeout c",
        "reserve_amount": 1.0
    });

    let (_, created_a) = send_json(app.clone(), "POST", "/v1/executions", body_a).await;
    let (_, created_b) = send_json(app.clone(), "POST", "/v1/executions", body_b).await;
    let (_, created_c) = send_json(app.clone(), "POST", "/v1/executions", body_c).await;

    let execution_id_a = created_a["execution_id"]
        .as_str()
        .expect("execution id a")
        .parse()
        .expect("uuid a");
    let execution_id_b = created_b["execution_id"]
        .as_str()
        .expect("execution id b")
        .parse()
        .expect("uuid b");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-batch",
        json!({ "claimed_by": "worker-1", "limit": 2, "note": "initial batch claim" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    {
        let mut map = state.executions.write().await;
        map.get_mut(&execution_id_a)
            .expect("record a")
            .lease_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
        map.get_mut(&execution_id_b)
            .expect("record b")
            .lease_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
    }

    let (timeout_status, body) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/timeout-expired",
        json!({
            "timed_out_by": "worker-sweeper",
            "reason": "lease expired sweep",
            "limit": 1
        }),
    )
    .await;
    assert_eq!(timeout_status, StatusCode::OK);
    let items = body["items"].as_array().expect("timeout items");
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0]["record"]["execution_id"],
        created_a["execution_id"]
    );
    assert_eq!(items[0]["record"]["status"], "TimedOut");
    assert_eq!(items[0]["refund_applied"], false);
    assert!(items[0]["record"]["worker_id"].is_null());
    assert!(items[0]["record"]["lease_expires_at"].is_null());

    let (get_status_a, fetched_a) = get_json(
        app.clone(),
        &format!(
            "/v1/executions/{}",
            created_a["execution_id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(get_status_a, StatusCode::OK);
    assert_eq!(fetched_a["status"], "TimedOut");

    let (get_status_b, fetched_b) = get_json(
        app.clone(),
        &format!(
            "/v1/executions/{}",
            created_b["execution_id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(get_status_b, StatusCode::OK);
    assert_eq!(fetched_b["status"], "Dispatching");
    assert_eq!(fetched_b["worker_id"], "worker-1");

    let (queue_status, queue) =
        get_json(app, "/v1/executions/worker-queue?claimable_only=true").await;
    assert_eq!(queue_status, StatusCode::OK);
    let queue_items = queue.as_array().expect("worker queue array");
    assert!(queue_items
        .iter()
        .any(|item| item["execution_id"] == created_b["execution_id"]));
    assert!(queue_items
        .iter()
        .any(|item| item["execution_id"] == created_c["execution_id"]));
    assert!(!queue_items
        .iter()
        .any(|item| item["execution_id"] == created_a["execution_id"]));
}

#[tokio::test]
async fn timeout_expired_executions_returns_empty_when_no_expired_leases_exist() {
    let app = build_router(test_state());

    let queued_worker = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000149",
        "trace_id": "00000000-0000-0000-0000-000000000249",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "no timeout candidates",
        "reserve_amount": 1.0
    });

    let _ = send_json(app.clone(), "POST", "/v1/executions", queued_worker).await;

    let (timeout_status, body) = send_json(
        app,
        "POST",
        "/v1/executions/timeout-expired",
        json!({
            "timed_out_by": "worker-sweeper",
            "reason": "nothing to timeout",
            "limit": 10
        }),
    )
    .await;
    assert_eq!(timeout_status, StatusCode::OK);
    let items = body["items"].as_array().expect("timeout items");
    assert!(items.is_empty());
}

#[tokio::test]
async fn retry_execution_returns_failed_queued_worker_to_queue_when_budget_remains() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000152",
        "trace_id": "00000000-0000-0000-0000-000000000252",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "retry me",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    let (process_status, processed) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/process"),
        json!({ "processed_by": "worker-1", "note": "first attempt fails" }),
    )
    .await;
    assert_eq!(process_status, StatusCode::OK);
    assert_eq!(processed["status"], "Failed");
    assert_eq!(processed["attempt_count"], 1);
    assert_eq!(processed["max_attempts"], 3);
    assert!(processed["result_payload"].is_object());

    let (retry_status, retried) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/retry"),
        json!({ "retried_by": "worker-ops", "note": "retry after transient failure" }),
    )
    .await;
    assert_eq!(retry_status, StatusCode::OK);
    assert_eq!(retried["status"], "Queued");
    assert_eq!(retried["dispatch_mode"], "queued_worker");
    assert_eq!(retried["attempt_count"], 1);
    assert_eq!(retried["max_attempts"], 3);
    assert!(retried["worker_id"].is_null());
    assert!(retried["lease_expires_at"].is_null());
    assert!(retried["started_at"].is_null());
    assert!(retried["ended_at"].is_null());
    assert!(retried["result_payload"].is_null());

    let (queue_status, queue) = get_json(
        app.clone(),
        "/v1/executions/worker-queue?claimable_only=true",
    )
    .await;
    assert_eq!(queue_status, StatusCode::OK);
    let items = queue.as_array().expect("worker queue array");
    let item = items
        .iter()
        .find(|item| item["execution_id"] == created["execution_id"])
        .expect("retried execution visible in queue");
    assert_eq!(item["attempt_count"], 1);
    assert_eq!(item["attempts_remaining"], 2);
    assert_eq!(item["retry_budget_exhausted"], false);

    let (get_status, fetched) = get_json(app, &format!("/v1/executions/{execution_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Queued");
    assert_eq!(fetched["attempt_count"], 1);
    assert!(fetched["result_payload"].is_null());
}

#[tokio::test]
async fn retry_execution_rejects_exhausted_retry_budget() {
    let state = test_state();
    let app = build_router(state.clone());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000153",
        "trace_id": "00000000-0000-0000-0000-000000000253",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "retry exhausted",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"]
        .as_str()
        .expect("execution id")
        .parse()
        .expect("uuid");

    {
        let mut map = state.executions.write().await;
        let record = map.get_mut(&execution_id).expect("execution record");
        record.status = ExecutionStatus::Failed;
        record.attempt_count = 3;
        record.max_attempts = 3;
        record.dispatch_mode = ExecutionDispatchMode::Manual;
        record.result_payload = Some(json!({ "error": "budget exhausted" }));
    }

    let (retry_status, body) = send_json(
        app,
        "POST",
        &format!(
            "/v1/executions/{}/retry",
            created["execution_id"].as_str().unwrap()
        ),
        json!({ "retried_by": "worker-ops", "note": "should fail" }),
    )
    .await;
    assert_eq!(retry_status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "execution retry budget exhausted");
    assert_eq!(body["status"], "Failed");
}

#[tokio::test]
async fn requeue_execution_returns_claimed_queued_worker_to_queue() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000140",
        "trace_id": "00000000-0000-0000-0000-000000000240",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "requeue me",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    let (requeue_status, requeued) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/requeue"),
        json!({ "worker_id": "worker-1", "note": "transient worker issue" }),
    )
    .await;
    assert_eq!(requeue_status, StatusCode::OK);
    assert_eq!(requeued["status"], "Queued");
    assert_eq!(requeued["dispatch_mode"], "queued_worker");
    assert!(requeued["worker_id"].is_null());
    assert!(requeued["lease_expires_at"].is_null());

    let (get_status, fetched) =
        get_json(app.clone(), &format!("/v1/executions/{execution_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Queued");
    assert_eq!(fetched["dispatch_mode"], "queued_worker");

    let (queue_status, queue) =
        get_json(app, "/v1/executions/worker-queue?claimable_only=true").await;
    assert_eq!(queue_status, StatusCode::OK);
    let items = queue.as_array().expect("worker queue array");
    assert!(items
        .iter()
        .any(|item| item["execution_id"] == created["execution_id"]));
}

#[tokio::test]
async fn requeue_execution_rejects_wrong_worker() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000141",
        "trace_id": "00000000-0000-0000-0000-000000000241",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "wrong worker",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    let (requeue_status, body) = send_json(
        app,
        "POST",
        &format!("/v1/executions/{execution_id}/requeue"),
        json!({ "worker_id": "worker-2", "note": "wrong worker" }),
    )
    .await;
    assert_eq!(requeue_status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "execution claimed by another worker");
    assert_eq!(body["status"], "Dispatching");
}

#[tokio::test]
async fn renew_execution_lease_extends_claim_for_same_worker() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000114",
        "trace_id": "00000000-0000-0000-0000-000000000214",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (claim_status, claimed) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);
    let claimed_lease = claimed["lease_expires_at"].as_str().expect("claimed lease");
    let claimed_lease = DateTime::parse_from_rfc3339(claimed_lease).expect("parse claimed lease");

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let (renew_status, renewed) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/renew-lease"),
        json!({ "worker_id": "worker-1", "note": "heartbeat" }),
    )
    .await;
    assert_eq!(renew_status, StatusCode::OK);
    assert_eq!(renewed["worker_id"], "worker-1");
    let renewed_lease = renewed["lease_expires_at"].as_str().expect("renewed lease");
    let renewed_lease = DateTime::parse_from_rfc3339(renewed_lease).expect("parse renewed lease");
    assert!(renewed_lease > claimed_lease);

    let (get_status, fetched) = get_json(app, &format!("/v1/executions/{execution_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["worker_id"], "worker-1");
}

#[tokio::test]
async fn expired_worker_lease_allows_reclaim_by_another_worker() {
    let state = test_state();
    let app = build_router(state.clone());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000116",
        "trace_id": "00000000-0000-0000-0000-000000000216",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"]
        .as_str()
        .expect("execution id")
        .parse()
        .expect("uuid");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    {
        let mut map = state.executions.write().await;
        let record = map.get_mut(&execution_id).expect("execution record");
        record.lease_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
    }

    let (reclaim_status, reclaimed) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-2", "note": "reclaim expired" }),
    )
    .await;
    assert_eq!(reclaim_status, StatusCode::OK);
    assert_eq!(reclaimed["worker_id"], "worker-2");
    assert_eq!(reclaimed["status"], "Dispatching");
    assert_eq!(reclaimed["dispatch_mode"], "queued_worker");

    let (process_status, body) = send_json(
        app,
        "POST",
        &format!("/v1/executions/{execution_id}/process"),
        json!({ "processed_by": "worker-1", "note": "stale worker" }),
    )
    .await;
    assert_eq!(process_status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "execution claimed by another worker");
    assert_eq!(body["worker_id"], "worker-2");
}

#[tokio::test]
async fn renew_execution_lease_rejects_wrong_worker() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000115",
        "trace_id": "00000000-0000-0000-0000-000000000215",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    let (renew_status, body) = send_json(
        app,
        "POST",
        &format!("/v1/executions/{execution_id}/renew-lease"),
        json!({ "worker_id": "worker-2", "note": "heartbeat" }),
    )
    .await;
    assert_eq!(renew_status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "execution claimed by another worker");
    assert_eq!(body["worker_id"], "worker-1");
}

#[tokio::test]
async fn claim_next_execution_returns_not_found_when_no_queued_worker_exists() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000112",
        "trace_id": "00000000-0000-0000-0000-000000000212",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });
    let _ = send_json(app.clone(), "POST", "/v1/executions", create_body).await;

    let (claim_status, body) = send_json(
        app,
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "no queued worker execution available");
}

#[tokio::test]
async fn process_execution_runs_claimed_queued_worker_and_surfaces_provider_failure() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000108",
        "trace_id": "00000000-0000-0000-0000-000000000208",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    let (process_status, processed) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/process"),
        json!({ "processed_by": "worker-1", "note": "dequeue" }),
    )
    .await;
    assert_eq!(process_status, StatusCode::OK);
    assert_eq!(processed["status"], "Failed");
    assert_eq!(processed["dispatch_mode"], "manual");
    assert!(processed["result_payload"]["error"]
        .as_str()
        .expect("provider error")
        .contains("unsupported provider adapter"));

    let (get_status, fetched) = get_json(app, &format!("/v1/executions/{execution_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Failed");
}

#[tokio::test]
async fn process_execution_requires_active_claim_for_queued_worker() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000118",
        "trace_id": "00000000-0000-0000-0000-000000000218",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (process_status, body) = send_json(
        app,
        "POST",
        &format!("/v1/executions/{execution_id}/process"),
        json!({ "processed_by": "worker-1", "note": "dequeue without claim" }),
    )
    .await;
    assert_eq!(process_status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "execution is not currently claimed");
    assert_eq!(body["status"], "Queued");
}

#[tokio::test]
async fn process_execution_uses_openclaw_cli_bridge_for_queued_worker() {
    let mut state = test_state();
    let mock_cli = create_mock_openclaw_cli_script();
    state.openclaw_cli_bin = mock_cli.command.clone();
    state.openclaw_config_path = Some("/tmp/openclaw-cex/openclaw.json".to_string());
    state.openclaw_state_dir = Some("/tmp/openclaw-cex".to_string());
    state.openclaw_agent_dir = Some("/tmp/openclaw-cex/agents/cex/agent".to_string());
    let app = build_router(state);
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000109",
        "trace_id": "00000000-0000-0000-0000-000000000209",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openclaw.gpt54",
        "capability_provider": "codex",
        "capability_provider_ref": "gpt-5.4",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");
    assert_eq!(created["dispatch_mode"], "queued_worker");
    assert_eq!(created["provider_target"], "codex://gpt-5.4");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-bridge", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    let (process_status, processed) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/process"),
        json!({ "processed_by": "worker-bridge", "note": "dequeue" }),
    )
    .await;
    assert_eq!(process_status, StatusCode::OK);
    assert_eq!(processed["status"], "Succeeded");
    assert_eq!(processed["provider_target"], "openai-codex://gpt-5.4");
    assert_eq!(processed["result_payload"]["bridge"], "openclaw-cli");
    assert_eq!(
        processed["result_payload"]["output_text"],
        "hello from openclaw bridge"
    );

    let captured_env: Value = serde_json::from_slice(
        &fs::read(&mock_cli.capture_path).expect("read openclaw env capture"),
    )
    .expect("decode openclaw env capture");
    assert_eq!(
        captured_env["OPENCLAW_CONFIG_PATH"],
        "/tmp/openclaw-cex/openclaw.json"
    );
    assert_eq!(captured_env["OPENCLAW_STATE_DIR"], "/tmp/openclaw-cex");
    assert_eq!(
        captured_env["OPENCLAW_AGENT_DIR"],
        "/tmp/openclaw-cex/agents/cex/agent"
    );

    let (get_status, fetched) = get_json(app, &format!("/v1/executions/{execution_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Succeeded");
}

#[tokio::test]
async fn process_execution_rejects_wrong_worker_for_claimed_execution() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000113",
        "trace_id": "00000000-0000-0000-0000-000000000213",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.openai.test",
        "capability_provider": "openai",
        "capability_provider_ref": "gpt-4.1-mini",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (claim_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/executions/claim-next",
        json!({ "claimed_by": "worker-1", "note": "claim next" }),
    )
    .await;
    assert_eq!(claim_status, StatusCode::OK);

    let (process_status, body) = send_json(
        app,
        "POST",
        &format!("/v1/executions/{execution_id}/process"),
        json!({ "processed_by": "worker-2", "note": "dequeue" }),
    )
    .await;
    assert_eq!(process_status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "execution claimed by another worker");
    assert_eq!(body["worker_id"], "worker-1");
}

#[tokio::test]
async fn process_execution_rejects_manual_dispatch_mode() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000109",
        "trace_id": "00000000-0000-0000-0000-000000000209",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (process_status, body) = send_json(
        app,
        "POST",
        &format!("/v1/executions/{execution_id}/process"),
        json!({ "processed_by": "worker-1", "note": "dequeue" }),
    )
    .await;
    assert_eq!(process_status, StatusCode::CONFLICT);
    assert_eq!(
        body["error"],
        "execution dispatch mode does not require worker processing"
    );
    assert_eq!(body["dispatch_mode"], "manual");
    assert_eq!(body["status"], "Queued");
}

#[tokio::test]
async fn provider_backed_start_executes_ollama_adapter_and_returns_succeeded() {
    let provider = start_mock_provider_server().await;
    let mut state = test_state();
    state.ollama_base_url = provider.base_url;
    let app = build_router(state);

    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000106",
        "trace_id": "00000000-0000-0000-0000-000000000206",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "capability_id": "cap.ollama.test",
        "capability_provider": "ollama",
        "capability_provider_ref": "qwen-test",
        "prompt": "say hi",
        "reserve_amount": 1.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (start_status, started) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/start"),
        json!({ "started_by": "tester", "note": "run provider" }),
    )
    .await;
    assert_eq!(start_status, StatusCode::OK);
    assert_eq!(started["status"], "Succeeded");
    assert_eq!(started["provider_target"], "ollama://qwen-test");
    assert_eq!(
        started["result_payload"]["output_text"],
        "hello from ollama"
    );

    let (get_status, fetched) = get_json(app, &format!("/v1/executions/{execution_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Succeeded");
    assert_eq!(fetched["provider_target"], "ollama://qwen-test");
    assert_eq!(
        fetched["result_payload"]["output_text"],
        "hello from ollama"
    );
}

#[tokio::test]
async fn get_execution_requires_admin_token() {
    let app = build_router(test_state());

    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000104",
        "trace_id": "00000000-0000-0000-0000-000000000204",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "prompt": "summarize logs",
        "reserve_amount": 5.0
    });

    let (_, created) = send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (status, body) =
        get_json_with_headers(app, &format!("/v1/executions/{execution_id}"), &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "missing admin token");
}

#[tokio::test]
async fn lifecycle_endpoints_reject_admin_token_for_other_org() {
    let app = build_router(AppState::new_for_tests(
        false,
        Some("exec-admin".to_string()),
        vec![
            "executions:manage".to_string(),
            "executions:read".to_string(),
        ],
        vec!["00000000-0000-0000-0000-00000000ce02".to_string()],
    ));

    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000105",
        "trace_id": "00000000-0000-0000-0000-000000000205",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "prompt": "publish deploy package",
        "reserve_amount": 25.0
    });

    let (_, created) = send_json_with_headers(
        app.clone(),
        "POST",
        "/v1/executions",
        &[("x-admin-token", "exec-admin")],
        create_body,
    )
    .await;
    let execution_id = created["execution_id"].as_str().expect("execution id");

    let (status, body) = send_json_with_headers(
        app,
        "POST",
        &format!("/v1/executions/{execution_id}/approve"),
        &[("x-admin-token", "exec-admin")],
        json!({ "approved_by": "tester", "note": "approve" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "admin token not authorized for org");
    assert_eq!(body["message"], "00000000-0000-0000-0000-00000000ce01");
}

#[tokio::test]
async fn approval_flow_http_endpoints_expose_pending_then_queued() {
    let app = build_router(test_state());
    let create_body = json!({
        "invocation_id": "00000000-0000-0000-0000-000000000103",
        "trace_id": "00000000-0000-0000-0000-000000000203",
        "org_id": "00000000-0000-0000-0000-00000000ce01",
        "prompt": "publish deploy package",
        "reserve_amount": 25.0
    });

    let (create_status, created) =
        send_json(app.clone(), "POST", "/v1/executions", create_body).await;
    assert_eq!(create_status, StatusCode::CREATED);
    assert_eq!(created["status"], "AwaitingApproval");
    assert_eq!(created["approval_required"], true);

    let execution_id = created["execution_id"].as_str().expect("execution id");
    let approve_body = json!({ "approved_by": "tester", "note": "approve" });
    let (approve_status, approved) = send_json(
        app.clone(),
        "POST",
        &format!("/v1/executions/{execution_id}/approve"),
        approve_body,
    )
    .await;
    assert_eq!(approve_status, StatusCode::OK);
    assert_eq!(approved["status"], "Queued");
    assert_eq!(approved["approved_by"], "tester");

    let (get_status, fetched) = get_json(app, &format!("/v1/executions/{execution_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["status"], "Queued");
}
