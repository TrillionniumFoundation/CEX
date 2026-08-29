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
        kind: "model".to_string(),
        provider: "demo".to_string(),
        provider_ref: format!("demo/{id}"),
        display_name: format!("Capability {id}"),
        version: "v1".to_string(),
        description: Some("test capability".to_string()),
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
    let bytes = to_bytes(response.into_body(), usize::MAX)
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
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let text = String::from_utf8(bytes.to_vec()).expect("decode response text");
    (status, content_type, text)
}

#[tokio::test]
async fn metrics_endpoint_exports_registry_counts() {
    let app = build_router(AppState::new_for_tests(vec![
        sample_capability("cap.a", true),
        sample_capability("cap.b", false),
    ]));

    let (status, content_type, body) = get_text(app, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/plain; version=0.0.4"));
    assert!(body.contains("cex_capability_service_up 1\n"));
    assert!(body.contains("cex_capability_records_total{state=\"total\"} 2\n"));
    assert!(body.contains("cex_capability_records_total{state=\"enabled\"} 1\n"));
    assert!(body.contains("cex_capability_records_total{state=\"disabled\"} 1\n"));
}

#[tokio::test]
async fn list_capabilities_returns_sorted_records() {
    let app = build_router(AppState::new_for_tests(vec![
        sample_capability("cap.b", true),
        sample_capability("cap.a", true),
    ]));

    let (status, body) = get_json(app, "/v1/capabilities").await;
    assert_eq!(status, StatusCode::OK);
    let records = body.as_array().expect("array response");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["capability_id"], "cap.a");
    assert_eq!(records[1]["capability_id"], "cap.b");
}

#[tokio::test]
async fn get_capability_returns_record() {
    let app = build_router(AppState::new_for_tests(vec![sample_capability(
        "cap.demo", true,
    )]));

    let (status, body) = get_json(app, "/v1/capabilities/cap.demo").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["capability_id"], "cap.demo");
    assert_eq!(body["enabled"], true);
}

#[tokio::test]
async fn get_capability_returns_404_for_unknown_id() {
    let app = build_router(AppState::new_for_tests(vec![sample_capability(
        "cap.demo", true,
    )]));

    let (status, body) = get_json(app, "/v1/capabilities/unknown.cap").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "capability not found");
}
