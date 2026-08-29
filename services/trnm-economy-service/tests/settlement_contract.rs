use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{Datelike, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, Executor, PgPool, Row};
use tower::util::ServiceExt;
use trnm_economy_service::{
    build_router,
    config::{AuthorityPrincipal, AuthorityRegistry, IssuerKeyRecord, IssuerKeyRegistry},
    contract::{
        serialized_intent_hash, ActorRef, AssetRef, EconomicIntent, EconomicIntentKind,
        IdempotencyKey, ServerSignedValueEntitlementV2, ValueEntitlementSource,
        EXPECTED_GAME_AUTHORITY_AUDIENCE, SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY,
        SERVER_SIGNED_VALUE_ENTITLEMENT_V2_CONTRACT, TERM_EXCHANGE_PROTOCOL_VERSION,
    },
    AppState, SettlementRepository,
};
use uuid::Uuid;

const CORE_MIGRATION: &str = include_str!("../../../migrations/0001_init_core_tables.sql");
const SUMMARY_MIGRATION: &str =
    include_str!("../../../migrations/0002_add_account_summary_columns.sql");
const SETTLEMENT_MIGRATION: &str = include_str!("../migrations/settlement_v1.sql");
const AUTHORITY_TOKEN: &str = "world-authority-token-for-contract-tests";
const ORG_ID: &str = "00000000-0000-0000-0000-00000000ce01";
const KEY_ID: &str = "test-ed25519-key-v1";

fn require_database_url() -> Option<String> {
    match std::env::var("TRNM_CEX_SETTLEMENT_TEST_DATABASE_URL") {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ if std::env::var("TRNM_REQUIRE_CEX_SETTLEMENT_DATABASE_TEST").as_deref() == Ok("1") => {
            panic!("TRNM_CEX_SETTLEMENT_TEST_DATABASE_URL is required")
        }
        _ => None,
    }
}

async fn reset_schema(pool: &PgPool) {
    pool.execute("drop schema if exists public cascade; create schema public")
        .await
        .expect("reset public schema");
    pool.execute(CORE_MIGRATION)
        .await
        .expect("apply core CEX schema");
    pool.execute(SUMMARY_MIGRATION)
        .await
        .expect("apply account summary schema");
    pool.execute(SETTLEMENT_MIGRATION)
        .await
        .expect("apply settlement schema");
    sqlx::query(
        "insert into public.organizations (org_id, name)
         values ($1::uuid, 'TRNM settlement tests')",
    )
    .bind(ORG_ID)
    .execute(pool)
    .await
    .expect("insert test organization");
}

fn token_sha256(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn issuer_registry(signing_key: &SigningKey) -> IssuerKeyRegistry {
    let public_key = signing_key.verifying_key();
    IssuerKeyRegistry::new(vec![IssuerKeyRecord {
        key_id: KEY_ID.to_string(),
        issuer: "trnm-online-game-server".to_string(),
        status: "active".to_string(),
        signature_algorithm: "ed25519".to_string(),
        public_key_sha256: format!("{:x}", Sha256::digest(public_key.to_bytes())),
        verifying_key: public_key,
    }])
    .expect("test issuer registry")
}

fn authority_registry(audience: &str) -> AuthorityRegistry {
    AuthorityRegistry::new(vec![AuthorityPrincipal {
        authority_id: "trnm-world-contract-test".to_string(),
        audience: audience.to_string(),
        token_sha256: token_sha256(AUTHORITY_TOKEN),
        active: true,
    }])
    .expect("test authority registry")
}

fn build_test_app(pool: PgPool, signing_key: &SigningKey, audience: &str) -> Router {
    let repository = SettlementRepository::new(pool);
    build_router(AppState::new(
        repository,
        authority_registry(audience),
        issuer_registry(signing_key),
    ))
}

async fn insert_wallet_account(pool: &PgPool) -> Uuid {
    let account_id = Uuid::new_v4();
    sqlx::query(
        "insert into public.accounts (
            account_id, org_id, account_type, currency_unit, status, balance, reserved
         ) values ($1, $2::uuid, 'trnm_wallet', 'wallet_credits', 'active', 0, 0)",
    )
    .bind(account_id)
    .bind(ORG_ID)
    .execute(pool)
    .await
    .expect("insert wallet account");
    account_id
}

fn signed_reward_intent(
    signing_key: &SigningKey,
    account_id: Uuid,
    intent_id: &str,
    amount: i64,
) -> EconomicIntent {
    let now = Utc::now();
    let mut entitlement = ServerSignedValueEntitlementV2 {
        contract_version: SERVER_SIGNED_VALUE_ENTITLEMENT_V2_CONTRACT.to_string(),
        entitlement_id: format!("entitlement:{intent_id}"),
        issuer: "trnm-online-game-server".to_string(),
        key_id: KEY_ID.to_string(),
        signature_algorithm: "ed25519".to_string(),
        actor_id: format!("actor:{account_id}"),
        account_id: account_id.to_string(),
        source: ValueEntitlementSource::Battle,
        source_id: format!("battle:{intent_id}"),
        intent_id: intent_id.to_string(),
        amount_credits: amount,
        currency: "wallet_credits".to_string(),
        budget_day: (now.year() as u32) * 10_000 + now.month() * 100 + now.day(),
        issued_at_epoch: now.timestamp(),
        expires_at_epoch: now.timestamp() + 600,
        match_id: format!("match:{intent_id}"),
        rules_version: "rules-v1".to_string(),
        build_id: "build-v1".to_string(),
        result_hash: "a".repeat(64),
        participants_hash: "b".repeat(64),
        nonce: format!("nonce:{intent_id}"),
        signature: String::new(),
    };
    let payload = entitlement.signing_payload().expect("signing payload");
    entitlement.signature = STANDARD.encode(signing_key.sign(&payload).to_bytes());

    EconomicIntent {
        protocol_version: TERM_EXCHANGE_PROTOCOL_VERSION.to_string(),
        intent_id: intent_id.to_string(),
        term_id: format!("term:{intent_id}"),
        term_version: "v1".to_string(),
        domain: "trnm_game".to_string(),
        kind: EconomicIntentKind::ReleaseReward,
        idempotency_key: IdempotencyKey {
            scope: format!("campaign:{account_id}"),
            key: intent_id.to_string(),
        },
        actors: vec![ActorRef {
            actor_id: format!("actor:{account_id}"),
            actor_kind: "trnm_player".to_string(),
            account_id: Some(account_id.to_string()),
        }],
        assets: vec![AssetRef {
            asset_id: "cex-wallet-credit".to_string(),
            asset_kind: "walletcredit".to_string(),
            quantity: amount,
            unit: "credits".to_string(),
        }],
        amount_credits: Some(amount),
        currency: Some("wallet_credits".to_string()),
        metadata: json!({
            SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY: entitlement,
            "value_event_id": format!("event:{intent_id}")
        }),
        created_at_epoch: now.timestamp(),
    }
}

fn complete_contract_intent(intent_id: &str) -> EconomicIntent {
    EconomicIntent {
        protocol_version: TERM_EXCHANGE_PROTOCOL_VERSION.to_string(),
        intent_id: intent_id.to_string(),
        term_id: format!("term:{intent_id}"),
        term_version: "v1".to_string(),
        domain: "trnm_game".to_string(),
        kind: EconomicIntentKind::CompleteContract,
        idempotency_key: IdempotencyKey {
            scope: "campaign:contract".to_string(),
            key: intent_id.to_string(),
        },
        actors: vec![ActorRef {
            actor_id: "actor:contract".to_string(),
            actor_kind: "trnm_player".to_string(),
            account_id: None,
        }],
        assets: Vec::new(),
        amount_credits: Some(0),
        currency: Some("wallet_credits".to_string()),
        metadata: json!({}),
        created_at_epoch: Utc::now().timestamp(),
    }
}

async fn send(
    app: Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    intent_hash: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header("x-trnm-game-authority", token);
    }
    if let Some(intent_hash) = intent_hash {
        builder = builder.header("x-trnm-intent-sha256", intent_hash);
    }
    let body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&value).expect("serialize request"))
        }
        None => Body::empty(),
    };
    let response = app
        .oneshot(builder.body(body).expect("build request"))
        .await
        .expect("router response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("decode response json")
    };
    (status, value)
}

async fn post_intent(
    app: Router,
    token: Option<&str>,
    intent: &EconomicIntent,
    hash_override: Option<&str>,
) -> (StatusCode, Value) {
    let hash = serialized_intent_hash(intent).expect("intent hash");
    send(
        app,
        Method::POST,
        "/v1/trnm/economy/intents",
        token,
        Some(hash_override.unwrap_or(&hash)),
        Some(json!({"intent": intent})),
    )
    .await
}

async fn wallet_balance_and_counts(pool: &PgPool, account_id: Uuid) -> (i64, i64, i64) {
    let row = sqlx::query(
        "select
            (select balance::bigint from public.accounts where account_id = $1) as balance,
            (select count(*) from public.ledger_entries where account_id = $1) as ledger_count,
            (select count(*) from public.trnm_economy_settlement_receipts_v1
              where account_id = $1) as receipt_count",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await
    .expect("load wallet counts");
    (
        row.get("balance"),
        row.get("ledger_count"),
        row.get("receipt_count"),
    )
}

#[tokio::test]
async fn durable_receipt_lookup_owner_contract_matrix() {
    let Some(database_url) = require_database_url() else {
        eprintln!("TRNM CEX settlement database test skipped: no database URL");
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(12)
        .connect(&database_url)
        .await
        .expect("connect settlement test database");
    reset_schema(&pool).await;

    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let app = build_test_app(pool.clone(), &signing_key, EXPECTED_GAME_AUTHORITY_AUDIENCE);

    let (readiness_status, readiness) = send(
        app.clone(),
        Method::GET,
        "/v1/trnm/economy/readiness",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(readiness_status, StatusCode::OK);
    assert_eq!(readiness["status"], "ok");
    assert_eq!(readiness["public_player_market_enabled"], false);

    let (issuer_status, issuer) = send(
        app.clone(),
        Method::POST,
        "/v1/trnm/economy/issuer-keys/status",
        Some(AUTHORITY_TOKEN),
        None,
        Some(json!({"key_id": KEY_ID})),
    )
    .await;
    assert_eq!(issuer_status, StatusCode::OK);
    assert_eq!(issuer["status"], "active");
    assert_eq!(issuer["signature_algorithm"], "ed25519");

    let account_id = insert_wallet_account(&pool).await;
    let intent = signed_reward_intent(&signing_key, account_id, "reward-response-loss", 25);
    let intent_hash = serialized_intent_hash(&intent).unwrap();

    // Simulate a client losing the HTTP response after the transaction commits.
    let (first_status, first_receipt) =
        post_intent(app.clone(), Some(AUTHORITY_TOKEN), &intent, None).await;
    assert_eq!(first_status, StatusCode::OK);
    drop(first_receipt);

    let (lookup_status, lookup) = send(
        app.clone(),
        Method::GET,
        &format!(
            "/v1/trnm/economy/receipts/by-intent?intent_id={}",
            intent.intent_id
        ),
        Some(AUTHORITY_TOKEN),
        Some(&intent_hash),
        None,
    )
    .await;
    assert_eq!(lookup_status, StatusCode::OK);
    assert_eq!(
        lookup["contract_version"],
        "trnm_cex_settlement_receipt_lookup_v1"
    );
    assert_eq!(lookup["intent_hash"], intent_hash);
    assert_eq!(lookup["receipt"]["status"], "approved_release");

    let (duplicate_status, duplicate_receipt) =
        post_intent(app.clone(), Some(AUTHORITY_TOKEN), &intent, None).await;
    assert_eq!(duplicate_status, StatusCode::OK);
    assert_eq!(duplicate_receipt, lookup["receipt"]);
    assert_eq!(
        wallet_balance_and_counts(&pool, account_id).await,
        (25, 1, 1)
    );

    let mut conflict = signed_reward_intent(&signing_key, account_id, "reward-response-loss", 30);
    conflict.term_id = intent.term_id.clone();
    let (conflict_status, conflict_body) =
        post_intent(app.clone(), Some(AUTHORITY_TOKEN), &conflict, None).await;
    assert_eq!(conflict_status, StatusCode::CONFLICT);
    assert_eq!(conflict_body["code"], "intent_hash_conflict");
    assert_eq!(
        wallet_balance_and_counts(&pool, account_id).await,
        (25, 1, 1)
    );

    let concurrent = signed_reward_intent(&signing_key, account_id, "reward-concurrent", 10);
    let first_app = app.clone();
    let second_app = app.clone();
    let first_intent = concurrent.clone();
    let second_intent = concurrent.clone();
    let (left, right) = tokio::join!(
        post_intent(first_app, Some(AUTHORITY_TOKEN), &first_intent, None),
        post_intent(second_app, Some(AUTHORITY_TOKEN), &second_intent, None)
    );
    assert_eq!(left.0, StatusCode::OK);
    assert_eq!(right.0, StatusCode::OK);
    assert_eq!(left.1, right.1);
    assert_eq!(
        wallet_balance_and_counts(&pool, account_id).await,
        (35, 2, 2)
    );

    let (missing_auth_status, missing_auth) = post_intent(
        app.clone(),
        None,
        &complete_contract_intent("contract-missing-auth"),
        None,
    )
    .await;
    assert_eq!(missing_auth_status, StatusCode::UNAUTHORIZED);
    assert_eq!(missing_auth["code"], "missing_game_authority");

    let wrong_audience_app = build_test_app(pool.clone(), &signing_key, "wrong-audience");
    let wrong_audience_intent = complete_contract_intent("contract-wrong-audience");
    let (wrong_audience_status, wrong_audience) = post_intent(
        wrong_audience_app,
        Some(AUTHORITY_TOKEN),
        &wrong_audience_intent,
        None,
    )
    .await;
    assert_eq!(wrong_audience_status, StatusCode::FORBIDDEN);
    assert_eq!(wrong_audience["code"], "wrong_game_authority_audience");

    let malformed_hash_intent = complete_contract_intent("contract-malformed-hash");
    let (malformed_status, malformed) = post_intent(
        app.clone(),
        Some(AUTHORITY_TOKEN),
        &malformed_hash_intent,
        Some("ABC"),
    )
    .await;
    assert_eq!(malformed_status, StatusCode::BAD_REQUEST);
    assert_eq!(malformed["code"], "invalid_intent_hash");

    let mut bad_signature =
        signed_reward_intent(&signing_key, account_id, "reward-bad-signature", 5);
    bad_signature.metadata[SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY]["signature"] =
        Value::String(STANDARD.encode([9_u8; 64]));
    let (bad_signature_status, bad_signature_body) =
        post_intent(app.clone(), Some(AUTHORITY_TOKEN), &bad_signature, None).await;
    assert_eq!(bad_signature_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        bad_signature_body["code"],
        "invalid_value_entitlement_signature"
    );

    let complete = complete_contract_intent("contract-durable");
    let complete_hash = serialized_intent_hash(&complete).unwrap();
    let (complete_status, complete_receipt) =
        post_intent(app.clone(), Some(AUTHORITY_TOKEN), &complete, None).await;
    assert_eq!(complete_status, StatusCode::OK);
    assert_eq!(complete_receipt["status"], "settled");
    let (complete_lookup_status, complete_lookup) = send(
        app.clone(),
        Method::GET,
        "/v1/trnm/economy/receipts/by-intent?intent_id=contract-durable",
        Some(AUTHORITY_TOKEN),
        Some(&complete_hash),
        None,
    )
    .await;
    assert_eq!(complete_lookup_status, StatusCode::OK);
    assert_eq!(complete_lookup["receipt"], complete_receipt);

    let capped_account = insert_wallet_account(&pool).await;
    for ordinal in 0..3 {
        let capped = signed_reward_intent(
            &signing_key,
            capped_account,
            &format!("reward-cap-{ordinal}"),
            100,
        );
        let (status, _) = post_intent(app.clone(), Some(AUTHORITY_TOKEN), &capped, None).await;
        assert_eq!(status, StatusCode::OK);
    }
    let over_cap = signed_reward_intent(&signing_key, capped_account, "reward-cap-over", 1);
    let (over_cap_status, over_cap_body) =
        post_intent(app.clone(), Some(AUTHORITY_TOKEN), &over_cap, None).await;
    assert_eq!(over_cap_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(over_cap_body["code"], "daily_reward_limit_exceeded");
    assert_eq!(
        wallet_balance_and_counts(&pool, capped_account).await,
        (300, 3, 3)
    );

    let mismatched_lookup_hash = "f".repeat(64);
    assert_ne!(mismatched_lookup_hash, complete_hash);
    let (mismatch_status, mismatch) = send(
        app,
        Method::GET,
        "/v1/trnm/economy/receipts/by-intent?intent_id=contract-durable",
        Some(AUTHORITY_TOKEN),
        Some(&mismatched_lookup_hash),
        None,
    )
    .await;
    assert_eq!(mismatch_status, StatusCode::CONFLICT);
    assert_eq!(mismatch["code"], "intent_hash_conflict");
}
