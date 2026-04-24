use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use identity_service::{build_router, AppState};
use serde_json::{json, Value};
use shared_types::AuthContext;
use std::collections::HashMap;
use tower::util::ServiceExt;

fn test_state(admin_token: Option<&str>) -> AppState {
    let mut api_keys = HashMap::new();
    api_keys.insert(
        "local-dev-key".to_string(),
        AuthContext {
            org_id: "00000000-0000-0000-0000-00000000ce01".to_string(),
            actor_id: Some("local-dev-actor".to_string()),
            actor_label: Some("Local Dev".to_string()),
        },
    );

    AppState::new_for_tests(api_keys, admin_token.map(str::to_string), None)
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

#[tokio::test]
async fn resolve_returns_static_api_key_context() {
    let app = build_router(test_state(Some("local-dev-admin-token")));

    let (status, body) = send_json_with_headers(
        app,
        "POST",
        "/v1/auth/resolve",
        &[],
        json!({ "api_key": "local-dev-key" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["org_id"], "00000000-0000-0000-0000-00000000ce01");
    assert_eq!(body["actor_id"], "local-dev-actor");
}

#[tokio::test]
async fn management_endpoints_require_admin_token() {
    let app = build_router(test_state(Some("local-dev-admin-token")));

    let (status, body) = get_json_with_headers(
        app,
        "/v1/api-keys?org_id=00000000-0000-0000-0000-00000000ce01",
        &[],
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "missing admin token");
}

#[tokio::test]
async fn management_endpoints_reject_invalid_admin_token() {
    let app = build_router(test_state(Some("local-dev-admin-token")));

    let (status, body) = send_json_with_headers(
        app,
        "POST",
        "/v1/api-keys",
        &[("x-admin-token", "wrong-token")],
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "label": "test"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "invalid admin token");
}

#[tokio::test]
async fn management_endpoints_reject_admin_token_without_manage_scope() {
    let mut api_keys = HashMap::new();
    api_keys.insert(
        "local-dev-key".to_string(),
        AuthContext {
            org_id: "00000000-0000-0000-0000-00000000ce01".to_string(),
            actor_id: Some("local-dev-actor".to_string()),
            actor_label: Some("Local Dev".to_string()),
        },
    );

    let app = build_router(AppState::new_for_tests_with_admin_scopes(
        api_keys,
        Some("audit-only-token".to_string()),
        None,
        None,
        vec!["audit:read".to_string()],
    ));

    let (status, body) = send_json_with_headers(
        app,
        "POST",
        "/v1/api-keys",
        &[("x-admin-token", "audit-only-token")],
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "label": "test"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "admin token lacks required scope");
    assert_eq!(body["message"], "api_keys:manage");
}

#[tokio::test]
async fn management_endpoints_reject_admin_token_for_other_org() {
    let mut api_keys = HashMap::new();
    api_keys.insert(
        "local-dev-key".to_string(),
        AuthContext {
            org_id: "00000000-0000-0000-0000-00000000ce01".to_string(),
            actor_id: Some("local-dev-actor".to_string()),
            actor_label: Some("Local Dev".to_string()),
        },
    );

    let app = build_router(AppState::new_for_tests_with_admin_scope_orgs(
        api_keys,
        Some("org-scoped-admin".to_string()),
        None,
        None,
        vec!["api_keys:manage".to_string()],
        vec!["00000000-0000-0000-0000-00000000ce02".to_string()],
    ));

    let (status, body) = send_json_with_headers(
        app,
        "POST",
        "/v1/api-keys",
        &[("x-admin-token", "org-scoped-admin")],
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "label": "test"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "admin token not authorized for org");
    assert_eq!(body["message"], "00000000-0000-0000-0000-00000000ce01");
}

#[tokio::test]
async fn list_endpoint_accepts_admin_token_with_read_scope_only() {
    let mut api_keys = HashMap::new();
    api_keys.insert(
        "local-dev-key".to_string(),
        AuthContext {
            org_id: "00000000-0000-0000-0000-00000000ce01".to_string(),
            actor_id: Some("local-dev-actor".to_string()),
            actor_label: Some("Local Dev".to_string()),
        },
    );

    let app = build_router(AppState::new_for_tests_with_admin_scopes(
        api_keys,
        Some("read-only-token".to_string()),
        None,
        None,
        vec!["api_keys:read".to_string()],
    ));

    let (status, body) = get_json_with_headers(
        app,
        "/v1/api-keys?org_id=00000000-0000-0000-0000-00000000ce01",
        &[("x-admin-token", "read-only-token")],
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body["error"],
        "database-backed api key management unavailable"
    );
}

#[tokio::test]
async fn list_endpoint_rejects_admin_token_for_other_org() {
    let mut api_keys = HashMap::new();
    api_keys.insert(
        "local-dev-key".to_string(),
        AuthContext {
            org_id: "00000000-0000-0000-0000-00000000ce01".to_string(),
            actor_id: Some("local-dev-actor".to_string()),
            actor_label: Some("Local Dev".to_string()),
        },
    );

    let app = build_router(AppState::new_for_tests_with_admin_scope_orgs(
        api_keys,
        Some("org-scoped-read-token".to_string()),
        None,
        None,
        vec!["api_keys:read".to_string()],
        vec!["00000000-0000-0000-0000-00000000ce02".to_string()],
    ));

    let (status, body) = get_json_with_headers(
        app,
        "/v1/api-keys?org_id=00000000-0000-0000-0000-00000000ce01",
        &[("x-admin-token", "org-scoped-read-token")],
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "admin token not authorized for org");
    assert_eq!(body["message"], "00000000-0000-0000-0000-00000000ce01");
}

#[tokio::test]
async fn list_endpoint_rejects_admin_token_without_read_or_manage_scope() {
    let mut api_keys = HashMap::new();
    api_keys.insert(
        "local-dev-key".to_string(),
        AuthContext {
            org_id: "00000000-0000-0000-0000-00000000ce01".to_string(),
            actor_id: Some("local-dev-actor".to_string()),
            actor_label: Some("Local Dev".to_string()),
        },
    );

    let app = build_router(AppState::new_for_tests_with_admin_scopes(
        api_keys,
        Some("audit-only-token".to_string()),
        None,
        None,
        vec!["audit:read".to_string()],
    ));

    let (status, body) = get_json_with_headers(
        app,
        "/v1/api-keys?org_id=00000000-0000-0000-0000-00000000ce01",
        &[("x-admin-token", "audit-only-token")],
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "admin token lacks required scope");
    assert_eq!(body["message"], "api_keys:read | api_keys:manage");
}

#[tokio::test]
async fn management_endpoints_accept_bearer_admin_token_before_db_check() {
    let app = build_router(test_state(Some("local-dev-admin-token")));

    let (status, body) = send_json_with_headers(
        app,
        "POST",
        "/v1/api-keys",
        &[("authorization", "Bearer local-dev-admin-token")],
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "label": "test"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body["error"],
        "database-backed api key management unavailable"
    );
}

#[tokio::test]
async fn management_endpoints_fail_closed_when_admin_token_not_configured() {
    let app = build_router(test_state(None));

    let (status, body) = get_json_with_headers(
        app,
        "/v1/api-keys?org_id=00000000-0000-0000-0000-00000000ce01",
        &[("x-admin-token", "anything")],
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "identity admin token not configured");
}
