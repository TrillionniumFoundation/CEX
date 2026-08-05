use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use sqlx::{Connection, PgConnection, Row};
use tower::ServiceExt;
use uuid::Uuid;

use super::*;
use crate::{
    app, key_rotation_signing_message,
    paper_raid_contracts::{
        canonical_json_bytes, canonical_json_sha256, human_key_registration_signing_bytes,
        human_key_revocation_signing_bytes, human_key_rotation_signing_bytes, sha256_digest,
        sign_authorship_consent, sign_consumer_user_assertion,
        team_member_acceptance_signing_bytes, AuthorshipConsentSigningV2,
        ConsumerUserAssertionClaimV2, HumanKeyRegistrationClaimV2, HumanKeyRevocationClaimV2,
        HumanKeyRotationClaimV2, TeamMemberAcceptanceSigningV2, AUTHORSHIP_CONSENT_V2,
        CONSUMER_USER_ASSERTION_V2, HUMAN_KEY_REGISTRATION_V2, HUMAN_KEY_REVOCATION_V2,
        HUMAN_KEY_ROTATION_V2, TEAM_MEMBER_ACCEPTANCE_V2,
    },
    AppState, RotateAgentKeyRequest, SecurityConfig, NAKAMA_TOKEN_HEADER, OPERATOR_TOKEN_HEADER,
    TRNM_TOKEN_HEADER, USER_ASSERTION_HEADER,
};

#[derive(Debug)]
struct Actor {
    player_id: Uuid,
    nakama_user_id: Uuid,
    binding_id: Uuid,
    subject_id: String,
    agent_id: String,
    role: String,
    agent_key: SigningKey,
    human_key: SigningKey,
    human_key_id: String,
    human_public_key: String,
    human_public_key_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlowOutcome {
    member_count: usize,
    missing_acceptance_code: String,
    tampered_acceptance_code: String,
    stale_version_code: String,
    stale_parent_code: String,
    team_status: String,
    team_version: u64,
    paper_phase: String,
    paper_version: u64,
    submission_status: String,
    authorization_epoch: u64,
    authorization_status: String,
    outbox_event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
struct FlowOptions {
    rotate_human_after_acceptance: bool,
    replace_session_epoch: bool,
    revoke_author_before_finalize: bool,
    concurrent_finalize: bool,
}

fn digest(label: &str) -> String {
    sha256_digest(label.as_bytes())
}

fn actors(member_count: usize) -> Vec<Actor> {
    (0..member_count)
        .map(|index| {
            let ordinal = index + 1;
            let human_key = SigningKey::from_bytes(&[0x40 + ordinal as u8; 32]);
            let human_public_key = BASE64.encode(human_key.verifying_key().to_bytes());
            let human_public_key_hash = sha256_digest(&human_key.verifying_key().to_bytes());
            Actor {
                player_id: Uuid::from_u128(
                    0x1000_0000_0000_4000_8000_0000_0000_0000
                        + (member_count as u128) * 0x100
                        + ordinal as u128,
                ),
                nakama_user_id: Uuid::from_u128(
                    0x2000_0000_0000_4000_8000_0000_0000_0000
                        + (member_count as u128) * 0x100
                        + ordinal as u128,
                ),
                binding_id: Uuid::from_u128(
                    0x3000_0000_0000_4000_8000_0000_0000_0000
                        + (member_count as u128) * 0x100
                        + ordinal as u128,
                ),
                subject_id: format!("oidc|paper-raid-{member_count}-{ordinal}"),
                agent_id: format!("did:trnm:paper-raid-{member_count}-{ordinal}"),
                role: format!("research-role-{ordinal}"),
                agent_key: SigningKey::from_bytes(&[0x20 + ordinal as u8; 32]),
                human_key,
                human_key_id: format!("human-key-{member_count}-{ordinal}-v1"),
                human_public_key,
                human_public_key_hash,
            }
        })
        .collect()
}

fn security() -> SecurityConfig {
    SecurityConfig::new("operator", "nakama")
        .with_trnm_token("trnm")
        .with_trusted_nakama_research_authority(
            "nakama-paper-raid-test-v1",
            SigningKey::from_bytes(&[0x75; 32])
                .verifying_key()
                .to_bytes(),
        )
        .expect("valid Nakama test authority")
}

fn signed_user_assertion(
    actor: &Actor,
    operation: &str,
    method: &str,
    path: &str,
    idempotency_key: &str,
    body_hash: String,
) -> String {
    let now = Utc::now().timestamp();
    let assertion = sign_consumer_user_assertion(
        ConsumerUserAssertionClaimV2 {
            schema: CONSUMER_USER_ASSERTION_V2.to_string(),
            assertion_id: Uuid::new_v4(),
            issuer: "hepta-test-consumer-edge".to_string(),
            audience: "hepta-paper-raid-v2".to_string(),
            subject_id: actor.subject_id.clone(),
            nakama_user_id: actor.nakama_user_id,
            player_id: actor.player_id,
            operation: operation.to_string(),
            http_method: method.to_string(),
            canonical_path: path.to_string(),
            idempotency_key: idempotency_key.to_string(),
            body_hash,
            issued_at_unix: now - 1,
            expires_at_unix: now + 120,
            nonce: idempotency_key.to_string(),
        },
        "hepta-test-consumer-edge-key-v2",
        &SigningKey::from_bytes(&[0x6c; 32]),
    )
    .expect("sign Consumer assertion");
    BASE64.encode(canonical_json_bytes(&assertion).expect("canonical assertion"))
}

async fn request(
    router: &Router,
    method: &str,
    path: &str,
    body: Value,
    user_assertion: Option<String>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header(OPERATOR_TOKEN_HEADER, "operator")
        .header(NAKAMA_TOKEN_HEADER, "nakama")
        .header(TRNM_TOKEN_HEADER, "trnm");
    if let Some(assertion) = user_assertion {
        builder = builder.header(USER_ASSERTION_HEADER, assertion);
    }
    let response = router
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "response is not JSON ({status}): {error}; {}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, value)
}

async fn user_post(
    router: &Router,
    actor: &Actor,
    operation: &str,
    path: &str,
    idempotency_key: &str,
    body: Value,
) -> (StatusCode, Value) {
    let assertion = signed_user_assertion(
        actor,
        operation,
        "POST",
        path,
        idempotency_key,
        canonical_json_sha256(&body).expect("request hash"),
    );
    request(router, "POST", path, body, Some(assertion)).await
}

async fn user_get(
    router: &Router,
    actor: &Actor,
    operation: &str,
    path: &str,
    nonce: &str,
) -> (StatusCode, Value) {
    let assertion = signed_user_assertion(actor, operation, "GET", path, nonce, sha256_digest(&[]));
    request(router, "GET", path, json!({}), Some(assertion)).await
}

fn assert_status(actual: (StatusCode, Value), expected: StatusCode) -> Value {
    assert_eq!(actual.0, expected, "unexpected response: {}", actual.1);
    actual.1
}

fn error_code(actual: (StatusCode, Value), expected: StatusCode) -> String {
    let body = assert_status(actual, expected);
    body["code"]
        .as_str()
        .unwrap_or_else(|| panic!("error response has no code: {body}"))
        .to_string()
}

async fn reset_postgres(database_url: &str) {
    let pool = sqlx::PgPool::connect(database_url)
        .await
        .expect("maintenance pool");
    sqlx::raw_sql(
        "truncate table
           hepta_nakama_research_session_completions,
           hepta_research_session_consumption_receipts,
           hepta_research_session_authorizations,
           hepta_research_session_authorization_sets,
           hepta_joint_paper_submissions,
           hepta_authorship_consents,
           hepta_paper_revisions,
           hepta_paper_work_items,
           hepta_paper_projects,
           hepta_research_team_member_acceptances,
           hepta_research_team_members,
           hepta_research_teams,
           hepta_agent_bindings,
           hepta_human_signing_keys,
           hepta_human_players,
           hepta_paper_raid_idempotency,
           hepta_inbox,
           hepta_outbox,
           hepta_league_state
         restart identity cascade;
         insert into hepta_league_state (state_key, revision, state_json)
         values ('primary', 0, '{
           \"agents\":{},\"challenges\":{},\"enrollments\":{},
           \"match_authorizations\":{},\"submissions\":{},
           \"used_agent_nonces\":[],\"events\":[],
           \"evaluator_manifests\":{},\"evaluation_reports\":{},
           \"reproduction_reports\":{},\"appeal_cases\":{},
           \"trnm_commands\":{},\"trnm_finality\":{},\"trnm_live_finality\":{},
           \"nakama_matches\":{},\"inbox_events\":{}
         }'::jsonb);",
    )
    .execute(&pool)
    .await
    .expect("reset dedicated test database");
}

async fn register_prerequisites(router: &Router, actors: &[Actor]) -> Uuid {
    for actor in actors {
        let body = json!({
            "agent_id": actor.agent_id,
            "owner_id": actor.subject_id,
            "organization_id": "paper-raid-test-lab",
            "protocol_version": "hepta_agent_protocol_v1",
            "public_key": BASE64.encode(actor.agent_key.verifying_key().to_bytes()),
            "capabilities": ["scientific_reasoning", "code_execution"]
        });
        assert_status(
            request(router, "POST", "/v1/hepta/agents", body, None).await,
            StatusCode::CREATED,
        );
    }
    let challenge = assert_status(
        request(
            router,
            "POST",
            "/v1/hepta/challenges",
            json!({
                "title": "Paper Raid public baseline reproduction",
                "description": "Public data, deterministic baseline and one ablation.",
                "ruleset_version": "paper-raid-golden-v2",
                "ruleset_hash": digest("paper-raid-ruleset"),
                "dataset_manifest_hash": digest("paper-raid-dataset"),
                "evaluator_manifest_hash": digest("paper-raid-evaluator"),
                "status": "open"
            }),
            None,
        )
        .await,
        StatusCode::CREATED,
    );
    Uuid::parse_str(challenge["challenge_id"].as_str().expect("challenge ID"))
        .expect("challenge UUID")
}

async fn create_players_and_bindings(router: &Router, actors: &[Actor]) {
    for actor in actors {
        let idempotency_key = format!("create-player-{}", actor.player_id);
        let now = Utc::now().timestamp();
        let registration = HumanKeyRegistrationClaimV2 {
            schema: HUMAN_KEY_REGISTRATION_V2.to_string(),
            player_id: actor.player_id,
            subject_id: actor.subject_id.clone(),
            nakama_user_id: actor.nakama_user_id,
            signing_key_id: actor.human_key_id.clone(),
            signing_public_key: actor.human_public_key.clone(),
            signing_public_key_hash: actor.human_public_key_hash.clone(),
            nonce: idempotency_key.clone(),
            issued_at_unix: now - 1,
            expires_at_unix: now + 300,
        };
        let proof = BASE64.encode(
            actor
                .human_key
                .sign(
                    &human_key_registration_signing_bytes(&registration)
                        .expect("registration frame"),
                )
                .to_bytes(),
        );
        let body = json!({
            "player_id": actor.player_id,
            "display_name": format!("Paper Raid author {}", actor.player_id),
            "signing_key_id": actor.human_key_id,
            "signing_public_key": actor.human_public_key,
            "key_issued_at_unix": registration.issued_at_unix,
            "key_expires_at_unix": registration.expires_at_unix,
            "key_proof_signature": proof,
            "idempotency_key": idempotency_key,
        });
        assert_status(
            user_post(
                router,
                actor,
                "create_human_player_v2",
                "/v2/hepta/players",
                &idempotency_key,
                body,
            )
            .await,
            StatusCode::CREATED,
        );

        let idempotency_key = format!("create-binding-{}", actor.binding_id);
        let body = json!({
            "binding_id": actor.binding_id,
            "player_id": actor.player_id,
            "agent_id": actor.agent_id,
            "idempotency_key": idempotency_key,
        });
        assert_status(
            user_post(
                router,
                actor,
                "create_agent_binding_v2",
                "/v2/hepta/agent-bindings",
                &idempotency_key,
                body,
            )
            .await,
            StatusCode::CREATED,
        );
    }
}

fn acceptance_body(
    actor: &Actor,
    acceptance_id: Uuid,
    team_id: Uuid,
    challenge_id: Uuid,
    slot: u32,
    compact_hash: &str,
    idempotency_key: &str,
) -> Value {
    let accepted_at_unix = Utc::now().timestamp();
    let signing = TeamMemberAcceptanceSigningV2 {
        schema: TEAM_MEMBER_ACCEPTANCE_V2.to_string(),
        acceptance_id,
        team_id,
        challenge_id,
        roster_version: 1,
        participant_slot: slot,
        player_id: actor.player_id,
        binding_id: actor.binding_id,
        agent_id: actor.agent_id.clone(),
        role: actor.role.clone(),
        collaboration_compact_hash: compact_hash.to_string(),
        signing_key_id: actor.human_key_id.clone(),
        signing_public_key: actor.human_public_key.clone(),
        signing_public_key_hash: actor.human_public_key_hash.clone(),
        accepted_at_unix,
    };
    let signature = BASE64.encode(
        actor
            .human_key
            .sign(&team_member_acceptance_signing_bytes(&signing).expect("acceptance frame"))
            .to_bytes(),
    );
    json!({
        "acceptance_id": acceptance_id,
        "expected_team_version": 1,
        "roster_version": 1,
        "participant_slot": slot,
        "binding_id": actor.binding_id,
        "role": actor.role,
        "collaboration_compact_hash": compact_hash,
        "accepted_at_unix": accepted_at_unix,
        "signature": signature,
        "idempotency_key": idempotency_key,
    })
}

async fn rotate_human_key(router: &Router, actor: &mut Actor) {
    let old_key = actor.human_key.clone();
    let old_key_hash = actor.human_public_key_hash.clone();
    let new_key = SigningKey::from_bytes(&[0x6f; 32]);
    let new_public_key = BASE64.encode(new_key.verifying_key().to_bytes());
    let new_public_key_hash = sha256_digest(&new_key.verifying_key().to_bytes());
    let idempotency_key = format!("rotate-human-{}", actor.player_id);
    let now = Utc::now().timestamp();
    let claim = HumanKeyRotationClaimV2 {
        schema: HUMAN_KEY_ROTATION_V2.to_string(),
        rotation_id: Uuid::new_v4(),
        player_id: actor.player_id,
        subject_id: actor.subject_id.clone(),
        nakama_user_id: actor.nakama_user_id,
        old_signing_key_id: actor.human_key_id.clone(),
        old_signing_public_key_hash: old_key_hash,
        new_signing_key_id: format!("{}-rotated", actor.human_key_id),
        new_signing_public_key: new_public_key.clone(),
        new_signing_public_key_hash: new_public_key_hash.clone(),
        nonce: idempotency_key.clone(),
        issued_at_unix: now - 1,
        expires_at_unix: now + 300,
    };
    let frame = human_key_rotation_signing_bytes(&claim).expect("rotation frame");
    let body = json!({
        "rotation_id": claim.rotation_id,
        "expected_player_version": 1,
        "new_signing_key_id": claim.new_signing_key_id,
        "new_signing_public_key": claim.new_signing_public_key,
        "issued_at_unix": claim.issued_at_unix,
        "expires_at_unix": claim.expires_at_unix,
        "old_key_signature": BASE64.encode(old_key.sign(&frame).to_bytes()),
        "new_key_signature": BASE64.encode(new_key.sign(&frame).to_bytes()),
        "idempotency_key": idempotency_key,
    });
    let path = format!("/v2/hepta/players/{}/signing-key/rotate", actor.player_id);
    assert_status(
        user_post(
            router,
            actor,
            "rotate_human_signing_key_v2",
            &path,
            &idempotency_key,
            body,
        )
        .await,
        StatusCode::OK,
    );
    actor.human_key = new_key;
    actor.human_key_id = claim.new_signing_key_id;
    actor.human_public_key = new_public_key;
    actor.human_public_key_hash = new_public_key_hash;
}

async fn revoke_human_key(router: &Router, actor: &Actor, expected_player_version: u64) {
    let idempotency_key = format!("revoke-human-{}", actor.player_id);
    let now = Utc::now().timestamp();
    let claim = HumanKeyRevocationClaimV2 {
        schema: HUMAN_KEY_REVOCATION_V2.to_string(),
        revocation_id: Uuid::new_v4(),
        player_id: actor.player_id,
        subject_id: actor.subject_id.clone(),
        nakama_user_id: actor.nakama_user_id,
        signing_key_id: actor.human_key_id.clone(),
        signing_public_key_hash: actor.human_public_key_hash.clone(),
        reason_hash: digest("compromise-revocation-test"),
        nonce: idempotency_key.clone(),
        issued_at_unix: now - 1,
        expires_at_unix: now + 300,
    };
    let signature = BASE64.encode(
        actor
            .human_key
            .sign(&human_key_revocation_signing_bytes(&claim).expect("revocation frame"))
            .to_bytes(),
    );
    let body = json!({
        "revocation_id": claim.revocation_id,
        "expected_player_version": expected_player_version,
        "reason_hash": claim.reason_hash,
        "issued_at_unix": claim.issued_at_unix,
        "expires_at_unix": claim.expires_at_unix,
        "signature": signature,
        "idempotency_key": idempotency_key,
    });
    let path = format!("/v2/hepta/players/{}/signing-key/revoke", actor.player_id);
    assert_status(
        user_post(
            router,
            actor,
            "revoke_human_signing_key_v2",
            &path,
            &idempotency_key,
            body,
        )
        .await,
        StatusCode::OK,
    );
}

async fn paper_raid_event_types(state: &AppState) -> Vec<String> {
    if let Some(pool) = &state.pool {
        return sqlx::query(
            "select event_type from hepta_outbox
             where schema_version = $1
             order by occurred_at, event_id",
        )
        .bind(PAPER_RAID_EVENT_SCHEMA_V2)
        .fetch_all(pool)
        .await
        .expect("Paper Raid outbox query")
        .into_iter()
        .map(|row| row.get("event_type"))
        .collect();
    }
    state
        .paper_raid
        .read()
        .await
        .events
        .iter()
        .map(|event| event.event_type.clone())
        .collect()
}

async fn issue_replace_and_consume_session(
    router: &Router,
    actors: &[Actor],
    member_count: usize,
    paper_id: Uuid,
    expected_paper_version: u64,
    replace_session_epoch: bool,
) -> (u64, String) {
    let session_id = format!("paper-raid-{member_count}-session");
    let issue_key = format!("issue-session-{member_count}");
    let mut set = assert_status(
        user_post(
            router,
            &actors[0],
            "issue_research_session_authorization_set_v1",
            "/v2/hepta/research-session-authorizations",
            &issue_key,
            json!({
                "session_id":session_id,
                "paper_project_id":paper_id,
                "expected_paper_version":expected_paper_version,
                "expected_team_version":2,
                "ttl_seconds":300,
                "idempotency_key":issue_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(set["team_roster_version"], 1);
    assert_eq!(set["roster_version"], 1);

    if replace_session_epoch {
        let replace_path =
            format!("/v2/hepta/research-session-authorizations/{session_id}/replace");
        let unchanged_key = format!("replace-unchanged-{member_count}");
        assert_eq!(
            error_code(
                user_post(
                    router,
                    &actors[0],
                    "replace_research_session_authorization_set_v1",
                    &replace_path,
                    &unchanged_key,
                    json!({
                        "paper_project_id":paper_id,
                        "expected_paper_version":expected_paper_version,
                        "expected_team_version":2,
                        "previous_roster_version":1,
                        "disconnected_participant_slot":1,
                        "ttl_seconds":300,
                        "idempotency_key":unchanged_key,
                    }),
                )
                .await,
                StatusCode::CONFLICT,
            ),
            "replacement_epoch_agent_key_scope"
        );
        let new_agent_key = SigningKey::from_bytes(&[0x7a; 32]);
        let nonce = format!("agent-rotation-{member_count}");
        let mut rotation = RotateAgentKeyRequest {
            agent_id: actors[0].agent_id.clone(),
            new_public_key: BASE64.encode(new_agent_key.verifying_key().to_bytes()),
            nonce,
            signature: String::new(),
        };
        rotation.signature = BASE64.encode(
            actors[0]
                .agent_key
                .sign(key_rotation_signing_message(&rotation).as_bytes())
                .to_bytes(),
        );
        assert_status(
            request(
                router,
                "POST",
                "/v1/hepta/agents/rotate-key",
                json!({
                    "agent_id":rotation.agent_id,
                    "new_public_key":rotation.new_public_key,
                    "nonce":rotation.nonce,
                    "signature":rotation.signature,
                }),
                None,
            )
            .await,
            StatusCode::OK,
        );
        let replace_key = format!("replace-session-{member_count}");
        set = assert_status(
            user_post(
                router,
                &actors[0],
                "replace_research_session_authorization_set_v1",
                &replace_path,
                &replace_key,
                json!({
                    "paper_project_id":paper_id,
                    "expected_paper_version":expected_paper_version,
                    "expected_team_version":2,
                    "previous_roster_version":1,
                    "disconnected_participant_slot":1,
                    "ttl_seconds":300,
                    "idempotency_key":replace_key,
                }),
            )
            .await,
            StatusCode::CREATED,
        );
        assert_eq!(set["roster_version"], 2);
        assert_eq!(set["supersedes_roster_version"], 1);
    }

    let ids = set["members"]
        .as_array()
        .expect("authorization members")
        .iter()
        .map(|member| member["authorization"]["claim"]["authorization_id"].clone())
        .collect::<Vec<_>>();
    let consume = assert_status(
        request(
            router,
            "POST",
            "/v2/hepta/nakama/research-session-authorizations/consumed",
            json!({
                "schema":"hepta.paper_raid.research_session_consumption.v1",
                "session_id":session_id,
                "roster_version":set["roster_version"],
                "roster_root":set["roster_root"],
                "authorization_ids":ids,
                "consumed_at_unix":Utc::now().timestamp(),
                "idempotency_key":format!("consume-session-{member_count}"),
            }),
            None,
        )
        .await,
        StatusCode::OK,
    );
    (
        consume["session_roster_version"]
            .as_u64()
            .expect("consumed epoch"),
        "consumed".to_string(),
    )
}

async fn run_full_flow(state: AppState, member_count: usize, options: FlowOptions) -> FlowOutcome {
    let router = app(state.clone());
    let mut actors = actors(member_count);
    let challenge_id = register_prerequisites(&router, &actors).await;
    create_players_and_bindings(&router, &actors).await;

    let base = (member_count as u128) * 0x100;
    let team_id = Uuid::from_u128(0x4000_0000_0000_4000_8000_0000_0000_0000 + base);
    let paper_id = Uuid::from_u128(0x5000_0000_0000_4000_8000_0000_0000_0000 + base);
    let revision_id = Uuid::from_u128(0x6000_0000_0000_4000_8000_0000_0000_0000 + base);
    let work_item_id = Uuid::from_u128(0x7000_0000_0000_4000_8000_0000_0000_0000 + base);
    let compact_hash = digest(&format!("compact-{member_count}"));
    let create_team_key = format!("create-team-{member_count}");
    let create_team_body = json!({
        "team_id": team_id,
        "challenge_id": challenge_id,
        "collaboration_compact_hash": compact_hash,
        "members": actors.iter().enumerate().map(|(index, actor)| json!({
            "participant_slot": index + 1,
            "player_id": actor.player_id,
            "binding_id": actor.binding_id,
            "role": actor.role,
        })).collect::<Vec<_>>(),
        "idempotency_key": create_team_key,
    });
    assert_status(
        user_post(
            &router,
            &actors[0],
            "create_research_team_v2",
            "/v2/hepta/teams",
            &create_team_key,
            create_team_body,
        )
        .await,
        StatusCode::CREATED,
    );

    let lock_path = format!("/v2/hepta/teams/{team_id}/lock");
    let lock_missing_key = format!("lock-missing-{member_count}");
    let missing_acceptance_code = error_code(
        user_post(
            &router,
            &actors[0],
            "lock_research_team_v2",
            &lock_path,
            &lock_missing_key,
            json!({"expected_version":1,"idempotency_key":lock_missing_key}),
        )
        .await,
        StatusCode::CONFLICT,
    );

    let acceptance_path = format!("/v2/hepta/teams/{team_id}/member-acceptances");
    let tamper_key = format!("accept-tamper-{member_count}");
    let tampered_acceptance_code = error_code(
        user_post(
            &router,
            &actors[0],
            "accept_research_team_membership_v2",
            &acceptance_path,
            &tamper_key,
            acceptance_body(
                &actors[0],
                Uuid::new_v4(),
                team_id,
                challenge_id,
                1,
                &digest("wrong compact"),
                &tamper_key,
            ),
        )
        .await,
        StatusCode::CONFLICT,
    );

    for index in 0..actors.len() {
        let key = format!("accept-{member_count}-{}", index + 1);
        assert_status(
            user_post(
                &router,
                &actors[index],
                "accept_research_team_membership_v2",
                &acceptance_path,
                &key,
                acceptance_body(
                    &actors[index],
                    Uuid::new_v4(),
                    team_id,
                    challenge_id,
                    (index + 1) as u32,
                    &compact_hash,
                    &key,
                ),
            )
            .await,
            StatusCode::CREATED,
        );
        if index == 0 && options.rotate_human_after_acceptance {
            rotate_human_key(&router, &mut actors[0]).await;
            let reaccept_key = format!("reaccept-{member_count}-1");
            assert_status(
                user_post(
                    &router,
                    &actors[0],
                    "accept_research_team_membership_v2",
                    &acceptance_path,
                    &reaccept_key,
                    acceptance_body(
                        &actors[0],
                        Uuid::new_v4(),
                        team_id,
                        challenge_id,
                        1,
                        &compact_hash,
                        &reaccept_key,
                    ),
                )
                .await,
                StatusCode::CREATED,
            );
        }
    }

    let lock_key = format!("lock-team-{member_count}");
    let team = assert_status(
        user_post(
            &router,
            &actors[0],
            "lock_research_team_v2",
            &lock_path,
            &lock_key,
            json!({"expected_version":1,"idempotency_key":lock_key}),
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(team["status"], "locked");
    assert_eq!(team["version"], 2);

    let create_paper_key = format!("create-paper-{member_count}");
    assert_status(
        user_post(
            &router,
            &actors[0],
            "create_paper_project_v2",
            "/v2/hepta/papers",
            &create_paper_key,
            json!({
                "paper_project_id": paper_id,
                "team_id": team_id,
                "title": format!("Paper Raid {member_count}-author paper"),
                "target_format": "workshop-short-paper-v1",
                "idempotency_key": create_paper_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );

    let (authorization_epoch, authorization_status) = issue_replace_and_consume_session(
        &router,
        &actors,
        member_count,
        paper_id,
        1,
        options.replace_session_epoch,
    )
    .await;

    let transition_path = format!("/v2/hepta/papers/{paper_id}/transition");
    let stale_key = format!("stale-transition-{member_count}");
    let stale_version_code = error_code(
        user_post(
            &router,
            &actors[0],
            "transition_paper_project_v2",
            &transition_path,
            &stale_key,
            json!({"expected_version":9,"next_phase":"preregistering","idempotency_key":stale_key}),
        )
        .await,
        StatusCode::CONFLICT,
    );
    let mut paper_version = 1_u64;
    for phase in ["preregistering", "researching", "experimenting", "drafting"] {
        let key = format!("phase-{member_count}-{phase}");
        let paper = assert_status(
            user_post(
                &router,
                &actors[0],
                "transition_paper_project_v2",
                &transition_path,
                &key,
                json!({"expected_version":paper_version,"next_phase":phase,"idempotency_key":key}),
            )
            .await,
            StatusCode::OK,
        );
        paper_version += 1;
        assert_eq!(paper["version"], paper_version);
    }

    let work_path = format!("/v2/hepta/papers/{paper_id}/work-items");
    let work_key = format!("create-work-{member_count}");
    let work_body = json!({
        "work_item_id": work_item_id,
        "expected_paper_version": paper_version,
        "kind": "experiment",
        "title": "Reproduce baseline and retain every run",
        "assigned_player_id": actors[0].player_id,
        "assigned_binding_id": actors[0].binding_id,
        "idempotency_key": work_key,
    });
    let work = assert_status(
        user_post(
            &router,
            &actors[0],
            "create_paper_work_item_v2",
            &work_path,
            &work_key,
            work_body.clone(),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(work["version"], 1);
    paper_version += 1;
    let replay = assert_status(
        user_post(
            &router,
            &actors[0],
            "create_paper_work_item_v2",
            &work_path,
            &work_key,
            work_body,
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(replay, work);

    let work_transition_path = format!("/v2/hepta/work-items/{work_item_id}/transition");
    let mut work_version = 1_u64;
    for status in ["in_progress", "review", "accepted"] {
        let key = format!("work-{member_count}-{status}");
        let artifact = (status == "accepted").then(|| digest("accepted-work-artifact"));
        let item = assert_status(
            user_post(
                &router,
                &actors[0],
                "transition_paper_work_item_v2",
                &work_transition_path,
                &key,
                json!({
                    "expected_version":work_version,
                    "next_status":status,
                    "artifact_manifest_hash":artifact,
                    "idempotency_key":key,
                }),
            )
            .await,
            StatusCode::OK,
        );
        work_version += 1;
        assert_eq!(item["version"], work_version);
    }

    let revision_path = format!("/v2/hepta/papers/{paper_id}/revisions");
    let revision_key = format!("revision-{member_count}-1");
    assert_status(
        user_post(
            &router,
            &actors[0],
            "create_paper_revision_v2",
            &revision_path,
            &revision_key,
            json!({
                "revision_id":revision_id,
                "expected_paper_version":paper_version,
                "parent_revision_id":null,
                "source_manifest_hash":digest("source-manifest"),
                "artifact_manifest_hash":digest("artifact-manifest"),
                "bibliography_hash":digest("bibliography"),
                "claim_evidence_graph_hash":digest("claim-evidence"),
                "idempotency_key":revision_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    paper_version += 1;
    let stale_revision_key = format!("revision-{member_count}-stale");
    let stale_parent_code = error_code(
        user_post(
            &router,
            &actors[0],
            "create_paper_revision_v2",
            &revision_path,
            &stale_revision_key,
            json!({
                "revision_id":Uuid::new_v4(),
                "expected_paper_version":paper_version,
                "parent_revision_id":null,
                "source_manifest_hash":digest("source-manifest-stale"),
                "artifact_manifest_hash":digest("artifact-manifest-stale"),
                "bibliography_hash":digest("bibliography-stale"),
                "claim_evidence_graph_hash":digest("claim-evidence-stale"),
                "idempotency_key":stale_revision_key,
            }),
        )
        .await,
        StatusCode::CONFLICT,
    );

    for phase in ["integrity_review", "reproducing", "author_approval"] {
        let key = format!("phase-{member_count}-{phase}");
        assert_status(
            user_post(
                &router,
                &actors[0],
                "transition_paper_project_v2",
                &transition_path,
                &key,
                json!({"expected_version":paper_version,"next_phase":phase,"idempotency_key":key}),
            )
            .await,
            StatusCode::OK,
        );
        paper_version += 1;
    }

    let promote_path = format!("/v2/hepta/papers/{paper_id}/revisions/{revision_id}/promote");
    let promote_key = format!("promote-{member_count}");
    let authors = actors
        .iter()
        .enumerate()
        .map(|(index, _actor)| {
            let slot = ((index + 1) % member_count) + 1;
            let player_id = actors[slot - 1].player_id;
            json!({
                "author_order": index + 1,
                "participant_slot": slot,
                "player_id": player_id,
                "display_name": format!("Paper Raid author {player_id}"),
                "credit_roles": ["methodology", "writing_review_editing"]
            })
        })
        .collect::<Vec<_>>();
    let promoted = assert_status(
        user_post(
            &router,
            &actors[0],
            "promote_paper_release_candidate_v2",
            &promote_path,
            &promote_key,
            json!({
                "expected_paper_version":paper_version,
                "expected_revision_version":1,
                "title":format!("Paper Raid {member_count}-author reproducible study"),
                "abstract_text":"A complete reproducible research collaboration test.",
                "collaboration_compact_hash":compact_hash,
                "research_protocol_snapshot_hash":digest("research-protocol"),
                "ethics_disclosure_hash":digest("ethics-disclosure"),
                "coi_disclosure_hash":digest("coi-disclosure"),
                "contribution_ledger_hash":digest("contribution-ledger"),
                "ai_disclosure_hash":digest("ai-disclosure"),
                "license":"CC-BY-4.0",
                "authors":authors,
                "idempotency_key":promote_key,
            }),
        )
        .await,
        StatusCode::OK,
    );
    paper_version += 1;
    let release_candidate_hash = promoted["release_candidate_hash"]
        .as_str()
        .expect("release candidate hash")
        .to_string();

    let consent_path = format!("/v2/hepta/papers/{paper_id}/author-consents");
    for (index, actor) in actors.iter().enumerate() {
        let consent_id =
            Uuid::from_u128(0x8000_0000_0000_4000_8000_0000_0000_0000 + base + index as u128 + 1);
        let signed_at_unix = Utc::now().timestamp();
        let signing = AuthorshipConsentSigningV2 {
            schema: AUTHORSHIP_CONSENT_V2.to_string(),
            consent_id,
            paper_project_id: paper_id,
            revision_id,
            player_id: actor.player_id,
            signing_key_id: actor.human_key_id.clone(),
            signing_public_key: actor.human_public_key.clone(),
            signing_public_key_hash: actor.human_public_key_hash.clone(),
            release_candidate_hash: release_candidate_hash.clone(),
            signed_at_unix,
        };
        let key = format!("consent-{member_count}-{}", index + 1);
        let body = json!({
            "consent_id":consent_id,
            "expected_paper_version":paper_version,
            "revision_id":revision_id,
            "player_id":actor.player_id,
            "signing_key_id":actor.human_key_id,
            "signing_public_key":actor.human_public_key,
            "signing_public_key_hash":actor.human_public_key_hash,
            "release_candidate_hash":release_candidate_hash,
            "signed_at_unix":signed_at_unix,
            "signature":sign_authorship_consent(&signing, &actor.human_key).expect("consent signature"),
            "idempotency_key":key,
        });
        assert_status(
            user_post(
                &router,
                actor,
                "create_authorship_consent_v2",
                &consent_path,
                &key,
                body,
            )
            .await,
            StatusCode::CREATED,
        );
        paper_version += 1;
    }

    if options.revoke_author_before_finalize {
        let expected_player_version = if options.rotate_human_after_acceptance {
            2
        } else {
            1
        };
        revoke_human_key(&router, &actors[0], expected_player_version).await;
    }

    let finalize_path = format!("/v2/hepta/papers/{paper_id}/finalize");
    let finalize = |suffix: &str| {
        let key = format!("finalize-{member_count}-{suffix}");
        let body = json!({
            "submission_id":Uuid::new_v4(),
            "expected_paper_version":paper_version,
            "revision_id":revision_id,
            "release_candidate_hash":release_candidate_hash,
            "idempotency_key":key,
        });
        (key, body)
    };
    let submission = if options.concurrent_finalize {
        let (key_a, body_a) = finalize("a");
        let (key_b, body_b) = finalize("b");
        let first = user_post(
            &router,
            &actors[0],
            "finalize_joint_paper_submission_v2",
            &finalize_path,
            &key_a,
            body_a,
        );
        let second = user_post(
            &router,
            &actors[0],
            "finalize_joint_paper_submission_v2",
            &finalize_path,
            &key_b,
            body_b,
        );
        let (first, second) = tokio::join!(first, second);
        let mut responses = [first, second];
        responses.sort_by_key(|response| response.0.as_u16());
        assert_eq!(responses[0].0, StatusCode::CREATED);
        assert_eq!(responses[1].0, StatusCode::CONFLICT);
        assert_eq!(responses[1].1["code"], "aggregate_version_conflict");
        responses[0].1.clone()
    } else {
        let (key, body) = finalize("single");
        let expected = if options.revoke_author_before_finalize {
            StatusCode::ACCEPTED
        } else {
            StatusCode::CREATED
        };
        assert_status(
            user_post(
                &router,
                &actors[0],
                "finalize_joint_paper_submission_v2",
                &finalize_path,
                &key,
                body,
            )
            .await,
            expected,
        )
    };
    let expected_submission_status = if options.revoke_author_before_finalize {
        "integrity_hold"
    } else {
        "submission_ready"
    };
    assert_eq!(submission["status"], expected_submission_status);

    let paper_path = format!("/v2/hepta/papers/{paper_id}");
    assert_eq!(
        request(&router, "GET", &paper_path, json!({}), None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let paper = assert_status(
        user_get(
            &router,
            &actors[0],
            "get_paper_project_v2",
            &paper_path,
            &format!("get-paper-{member_count}"),
        )
        .await,
        StatusCode::OK,
    );

    FlowOutcome {
        member_count,
        missing_acceptance_code,
        tampered_acceptance_code,
        stale_version_code,
        stale_parent_code,
        team_status: team["status"].as_str().expect("team status").to_string(),
        team_version: team["version"].as_u64().expect("team version"),
        paper_phase: paper["phase"].as_str().expect("paper phase").to_string(),
        paper_version: paper["version"].as_u64().expect("paper version"),
        submission_status: submission["status"]
            .as_str()
            .expect("submission status")
            .to_string(),
        authorization_epoch,
        authorization_status,
        outbox_event_types: paper_raid_event_types(&state).await,
    }
}

#[tokio::test]
async fn memory_three_author_golden_flow_is_complete_and_fail_closed() {
    let outcome = run_full_flow(AppState::new(security()), 3, FlowOptions::default()).await;
    assert_eq!(outcome.team_status, "locked");
    assert_eq!(outcome.paper_phase, "submission_ready");
    assert_eq!(outcome.submission_status, "submission_ready");
    assert_eq!(outcome.authorization_epoch, 1);
    assert_eq!(outcome.authorization_status, "consumed");
    assert_eq!(
        outcome.missing_acceptance_code,
        "team_acceptances_incomplete"
    );
    assert_eq!(outcome.tampered_acceptance_code, "stale_team_proposal");
    assert_eq!(outcome.stale_version_code, "aggregate_version_conflict");
    assert_eq!(outcome.stale_parent_code, "stale_parent_revision");
}

#[tokio::test]
async fn postgres_matches_memory_and_covers_four_five_restart_concurrency_and_hold() {
    let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
        eprintln!("HEPTA_TEST_DATABASE_URL unset; Paper Raid PostgreSQL conformance skipped");
        return;
    };
    let mut lock = PgConnection::connect(&database_url)
        .await
        .expect("PostgreSQL test lock connection");
    sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("serialize Hepta PostgreSQL tests");

    let memory = run_full_flow(AppState::new(security()), 3, FlowOptions::default()).await;
    let pg_state = AppState::connect(&database_url, security())
        .await
        .expect("Paper Raid PostgreSQL state");
    reset_postgres(&database_url).await;
    let postgres = run_full_flow(pg_state, 3, FlowOptions::default()).await;
    assert_eq!(postgres, memory);

    reset_postgres(&database_url).await;
    let four_state = AppState::connect(&database_url, security())
        .await
        .expect("four-author state");
    let four = run_full_flow(
        four_state,
        4,
        FlowOptions {
            rotate_human_after_acceptance: true,
            replace_session_epoch: true,
            concurrent_finalize: true,
            ..FlowOptions::default()
        },
    )
    .await;
    assert_eq!(four.member_count, 4);
    assert_eq!(four.paper_phase, "submission_ready");
    assert_eq!(four.authorization_epoch, 2);

    reset_postgres(&database_url).await;
    let five_state = AppState::connect(&database_url, security())
        .await
        .expect("five-author state");
    let five = run_full_flow(
        five_state.clone(),
        5,
        FlowOptions {
            revoke_author_before_finalize: true,
            ..FlowOptions::default()
        },
    )
    .await;
    assert_eq!(five.member_count, 5);
    assert_eq!(five.paper_phase, "integrity_hold");
    assert_eq!(five.submission_status, "integrity_hold");

    let restarted = AppState::connect(&database_url, security())
        .await
        .expect("restart Paper Raid state");
    let five_actors = actors(5);
    let five_paper_id = Uuid::from_u128(0x5000_0000_0000_4000_8000_0000_0000_0000 + 5 * 0x100);
    let paper_path = format!("/v2/hepta/papers/{five_paper_id}");
    let recovered = assert_status(
        user_get(
            &app(restarted.clone()),
            &five_actors[0],
            "get_paper_project_v2",
            &paper_path,
            "restart-read-five",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(recovered["phase"], "integrity_hold");

    let claimed = five_state
        .claim_outbox("paper-raid-crashed-worker", 1, 30)
        .await
        .expect("claim Paper Raid outbox");
    assert_eq!(claimed.len(), 1);
    sqlx::query(
        "update hepta_outbox set lease_expires_at = now() - interval '1 second'
         where event_id = $1",
    )
    .bind(claimed[0].event_id)
    .execute(restarted.pool.as_ref().expect("PostgreSQL pool"))
    .await
    .expect("expire crashed lease");
    let recovered_claim = restarted
        .claim_outbox("paper-raid-recovery-worker", 1, 30)
        .await
        .expect("recover Paper Raid outbox");
    assert_eq!(recovered_claim.len(), 1);
    assert_eq!(recovered_claim[0].event_id, claimed[0].event_id);

    sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("release Hepta PostgreSQL test lock");
}
