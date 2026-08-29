use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, Executor, PgPool, Row};
use std::{
    fs,
    path::{Path, PathBuf},
};
use trnm_economy_service::{
    contract::{
        serialized_intent_hash, stable_receipt_id, ActorRef, EconomicIntent, EconomicIntentKind,
        IdempotencyKey, TERM_EXCHANGE_PROTOCOL_VERSION,
    },
    repository::SettlementPlan,
    SettlementRepository,
};

const SETTLEMENT_MIGRATION: &str = include_str!("../migrations/settlement_v1.sql");

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

    // Keep the byte-immutability lane on the same integrated schema as the
    // reward lane. Discover and apply every numbered migration in order at
    // runtime, then layer the service-owned receipt bootstrap on top.
    let migration_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let mut migrations: Vec<PathBuf> = fs::read_dir(&migration_dir)
        .expect("read numbered CEX migration directory")
        .map(|entry| entry.expect("read migration directory entry").path())
        .filter(|path| {
            path.extension().and_then(|value| value.to_str()) == Some("sql")
                && path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| {
                        name.len() >= 5 && name.as_bytes()[..4].iter().all(u8::is_ascii_digit)
                    })
        })
        .collect();
    migrations.sort_by_key(|path| {
        path.file_name()
            .and_then(|value| value.to_str())
            .and_then(|name| name.get(..4))
            .and_then(|number| number.parse::<u16>().ok())
            .expect("numbered migration filename")
    });
    assert!(
        !migrations.is_empty(),
        "numbered CEX migration chain is empty"
    );
    for path in migrations {
        let migration = fs::read_to_string(&path).expect("read numbered CEX migration");
        pool.execute(migration.as_str())
            .await
            .unwrap_or_else(|error| panic!("apply {}: {error}", path.display()));
    }
    pool.execute(SETTLEMENT_MIGRATION)
        .await
        .expect("apply settlement schema");

    let exact_ready = sqlx::query_scalar::<_, bool>(
        "select to_regprocedure(
            'public.cex_apply_ledger_effect_v1(uuid,uuid,uuid,text,bigint,smallint,text,uuid,text,text,text,text,text)'
         ) is not null",
    )
    .fetch_one(pool)
    .await
    .expect("verify exact Ledger v2 function");
    assert!(
        exact_ready,
        "integrated CEX migration chain must expose Ledger v2"
    );
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
            scope: "campaign:durable-bytes".to_string(),
            key: intent_id.to_string(),
        },
        actors: vec![ActorRef {
            actor_id: "actor:durable-bytes".to_string(),
            actor_kind: "trnm_player".to_string(),
            account_id: None,
        }],
        assets: Vec::new(),
        amount_credits: Some(0),
        currency: Some("wallet_credits".to_string()),
        metadata: json!({"fixture": "durable-bytes-immutability"}),
        created_at_epoch: 1_787_918_400,
    }
}

fn assert_sqlstate(error: &sqlx::Error, expected: &str) {
    let code = error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|value| value.into_owned());
    assert_eq!(code.as_deref(), Some(expected));
}

#[tokio::test]
async fn exact_intent_bytes_are_durable_hash_checked_and_append_only() {
    let Some(database_url) = require_database_url() else {
        eprintln!("TRNM CEX durable-byte database test skipped: no database URL");
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect settlement test database");
    reset_schema(&pool).await;

    let repository = SettlementRepository::new(pool.clone());
    let intent = complete_contract_intent("contract-exact-bytes");
    let intent_bytes = serde_json::to_vec(&intent).expect("encode exact intent bytes");
    let intent_hash = serialized_intent_hash(&intent).expect("hash exact intent bytes");
    assert_eq!(intent_hash, format!("{:x}", Sha256::digest(&intent_bytes)));

    let receipt = repository
        .submit(
            &intent,
            &intent_hash,
            "world-authority-durable-bytes-test",
            SettlementPlan::CompleteContract,
        )
        .await
        .expect("commit exact durable intent and receipt");

    let row = sqlx::query(
        "select intent_hash, intent_bytes, intent_json, receipt_json
           from public.trnm_economy_settlement_receipts_v1
          where intent_id = $1",
    )
    .bind(&intent.intent_id)
    .fetch_one(&pool)
    .await
    .expect("load exact durable row");
    let stored_hash: String = row.get("intent_hash");
    let stored_bytes: Vec<u8> = row.get("intent_bytes");
    let stored_json: serde_json::Value = row.get("intent_json");
    let stored_receipt: serde_json::Value = row.get("receipt_json");
    assert_eq!(stored_hash, intent_hash);
    assert_eq!(stored_bytes, intent_bytes);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&stored_bytes)
            .expect("decode stored exact bytes"),
        stored_json
    );
    assert_eq!(stored_receipt, serde_json::to_value(&receipt).unwrap());

    let recovered = repository
        .lookup(&intent.intent_id)
        .await
        .expect("lookup exact durable row")
        .expect("durable receipt exists");
    assert_eq!(recovered.0, intent_hash);
    assert_eq!(recovered.1, receipt);

    let bad_intent = complete_contract_intent("contract-digest-negative");
    let bad_bytes = serde_json::to_vec(&bad_intent).unwrap();
    let bad_json = serde_json::to_value(&bad_intent).unwrap();
    let declared_hash = "0".repeat(64);
    assert_ne!(declared_hash, format!("{:x}", Sha256::digest(&bad_bytes)));
    let bad_receipt_id = stable_receipt_id(&declared_hash);
    let bad_receipt = json!({
        "protocol_version": TERM_EXCHANGE_PROTOCOL_VERSION,
        "receipt_id": bad_receipt_id.clone(),
        "intent_id": bad_intent.intent_id.clone(),
        "term_id": bad_intent.term_id.clone(),
        "backend_id": "cex-settlement-backend",
        "backend_kind": "cex",
        "status": "settled",
        "progression_class": "progression_allowed",
        "settlement_reference": null,
        "ledger_entry_id": null,
        "reason": null,
        "evidence": {"intent_hash": declared_hash.clone()},
        "finalized_at_epoch": 1_787_918_400_i64
    });
    let digest_error = sqlx::query(
        "insert into public.trnm_economy_settlement_receipts_v1 (
            intent_id, intent_hash, intent_bytes, intent_json,
            receipt_id, receipt_json, authority_id
         ) values ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&bad_intent.intent_id)
    .bind(&declared_hash)
    .bind(bad_bytes)
    .bind(bad_json)
    .bind(&bad_receipt_id)
    .bind(bad_receipt)
    .bind("malicious-direct-writer")
    .execute(&pool)
    .await
    .expect_err("database must reject bytes/hash mismatch");
    assert_sqlstate(&digest_error, "23514");

    let update_error = sqlx::query(
        "update public.trnm_economy_settlement_receipts_v1
            set authority_id = 'tampered-authority'
          where intent_id = $1",
    )
    .bind(&intent.intent_id)
    .execute(&pool)
    .await
    .expect_err("append-only receipt row must reject update");
    assert_sqlstate(&update_error, "55000");

    let delete_error = sqlx::query(
        "delete from public.trnm_economy_settlement_receipts_v1
          where intent_id = $1",
    )
    .bind(&intent.intent_id)
    .execute(&pool)
    .await
    .expect_err("append-only receipt row must reject delete");
    assert_sqlstate(&delete_error, "55000");

    let truncate_error = sqlx::query("truncate public.trnm_economy_settlement_receipts_v1")
        .execute(&pool)
        .await
        .expect_err("append-only receipt table must reject truncate");
    assert_sqlstate(&truncate_error, "55000");

    let count = sqlx::query_scalar::<_, i64>(
        "select count(*) from public.trnm_economy_settlement_receipts_v1",
    )
    .fetch_one(&pool)
    .await
    .expect("count retained append-only rows");
    assert_eq!(count, 1);
}
