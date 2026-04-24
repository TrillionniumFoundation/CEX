use audit_service::{build_router, state::AppState};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::util::ServiceExt;
use uuid::Uuid;

fn test_state() -> AppState {
    AppState::new_for_tests(false, Some("local-dev-admin-token".to_string()))
}

fn fail_fast_state() -> AppState {
    AppState::new_for_tests(true, Some("local-dev-admin-token".to_string()))
}

async fn send_json(app: Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
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

async fn get_json(app: Router, uri: &str) -> (StatusCode, Value) {
    get_json_with_headers(app, uri, &[]).await
}

async fn get_json_with_headers(
    app: Router,
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
        .expect("read response body");
    let json: Value = serde_json::from_slice(&bytes).expect("decode response json");
    (status, json)
}

#[tokio::test]
async fn create_event_and_list_by_trace_round_trip() {
    let app = build_router(test_state());
    let trace_id = Uuid::from_u128(0x11111111111111111111111111111111_u128);

    let (create_status, created) = send_json(
        app.clone(),
        "POST",
        "/v1/audit/events",
        json!({
            "trace_id": trace_id,
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "actor_type": "gateway-service",
            "actor_id": "local-dev-actor",
            "event_type": "invocation.created",
            "payload": { "invocation_id": "abc" }
        }),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED);
    assert_eq!(created["trace_id"], trace_id.to_string());
    assert_eq!(created["event_type"], "invocation.created");

    let (list_status, listed) =
        get_json(app.clone(), &format!("/v1/audit/events/trace/{trace_id}")).await;
    assert_eq!(list_status, StatusCode::UNAUTHORIZED);
    assert_eq!(listed["error"], "missing admin token");

    let (list_status, listed) = get_json_with_headers(
        app,
        &format!("/v1/audit/events/trace/{trace_id}"),
        &[("x-admin-token", "local-dev-admin-token")],
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    let items = listed.as_array().expect("audit events array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["event_id"], created["event_id"]);
    assert_eq!(items[0]["payload"]["invocation_id"], "abc");
}

#[tokio::test]
async fn list_by_trace_filters_other_traces_and_preserves_creation_order() {
    let app = build_router(test_state());
    let trace_a = Uuid::from_u128(0x22222222222222222222222222222222_u128);
    let trace_b = Uuid::from_u128(0x33333333333333333333333333333333_u128);

    let (_, first) = send_json(
        app.clone(),
        "POST",
        "/v1/audit/events",
        json!({
            "trace_id": trace_a,
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "actor_type": "gateway-service",
            "actor_id": null,
            "event_type": "invocation.created",
            "payload": { "step": 1 }
        }),
    )
    .await;

    let _ = send_json(
        app.clone(),
        "POST",
        "/v1/audit/events",
        json!({
            "trace_id": trace_b,
            "org_id": "00000000-0000-0000-0000-00000000ce02",
            "actor_type": "gateway-service",
            "actor_id": null,
            "event_type": "invocation.created",
            "payload": { "step": 999 }
        }),
    )
    .await;

    let (_, second) = send_json(
        app.clone(),
        "POST",
        "/v1/audit/events",
        json!({
            "trace_id": trace_a,
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "actor_type": "execution-service",
            "actor_id": null,
            "event_type": "execution.auto_approved",
            "payload": { "step": 2 }
        }),
    )
    .await;

    let (list_status, _listed) =
        get_json(app.clone(), &format!("/v1/audit/events/trace/{trace_a}")).await;
    assert_eq!(list_status, StatusCode::UNAUTHORIZED);

    let (list_status, listed) = get_json_with_headers(
        app,
        &format!("/v1/audit/events/trace/{trace_a}"),
        &[("authorization", "Bearer local-dev-admin-token")],
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);

    let items = listed.as_array().expect("audit events array");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["event_id"], first["event_id"]);
    assert_eq!(items[1]["event_id"], second["event_id"]);
    assert_eq!(items[0]["payload"]["step"], 1);
    assert_eq!(items[1]["payload"]["step"], 2);
}

#[tokio::test]
async fn fail_fast_without_pool_returns_service_unavailable() {
    let app = build_router(fail_fast_state());
    let trace_id = Uuid::from_u128(0x44444444444444444444444444444444_u128);

    let (create_status, create_body) = send_json(
        app.clone(),
        "POST",
        "/v1/audit/events",
        json!({
            "trace_id": trace_id,
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "actor_type": "gateway-service",
            "actor_id": null,
            "event_type": "invocation.created",
            "payload": { "step": 1 }
        }),
    )
    .await;
    assert_eq!(create_status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(create_body["error"]
        .as_str()
        .expect("create error")
        .contains("audit postgres pool not initialized"));

    let (list_status, _list_body) =
        get_json(app.clone(), &format!("/v1/audit/events/trace/{trace_id}")).await;
    assert_eq!(list_status, StatusCode::UNAUTHORIZED);

    let (list_status, list_body) = get_json_with_headers(
        app,
        &format!("/v1/audit/events/trace/{trace_id}"),
        &[("x-admin-token", "local-dev-admin-token")],
    )
    .await;
    assert_eq!(list_status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(list_body["error"]
        .as_str()
        .expect("list error")
        .contains("audit postgres pool not initialized"));
}

#[tokio::test]
async fn list_by_trace_rejects_admin_token_for_other_org() {
    let app = build_router(AppState::new_for_tests_with_admin_scope_orgs(
        false,
        Some("audit-read-token".to_string()),
        vec!["audit:read".to_string()],
        vec!["00000000-0000-0000-0000-00000000ce02".to_string()],
    ));
    let trace_id = Uuid::from_u128(0x54545454545454545454545454545454_u128);

    let (create_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/audit/events",
        json!({
            "trace_id": trace_id,
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "actor_type": "gateway-service",
            "actor_id": null,
            "event_type": "invocation.created",
            "payload": { "step": 1 }
        }),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED);

    let (status, body) = get_json_with_headers(
        app,
        &format!("/v1/audit/events/trace/{trace_id}"),
        &[("x-admin-token", "audit-read-token")],
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "admin token not authorized for org");
    assert_eq!(body["message"], "00000000-0000-0000-0000-00000000ce01");
}

#[tokio::test]
async fn list_by_trace_rejects_admin_token_without_audit_read_scope() {
    let app = build_router(AppState::new_for_tests_with_admin_scopes(
        false,
        Some("ops-token".to_string()),
        vec!["api_keys:manage".to_string()],
    ));
    let trace_id = Uuid::from_u128(0x55555555555555555555555555555555_u128);

    let (status, body) = get_json_with_headers(
        app,
        &format!("/v1/audit/events/trace/{trace_id}"),
        &[("x-admin-token", "ops-token")],
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "admin token lacks required scope");
    assert_eq!(body["message"], "audit:read");
}
