use sqlx::{executor::Executor, row::Row};
use sqlx_postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

const OUTBOX_MIGRATION: &str = include_str!("../migrations/0016_online_settlement_outbox_v1.sql");
const WORKER_MIGRATION: &str =
    include_str!("../migrations/0017_online_settlement_worker_runtime_v1.sql");

fn require_database_url() -> Option<String> {
    match std::env::var("TRNM_SETTLEMENT_TEST_DATABASE_URL") {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ if std::env::var("TRNM_REQUIRE_SETTLEMENT_DATABASE_TEST").as_deref() == Ok("1") => {
            panic!("TRNM_SETTLEMENT_TEST_DATABASE_URL is required")
        }
        _ => None,
    }
}

fn assert_sqlstate(error: sqlx::Error, expected: &str) {
    let sqlx::Error::Database(database) = error else {
        panic!("expected PostgreSQL error {expected}, got {error}");
    };
    assert_eq!(database.code().as_deref(), Some(expected));
}

async fn reset_schema(pool: &PgPool) {
    pool.execute("drop schema if exists public cascade; create schema public")
        .await
        .expect("reset settlement database schema");
    pool.execute(
        "create table public.trnm_online_campaigns (
            campaign_id text primary key,
            state_hash text not null
        );
        create table public.trnm_online_matches (
            match_id uuid primary key,
            phase text not null,
            settlement_state text not null,
            terminal_publication_state text not null,
            checkpoint_sequence bigint not null,
            next_sequence bigint not null,
            result_hash text,
            terminal_publication_actor_generation bigint,
            assigned_instance_id text,
            assigned_instance_epoch bigint,
            assigned_physical_host_id text,
            authoritative_tick bigint not null,
            match_revision bigint not null,
            snapshot_hash text not null,
            updated_at timestamptz not null default clock_timestamp()
        );
        create table public.trnm_online_match_members (
            match_id uuid not null references public.trnm_online_matches(match_id),
            player_id text not null,
            campaign_id text not null references public.trnm_online_campaigns(campaign_id),
            next_input_sequence bigint not null,
            primary key (match_id, player_id)
        );
        create table public.trnm_online_terminal_publication_acks (
            match_id uuid primary key references public.trnm_online_matches(match_id),
            local_tombstone_state text not null,
            actor_generation bigint not null,
            instance_id text not null,
            actor_epoch bigint not null,
            physical_host_id text not null,
            authoritative_tick bigint not null,
            next_sequence bigint not null,
            match_revision bigint not null,
            next_input_sequences jsonb not null,
            snapshot_hash text not null,
            phase text not null,
            result_hash text,
            published_settlement_state text not null
        )",
    )
    .await
    .expect("create settlement migration scaffold");
    pool.execute(OUTBOX_MIGRATION)
        .await
        .expect("apply settlement outbox migration");
    pool.execute(WORKER_MIGRATION)
        .await
        .expect("apply settlement worker migration");
}

async fn insert_capture_and_job(
    pool: &PgPool,
    match_id: Uuid,
    campaign_id: &str,
    generation: i64,
    capture_state: &str,
    intent_id: &str,
    intent_hash: &str,
) -> (String, String) {
    let capture_id = format!("trnm-settlement-capture-v1:{generation:064x}");
    let job_id = format!("trnm-settlement-outbox-v1:{generation:064x}");
    sqlx::query::query(
        "insert into public.trnm_online_settlement_captures (
            capture_id, contract_version, match_id, capture_generation,
            terminal_identity_hash, terminal_identity_json,
            campaign_fences_json, head_intent_ids_json, state
         ) values ($1, 'trnm_settlement_capture_v1', $2, $3, $4, '{}', '{}', '{}', $5)",
    )
    .bind(&capture_id)
    .bind(match_id)
    .bind(generation)
    .bind(format!("{generation:064x}"))
    .bind(capture_state)
    .execute(pool)
    .await
    .expect("insert settlement capture");

    sqlx::query::query(
        "insert into public.trnm_online_settlement_jobs (
            job_id, contract_version, capture_id, capture_generation,
            match_id, campaign_id, intent_id, intent_hash,
            expected_campaign_revision, expected_campaign_state_hash,
            queue_lane, intent_json
         ) values (
            $1, 'trnm_settlement_outbox_v1', $2, $3,
            $4, $5, $6, $7, 0, $8, 'ordinary', '{}'
         )",
    )
    .bind(&job_id)
    .bind(&capture_id)
    .bind(generation)
    .bind(match_id)
    .bind(campaign_id)
    .bind(intent_id)
    .bind(intent_hash)
    .bind("c".repeat(64))
    .execute(pool)
    .await
    .expect("insert settlement job");

    (capture_id, job_id)
}

#[tokio::test]
async fn settlement_database_identity_lease_and_retention_contract() {
    let Some(database_url) = require_database_url() else {
        eprintln!("settlement database contract skipped: no test database URL");
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect settlement test database");
    reset_schema(&pool).await;

    let match_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
    let campaign_id = "campaign-a";
    sqlx::query::query(
        "insert into public.trnm_online_campaigns (campaign_id, state_hash)
         values ($1, $2)",
    )
    .bind(campaign_id)
    .bind("c".repeat(64))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query::query(
        "insert into public.trnm_online_matches (
            match_id, phase, settlement_state, terminal_publication_state,
            checkpoint_sequence, next_sequence, result_hash,
            terminal_publication_actor_generation, assigned_instance_id,
            assigned_instance_epoch, assigned_physical_host_id,
            authoritative_tick, match_revision, snapshot_hash
         ) values (
            $1, 'complete', 'pending', 'acknowledged',
            0, 0, $2, 1, 'instance-a', 1, 'host-a', 0, 0, $3
         )",
    )
    .bind(match_id)
    .bind("d".repeat(64))
    .bind("e".repeat(64))
    .execute(&pool)
    .await
    .unwrap();

    let (_, first_job) = insert_capture_and_job(
        &pool,
        match_id,
        campaign_id,
        1,
        "stale",
        "intent-a",
        &"a".repeat(64),
    )
    .await;
    let (_, active_job) = insert_capture_and_job(
        &pool,
        match_id,
        campaign_id,
        2,
        "active",
        "intent-a",
        &"a".repeat(64),
    )
    .await;
    let (_, _changed_payload_job) = insert_capture_and_job(
        &pool,
        match_id,
        campaign_id,
        3,
        "stale",
        "intent-a",
        &"b".repeat(64),
    )
    .await;

    let identities = sqlx::query::query(
        "select job_id, remote_request_id
           from public.trnm_online_settlement_jobs
          order by capture_generation",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(identities.len(), 3);
    let remote_ids = identities
        .iter()
        .map(|row| row.get::<String, _>("remote_request_id"))
        .collect::<Vec<_>>();
    assert_eq!(remote_ids[0], remote_ids[1]);
    assert_eq!(remote_ids[1], remote_ids[2]);
    assert!(remote_ids[0].starts_with("trnm-settlement-remote-v1:"));
    assert_eq!(remote_ids[0].len(), "trnm-settlement-remote-v1:".len() + 64);

    let direct_identity_drift = sqlx::query::query(
        "update public.trnm_online_settlement_jobs
            set remote_request_id = $2
          where job_id = $1",
    )
    .bind(&first_job)
    .bind(format!("trnm-settlement-remote-v1:{}", "f".repeat(64)))
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(direct_identity_drift, "23514");

    let durable_identity_drift = sqlx::query::query(
        "update public.trnm_online_settlement_jobs
            set intent_id = 'intent-rebound'
          where job_id = $1",
    )
    .bind(&first_job)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(durable_identity_drift, "23514");

    for column in ["authorization_request_id", "entitlement_nonce"] {
        let statement = format!(
            "update public.trnm_online_settlement_jobs set {column} = 'wrong' where job_id = $1"
        );
        let error = sqlx::query::query(&statement)
            .bind(&active_job)
            .execute(&pool)
            .await
            .unwrap_err();
        assert_sqlstate(error, "23514");
    }

    let retired_claim = sqlx::query::query(
        "select * from public.trnm_online_claim_settlement_job_v1('old-worker', 30000)",
    )
    .execute(&pool)
    .await
    .unwrap_err();
    assert_sqlstate(retired_claim, "0A000");

    let first_lease = sqlx::query::query(
        "select job_id, lease_generation, authorization_request_id, entitlement_nonce
           from public.trnm_online_claim_settlement_job_v2($1, $2)",
    )
    .bind("worker-a")
    .bind(30_000_i64)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(first_lease.get::<String, _>("job_id"), active_job);
    let first_generation = first_lease.get::<i64, _>("lease_generation");
    assert_eq!(
        first_lease.get::<String, _>("authorization_request_id"),
        remote_ids[1]
    );
    assert_eq!(
        first_lease.get::<String, _>("entitlement_nonce"),
        remote_ids[1]
    );

    sqlx::query::query(
        "update public.trnm_online_settlement_jobs
            set lease_expires_at = clock_timestamp() - interval '1 second'
          where job_id = $1",
    )
    .bind(&active_job)
    .execute(&pool)
    .await
    .unwrap();

    let authorization_after_expiry = sqlx::query_scalar::query_scalar::<_, bool>(
        "select public.trnm_online_store_settlement_authorization_v1(
            $1, 'worker-a', $2, $3, '{}', null
         )",
    )
    .bind(&active_job)
    .bind(first_generation)
    .bind(&remote_ids[1])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!authorization_after_expiry);

    let attempt_after_expiry = sqlx::query_scalar::query_scalar::<_, Option<i32>>(
        "select public.trnm_online_begin_settlement_remote_attempt_v1($1, 'worker-a', $2)",
    )
    .bind(&active_job)
    .bind(first_generation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(attempt_after_expiry, None);

    let complete_after_expiry = sqlx::query_scalar::query_scalar::<_, bool>(
        "select public.trnm_online_complete_settlement_job_v1(
            $1, 'worker-a', $2, 'receipt-a', $3, '{}', null
         )",
    )
    .bind(&active_job)
    .bind(first_generation)
    .bind("1".repeat(64))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!complete_after_expiry);

    let retry_after_expiry = sqlx::query_scalar::query_scalar::<_, Option<String>>(
        "select public.trnm_online_retry_settlement_job_v1(
            $1, 'worker-a', $2, 'expired', 1000
         )",
    )
    .bind(&active_job)
    .bind(first_generation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retry_after_expiry, None);

    let dead_letter_after_expiry = sqlx::query_scalar::query_scalar::<_, bool>(
        "select public.trnm_online_dead_letter_settlement_job_v1(
            $1, 'worker-a', $2, 'expired'
         )",
    )
    .bind(&active_job)
    .bind(first_generation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!dead_letter_after_expiry);

    let takeover = sqlx::query::query(
        "select job_id, lease_generation
           from public.trnm_online_claim_settlement_job_v2($1, $2)",
    )
    .bind("worker-b")
    .bind(30_000_i64)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(takeover.get::<String, _>("job_id"), active_job);
    let second_generation = takeover.get::<i64, _>("lease_generation");
    assert_eq!(second_generation, first_generation + 1);

    let stale_worker_retry = sqlx::query_scalar::query_scalar::<_, Option<String>>(
        "select public.trnm_online_retry_settlement_job_v1(
            $1, 'worker-a', $2, 'stale', 1000
         )",
    )
    .bind(&active_job)
    .bind(first_generation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stale_worker_retry, None);

    let current_attempt = sqlx::query_scalar::query_scalar::<_, Option<i32>>(
        "select public.trnm_online_begin_settlement_remote_attempt_v1($1, 'worker-b', $2)",
    )
    .bind(&active_job)
    .bind(second_generation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(current_attempt, Some(1));

    let completed = sqlx::query_scalar::query_scalar::<_, bool>(
        "select public.trnm_online_complete_settlement_job_v1(
            $1, 'worker-b', $2, 'receipt-b', $3, '{}', null
         )",
    )
    .bind(&active_job)
    .bind(second_generation)
    .bind("2".repeat(64))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(completed);

    let status = sqlx::query::query(
        "select remote_state, application_state
           from public.trnm_online_settlement_job_status_v1
          where job_id = $1",
    )
    .bind(&active_job)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status.get::<String, _>("remote_state"), "remote_succeeded");
    assert_eq!(
        status.get::<String, _>("application_state"),
        "pending_apply"
    );

    sqlx::query::query(
        "update public.trnm_online_settlement_jobs
            set campaign_applied_at = clock_timestamp()
          where job_id = $1",
    )
    .bind(&active_job)
    .execute(&pool)
    .await
    .unwrap();
    let application_state = sqlx::query_scalar::query_scalar::<_, String>(
        "select application_state
           from public.trnm_online_settlement_job_status_v1
          where job_id = $1",
    )
    .bind(&active_job)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(application_state, "applied");

    let delete_match =
        sqlx::query::query("delete from public.trnm_online_matches where match_id = $1")
            .bind(match_id)
            .execute(&pool)
            .await
            .unwrap_err();
    assert_sqlstate(delete_match, "23503");

    let delete_campaign =
        sqlx::query::query("delete from public.trnm_online_campaigns where campaign_id = $1")
            .bind(campaign_id)
            .execute(&pool)
            .await
            .unwrap_err();
    assert_sqlstate(delete_campaign, "23503");

    pool.close().await;
}
