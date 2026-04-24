use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use shared_config::{
    load_execution_scoped_admin_tokens, load_ledger_scoped_admin_tokens,
    select_execution_manage_token, select_execution_read_or_manage_token,
    select_ledger_manage_token, select_ledger_read_or_manage_token,
};
use sqlx::{postgres::PgPoolOptions, Row};
use std::{env, fs, path::PathBuf, process::Command};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

#[derive(Debug)]
struct ApprovalRow {
    status: String,
    resolver_id: String,
    resolution: String,
    resolution_payload: Value,
}

fn http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build reqwest client")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("gateway-service parent")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn scripts_dir() -> PathBuf {
    repo_root().join("scripts")
}

fn database_url() -> String {
    if let Ok(url) = env::var("DATABASE_URL") {
        return url;
    }

    let env_path = repo_root().join(".env");
    let contents = fs::read_to_string(&env_path)
        .unwrap_or_else(|err| panic!("read {} failed: {err}", env_path.display()));

    contents
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("DATABASE_URL="))
        .map(str::to_string)
        .unwrap_or_else(|| panic!("DATABASE_URL not found in {}", env_path.display()))
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

fn resolve_execution_manage_token() -> String {
    select_execution_manage_token(&load_execution_scoped_admin_tokens())
        .map(|token| token.token.clone())
        .unwrap_or_else(|| "local-dev-admin-token".to_string())
}

fn resolve_execution_read_token() -> String {
    select_execution_read_or_manage_token(&load_execution_scoped_admin_tokens())
        .map(|token| token.token.clone())
        .unwrap_or_else(|| "local-dev-admin-token".to_string())
}

fn resolve_ledger_manage_token() -> String {
    select_ledger_manage_token(&load_ledger_scoped_admin_tokens())
        .map(|token| token.token.clone())
        .unwrap_or_else(|| "local-dev-admin-token".to_string())
}

fn resolve_ledger_read_token() -> String {
    select_ledger_read_or_manage_token(&load_ledger_scoped_admin_tokens())
        .map(|token| token.token.clone())
        .unwrap_or_else(|| "local-dev-admin-token".to_string())
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

async fn post_json(client: &Client, url: &str, body: Value) -> (StatusCode, Value) {
    let mut request = client.post(url);
    if url.starts_with("http://127.0.0.1:8080/") {
        request = request.header("x-api-key", "local-dev-key");
    }
    if url.starts_with("http://127.0.0.1:7002/") {
        request = request.header("x-admin-token", resolve_ledger_manage_token());
    }
    if url.starts_with("http://127.0.0.1:7003/") {
        request = request.header("x-admin-token", resolve_execution_manage_token());
    }
    let response = request.json(&body).send().await.expect("send POST request");
    let status = response.status();
    let body_text = response.text().await.expect("read POST response body");
    let json: Value = serde_json::from_str(&body_text).expect("decode POST response json");
    (status, json)
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

async fn approval_row_for_execution(execution_id: Uuid) -> ApprovalRow {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url())
        .await
        .expect("connect to postgres for approval probe");

    let row = sqlx::query(
        "select status, coalesce(resolver_id, '') as resolver_id, coalesce(resolution, '') as resolution, coalesce(resolution_payload, '{}'::jsonb)::text as resolution_payload from approvals where execution_id = $1 order by requested_at desc limit 1",
    )
    .bind(execution_id)
    .fetch_optional(&pool)
    .await
    .expect("query approval row")
    .unwrap_or_else(|| panic!("approval row not found for execution {execution_id}"));

    ApprovalRow {
        status: row.try_get::<String, _>("status").expect("approval status"),
        resolver_id: row
            .try_get::<String, _>("resolver_id")
            .expect("approval resolver_id"),
        resolution: row
            .try_get::<String, _>("resolution")
            .expect("approval resolution"),
        resolution_payload: serde_json::from_str(
            &row.try_get::<String, _>("resolution_payload")
                .expect("approval resolution_payload"),
        )
        .expect("decode approval resolution payload"),
    }
}

#[tokio::test]
#[ignore = "requires detached local runtime and direct postgres access"]
async fn runtime_approval_probe_reject_refund_matches_db_state() {
    let client = http_client();
    assert_health(&client).await;

    let reviewer = "runtime-db-probe-reviewer";
    let reason = "runtime approval probe reject refund";

    let account = create_account(&client, 100.0).await;
    let account_id = account["account_id"].as_str().expect("account id");
    let unique_suffix = Uuid::new_v4();

    let (invocation_status, invocation) = post_json(
        &client,
        "http://127.0.0.1:8080/v1/invocations",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_id": account_id,
            "prompt": format!("publish deployment package to external users approval probe reject {unique_suffix}"),
            "reserve_amount": 25.0
        }),
    )
    .await;
    assert_eq!(invocation_status, StatusCode::ACCEPTED);
    assert_eq!(invocation["status"], "AwaitingApproval");

    let invocation_id = invocation["invocation_id"].as_str().expect("invocation id");
    let execution_id = Uuid::parse_str(invocation["execution_id"].as_str().expect("execution id"))
        .expect("parse execution id");

    let pending_row = approval_row_for_execution(execution_id).await;
    assert_eq!(pending_row.status, "pending");
    assert_eq!(pending_row.resolver_id, "");
    assert_eq!(pending_row.resolution, "");

    let (reject_status, execution_after_reject) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{execution_id}/reject"),
        json!({
            "rejected_by": reviewer,
            "reason": reason
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
    assert_eq!(invocation_after_status, StatusCode::OK);
    assert_eq!(account_after_status, StatusCode::OK);
    assert_eq!(invocation_after["status"], "Refunded");
    assert_eq!(invocation_after["ledger_refunded"], true);
    assert_invocation_execution_snapshot_matches_execution(
        &invocation_after,
        &execution_after_reject,
    );
    assert_eq!(account_after["balance"], 100.0);
    assert_eq!(account_after["reserved"], 0.0);

    let rejected_row = approval_row_for_execution(execution_id).await;
    assert_eq!(rejected_row.status, "rejected");
    assert_eq!(rejected_row.resolver_id, reviewer);
    assert_eq!(rejected_row.resolution, reason);
    assert_eq!(rejected_row.resolution_payload["reason"], reason);

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

    let rejected_row_after_restart = approval_row_for_execution(execution_id).await;
    assert_eq!(rejected_row_after_restart.status, "rejected");
    assert_eq!(rejected_row_after_restart.resolver_id, reviewer);
    assert_eq!(rejected_row_after_restart.resolution, reason);
    assert_eq!(
        rejected_row_after_restart.resolution_payload["reason"],
        reason
    );
}

#[tokio::test]
#[ignore = "requires detached local runtime and direct postgres access"]
async fn runtime_approval_probe_workflow_persistence_matches_db_state() {
    let client = http_client();
    assert_health(&client).await;

    let approver = "runtime-db-probe-approver";
    let note = "runtime approval probe workflow persistence";

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
            "prompt": format!("workflow persistence auto path approval probe {auto_suffix}"),
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
            "prompt": format!("publish deployment package to external users workflow approval probe {approval_suffix}"),
            "reserve_amount": 25.0
        }),
    )
    .await;
    assert_eq!(approval_invocation_status, StatusCode::ACCEPTED);
    assert_eq!(approval_invocation["status"], "AwaitingApproval");
    let approval_invocation_id = approval_invocation["invocation_id"]
        .as_str()
        .expect("approval invocation id");
    let approval_execution_id = Uuid::parse_str(
        approval_invocation["execution_id"]
            .as_str()
            .expect("approval execution id"),
    )
    .expect("parse approval execution id");

    let pending_row = approval_row_for_execution(approval_execution_id).await;
    assert_eq!(pending_row.status, "pending");
    assert_eq!(pending_row.resolver_id, "");
    assert_eq!(pending_row.resolution, "");

    let (approve_status, approved_execution) = post_json(
        &client,
        &format!("http://127.0.0.1:7003/v1/executions/{approval_execution_id}/approve"),
        json!({
            "approved_by": approver,
            "note": note
        }),
    )
    .await;
    assert_eq!(approve_status, StatusCode::OK);
    assert_eq!(approved_execution["status"], "Queued");
    assert_eq!(approved_execution["approved_by"], approver);

    let approved_row = approval_row_for_execution(approval_execution_id).await;
    assert_eq!(approved_row.status, "approved");
    assert_eq!(approved_row.resolver_id, approver);
    assert_eq!(approved_row.resolution, note);
    assert_eq!(approved_row.resolution_payload["note"], note);

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
    assert_eq!(approval_execution_after["approved_by"], approver);
    assert_eq!(approval_account_after["balance"], 100.0);
    assert_eq!(approval_account_after["reserved"], 25.0);

    let approved_row_after_restart = approval_row_for_execution(approval_execution_id).await;
    assert_eq!(approved_row_after_restart.status, "approved");
    assert_eq!(approved_row_after_restart.resolver_id, approver);
    assert_eq!(approved_row_after_restart.resolution, note);
    assert_eq!(approved_row_after_restart.resolution_payload["note"], note);
}
