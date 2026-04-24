use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use shared_config::{
    load_audit_scoped_admin_tokens, load_identity_scoped_admin_tokens, select_audit_read_token,
    select_identity_manage_token, select_identity_read_or_manage_token,
};
use std::{
    env,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::time::{sleep, Duration};

fn http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("build reqwest client")
}

fn scripts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("identity-service parent")
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
        ("audit", "http://127.0.0.1:7004/health"),
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
    health_once(client).await.expect("identity runtime health");
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
        "identity runtime failed health checks after restart: {}",
        last_error.unwrap_or_else(|| "unknown error".to_string())
    );
}

#[derive(Debug, Clone)]
struct RuntimeAdminPrincipal {
    token: String,
    actor_id: String,
    actor_label: String,
}

fn resolve_identity_admin_principal() -> RuntimeAdminPrincipal {
    let tokens = load_identity_scoped_admin_tokens();
    let token = select_identity_manage_token(&tokens)
        .expect("identity manage token")
        .clone();
    RuntimeAdminPrincipal {
        token: token.token,
        actor_id: token.actor_id,
        actor_label: token
            .actor_label
            .unwrap_or_else(|| "Identity Admin".to_string()),
    }
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

async fn post_json(client: &Client, url: &str, body: Value) -> (StatusCode, Value) {
    let response = client
        .post(url)
        .json(&body)
        .send()
        .await
        .expect("send POST request");
    let status = response.status();
    let body_text = response.text().await.expect("read POST response body");
    let json: Value = serde_json::from_str(&body_text).expect("decode POST response json");
    (status, json)
}

fn unique_label(prefix: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_nanos();
    format!("{prefix}-{now}")
}

async fn post_admin_json(client: &Client, url: &str, body: Value) -> (StatusCode, Value) {
    let response = client
        .post(url)
        .header("x-admin-token", resolve_identity_admin_principal().token)
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

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001"]
async fn identity_runtime_issue_list_revoke_resolve_matches_regression() {
    let client = http_client();
    let admin = resolve_identity_admin_principal();
    restart_runtime(&client).await;
    assert_health(&client).await;

    let issued = issue_api_key(&client, &unique_label("identity-blackbox-key")).await;
    let api_key = issued["api_key"].as_str().expect("issued api key");
    let api_key_id = issued["record"]["api_key_id"]
        .as_str()
        .expect("issued api key id");

    let (resolve_ok_status, resolve_ok) = post_json(
        &client,
        "http://127.0.0.1:7001/v1/auth/resolve",
        json!({ "api_key": api_key }),
    )
    .await;
    assert_eq!(resolve_ok_status, StatusCode::OK);
    assert_eq!(resolve_ok["org_id"], "00000000-0000-0000-0000-00000000ce01");

    let (list_status, listed) = get_identity_api_key_list_json(
        &client,
        "http://127.0.0.1:7001/v1/api-keys?org_id=00000000-0000-0000-0000-00000000ce01",
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    let items = listed["items"].as_array().expect("api key items array");
    assert!(items.iter().any(|item| item["api_key_id"] == api_key_id));

    let (audit_issue_status, audit_issue_events) = get_audit_admin_json(
        &client,
        &format!("http://127.0.0.1:7004/v1/audit/events/trace/{api_key_id}"),
    )
    .await;
    assert_eq!(audit_issue_status, StatusCode::OK);
    let audit_issue_items = audit_issue_events
        .as_array()
        .expect("issue audit events array");
    assert_eq!(audit_issue_items.len(), 1);
    assert_eq!(
        audit_issue_items[0]["event_type"],
        "identity.api_key.issued"
    );
    assert_eq!(audit_issue_items[0]["actor_id"], admin.actor_id);
    assert_eq!(audit_issue_items[0]["payload"]["api_key_id"], api_key_id);
    assert_eq!(
        audit_issue_items[0]["payload"]["key_prefix"],
        issued["record"]["key_prefix"]
    );
    assert_eq!(
        audit_issue_items[0]["payload"]["admin_actor_id"],
        admin.actor_id
    );
    assert_eq!(
        audit_issue_items[0]["payload"]["admin_actor_label"],
        admin.actor_label
    );
    assert!(audit_issue_items[0]["payload"].get("api_key").is_none());

    let (revoke_status, revoked) = post_admin_json(
        &client,
        &format!("http://127.0.0.1:7001/v1/api-keys/{api_key_id}/revoke"),
        json!({ "reason": "rotation" }),
    )
    .await;
    assert_eq!(revoke_status, StatusCode::OK);
    assert_eq!(revoked["status"], "revoked");
    assert_eq!(revoked["revoked_reason"], "rotation");

    let (audit_revoke_status, audit_revoke_events) = get_audit_admin_json(
        &client,
        &format!("http://127.0.0.1:7004/v1/audit/events/trace/{api_key_id}"),
    )
    .await;
    assert_eq!(audit_revoke_status, StatusCode::OK);
    let audit_revoke_items = audit_revoke_events
        .as_array()
        .expect("revoke audit events array");
    assert_eq!(audit_revoke_items.len(), 2);
    assert_eq!(
        audit_revoke_items[0]["event_type"],
        "identity.api_key.issued"
    );
    assert_eq!(
        audit_revoke_items[1]["event_type"],
        "identity.api_key.revoked"
    );
    assert_eq!(audit_revoke_items[1]["actor_id"], admin.actor_id);
    assert_eq!(
        audit_revoke_items[1]["payload"]["revoked_reason"],
        "rotation"
    );
    assert_eq!(
        audit_revoke_items[1]["payload"]["admin_actor_id"],
        admin.actor_id
    );
    assert_eq!(
        audit_revoke_items[1]["payload"]["admin_actor_label"],
        admin.actor_label
    );
    assert!(audit_revoke_items[1]["payload"].get("api_key").is_none());

    let (resolve_rejected_status, resolve_rejected) = post_json(
        &client,
        "http://127.0.0.1:7001/v1/auth/resolve",
        json!({ "api_key": api_key }),
    )
    .await;
    assert_eq!(resolve_rejected_status, StatusCode::UNAUTHORIZED);
    assert_eq!(resolve_rejected["error"], "revoked api key");
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7001"]
async fn identity_runtime_expired_key_matches_regression() {
    let client = http_client();
    let admin = resolve_identity_admin_principal();
    restart_runtime(&client).await;
    assert_health(&client).await;

    let issued = issue_expired_api_key(&client, &unique_label("identity-expired-key")).await;
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

    let (audit_status, audit_events) = get_audit_admin_json(
        &client,
        &format!("http://127.0.0.1:7004/v1/audit/events/trace/{api_key_id}"),
    )
    .await;
    assert_eq!(audit_status, StatusCode::OK);
    let audit_items = audit_events.as_array().expect("expired audit events array");
    assert_eq!(audit_items.len(), 1);
    assert_eq!(audit_items[0]["event_type"], "identity.api_key.issued");
    assert_eq!(audit_items[0]["actor_id"], admin.actor_id);
    assert_eq!(
        audit_items[0]["payload"]["expires_at"],
        "2000-01-01T00:00:00Z"
    );
    assert_eq!(audit_items[0]["payload"]["admin_actor_id"], admin.actor_id);
    assert_eq!(
        audit_items[0]["payload"]["admin_actor_label"],
        admin.actor_label
    );
    assert!(audit_items[0]["payload"].get("api_key").is_none());

    let (resolve_rejected_status, resolve_rejected) = post_json(
        &client,
        "http://127.0.0.1:7001/v1/auth/resolve",
        json!({ "api_key": api_key }),
    )
    .await;
    assert_eq!(resolve_rejected_status, StatusCode::UNAUTHORIZED);
    assert_eq!(resolve_rejected["error"], "expired api key");
}
