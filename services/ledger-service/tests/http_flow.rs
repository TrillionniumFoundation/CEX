use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use ledger_service::{
    build_router, repository::postgres::PostgresLedgerRepository, state::AppState,
};
use serde_json::{json, Value};
use tower::util::ServiceExt;

fn test_state() -> AppState {
    AppState::new_for_tests(
        PostgresLedgerRepository::new_placeholder(),
        false,
        Some("local-dev-admin-token".to_string()),
        vec!["ledger:manage".to_string(), "ledger:read".to_string()],
        Vec::new(),
    )
}

async fn send_json_with_headers(
    app: Router,
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
        .expect("read response body");
    let json: Value = serde_json::from_slice(&bytes).expect("decode response json");
    (status, json)
}

async fn send_json(app: Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
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

async fn get_json(app: Router, uri: &str) -> (StatusCode, Value) {
    get_json_with_headers(app, uri, &[("x-admin-token", "local-dev-admin-token")]).await
}

async fn get_text(app: Router, uri: &str) -> (StatusCode, String, String) {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build get request");

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
async fn metrics_endpoint_exports_ledger_runtime_gauges() {
    let app = build_router(test_state());

    let (status, content_type, body) = get_text(app, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/plain; version=0.0.4"));
    assert!(body.contains("cex_ledger_service_up 1\n"));
    assert!(body.contains("cex_ledger_memory_accounts_total 0\n"));
    assert!(body.contains("cex_ledger_memory_entries_total 0\n"));
    assert!(body.contains("cex_ledger_admin_tokens_total 1\n"));
}

async fn create_account(app: Router, initial_balance: f64) -> (String, Value) {
    let (status, created) = send_json(
        app,
        "POST",
        "/v1/accounts",
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_type": "user",
            "currency_unit": "credit",
            "initial_balance": initial_balance
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    (
        created["account_id"]
            .as_str()
            .expect("account id")
            .to_string(),
        created,
    )
}

#[tokio::test]
async fn create_account_requires_admin_token() {
    let app = build_router(test_state());
    let (status, body) = send_json_with_headers(
        app,
        "POST",
        "/v1/accounts",
        &[],
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_type": "user",
            "currency_unit": "credit",
            "initial_balance": 100.0
        }),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "missing admin token");
}

#[tokio::test]
async fn create_account_and_get_round_trip() {
    let app = build_router(test_state());
    let (account_id, created) = create_account(app.clone(), 100.0).await;
    assert_eq!(created["balance"], 100.0);
    assert_eq!(created["reserved"], 0.0);
    assert_eq!(created["currency_unit"], "credit");

    let (status, fetched) = get_json(app, &format!("/v1/accounts/{account_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["account_id"], created["account_id"]);
    assert_eq!(fetched["balance"], 100.0);
    assert_eq!(fetched["reserved"], 0.0);
}

#[tokio::test]
async fn ledger_endpoints_reject_admin_token_for_other_org() {
    let app = build_router(AppState::new_for_tests(
        PostgresLedgerRepository::new_placeholder(),
        false,
        Some("ledger-admin".to_string()),
        vec!["ledger:manage".to_string(), "ledger:read".to_string()],
        vec!["00000000-0000-0000-0000-00000000ce02".to_string()],
    ));

    let (status, body) = send_json_with_headers(
        app,
        "POST",
        "/v1/accounts",
        &[("x-admin-token", "ledger-admin")],
        json!({
            "org_id": "00000000-0000-0000-0000-00000000ce01",
            "account_type": "user",
            "currency_unit": "credit",
            "initial_balance": 100.0
        }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "admin token not authorized for org");
    assert_eq!(body["message"], "00000000-0000-0000-0000-00000000ce01");
}

#[tokio::test]
async fn grant_adds_balance_without_reserved_funds() {
    let app = build_router(test_state());
    let (account_id, _) = create_account(app.clone(), 100.0).await;

    let (grant_status, grant_json) = send_json(
        app.clone(),
        "POST",
        "/v1/ledger/grant",
        json!({
            "account_id": account_id,
            "amount": 3.15,
            "reference_id": "league-reward-test",
            "idempotency_key": "league-reward-key-1"
        }),
    )
    .await;
    assert_eq!(grant_status, StatusCode::OK);
    assert_eq!(grant_json["account"]["balance"], 103.15);
    assert_eq!(grant_json["account"]["reserved"], 0.0);
    assert_eq!(grant_json["entry"]["action"], "grant");

    let (get_status, fetched) = get_json(app, &format!("/v1/accounts/{account_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["balance"], 103.15);
    assert_eq!(fetched["reserved"], 0.0);
}

#[tokio::test]
async fn reserve_then_consume_updates_reserved_and_balance() {
    let app = build_router(test_state());
    let (account_id, _) = create_account(app.clone(), 100.0).await;

    let (reserve_status, reserve_json) = send_json(
        app.clone(),
        "POST",
        "/v1/ledger/reserve",
        json!({
            "account_id": account_id,
            "amount": 5.0,
            "reference_id": "reserve-test",
            "idempotency_key": "reserve-key-1"
        }),
    )
    .await;
    assert_eq!(reserve_status, StatusCode::OK);
    assert_eq!(reserve_json["account"]["balance"], 100.0);
    assert_eq!(reserve_json["account"]["reserved"], 5.0);

    let (consume_status, consume_json) = send_json(
        app.clone(),
        "POST",
        "/v1/ledger/consume",
        json!({
            "account_id": account_id,
            "amount": 5.0,
            "reference_id": "consume-test",
            "idempotency_key": "consume-key-1"
        }),
    )
    .await;
    assert_eq!(consume_status, StatusCode::OK);
    assert_eq!(consume_json["account"]["balance"], 95.0);
    assert_eq!(consume_json["account"]["reserved"], 0.0);

    let (get_status, fetched) = get_json(app, &format!("/v1/accounts/{account_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["balance"], 95.0);
    assert_eq!(fetched["reserved"], 0.0);
}

#[tokio::test]
async fn reserve_then_refund_releases_reserved_without_minting_balance() {
    let app = build_router(test_state());
    let (account_id, _) = create_account(app.clone(), 100.0).await;

    let (reserve_status, reserve_json) = send_json(
        app.clone(),
        "POST",
        "/v1/ledger/reserve",
        json!({
            "account_id": account_id,
            "amount": 7.0,
            "reference_id": "refund-test",
            "idempotency_key": "reserve-key-2"
        }),
    )
    .await;
    assert_eq!(reserve_status, StatusCode::OK);
    assert_eq!(reserve_json["account"]["balance"], 100.0);
    assert_eq!(reserve_json["account"]["reserved"], 7.0);

    let (refund_status, refund_json) = send_json(
        app.clone(),
        "POST",
        "/v1/ledger/refund",
        json!({
            "account_id": account_id,
            "amount": 7.0,
            "reference_id": "refund-test",
            "idempotency_key": "refund-key-2"
        }),
    )
    .await;
    assert_eq!(refund_status, StatusCode::OK);
    assert_eq!(refund_json["account"]["balance"], 100.0);
    assert_eq!(refund_json["account"]["reserved"], 0.0);

    let (get_status, fetched) = get_json(app, &format!("/v1/accounts/{account_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["balance"], 100.0);
    assert_eq!(fetched["reserved"], 0.0);
}

#[tokio::test]
async fn duplicate_idempotency_key_returns_conflict_without_double_reserve() {
    let app = build_router(test_state());
    let (account_id, _) = create_account(app.clone(), 100.0).await;

    let request_body = json!({
        "account_id": account_id,
        "amount": 8.0,
        "reference_id": "dup-test",
        "idempotency_key": "dup-key-1"
    });

    let (first_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/ledger/reserve",
        request_body.clone(),
    )
    .await;
    let (second_status, second_json) =
        send_json(app.clone(), "POST", "/v1/ledger/reserve", request_body).await;
    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(second_status, StatusCode::CONFLICT);
    assert_eq!(second_json["error"], "duplicate idempotency key");

    let (get_status, fetched) = get_json(app, &format!("/v1/accounts/{account_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["balance"], 100.0);
    assert_eq!(fetched["reserved"], 8.0);
}

#[tokio::test]
async fn insufficient_amount_paths_return_bad_request() {
    let app = build_router(test_state());
    let (account_id, _) = create_account(app.clone(), 100.0).await;

    let (reserve_status, reserve_json) = send_json(
        app.clone(),
        "POST",
        "/v1/ledger/reserve",
        json!({
            "account_id": account_id,
            "amount": 150.0,
            "reference_id": "too-much-reserve",
            "idempotency_key": "reserve-too-much"
        }),
    )
    .await;
    assert_eq!(reserve_status, StatusCode::BAD_REQUEST);
    assert!(reserve_json["error"]
        .as_str()
        .expect("reserve error")
        .contains("insufficient available balance"));

    let (grant_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/ledger/grant",
        json!({
            "account_id": account_id,
            "amount": 100.0,
            "reference_id": "seed-retry-reserve-after-failure",
            "idempotency_key": "seed-retry-reserve-after-failure"
        }),
    )
    .await;
    assert_eq!(grant_status, StatusCode::OK);

    let (reserve_retry_status, reserve_retry_json) = send_json(
        app.clone(),
        "POST",
        "/v1/ledger/reserve",
        json!({
            "account_id": account_id,
            "amount": 150.0,
            "reference_id": "too-much-reserve",
            "idempotency_key": "reserve-too-much"
        }),
    )
    .await;
    assert_eq!(reserve_retry_status, StatusCode::OK);
    assert_eq!(reserve_retry_json["account"]["balance"], 200.0);
    assert_eq!(reserve_retry_json["account"]["reserved"], 150.0);

    let (refund_account_id, _) = create_account(app.clone(), 100.0).await;
    let refund_body = json!({
        "account_id": refund_account_id,
        "amount": 1.0,
        "reference_id": "refund-without-reserve",
        "idempotency_key": "refund-without-reserve"
    });
    let (refund_status, refund_json) = send_json(
        app.clone(),
        "POST",
        "/v1/ledger/refund",
        refund_body.clone(),
    )
    .await;
    assert_eq!(refund_status, StatusCode::BAD_REQUEST);
    assert!(refund_json["error"]
        .as_str()
        .expect("refund error")
        .contains("insufficient reserved balance"));

    let (reserve_refund_retry_status, _) = send_json(
        app.clone(),
        "POST",
        "/v1/ledger/reserve",
        json!({
            "account_id": refund_account_id,
            "amount": 1.0,
            "reference_id": "seed-retry-refund-after-failure",
            "idempotency_key": "seed-retry-refund-after-failure"
        }),
    )
    .await;
    assert_eq!(reserve_refund_retry_status, StatusCode::OK);

    let (refund_retry_status, refund_retry_json) =
        send_json(app.clone(), "POST", "/v1/ledger/refund", refund_body).await;
    assert_eq!(refund_retry_status, StatusCode::OK);
    assert_eq!(refund_retry_json["account"]["balance"], 100.0);
    assert_eq!(refund_retry_json["account"]["reserved"], 0.0);
}
