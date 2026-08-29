use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::SigningKey;
use hepta_research_league::{app, AppState, SecurityConfig};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn request(app: axum::Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-hepta-operator-token", "operator")
                .header("x-hepta-nakama-token", "nakama")
                .header("x-hepta-trnm-token", "trnm")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, serde_json::from_slice(&bytes).expect("json"))
}

#[tokio::test]
async fn postgres_survives_restart_and_multi_instance_outbox_claims_do_not_overlap() {
    let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
        eprintln!("HEPTA_TEST_DATABASE_URL unset; PostgreSQL integration test skipped");
        return;
    };
    let security = SecurityConfig::new("operator", "nakama").with_trnm_token("trnm");
    let first = AppState::connect(&database_url, security.clone())
        .await
        .expect("first durable state");
    sqlx::raw_sql(
        "truncate table hepta_inbox, hepta_outbox, hepta_league_state cascade;
         insert into hepta_league_state (state_key, revision, state_json)
         values ('primary', 0, '{
           \"agents\":{},\"challenges\":{},\"enrollments\":{},
           \"match_authorizations\":{},\"submissions\":{},
           \"used_agent_nonces\":[],\"events\":[],
           \"evaluator_manifests\":{},\"evaluation_reports\":{},
           \"reproduction_reports\":{},\"appeal_cases\":{},
           \"trnm_commands\":{},\"trnm_finality\":{},
           \"nakama_matches\":{},\"inbox_events\":{}
         }'::jsonb);",
    )
    .execute(
        &sqlx::PgPool::connect(&database_url)
            .await
            .expect("maintenance pool"),
    )
    .await
    .expect("clean dedicated test database");
    let second = AppState::connect(&database_url, security.clone())
        .await
        .expect("second durable state");

    let key_a = BASE64.encode(SigningKey::from_bytes(&[31; 32]).verifying_key().to_bytes());
    let key_b = BASE64.encode(SigningKey::from_bytes(&[32; 32]).verifying_key().to_bytes());
    let register_a = request(
        app(first.clone()),
        "POST",
        "/v1/hepta/agents",
        json!({
            "agent_id":"did:trnm:pg-a","owner_id":"owner-a",
            "protocol_version":"hepta_agent_protocol_v1","public_key":key_a
        }),
    );
    let register_b = request(
        app(second.clone()),
        "POST",
        "/v1/hepta/agents",
        json!({
            "agent_id":"did:trnm:pg-b","owner_id":"owner-b",
            "protocol_version":"hepta_agent_protocol_v1","public_key":key_b
        }),
    );
    let (a, b) = tokio::join!(register_a, register_b);
    assert_eq!(a.0, StatusCode::CREATED);
    assert_eq!(b.0, StatusCode::CREATED);

    let recovered = AppState::connect(&database_url, security)
        .await
        .expect("restart durable state");
    for agent in ["did:trnm:pg-a", "did:trnm:pg-b"] {
        let (status, profile) = request(
            app(recovered.clone()),
            "GET",
            &format!("/v1/hepta/research-terminal/agents/{agent}/profile"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(profile["agent"]["agent_id"], agent);
    }

    let (claims_a, claims_b) = tokio::join!(
        recovered.claim_outbox("worker-a", 1, 30),
        second.claim_outbox("worker-b", 1, 30)
    );
    let claims_a = claims_a.expect("worker a claim");
    let claims_b = claims_b.expect("worker b claim");
    assert_eq!(claims_a.len(), 1);
    assert_eq!(claims_b.len(), 1);
    assert_ne!(claims_a[0].event_id, claims_b[0].event_id);
    assert!(recovered
        .acknowledge_outbox("worker-a", claims_a[0].event_id)
        .await
        .expect("ack worker a"));
    assert!(!recovered
        .acknowledge_outbox("wrong-worker", claims_b[0].event_id)
        .await
        .expect("wrong worker rejected"));
    let maintenance = sqlx::PgPool::connect(&database_url)
        .await
        .expect("recovery maintenance pool");
    sqlx::query(
        "update hepta_outbox
         set lease_expires_at = now() - interval '1 second'
         where event_id = $1 and lease_owner = 'worker-b'",
    )
    .bind(claims_b[0].event_id)
    .execute(&maintenance)
    .await
    .expect("simulate crashed worker lease expiry");
    let replay = second
        .claim_outbox("worker-c", 1, 30)
        .await
        .expect("recovery worker claim");
    assert_eq!(replay.len(), 1);
    assert_eq!(replay[0].event_id, claims_b[0].event_id);
    assert!(second
        .acknowledge_outbox("worker-c", replay[0].event_id)
        .await
        .expect("ack replayed event"));
}
