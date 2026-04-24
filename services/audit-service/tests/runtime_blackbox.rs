use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use shared_config::{load_audit_scoped_admin_tokens, select_audit_read_token};
use std::{path::PathBuf, process::Command};
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
        .expect("audit-service parent")
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
    let response = client
        .get("http://127.0.0.1:7004/health")
        .send()
        .await
        .map_err(|err| format!("health request failed for audit: {err}"))?;

    if response.status() != StatusCode::OK {
        return Err(format!(
            "health check failed for audit: status={} url=http://127.0.0.1:7004/health",
            response.status()
        ));
    }

    Ok(())
}

async fn assert_health(client: &Client) {
    health_once(client).await.expect("audit runtime health");
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
        "audit runtime failed health checks after restart: {}",
        last_error.unwrap_or_else(|| "unknown error".to_string())
    );
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

async fn get_json(client: &Client, url: &str) -> (StatusCode, Value) {
    let response = client.get(url).send().await.expect("send GET request");
    let status = response.status();
    let body_text = response.text().await.expect("read GET response body");
    let json: Value = serde_json::from_str(&body_text).expect("decode GET response json");
    (status, json)
}

async fn get_admin_json(client: &Client, url: &str) -> (StatusCode, Value) {
    let response = client
        .get(url)
        .header("x-admin-token", resolve_audit_admin_token())
        .send()
        .await
        .expect("send admin GET request");
    let status = response.status();
    let body_text = response.text().await.expect("read admin GET response body");
    let json: Value = serde_json::from_str(&body_text).expect("decode admin GET response json");
    (status, json)
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7004"]
async fn audit_runtime_trace_read_requires_admin_token() {
    let client = http_client();
    restart_runtime(&client).await;
    assert_health(&client).await;

    let trace_id = Uuid::new_v4();
    let (create_status, created) = post_json(
        &client,
        "http://127.0.0.1:7004/v1/audit/events",
        json!({
            "trace_id": trace_id,
            "actor_type": "identity-service",
            "actor_id": "local-dev-admin",
            "event_type": "identity.api_key.issued",
            "payload": {
                "api_key_id": trace_id,
                "key_prefix": "cex_pk_runtime"
            }
        }),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED);
    assert_eq!(created["trace_id"], trace_id.to_string());

    let (missing_status, missing_body) = get_json(
        &client,
        &format!("http://127.0.0.1:7004/v1/audit/events/trace/{trace_id}"),
    )
    .await;
    assert_eq!(missing_status, StatusCode::UNAUTHORIZED);
    assert_eq!(missing_body["error"], "missing admin token");

    let response = client
        .get(format!(
            "http://127.0.0.1:7004/v1/audit/events/trace/{trace_id}"
        ))
        .header("x-admin-token", "wrong-token")
        .send()
        .await
        .expect("send invalid admin GET request");
    let invalid_status = response.status();
    let invalid_text = response
        .text()
        .await
        .expect("read invalid admin response body");
    let invalid_body: Value =
        serde_json::from_str(&invalid_text).expect("decode invalid admin json");
    assert_eq!(invalid_status, StatusCode::UNAUTHORIZED);
    assert_eq!(invalid_body["error"], "invalid admin token");
}

#[tokio::test]
#[ignore = "requires detached local runtime on 127.0.0.1:7004"]
async fn audit_runtime_trace_read_accepts_configured_admin_token() {
    let client = http_client();
    restart_runtime(&client).await;
    assert_health(&client).await;

    let trace_id = Uuid::new_v4();
    let (create_status, created) = post_json(
        &client,
        "http://127.0.0.1:7004/v1/audit/events",
        json!({
            "trace_id": trace_id,
            "actor_type": "identity-service",
            "actor_id": "local-dev-admin",
            "event_type": "identity.api_key.revoked",
            "payload": {
                "api_key_id": trace_id,
                "revoked_reason": "rotation"
            }
        }),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED);
    assert_eq!(created["trace_id"], trace_id.to_string());

    let (list_status, listed) = get_admin_json(
        &client,
        &format!("http://127.0.0.1:7004/v1/audit/events/trace/{trace_id}"),
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    let items = listed.as_array().expect("audit events array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["event_type"], "identity.api_key.revoked");
    assert_eq!(items[0]["payload"]["api_key_id"], trace_id.to_string());
    assert_eq!(items[0]["payload"]["revoked_reason"], "rotation");
}
