use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use ledger_service::{
    build_router, repository::postgres::PostgresLedgerRepository, state::AppState,
};
use serde_json::{json, Value};
use term_exchange_protocol::TERM_EXCHANGE_PROTOCOL_VERSION;
use tower::util::ServiceExt;
use uuid::Uuid;

fn test_state() -> AppState {
    AppState::new_for_tests(
        PostgresLedgerRepository::new_placeholder(),
        false,
        Some("local-dev-admin-token".to_string()),
        vec!["ledger:manage".to_string(), "ledger:read".to_string()],
        Vec::new(),
    )
}

fn persistence_required_test_state() -> AppState {
    AppState::new_for_tests_configured(
        PostgresLedgerRepository::new_placeholder(),
        false,
        Some("local-dev-admin-token".to_string()),
        vec!["ledger:manage".to_string(), "ledger:read".to_string()],
        Vec::new(),
        false,
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

const TEST_ORG_ID: &str = "00000000-0000-0000-0000-00000000ce01";
const TEST_SCALE: u8 = 6;

fn exact_account_request(account_id: Uuid, opening_minor: i64) -> Value {
    json!({
        "account_id": account_id,
        "org_id": TEST_ORG_ID,
        "trace_id": Uuid::new_v4(),
        "account_type": "user",
        "currency_unit": "credit",
        "currency_scale": TEST_SCALE,
        "opening_minor": opening_minor.to_string(),
        "idempotency_scope": format!("http-flow:account:{account_id}"),
        "idempotency_key": format!("open:{account_id}"),
    })
}

async fn create_account(app: Router, opening_minor: i64) -> (Uuid, Value) {
    let account_id = Uuid::new_v4();
    let (status, created) = send_json(
        app,
        "POST",
        "/v2/accounts",
        exact_account_request(account_id, opening_minor),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["account"]["account_id"], account_id.to_string());
    (account_id, created)
}

fn exact_effect_request(
    account_id: Uuid,
    operation_kind: &str,
    amount_minor: i64,
    idempotency_key: &str,
) -> Value {
    json!({
        "account_id": account_id,
        "trace_id": Uuid::new_v4(),
        "operation_id": Uuid::new_v4(),
        "operation_kind": operation_kind,
        "currency_unit": "credit",
        "currency_scale": TEST_SCALE,
        "amount_minor": amount_minor.to_string(),
        "reference_type": "http_flow",
        "reference_id": Uuid::new_v4(),
        "idempotency_scope": format!("http-flow:{account_id}:{operation_kind}"),
        "idempotency_key": idempotency_key,
    })
}

fn native_intent_request(account_id: Uuid) -> Value {
    json!({
        "intent": {
            "protocol_version": TERM_EXCHANGE_PROTOCOL_VERSION,
            "intent_id": format!("placeholder-intent-{account_id}"),
            "term_id": "placeholder-test-term",
            "term_version": "1",
            "domain": "trnm_game",
            "kind": "reserve",
            "idempotency_key": {
                "scope": "placeholder-test",
                "key": format!("reserve-{account_id}")
            },
            "actors": [{
                "actor_id": "placeholder-player",
                "actor_kind": "player",
                "account_id": account_id.to_string()
            }],
            "assets": [],
            "amount_credits": 1,
            "currency": "wallet_credits",
            "metadata": {},
            "created_at_epoch": 0
        }
    })
}

#[tokio::test]
async fn create_account_requires_admin_token() {
    let app = build_router(test_state());
    let account_id = Uuid::new_v4();
    let (status, body) = send_json_with_headers(
        app,
        "POST",
        "/v2/accounts",
        &[],
        exact_account_request(account_id, 100_000_000),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "missing admin token");
}

#[tokio::test]
async fn trnm_session_verify_requires_a_signed_player_session() {
    let app = build_router(test_state());
    let (status, body) = send_json_with_headers(
        app,
        "POST",
        "/v1/trnm/identity/session/verify",
        &[],
        json!({
            "player_id": "player-a",
            "account_id": "00000000-0000-0000-0000-000000000001"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!body["error"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn only_the_scoped_game_authority_may_issue_value_entitlements() {
    let body = json!({
        "actor_id": "player-a",
        "account_id": "00000000-0000-0000-0000-000000000001",
        "source": "battle",
        "source_id": "battle-a",
        "intent_id": "intent-a",
        "amount_credits": 25
    });
    let (admin_status, _) = send_json_with_headers(
        build_router(test_state()),
        "POST",
        "/v1/trnm/economy/entitlements",
        &[("x-admin-token", "local-dev-admin-token")],
        body.clone(),
    )
    .await;
    assert_eq!(admin_status, StatusCode::UNAUTHORIZED);

    let (authority_status, entitlement) = send_json_with_headers(
        build_router(test_state()),
        "POST",
        "/v1/trnm/economy/entitlements",
        &[("x-trnm-game-authority", "test-game-authority-token")],
        body,
    )
    .await;
    assert_eq!(authority_status, StatusCode::CREATED);
    assert_eq!(entitlement["actor_id"], "player-a");
    assert_eq!(entitlement["amount_credits"], 25);
}

#[tokio::test]
async fn game_authority_can_verify_the_active_online_issuer_fingerprint() {
    let body = json!({"key_id": "test-online-ed25519-v1"});
    let (anonymous_status, _) = send_json_with_headers(
        build_router(test_state()),
        "POST",
        "/v1/trnm/economy/issuer-keys/status",
        &[],
        body.clone(),
    )
    .await;
    assert_eq!(anonymous_status, StatusCode::UNAUTHORIZED);
    let (status, key) = send_json_with_headers(
        build_router(test_state()),
        "POST",
        "/v1/trnm/economy/issuer-keys/status",
        &[("x-trnm-game-authority", "test-game-authority-token")],
        body,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(key["key_id"], "test-online-ed25519-v1");
    assert_eq!(key["issuer"], "trnm-online-game-server");
    assert_eq!(key["status"], "active");
    assert_eq!(key["signature_algorithm"], "ed25519");
    assert_eq!(key["public_key_sha256"].as_str().unwrap().len(), 64);
}

#[tokio::test]
async fn create_account_and_get_round_trip() {
    let app = build_router(test_state());
    let (account_id, created) = create_account(app.clone(), 100_000_000).await;
    assert_eq!(created["account"]["balance_minor"], "100000000");
    assert_eq!(created["account"]["reserved_minor"], "0");
    assert_eq!(created["account"]["currency_unit"], "credit");
    assert_eq!(created["opening"]["opening_kind"], "genesis");

    let (status, fetched) = get_json(app, &format!("/v2/accounts/{account_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["account_id"], account_id.to_string());
    assert_eq!(fetched["balance_minor"], "100000000");
    assert_eq!(fetched["reserved_minor"], "0");
    assert_eq!(fetched["schema_version"], "cex.account.money.v2");
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
    let account_id = Uuid::new_v4();

    let (status, body) = send_json_with_headers(
        app,
        "POST",
        "/v2/accounts",
        &[("x-admin-token", "ledger-admin")],
        exact_account_request(account_id, 100_000_000),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "ledger_org_forbidden");
    assert_eq!(
        body["message"],
        "authenticated principal is not authorized for this organization"
    );
}

#[tokio::test]
async fn fail_fast_placeholder_repository_rejects_memory_only_account_creation() {
    let state = AppState::new_for_tests(
        PostgresLedgerRepository::new_placeholder(),
        true,
        Some("local-dev-admin-token".to_string()),
        vec!["ledger:manage".to_string(), "ledger:read".to_string()],
        Vec::new(),
    );
    let app = build_router(state.clone());
    let account_id = Uuid::new_v4();

    let (status, body) = send_json(
        app,
        "POST",
        "/v2/accounts",
        exact_account_request(account_id, 100_000_000),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "ledger_operation_persistence_unavailable");
    assert!(state.accounts.read().await.is_empty());
    assert!(state
        .exact_memory
        .read()
        .await
        .account_openings_by_account
        .is_empty());
}

#[tokio::test]
async fn placeholder_without_explicit_test_opt_in_rejects_exact_account_opening() {
    let state = persistence_required_test_state();
    let app = build_router(state.clone());
    let account_id = Uuid::new_v4();

    let (status, body) = send_json(
        app,
        "POST",
        "/v2/accounts",
        exact_account_request(account_id, 100_000_000),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "ledger_operation_persistence_unavailable");
    assert!(state.accounts.read().await.is_empty());
    assert!(state.entries.read().await.is_empty());
    assert!(state
        .exact_memory
        .read()
        .await
        .account_openings_by_account
        .is_empty());
}

#[tokio::test]
async fn placeholder_without_explicit_test_opt_in_rejects_exact_effect() {
    let state = persistence_required_test_state();
    let app = build_router(state.clone());
    let account_id = Uuid::new_v4();

    let (status, body) = send_json(
        app,
        "POST",
        "/v2/ledger/effects",
        exact_effect_request(account_id, "grant", 1_000_000, "placeholder-effect"),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "ledger_operation_persistence_unavailable");
    assert!(state.accounts.read().await.is_empty());
    assert!(state.entries.read().await.is_empty());
    assert!(state
        .exact_memory
        .read()
        .await
        .effects_by_operation
        .is_empty());
}

#[tokio::test]
async fn placeholder_without_explicit_test_opt_in_rejects_native_intent() {
    let state = persistence_required_test_state();
    let app = build_router(state.clone());
    let account_id = Uuid::new_v4();

    let (status, _) = send_json(
        app,
        "POST",
        "/v1/trnm/economy/intents",
        native_intent_request(account_id),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(state.accounts.read().await.is_empty());
    assert!(state.entries.read().await.is_empty());
    assert!(state.idempotency_keys.read().await.is_empty());
}

#[tokio::test]
async fn placeholder_without_explicit_test_opt_in_rejects_wallet_snapshot_fallback() {
    let state = persistence_required_test_state();
    let app = build_router(state.clone());
    let account_id = Uuid::new_v4();

    let (status, _) = send_json(
        app,
        "POST",
        "/v1/trnm/economy/wallet",
        json!({
            "actor_id": "placeholder-player",
            "account_id": account_id,
            "reconciliation_cursor": 1
        }),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(state.accounts.read().await.is_empty());
    assert!(state.entries.read().await.is_empty());
}

#[tokio::test]
async fn placeholder_without_explicit_test_opt_in_rejects_value_entitlement_issue() {
    let state = persistence_required_test_state();
    let app = build_router(state.clone());
    let account_id = Uuid::new_v4();

    let (status, _) = send_json_with_headers(
        app,
        "POST",
        "/v1/trnm/economy/entitlements",
        &[("x-trnm-game-authority", "test-game-authority-token")],
        json!({
            "actor_id": "placeholder-player",
            "account_id": account_id,
            "source": "battle",
            "source_id": "battle-id",
            "intent_id": "placeholder-intent",
            "amount_credits": 1
        }),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(state.accounts.read().await.is_empty());
    assert!(state.entries.read().await.is_empty());
}

#[tokio::test]
async fn grant_adds_balance_without_reserved_funds() {
    let app = build_router(test_state());
    let (account_id, _) = create_account(app.clone(), 100_000_000).await;

    let (grant_status, grant_json) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        exact_effect_request(account_id, "grant", 3_150_000, "league-reward-key-1"),
    )
    .await;
    assert_eq!(grant_status, StatusCode::CREATED);
    assert_eq!(grant_json["account"]["balance_minor"], 103_150_000);
    assert_eq!(grant_json["account"]["reserved_minor"], 0);
    assert_eq!(grant_json["effect"]["operation_kind"], "grant");
    assert_eq!(grant_json["effect"]["amount_minor"], 3_150_000);

    let (get_status, fetched) = get_json(app, &format!("/v2/accounts/{account_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["balance_minor"], "103150000");
    assert_eq!(fetched["reserved_minor"], "0");
}

#[tokio::test]
async fn reserve_then_consume_updates_reserved_and_balance() {
    let app = build_router(test_state());
    let (account_id, _) = create_account(app.clone(), 100_000_000).await;

    let (reserve_status, reserve_json) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        exact_effect_request(account_id, "reserve", 5_000_000, "reserve-key-1"),
    )
    .await;
    assert_eq!(reserve_status, StatusCode::CREATED);
    assert_eq!(reserve_json["account"]["balance_minor"], 100_000_000);
    assert_eq!(reserve_json["account"]["reserved_minor"], 5_000_000);

    let (consume_status, consume_json) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        exact_effect_request(account_id, "consume", 5_000_000, "consume-key-1"),
    )
    .await;
    assert_eq!(consume_status, StatusCode::CREATED);
    assert_eq!(consume_json["account"]["balance_minor"], 95_000_000);
    assert_eq!(consume_json["account"]["reserved_minor"], 0);

    let (get_status, fetched) = get_json(app, &format!("/v2/accounts/{account_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["balance_minor"], "95000000");
    assert_eq!(fetched["reserved_minor"], "0");
}

#[tokio::test]
async fn reserve_then_refund_releases_reserved_without_minting_balance() {
    let app = build_router(test_state());
    let (account_id, _) = create_account(app.clone(), 100_000_000).await;

    let (reserve_status, reserve_json) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        exact_effect_request(account_id, "reserve", 7_000_000, "reserve-key-2"),
    )
    .await;
    assert_eq!(reserve_status, StatusCode::CREATED);
    assert_eq!(reserve_json["account"]["balance_minor"], 100_000_000);
    assert_eq!(reserve_json["account"]["reserved_minor"], 7_000_000);

    let (refund_status, refund_json) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        exact_effect_request(account_id, "refund", 7_000_000, "refund-key-2"),
    )
    .await;
    assert_eq!(refund_status, StatusCode::CREATED);
    assert_eq!(refund_json["account"]["balance_minor"], 100_000_000);
    assert_eq!(refund_json["account"]["reserved_minor"], 0);

    let (get_status, fetched) = get_json(app, &format!("/v2/accounts/{account_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["balance_minor"], "100000000");
    assert_eq!(fetched["reserved_minor"], "0");
}

#[tokio::test]
async fn exact_idempotency_replays_without_double_reserve() {
    let app = build_router(test_state());
    let (account_id, _) = create_account(app.clone(), 100_000_000).await;
    let request_body = exact_effect_request(account_id, "reserve", 8_000_000, "dup-key-1");

    let (first_status, first_json) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        request_body.clone(),
    )
    .await;
    let (second_status, second_json) =
        send_json(app.clone(), "POST", "/v2/ledger/effects", request_body).await;

    assert_eq!(first_status, StatusCode::CREATED);
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(first_json["replayed"], false);
    assert_eq!(second_json["replayed"], true);
    assert_eq!(
        second_json["effect"]["entry_id"],
        first_json["effect"]["entry_id"]
    );

    let (get_status, fetched) = get_json(app, &format!("/v2/accounts/{account_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["balance_minor"], "100000000");
    assert_eq!(fetched["reserved_minor"], "8000000");
}

#[tokio::test]
async fn retired_legacy_value_routes_fail_closed_without_memory_mutation() {
    let state = test_state();
    let app = build_router(state.clone());

    let (account_status, account_body) =
        send_json(app.clone(), "POST", "/v1/accounts", json!({"legacy": true})).await;
    assert_eq!(account_status, StatusCode::GONE);
    assert_eq!(account_body["code"], "ledger_v1_value_write_gone");

    let (effect_status, effect_body) =
        send_json(app, "POST", "/v1/ledger/reserve", json!({"legacy": true})).await;
    assert_eq!(effect_status, StatusCode::GONE);
    assert_eq!(effect_body["code"], "ledger_v1_value_write_gone");
    assert!(state.accounts.read().await.is_empty());
    assert!(state.entries.read().await.is_empty());
    assert!(state.idempotency_keys.read().await.is_empty());
    assert!(state
        .exact_memory
        .read()
        .await
        .effects_by_operation
        .is_empty());
}

#[tokio::test]
async fn insufficient_exact_amount_paths_are_retryable_after_state_changes() {
    let app = build_router(test_state());
    let (account_id, _) = create_account(app.clone(), 100_000_000).await;
    let reserve_body = exact_effect_request(account_id, "reserve", 150_000_000, "reserve-too-much");

    let (reserve_status, reserve_json) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        reserve_body.clone(),
    )
    .await;
    assert_eq!(reserve_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(reserve_json["code"], "ledger_insufficient_funds");

    let (grant_status, _) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        exact_effect_request(
            account_id,
            "grant",
            100_000_000,
            "seed-retry-reserve-after-failure",
        ),
    )
    .await;
    assert_eq!(grant_status, StatusCode::CREATED);

    let (reserve_retry_status, reserve_retry_json) =
        send_json(app.clone(), "POST", "/v2/ledger/effects", reserve_body).await;
    assert_eq!(reserve_retry_status, StatusCode::CREATED);
    assert_eq!(reserve_retry_json["account"]["balance_minor"], 200_000_000);
    assert_eq!(reserve_retry_json["account"]["reserved_minor"], 150_000_000);

    let (refund_account_id, _) = create_account(app.clone(), 100_000_000).await;
    let refund_body = exact_effect_request(
        refund_account_id,
        "refund",
        1_000_000,
        "refund-without-reserve",
    );
    let (refund_status, refund_json) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        refund_body.clone(),
    )
    .await;
    assert_eq!(refund_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(refund_json["code"], "ledger_insufficient_funds");

    let (reserve_refund_retry_status, _) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        exact_effect_request(
            refund_account_id,
            "reserve",
            1_000_000,
            "seed-retry-refund-after-failure",
        ),
    )
    .await;
    assert_eq!(reserve_refund_retry_status, StatusCode::CREATED);

    let (refund_retry_status, refund_retry_json) =
        send_json(app, "POST", "/v2/ledger/effects", refund_body).await;
    assert_eq!(refund_retry_status, StatusCode::CREATED);
    assert_eq!(refund_retry_json["account"]["balance_minor"], 100_000_000);
    assert_eq!(refund_retry_json["account"]["reserved_minor"], 0);
}
