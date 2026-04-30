use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use shared_config::{
    load_audit_scoped_admin_tokens, load_execution_scoped_admin_tokens,
    load_identity_scoped_admin_tokens, load_ledger_scoped_admin_tokens, select_audit_read_token,
    select_execution_manage_token, select_execution_read_or_manage_token,
    select_identity_manage_token, select_identity_read_or_manage_token, select_ledger_manage_token,
    select_ledger_read_or_manage_token,
};
use std::{
    env,
    path::PathBuf,
    process::Command,
    sync::{Mutex, OnceLock},
};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

fn http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build reqwest client")
}

fn scripts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("gateway-service parent")
        .parent()
        .expect("repo root")
        .join("scripts")
}

fn run_powershell_script(script_name: &str, extra_args: &[&str]) -> String {
    let script_path = scripts_dir().join(script_name);
    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script_path)
        .args(extra_args)
        .output()
        .expect("run PowerShell script");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "script {} failed\nstdout:\n{}\nstderr:\n{}",
        script_path.display(),
        stdout,
        stderr
    );

    stdout
}

async fn health_once(client: &Client) -> Result<(), String> {
    for (name, url) in [
        ("identity", "http://127.0.0.1:7001/health"),
        ("ledger", "http://127.0.0.1:7002/health"),
        ("execution", "http://127.0.0.1:7003/health"),
        ("audit", "http://127.0.0.1:7004/health"),
        ("gateway", "http://127.0.0.1:8080/health"),
    ] {
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|err| format!("health request failed for {name}: {err}"))?;
        if response.status() != StatusCode::OK {
            return Err(format!(
                "health check failed for {name}: status={} url={url}",
                response.status()
            ));
        }
    }
    Ok(())
}

async fn assert_health(client: &Client) {
    health_once(client).await.expect("runtime health");
}

async fn restart_runtime(client: &Client) {
    run_powershell_script(
        "start-local-runtime-detached.ps1",
        &["-SkipBuild", "-Restart"],
    );

    let mut last_error = None;
    for _ in 0..20 {
        match health_once(client).await {
            Ok(()) => return,
            Err(err) => {
                last_error = Some(err);
                sleep(Duration::from_secs(1)).await;
            }
        }
    }

    panic!(
        "runtime failed health checks after restart: {}",
        last_error.unwrap_or_else(|| "unknown error".to_string())
    );
}

async fn get_json(client: &Client, url: &str) -> (StatusCode, Value) {
    let mut request = client.get(url);
    if url.starts_with("http://127.0.0.1:8080/") {
        request = request.header("x-api-key", "local-dev-key");
    }
    if url.starts_with("http://127.0.0.1:7002/") {
        request = request.header("x-admin-token", resolve_ledger_read_token());
    }
    if url.starts_with("http://127.0.0.1:7003/") {
        request = request.header("x-admin-token", resolve_execution_read_token());
    }
    let response = request.send().await.expect("send GET request");
    let status = response.status();
    let body = response.text().await.expect("read GET response body");
    let json: Value = serde_json::from_str(&body).expect("decode GET response json");
    (status, json)
}

fn assert_invocation_execution_snapshot_matches_execution(invocation: &Value, execution: &Value) {
    assert_eq!(
        invocation["execution"]["dispatch_mode"],
        execution["dispatch_mode"]
    );
    assert_eq!(
        invocation["execution"]["attempt_count"],
        execution["attempt_count"]
    );
    assert_eq!(
        invocation["execution"]["max_attempts"],
        execution["max_attempts"]
    );
    assert_eq!(
        invocation["execution"]["attempts_remaining"],
        (execution["max_attempts"]
            .as_i64()
            .expect("execution max_attempts")
            - execution["attempt_count"]
                .as_i64()
                .expect("execution attempt_count"))
        .max(0)
    );
    assert_eq!(
        invocation["execution"]["retry_budget_exhausted"],
        invocation["execution"]["attempts_remaining"] == 0
    );
}

async fn post_json_with_api_key(
    client: &Client,
    url: &str,
    api_key: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = client
        .post(url)
        .header("x-api-key", api_key)
        .json(&body)
        .send()
        .await
        .expect("send POST request");
    let status = response.status();
    let body_text = response.text().await.expect("read POST response body");
    let json: Value = serde_json::from_str(&body_text).expect("decode POST response json");
    (status, json)
}

async fn post_json(client: &Client, url: &str, body: Value) -> (StatusCode, Value) {
    if url.starts_with("http://127.0.0.1:7002/") {
        let response = client
            .post(url)
            .header("x-admin-token", resolve_ledger_manage_token())
            .json(&body)
            .send()
            .await
            .expect("send POST request");
        let status = response.status();
        let body_text = response.text().await.expect("read POST response body");
        let json: Value = serde_json::from_str(&body_text).expect("decode POST response json");
        return (status, json);
    }

    if url.starts_with("http://127.0.0.1:7003/") {
        let response = client
            .post(url)
            .header("x-admin-token", resolve_execution_manage_token())
            .json(&body)
            .send()
            .await
            .expect("send POST request");
        let status = response.status();
        let body_text = response.text().await.expect("read POST response body");
        let json: Value = serde_json::from_str(&body_text).expect("decode POST response json");
        return (status, json);
    }

    post_json_with_api_key(client, url, "local-dev-key", body).await
}

fn resolve_identity_admin_token() -> String {
    let tokens = load_identity_scoped_admin_tokens();
    select_identity_manage_token(&tokens)
        .expect("identity manage token")
        .token
        .clone()
}

fn resolve_identity_api_key_list_token() -> String {
    let tokens = load_identity_scoped_admin_tokens();
    select_identity_read_or_manage_token(&tokens)
        .expect("identity list token")
        .token
        .clone()
}

fn resolve_audit_admin_token() -> String {
    let tokens = load_audit_scoped_admin_tokens();
    select_audit_read_token(&tokens)
        .expect("audit read token")
        .token
        .clone()
}

fn resolve_execution_manage_token() -> String {
    let tokens = load_execution_scoped_admin_tokens();
    select_execution_manage_token(&tokens)
        .expect("execution manage token")
        .token
        .clone()
}

fn resolve_execution_read_token() -> String {
    let tokens = load_execution_scoped_admin_tokens();
    select_execution_read_or_manage_token(&tokens)
        .expect("execution read token")
        .token
        .clone()
}

fn resolve_ledger_manage_token() -> String {
    let tokens = load_ledger_scoped_admin_tokens();
    select_ledger_manage_token(&tokens)
        .expect("ledger manage token")
        .token
        .clone()
}

fn resolve_ledger_read_token() -> String {
    let tokens = load_ledger_scoped_admin_tokens();
    select_ledger_read_or_manage_token(&tokens)
        .expect("ledger read token")
        .token
        .clone()
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn resolve_identity_admin_token_prefers_json_bundle_over_legacy_single_token() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe {
        env::set_var(
            "IDENTITY_ADMIN_TOKENS_JSON",
            r#"[{"token":"json-manage-token","scopes":["api_keys:manage"]}]"#,
        );
        env::set_var("IDENTITY_ADMIN_TOKEN", "legacy-single-token");
        env::remove_var("AUDIT_ADMIN_TOKENS_JSON");
        env::remove_var("AUDIT_ADMIN_TOKEN");
    }

    assert_eq!(resolve_identity_admin_token(), "json-manage-token");

    unsafe {
        env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
        env::remove_var("IDENTITY_ADMIN_TOKEN");
    }
}

#[test]
fn resolve_identity_api_key_list_token_prefers_read_scope_over_manage_scope() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe {
        env::set_var(
            "IDENTITY_ADMIN_TOKENS_JSON",
            r#"[{"token":"identity-read-token","scopes":["api_keys:read"]},{"token":"identity-manage-token","scopes":["api_keys:manage"]}]"#,
        );
        env::remove_var("IDENTITY_ADMIN_TOKEN");
        env::remove_var("AUDIT_ADMIN_TOKENS_JSON");
        env::remove_var("AUDIT_ADMIN_TOKEN");
    }

    assert_eq!(resolve_identity_api_key_list_token(), "identity-read-token");

    unsafe {
        env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
    }
}

#[test]
fn resolve_identity_api_key_list_token_falls_back_to_manage_scope() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe {
        env::set_var(
            "IDENTITY_ADMIN_TOKENS_JSON",
            r#"[{"token":"identity-manage-token","scopes":["api_keys:manage"]}]"#,
        );
        env::remove_var("IDENTITY_ADMIN_TOKEN");
        env::remove_var("AUDIT_ADMIN_TOKENS_JSON");
        env::remove_var("AUDIT_ADMIN_TOKEN");
    }

    assert_eq!(
        resolve_identity_api_key_list_token(),
        "identity-manage-token"
    );

    unsafe {
        env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
    }
}

#[test]
fn resolve_audit_admin_token_prefers_audit_specific_bundle_over_shared_identity_bundle() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe {
        env::set_var(
            "AUDIT_ADMIN_TOKENS_JSON",
            r#"[{"token":"audit-read-token","scopes":["audit:read"]}]"#,
        );
        env::set_var(
            "IDENTITY_ADMIN_TOKENS_JSON",
            r#"[{"token":"shared-admin-token","scopes":["api_keys:manage","audit:read"]}]"#,
        );
        env::remove_var("AUDIT_ADMIN_TOKEN");
        env::remove_var("IDENTITY_ADMIN_TOKEN");
    }

    assert_eq!(resolve_audit_admin_token(), "audit-read-token");

    unsafe {
        env::remove_var("AUDIT_ADMIN_TOKENS_JSON");
        env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
    }
}

#[test]
fn resolve_audit_admin_token_falls_back_to_identity_single_token() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe {
        env::remove_var("AUDIT_ADMIN_TOKENS_JSON");
        env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
        env::remove_var("AUDIT_ADMIN_TOKEN");
        env::set_var("IDENTITY_ADMIN_TOKEN", "legacy-shared-token");
    }

    assert_eq!(resolve_audit_admin_token(), "legacy-shared-token");

    unsafe {
        env::remove_var("IDENTITY_ADMIN_TOKEN");
    }
}

fn seed_local_dev() {
    run_powershell_script("seed-local-dev.ps1", &[]);
}

async fn post_admin_json(client: &Client, url: &str, body: Value) -> (StatusCode, Value) {
    let response = client
        .post(url)
        .header("x-admin-token", resolve_identity_admin_token())
        .json(&body)
        .send()
        .await
        .expect("send admin POST request");
    let status = response.status();
    let body_text = response
        .text()
        .await
        .expect("read admin POST response body");
    let json: Value = serde_json::from_str(&body_text).expect("decode admin POST response json");
    (status, json)
}

async fn get_identity_api_key_list_json(client: &Client, url: &str) -> (StatusCode, Value) {
    let response = client
        .get(url)
        .header("x-admin-token", resolve_identity_api_key_list_token())
        .send()
        .await
        .expect("send admin GET request");
    let status = response.status();
    let body = response.text().await.expect("read admin GET response body");
    let json: Value = serde_json::from_str(&body).expect("decode admin GET response json");
    (status, json)
}

async fn get_audit_admin_json(client: &Client, url: &str) -> (StatusCode, Value) {
    let response = client
        .get(url)
        .header("x-admin-token", resolve_audit_admin_token())
        .send()
        .await
        .expect("send audit admin GET request");
    let status = response.status();
    let body = response
        .text()
        .await
        .expect("read audit admin GET response body");
    let json: Value = serde_json::from_str(&body).expect("decode audit admin GET response json");
    (status, json)
}

async fn issue_api_key(client: &Client, label: &str) -> Value {
    let (status, issued) = post_admin_json(
        client,
        "http://127.0.0.1:7001/v1/api-keys",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "label": label
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    issued
}

async fn issue_expired_api_key(client: &Client, label: &str) -> Value {
    let (status, issued) = post_admin_json(
        client,
        "http://127.0.0.1:7001/v1/api-keys",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "label": label,
            "expires_at": "2000-01-01T00:00:00Z"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    issued
}

async fn create_account(client: &Client, initial_balance: f64) -> Value {
    let (status, account) = post_json(
        client,
        "http://127.0.0.1:7002/v1/accounts",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_type": "org_wallet",
            "currency_unit": "credit",
            "initial_balance": initial_balance
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    account
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001/7002/7003/7004/8080"]
async fn blackbox_runtime_happy_path_matches_api_regression() {
    let client = http_client();
    assert_health(&client).await;

    let account = create_account(&client, 100.0).await;
    let account_id = account["account_id"].as_str().expect("account id");

    let (account_status, account_fetched_1) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{account_id}"),
    )
    .await;
    assert_eq!(account_status, StatusCode::OK);
    assert_eq!(account_fetched_1["balance"], 100.0);
    assert_eq!(account_fetched_1["reserved"], 0.0);

    let unique_suffix = Uuid::new_v4();
    let (invocation_status, invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": account_id,
            "prompt": format!("runtime blackbox api regression {unique_suffix}"),
            "reserve_amount": 5.0
        }),
    )
    .await;
    assert_eq!(invocation_status, StatusCode::CREATED);
    assert_eq!(invocation["status"], "Queued");
    assert_eq!(invocation["ledger_reserved"], true);
    assert!(invocation["execution_id"].is_string());

    let invocation_id = invocation["invocation_id"].as_str().expect("invocation id");
    let execution_id = invocation["execution_id"].as_str().expect("execution id");
    let trace_id = invocation["trace"]["trace_id"].as_str().expect("trace id");

    let (invocation_get_status, invocation_fetched) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{invocation_id}"),
    )
    .await;
    let (execution_get_status, execution_fetched) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{execution_id}"),
    )
    .await;
    let (account_get_status, account_fetched_2) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{account_id}"),
    )
    .await;
    let (audit_get_status, audit_fetched) = get_audit_admin_json(
        &client,
        &format!("http://127.0.0.1:7004/v1/audit/events/trace/{trace_id}"),
    )
    .await;

    assert_eq!(invocation_get_status, StatusCode::OK);
    assert_eq!(execution_get_status, StatusCode::OK);
    assert_eq!(account_get_status, StatusCode::OK);
    assert_eq!(audit_get_status, StatusCode::OK);

    assert_eq!(invocation_fetched["status"], "Queued");
    assert_eq!(execution_fetched["status"], "Queued");
    assert_invocation_execution_snapshot_matches_execution(&invocation_fetched, &execution_fetched);
    assert_eq!(account_fetched_2["balance"], 100.0);
    assert_eq!(account_fetched_2["reserved"], 5.0);

    let audit_events = audit_fetched.as_array().expect("audit events array");
    let audit_event_types: Vec<&str> = audit_events
        .iter()
        .filter_map(|event| event["event_type"].as_str())
        .collect();
    assert!(audit_event_types.contains(&"invocation.created"));
    assert!(audit_event_types
        .iter()
        .any(|event| *event == "invocation.accepted" || *event == "execution.auto_approved"));
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001/7002/7003/7004/8080"]
async fn blackbox_runtime_approval_flow_matches_regression() {
    let client = http_client();
    assert_health(&client).await;

    let account = create_account(&client, 100.0).await;
    let account_id = account["account_id"].as_str().expect("account id");

    let unique_suffix = Uuid::new_v4();
    let (invocation_status, invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": account_id,
            "prompt": format!("publish deployment package to external users {unique_suffix}"),
            "reserve_amount": 25.0
        }),
    )
    .await;
    assert_eq!(invocation_status, StatusCode::ACCEPTED);
    assert_eq!(invocation["status"], "AwaitingApproval");
    assert!(invocation["execution_id"].is_string());

    let invocation_id = invocation["invocation_id"].as_str().expect("invocation id");
    let execution_id = invocation["execution_id"].as_str().expect("execution id");
    let trace_id = invocation["trace"]["trace_id"].as_str().expect("trace id");

    let (execution_before_status, execution_before) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{execution_id}"),
    )
    .await;
    let (audit_before_status, audit_before) = get_audit_admin_json(
        &client,
        &format!("http://127.0.0.1:7004/v1/audit/events/trace/{trace_id}"),
    )
    .await;
    assert_eq!(execution_before_status, StatusCode::OK);
    assert_eq!(audit_before_status, StatusCode::OK);
    assert_eq!(execution_before["status"], "AwaitingApproval");
    assert!(execution_before["policy_reason"]
        .as_str()
        .expect("policy reason")
        .contains("publish"));

    let (approve_status, execution_after) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{execution_id}/approve"),
        json!({
            "approved_by": "runtime-blackbox-approver",
            "note": "runtime blackbox approval regression"
        }),
    )
    .await;
    assert_eq!(approve_status, StatusCode::OK);
    assert_eq!(execution_after["status"], "Queued");
    assert_eq!(execution_after["approved_by"], "runtime-blackbox-approver");

    let (invocation_after_status, invocation_after) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{invocation_id}"),
    )
    .await;
    let (account_after_status, account_after) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{account_id}"),
    )
    .await;
    let (audit_after_status, audit_after) = get_audit_admin_json(
        &client,
        &format!("http://127.0.0.1:7004/v1/audit/events/trace/{trace_id}"),
    )
    .await;

    assert_eq!(invocation_after_status, StatusCode::OK);
    assert_eq!(account_after_status, StatusCode::OK);
    assert_eq!(audit_after_status, StatusCode::OK);
    assert_eq!(invocation_after["status"], "Queued");
    assert_invocation_execution_snapshot_matches_execution(&invocation_after, &execution_after);
    assert_eq!(account_after["balance"], 100.0);
    assert_eq!(account_after["reserved"], 25.0);

    let audit_before_types: Vec<&str> = audit_before
        .as_array()
        .expect("audit before array")
        .iter()
        .filter_map(|event| event["event_type"].as_str())
        .collect();
    let audit_after_types: Vec<&str> = audit_after
        .as_array()
        .expect("audit after array")
        .iter()
        .filter_map(|event| event["event_type"].as_str())
        .collect();

    assert!(audit_before_types
        .iter()
        .any(|event| *event == "invocation.awaiting_approval"
            || *event == "execution.awaiting_approval"));
    assert!(audit_after_types.contains(&"execution.approved"));
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001/7002/7003/7004/8080"]
async fn blackbox_runtime_reject_refund_flow_matches_regression() {
    let client = http_client();
    assert_health(&client).await;

    let account = create_account(&client, 100.0).await;
    let account_id = account["account_id"].as_str().expect("account id");
    let unique_suffix = Uuid::new_v4();

    let (invocation_status, invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": account_id,
            "prompt": format!("publish deployment package to external users reject-refund {unique_suffix}"),
            "reserve_amount": 25.0
        }),
    )
    .await;
    assert_eq!(invocation_status, StatusCode::ACCEPTED);
    assert_eq!(invocation["status"], "AwaitingApproval");

    let invocation_id = invocation["invocation_id"].as_str().expect("invocation id");
    let execution_id = invocation["execution_id"].as_str().expect("execution id");

    let (account_before_status, account_before) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{account_id}"),
    )
    .await;
    assert_eq!(account_before_status, StatusCode::OK);
    assert_eq!(account_before["reserved"], 25.0);

    let (reject_status, execution_after_reject) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{execution_id}/reject"),
        json!({
            "rejected_by": "runtime-blackbox-reviewer",
            "reason": "runtime blackbox reject refund regression"
        }),
    )
    .await;
    assert_eq!(reject_status, StatusCode::OK);
    assert_eq!(execution_after_reject["status"], "Cancelled");

    let (invocation_after_status, invocation_after) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{invocation_id}"),
    )
    .await;
    let (account_after_status, account_after) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{account_id}"),
    )
    .await;
    let (execution_after_status, execution_after_get) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{execution_id}"),
    )
    .await;

    assert_eq!(invocation_after_status, StatusCode::OK);
    assert_eq!(account_after_status, StatusCode::OK);
    assert_eq!(execution_after_status, StatusCode::OK);
    assert_eq!(invocation_after["status"], "Refunded");
    assert_eq!(invocation_after["ledger_refunded"], true);
    assert_invocation_execution_snapshot_matches_execution(&invocation_after, &execution_after_get);
    assert_eq!(account_after["balance"], 100.0);
    assert_eq!(account_after["reserved"], 0.0);
    assert_eq!(execution_after_get["status"], "Cancelled");

    restart_runtime(&client).await;

    let (invocation_restart_status, invocation_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{invocation_id}"),
    )
    .await;
    let (account_restart_status, account_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{account_id}"),
    )
    .await;
    let (execution_restart_status, execution_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{execution_id}"),
    )
    .await;

    assert_eq!(invocation_restart_status, StatusCode::OK);
    assert_eq!(account_restart_status, StatusCode::OK);
    assert_eq!(execution_restart_status, StatusCode::OK);
    assert_eq!(invocation_after_restart["status"], "Refunded");
    assert_eq!(account_after_restart["reserved"], 0.0);
    assert_eq!(execution_after_restart["status"], "Cancelled");
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001/7002/7003/7004/8080"]
async fn blackbox_runtime_settlement_flows_match_regression() {
    let client = http_client();
    assert_health(&client).await;

    let success_account = create_account(&client, 100.0).await;
    let success_account_id = success_account["account_id"]
        .as_str()
        .expect("success account id");
    let success_suffix = Uuid::new_v4();
    let (success_invocation_status, success_invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": success_account_id,
            "prompt": format!("settlement success runtime blackbox {success_suffix}"),
            "reserve_amount": 5.0
        }),
    )
    .await;
    assert_eq!(success_invocation_status, StatusCode::CREATED);
    assert_eq!(success_invocation["status"], "Queued");
    let success_invocation_id = success_invocation["invocation_id"]
        .as_str()
        .expect("success invocation id");
    let success_execution_id = success_invocation["execution_id"]
        .as_str()
        .expect("success execution id");

    let (success_settle_status, success_execution_after) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{success_execution_id}/succeed"),
        json!({
            "settled_by": "runtime-blackbox-runner",
            "note": "runtime blackbox settlement success"
        }),
    )
    .await;
    assert_eq!(success_settle_status, StatusCode::OK);
    assert_eq!(success_execution_after["status"], "Succeeded");

    let (success_invocation_after_status, success_invocation_after) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{success_invocation_id}"),
    )
    .await;
    let (success_account_after_status, success_account_after) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{success_account_id}"),
    )
    .await;
    assert_eq!(success_invocation_after_status, StatusCode::OK);
    assert_eq!(success_account_after_status, StatusCode::OK);
    assert_eq!(success_invocation_after["status"], "Succeeded");
    assert_invocation_execution_snapshot_matches_execution(
        &success_invocation_after,
        &success_execution_after,
    );
    assert_eq!(success_account_after["balance"], 95.0);
    assert_eq!(success_account_after["reserved"], 0.0);

    let fail_account = create_account(&client, 100.0).await;
    let fail_account_id = fail_account["account_id"]
        .as_str()
        .expect("fail account id");
    let fail_suffix = Uuid::new_v4();
    let (fail_invocation_status, fail_invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": fail_account_id,
            "prompt": format!("settlement fail runtime blackbox {fail_suffix}"),
            "reserve_amount": 6.0
        }),
    )
    .await;
    assert_eq!(fail_invocation_status, StatusCode::CREATED);
    assert_eq!(fail_invocation["status"], "Queued");
    let fail_invocation_id = fail_invocation["invocation_id"]
        .as_str()
        .expect("fail invocation id");
    let fail_execution_id = fail_invocation["execution_id"]
        .as_str()
        .expect("fail execution id");

    let (fail_settle_status, fail_execution_after) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{fail_execution_id}/fail"),
        json!({
            "failed_by": "runtime-blackbox-runner",
            "reason": "runtime blackbox settlement failure"
        }),
    )
    .await;
    assert_eq!(fail_settle_status, StatusCode::OK);
    assert_eq!(fail_execution_after["status"], "Failed");

    let (fail_invocation_after_status, fail_invocation_after) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{fail_invocation_id}"),
    )
    .await;
    let (fail_account_after_status, fail_account_after) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{fail_account_id}"),
    )
    .await;
    assert_eq!(fail_invocation_after_status, StatusCode::OK);
    assert_eq!(fail_account_after_status, StatusCode::OK);
    assert_eq!(fail_invocation_after["status"], "Refunded");
    assert_eq!(fail_invocation_after["ledger_refunded"], true);
    assert_invocation_execution_snapshot_matches_execution(
        &fail_invocation_after,
        &fail_execution_after,
    );
    assert_eq!(fail_account_after["balance"], 100.0);
    assert_eq!(fail_account_after["reserved"], 0.0);

    restart_runtime(&client).await;

    let (success_invocation_restart_status, success_invocation_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{success_invocation_id}"),
    )
    .await;
    let (success_execution_restart_status, success_execution_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{success_execution_id}"),
    )
    .await;
    let (success_account_restart_status, success_account_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{success_account_id}"),
    )
    .await;
    let (fail_invocation_restart_status, fail_invocation_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{fail_invocation_id}"),
    )
    .await;
    let (fail_execution_restart_status, fail_execution_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{fail_execution_id}"),
    )
    .await;
    let (fail_account_restart_status, fail_account_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{fail_account_id}"),
    )
    .await;

    assert_eq!(success_invocation_restart_status, StatusCode::OK);
    assert_eq!(success_execution_restart_status, StatusCode::OK);
    assert_eq!(success_account_restart_status, StatusCode::OK);
    assert_eq!(fail_invocation_restart_status, StatusCode::OK);
    assert_eq!(fail_execution_restart_status, StatusCode::OK);
    assert_eq!(fail_account_restart_status, StatusCode::OK);

    assert_eq!(success_invocation_after_restart["status"], "Succeeded");
    assert_eq!(success_execution_after_restart["status"], "Succeeded");
    assert_eq!(success_account_after_restart["balance"], 95.0);
    assert_eq!(success_account_after_restart["reserved"], 0.0);
    assert_eq!(fail_invocation_after_restart["status"], "Refunded");
    assert_eq!(fail_execution_after_restart["status"], "Failed");
    assert_eq!(fail_account_after_restart["balance"], 100.0);
    assert_eq!(fail_account_after_restart["reserved"], 0.0);
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001/7002/7003/7004/8080"]
async fn blackbox_runtime_lifecycle_cancel_and_timeout_match_regression() {
    let client = http_client();
    assert_health(&client).await;

    let dispatch_body = json!({
        "dispatched_by": "runtime-blackbox-runner",
        "note": "runtime blackbox lifecycle dispatch"
    });
    let start_body = json!({
        "started_by": "runtime-blackbox-runner",
        "note": "runtime blackbox lifecycle start"
    });
    let cancel_body = json!({
        "cancelled_by": "runtime-blackbox-runner",
        "reason": "runtime blackbox lifecycle cancel"
    });
    let timeout_body = json!({
        "timed_out_by": "runtime-blackbox-runner",
        "reason": "runtime blackbox lifecycle timeout"
    });

    let cancel_account = create_account(&client, 100.0).await;
    let cancel_account_id = cancel_account["account_id"]
        .as_str()
        .expect("cancel account id");
    let cancel_suffix = Uuid::new_v4();
    let (cancel_invocation_status, cancel_invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": cancel_account_id,
            "prompt": format!("runtime blackbox lifecycle cancel {cancel_suffix}"),
            "reserve_amount": 8.0
        }),
    )
    .await;
    assert_eq!(cancel_invocation_status, StatusCode::CREATED);
    assert_eq!(cancel_invocation["status"], "Queued");
    let cancel_invocation_id = cancel_invocation["invocation_id"]
        .as_str()
        .expect("cancel invocation id");
    let cancel_execution_id = cancel_invocation["execution_id"]
        .as_str()
        .expect("cancel execution id");

    let (cancel_dispatch_status, cancel_dispatch) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{cancel_execution_id}/dispatch"),
        dispatch_body.clone(),
    )
    .await;
    assert_eq!(cancel_dispatch_status, StatusCode::OK);
    assert_eq!(cancel_dispatch["status"], "Dispatching");

    let (cancel_invocation_dispatch_status, cancel_invocation_after_dispatch) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{cancel_invocation_id}"),
    )
    .await;
    assert_eq!(cancel_invocation_dispatch_status, StatusCode::OK);
    assert_eq!(cancel_invocation_after_dispatch["status"], "Dispatching");

    let (cancel_start_status, cancel_start) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{cancel_execution_id}/start"),
        start_body.clone(),
    )
    .await;
    assert_eq!(cancel_start_status, StatusCode::OK);
    assert_eq!(cancel_start["status"], "Running");

    let (cancel_invocation_start_status, cancel_invocation_after_start) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{cancel_invocation_id}"),
    )
    .await;
    assert_eq!(cancel_invocation_start_status, StatusCode::OK);
    assert_eq!(cancel_invocation_after_start["status"], "Running");

    let (cancel_final_status, cancel_final) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{cancel_execution_id}/cancel"),
        cancel_body,
    )
    .await;
    assert_eq!(cancel_final_status, StatusCode::OK);
    assert_eq!(cancel_final["status"], "Cancelled");

    let (cancel_invocation_final_status, cancel_invocation_after_final) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{cancel_invocation_id}"),
    )
    .await;
    let (cancel_account_final_status, cancel_account_after_final) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{cancel_account_id}"),
    )
    .await;
    assert_eq!(cancel_invocation_final_status, StatusCode::OK);
    assert_eq!(cancel_account_final_status, StatusCode::OK);
    assert_eq!(cancel_invocation_after_final["status"], "Refunded");
    assert_eq!(cancel_invocation_after_final["ledger_refunded"], true);
    assert_invocation_execution_snapshot_matches_execution(
        &cancel_invocation_after_final,
        &cancel_final,
    );
    assert_eq!(cancel_account_after_final["balance"], 100.0);
    assert_eq!(cancel_account_after_final["reserved"], 0.0);

    let timeout_account = create_account(&client, 100.0).await;
    let timeout_account_id = timeout_account["account_id"]
        .as_str()
        .expect("timeout account id");
    let timeout_suffix = Uuid::new_v4();
    let (timeout_invocation_status, timeout_invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": timeout_account_id,
            "prompt": format!("runtime blackbox lifecycle timeout {timeout_suffix}"),
            "reserve_amount": 9.0
        }),
    )
    .await;
    assert_eq!(timeout_invocation_status, StatusCode::CREATED);
    assert_eq!(timeout_invocation["status"], "Queued");
    let timeout_invocation_id = timeout_invocation["invocation_id"]
        .as_str()
        .expect("timeout invocation id");
    let timeout_execution_id = timeout_invocation["execution_id"]
        .as_str()
        .expect("timeout execution id");

    let (timeout_dispatch_status, timeout_dispatch) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{timeout_execution_id}/dispatch"),
        dispatch_body,
    )
    .await;
    assert_eq!(timeout_dispatch_status, StatusCode::OK);
    assert_eq!(timeout_dispatch["status"], "Dispatching");

    let (timeout_start_status, timeout_start) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{timeout_execution_id}/start"),
        start_body,
    )
    .await;
    assert_eq!(timeout_start_status, StatusCode::OK);
    assert_eq!(timeout_start["status"], "Running");

    let (timeout_final_status, timeout_final) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{timeout_execution_id}/timeout"),
        timeout_body,
    )
    .await;
    assert_eq!(timeout_final_status, StatusCode::OK);
    assert_eq!(timeout_final["status"], "TimedOut");

    let (timeout_invocation_final_status, timeout_invocation_after_final) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{timeout_invocation_id}"),
    )
    .await;
    let (timeout_account_final_status, timeout_account_after_final) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{timeout_account_id}"),
    )
    .await;
    assert_eq!(timeout_invocation_final_status, StatusCode::OK);
    assert_eq!(timeout_account_final_status, StatusCode::OK);
    assert_eq!(timeout_invocation_after_final["status"], "Refunded");
    assert_eq!(timeout_invocation_after_final["ledger_refunded"], true);
    assert_invocation_execution_snapshot_matches_execution(
        &timeout_invocation_after_final,
        &timeout_final,
    );
    assert_eq!(timeout_account_after_final["balance"], 100.0);
    assert_eq!(timeout_account_after_final["reserved"], 0.0);

    restart_runtime(&client).await;

    let (cancel_invocation_restart_status, cancel_invocation_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{cancel_invocation_id}"),
    )
    .await;
    let (cancel_execution_restart_status, cancel_execution_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{cancel_execution_id}"),
    )
    .await;
    let (cancel_account_restart_status, cancel_account_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{cancel_account_id}"),
    )
    .await;
    let (timeout_invocation_restart_status, timeout_invocation_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{timeout_invocation_id}"),
    )
    .await;
    let (timeout_execution_restart_status, timeout_execution_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{timeout_execution_id}"),
    )
    .await;
    let (timeout_account_restart_status, timeout_account_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{timeout_account_id}"),
    )
    .await;

    assert_eq!(cancel_invocation_restart_status, StatusCode::OK);
    assert_eq!(cancel_execution_restart_status, StatusCode::OK);
    assert_eq!(cancel_account_restart_status, StatusCode::OK);
    assert_eq!(timeout_invocation_restart_status, StatusCode::OK);
    assert_eq!(timeout_execution_restart_status, StatusCode::OK);
    assert_eq!(timeout_account_restart_status, StatusCode::OK);
    assert_eq!(cancel_invocation_after_restart["status"], "Refunded");
    assert_eq!(cancel_execution_after_restart["status"], "Cancelled");
    assert_eq!(cancel_account_after_restart["balance"], 100.0);
    assert_eq!(cancel_account_after_restart["reserved"], 0.0);
    assert_eq!(timeout_invocation_after_restart["status"], "Refunded");
    assert_eq!(timeout_execution_after_restart["status"], "TimedOut");
    assert_eq!(timeout_account_after_restart["balance"], 100.0);
    assert_eq!(timeout_account_after_restart["reserved"], 0.0);
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001/7002/7003/7004/8080"]
async fn blackbox_runtime_state_machine_replay_and_conflict_match_regression() {
    let client = http_client();
    assert_health(&client).await;

    let dispatch_body = json!({
        "dispatched_by": "runtime-blackbox-runner",
        "note": "runtime blackbox state machine dispatch"
    });
    let start_body = json!({
        "started_by": "runtime-blackbox-runner",
        "note": "runtime blackbox state machine start"
    });
    let succeed_body = json!({
        "settled_by": "runtime-blackbox-runner",
        "note": "runtime blackbox state machine succeed"
    });
    let fail_body = json!({
        "failed_by": "runtime-blackbox-runner",
        "reason": "runtime blackbox state machine fail"
    });
    let cancel_body = json!({
        "cancelled_by": "runtime-blackbox-runner",
        "reason": "runtime blackbox state machine cancel"
    });
    let timeout_body = json!({
        "timed_out_by": "runtime-blackbox-runner",
        "reason": "runtime blackbox state machine timeout"
    });

    let success_account = create_account(&client, 100.0).await;
    let success_account_id = success_account["account_id"]
        .as_str()
        .expect("success account id");
    let success_suffix = Uuid::new_v4();
    let (success_invocation_status, success_invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": success_account_id,
            "prompt": format!("runtime blackbox replay success {success_suffix}"),
            "reserve_amount": 4.0
        }),
    )
    .await;
    assert_eq!(success_invocation_status, StatusCode::CREATED);
    let success_invocation_id = success_invocation["invocation_id"]
        .as_str()
        .expect("success invocation id");
    let success_execution_id = success_invocation["execution_id"]
        .as_str()
        .expect("success execution id");

    let (success_dispatch_1_status, success_dispatch_1) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{success_execution_id}/dispatch"),
        dispatch_body.clone(),
    )
    .await;
    let (success_dispatch_2_status, success_dispatch_2) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{success_execution_id}/dispatch"),
        dispatch_body.clone(),
    )
    .await;
    assert_eq!(success_dispatch_1_status, StatusCode::OK);
    assert_eq!(success_dispatch_2_status, StatusCode::OK);
    assert_eq!(success_dispatch_1["status"], "Dispatching");
    assert_eq!(success_dispatch_2["status"], "Dispatching");

    let (success_start_1_status, success_start_1) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{success_execution_id}/start"),
        start_body.clone(),
    )
    .await;
    let (success_start_2_status, success_start_2) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{success_execution_id}/start"),
        start_body.clone(),
    )
    .await;
    assert_eq!(success_start_1_status, StatusCode::OK);
    assert_eq!(success_start_2_status, StatusCode::OK);
    assert_eq!(success_start_1["status"], "Running");
    assert_eq!(success_start_2["status"], "Running");

    let (success_succeed_1_status, success_succeed_1) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{success_execution_id}/succeed"),
        succeed_body.clone(),
    )
    .await;
    let (success_succeed_2_status, success_succeed_2) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{success_execution_id}/succeed"),
        succeed_body,
    )
    .await;
    let (success_fail_after_status, success_fail_after) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{success_execution_id}/fail"),
        fail_body.clone(),
    )
    .await;
    assert_eq!(success_succeed_1_status, StatusCode::OK);
    assert_eq!(success_succeed_2_status, StatusCode::OK);
    assert_eq!(success_succeed_1["status"], "Succeeded");
    assert_eq!(success_succeed_2["status"], "Succeeded");
    assert_eq!(success_fail_after_status, StatusCode::CONFLICT);
    assert_eq!(success_fail_after["status"], "Succeeded");

    let cancel_account = create_account(&client, 100.0).await;
    let cancel_account_id = cancel_account["account_id"]
        .as_str()
        .expect("cancel account id");
    let cancel_suffix = Uuid::new_v4();
    let (cancel_invocation_status, cancel_invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": cancel_account_id,
            "prompt": format!("runtime blackbox replay cancel {cancel_suffix}"),
            "reserve_amount": 6.0
        }),
    )
    .await;
    assert_eq!(cancel_invocation_status, StatusCode::CREATED);
    let cancel_invocation_id = cancel_invocation["invocation_id"]
        .as_str()
        .expect("cancel invocation id");
    let cancel_execution_id = cancel_invocation["execution_id"]
        .as_str()
        .expect("cancel execution id");

    let _ = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{cancel_execution_id}/dispatch"),
        dispatch_body.clone(),
    )
    .await;
    let _ = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{cancel_execution_id}/start"),
        start_body.clone(),
    )
    .await;
    let (cancel_1_status, cancel_1) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{cancel_execution_id}/cancel"),
        cancel_body.clone(),
    )
    .await;
    let (cancel_2_status, cancel_2) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{cancel_execution_id}/cancel"),
        cancel_body,
    )
    .await;
    let (cancel_start_after_status, cancel_start_after) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{cancel_execution_id}/start"),
        start_body.clone(),
    )
    .await;
    assert_eq!(cancel_1_status, StatusCode::OK);
    assert_eq!(cancel_2_status, StatusCode::OK);
    assert_eq!(cancel_1["status"], "Cancelled");
    assert_eq!(cancel_2["status"], "Cancelled");
    assert_eq!(cancel_start_after_status, StatusCode::CONFLICT);
    assert_eq!(cancel_start_after["status"], "Cancelled");

    let timeout_account = create_account(&client, 100.0).await;
    let timeout_account_id = timeout_account["account_id"]
        .as_str()
        .expect("timeout account id");
    let timeout_suffix = Uuid::new_v4();
    let (timeout_invocation_status, timeout_invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": timeout_account_id,
            "prompt": format!("runtime blackbox replay timeout {timeout_suffix}"),
            "reserve_amount": 7.0
        }),
    )
    .await;
    assert_eq!(timeout_invocation_status, StatusCode::CREATED);
    let timeout_invocation_id = timeout_invocation["invocation_id"]
        .as_str()
        .expect("timeout invocation id");
    let timeout_execution_id = timeout_invocation["execution_id"]
        .as_str()
        .expect("timeout execution id");

    let _ = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{timeout_execution_id}/dispatch"),
        dispatch_body,
    )
    .await;
    let _ = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{timeout_execution_id}/start"),
        start_body,
    )
    .await;
    let (timeout_1_status, timeout_1) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{timeout_execution_id}/timeout"),
        timeout_body.clone(),
    )
    .await;
    let (timeout_2_status, timeout_2) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{timeout_execution_id}/timeout"),
        timeout_body,
    )
    .await;
    let (timeout_dispatch_after_status, timeout_dispatch_after) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{timeout_execution_id}/dispatch"),
        json!({
            "dispatched_by": "runtime-blackbox-runner",
            "note": "runtime blackbox invalid dispatch after timeout"
        }),
    )
    .await;
    assert_eq!(timeout_1_status, StatusCode::OK);
    assert_eq!(timeout_2_status, StatusCode::OK);
    assert_eq!(timeout_1["status"], "TimedOut");
    assert_eq!(timeout_2["status"], "TimedOut");
    assert_eq!(timeout_dispatch_after_status, StatusCode::CONFLICT);
    assert_eq!(timeout_dispatch_after["status"], "TimedOut");

    restart_runtime(&client).await;

    let (success_execution_restart_status, success_execution_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{success_execution_id}"),
    )
    .await;
    let (success_invocation_restart_status, success_invocation_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{success_invocation_id}"),
    )
    .await;
    let (success_account_restart_status, success_account_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{success_account_id}"),
    )
    .await;
    let (cancel_execution_restart_status, cancel_execution_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{cancel_execution_id}"),
    )
    .await;
    let (cancel_invocation_restart_status, cancel_invocation_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{cancel_invocation_id}"),
    )
    .await;
    let (cancel_account_restart_status, cancel_account_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{cancel_account_id}"),
    )
    .await;
    let (timeout_execution_restart_status, timeout_execution_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{timeout_execution_id}"),
    )
    .await;
    let (timeout_invocation_restart_status, timeout_invocation_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{timeout_invocation_id}"),
    )
    .await;
    let (timeout_account_restart_status, timeout_account_after_restart) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{timeout_account_id}"),
    )
    .await;

    assert_eq!(success_execution_restart_status, StatusCode::OK);
    assert_eq!(success_invocation_restart_status, StatusCode::OK);
    assert_eq!(success_account_restart_status, StatusCode::OK);
    assert_eq!(cancel_execution_restart_status, StatusCode::OK);
    assert_eq!(cancel_invocation_restart_status, StatusCode::OK);
    assert_eq!(cancel_account_restart_status, StatusCode::OK);
    assert_eq!(timeout_execution_restart_status, StatusCode::OK);
    assert_eq!(timeout_invocation_restart_status, StatusCode::OK);
    assert_eq!(timeout_account_restart_status, StatusCode::OK);

    assert_eq!(success_execution_after_restart["status"], "Succeeded");
    assert_eq!(success_invocation_after_restart["status"], "Succeeded");
    assert_eq!(success_account_after_restart["balance"], 96.0);
    assert_eq!(success_account_after_restart["reserved"], 0.0);
    assert_eq!(cancel_execution_after_restart["status"], "Cancelled");
    assert_eq!(cancel_invocation_after_restart["status"], "Refunded");
    assert_eq!(cancel_account_after_restart["balance"], 100.0);
    assert_eq!(cancel_account_after_restart["reserved"], 0.0);
    assert_eq!(timeout_execution_after_restart["status"], "TimedOut");
    assert_eq!(timeout_invocation_after_restart["status"], "Refunded");
    assert_eq!(timeout_account_after_restart["balance"], 100.0);
    assert_eq!(timeout_account_after_restart["reserved"], 0.0);
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001/7002/7003/7004/8080"]
async fn blackbox_runtime_workflow_persistence_matches_regression() {
    let client = http_client();
    assert_health(&client).await;

    let auto_account = create_account(&client, 100.0).await;
    let auto_account_id = auto_account["account_id"]
        .as_str()
        .expect("auto account id");
    let auto_suffix = Uuid::new_v4();
    let (auto_invocation_status, auto_invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": auto_account_id,
            "prompt": format!("workflow persistence auto path {auto_suffix}"),
            "reserve_amount": 5.0
        }),
    )
    .await;
    assert_eq!(auto_invocation_status, StatusCode::CREATED);
    assert_eq!(auto_invocation["status"], "Queued");
    let auto_invocation_id = auto_invocation["invocation_id"]
        .as_str()
        .expect("auto invocation id");
    let auto_execution_id = auto_invocation["execution_id"]
        .as_str()
        .expect("auto execution id");

    let approval_account = create_account(&client, 100.0).await;
    let approval_account_id = approval_account["account_id"]
        .as_str()
        .expect("approval account id");
    let approval_suffix = Uuid::new_v4();
    let (approval_invocation_status, approval_invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": approval_account_id,
            "prompt": format!("publish deployment package to external users workflow persistence {approval_suffix}"),
            "reserve_amount": 25.0
        }),
    )
    .await;
    assert_eq!(approval_invocation_status, StatusCode::ACCEPTED);
    assert_eq!(approval_invocation["status"], "AwaitingApproval");
    let approval_invocation_id = approval_invocation["invocation_id"]
        .as_str()
        .expect("approval invocation id");
    let approval_execution_id = approval_invocation["execution_id"]
        .as_str()
        .expect("approval execution id");

    let (approve_status, approved_execution) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{approval_execution_id}/approve"),
        json!({
            "approved_by": "runtime-blackbox-approver",
            "note": "runtime blackbox workflow persistence"
        }),
    )
    .await;
    assert_eq!(approve_status, StatusCode::OK);
    assert_eq!(approved_execution["status"], "Queued");

    restart_runtime(&client).await;

    let (auto_invocation_after_status, auto_invocation_after) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{auto_invocation_id}"),
    )
    .await;
    let (auto_execution_after_status, auto_execution_after) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{auto_execution_id}"),
    )
    .await;
    let (auto_account_after_status, auto_account_after) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{auto_account_id}"),
    )
    .await;
    let (approval_invocation_after_status, approval_invocation_after) = get_json(
        &client,
        &format!("http://127.0.0.1:8080/v1/invocations/{approval_invocation_id}"),
    )
    .await;
    let (approval_execution_after_status, approval_execution_after) = get_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{approval_execution_id}"),
    )
    .await;
    let (approval_account_after_status, approval_account_after) = get_json(
        &client,
        &format!("http://127.0.0.1:7002/v1/accounts/{approval_account_id}"),
    )
    .await;

    assert_eq!(auto_invocation_after_status, StatusCode::OK);
    assert_eq!(auto_execution_after_status, StatusCode::OK);
    assert_eq!(auto_account_after_status, StatusCode::OK);
    assert_eq!(approval_invocation_after_status, StatusCode::OK);
    assert_eq!(approval_execution_after_status, StatusCode::OK);
    assert_eq!(approval_account_after_status, StatusCode::OK);

    assert_eq!(auto_invocation_after["status"], "Queued");
    assert_eq!(auto_execution_after["status"], "Queued");
    assert_invocation_execution_snapshot_matches_execution(
        &auto_invocation_after,
        &auto_execution_after,
    );
    assert_eq!(auto_account_after["balance"], 100.0);
    assert_eq!(auto_account_after["reserved"], 5.0);

    assert_eq!(approval_invocation_after["status"], "Queued");
    assert_eq!(approval_execution_after["status"], "Queued");
    assert_invocation_execution_snapshot_matches_execution(
        &approval_invocation_after,
        &approval_execution_after,
    );
    assert_eq!(
        approval_execution_after["approved_by"],
        "runtime-blackbox-approver"
    );
    assert_eq!(approval_account_after["balance"], 100.0);
    assert_eq!(approval_account_after["reserved"], 25.0);
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001/7002/7003/7004/8080"]
async fn blackbox_runtime_api_key_issue_revoke_flow_matches_regression() {
    let client = http_client();
    assert_health(&client).await;
    seed_local_dev();

    let account = create_account(&client, 100.0).await;
    let account_id = account["account_id"].as_str().expect("account id");

    let issued = issue_api_key(&client, "Runtime Blackbox Rotation Key").await;
    let api_key = issued["api_key"].as_str().expect("issued api key");
    let api_key_id = issued["record"]["api_key_id"]
        .as_str()
        .expect("issued api key id");

    let unique_suffix = Uuid::new_v4();
    let (invocation_status, invocation) = post_json_with_api_key(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        api_key,
        json!({
            "account_id": account_id,
            "prompt": format!("runtime issued key regression {unique_suffix}"),
            "reserve_amount": 3.0
        }),
    )
    .await;
    assert_eq!(invocation_status, StatusCode::CREATED);
    assert_eq!(invocation["status"], "Queued");

    let (list_status, listed) = get_identity_api_key_list_json(
        &client,
        "http://127.0.0.1:7001/v1/api-keys?org_id=00000000-0000-0000-0000-00000000ce01",
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    let items = listed["items"].as_array().expect("api key items array");
    assert!(items.iter().any(|item| item["api_key_id"] == api_key_id));

    let (revoke_status, revoked) = post_admin_json(
        &client,
        &format!("http://127.0.0.1:7001/v1/api-keys/{api_key_id}/revoke"),
        json!({
            "reason": "rotation"
        }),
    )
    .await;
    assert_eq!(revoke_status, StatusCode::OK);
    assert_eq!(revoked["status"], "revoked");
    assert_eq!(revoked["revoked_reason"], "rotation");

    let (rejected_status, rejected) = post_json_with_api_key(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        api_key,
        json!({
            "account_id": account_id,
            "prompt": "runtime revoked key should fail",
            "reserve_amount": 1.0
        }),
    )
    .await;
    assert_eq!(rejected_status, StatusCode::UNAUTHORIZED);
    assert_eq!(rejected["error"], "auth resolution failed");
    assert!(rejected["message"]
        .as_str()
        .expect("auth failure message")
        .contains("revoked api key"));
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001/7002/7003/7004/8080"]
async fn blackbox_runtime_api_key_expiry_flow_matches_regression() {
    let client = http_client();
    assert_health(&client).await;
    seed_local_dev();

    let account = create_account(&client, 100.0).await;
    let account_id = account["account_id"].as_str().expect("account id");

    let issued = issue_expired_api_key(&client, "Runtime Blackbox Expired Key").await;
    let api_key = issued["api_key"].as_str().expect("expired api key");
    let api_key_id = issued["record"]["api_key_id"]
        .as_str()
        .expect("expired api key id");

    let (list_status, listed) = get_identity_api_key_list_json(
        &client,
        "http://127.0.0.1:7001/v1/api-keys?org_id=00000000-0000-0000-0000-00000000ce01",
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    let items = listed["items"].as_array().expect("api key items array");
    let expired_item = items
        .iter()
        .find(|item| item["api_key_id"] == api_key_id)
        .expect("expired api key item");
    assert_eq!(expired_item["status"], "expired");

    let (rejected_status, rejected) = post_json_with_api_key(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        api_key,
        json!({
            "account_id": account_id,
            "prompt": "runtime expired key should fail",
            "reserve_amount": 1.0
        }),
    )
    .await;
    assert_eq!(rejected_status, StatusCode::UNAUTHORIZED);
    assert_eq!(rejected["error"], "auth resolution failed");
    assert!(rejected["message"]
        .as_str()
        .expect("auth failure message")
        .contains("expired api key"));
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001/7002/7003/7004/8080"]
async fn blackbox_runtime_audit_persistence_matches_regression() {
    let client = http_client();
    assert_health(&client).await;

    let trace_id = Uuid::new_v4();
    let nonce = Uuid::new_v4().to_string();
    let (create_status, created) = post_json(
        &client,
        "http://127.0.0.1:7004/v1/audit/events",
        json!({
            "trace_id": trace_id,
            "actor_type": "runtime-blackbox",
            "actor_id": "local-dev",
            "event_type": "audit.persistence.probe",
            "payload": {
                "probe": "restart-survival",
                "nonce": nonce
            }
        }),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED);
    let created_event_id = created["event_id"].as_str().expect("created event id");

    let (before_status, before_events) = get_audit_admin_json(
        &client,
        &format!("http://127.0.0.1:7004/v1/audit/events/trace/{trace_id}"),
    )
    .await;
    assert_eq!(before_status, StatusCode::OK);
    let before_items = before_events.as_array().expect("before audit events array");
    assert_eq!(before_items.len(), 1);
    assert_eq!(before_items[0]["event_id"], created_event_id);
    assert_eq!(before_items[0]["payload"]["nonce"], nonce);

    restart_runtime(&client).await;

    let (after_status, after_events) = get_audit_admin_json(
        &client,
        &format!("http://127.0.0.1:7004/v1/audit/events/trace/{trace_id}"),
    )
    .await;
    assert_eq!(after_status, StatusCode::OK);
    let after_items = after_events.as_array().expect("after audit events array");
    assert_eq!(after_items.len(), 1);
    assert_eq!(after_items[0]["event_id"], created_event_id);
    assert_eq!(after_items[0]["event_type"], "audit.persistence.probe");
    assert_eq!(after_items[0]["payload"]["probe"], "restart-survival");
    assert_eq!(after_items[0]["payload"]["nonce"], nonce);
}
