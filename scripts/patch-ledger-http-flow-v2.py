#!/usr/bin/env python3
"""Migrate the ledger HTTP integration suite from retired v1 value writes to v2."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PATH = ROOT / "services/ledger-service/tests/http_flow.rs"

CLEAN_IMPORTS = """use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use ledger_service::{
    build_router, repository::postgres::PostgresLedgerRepository, state::AppState,
};
use serde_json::{json, Value};
use tower::util::ServiceExt;
use uuid::Uuid;

"""

HELPERS_AND_AUTH = r'''const TEST_ORG_ID: &str = "00000000-0000-0000-0000-00000000ce01";
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
'''

LEDGER_TESTS = r'''#[tokio::test]
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
    assert!(state.exact_memory.read().await.account_openings_by_account.is_empty());
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

    let (get_status, fetched) =
        get_json(app, &format!("/v2/accounts/{account_id}")).await;
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

    let (get_status, fetched) =
        get_json(app, &format!("/v2/accounts/{account_id}")).await;
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

    let (get_status, fetched) =
        get_json(app, &format!("/v2/accounts/{account_id}")).await;
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
    let (second_status, second_json) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        request_body,
    )
    .await;

    assert_eq!(first_status, StatusCode::CREATED);
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(first_json["replayed"], false);
    assert_eq!(second_json["replayed"], true);
    assert_eq!(
        second_json["effect"]["entry_id"],
        first_json["effect"]["entry_id"]
    );

    let (get_status, fetched) =
        get_json(app, &format!("/v2/accounts/{account_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["balance_minor"], "100000000");
    assert_eq!(fetched["reserved_minor"], "8000000");
}

#[tokio::test]
async fn retired_legacy_value_routes_fail_closed_without_memory_mutation() {
    let state = test_state();
    let app = build_router(state.clone());

    let (account_status, account_body) = send_json(
        app.clone(),
        "POST",
        "/v1/accounts",
        json!({"legacy": true}),
    )
    .await;
    assert_eq!(account_status, StatusCode::GONE);
    assert_eq!(account_body["code"], "ledger_v1_value_write_gone");

    let (effect_status, effect_body) = send_json(
        app,
        "POST",
        "/v1/ledger/reserve",
        json!({"legacy": true}),
    )
    .await;
    assert_eq!(effect_status, StatusCode::GONE);
    assert_eq!(effect_body["code"], "ledger_v1_value_write_gone");
    assert!(state.accounts.read().await.is_empty());
    assert!(state.entries.read().await.is_empty());
    assert!(state.idempotency_keys.read().await.is_empty());
    assert!(state.exact_memory.read().await.effects_by_operation.is_empty());
}

#[tokio::test]
async fn insufficient_exact_amount_paths_are_retryable_after_state_changes() {
    let app = build_router(test_state());
    let (account_id, _) = create_account(app.clone(), 100_000_000).await;
    let reserve_body = exact_effect_request(
        account_id,
        "reserve",
        150_000_000,
        "reserve-too-much",
    );

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

    let (reserve_retry_status, reserve_retry_json) = send_json(
        app.clone(),
        "POST",
        "/v2/ledger/effects",
        reserve_body,
    )
    .await;
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

    let (refund_retry_status, refund_retry_json) = send_json(
        app,
        "POST",
        "/v2/ledger/effects",
        refund_body,
    )
    .await;
    assert_eq!(refund_retry_status, StatusCode::CREATED);
    assert_eq!(refund_retry_json["account"]["balance_minor"], 100_000_000);
    assert_eq!(refund_retry_json["account"]["reserved_minor"], 0);
}
'''


def main() -> None:
    text = PATH.read_text(encoding="utf-8")

    test_state_marker = "fn test_state()"
    if text.count(test_state_marker) != 1:
        raise SystemExit("cannot locate unique test_state marker")
    text = CLEAN_IMPORTS + text[text.index(test_state_marker) :]

    helper_start = "async fn create_account("
    trnm_marker = "#[tokio::test]\nasync fn trnm_session_verify_requires_a_signed_player_session()"
    if text.count(helper_start) != 1 or text.count(trnm_marker) != 1:
        raise SystemExit("cannot locate helper/auth replacement boundaries")
    start = text.index(helper_start)
    end = text.index(trnm_marker)
    text = text[:start] + HELPERS_AND_AUTH + "\n" + text[end:]

    ledger_start = "#[tokio::test]\nasync fn create_account_and_get_round_trip()"
    if text.count(ledger_start) != 1:
        raise SystemExit("cannot locate ledger test suffix")
    text = text[: text.index(ledger_start)] + LEDGER_TESTS

    PATH.write_text(text, encoding="utf-8")
    print("ledger HTTP flow suite migrated to exact v2 APIs")


if __name__ == "__main__":
    main()
