use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use capability_service::{build_router, AppState};
use serde_json::Value;
use shared_types::CapabilityRecord;
use tower::util::ServiceExt;

fn sample_capability(id: &str, enabled: bool) -> CapabilityRecord {
    CapabilityRecord {
        capability_id: id.to_string(),
        kind: "external_agent_capability".to_string(),
        provider: "external-agent".to_string(),
        provider_ref: format!("did:trnm:agent-alpha#{id}"),
        display_name: format!("Capability {id}"),
        version: "hepta_agent_protocol_v1".to_string(),
        description: Some("test external Agent capability declaration".to_string()),
        enabled,
    }
}

async fn get_json(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request");

    let response = app.oneshot(request).await.expect("router response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1_048_576)
        .await
        .expect("read response body");
    let json = serde_json::from_slice(&bytes).expect("decode response json");
    (status, json)
}

async fn get_text(app: axum::Router, uri: &str) -> (StatusCode, String, String) {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request");

    let response = app.oneshot(request).await.expect("router response");
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = to_bytes(response.into_body(), 1_048_576)
        .await
        .expect("read response body");
    let text = String::from_utf8(bytes.to_vec()).expect("decode response text");
    (status, content_type, text)
}

#[tokio::test]
async fn health_reports_external_only_ready_registry() {
    let app = build_router(AppState::new_for_tests(vec![sample_capability(
        "cap.external-agent.alpha",
        true,
    )]));

    let (status, body) = get_json(app, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ready"], true);
    assert_eq!(body["runtime_policy"], "external_only");
    assert_eq!(body["record_count"], 1);
    assert_eq!(body["production_authorization"], "not_granted");
}

#[tokio::test]
async fn empty_registry_is_not_ready() {
    let app = build_router(AppState::new_empty_for_tests());
    let (status, body) = get_json(app, "/health").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["ready"], false);
    assert_eq!(body["record_count"], 0);
}

#[tokio::test]
async fn metrics_endpoint_exports_registry_counts() {
    let app = build_router(AppState::new_for_tests(vec![
        sample_capability("cap.external-agent.b", true),
        sample_capability("cap.external-agent.a", false),
    ]));

    let (status, content_type, body) = get_text(app, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/plain; version=0.0.4"));
    assert!(body.contains("cex_capability_service_up 1\n"));
    assert!(body.contains("cex_capability_service_ready 1\n"));
    assert!(body.contains("cex_capability_records_total{state=\"total\"} 2\n"));
    assert!(body.contains("cex_capability_records_total{state=\"enabled\"} 1\n"));
    assert!(body.contains("cex_capability_records_total{state=\"disabled\"} 1\n"));
}

#[tokio::test]
async fn list_capabilities_returns_sorted_records() {
    let app = build_router(AppState::new_for_tests(vec![
        sample_capability("cap.external-agent.b", true),
        sample_capability("cap.external-agent.a", true),
    ]));

    let (status, body) = get_json(app, "/v1/capabilities").await;
    assert_eq!(status, StatusCode::OK);
    let records = body.as_array().expect("array response");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["capability_id"], "cap.external-agent.a");
    assert_eq!(records[1]["capability_id"], "cap.external-agent.b");
    assert_eq!(records[0]["kind"], "external_agent_capability");
}

#[tokio::test]
async fn get_capability_returns_record() {
    let app = build_router(AppState::new_for_tests(vec![sample_capability(
        "cap.external-agent.demo",
        true,
    )]));

    let (status, body) = get_json(app, "/v1/capabilities/cap.external-agent.demo").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["capability_id"], "cap.external-agent.demo");
    assert_eq!(body["provider"], "external-agent");
    assert_eq!(body["enabled"], true);
}

#[tokio::test]
async fn get_capability_returns_404_for_unknown_id() {
    let app = build_router(AppState::new_for_tests(vec![sample_capability(
        "cap.external-agent.demo",
        true,
    )]));

    let (status, body) = get_json(app, "/v1/capabilities/unknown.cap").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "capability not found");
}
