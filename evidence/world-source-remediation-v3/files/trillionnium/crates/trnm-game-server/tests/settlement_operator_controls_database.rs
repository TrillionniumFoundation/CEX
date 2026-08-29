use sqlx::{executor::Executor, row::Row};
use sqlx_postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

const OUTBOX_MIGRATION: &str = include_str!("../migrations/0016_online_settlement_outbox_v1.sql");
const WORKER_MIGRATION: &str =
    include_str!("../migrations/0017_online_settlement_worker_runtime_v1.sql");
const OPERATOR_MIGRATION: &str =
    include_str!("../migrations/0018_online_settlement_operator_controls_v1.sql");

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
        .expect("reset settlement operator schema");
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
    .expect("create settlement operator scaffold");
    pool.execute(OUTBOX_MIGRATION)
        .await
        .expect("apply outbox migration");
    pool.execute(WORKER_MIGRATION)
        .await
        .expect("apply worker migration");
    pool.execute(OPERATOR_MIGRATION)
        .await
        .expect("apply operator migration");
}

async fn insert_dead_letter_job(pool: &PgPool) -> (Uuid, String, String, String, String, String) {
    let match_id = Uuid::parse_str("77777777-7777-7777-7777-777777777777").unwrap();
    let campaign_id = "campaign-operator".to_string();
    let capture_id = format!("trnm-settlement-capture-v1:{}", "7".repeat(64));
    let job_id = format!("trnm-settlement-outbox-v1:{}", "7".repeat(64));
    let intent_id = "operator-intent-v1".to_string();
    let intent_hash = "a".repeat(64);

    sqlx::query::query(
        "insert into public.trnm_online_campaigns (campaign_id, state_hash)
         values ($1, $2)",
    )
    .bind(&campaign_id)
    .bind("b".repeat(64))
    .execute(pool)
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
            0, 0, $2, 1, 'instance-operator', 1, 'host-operator', 0, 0, $3
         )",
    )
    .bind(match_id)
    .bind("c".repeat(64))
    .bind("d".repeat(64))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query::query(
        "insert into public.trnm_online_settlement_captures (
            capture_id, contract_version, match_id, capture_generation,
            terminal_identity_hash, terminal_identity_json,
            campaign_fences_json, head_intent_ids_json,
            state, last_error
         ) values (
            $1, 'trnm_settlement_capture_v1', $2, 1,
            $3, '{}', '{}', '{}', 'dead_letter', 'remote failure'
         )",
    )
    .bind(&capture_id)
    .bind(match_id)
    .bind("e".repeat(64))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query::query(
        "insert into public.trnm_online_settlement_jobs (
            job_id, contract_version, capture_id, capture_generation,
            match_id, campaign_id, intent_id, intent_hash,
            expected_campaign_revision, expected_campaign_state_hash,
            queue_lane, intent_json, state, attempts, remote_attempts,
            last_error, failure_class, completed_at
         ) values (
            $1, 'trnm_settlement_outbox_v1', $2, 1,
            $3, $4, $5, $6,
            0, $7, 'ordinary', $8,
            'dead_letter', 16, 16, 'ambiguous remote failure', 'permanent',
            clock_timestamp()
         )",
    )
    .bind(&job_id)
    .bind(&capture_id)
    .bind(match_id)
    .bind(&campaign_id)
    .bind(&intent_id)
    .bind(&intent_hash)
    .bind("b".repeat(64))
    .bind(serde_json::json!({
        "actors": [{"actor_id": "operator-player", "account_id": "account-operator"}]
    }))
    .execute(pool)
    .await
    .unwrap();

    let remote_request_id = sqlx::query_scalar::query_scalar::<_, String>(
        "select remote_request_id
           from public.trnm_online_settlement_jobs
          where job_id = $1",
    )
    .bind(&job_id)
    .fetch_one(pool)
    .await
    .unwrap();

    (
        match_id,
        campaign_id,
        capture_id,
        job_id,
        intent_id,
        format!("{intent_hash}:{remote_request_id}"),
    )
}

#[allow(clippy::too_many_arguments)]
async fn authorize_replay(
    pool: &PgPool,
    request_id: &str,
    job_id: &str,
    capture_id: &str,
    intent_id: &str,
    intent_hash: &str,
    remote_request_id: &str,
    reason: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::query_scalar::<_, bool>(
        "select public.trnm_online_authorize_settlement_replay_v1(
            $1, $2, $3, $4, $5, $6, $7, $8, $9
         )",
    )
    .bind(request_id)
    .bind(job_id)
    .bind(capture_id)
    .bind(intent_id)
    .bind(intent_hash)
    .bind(remote_request_id)
    .bind("operator-security")
    .bind("WORLD-P0-001")
    .bind(reason)
    .fetch_one(pool)
    .await
}

#[tokio::test]
async fn settlement_operator_replay_is_exact_audited_one_attempt_and_append_only() {
    let Some(database_url) = require_database_url() else {
        eprintln!("settlement operator database test skipped: no database URL");
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect settlement operator database");
    reset_schema(&pool).await;

    let (_match_id, _campaign_id, capture_id, job_id, intent_id, hashes) =
        insert_dead_letter_job(&pool).await;
    let (intent_hash, remote_request_id) = hashes.split_once(':').unwrap();
    let first_request = format!("trnm-settlement-replay-v1:{}", "1".repeat(64));
    let first_reason = "Recover the exact ambiguous CEX request after incident review";

    assert!(authorize_replay(
        &pool,
        &first_request,
        &job_id,
        &capture_id,
        &intent_id,
        intent_hash,
        remote_request_id,
        first_reason,
    )
    .await
    .unwrap());

    let replayed = sqlx::query::query(
        "select state, remote_attempts, lease_owner, next_attempt_at,
                completed_at, remote_request_id, intent_hash
           from public.trnm_online_settlement_jobs
          where job_id = $1",
    )
    .bind(&job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(replayed.get::<String, _>("state"), "retryable");
    assert_eq!(replayed.get::<i32, _>("remote_attempts"), 15);
    assert_eq!(replayed.get::<Option<String>, _>("lease_owner"), None);
    assert!(replayed
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("next_attempt_at")
        .is_some());
    assert!(replayed
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at")
        .is_none());
    assert_eq!(
        replayed.get::<String, _>("remote_request_id"),
        remote_request_id
    );
    assert_eq!(replayed.get::<String, _>("intent_hash"), intent_hash);

    let capture_state = sqlx::query_scalar::query_scalar::<_, String>(
        "select state from public.trnm_online_settlement_captures where capture_id = $1",
    )
    .bind(&capture_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(capture_state, "active");

    let evidence = sqlx::query::query(
        "select replay_generation, prior_state, prior_remote_attempts,
                policy_revision, retain_until, authorized_at
           from public.trnm_online_settlement_operator_replay_requests
          where request_id = $1",
    )
    .bind(&first_request)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(evidence.get::<i64, _>("replay_generation"), 1);
    assert_eq!(evidence.get::<String, _>("prior_state"), "dead_letter");
    assert_eq!(evidence.get::<i32, _>("prior_remote_attempts"), 16);
    assert_eq!(evidence.get::<i64, _>("policy_revision"), 1);
    let authorized_at = evidence.get::<chrono::DateTime<chrono::Utc>, _>("authorized_at");
    let retain_until = evidence.get::<chrono::DateTime<chrono::Utc>, _>("retain_until");
    assert!(retain_until >= authorized_at + chrono::Duration::days(365));

    assert!(authorize_replay(
        &pool,
        &first_request,
        &job_id,
        &capture_id,
        &intent_id,
        intent_hash,
        remote_request_id,
        first_reason,
    )
    .await
    .unwrap());
    let evidence_count = sqlx::query_scalar::query_scalar::<_, i64>(
        "select count(*)
           from public.trnm_online_settlement_operator_replay_requests
          where request_id = $1",
    )
    .bind(&first_request)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(evidence_count, 1);

    let changed_duplicate = authorize_replay(
        &pool,
        &first_request,
        &job_id,
        &capture_id,
        &intent_id,
        intent_hash,
        remote_request_id,
        "Changed incident material must fail closed",
    )
    .await
    .unwrap_err();
    assert_sqlstate(changed_duplicate, "23514");

    let replay_while_retryable = authorize_replay(
        &pool,
        &format!("trnm-settlement-replay-v1:{}", "2".repeat(64)),
        &job_id,
        &capture_id,
        &intent_id,
        intent_hash,
        remote_request_id,
        "A second authorization before terminal failure is forbidden",
    )
    .await
    .unwrap_err();
    assert_sqlstate(replay_while_retryable, "55000");

    let policy_revision = sqlx::query_scalar::query_scalar::<_, i64>(
        "select public.trnm_online_append_settlement_operator_policy_v1(
            3650, 1, 1, 120, 120, 2,
            'security-reviewer', 'WORLD-P0-001:OPS',
            'Reviewed retention and paging thresholds for staging qualification'
         )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(policy_revision, 2);

    let lease = sqlx::query::query(
        "select job_id, lease_generation
           from public.trnm_online_claim_settlement_job_v2('operator-worker', 30000)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(lease.get::<String, _>("job_id"), job_id);
    let lease_generation = lease.get::<i64, _>("lease_generation");
    let remote_attempt = sqlx::query_scalar::query_scalar::<_, Option<i32>>(
        "select public.trnm_online_begin_settlement_remote_attempt_v1(
            $1, 'operator-worker', $2
         )",
    )
    .bind(&job_id)
    .bind(lease_generation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remote_attempt, Some(16));
    let retry_state = sqlx::query_scalar::query_scalar::<_, Option<String>>(
        "select public.trnm_online_retry_settlement_job_v1(
            $1, 'operator-worker', $2, 'response lost again', 0
         )",
    )
    .bind(&job_id)
    .bind(lease_generation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retry_state.as_deref(), Some("dead_letter"));
    sqlx::query::query(
        "update public.trnm_online_settlement_captures
            set state = 'dead_letter', last_error = 'response lost again'
          where capture_id = $1 and state = 'active'",
    )
    .bind(&capture_id)
    .execute(&pool)
    .await
    .unwrap();

    let second_request = format!("trnm-settlement-replay-v1:{}", "3".repeat(64));
    assert!(authorize_replay(
        &pool,
        &second_request,
        &job_id,
        &capture_id,
        &intent_id,
        intent_hash,
        remote_request_id,
        "Authorize one final lookup-before-submit attempt under revised policy",
    )
    .await
    .unwrap());
    let second_evidence = sqlx::query::query(
        "select replay_generation, policy_revision
           from public.trnm_online_settlement_operator_replay_requests
          where request_id = $1",
    )
    .bind(&second_request)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(second_evidence.get::<i64, _>("replay_generation"), 2);
    assert_eq!(second_evidence.get::<i64, _>("policy_revision"), 2);

    sqlx::query::query(
        "update public.trnm_online_settlement_jobs
            set state = 'dead_letter', remote_attempts = 16,
                completed_at = clock_timestamp(),
                receipt_id = 'receipt-must-block-replay',
                receipt_hash = $2,
                receipt_json = '{}'
          where job_id = $1",
    )
    .bind(&job_id)
    .bind("f".repeat(64))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query::query(
        "update public.trnm_online_settlement_captures
            set state = 'dead_letter', last_error = 'receipt already exists'
          where capture_id = $1 and state = 'active'",
    )
    .bind(&capture_id)
    .execute(&pool)
    .await
    .unwrap();
    let receipt_replay = authorize_replay(
        &pool,
        &format!("trnm-settlement-replay-v1:{}", "4".repeat(64)),
        &job_id,
        &capture_id,
        &intent_id,
        intent_hash,
        remote_request_id,
        "A durable receipt must make operator replay impossible",
    )
    .await
    .unwrap_err();
    assert_sqlstate(receipt_replay, "55000");

    for statement in [
        "update public.trnm_online_settlement_operator_replay_requests set reason = 'tampered evidence'",
        "delete from public.trnm_online_settlement_operator_replay_requests",
        "truncate public.trnm_online_settlement_operator_replay_requests",
        "update public.trnm_online_settlement_operator_policy_revisions set reason = 'tampered policy'",
        "delete from public.trnm_online_settlement_operator_policy_revisions",
    ] {
        let error = sqlx::query::query(statement)
            .execute(&pool)
            .await
            .unwrap_err();
        assert_sqlstate(error, "55000");
    }

    let policy_truncate =
        sqlx::query::query("truncate public.trnm_online_settlement_operator_policy_revisions")
            .execute(&pool)
            .await
            .unwrap_err();
    let sqlx::Error::Database(database) = policy_truncate else {
        panic!("expected PostgreSQL truncate rejection, got {policy_truncate}");
    };
    assert!(
        matches!(database.code().as_deref(), Some("55000" | "0A000")),
        "expected append-only trigger or FK-first truncate rejection, got {:?}",
        database.code()
    );

    let current_policy = sqlx::query::query(
        "select policy_revision, retention_days
           from public.trnm_online_settlement_operator_policy_current_v1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(current_policy.get::<i64, _>("policy_revision"), 2);
    assert_eq!(current_policy.get::<i32, _>("retention_days"), 3650);

    let alerts = sqlx::query::query(
        "select remote_dead_letter, replay_total, replay_last_24h,
                dead_letter_alert, replay_volume_alert,
                earliest_retain_until
           from public.trnm_online_settlement_operator_alerts_v1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(alerts.get::<i64, _>("remote_dead_letter"), 1);
    assert_eq!(alerts.get::<i64, _>("replay_total"), 2);
    assert_eq!(alerts.get::<i64, _>("replay_last_24h"), 2);
    assert!(alerts.get::<bool, _>("dead_letter_alert"));
    assert!(alerts.get::<bool, _>("replay_volume_alert"));
    assert!(alerts
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("earliest_retain_until")
        .is_some());
}
