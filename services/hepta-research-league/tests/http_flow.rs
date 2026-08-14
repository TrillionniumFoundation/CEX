use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use hepta_research_league::{
    app, key_rotation_signing_message, submission_signing_message, AppState, RotateAgentKeyRequest,
    SubmitArtifactRequest, NAKAMA_TOKEN_HEADER, OPERATOR_TOKEN_HEADER,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

const HASH_A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const HASH_C: &str = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const EVIDENCE_RULESET_HASH: &str =
    "sha256:54a740273a82d56938a20db1669b236e8b3724defffe32fb91768632f7994453";
const EVIDENCE_DATASET_HASH: &str =
    "sha256:1363088a76b2dd5c77b04b2620a4806100f1125c482a3e6413691d1a8c372a75";
const EVIDENCE_EVALUATOR_HASH: &str =
    "sha256:7d7f0096261132ceda30e66586e49129aaa99bb979772ceec9b2940eda21261e";

async fn request_json(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    request_json_with_tokens(
        app,
        method,
        uri,
        body,
        "hepta-test-operator-token",
        "hepta-test-nakama-token",
    )
    .await
}

async fn request_json_with_tokens(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Value,
    operator_token: &str,
    nakama_token: &str,
) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .header(OPERATOR_TOKEN_HEADER, operator_token)
                .header(NAKAMA_TOKEN_HEADER, nakama_token)
                .body(Body::from(body.to_string()))
                .expect("build request"),
        )
        .await
        .expect("request succeeds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let body = serde_json::from_slice(&bytes).expect("response is json");
    (status, body)
}

async fn request_json_without_service_tokens(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("build request"),
        )
        .await
        .expect("request succeeds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let body = serde_json::from_slice(&bytes).expect("response is json");
    (status, body)
}

async fn request_status_with_operator_token(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Value,
) -> StatusCode {
    app.oneshot(
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header(OPERATOR_TOKEN_HEADER, "hepta-test-operator-token")
            .body(Body::from(body.to_string()))
            .expect("build request"),
    )
    .await
    .expect("request succeeds")
    .status()
}

fn evidence_audit_ruleset() -> Value {
    json!({
        "schema": "hepta.challenge.ruleset.v1",
        "template": "evidence-audit",
        "duration_seconds": 2700,
        "grace_seconds": 900,
        "phase_gates": [
            {"transition":"preregistering_to_researching","requirements":[
                {"kind":"work_items","minimum":1},
                {"kind":"artifact_manifests","minimum":1}
            ]},
            {"transition":"researching_to_experimenting","requirements":[
                {"kind":"evidence_cards","minimum":2},
                {"kind":"citations","minimum":2},
                {"kind":"claims","minimum":2}
            ]},
            {"transition":"experimenting_to_drafting","requirements":[
                {"kind":"artifact_manifests","minimum":1}
            ]},
            {"transition":"drafting_to_integrity_review","requirements":[
                {"kind":"all_work_items_terminal","minimum":1},
                {"kind":"section_revisions","minimum":1},
                {"kind":"paper_revisions","minimum":1}
            ]},
            {"transition":"integrity_review_to_reproducing","requirements":[
                {"kind":"approving_section_reviews","minimum":1},
                {"kind":"section_merges","minimum":1}
            ]},
            {"transition":"reproducing_to_author_approval","requirements":[
                {"kind":"paper_revision_covers_section_merges","minimum":1}
            ]}
        ],
        "victory_requirements": [
            {"kind":"accepted_work_items","minimum":1},
            {"kind":"evidence_cards","minimum":2},
            {"kind":"citations","minimum":2},
            {"kind":"claims","minimum":2},
            {"kind":"release_candidate","minimum":1},
            {"kind":"all_author_consents","minimum":1},
            {"kind":"paper_revision_covers_section_merges","minimum":1}
        ],
        "gameplay": {
            "difficulty": "introductory",
            "objective": "Audit core claims, citations, licenses, and artifact provenance.",
            "risk": "Citation mismatch and unsupported core claims.",
            "modifiers": ["core-claim-coverage","license-audit","provenance-chain"],
            "victory_summary": "Every core claim is evidence-bound and every citation and provenance hard gate passes.",
            "role_resources": {
                "captain_focus": 2,
                "evidence_focus": 4,
                "experiment_focus": 2,
                "run_budget": 2,
                "retained_failure_focus_refund": 1
            }
        }
    })
}

fn evidence_audit_challenge(status: &str) -> Value {
    json!({
        "title": "Paper Raid: Evidence and Citation Audit",
        "description": "template=evidence-audit; paper-raid-alpha-template-evidence-audit-v1",
        "ruleset_version": "paper-raid-evidence-audit-v1",
        "ruleset_hash": EVIDENCE_RULESET_HASH,
        "dataset_manifest_hash": EVIDENCE_DATASET_HASH,
        "evaluator_manifest_hash": EVIDENCE_EVALUATOR_HASH,
        "ruleset": evidence_audit_ruleset(),
        "status": status
    })
}

fn evidence_audit_activation_request() -> Value {
    json!({
        "schema": "hepta.challenge_pack.activation_request.v1",
        "template": "evidence-audit",
        "pack_id": "paper-raid-evidence-audit-seeded-v1",
        "expected_status": "draft",
        "requested_status": "open",
        "ruleset_version": "paper-raid-evidence-audit-v1",
        "ruleset_hash": EVIDENCE_RULESET_HASH,
        "dataset_manifest_hash": EVIDENCE_DATASET_HASH,
        "evaluator_manifest_hash": EVIDENCE_EVALUATOR_HASH,
        "candidate": {
            "schema": "trnm.paper-raid.current-candidate-binding.v2",
            "state": "immutable_candidate_pending_evidence",
            "integration_base_revision": "1111111111111111111111111111111111111111",
            "integration_source_tree": "2222222222222222222222222222222222222222",
            "hepta_base_revision": "3333333333333333333333333333333333333333",
            "hepta_source_tree": "4444444444444444444444444444444444444444",
            "component_pins_authoritative": true,
            "working_tree_clean": true,
            "tracked_image_lock_status": "unbound",
            "source_fileset_sha256": "sha256:5555555555555555555555555555555555555555555555555555555555555555",
            "release_image_lock_status": "locked",
            "release_id": "paper-raid-test-candidate-v1",
            "release_image_lock_sha256": "sha256:6666666666666666666666666666666666666666666666666666666666666666",
            "release_provenance_sha256": "sha256:7777777777777777777777777777777777777777777777777777777777777777"
        },
        "evidence": {
            "schema": "hepta.challenge_pack.activation_evidence.v1",
            "source_catalog_sha256": "sha256:fc649c1f55ef484bc2f8baf279dec1c668eaa610691ac4234a74df1d90aaa6bb",
            "pack_manifest_sha256": "sha256:69a695a6a75dd71e2c53c7a832298ab2ecbcd4086fa4ef653914dd2e5770b37c",
            "cas_activation_receipt_schema": "hepta.challenge_pack.cas_activation_receipt.v1",
            "cas_activation_receipt_sha256": "sha256:8888888888888888888888888888888888888888888888888888888888888888",
            "cas_activation_catalog_patch_schema": "hepta.challenge_pack.activation_catalog_patch.v2",
            "cas_activation_catalog_patch_sha256": "sha256:9999999999999999999999999999999999999999999999999999999999999999",
            "cas_all_packs_verified": true,
            "cas_scoped_readback": true,
            "cas_pack_count": 3,
            "cas_object_count": 26,
            "strict_review_evidence_schema": "trnm.paper-raid.strict-review-evidence.v1",
            "strict_review_evidence_sha256": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "cross_paper_denial_receipt_sha256": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }
    })
}

#[tokio::test]
async fn evidence_audit_pack_activation_is_authenticated_atomic_and_exactly_idempotent() {
    let app = app(AppState::default());
    let (status, challenge) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/challenges",
        evidence_audit_challenge("draft"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let challenge_id = challenge["challenge_id"].as_str().expect("challenge id");
    let path = format!("/v1/hepta/operator/challenges/{challenge_id}/pack-activation");
    let request = evidence_audit_activation_request();

    let (status, unauthorized) =
        request_json_without_service_tokens(app.clone(), "POST", &path, request.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(unauthorized["code"], "operator_auth_failed");

    let mut unknown = request.clone();
    unknown["unexpected"] = json!(true);
    assert_eq!(
        request_status_with_operator_token(app.clone(), "POST", &path, unknown).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let (status, activated) = request_json(app.clone(), "POST", &path, request.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        activated["schema"],
        "hepta.challenge_pack.activation_record.v1"
    );
    assert_eq!(activated["challenge_id"], challenge_id);
    assert_eq!(activated["previous_status"], "draft");
    assert_eq!(activated["activated_status"], "open");
    assert_eq!(activated["request"], request);

    let (status, exact) = request_json(app.clone(), "GET", &path, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(exact, activated);
    let (status, unauthorized) =
        request_json_without_service_tokens(app.clone(), "GET", &path, json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(unauthorized["code"], "operator_auth_failed");

    let (status, replayed) = request_json(app.clone(), "POST", &path, request.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed, activated);

    let (status, opened) = request_json(
        app.clone(),
        "GET",
        &format!("/v1/hepta/challenges/{challenge_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(opened["status"], "open");

    let (status, events) = request_json(app.clone(), "GET", "/v1/hepta/events", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        events
            .as_array()
            .expect("events")
            .iter()
            .filter(|event| event["event_type"] == "hepta.challenge_pack.activated.v1")
            .count(),
        1
    );

    let mut conflicting = request;
    conflicting["evidence"]["cross_paper_denial_receipt_sha256"] =
        json!("sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc");
    let (status, conflict) = request_json(app, "POST", &path, conflicting).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        conflict["code"],
        "challenge_pack_activation_replay_conflict"
    );
}

#[tokio::test]
async fn pack_activation_rejects_non_evidence_challenges_and_status_drift() {
    let app = app(AppState::default());
    let (status, benchmark) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/challenges",
        json!({
            "title": "Paper Raid: Benchmark and Ablation",
            "description": "template=benchmark-ablation",
            "ruleset_version": "paper-raid-benchmark-ablation-v1",
            "ruleset_hash": HASH_A,
            "dataset_manifest_hash": HASH_B,
            "evaluator_manifest_hash": HASH_C,
            "status": "draft"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let benchmark_id = benchmark["challenge_id"].as_str().expect("challenge id");
    let benchmark_path = format!("/v1/hepta/operator/challenges/{benchmark_id}/pack-activation");
    let (status, mismatch) = request_json(
        app.clone(),
        "POST",
        &benchmark_path,
        evidence_audit_activation_request(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        mismatch["code"],
        "challenge_pack_activation_binding_mismatch"
    );
    let (status, unchanged) = request_json(
        app.clone(),
        "GET",
        &format!("/v1/hepta/challenges/{benchmark_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unchanged["status"], "draft");

    let (status, replication) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/challenges",
        json!({
            "title": "Paper Raid: Independent Replication",
            "description": "template=replication",
            "ruleset_version": "paper-raid-replication-v1",
            "ruleset_hash": HASH_A,
            "dataset_manifest_hash": HASH_B,
            "evaluator_manifest_hash": HASH_C,
            "status": "draft"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let replication_id = replication["challenge_id"].as_str().expect("challenge id");
    let (status, mismatch) = request_json(
        app.clone(),
        "POST",
        &format!("/v1/hepta/operator/challenges/{replication_id}/pack-activation"),
        evidence_audit_activation_request(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        mismatch["code"],
        "challenge_pack_activation_binding_mismatch"
    );
    let (status, unchanged) = request_json(
        app.clone(),
        "GET",
        &format!("/v1/hepta/challenges/{replication_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unchanged["status"], "draft");

    let (status, already_open) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/challenges",
        evidence_audit_challenge("open"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let open_id = already_open["challenge_id"].as_str().expect("challenge id");
    let (status, drift) = request_json(
        app.clone(),
        "POST",
        &format!("/v1/hepta/operator/challenges/{open_id}/pack-activation"),
        evidence_audit_activation_request(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(drift["code"], "challenge_pack_activation_status_drift");

    let mut wrong_pack = evidence_audit_activation_request();
    wrong_pack["template"] = json!("replication");
    wrong_pack["pack_id"] = json!("paper-raid-replication-seeded-v1");
    let (status, invalid) = request_json(
        app,
        "POST",
        &format!("/v1/hepta/operator/challenges/{open_id}/pack-activation"),
        wrong_pack,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        invalid["code"],
        "challenge_pack_activation_contract_invalid"
    );
}

#[tokio::test]
async fn operator_challenge_bootstrap_supports_exact_idempotent_lookup() {
    let app = app(AppState::default());
    let challenge_input = json!({
        "title": "Alpha Baseline Reproduction",
        "description": "Frozen public-data bootstrap challenge.",
        "ruleset_version": "paper-raid-alpha-v1",
        "ruleset_hash": HASH_A,
        "dataset_manifest_hash": HASH_B,
        "evaluator_manifest_hash": HASH_C,
        "status": "open"
    });

    let (status, unauthorized) = request_json_without_service_tokens(
        app.clone(),
        "GET",
        "/v1/hepta/operator/challenges",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(unauthorized["code"], "operator_auth_failed");

    let (status, empty) = request_json(
        app.clone(),
        "GET",
        "/v1/hepta/operator/challenges",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty, json!([]));

    let (status, created) =
        request_json(app.clone(), "POST", "/v1/hepta/challenges", challenge_input).await;
    assert_eq!(status, StatusCode::CREATED);
    let challenge_id = created["challenge_id"].as_str().expect("challenge id");

    let (status, listed) = request_json(
        app.clone(),
        "GET",
        "/v1/hepta/operator/challenges",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().expect("challenge list").len(), 1);
    assert_eq!(listed[0], created);

    let operator_exact_path = format!("/v1/hepta/operator/challenges/{challenge_id}");
    let (status, exact) = request_json(app.clone(), "GET", &operator_exact_path, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(exact, created);

    let (status, unauthorized) =
        request_json_without_service_tokens(app.clone(), "GET", &operator_exact_path, json!({}))
            .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(unauthorized["code"], "operator_auth_failed");

    let legacy_exact_path = format!("/v1/hepta/challenges/{challenge_id}");
    let (status, legacy_exact) =
        request_json_without_service_tokens(app, "GET", &legacy_exact_path, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(legacy_exact, created);
}

#[tokio::test]
async fn external_agent_completes_registration_enrollment_authorization_and_signed_submission() {
    let app = app(AppState::default());
    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let public_key = BASE64.encode(signing_key.verifying_key().to_bytes());

    let (status, manifest) =
        request_json(app.clone(), "GET", "/v1/hepta/manifest", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(manifest["agent_execution_mode"], "external_only");

    let (status, agent) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/agents",
        json!({
            "agent_id": "did:trnm:agent-alpha",
            "owner_id": "researcher-alpha",
            "organization_id": "lab-seven",
            "protocol_version": "hepta_agent_protocol_v1",
            "public_key": public_key,
            "capabilities": ["scientific_reasoning", "code_execution"]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(agent["agent_id"], "did:trnm:agent-alpha");

    let (status, challenge) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/challenges",
        json!({
            "title": "Reproducible Catalyst Search",
            "description": "Find and document a reproducible catalyst candidate.",
            "ruleset_version": "catalyst-v1",
            "ruleset_hash": HASH_A,
            "dataset_manifest_hash": HASH_B,
            "evaluator_manifest_hash": HASH_C,
            "status": "open"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let challenge_id = challenge["challenge_id"]
        .as_str()
        .expect("challenge id string");

    let (status, enrollment) = request_json(
        app.clone(),
        "POST",
        &format!("/v1/hepta/challenges/{challenge_id}/enrollments"),
        json!({"agent_id": "did:trnm:agent-alpha"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(enrollment["challenge_id"], challenge_id);

    let requested_match_id = Uuid::new_v4();
    let authorization_request = json!({
        "match_id": requested_match_id,
        "challenge_id": challenge_id,
        "agent_id": "did:trnm:agent-alpha",
        "subject_user_id": "nakama-user-alpha",
        "participant_slot": 1,
        "role": "challenger",
        "ttl_seconds": 300
    });
    let (status, authorization) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/match-authorizations",
        authorization_request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        authorization["claim"]["schema"],
        "trnm.match.authorization.v1"
    );
    assert_eq!(
        authorization["claim"]["subject_user_id"],
        "nakama-user-alpha"
    );
    assert_eq!(authorization["claim"]["agent_did"], "did:trnm:agent-alpha");
    assert_eq!(
        authorization["claim"]["agent_public_key"],
        BASE64.encode(signing_key.verifying_key().to_bytes())
    );
    assert_eq!(authorization["claim"]["participant_slot"], 1);
    assert_eq!(authorization["claim"]["role"], "challenger");
    assert_eq!(authorization["claim"]["ruleset_hash"], HASH_A);
    assert_eq!(authorization["claim"]["dataset_hash"], HASH_B);
    assert!(authorization["claim"]["agent_key_id"]
        .as_str()
        .is_some_and(|value| value.starts_with("sha256:")));
    assert!(authorization["claim"]["challenge_snapshot_hash"]
        .as_str()
        .is_some_and(|value| value.starts_with("sha256:")));
    assert_eq!(
        authorization["claim"]["expires_at_unix"]
            .as_i64()
            .expect("expiry"),
        authorization["claim"]["issued_at_unix"]
            .as_i64()
            .expect("issuance")
            + 300
    );
    assert_eq!(authorization["issuer_key_id"], "hepta-test-issuer-key-v1");
    assert_eq!(
        BASE64
            .decode(authorization["signature"].as_str().expect("signature"))
            .expect("base64 signature")
            .len(),
        64
    );
    assert!(authorization.get("match_token").is_none());
    let (status, retried_authorization) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/match-authorizations",
        authorization_request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(retried_authorization, authorization);
    let mut conflicting_authorization_request = authorization_request.clone();
    conflicting_authorization_request["role"] = json!("defender");
    let (status, conflict) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/match-authorizations",
        conflicting_authorization_request,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["code"], "match_authorization_conflict");

    let consumption_request = json!({
        "authorization_id": authorization["claim"]["authorization_id"],
        "match_id": authorization["claim"]["match_id"],
        "agent_id": "did:trnm:agent-alpha"
    });
    let (status, claims) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/nakama/match-authorizations/consumed",
        consumption_request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(claims["participant_slot"], 1);
    let (status, repeated_claims) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/nakama/match-authorizations/consumed",
        consumption_request,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated_claims, claims);

    let submission_id = Uuid::new_v4();
    let challenge_id = Uuid::parse_str(challenge_id).expect("valid challenge id");
    let match_id = Uuid::parse_str(
        authorization["claim"]["match_id"]
            .as_str()
            .expect("match id string"),
    )
    .expect("valid match id");
    let unsigned = SubmitArtifactRequest {
        protocol_version: "hepta_agent_protocol_v1".to_string(),
        submission_id,
        challenge_id,
        match_id,
        agent_id: "did:trnm:agent-alpha".to_string(),
        artifact_hash: HASH_A.to_string(),
        evidence_manifest_hash: HASH_B.to_string(),
        nonce: "run-0001".to_string(),
        signature: String::new(),
    };
    let signature = BASE64.encode(
        signing_key
            .sign(submission_signing_message(&unsigned).as_bytes())
            .to_bytes(),
    );
    let submission_request = json!({
        "protocol_version": unsigned.protocol_version,
        "submission_id": unsigned.submission_id,
        "challenge_id": unsigned.challenge_id,
        "match_id": unsigned.match_id,
        "agent_id": unsigned.agent_id,
        "artifact_hash": unsigned.artifact_hash,
        "evidence_manifest_hash": unsigned.evidence_manifest_hash,
        "nonce": unsigned.nonce,
        "signature": signature
    });
    let (status, submission) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/submissions",
        submission_request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(submission["submission_id"], submission_id.to_string());

    let (status, repeated_submission) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/submissions",
        submission_request,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        repeated_submission["submission_id"],
        submission_id.to_string()
    );

    let (status, events) = request_json(app, "GET", "/v1/hepta/events", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let events = events.as_array().expect("events array");
    assert_eq!(events.len(), 6);
    assert_eq!(
        events[4]["event_type"],
        "hepta.match.authorization_consumed.v1"
    );
    assert_eq!(events[5]["event_type"], "hepta.submission.accepted.v1");
    assert!(events.iter().all(|event| event["payload_hash"]
        .as_str()
        .is_some_and(|hash| hash.starts_with("sha256:"))));
}

#[tokio::test]
async fn enforces_service_auth_and_agent_key_rotation_nonce_replay_protection() {
    let app = app(AppState::default());
    let old_key = SigningKey::from_bytes(&[11_u8; 32]);
    let new_key = SigningKey::from_bytes(&[12_u8; 32]);

    let (status, error) = request_json_with_tokens(
        app.clone(),
        "POST",
        "/v1/hepta/challenges",
        json!({
            "title": "Unauthorized Challenge",
            "description": "Must not be created.",
            "ruleset_version": "auth-v1",
            "ruleset_hash": HASH_A,
            "dataset_manifest_hash": HASH_B,
            "evaluator_manifest_hash": HASH_C,
            "status": "open"
        }),
        "wrong-operator-token",
        "hepta-test-nakama-token",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["code"], "operator_auth_failed");

    let (status, error) = request_json_with_tokens(
        app.clone(),
        "POST",
        "/v1/hepta/nakama/match-authorizations/consumed",
        json!({
            "authorization_id": Uuid::new_v4(),
            "match_id": Uuid::new_v4(),
            "agent_id": "did:trnm:unknown"
        }),
        "hepta-test-operator-token",
        "wrong-nakama-token",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["code"], "nakama_auth_failed");

    let (status, _) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/agents",
        json!({
            "agent_id": "did:trnm:rotating-agent",
            "owner_id": "researcher-rotation",
            "protocol_version": "hepta_agent_protocol_v1",
            "public_key": BASE64.encode(old_key.verifying_key().to_bytes())
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let unsigned_rotation = RotateAgentKeyRequest {
        agent_id: "did:trnm:rotating-agent".to_string(),
        new_public_key: BASE64.encode(new_key.verifying_key().to_bytes()),
        nonce: "rotation-0001".to_string(),
        signature: String::new(),
    };
    let rotation_signature = BASE64.encode(
        old_key
            .sign(key_rotation_signing_message(&unsigned_rotation).as_bytes())
            .to_bytes(),
    );
    let rotation_request = json!({
        "agent_id": unsigned_rotation.agent_id,
        "new_public_key": unsigned_rotation.new_public_key,
        "nonce": unsigned_rotation.nonce,
        "signature": rotation_signature
    });
    let (status, rotation) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/agents/rotate-key",
        rotation_request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rotation["agent_id"], "did:trnm:rotating-agent");

    let (status, error) =
        request_json(app, "POST", "/v1/hepta/agents/rotate-key", rotation_request).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "agent_nonce_reused");
}

#[tokio::test]
async fn rejects_submission_signed_by_a_different_agent_key() {
    let app = app(AppState::default());
    let registered_key = SigningKey::from_bytes(&[1_u8; 32]);
    let attacker_key = SigningKey::from_bytes(&[2_u8; 32]);

    let (status, _) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/agents",
        json!({
            "agent_id": "did:trnm:agent-beta",
            "owner_id": "researcher-beta",
            "protocol_version": "hepta_agent_protocol_v1",
            "public_key": BASE64.encode(registered_key.verifying_key().to_bytes())
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, challenge) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/challenges",
        json!({
            "title": "Signed Submission Gate",
            "description": "Reject forged Agent output.",
            "ruleset_version": "signature-v1",
            "ruleset_hash": HASH_A,
            "dataset_manifest_hash": HASH_B,
            "evaluator_manifest_hash": HASH_C,
            "status": "open"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let challenge_id = challenge["challenge_id"].as_str().expect("challenge id");

    let (status, _) = request_json(
        app.clone(),
        "POST",
        &format!("/v1/hepta/challenges/{challenge_id}/enrollments"),
        json!({"agent_id": "did:trnm:agent-beta"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, authorization) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/match-authorizations",
        json!({
            "match_id": null,
            "challenge_id": challenge_id,
            "agent_id": "did:trnm:agent-beta",
            "subject_user_id": "nakama-user-beta",
            "participant_slot": 2,
            "role": "defender"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = request_json(
        app.clone(),
        "POST",
        "/v1/hepta/nakama/match-authorizations/consumed",
        json!({
            "authorization_id": authorization["claim"]["authorization_id"],
            "match_id": authorization["claim"]["match_id"],
            "agent_id": "did:trnm:agent-beta"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let unsigned = SubmitArtifactRequest {
        protocol_version: "hepta_agent_protocol_v1".to_string(),
        submission_id: Uuid::new_v4(),
        challenge_id: Uuid::parse_str(challenge_id).expect("valid challenge id"),
        match_id: Uuid::parse_str(
            authorization["claim"]["match_id"]
                .as_str()
                .expect("match id"),
        )
        .expect("valid match id"),
        agent_id: "did:trnm:agent-beta".to_string(),
        artifact_hash: HASH_A.to_string(),
        evidence_manifest_hash: HASH_B.to_string(),
        nonce: "forged-run".to_string(),
        signature: String::new(),
    };
    let signature = BASE64.encode(
        attacker_key
            .sign(submission_signing_message(&unsigned).as_bytes())
            .to_bytes(),
    );
    let (status, error) = request_json(
        app,
        "POST",
        "/v1/hepta/submissions",
        json!({
            "protocol_version": unsigned.protocol_version,
            "submission_id": unsigned.submission_id,
            "challenge_id": unsigned.challenge_id,
            "match_id": unsigned.match_id,
            "agent_id": unsigned.agent_id,
            "artifact_hash": unsigned.artifact_hash,
            "evidence_manifest_hash": unsigned.evidence_manifest_hash,
            "nonce": unsigned.nonce,
            "signature": signature
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["code"], "signature_verification_failed");
}

#[tokio::test]
async fn rate_limits_repeated_agent_registration_by_owner() {
    let app = app(AppState::default());
    let signing_key = SigningKey::from_bytes(&[55_u8; 32]);
    let public_key = BASE64.encode(signing_key.verifying_key().to_bytes());
    for index in 0..20 {
        let (status, _) = request_json(
            app.clone(),
            "POST",
            "/v1/hepta/agents",
            json!({
                "agent_id": format!("did:trnm:rate-{index}"),
                "owner_id": "rate-limited-owner",
                "protocol_version": "hepta_agent_protocol_v1",
                "public_key": public_key
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }
    let (status, error) = request_json(
        app,
        "POST",
        "/v1/hepta/agents",
        json!({
            "agent_id": "did:trnm:rate-overflow",
            "owner_id": "rate-limited-owner",
            "protocol_version": "hepta_agent_protocol_v1",
            "public_key": public_key
        }),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(error["code"], "rate_limit_exceeded");
}
