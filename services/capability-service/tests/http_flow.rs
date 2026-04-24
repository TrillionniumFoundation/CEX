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
