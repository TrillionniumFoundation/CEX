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
use std::collections::BTreeMap;
use tower::ServiceExt;
use uuid::Uuid;

use super::*;
use crate::{
    app,
    paper_chain_finality_v1::{
        paper_trnm_submission_commitment_hash, PaperTrnmCommandBindingV1,
        PAPER_TRNM_COMMAND_BINDING_SCHEMA_V1,
    },
    paper_raid_contracts::{
        agent_binding_key_rotation_signing_bytes, agent_binding_proof_signing_bytes,
        agent_proposal_signing_bytes, canonical_json_bytes, canonical_json_sha256,
        human_decision_signing_bytes, human_evidence_verification_signing_bytes,
        human_key_registration_signing_bytes, human_key_revocation_signing_bytes,
        human_key_rotation_signing_bytes, paper_appeal_resolution_signing_bytes,
        paper_appeal_signing_bytes, paper_evaluation_signing_bytes,
        paper_reproduction_signing_bytes, paper_review_attestation_signing_bytes,
        section_merge_signing_bytes, section_review_signing_bytes, sha256_digest,
        sign_authorship_consent, sign_consumer_user_assertion,
        team_member_acceptance_signing_bytes, AgentBindingKeyRotationClaimV2,
        AgentBindingProofClaimV2, AgentProposalSigningV1, AuthorshipConsentSigningV2,
        ConsumerUserAssertionClaimV2, HumanDecisionSigningV1, HumanEvidenceVerificationSigningV1,
        HumanKeyRegistrationClaimV2, HumanKeyRevocationClaimV2, HumanKeyRotationClaimV2,
        PaperAppealResolutionSigningV1, PaperAppealSigningV1, PaperEvaluationSigningV1,
        PaperReproductionSigningV1, PaperReviewAttestationSigningV1, SectionMergeSigningV1,
        SectionReviewSigningV1, TeamMemberAcceptanceSigningV2, AGENT_BINDING_KEY_ROTATION_V2,
        AGENT_BINDING_PROOF_V2, AGENT_PROPOSAL_V1, AUTHORSHIP_CONSENT_V2,
        CONSUMER_USER_ASSERTION_V2, HUMAN_DECISION_V1, HUMAN_EVIDENCE_VERIFICATION_V1,
        HUMAN_KEY_REGISTRATION_V2, HUMAN_KEY_REVOCATION_V2, HUMAN_KEY_ROTATION_V2,
        JSON_SAFE_U64_MAX, PAPER_APPEAL_RESOLUTION_V1, PAPER_APPEAL_V1, PAPER_EVALUATION_V1,
        PAPER_REPRODUCTION_V1, PAPER_REVIEW_ATTESTATION_V1, SECTION_MERGE_V1, SECTION_REVIEW_V1,
        TEAM_MEMBER_ACCEPTANCE_V2,
    },
    AppState, SecurityConfig, NAKAMA_TOKEN_HEADER, OPERATOR_TOKEN_HEADER, TRNM_TOKEN_HEADER,
    USER_ASSERTION_HEADER,
};

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
struct VerifiedArtifactHashes {
    manifest_id: Uuid,
    artifact_manifest_hash: String,
    source_manifest_hash: String,
    bibliography_hash: String,
    claim_evidence_graph_hash: String,
}

fn integration_artifact_bundle(
    challenge_id: Uuid,
    bundle_suffix: &str,
) -> (Value, String, Vec<Value>) {
    let mut bundle: Value = serde_json::from_str(include_str!(
        "../../../docs/sdk-fixtures/integration-paper-raid-artifact-bundle-v1.json"
    ))
    .expect("vendored Integration artifact bundle fixture");
    bundle["challenge_id"] = json!(challenge_id.to_string());
    bundle["bundle_id"] = json!(format!("paper-raid-synthetic-ablation-001-{bundle_suffix}"));
    let mut canonical = canonical_json_bytes(&bundle).expect("canonical neutral artifact bundle");
    canonical.push(b'\n');
    let source_manifest_sha256 = sha256_digest(&canonical)
        .strip_prefix("sha256:")
        .expect("digest prefix")
        .to_string();
    let storage_locations = bundle["objects"]
        .as_array()
        .expect("artifact objects")
        .iter()
        .map(|object| {
            let sha256 = object["sha256"].as_str().expect("object digest");
            json!({
                "logical_path": object["logical_path"],
                "sha256": sha256,
                "uri": format!("cas://sha256/{sha256}"),
                "acl": "team",
            })
        })
        .collect();
    (bundle, source_manifest_sha256, storage_locations)
}

async fn create_verified_artifact_manifest(
    router: &Router,
    actor: &Actor,
    paper_id: Uuid,
    challenge_id: Uuid,
    expected_paper_version: u64,
    suffix: &str,
) -> VerifiedArtifactHashes {
    let (source_bundle, expected_source_manifest_sha256, storage_locations) =
        integration_artifact_bundle(challenge_id, suffix);
    let manifest_id = Uuid::new_v4();
    let key = format!("artifact-{paper_id}-{suffix}");
    let path = format!("/v2/hepta/papers/{paper_id}/artifact-manifests");
    let manifest = assert_status(
        user_post(
            router,
            actor,
            "create_artifact_manifest_v3",
            &path,
            &key,
            json!({
                "manifest_id": manifest_id,
                "expected_paper_version": expected_paper_version,
                "expected_source_manifest_sha256": expected_source_manifest_sha256,
                "source_bundle": source_bundle,
                "storage_locations": storage_locations,
                "idempotency_key": key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    let role_hash = |role: &str| {
        let matching = manifest["objects"]
            .as_array()
            .expect("manifest objects")
            .iter()
            .filter(|object| object["role"] == role)
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1, "fixture role must be unique: {role}");
        format!(
            "sha256:{}",
            matching[0]["sha256"].as_str().expect("role digest")
        )
    };
    VerifiedArtifactHashes {
        manifest_id,
        artifact_manifest_hash: manifest["manifest_hash"]
            .as_str()
            .expect("manifest root")
            .to_string(),
        source_manifest_hash: role_hash("paper_source"),
        bibliography_hash: role_hash("bibliography"),
        claim_evidence_graph_hash: role_hash("claim_evidence_graph"),
    }
}

struct DraftingPaperContext {
    router: Router,
    state: AppState,
    actors: Vec<Actor>,
    paper_id: Uuid,
    work_item_id: Uuid,
    revision_id: Uuid,
    paper_version: u64,
    artifact: VerifiedArtifactHashes,
}

async fn create_matchmaking_ticket_for(
    router: &Router,
    actor: &Actor,
    challenge_id: Uuid,
    requested_team_size: u32,
) -> (StatusCode, Value) {
    let ticket_id = Uuid::new_v4();
    let key = format!("p3-ticket-{ticket_id}");
    user_post(
        router,
        actor,
        "create_matchmaking_ticket_v3",
        "/v2/hepta/matchmaking/tickets",
        &key,
        json!({
            "ticket_id":ticket_id,
            "challenge_id":challenge_id,
            "requested_team_size":requested_team_size,
            "roles":[actor.role],
            "availability_hash":digest(&format!("availability-{}", actor.player_id)),
            "idempotency_key":key,
        }),
    )
    .await
}

async fn decide_team_proposal(
    router: &Router,
    actor: &Actor,
    proposal_id: Uuid,
    expected_version: u64,
    decision: &str,
    suffix: &str,
) -> (StatusCode, Value) {
    let path = format!("/v2/hepta/team-proposals/{proposal_id}/decisions");
    let decision_id = Uuid::new_v4();
    let key = format!("p3-team-decision-{decision_id}-{suffix}");
    user_post(
        router,
        actor,
        "create_team_proposal_decision_v3",
        &path,
        &key,
        json!({
            "decision_id":decision_id,
            "expected_proposal_version":expected_version,
            "decision":decision,
            "idempotency_key":key,
        }),
    )
    .await
}

async fn exercise_matchmaking(
    router: &Router,
    actors: &[Actor],
    challenge_id: Uuid,
) -> (Uuid, Uuid) {
    assert_eq!(actors.len(), 7);
    assert_eq!(
        error_code(
            create_matchmaking_ticket_for(router, &actors[0], challenge_id, 4).await,
            StatusCode::BAD_REQUEST,
        ),
        "alpha_team_size_requires_three"
    );
    let mut proposals = Vec::new();
    for actor in actors {
        let response = assert_status(
            create_matchmaking_ticket_for(router, actor, challenge_id, 3).await,
            StatusCode::CREATED,
        );
        if let Some(proposal) = response["team_proposal"].as_object() {
            proposals.push(
                Uuid::parse_str(proposal["proposal_id"].as_str().expect("team proposal id"))
                    .expect("proposal UUID"),
            );
        }
    }
    assert_eq!(
        proposals.len(),
        2,
        "seven tickets form two groups and one queue tail"
    );

    let first_a = decide_team_proposal(router, &actors[0], proposals[0], 1, "accept", "race-a");
    let first_b = decide_team_proposal(router, &actors[1], proposals[0], 1, "accept", "race-b");
    let (first_a, first_b) = tokio::join!(first_a, first_b);
    let loser = if first_a.0 == StatusCode::CREATED {
        assert_eq!(first_b.0, StatusCode::CONFLICT);
        assert_eq!(first_b.1["code"], "aggregate_version_conflict");
        &actors[1]
    } else {
        assert_eq!(first_b.0, StatusCode::CREATED);
        assert_eq!(first_a.0, StatusCode::CONFLICT);
        assert_eq!(first_a.1["code"], "aggregate_version_conflict");
        &actors[0]
    };
    assert_status(
        decide_team_proposal(router, loser, proposals[0], 2, "accept", "retry").await,
        StatusCode::CREATED,
    );
    let accepted = assert_status(
        decide_team_proposal(router, &actors[2], proposals[0], 3, "accept", "terminal").await,
        StatusCode::CREATED,
    );
    assert_eq!(accepted["proposal"]["status"], "accepted");

    let declined = assert_status(
        decide_team_proposal(router, &actors[3], proposals[1], 1, "decline", "decline").await,
        StatusCode::CREATED,
    );
    assert_eq!(declined["proposal"]["status"], "declined");
    assert!(declined["replacement_proposal"].is_object());
    (proposals[0], proposals[1])
}

#[allow(clippy::too_many_arguments)]
fn human_verification_signature(
    actor: &Actor,
    paper_id: Uuid,
    record_kind: &str,
    record_id: Uuid,
    source_identifier: &str,
    source_hash: &str,
    locator: &str,
    license: &str,
    signed_at_unix: i64,
) -> String {
    let signing = HumanEvidenceVerificationSigningV1 {
        schema: HUMAN_EVIDENCE_VERIFICATION_V1.to_string(),
        verification_id: record_id,
        paper_project_id: paper_id,
        record_kind: record_kind.to_string(),
        record_id,
        source_identifier: source_identifier.to_string(),
        source_hash: source_hash.to_string(),
        locator: locator.to_string(),
        license: license.to_string(),
        player_id: actor.player_id,
        signing_key_id: actor.human_key_id.clone(),
        signing_public_key_hash: actor.human_public_key_hash.clone(),
        signed_at_unix,
    };
    BASE64.encode(
        actor
            .human_key
            .sign(
                &human_evidence_verification_signing_bytes(&signing)
                    .expect("human verification frame"),
            )
            .to_bytes(),
    )
}

fn signed_agent_proposal_body(
    context: &DraftingPaperContext,
    proposal_id: Uuid,
    section_key: &str,
    parent_revision_id: Uuid,
    payload_hash: &str,
    idempotency_key: &str,
) -> Value {
    let actor = &context.actors[0];
    let signed_at_unix = Utc::now().timestamp();
    let agent_key_id = sha256_digest(&actor.agent_key.verifying_key().to_bytes());
    let signing = AgentProposalSigningV1 {
        schema: AGENT_PROPOSAL_V1.to_string(),
        proposal_id,
        paper_project_id: context.paper_id,
        work_item_id: context.work_item_id,
        section_key: section_key.to_string(),
        parent_revision_id,
        proposal_kind: "delivery".to_string(),
        payload_hash: payload_hash.to_string(),
        artifact_manifest_hash: context.artifact.artifact_manifest_hash.clone(),
        agent_id: actor.agent_id.clone(),
        binding_id: actor.binding_id,
        agent_key_id: agent_key_id.clone(),
        signed_at_unix,
    };
    json!({
        "proposal_id":proposal_id,
        "work_item_id":context.work_item_id,
        "section_key":section_key,
        "parent_revision_id":parent_revision_id,
        "proposal_kind":"delivery",
        "payload_hash":payload_hash,
        "artifact_manifest_id":context.artifact.manifest_id,
        "agent_id":actor.agent_id,
        "binding_id":actor.binding_id,
        "agent_key_id":agent_key_id,
        "signed_at_unix":signed_at_unix,
        "signature":BASE64.encode(actor.agent_key.sign(
            &agent_proposal_signing_bytes(&signing).expect("Agent proposal frame")
        ).to_bytes()),
        "idempotency_key":idempotency_key,
    })
}

#[allow(clippy::too_many_arguments)]
fn signed_human_decision_body(
    context: &DraftingPaperContext,
    actor: &Actor,
    decision_id: Uuid,
    proposal_id: Uuid,
    expected_proposal_version: u64,
    decision: &str,
    reason_hash: &str,
    idempotency_key: &str,
) -> Value {
    let signed_at_unix = Utc::now().timestamp();
    let signing = HumanDecisionSigningV1 {
        schema: HUMAN_DECISION_V1.to_string(),
        decision_id,
        paper_project_id: context.paper_id,
        proposal_id,
        player_id: actor.player_id,
        decision: decision.to_string(),
        reason_hash: reason_hash.to_string(),
        expected_proposal_version,
        signing_key_id: actor.human_key_id.clone(),
        signing_public_key_hash: actor.human_public_key_hash.clone(),
        signed_at_unix,
    };
    json!({
        "decision_id":decision_id,
        "proposal_id":proposal_id,
        "expected_proposal_version":expected_proposal_version,
        "decision":decision,
        "reason_hash":reason_hash,
        "signing_key_id":actor.human_key_id,
        "signing_public_key":actor.human_public_key,
        "signing_public_key_hash":actor.human_public_key_hash,
        "signed_at_unix":signed_at_unix,
        "signature":BASE64.encode(actor.human_key.sign(
            &human_decision_signing_bytes(&signing).expect("human decision frame")
        ).to_bytes()),
        "idempotency_key":idempotency_key,
    })
}

async fn create_locked_drafting_paper(
    state: AppState,
    actors: Vec<Actor>,
    challenge_id: Uuid,
) -> DraftingPaperContext {
    let router = app(state.clone());
    let team_id = Uuid::new_v4();
    let paper_id = Uuid::new_v4();
    let work_item_id = Uuid::new_v4();
    let revision_id = Uuid::new_v4();
    let compact_hash = digest("p3-collaboration-compact");
    let create_team_key = format!("p3-create-team-{team_id}");
    assert_status(
        user_post(
            &router,
            &actors[0],
            "create_research_team_v2",
            "/v2/hepta/teams",
            &create_team_key,
            json!({
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
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    let acceptance_path = format!("/v2/hepta/teams/{team_id}/member-acceptances");
    for (index, actor) in actors.iter().enumerate() {
        let key = format!("p3-accept-{team_id}-{}", actor.player_id);
        let body = acceptance_body(
            actor,
            Uuid::new_v4(),
            team_id,
            challenge_id,
            u32::try_from(index + 1).expect("slot"),
            &compact_hash,
            &key,
        );
        assert_status(
            user_post(
                &router,
                actor,
                "accept_research_team_membership_v2",
                &acceptance_path,
                &key,
                body,
            )
            .await,
            StatusCode::CREATED,
        );
    }
    let lock_path = format!("/v2/hepta/teams/{team_id}/lock");
    let lock_key = format!("p3-lock-{team_id}");
    assert_status(
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
    let paper_key = format!("p3-paper-{paper_id}");
    assert_status(
        user_post(
            &router,
            &actors[0],
            "create_paper_project_v2",
            "/v2/hepta/papers",
            &paper_key,
            json!({
                "paper_project_id":paper_id,
                "team_id":team_id,
                "title":"Paper Collaboration Kernel",
                "target_format":"workshop-short-paper-v1",
                "idempotency_key":paper_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    let transition_path = format!("/v2/hepta/papers/{paper_id}/transition");
    let mut paper_version = 1_u64;
    for phase in ["preregistering", "researching", "experimenting", "drafting"] {
        let key = format!("p3-phase-{paper_id}-{phase}");
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
    let artifact = create_verified_artifact_manifest(
        &router,
        &actors[0],
        paper_id,
        challenge_id,
        paper_version,
        "collaboration",
    )
    .await;
    let work_path = format!("/v2/hepta/papers/{paper_id}/work-items");
    let work_key = format!("p3-work-{work_item_id}");
    assert_status(
        user_post(
            &router,
            &actors[0],
            "create_paper_work_item_v2",
            &work_path,
            &work_key,
            json!({
                "work_item_id":work_item_id,
                "expected_paper_version":paper_version,
                "kind":"paper_section",
                "title":"Draft methods section",
                "assigned_player_id":actors[0].player_id,
                "assigned_binding_id":actors[0].binding_id,
                "idempotency_key":work_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    paper_version += 1;
    let revision_path = format!("/v2/hepta/papers/{paper_id}/revisions");
    let revision_key = format!("p3-revision-{revision_id}");
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
                "source_manifest_hash":artifact.source_manifest_hash,
                "artifact_manifest_hash":artifact.artifact_manifest_hash,
                "bibliography_hash":artifact.bibliography_hash,
                "claim_evidence_graph_hash":artifact.claim_evidence_graph_hash,
                "idempotency_key":revision_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    paper_version += 1;
    DraftingPaperContext {
        router,
        state,
        actors,
        paper_id,
        work_item_id,
        revision_id,
        paper_version,
        artifact,
    }
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

pub(super) fn security() -> SecurityConfig {
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
    signed_user_assertion_at(
        actor,
        operation,
        method,
        path,
        idempotency_key,
        body_hash,
        now - 1,
        now + 120,
    )
}

#[allow(clippy::too_many_arguments)]
fn signed_user_assertion_with_key(
    actor: &Actor,
    operation: &str,
    method: &str,
    path: &str,
    idempotency_key: &str,
    body_hash: String,
    issuer_key_id: &str,
    signing_key: &SigningKey,
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
        issuer_key_id,
        signing_key,
    )
    .expect("sign Consumer assertion with overlap key");
    BASE64.encode(canonical_json_bytes(&assertion).expect("canonical assertion"))
}

#[allow(clippy::too_many_arguments)]
fn signed_user_assertion_at(
    actor: &Actor,
    operation: &str,
    method: &str,
    path: &str,
    idempotency_key: &str,
    body_hash: String,
    issued_at_unix: i64,
    expires_at_unix: i64,
) -> String {
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
            issued_at_unix,
            expires_at_unix,
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

pub(crate) async fn reset_postgres(database_url: &str) {
    let pool = sqlx::PgPool::connect(database_url)
        .await
        .expect("maintenance pool");
    sqlx::raw_sql(
        "truncate table
           hepta_paper_appeal_resolutions,
           hepta_paper_appeals,
           hepta_paper_reproductions,
           hepta_paper_raid_scores,
           hepta_paper_scores,
           hepta_paper_evaluation_panel_attestations,
           hepta_paper_evaluations,
           hepta_paper_contribution_ledgers,
           hepta_paper_room_events,
           hepta_section_merges,
           hepta_section_reviews,
           hepta_section_heads,
           hepta_section_revisions,
           hepta_human_decisions,
           hepta_agent_proposals,
           hepta_section_leases,
           hepta_claim_records,
           hepta_figure_lineage,
           hepta_run_records,
           hepta_experiment_plans,
           hepta_collaboration_revision_refs,
           hepta_citation_records,
           hepta_evidence_cards,
           hepta_paper_revision_artifact_bindings,
           hepta_artifact_manifests,
           hepta_team_proposal_decisions,
           hepta_team_proposals,
           hepta_matchmaking_tickets,
           hepta_nakama_research_control_commands,
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
           hepta_agent_binding_key_rotations,
           hepta_agent_binding_nonces,
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
        create_player_only(router, actor).await;

        let idempotency_key = format!("create-binding-{}", actor.binding_id);
        let body = agent_binding_body(actor, &idempotency_key, &idempotency_key);
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

async fn create_player_only(router: &Router, actor: &Actor) {
    let idempotency_key = format!("create-player-{}", actor.player_id);
    let now = Utc::now().timestamp();
    let body = human_player_body_at(actor, &idempotency_key, now - 1, now + 300);
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
}

fn human_player_body_at(
    actor: &Actor,
    idempotency_key: &str,
    issued_at_unix: i64,
    expires_at_unix: i64,
) -> Value {
    let registration = HumanKeyRegistrationClaimV2 {
        schema: HUMAN_KEY_REGISTRATION_V2.to_string(),
        player_id: actor.player_id,
        subject_id: actor.subject_id.clone(),
        nakama_user_id: actor.nakama_user_id,
        signing_key_id: actor.human_key_id.clone(),
        signing_public_key: actor.human_public_key.clone(),
        signing_public_key_hash: actor.human_public_key_hash.clone(),
        nonce: idempotency_key.to_string(),
        issued_at_unix,
        expires_at_unix,
    };
    let proof = BASE64.encode(
        actor
            .human_key
            .sign(&human_key_registration_signing_bytes(&registration).expect("registration frame"))
            .to_bytes(),
    );
    json!({
        "player_id": actor.player_id,
        "display_name": format!("Paper Raid author {}", actor.player_id),
        "signing_key_id": actor.human_key_id,
        "signing_public_key": actor.human_public_key,
        "key_issued_at_unix": registration.issued_at_unix,
        "key_expires_at_unix": registration.expires_at_unix,
        "key_proof_signature": proof,
        "idempotency_key": idempotency_key,
    })
}

fn agent_binding_body(actor: &Actor, idempotency_key: &str, proof_nonce: &str) -> Value {
    let now = Utc::now().timestamp();
    agent_binding_body_at(actor, idempotency_key, proof_nonce, now - 1, now + 300)
}

fn agent_binding_body_at(
    actor: &Actor,
    idempotency_key: &str,
    proof_nonce: &str,
    issued_at_unix: i64,
    expires_at_unix: i64,
) -> Value {
    let agent_public_key = BASE64.encode(actor.agent_key.verifying_key().to_bytes());
    let agent_key_id = sha256_digest(&actor.agent_key.verifying_key().to_bytes());
    let proof = AgentBindingProofClaimV2 {
        schema: AGENT_BINDING_PROOF_V2.to_string(),
        binding_id: actor.binding_id,
        agent_id: actor.agent_id.clone(),
        agent_key_id: agent_key_id.clone(),
        agent_public_key: agent_public_key.clone(),
        agent_public_key_hash: agent_key_id.clone(),
        subject_id: actor.subject_id.clone(),
        player_id: actor.player_id,
        nonce: proof_nonce.to_string(),
        issued_at_unix,
        expires_at_unix,
    };
    let proof_signature = BASE64.encode(
        actor
            .agent_key
            .sign(&agent_binding_proof_signing_bytes(&proof).expect("Agent binding frame"))
            .to_bytes(),
    );
    json!({
        "binding_id": actor.binding_id,
        "player_id": actor.player_id,
        "agent_id": actor.agent_id,
        "agent_key_id": agent_key_id,
        "agent_public_key": agent_public_key,
        "agent_proof_nonce": proof_nonce,
        "agent_proof_issued_at_unix": proof.issued_at_unix,
        "agent_proof_expires_at_unix": proof.expires_at_unix,
        "agent_proof_signature": proof_signature,
        "idempotency_key": idempotency_key,
    })
}

fn agent_binding_rotation_body(
    actor: &Actor,
    binding_id: Uuid,
    expected_binding_version: u64,
    old_key: &SigningKey,
    new_key: &SigningKey,
    idempotency_key: &str,
) -> Value {
    agent_binding_rotation_body_with_id(
        actor,
        binding_id,
        Uuid::new_v4(),
        expected_binding_version,
        old_key,
        new_key,
        idempotency_key,
    )
}

fn agent_binding_rotation_body_with_id(
    actor: &Actor,
    binding_id: Uuid,
    rotation_id: Uuid,
    expected_binding_version: u64,
    old_key: &SigningKey,
    new_key: &SigningKey,
    idempotency_key: &str,
) -> Value {
    let old_agent_public_key = BASE64.encode(old_key.verifying_key().to_bytes());
    let old_agent_key_id = sha256_digest(&old_key.verifying_key().to_bytes());
    let new_agent_public_key = BASE64.encode(new_key.verifying_key().to_bytes());
    let new_agent_key_id = sha256_digest(&new_key.verifying_key().to_bytes());
    let issued_at_unix = Utc::now().timestamp();
    let claim = AgentBindingKeyRotationClaimV2 {
        schema: AGENT_BINDING_KEY_ROTATION_V2.to_string(),
        rotation_id,
        binding_id,
        expected_binding_version,
        player_id: actor.player_id,
        subject_id: actor.subject_id.clone(),
        agent_id: actor.agent_id.clone(),
        old_agent_key_id: old_agent_key_id.clone(),
        old_agent_public_key: old_agent_public_key.clone(),
        old_agent_public_key_hash: old_agent_key_id.clone(),
        new_agent_key_id: new_agent_key_id.clone(),
        new_agent_public_key: new_agent_public_key.clone(),
        new_agent_public_key_hash: new_agent_key_id.clone(),
        nonce: idempotency_key.to_string(),
        issued_at_unix,
        expires_at_unix: issued_at_unix + 300,
    };
    let frame = agent_binding_key_rotation_signing_bytes(&claim)
        .expect("valid Agent binding key rotation frame");
    json!({
        "rotation_id":claim.rotation_id,
        "expected_binding_version":expected_binding_version,
        "agent_id":actor.agent_id,
        "old_agent_key_id":old_agent_key_id,
        "old_agent_public_key":old_agent_public_key,
        "new_agent_key_id":new_agent_key_id,
        "new_agent_public_key":new_agent_public_key,
        "issued_at_unix":issued_at_unix,
        "expires_at_unix":issued_at_unix + 300,
        "old_key_signature":BASE64.encode(old_key.sign(&frame).to_bytes()),
        "new_key_signature":BASE64.encode(new_key.sign(&frame).to_bytes()),
        "idempotency_key":idempotency_key,
    })
}

async fn seed_applied_idempotent_response(
    state: &AppState,
    operation: &str,
    idempotency_key: &str,
    request_hash: &str,
    aggregate_id: Uuid,
    response: Value,
) {
    if let Some(pool) = &state.pool {
        sqlx::query(
            "insert into hepta_paper_raid_idempotency (
                operation, idempotency_key, request_hash, aggregate_id,
                response_status, response_json
             ) values ($1,$2,$3,$4,$5,$6::jsonb)",
        )
        .bind(operation)
        .bind(idempotency_key)
        .bind(request_hash)
        .bind(aggregate_id)
        .bind(i32::from(StatusCode::CREATED.as_u16()))
        .bind(response)
        .execute(pool)
        .await
        .expect("seed committed idempotent response");
    } else {
        state.paper_raid.write().await.idempotency.insert(
            memory_idempotency_key(operation, idempotency_key),
            MemoryIdempotencyRecord {
                request_hash: request_hash.to_string(),
                status: StatusCode::CREATED,
                response,
            },
        );
    }
}

async fn exercise_expired_onboarding_replay(state: AppState) {
    let router = app(state.clone());
    let actor = actors(1).remove(0);
    let now = Utc::now();
    let issued_at_unix = now.timestamp() - 180;
    let expires_at_unix = now.timestamp() - 60;

    let player_key = format!("lost-player-response-{}", actor.player_id);
    let player_body = human_player_body_at(&actor, &player_key, issued_at_unix, expires_at_unix);
    let player_response = serde_json::to_value(HumanPlayer {
        player_id: actor.player_id,
        subject_id: actor.subject_id.clone(),
        nakama_user_id: actor.nakama_user_id,
        display_name: format!("Paper Raid author {}", actor.player_id),
        signing_key_id: actor.human_key_id.clone(),
        signing_public_key: actor.human_public_key.clone(),
        signing_public_key_hash: actor.human_public_key_hash.clone(),
        status: HumanPlayerStatus::Active,
        version: 1,
        created_at: now,
        updated_at: now,
    })
    .expect("encode seeded human player");
    let player_request_hash = canonical_json_sha256(&player_body).expect("player request hash");
    seed_applied_idempotent_response(
        &state,
        "create_human_player_v2",
        &player_key,
        &player_request_hash,
        actor.player_id,
        player_response.clone(),
    )
    .await;
    let player_assertion = signed_user_assertion_at(
        &actor,
        "create_human_player_v2",
        "POST",
        "/v2/hepta/players",
        &player_key,
        player_request_hash,
        issued_at_unix,
        expires_at_unix,
    );
    let replayed_player = assert_status(
        request(
            &router,
            "POST",
            "/v2/hepta/players",
            player_body,
            Some(player_assertion),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(replayed_player, player_response);

    let new_player_key = format!("expired-new-player-{}", actor.player_id);
    let new_player_body =
        human_player_body_at(&actor, &new_player_key, issued_at_unix, expires_at_unix);
    let new_player_hash = canonical_json_sha256(&new_player_body).expect("new player request hash");
    let new_player_assertion = signed_user_assertion_at(
        &actor,
        "create_human_player_v2",
        "POST",
        "/v2/hepta/players",
        &new_player_key,
        new_player_hash,
        issued_at_unix,
        expires_at_unix,
    );
    assert_eq!(
        error_code(
            request(
                &router,
                "POST",
                "/v2/hepta/players",
                new_player_body,
                Some(new_player_assertion),
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "user_assertion_expired"
    );

    let binding_key = format!("lost-binding-response-{}", actor.binding_id);
    let binding_body = agent_binding_body_at(
        &actor,
        &binding_key,
        &binding_key,
        issued_at_unix,
        expires_at_unix,
    );
    let agent_public_key = BASE64.encode(actor.agent_key.verifying_key().to_bytes());
    let agent_key_id = sha256_digest(&actor.agent_key.verifying_key().to_bytes());
    let binding_response = serde_json::to_value(AgentBinding {
        binding_id: actor.binding_id,
        player_id: actor.player_id,
        agent_id: actor.agent_id.clone(),
        agent_key_id: agent_key_id.clone(),
        agent_public_key,
        agent_public_key_hash: agent_key_id,
        status: AgentBindingStatus::Active,
        version: 1,
        created_at: now,
        updated_at: now,
    })
    .expect("encode seeded Agent binding");
    let binding_request_hash =
        canonical_json_sha256(&binding_body).expect("Agent binding request hash");
    seed_applied_idempotent_response(
        &state,
        "create_agent_binding_v2",
        &binding_key,
        &binding_request_hash,
        actor.binding_id,
        binding_response.clone(),
    )
    .await;
    let binding_assertion = signed_user_assertion_at(
        &actor,
        "create_agent_binding_v2",
        "POST",
        "/v2/hepta/agent-bindings",
        &binding_key,
        binding_request_hash,
        issued_at_unix,
        expires_at_unix,
    );
    let replayed_binding = assert_status(
        request(
            &router,
            "POST",
            "/v2/hepta/agent-bindings",
            binding_body,
            Some(binding_assertion),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(replayed_binding, binding_response);
}

async fn exercise_real_onboarding_expiry_replay(state: AppState) {
    const PROOF_LIFETIME_SECONDS: i64 = 5;
    const EXPIRY_WAIT: std::time::Duration = std::time::Duration::from_secs(6);

    let router = app(state.clone());
    let actor = actors(1).remove(0);

    let player_key = format!("real-player-response-loss-{}", actor.player_id);
    let player_now = Utc::now().timestamp();
    let player_body = human_player_body_at(
        &actor,
        &player_key,
        player_now,
        player_now + PROOF_LIFETIME_SECONDS,
    );
    let player_hash = canonical_json_sha256(&player_body).expect("real player request hash");
    let player_assertion = signed_user_assertion_at(
        &actor,
        "create_human_player_v2",
        "POST",
        "/v2/hepta/players",
        &player_key,
        player_hash,
        player_now,
        player_now + PROOF_LIFETIME_SECONDS,
    );
    let created_player = assert_status(
        request(
            &router,
            "POST",
            "/v2/hepta/players",
            player_body.clone(),
            Some(player_assertion.clone()),
        )
        .await,
        StatusCode::CREATED,
    );
    tokio::time::sleep(EXPIRY_WAIT).await;
    let replayed_player = assert_status(
        request(
            &router,
            "POST",
            "/v2/hepta/players",
            player_body,
            Some(player_assertion),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(replayed_player, created_player);

    // A different, unapplied command with a currently valid Consumer
    // assertion still rejects the now-expired human proof.
    let expired_new_player_key = format!("real-expired-new-player-{}", actor.player_id);
    let expired_new_player_body = human_player_body_at(
        &actor,
        &expired_new_player_key,
        player_now,
        player_now + PROOF_LIFETIME_SECONDS,
    );
    let current_assertion = signed_user_assertion(
        &actor,
        "create_human_player_v2",
        "POST",
        "/v2/hepta/players",
        &expired_new_player_key,
        canonical_json_sha256(&expired_new_player_body).expect("expired new player request hash"),
    );
    assert_eq!(
        error_code(
            request(
                &router,
                "POST",
                "/v2/hepta/players",
                expired_new_player_body,
                Some(current_assertion),
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "invalid_human_key_registration"
    );

    let binding_key = format!("real-binding-response-loss-{}", actor.binding_id);
    let binding_now = Utc::now().timestamp();
    let binding_body = agent_binding_body_at(
        &actor,
        &binding_key,
        &binding_key,
        binding_now,
        binding_now + PROOF_LIFETIME_SECONDS,
    );
    let binding_hash = canonical_json_sha256(&binding_body).expect("real binding request hash");
    let binding_assertion = signed_user_assertion_at(
        &actor,
        "create_agent_binding_v2",
        "POST",
        "/v2/hepta/agent-bindings",
        &binding_key,
        binding_hash,
        binding_now,
        binding_now + PROOF_LIFETIME_SECONDS,
    );
    let created_binding = assert_status(
        request(
            &router,
            "POST",
            "/v2/hepta/agent-bindings",
            binding_body.clone(),
            Some(binding_assertion.clone()),
        )
        .await,
        StatusCode::CREATED,
    );
    tokio::time::sleep(EXPIRY_WAIT).await;
    let replayed_binding = assert_status(
        request(
            &router,
            "POST",
            "/v2/hepta/agent-bindings",
            binding_body,
            Some(binding_assertion),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(replayed_binding, created_binding);

    let mut new_binding_actor = actor.clone();
    new_binding_actor.binding_id = Uuid::new_v4();
    new_binding_actor.agent_id = format!("{}-expired-new", actor.agent_id);
    let expired_new_binding_key = format!("real-expired-new-binding-{}", actor.binding_id);
    let expired_new_binding_body = agent_binding_body_at(
        &new_binding_actor,
        &expired_new_binding_key,
        &expired_new_binding_key,
        binding_now,
        binding_now + PROOF_LIFETIME_SECONDS,
    );
    let current_binding_assertion = signed_user_assertion(
        &actor,
        "create_agent_binding_v2",
        "POST",
        "/v2/hepta/agent-bindings",
        &expired_new_binding_key,
        canonical_json_sha256(&expired_new_binding_body).expect("expired new binding request hash"),
    );
    assert_eq!(
        error_code(
            request(
                &router,
                "POST",
                "/v2/hepta/agent-bindings",
                expired_new_binding_body,
                Some(current_binding_assertion),
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "invalid_agent_binding_proof"
    );

    let me = assert_status(
        user_get(
            &router,
            &actor,
            "get_self_human_player_v2",
            "/v2/hepta/players/me",
            "real-response-loss-player-self-read",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(me, created_player);
    let bindings = assert_status(
        user_get(
            &router,
            &actor,
            "list_self_agent_bindings_v2",
            "/v2/hepta/agent-bindings",
            "real-response-loss-binding-self-read",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(bindings, json!([created_binding]));

    if let Some(pool) = &state.pool {
        let players: i64 =
            sqlx::query_scalar("select count(*) from hepta_human_players where player_id=$1")
                .bind(actor.player_id)
                .fetch_one(pool)
                .await
                .expect("count committed players");
        let bindings: i64 =
            sqlx::query_scalar("select count(*) from hepta_agent_bindings where binding_id=$1")
                .bind(actor.binding_id)
                .fetch_one(pool)
                .await
                .expect("count committed Agent bindings");
        let nonces: i64 = sqlx::query_scalar(
            "select count(*) from hepta_agent_binding_nonces where binding_id=$1",
        )
        .bind(actor.binding_id)
        .fetch_one(pool)
        .await
        .expect("count committed Agent proof nonces");
        assert_eq!((players, bindings, nonces), (1, 1, 1));
    } else {
        let memory = state.paper_raid.read().await;
        assert_eq!(memory.players.len(), 1);
        assert_eq!(memory.bindings.len(), 1);
        assert_eq!(memory.used_agent_binding_nonces.len(), 1);
    }
}

#[tokio::test]
async fn applied_onboarding_replays_after_credentials_expire_but_new_requests_do_not() {
    exercise_expired_onboarding_replay(AppState::new(security())).await;
}

#[tokio::test]
async fn real_onboarding_transactions_replay_after_proof_expiry_without_duplicates() {
    exercise_real_onboarding_expiry_replay(AppState::new(security())).await;
}

#[tokio::test]
async fn secure_onboarding_requires_consumer_and_agent_pop_and_scopes_reads() {
    let router = app(AppState::new(security()));
    let actors = actors(2);
    let owner = &actors[0];
    let other = &actors[1];
    create_player_only(&router, owner).await;
    create_player_only(&router, other).await;

    // A legacy v1 Agent may self-claim this subject, but that record is not an
    // authority for Paper Raid onboarding.
    assert_status(
        request(
            &router,
            "POST",
            "/v1/hepta/agents",
            json!({
                "agent_id": owner.agent_id,
                "owner_id": owner.subject_id,
                "organization_id": "untrusted-legacy-registry",
                "protocol_version": "hepta_agent_protocol_v1",
                "public_key": BASE64.encode(other.agent_key.verifying_key().to_bytes()),
                "capabilities": []
            }),
            None,
        )
        .await,
        StatusCode::CREATED,
    );

    let idempotency_key = format!("secure-binding-{}", owner.binding_id);
    let valid_body = agent_binding_body(owner, &idempotency_key, &idempotency_key);
    assert_eq!(
        error_code(
            request(
                &router,
                "POST",
                "/v2/hepta/agent-bindings",
                valid_body.clone(),
                None,
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "user_assertion_required"
    );

    let mut invalid_pop_body = valid_body.clone();
    invalid_pop_body["agent_proof_signature"] =
        json!(BASE64.encode(other.agent_key.sign(b"not-the-binding-frame").to_bytes()));
    assert_eq!(
        error_code(
            user_post(
                &router,
                owner,
                "create_agent_binding_v2",
                "/v2/hepta/agent-bindings",
                &idempotency_key,
                invalid_pop_body,
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "invalid_agent_binding_proof"
    );

    let impersonation_key = format!("impersonate-binding-{}", owner.binding_id);
    let impersonation_body = agent_binding_body(owner, &impersonation_key, &impersonation_key);
    assert_eq!(
        error_code(
            user_post(
                &router,
                other,
                "create_agent_binding_v2",
                "/v2/hepta/agent-bindings",
                &impersonation_key,
                impersonation_body,
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "user_assertion_player_mismatch"
    );

    let created = assert_status(
        user_post(
            &router,
            owner,
            "create_agent_binding_v2",
            "/v2/hepta/agent-bindings",
            &idempotency_key,
            valid_body.clone(),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(created["agent_id"], owner.agent_id);
    assert_eq!(
        created["agent_public_key"],
        BASE64.encode(owner.agent_key.verifying_key().to_bytes())
    );

    // The exact same Consumer assertion cannot authorize changed request
    // bytes, even when all fields remain syntactically valid.
    let assertion = signed_user_assertion(
        owner,
        "create_agent_binding_v2",
        "POST",
        "/v2/hepta/agent-bindings",
        &idempotency_key,
        canonical_json_sha256(&valid_body).expect("binding request hash"),
    );
    let mut changed_body = valid_body;
    changed_body["agent_id"] = json!(format!("{}-replayed", owner.agent_id));
    assert_eq!(
        error_code(
            request(
                &router,
                "POST",
                "/v2/hepta/agent-bindings",
                changed_body,
                Some(assertion),
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "user_assertion_scope_mismatch"
    );

    let me = assert_status(
        user_get(
            &router,
            owner,
            "get_self_human_player_v2",
            "/v2/hepta/players/me",
            "owner-self-read",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(me["player_id"], owner.player_id.to_string());
    let owner_bindings = assert_status(
        user_get(
            &router,
            owner,
            "list_self_agent_bindings_v2",
            "/v2/hepta/agent-bindings",
            "owner-bindings-read",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(owner_bindings.as_array().expect("owner bindings").len(), 1);
    let other_bindings = assert_status(
        user_get(
            &router,
            other,
            "list_self_agent_bindings_v2",
            "/v2/hepta/agent-bindings",
            "other-bindings-read",
        )
        .await,
        StatusCode::OK,
    );
    assert!(other_bindings
        .as_array()
        .expect("other bindings")
        .is_empty());
}

#[tokio::test]
async fn consumer_edge_overlap_key_accepts_new_signer_and_rejects_unknown_key() {
    let overlap_key = SigningKey::from_bytes(&[0x6d; 32]);
    let security = security()
        .with_consumer_edge_verifying_key(
            "hepta-test-consumer-edge-key-v3",
            overlap_key.verifying_key().to_bytes(),
        )
        .expect("add Consumer Edge overlap key");
    let router = app(AppState::new(security));
    let actor = &actors(1)[0];
    let path = "/v2/hepta/players/me";
    let accepted = signed_user_assertion_with_key(
        actor,
        "get_self_human_player_v2",
        "GET",
        path,
        "overlap-read",
        sha256_digest(&[]),
        "hepta-test-consumer-edge-key-v3",
        &overlap_key,
    );
    assert_eq!(
        request(&router, "GET", path, Value::Null, Some(accepted))
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    let unknown = signed_user_assertion_with_key(
        actor,
        "get_self_human_player_v2",
        "GET",
        path,
        "unknown-read",
        sha256_digest(&[]),
        "hepta-test-consumer-edge-key-v4",
        &SigningKey::from_bytes(&[0x6e; 32]),
    );
    assert_eq!(
        error_code(
            request(&router, "GET", path, Value::Null, Some(unknown)).await,
            StatusCode::FORBIDDEN,
        ),
        "user_assertion_unknown_issuer_key"
    );
}

async fn exercise_agent_binding_rotation_security(state: AppState) {
    let router = app(state.clone());
    let actors = actors(2);
    create_players_and_bindings(&router, &actors).await;
    let actor = &actors[0];
    let other = &actors[1];
    let new_key = SigningKey::from_bytes(&[0x71; 32]);
    let third_key = SigningKey::from_bytes(&[0x72; 32]);
    let path = format!("/v2/hepta/agent-bindings/{}/rotate-key", actor.binding_id);

    let valid_key = format!("secure-agent-rotation-{}", actor.binding_id);
    let valid_body = agent_binding_rotation_body(
        actor,
        actor.binding_id,
        1,
        &actor.agent_key,
        &new_key,
        &valid_key,
    );
    let valid_rotation_id = Uuid::parse_str(
        valid_body["rotation_id"]
            .as_str()
            .expect("rotation ID string"),
    )
    .expect("rotation ID UUID");
    let mut substitution = valid_body.clone();
    let old_signature = substitution["old_key_signature"].clone();
    substitution["old_key_signature"] = substitution["new_key_signature"].clone();
    substitution["new_key_signature"] = old_signature;
    assert_eq!(
        error_code(
            user_post(
                &router,
                actor,
                "rotate_agent_binding_key_v2",
                &path,
                &valid_key,
                substitution,
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "invalid_agent_binding_key_rotation"
    );

    let overflow_key = format!("overflow-agent-rotation-{}", actor.binding_id);
    let mut overflow_body = valid_body.clone();
    overflow_body["expected_binding_version"] = json!(JSON_SAFE_U64_MAX + 1);
    overflow_body["idempotency_key"] = json!(overflow_key.clone());
    assert_eq!(
        error_code(
            user_post(
                &router,
                actor,
                "rotate_agent_binding_key_v2",
                &path,
                &overflow_key,
                overflow_body,
            )
            .await,
            StatusCode::BAD_REQUEST,
        ),
        "invalid_expected_version"
    );

    let cross_key = format!("cross-agent-rotation-{}", other.binding_id);
    let cross_path = format!("/v2/hepta/agent-bindings/{}/rotate-key", other.binding_id);
    let cross_body = agent_binding_rotation_body(
        actor,
        other.binding_id,
        1,
        &actor.agent_key,
        &new_key,
        &cross_key,
    );
    assert_eq!(
        error_code(
            user_post(
                &router,
                actor,
                "rotate_agent_binding_key_v2",
                &cross_path,
                &cross_key,
                cross_body,
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "agent_binding_rotation_scope_mismatch"
    );

    let rotated = assert_status(
        user_post(
            &router,
            actor,
            "rotate_agent_binding_key_v2",
            &path,
            &valid_key,
            valid_body.clone(),
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(rotated["version"], 2);
    assert_eq!(
        rotated["agent_key_id"],
        sha256_digest(&new_key.verifying_key().to_bytes())
    );
    let replay = assert_status(
        user_post(
            &router,
            actor,
            "rotate_agent_binding_key_v2",
            &path,
            &valid_key,
            valid_body.clone(),
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(replay, rotated);

    let reused_rotation_key = format!("reused-global-rotation-{}", other.binding_id);
    let other_new_key = SigningKey::from_bytes(&[0x73; 32]);
    let reused_rotation_body = agent_binding_rotation_body_with_id(
        other,
        other.binding_id,
        valid_rotation_id,
        1,
        &other.agent_key,
        &other_new_key,
        &reused_rotation_key,
    );
    assert_eq!(
        error_code(
            user_post(
                &router,
                other,
                "rotate_agent_binding_key_v2",
                &cross_path,
                &reused_rotation_key,
                reused_rotation_body,
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "agent_binding_rotation_reused"
    );

    let stale_key = format!("stale-agent-rotation-{}", actor.binding_id);
    let stale_body =
        agent_binding_rotation_body(actor, actor.binding_id, 1, &new_key, &third_key, &stale_key);
    assert_eq!(
        error_code(
            user_post(
                &router,
                actor,
                "rotate_agent_binding_key_v2",
                &path,
                &stale_key,
                stale_body,
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "aggregate_version_conflict"
    );

    let nonce_reuse_body =
        agent_binding_rotation_body(actor, actor.binding_id, 2, &new_key, &third_key, &valid_key);
    assert_eq!(
        error_code(
            user_post(
                &router,
                actor,
                "rotate_agent_binding_key_v2",
                &path,
                &valid_key,
                nonce_reuse_body,
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "idempotency_key_conflict"
    );

    let no_op_key = format!("no-op-agent-rotation-{}", actor.binding_id);
    let current_public_key = BASE64.encode(new_key.verifying_key().to_bytes());
    let current_key_id = sha256_digest(&new_key.verifying_key().to_bytes());
    let no_op_time = Utc::now().timestamp();
    let no_op_body = json!({
        "rotation_id":Uuid::new_v4(),
        "expected_binding_version":2,
        "agent_id":actor.agent_id,
        "old_agent_key_id":current_key_id,
        "old_agent_public_key":current_public_key,
        "new_agent_key_id":current_key_id,
        "new_agent_public_key":current_public_key,
        "issued_at_unix":no_op_time,
        "expires_at_unix":no_op_time + 300,
        "old_key_signature":BASE64.encode([0_u8;64]),
        "new_key_signature":BASE64.encode([0_u8;64]),
        "idempotency_key":no_op_key,
    });
    assert_eq!(
        error_code(
            user_post(
                &router,
                actor,
                "rotate_agent_binding_key_v2",
                &path,
                &no_op_key,
                no_op_body,
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "invalid_agent_binding_key_rotation"
    );

    if let Some(pool) = &state.pool {
        let version: i64 =
            sqlx::query_scalar("select version from hepta_agent_bindings where binding_id=$1")
                .bind(actor.binding_id)
                .fetch_one(pool)
                .await
                .expect("Agent binding version");
        let rotations: i64 = sqlx::query_scalar(
            "select count(*) from hepta_agent_binding_key_rotations where binding_id=$1",
        )
        .bind(actor.binding_id)
        .fetch_one(pool)
        .await
        .expect("Agent rotation history count");
        let events: i64 = sqlx::query_scalar(
            "select count(*) from hepta_outbox where event_type='hepta.paper_raid.agent_binding.key_rotated.v2' and aggregate_id=$1",
        )
        .bind(actor.binding_id.to_string())
        .fetch_one(pool)
        .await
        .expect("Agent rotation outbox count");
        assert_eq!((version, rotations, events), (2, 1, 1));
    } else {
        let memory = state.paper_raid.read().await;
        assert_eq!(memory.bindings[&actor.binding_id].version, 2);
        assert_eq!(memory.used_agent_binding_rotation_nonces.len(), 1);
        assert_eq!(memory.used_agent_binding_rotation_ids.len(), 1);
        assert_eq!(
            memory
                .events
                .iter()
                .filter(|event| event.event_type == "hepta.paper_raid.agent_binding.key_rotated.v2")
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn agent_binding_rotation_is_dual_pop_scoped_versioned_and_idempotent() {
    exercise_agent_binding_rotation_security(AppState::new(security())).await;
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

async fn collaboration_snapshot(context: &DraftingPaperContext, nonce: &str) -> Value {
    let room_path = format!("/v2/hepta/papers/{}/room", context.paper_id);
    let room = assert_status(
        user_get(
            &context.router,
            &context.actors[0],
            "get_paper_room_v3",
            &room_path,
            &format!("room-{nonce}"),
        )
        .await,
        StatusCode::OK,
    );
    let events_path = format!(
        "/v2/hepta/papers/{}/events?after_cursor=0",
        context.paper_id
    );
    let events = assert_status(
        user_get(
            &context.router,
            &context.actors[0],
            "list_paper_room_events_v3",
            &events_path,
            &format!("events-{nonce}"),
        )
        .await,
        StatusCode::OK,
    );
    let outbox = if let Some(pool) = &context.state.pool {
        json!(sqlx::query(
            "select event_id,event_type,aggregate_id,aggregate_version,payload_hash,payload
             from hepta_outbox order by occurred_at,event_id",
        )
        .fetch_all(pool)
        .await
        .expect("outbox snapshot")
        .into_iter()
        .map(|row| {
            json!({
                "event_id":row.get::<Uuid,_>("event_id"),
                "event_type":row.get::<String,_>("event_type"),
                "aggregate_id":row.get::<String,_>("aggregate_id"),
                "aggregate_version":row.get::<i64,_>("aggregate_version"),
                "payload_hash":row.get::<String,_>("payload_hash"),
                "payload":row.get::<Value,_>("payload"),
            })
        })
        .collect::<Vec<_>>())
    } else {
        let memory = context.state.paper_raid.read().await;
        serde_json::to_value(&memory.events).expect("memory outbox snapshot")
    };
    json!({"room":room,"events":events,"outbox":outbox})
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CollaborationOutcome {
    revision_unknown_code: String,
    revision_cross_paper_code: String,
    revision_descriptor_code: String,
    uri_tamper_code: String,
    doi_tamper_code: String,
    duplicate_proposal_code: String,
    stale_fencing_code: String,
    stale_parent_code: String,
    cyclic_parent_code: String,
    cross_section_parent_code: String,
    cross_paper_parent_code: String,
    frozen_phase_code: String,
    room_event_count: usize,
    room_last_cursor: u64,
    revision_binding_count: usize,
    section_merge_count: usize,
}

async fn run_collaboration_kernel_flow(state: AppState) -> CollaborationOutcome {
    let router = app(state.clone());
    let all_actors = actors(7);
    let challenge_id = register_prerequisites(&router, &all_actors).await;
    create_players_and_bindings(&router, &all_actors).await;
    exercise_matchmaking(&router, &all_actors, challenge_id).await;

    let mut context =
        create_locked_drafting_paper(state.clone(), all_actors[0..3].to_vec(), challenge_id).await;
    let cross_context =
        create_locked_drafting_paper(state.clone(), all_actors[3..6].to_vec(), challenge_id).await;
    let cross_artifact = create_verified_artifact_manifest(
        &cross_context.router,
        &cross_context.actors[0],
        cross_context.paper_id,
        challenge_id,
        cross_context.paper_version,
        "cross-paper-only",
    )
    .await;

    let revision_path = format!("/v2/hepta/papers/{}/revisions", context.paper_id);
    let revision_negative = |suffix: &str,
                             artifact_manifest_hash: String,
                             source_manifest_hash: String,
                             bibliography_hash: String,
                             claim_evidence_graph_hash: String| {
        let key = format!("p3-revision-negative-{suffix}-{}", context.paper_id);
        (
            key.clone(),
            json!({
                "revision_id":Uuid::new_v4(),
                "expected_paper_version":context.paper_version,
                "parent_revision_id":context.revision_id,
                "source_manifest_hash":source_manifest_hash,
                "artifact_manifest_hash":artifact_manifest_hash,
                "bibliography_hash":bibliography_hash,
                "claim_evidence_graph_hash":claim_evidence_graph_hash,
                "idempotency_key":key,
            }),
        )
    };
    let (unknown_key, unknown_body) = revision_negative(
        "unknown",
        digest("unknown-artifact-root"),
        context.artifact.source_manifest_hash.clone(),
        context.artifact.bibliography_hash.clone(),
        context.artifact.claim_evidence_graph_hash.clone(),
    );
    let revision_unknown_code = error_code(
        user_post(
            &context.router,
            &context.actors[0],
            "create_paper_revision_v2",
            &revision_path,
            &unknown_key,
            unknown_body,
        )
        .await,
        StatusCode::CONFLICT,
    );
    let (cross_key, cross_body) = revision_negative(
        "cross",
        cross_artifact.artifact_manifest_hash.clone(),
        cross_artifact.source_manifest_hash.clone(),
        cross_artifact.bibliography_hash.clone(),
        cross_artifact.claim_evidence_graph_hash.clone(),
    );
    let revision_cross_paper_code = error_code(
        user_post(
            &context.router,
            &context.actors[0],
            "create_paper_revision_v2",
            &revision_path,
            &cross_key,
            cross_body,
        )
        .await,
        StatusCode::CONFLICT,
    );
    let (tampered_key, tampered_body) = revision_negative(
        "descriptor",
        context.artifact.artifact_manifest_hash.clone(),
        digest("tampered-paper-source"),
        context.artifact.bibliography_hash.clone(),
        context.artifact.claim_evidence_graph_hash.clone(),
    );
    let revision_descriptor_code = error_code(
        user_post(
            &context.router,
            &context.actors[0],
            "create_paper_revision_v2",
            &revision_path,
            &tampered_key,
            tampered_body,
        )
        .await,
        StatusCode::CONFLICT,
    );

    let actor = &context.actors[0];
    let evidence_id = Uuid::new_v4();
    let source_uri = "https://example.org/public/paper-source";
    let source_hash = digest("public-paper-source");
    let locator = "page 4, table 2";
    let license = "CC-BY-4.0";
    let evidence_time = Utc::now().timestamp();
    let evidence_signature = human_verification_signature(
        actor,
        context.paper_id,
        "evidence_card",
        evidence_id,
        &format!("evidence-uri\n{source_uri}"),
        &source_hash,
        locator,
        license,
        evidence_time,
    );
    let evidence_path = format!("/v2/hepta/papers/{}/evidence-cards", context.paper_id);
    let evidence_key = format!("p3-evidence-{evidence_id}");
    let evidence_body = json!({
        "evidence_card_id":evidence_id,
        "source_uri":source_uri,
        "source_hash":source_hash,
        "locator":locator,
        "license":license,
        "verification_key_id":actor.human_key_id,
        "verification_public_key":actor.human_public_key,
        "verification_public_key_hash":actor.human_public_key_hash,
        "signed_at_unix":evidence_time,
        "verification_signature":evidence_signature,
        "idempotency_key":evidence_key,
    });
    assert_status(
        user_post(
            &context.router,
            actor,
            "create_evidence_card_v3",
            &evidence_path,
            &evidence_key,
            evidence_body.clone(),
        )
        .await,
        StatusCode::CREATED,
    );
    let mut uri_tamper = evidence_body;
    let uri_tamper_key = format!("p3-evidence-uri-tamper-{evidence_id}");
    uri_tamper["source_uri"] = json!("https://example.org/public/other-source");
    uri_tamper["idempotency_key"] = json!(uri_tamper_key.clone());
    let uri_tamper_code = error_code(
        user_post(
            &context.router,
            actor,
            "create_evidence_card_v3",
            &evidence_path,
            &uri_tamper_key,
            uri_tamper,
        )
        .await,
        StatusCode::FORBIDDEN,
    );

    let citation_id = Uuid::new_v4();
    let doi = "10.1234/hepta.paper-raid";
    let canonical_url = "https://doi.org/10.1234/hepta.paper-raid";
    let citation_time = Utc::now().timestamp();
    let citation_source = format!("citation-doi\n{doi}\ncitation-url\n{canonical_url}");
    let citation_signature = human_verification_signature(
        actor,
        context.paper_id,
        "citation",
        citation_id,
        &citation_source,
        &source_hash,
        locator,
        license,
        citation_time,
    );
    let citation_path = format!("/v2/hepta/papers/{}/citations", context.paper_id);
    let citation_key = format!("p3-citation-{citation_id}");
    let citation_body = json!({
        "citation_id":citation_id,
        "evidence_card_id":evidence_id,
        "doi":doi,
        "canonical_url":canonical_url,
        "source_hash":source_hash,
        "locator":locator,
        "license":license,
        "verification_key_id":actor.human_key_id,
        "verification_public_key":actor.human_public_key,
        "verification_public_key_hash":actor.human_public_key_hash,
        "signed_at_unix":citation_time,
        "verification_signature":citation_signature,
        "idempotency_key":citation_key,
    });
    assert_status(
        user_post(
            &context.router,
            actor,
            "create_citation_record_v3",
            &citation_path,
            &citation_key,
            citation_body.clone(),
        )
        .await,
        StatusCode::CREATED,
    );
    let mut doi_tamper = citation_body;
    let doi_tamper_key = format!("p3-citation-doi-tamper-{citation_id}");
    doi_tamper["doi"] = json!("10.1234/hepta.other");
    doi_tamper["idempotency_key"] = json!(doi_tamper_key.clone());
    let doi_tamper_code = error_code(
        user_post(
            &context.router,
            actor,
            "create_citation_record_v3",
            &citation_path,
            &doi_tamper_key,
            doi_tamper,
        )
        .await,
        StatusCode::FORBIDDEN,
    );

    let section_key = "methods";
    let lease_id = Uuid::new_v4();
    let lease_path = format!("/v2/hepta/papers/{}/section-leases", context.paper_id);
    let lease_key = format!("p3-lease-{lease_id}");
    let lease = assert_status(
        user_post(
            &context.router,
            actor,
            "acquire_section_lease_v3",
            &lease_path,
            &lease_key,
            json!({
                "lease_id":lease_id,
                "section_key":section_key,
                "holder_binding_id":actor.binding_id,
                "expected_previous_fencing_token":0,
                "ttl_seconds":3600,
                "idempotency_key":lease_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(lease["fencing_token"], 1);

    let proposal_id = Uuid::new_v4();
    let payload_hash = digest("methods-section-patch");
    let proposal_key = format!("p3-proposal-{proposal_id}");
    let proposal_path = format!("/v2/hepta/papers/{}/agent-proposals", context.paper_id);
    let proposal_body = signed_agent_proposal_body(
        &context,
        proposal_id,
        section_key,
        context.revision_id,
        &payload_hash,
        &proposal_key,
    );
    assert_status(
        request(&context.router, "POST", &proposal_path, proposal_body, None).await,
        StatusCode::CREATED,
    );

    let before_duplicate = collaboration_snapshot(&context, "before-duplicate").await;
    let duplicate_key = format!("p3-proposal-duplicate-{proposal_id}");
    let duplicate_body = signed_agent_proposal_body(
        &context,
        proposal_id,
        section_key,
        context.revision_id,
        &digest("different-valid-payload"),
        &duplicate_key,
    );
    let duplicate_proposal_code = error_code(
        request(
            &context.router,
            "POST",
            &proposal_path,
            duplicate_body,
            None,
        )
        .await,
        StatusCode::CONFLICT,
    );
    let after_duplicate = collaboration_snapshot(&context, "after-duplicate").await;
    assert_eq!(
        before_duplicate, after_duplicate,
        "failed memory/PG mutation must be atomic"
    );

    let decision_id = Uuid::new_v4();
    let decision_key = format!("p3-human-decision-{decision_id}");
    let decision_path = format!("/v2/hepta/papers/{}/human-decisions", context.paper_id);
    let decision_body = signed_human_decision_body(
        &context,
        &context.actors[1],
        decision_id,
        proposal_id,
        1,
        "accept",
        &digest("accept-methods-patch"),
        &decision_key,
    );
    assert_status(
        user_post(
            &context.router,
            &context.actors[1],
            "create_human_decision_v3",
            &decision_path,
            &decision_key,
            decision_body,
        )
        .await,
        StatusCode::CREATED,
    );

    let section_revision_id = Uuid::new_v4();
    let section_revision_key = format!("p3-section-revision-{section_revision_id}");
    let section_revision_path = format!("/v2/hepta/papers/{}/section-revisions", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            actor,
            "create_section_revision_v3",
            &section_revision_path,
            &section_revision_key,
            json!({
                "section_revision_id":section_revision_id,
                "section_key":section_key,
                "parent_revision_id":context.revision_id,
                "proposal_id":proposal_id,
                "lease_id":lease_id,
                "fencing_token":1,
                "patch_manifest_id":context.artifact.manifest_id,
                "patch_hash":payload_hash,
                "idempotency_key":section_revision_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );

    let reviewer = &context.actors[1];
    let review_id = Uuid::new_v4();
    let review_hash = digest("independent-methods-review");
    let review_time = Utc::now().timestamp();
    let review_signing = SectionReviewSigningV1 {
        schema: SECTION_REVIEW_V1.to_string(),
        review_id,
        paper_project_id: context.paper_id,
        section_revision_id,
        reviewer_player_id: reviewer.player_id,
        verdict: "approve".to_string(),
        review_hash: review_hash.clone(),
        expected_revision_version: 1,
        signing_key_id: reviewer.human_key_id.clone(),
        signing_public_key_hash: reviewer.human_public_key_hash.clone(),
        signed_at_unix: review_time,
    };
    let review_key = format!("p3-review-{review_id}");
    let review_path = format!(
        "/v2/hepta/papers/{}/section-revisions/{section_revision_id}/reviews",
        context.paper_id
    );
    assert_status(
        user_post(
            &context.router,
            reviewer,
            "create_section_review_v3",
            &review_path,
            &review_key,
            json!({
                "review_id":review_id,
                "expected_revision_version":1,
                "verdict":"approve",
                "review_hash":review_hash,
                "signing_key_id":reviewer.human_key_id,
                "signing_public_key":reviewer.human_public_key,
                "signing_public_key_hash":reviewer.human_public_key_hash,
                "signed_at_unix":review_time,
                "signature":BASE64.encode(reviewer.human_key.sign(
                    &section_review_signing_bytes(&review_signing).expect("section review frame")
                ).to_bytes()),
                "idempotency_key":review_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );

    let merge_id = Uuid::new_v4();
    let merge_time = Utc::now().timestamp();
    let merge_signing = SectionMergeSigningV1 {
        schema: SECTION_MERGE_V1.to_string(),
        merge_id,
        paper_project_id: context.paper_id,
        section_key: section_key.to_string(),
        section_revision_id,
        parent_revision_id: context.revision_id,
        merged_section_revision_id: section_revision_id,
        lease_id,
        fencing_token: 1,
        merged_by_player_id: actor.player_id,
        signing_key_id: actor.human_key_id.clone(),
        signing_public_key_hash: actor.human_public_key_hash.clone(),
        merged_at_unix: merge_time,
    };
    let merge_key = format!("p3-merge-{merge_id}");
    let merge_path = format!("/v2/hepta/papers/{}/section-merges", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            actor,
            "create_section_merge_v3",
            &merge_path,
            &merge_key,
            json!({
                "merge_id":merge_id,
                "section_revision_id":section_revision_id,
                "expected_revision_version":2,
                "parent_revision_id":context.revision_id,
                "merged_section_revision_id":section_revision_id,
                "lease_id":lease_id,
                "fencing_token":1,
                "signing_key_id":actor.human_key_id,
                "signing_public_key":actor.human_public_key,
                "signing_public_key_hash":actor.human_public_key_hash,
                "merged_at_unix":merge_time,
                "signature":BASE64.encode(actor.human_key.sign(
                    &section_merge_signing_bytes(&merge_signing).expect("section merge frame")
                ).to_bytes()),
                "idempotency_key":merge_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );

    let stale_fence_lease_id = Uuid::new_v4();
    let stale_fence_key = format!("p3-stale-fence-{stale_fence_lease_id}");
    let stale_fencing_code = error_code(
        user_post(
            &context.router,
            actor,
            "acquire_section_lease_v3",
            &lease_path,
            &stale_fence_key,
            json!({
                "lease_id":stale_fence_lease_id,
                "section_key":section_key,
                "holder_binding_id":actor.binding_id,
                "expected_previous_fencing_token":0,
                "ttl_seconds":3600,
                "idempotency_key":stale_fence_key,
            }),
        )
        .await,
        StatusCode::CONFLICT,
    );

    let next_lease_id = Uuid::new_v4();
    let next_lease_key = format!("p3-lease-next-{next_lease_id}");
    assert_status(
        user_post(
            &context.router,
            actor,
            "acquire_section_lease_v3",
            &lease_path,
            &next_lease_key,
            json!({
                "lease_id":next_lease_id,
                "section_key":section_key,
                "holder_binding_id":actor.binding_id,
                "expected_previous_fencing_token":1,
                "ttl_seconds":3600,
                "idempotency_key":next_lease_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    let stale_key = format!("p3-stale-parent-{}", Uuid::new_v4());
    let stale_parent_code = error_code(
        request(
            &context.router,
            "POST",
            &proposal_path,
            signed_agent_proposal_body(
                &context,
                Uuid::new_v4(),
                section_key,
                context.revision_id,
                &digest("stale-parent"),
                &stale_key,
            ),
            None,
        )
        .await,
        StatusCode::CONFLICT,
    );

    let results_lease_id = Uuid::new_v4();
    let results_lease_key = format!("p3-results-lease-{results_lease_id}");
    assert_status(
        user_post(
            &context.router,
            actor,
            "acquire_section_lease_v3",
            &lease_path,
            &results_lease_key,
            json!({
                "lease_id":results_lease_id,
                "section_key":"results",
                "holder_binding_id":actor.binding_id,
                "expected_previous_fencing_token":0,
                "ttl_seconds":3600,
                "idempotency_key":results_lease_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    let cyclic_parent_key = format!("p3-cyclic-parent-{}", Uuid::new_v4());
    let cyclic_parent_code = error_code(
        request(
            &context.router,
            "POST",
            &proposal_path,
            signed_agent_proposal_body(
                &context,
                context.revision_id,
                "results",
                context.revision_id,
                &digest("cyclic-section-parent"),
                &cyclic_parent_key,
            ),
            None,
        )
        .await,
        StatusCode::CONFLICT,
    );
    let cross_section_key = format!("p3-cross-section-{}", Uuid::new_v4());
    let cross_section_parent_code = error_code(
        request(
            &context.router,
            "POST",
            &proposal_path,
            signed_agent_proposal_body(
                &context,
                Uuid::new_v4(),
                "results",
                section_revision_id,
                &digest("cross-section-parent"),
                &cross_section_key,
            ),
            None,
        )
        .await,
        StatusCode::CONFLICT,
    );
    let cross_paper_key = format!("p3-cross-paper-parent-{}", Uuid::new_v4());
    let cross_paper_parent_code = error_code(
        request(
            &context.router,
            "POST",
            &proposal_path,
            signed_agent_proposal_body(
                &context,
                Uuid::new_v4(),
                "results",
                cross_context.revision_id,
                &digest("cross-paper-parent"),
                &cross_paper_key,
            ),
            None,
        )
        .await,
        StatusCode::CONFLICT,
    );

    let transition_path = format!("/v2/hepta/papers/{}/transition", context.paper_id);
    for phase in ["integrity_review", "reproducing", "author_approval"] {
        let key = format!("p3-freeze-phase-{}-{phase}", context.paper_id);
        assert_status(
            user_post(
                &context.router,
                actor,
                "transition_paper_project_v2",
                &transition_path,
                &key,
                json!({
                    "expected_version":context.paper_version,
                    "next_phase":phase,
                    "idempotency_key":key,
                }),
            )
            .await,
            StatusCode::OK,
        );
        context.paper_version += 1;
    }
    let (frozen_bundle, frozen_sha256, frozen_locations) =
        integration_artifact_bundle(challenge_id, "frozen");
    let frozen_manifest_id = Uuid::new_v4();
    let frozen_key = format!("p3-frozen-artifact-{frozen_manifest_id}");
    let artifact_path = format!("/v2/hepta/papers/{}/artifact-manifests", context.paper_id);
    let frozen_phase_code = error_code(
        user_post(
            &context.router,
            actor,
            "create_artifact_manifest_v3",
            &artifact_path,
            &frozen_key,
            json!({
                "manifest_id":frozen_manifest_id,
                "expected_paper_version":context.paper_version,
                "expected_source_manifest_sha256":frozen_sha256,
                "source_bundle":frozen_bundle,
                "storage_locations":frozen_locations,
                "idempotency_key":frozen_key,
            }),
        )
        .await,
        StatusCode::CONFLICT,
    );

    let final_snapshot = collaboration_snapshot(&context, "final").await;
    let room = &final_snapshot["room"];
    let events = final_snapshot["events"].as_array().expect("room events");
    let room_last_cursor = room["last_event_cursor"].as_u64().expect("room cursor");
    assert_eq!(
        events.last().and_then(|event| event["cursor"].as_u64()),
        Some(room_last_cursor),
        "repeatable read must not expose a model/cursor split"
    );

    CollaborationOutcome {
        revision_unknown_code,
        revision_cross_paper_code,
        revision_descriptor_code,
        uri_tamper_code,
        doi_tamper_code,
        duplicate_proposal_code,
        stale_fencing_code,
        stale_parent_code,
        cyclic_parent_code,
        cross_section_parent_code,
        cross_paper_parent_code,
        frozen_phase_code,
        room_event_count: events.len(),
        room_last_cursor,
        revision_binding_count: room["revision_artifact_bindings"]
            .as_array()
            .expect("revision bindings")
            .len(),
        section_merge_count: room["section_merges"]
            .as_array()
            .expect("section merges")
            .len(),
    }
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
        let rotation_key = format!("agent-binding-rotation-{member_count}");
        let old_public_key = BASE64.encode(actors[0].agent_key.verifying_key().to_bytes());
        let old_key_id = sha256_digest(&actors[0].agent_key.verifying_key().to_bytes());
        let new_public_key = BASE64.encode(new_agent_key.verifying_key().to_bytes());
        let new_key_id = sha256_digest(&new_agent_key.verifying_key().to_bytes());
        let issued_at_unix = Utc::now().timestamp();
        let rotation = AgentBindingKeyRotationClaimV2 {
            schema: AGENT_BINDING_KEY_ROTATION_V2.to_string(),
            rotation_id: Uuid::new_v4(),
            binding_id: actors[0].binding_id,
            expected_binding_version: 1,
            player_id: actors[0].player_id,
            subject_id: actors[0].subject_id.clone(),
            agent_id: actors[0].agent_id.clone(),
            old_agent_key_id: old_key_id.clone(),
            old_agent_public_key: old_public_key.clone(),
            old_agent_public_key_hash: old_key_id.clone(),
            new_agent_key_id: new_key_id.clone(),
            new_agent_public_key: new_public_key.clone(),
            new_agent_public_key_hash: new_key_id.clone(),
            nonce: rotation_key.clone(),
            issued_at_unix,
            expires_at_unix: issued_at_unix + 300,
        };
        let rotation_frame = agent_binding_key_rotation_signing_bytes(&rotation)
            .expect("Agent binding rotation frame");
        let rotation_path = format!(
            "/v2/hepta/agent-bindings/{}/rotate-key",
            actors[0].binding_id
        );
        assert_status(
            user_post(
                router,
                &actors[0],
                "rotate_agent_binding_key_v2",
                &rotation_path,
                &rotation_key,
                json!({
                    "rotation_id":rotation.rotation_id,
                    "expected_binding_version":rotation.expected_binding_version,
                    "agent_id":rotation.agent_id,
                    "old_agent_key_id":old_key_id,
                    "old_agent_public_key":old_public_key,
                    "new_agent_key_id":new_key_id,
                    "new_agent_public_key":new_public_key,
                    "issued_at_unix":rotation.issued_at_unix,
                    "expires_at_unix":rotation.expires_at_unix,
                    "old_key_signature":BASE64.encode(actors[0].agent_key.sign(&rotation_frame).to_bytes()),
                    "new_key_signature":BASE64.encode(new_agent_key.sign(&rotation_frame).to_bytes()),
                    "idempotency_key":rotation_key,
                }),
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

    let verified_artifact = create_verified_artifact_manifest(
        &router,
        &actors[0],
        paper_id,
        challenge_id,
        paper_version,
        &format!("authors-{member_count}"),
    )
    .await;

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
        let artifact =
            (status == "accepted").then(|| verified_artifact.artifact_manifest_hash.clone());
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
                "source_manifest_hash":verified_artifact.source_manifest_hash,
                "artifact_manifest_hash":verified_artifact.artifact_manifest_hash,
                "bibliography_hash":verified_artifact.bibliography_hash,
                "claim_evidence_graph_hash":verified_artifact.claim_evidence_graph_hash,
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
    let contribution_ledger_id = Uuid::from_u128(0x9000_0000_0000_4000_8000_0000_0000_0000 + base);
    let mut contribution_entries = actors
        .iter()
        .map(|actor| {
            json!({
                "player_id": actor.player_id,
                "credit_roles": ["methodology", "writing_review_editing"],
                "accepted_artifact_manifest_ids": [],
                "accepted_section_review_ids": [],
                "contribution_points": 0,
            })
        })
        .collect::<Vec<_>>();
    contribution_entries.sort_by_key(|entry| entry["player_id"].as_str().unwrap().to_string());
    let contribution_ledger_hash = canonical_json_sha256(&json!({
        "schema": CONTRIBUTION_LEDGER_SCHEMA_V1,
        "contribution_ledger_id": contribution_ledger_id,
        "paper_project_id": paper_id,
        "entries": contribution_entries,
    }))
    .expect("contribution ledger preimage");
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
                "contribution_ledger_hash":contribution_ledger_hash,
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

    let ledger_path = format!("/v2/hepta/papers/{paper_id}/contribution-ledgers");
    let ledger_key = format!("contribution-ledger-{member_count}");
    let ledger = assert_status(
        user_post(
            &router,
            &actors[0],
            "create_contribution_ledger_v1",
            &ledger_path,
            &ledger_key,
            json!({
                "contribution_ledger_id":contribution_ledger_id,
                "expected_paper_version":paper_version,
                "release_candidate_hash":release_candidate_hash,
                "entries":actors.iter().map(|actor| json!({
                    "player_id":actor.player_id,
                    "credit_roles":["methodology","writing_review_editing"],
                    "accepted_artifact_manifest_ids":[],
                    "accepted_section_review_ids":[],
                })).collect::<Vec<_>>(),
                "idempotency_key":ledger_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(ledger["ledger_hash"], contribution_ledger_hash);

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

pub(super) async fn seed_three_member_postgres_flow_for_control_test(state: AppState) {
    let _ = run_full_flow(state, 3, FlowOptions::default()).await;
}

#[allow(dead_code)]
pub(crate) async fn seed_paper_chain_finality_test(state: AppState) -> PaperTrnmCommandBindingV1 {
    let _ = run_full_flow(state.clone(), 3, FlowOptions::default()).await;
    let router = app(state);
    let authors = actors(3);
    let external = actors(11);
    register_prerequisites(&router, &external).await;
    create_players_and_bindings(&router, &external).await;
    let paper_id = Uuid::from_u128(0x5000_0000_0000_4000_8000_0000_0000_0000 + 3 * 0x100);
    let submission = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_joint_paper_submission_v2",
            &format!("/v2/hepta/papers/{paper_id}/submission"),
            "chain-finality-read-submission",
        )
        .await,
        StatusCode::OK,
    );
    let evaluation_id = Uuid::new_v4();
    let evaluation_key = "chain-finality-evaluation";
    let evaluation = assert_status(
        user_post(
            &router,
            &external[0],
            "create_paper_evaluation_v1",
            &format!("/v2/hepta/papers/{paper_id}/evaluations"),
            evaluation_key,
            evaluation_body(
                paper_id,
                &submission,
                &external[0],
                [&external[1], &external[2]],
                evaluation_id,
                None,
                evaluation_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    let reproduction_id = Uuid::new_v4();
    let reproduction_key = "chain-finality-reproduction";
    let reproduction = assert_status(
        user_post(
            &router,
            &external[3],
            "create_paper_reproduction_v1",
            &format!("/v2/hepta/papers/{paper_id}/evaluations/{evaluation_id}/reproductions"),
            reproduction_key,
            reproduction_body(
                paper_id,
                &evaluation,
                &external[3],
                reproduction_id,
                None,
                true,
                reproduction_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    let evaluation: PaperEvaluation =
        serde_json::from_value(evaluation).expect("decode seeded evaluation");
    let reproduction: PaperReproduction =
        serde_json::from_value(reproduction).expect("decode seeded reproduction");
    let submission: JointPaperSubmission =
        serde_json::from_value(submission).expect("decode seeded submission");
    let mut binding = PaperTrnmCommandBindingV1 {
        schema: PAPER_TRNM_COMMAND_BINDING_SCHEMA_V1.to_string(),
        paper_project_id: paper_id,
        submission_id: submission.submission_id,
        evaluation_id,
        research_session_id: "paper-raid-3-session".to_string(),
        research_session_roster_version: 1,
        match_evidence_commitment_id: digest("paper-chain-finality-match-evidence"),
        match_evidence_object_version: 1,
        release_candidate_hash: submission.release_candidate_hash,
        paper_bundle_hash: submission.paper_bundle_hash,
        submission_commitment_hash: digest("placeholder-submission-commitment"),
        tolerance_policy_hash: evaluation.tolerance_policy_hash,
        evaluation_signing_hash: evaluation.evaluation_signing_hash,
        reproduction_id,
        reproduction_report_hash: reproduction.report_hash,
        evaluation_score_bps: evaluation.paper_score.score_bps,
        evaluation_accepted: evaluation.status == PaperEvaluationStatus::Accepted,
        evaluation_completed_at_unix_s: u64::try_from(evaluation.created_at.timestamp())
            .expect("positive evaluation time"),
    };
    binding.submission_commitment_hash =
        paper_trnm_submission_commitment_hash(&binding).expect("Paper submission commitment");
    binding
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReviewFlowOutcome {
    evaluation_status: String,
    settlement_before_appeal: String,
    reproduction_passed: String,
    reproduction_failed: String,
    challenged_state: String,
    resolved_state: String,
    panel_overlap_code: String,
    appellant_spoof_code: String,
    resolver_overlap_code: String,
    evaluation_count: usize,
    reproduction_count: usize,
    appeal_count: usize,
    resolution_count: usize,
    contribution_room_event_count: usize,
    evaluation_room_event_count: usize,
    reproduction_room_event_count: usize,
    appeal_room_event_count: usize,
    resolution_room_event_count: usize,
    appeal_outbox_event_count: usize,
    resolution_outbox_event_count: usize,
    appeal_room_settlement_state: String,
    resolution_room_settlement_state: String,
}

fn evaluation_body(
    paper_id: Uuid,
    submission: &Value,
    evaluator: &Actor,
    reviewers: [&Actor; 2],
    evaluation_id: Uuid,
    supersedes_evaluation_id: Option<Uuid>,
    idempotency_key: &str,
) -> Value {
    let tolerance_policy = TolerancePolicy {
        schema: TOLERANCE_POLICY_SCHEMA_V1.to_string(),
        version: "1".to_string(),
        rules: vec![
            ToleranceRule::Absolute {
                metric: "accuracy_micros".to_string(),
                max_delta_micros: 20,
            },
            ToleranceRule::Relative {
                metric: "loss_micros".to_string(),
                max_delta_bps: 500,
            },
            ToleranceRule::Statistical {
                metric: "effect_micros".to_string(),
                minimum_interval_overlap_bps: 7_500,
                maximum_effect_delta_micros: 30,
                minimum_p_value_micros: 50_000,
            },
            ToleranceRule::Seed {
                expected_seed_set_hash: digest("review-seed-set"),
            },
        ],
    };
    let reference_metrics_micros = BTreeMap::from([
        ("accuracy_micros".to_string(), 900_000_i64),
        ("effect_micros".to_string(), 120_i64),
        ("loss_micros".to_string(), 100_000_i64),
    ]);
    let score_components = PaperScoreComponents {
        method_rigor_bps: 2_200,
        experiment_statistics_bps: 1_300,
        reproducibility_bps: 1_300,
        evidence_citations_bps: 1_300,
        value_originality_bps: 1_200,
        argument_expression_bps: 800,
        ethics_transparency_bps: 400,
    };
    let hard_gates = PaperHardGates {
        citations_and_data_authentic: true,
        failed_runs_disclosed: true,
        all_authors_consented: true,
        core_claims_have_evidence: true,
        artifact_lineage_complete: true,
        license_ethics_coi_complete: true,
    };
    let score_bps = 8_500_u16;
    let paper_score_hash = canonical_json_sha256(&json!({
        "schema":PAPER_SCORE_SCHEMA_V1,
        "evaluation_id":evaluation_id,
        "paper_project_id":paper_id,
        "components":score_components,
        "hard_gates":hard_gates,
        "score_bps":score_bps,
        "eligible":true,
    }))
    .expect("paper score hash");
    let tolerance_policy_hash =
        canonical_json_sha256(&tolerance_policy).expect("tolerance policy hash");
    let reference_metrics_hash =
        canonical_json_sha256(&reference_metrics_micros).expect("reference metrics hash");
    let hard_gates_hash = canonical_json_sha256(&hard_gates).expect("hard gates hash");
    let signed_at_unix = Utc::now().timestamp();
    let release_candidate_hash = submission["release_candidate_hash"]
        .as_str()
        .expect("release hash")
        .to_string();
    let paper_bundle_hash = submission["paper_bundle_hash"]
        .as_str()
        .expect("bundle hash")
        .to_string();
    let signing = PaperEvaluationSigningV1 {
        schema: PAPER_EVALUATION_V1.to_string(),
        evaluation_id,
        paper_project_id: paper_id,
        submission_id: Uuid::parse_str(
            submission["submission_id"].as_str().expect("submission id"),
        )
        .expect("submission UUID"),
        release_candidate_hash: release_candidate_hash.clone(),
        paper_bundle_hash: paper_bundle_hash.clone(),
        supersedes_evaluation_id,
        tolerance_policy_hash,
        paper_score_hash,
        reference_metrics_hash,
        hard_gates_hash,
        evaluator_player_id: evaluator.player_id,
        signing_key_id: evaluator.human_key_id.clone(),
        signing_public_key_hash: evaluator.human_public_key_hash.clone(),
        coi_attestation_hash: digest(&format!("coi-evaluator-{}", evaluator.player_id)),
        signed_at_unix,
    };
    let frame = paper_evaluation_signing_bytes(&signing).expect("evaluation frame");
    let evaluation_signing_hash = sha256_digest(&frame);
    let reviewer_attestations = reviewers
        .iter()
        .map(|reviewer| {
            let attestation_id = Uuid::new_v4();
            let reviewer_signed_at = Utc::now().timestamp();
            let coi = digest(&format!("coi-reviewer-{}", reviewer.player_id));
            let signing = PaperReviewAttestationSigningV1 {
                schema: PAPER_REVIEW_ATTESTATION_V1.to_string(),
                attestation_id,
                evaluation_id,
                evaluation_signing_hash: evaluation_signing_hash.clone(),
                reviewer_player_id: reviewer.player_id,
                verdict: "approve".to_string(),
                signing_key_id: reviewer.human_key_id.clone(),
                signing_public_key_hash: reviewer.human_public_key_hash.clone(),
                coi_attestation_hash: coi.clone(),
                signed_at_unix: reviewer_signed_at,
            };
            let frame =
                paper_review_attestation_signing_bytes(&signing).expect("review attestation frame");
            json!({
                "attestation_id":attestation_id,
                "reviewer_player_id":reviewer.player_id,
                "verdict":"approve",
                "signing_key_id":reviewer.human_key_id,
                "signing_public_key":reviewer.human_public_key,
                "signing_public_key_hash":reviewer.human_public_key_hash,
                "coi_attestation_hash":coi,
                "signed_at_unix":reviewer_signed_at,
                "signature":BASE64.encode(reviewer.human_key.sign(&frame).to_bytes()),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "evaluation_id":evaluation_id,
        "submission_id":signing.submission_id,
        "supersedes_evaluation_id":supersedes_evaluation_id,
        "release_candidate_hash":release_candidate_hash,
        "paper_bundle_hash":paper_bundle_hash,
        "tolerance_policy":tolerance_policy,
        "reference_metrics_micros":reference_metrics_micros,
        "score_components":score_components,
        "hard_gates":hard_gates,
        "evaluator_player_id":evaluator.player_id,
        "evaluator_signing_key_id":evaluator.human_key_id,
        "evaluator_signing_public_key":evaluator.human_public_key,
        "evaluator_signing_public_key_hash":evaluator.human_public_key_hash,
        "evaluator_coi_attestation_hash":signing.coi_attestation_hash,
        "evaluator_signed_at_unix":signed_at_unix,
        "evaluator_signature":BASE64.encode(evaluator.human_key.sign(&frame).to_bytes()),
        "reviewer_attestations":reviewer_attestations,
        "idempotency_key":idempotency_key,
    })
}

fn reproduction_body(
    paper_id: Uuid,
    evaluation: &Value,
    reproducer: &Actor,
    reproduction_id: Uuid,
    supersedes: Option<Uuid>,
    passing: bool,
    idempotency_key: &str,
) -> Value {
    let observed_metrics_micros = BTreeMap::from([
        (
            "accuracy_micros".to_string(),
            if passing { 900_010_i64 } else { 899_000_i64 },
        ),
        ("effect_micros".to_string(), 125_i64),
        ("loss_micros".to_string(), 102_000_i64),
    ]);
    let statistical_evidence = BTreeMap::from([(
        "effect_micros".to_string(),
        StatisticalEvidence {
            interval_overlap_bps: if passing { 8_500 } else { 1_000 },
            effect_delta_micros: if passing { 10 } else { 500 },
            p_value_micros: if passing { 80_000 } else { 10_000 },
        },
    )]);
    let seed_set_hash = if passing {
        digest("review-seed-set")
    } else {
        digest("wrong-seed-set")
    };
    let signed_at_unix = Utc::now().timestamp();
    let evaluation_id =
        Uuid::parse_str(evaluation["evaluation_id"].as_str().expect("evaluation id"))
            .expect("evaluation UUID");
    let signing = PaperReproductionSigningV1 {
        schema: PAPER_REPRODUCTION_V1.to_string(),
        reproduction_id,
        evaluation_id,
        paper_project_id: paper_id,
        release_candidate_hash: evaluation["release_candidate_hash"]
            .as_str()
            .expect("release hash")
            .to_string(),
        paper_bundle_hash: evaluation["paper_bundle_hash"]
            .as_str()
            .expect("bundle hash")
            .to_string(),
        tolerance_policy_hash: evaluation["tolerance_policy_hash"]
            .as_str()
            .expect("tolerance hash")
            .to_string(),
        observed_metrics_hash: canonical_json_sha256(&observed_metrics_micros)
            .expect("observed metrics hash"),
        statistical_evidence_hash: canonical_json_sha256(&statistical_evidence)
            .expect("statistical evidence hash"),
        seed_set_hash: seed_set_hash.clone(),
        environment_hash: digest("reproduction-environment"),
        run_manifest_hash: digest(&format!("reproduction-run-{reproduction_id}")),
        supersedes_reproduction_id: supersedes,
        reproducer_player_id: reproducer.player_id,
        signing_key_id: reproducer.human_key_id.clone(),
        signing_public_key_hash: reproducer.human_public_key_hash.clone(),
        coi_attestation_hash: digest(&format!("coi-reproducer-{}", reproducer.player_id)),
        signed_at_unix,
    };
    let frame = paper_reproduction_signing_bytes(&signing).expect("reproduction frame");
    json!({
        "reproduction_id":reproduction_id,
        "supersedes_reproduction_id":supersedes,
        "release_candidate_hash":signing.release_candidate_hash,
        "paper_bundle_hash":signing.paper_bundle_hash,
        "observed_metrics_micros":observed_metrics_micros,
        "statistical_evidence":statistical_evidence,
        "seed_set_hash":seed_set_hash,
        "environment_hash":signing.environment_hash,
        "run_manifest_hash":signing.run_manifest_hash,
        "reproducer_player_id":reproducer.player_id,
        "signing_key_id":reproducer.human_key_id,
        "signing_public_key":reproducer.human_public_key,
        "signing_public_key_hash":reproducer.human_public_key_hash,
        "coi_attestation_hash":signing.coi_attestation_hash,
        "signed_at_unix":signed_at_unix,
        "signature":BASE64.encode(reproducer.human_key.sign(&frame).to_bytes()),
        "idempotency_key":idempotency_key,
    })
}

fn appeal_body(
    paper_id: Uuid,
    evaluation: &Value,
    appellant: &Actor,
    appeal_id: Uuid,
    idempotency_key: &str,
) -> Value {
    let signed_at_unix = Utc::now().timestamp();
    let evaluation_id =
        Uuid::parse_str(evaluation["evaluation_id"].as_str().expect("evaluation id"))
            .expect("evaluation UUID");
    let signing = PaperAppealSigningV1 {
        schema: PAPER_APPEAL_V1.to_string(),
        appeal_id,
        evaluation_id,
        paper_project_id: paper_id,
        release_candidate_hash: evaluation["release_candidate_hash"]
            .as_str()
            .expect("release hash")
            .to_string(),
        appellant_player_id: appellant.player_id,
        grounds_hash: digest("appeal-grounds"),
        evidence_manifest_hash: digest("appeal-evidence"),
        signing_key_id: appellant.human_key_id.clone(),
        signing_public_key_hash: appellant.human_public_key_hash.clone(),
        signed_at_unix,
    };
    let frame = paper_appeal_signing_bytes(&signing).expect("Appeal frame");
    json!({
        "appeal_id":appeal_id,
        "release_candidate_hash":signing.release_candidate_hash,
        "appellant_player_id":appellant.player_id,
        "grounds_hash":signing.grounds_hash,
        "evidence_manifest_hash":signing.evidence_manifest_hash,
        "signing_key_id":appellant.human_key_id,
        "signing_public_key":appellant.human_public_key,
        "signing_public_key_hash":appellant.human_public_key_hash,
        "signed_at_unix":signed_at_unix,
        "signature":BASE64.encode(appellant.human_key.sign(&frame).to_bytes()),
        "idempotency_key":idempotency_key,
    })
}

struct ResolutionBody<'a> {
    resolution_id: Uuid,
    outcome: &'a str,
    superseding_evaluation_id: Option<Uuid>,
    idempotency_key: &'a str,
}

fn resolution_body(
    paper_id: Uuid,
    evaluation: &Value,
    appeal_id: Uuid,
    resolver: &Actor,
    request: ResolutionBody<'_>,
) -> Value {
    let signed_at_unix = Utc::now().timestamp();
    let evaluation_id =
        Uuid::parse_str(evaluation["evaluation_id"].as_str().expect("evaluation id"))
            .expect("evaluation UUID");
    let signing = PaperAppealResolutionSigningV1 {
        schema: PAPER_APPEAL_RESOLUTION_V1.to_string(),
        resolution_id: request.resolution_id,
        appeal_id,
        evaluation_id,
        paper_project_id: paper_id,
        release_candidate_hash: evaluation["release_candidate_hash"]
            .as_str()
            .expect("release hash")
            .to_string(),
        outcome: request.outcome.to_string(),
        superseding_evaluation_id: request.superseding_evaluation_id,
        decision_hash: digest(&format!("appeal-resolution-{}", request.outcome)),
        resolver_player_id: resolver.player_id,
        signing_key_id: resolver.human_key_id.clone(),
        signing_public_key_hash: resolver.human_public_key_hash.clone(),
        signed_at_unix,
    };
    let frame = paper_appeal_resolution_signing_bytes(&signing).expect("Appeal resolution frame");
    json!({
        "resolution_id":request.resolution_id,
        "outcome":request.outcome,
        "superseding_evaluation_id":request.superseding_evaluation_id,
        "decision_hash":signing.decision_hash,
        "resolver_player_id":resolver.player_id,
        "signing_key_id":resolver.human_key_id,
        "signing_public_key":resolver.human_public_key,
        "signing_public_key_hash":resolver.human_public_key_hash,
        "signed_at_unix":signed_at_unix,
        "signature":BASE64.encode(resolver.human_key.sign(&frame).to_bytes()),
        "idempotency_key":request.idempotency_key,
    })
}

async fn run_review_flow(state: AppState) -> ReviewFlowOutcome {
    let _ = run_full_flow(state.clone(), 3, FlowOptions::default()).await;
    let router = app(state.clone());
    let authors = actors(3);
    let external = actors(11);
    register_prerequisites(&router, &external).await;
    create_players_and_bindings(&router, &external).await;
    let paper_id = Uuid::from_u128(0x5000_0000_0000_4000_8000_0000_0000_0000 + 3 * 0x100);
    let submission_path = format!("/v2/hepta/papers/{paper_id}/submission");
    let submission = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_joint_paper_submission_v2",
            &submission_path,
            "p5-read-submission",
        )
        .await,
        StatusCode::OK,
    );
    let evaluation_path = format!("/v2/hepta/papers/{paper_id}/evaluations");
    let bad_evaluation_id = Uuid::new_v4();
    let bad_key = "p5-panel-overlap";
    let panel_overlap_code = error_code(
        user_post(
            &router,
            &authors[0],
            "create_paper_evaluation_v1",
            &evaluation_path,
            bad_key,
            evaluation_body(
                paper_id,
                &submission,
                &authors[0],
                [&external[0], &external[1]],
                bad_evaluation_id,
                None,
                bad_key,
            ),
        )
        .await,
        StatusCode::FORBIDDEN,
    );
    let evaluation_id = Uuid::new_v4();
    let evaluation_key = "p5-evaluation";
    let evaluation = assert_status(
        user_post(
            &router,
            &external[0],
            "create_paper_evaluation_v1",
            &evaluation_path,
            evaluation_key,
            evaluation_body(
                paper_id,
                &submission,
                &external[0],
                [&external[1], &external[2]],
                evaluation_id,
                None,
                evaluation_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(evaluation["paper_score"]["score_bps"], 8_500);
    assert_eq!(evaluation["settlement_state"], "pending_finality");
    let reproduction_path =
        format!("/v2/hepta/papers/{paper_id}/evaluations/{evaluation_id}/reproductions");
    let passing_id = Uuid::new_v4();
    let passing_key = "p5-reproduction-pass";
    let passing = assert_status(
        user_post(
            &router,
            &external[3],
            "create_paper_reproduction_v1",
            &reproduction_path,
            passing_key,
            reproduction_body(
                paper_id,
                &evaluation,
                &external[3],
                passing_id,
                None,
                true,
                passing_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    let failing_id = Uuid::new_v4();
    let failing_key = "p5-reproduction-fail";
    let failing = assert_status(
        user_post(
            &router,
            &external[4],
            "create_paper_reproduction_v1",
            &reproduction_path,
            failing_key,
            reproduction_body(
                paper_id,
                &evaluation,
                &external[4],
                failing_id,
                None,
                false,
                failing_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    let superseding_reproduction_id = Uuid::new_v4();
    let superseding_reproduction_key = "p5-reproduction-superseding";
    assert_status(
        user_post(
            &router,
            &external[3],
            "create_paper_reproduction_v1",
            &reproduction_path,
            superseding_reproduction_key,
            reproduction_body(
                paper_id,
                &evaluation,
                &external[3],
                superseding_reproduction_id,
                Some(passing_id),
                true,
                superseding_reproduction_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    let appeal_path = format!("/v2/hepta/papers/{paper_id}/evaluations/{evaluation_id}/appeals");
    let spoof_key = "p5-appeal-spoof";
    let appellant_spoof_code = error_code(
        user_post(
            &router,
            &authors[1],
            "create_paper_appeal_v1",
            &appeal_path,
            spoof_key,
            appeal_body(
                paper_id,
                &evaluation,
                &authors[0],
                Uuid::new_v4(),
                spoof_key,
            ),
        )
        .await,
        StatusCode::FORBIDDEN,
    );
    let appeal_id = Uuid::new_v4();
    let appeal_key = "p5-appeal";
    let appeal_request = appeal_body(paper_id, &evaluation, &authors[0], appeal_id, appeal_key);
    let appeal = assert_status(
        user_post(
            &router,
            &authors[0],
            "create_paper_appeal_v1",
            &appeal_path,
            appeal_key,
            appeal_request.clone(),
        )
        .await,
        StatusCode::CREATED,
    );
    let appeal_replay = assert_status(
        user_post(
            &router,
            &authors[0],
            "create_paper_appeal_v1",
            &appeal_path,
            appeal_key,
            appeal_request,
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(appeal_replay, appeal, "Appeal replay must be exact");
    let events_path = format!("/v2/hepta/papers/{paper_id}/events?after_cursor=0");
    let challenged_events = assert_status(
        user_get(
            &router,
            &authors[0],
            "list_paper_room_events_v3",
            &events_path,
            "p5-read-events-challenged",
        )
        .await,
        StatusCode::OK,
    );
    let opened_events = challenged_events
        .as_array()
        .expect("Paper Room events")
        .iter()
        .filter(|event| event["event_type"] == "hepta.paper_raid.appeal.opened.v1")
        .collect::<Vec<_>>();
    assert_eq!(
        opened_events.len(),
        1,
        "Appeal replay must not duplicate its Paper Room event"
    );
    assert_eq!(
        opened_events[0]["payload"]["settlement_state"],
        "challenged"
    );
    let read_path = format!("/v2/hepta/papers/{paper_id}/review-state");
    let challenged = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_paper_review_state_v1",
            &read_path,
            "p5-read-challenged",
        )
        .await,
        StatusCode::OK,
    );
    let superseding_evaluation_id = Uuid::new_v4();
    let superseding_evaluation_key = "p5-evaluation-superseding";
    assert_status(
        user_post(
            &router,
            &external[6],
            "create_paper_evaluation_v1",
            &evaluation_path,
            superseding_evaluation_key,
            evaluation_body(
                paper_id,
                &submission,
                &external[6],
                [&external[7], &external[8]],
                superseding_evaluation_id,
                Some(evaluation_id),
                superseding_evaluation_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    let resolve_path = format!("/v2/hepta/papers/{paper_id}/appeals/{appeal_id}/resolve");
    let overlap_key = "p5-resolution-overlap";
    let resolver_overlap_code = error_code(
        user_post(
            &router,
            &authors[1],
            "resolve_paper_appeal_v1",
            &resolve_path,
            overlap_key,
            resolution_body(
                paper_id,
                &evaluation,
                appeal_id,
                &authors[1],
                ResolutionBody {
                    resolution_id: Uuid::new_v4(),
                    outcome: "upheld",
                    superseding_evaluation_id: Some(superseding_evaluation_id),
                    idempotency_key: overlap_key,
                },
            ),
        )
        .await,
        StatusCode::FORBIDDEN,
    );
    let resolution_key = "p5-resolution";
    let resolution_request = resolution_body(
        paper_id,
        &evaluation,
        appeal_id,
        &external[9],
        ResolutionBody {
            resolution_id: Uuid::new_v4(),
            outcome: "upheld",
            superseding_evaluation_id: Some(superseding_evaluation_id),
            idempotency_key: resolution_key,
        },
    );
    let resolution = assert_status(
        user_post(
            &router,
            &external[9],
            "resolve_paper_appeal_v1",
            &resolve_path,
            resolution_key,
            resolution_request.clone(),
        )
        .await,
        StatusCode::CREATED,
    );
    let resolution_replay = assert_status(
        user_post(
            &router,
            &external[9],
            "resolve_paper_appeal_v1",
            &resolve_path,
            resolution_key,
            resolution_request,
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(
        resolution_replay, resolution,
        "Appeal resolution replay must be exact"
    );
    let room_events = assert_status(
        user_get(
            &router,
            &authors[0],
            "list_paper_room_events_v3",
            &events_path,
            "p5-read-events-resolved",
        )
        .await,
        StatusCode::OK,
    );
    let room_events = room_events.as_array().expect("Paper Room events");
    let room_event_count = |event_type: &str| {
        room_events
            .iter()
            .filter(|event| event["event_type"] == event_type)
            .count()
    };
    let resolved_events = room_events
        .iter()
        .filter(|event| event["event_type"] == "hepta.paper_raid.appeal.resolved.v1")
        .collect::<Vec<_>>();
    assert_eq!(
        resolved_events.len(),
        1,
        "resolution replay must not duplicate its Paper Room event"
    );
    assert_eq!(
        resolved_events[0]["payload"]["settlement_state"],
        "resolved"
    );
    let outbox_event_types = paper_raid_event_types(&state).await;
    let outbox_event_count = |event_type: &str| {
        outbox_event_types
            .iter()
            .filter(|candidate| candidate.as_str() == event_type)
            .count()
    };
    let resolved = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_paper_review_state_v1",
            &read_path,
            "p5-read-resolved",
        )
        .await,
        StatusCode::OK,
    );
    ReviewFlowOutcome {
        evaluation_status: evaluation["status"].as_str().unwrap().to_string(),
        settlement_before_appeal: evaluation["settlement_state"].as_str().unwrap().to_string(),
        reproduction_passed: passing["status"].as_str().unwrap().to_string(),
        reproduction_failed: failing["status"].as_str().unwrap().to_string(),
        challenged_state: challenged["evaluations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["evaluation_id"] == evaluation_id.to_string())
            .unwrap()["settlement_state"]
            .as_str()
            .unwrap()
            .to_string(),
        resolved_state: resolved["evaluations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["evaluation_id"] == evaluation_id.to_string())
            .unwrap()["settlement_state"]
            .as_str()
            .unwrap()
            .to_string(),
        panel_overlap_code,
        appellant_spoof_code,
        resolver_overlap_code,
        evaluation_count: resolved["evaluations"].as_array().unwrap().len(),
        reproduction_count: resolved["reproductions"].as_array().unwrap().len(),
        appeal_count: resolved["appeals"].as_array().unwrap().len(),
        resolution_count: resolved["resolutions"].as_array().unwrap().len(),
        contribution_room_event_count: room_event_count(
            "hepta.paper_raid.contribution_ledger.frozen.v1",
        ),
        evaluation_room_event_count: room_event_count("hepta.paper_raid.evaluation.recorded.v1"),
        reproduction_room_event_count: room_event_count(
            "hepta.paper_raid.reproduction.recorded.v1",
        ),
        appeal_room_event_count: room_event_count("hepta.paper_raid.appeal.opened.v1"),
        resolution_room_event_count: room_event_count("hepta.paper_raid.appeal.resolved.v1"),
        appeal_outbox_event_count: outbox_event_count("hepta.paper_raid.appeal.opened.v1"),
        resolution_outbox_event_count: outbox_event_count("hepta.paper_raid.appeal.resolved.v1"),
        appeal_room_settlement_state: opened_events[0]["payload"]["settlement_state"]
            .as_str()
            .expect("Appeal event settlement state")
            .to_string(),
        resolution_room_settlement_state: resolved_events[0]["payload"]["settlement_state"]
            .as_str()
            .expect("resolution event settlement state")
            .to_string(),
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
async fn memory_review_reproduction_and_appeal_are_signed_independent_and_immutable() {
    let outcome = run_review_flow(AppState::new(security())).await;
    assert_eq!(outcome.evaluation_status, "accepted");
    assert_eq!(outcome.settlement_before_appeal, "pending_finality");
    assert_eq!(outcome.reproduction_passed, "reproduced");
    assert_eq!(outcome.reproduction_failed, "failed_tolerance");
    assert_eq!(outcome.challenged_state, "challenged");
    assert_eq!(outcome.resolved_state, "resolved");
    assert_eq!(outcome.panel_overlap_code, "review_panel_not_independent");
    assert_eq!(outcome.appellant_spoof_code, "appellant_assertion_mismatch");
    assert_eq!(
        outcome.resolver_overlap_code,
        "appeal_resolver_not_independent"
    );
    assert_eq!(outcome.evaluation_count, 2);
    assert_eq!(outcome.reproduction_count, 3);
    assert_eq!(outcome.appeal_count, 1);
    assert_eq!(outcome.resolution_count, 1);
    assert_eq!(outcome.contribution_room_event_count, 1);
    assert_eq!(outcome.evaluation_room_event_count, 2);
    assert_eq!(outcome.reproduction_room_event_count, 3);
    assert_eq!(outcome.appeal_room_event_count, 1);
    assert_eq!(outcome.resolution_room_event_count, 1);
    assert_eq!(outcome.appeal_outbox_event_count, 1);
    assert_eq!(outcome.resolution_outbox_event_count, 1);
    assert_eq!(outcome.appeal_room_settlement_state, "challenged");
    assert_eq!(outcome.resolution_room_settlement_state, "resolved");
}

#[tokio::test]
async fn memory_collaboration_kernel_is_atomic_signed_and_phase_frozen() {
    let outcome = run_collaboration_kernel_flow(AppState::new(security())).await;
    assert_eq!(
        outcome.revision_unknown_code,
        "paper_revision_artifact_manifest_unknown"
    );
    assert_eq!(
        outcome.revision_cross_paper_code,
        "paper_revision_artifact_cross_paper"
    );
    assert_eq!(
        outcome.revision_descriptor_code,
        "paper_revision_artifact_descriptor_mismatch"
    );
    assert_eq!(
        outcome.uri_tamper_code,
        "human_verification_signature_failed"
    );
    assert_eq!(
        outcome.doi_tamper_code,
        "human_verification_signature_failed"
    );
    assert_eq!(outcome.duplicate_proposal_code, "agent_proposal_exists");
    assert_eq!(outcome.stale_fencing_code, "stale_fencing_token");
    assert_eq!(outcome.stale_parent_code, "stale_section_parent");
    assert_eq!(outcome.cyclic_parent_code, "stale_section_parent");
    assert_eq!(outcome.cross_section_parent_code, "stale_section_parent");
    assert_eq!(outcome.cross_paper_parent_code, "stale_section_parent");
    assert_eq!(
        outcome.frozen_phase_code,
        "paper_phase_disallows_collaboration_mutation"
    );
    assert_eq!(outcome.revision_binding_count, 1);
    assert_eq!(outcome.section_merge_count, 1);
    assert!(outcome.room_event_count >= 11);
    assert!(outcome.room_last_cursor >= outcome.room_event_count as u64);
}

#[tokio::test]
async fn postgres_collaboration_kernel_matches_memory_and_migration_is_repeatable() {
    let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
        eprintln!("HEPTA_TEST_DATABASE_URL unset; collaboration PostgreSQL conformance skipped");
        return;
    };
    let mut lock = PgConnection::connect(&database_url)
        .await
        .expect("PostgreSQL collaboration test lock");
    sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("serialize Hepta PostgreSQL tests");
    let state = AppState::connect(&database_url, security())
        .await
        .expect("collaboration PostgreSQL state");
    let pool = state.pool.as_ref().expect("PostgreSQL pool");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0033_add_hepta_paper_collaboration_kernel.sql"
    ))
    .execute(pool)
    .await
    .expect("0033 second application");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0033_add_hepta_paper_collaboration_kernel.sql"
    ))
    .execute(pool)
    .await
    .expect("0033 third application");
    reset_postgres(&database_url).await;
    let postgres = run_collaboration_kernel_flow(state).await;
    let memory = run_collaboration_kernel_flow(AppState::new(security())).await;
    assert_eq!(postgres, memory);
    sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("release Hepta PostgreSQL test lock");
}

#[tokio::test]
async fn postgres_applied_onboarding_replays_after_credentials_expire() {
    let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
        eprintln!("HEPTA_TEST_DATABASE_URL unset; secure onboarding PostgreSQL replay skipped");
        return;
    };
    let mut lock = PgConnection::connect(&database_url)
        .await
        .expect("PostgreSQL onboarding replay test lock");
    sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("serialize Hepta PostgreSQL tests");
    let state = AppState::connect(&database_url, security())
        .await
        .expect("secure onboarding PostgreSQL state");
    reset_postgres(&database_url).await;
    exercise_expired_onboarding_replay(state.clone()).await;
    reset_postgres(&database_url).await;
    exercise_real_onboarding_expiry_replay(state).await;
    reset_postgres(&database_url).await;
    let rotation_state = AppState::connect(&database_url, security())
        .await
        .expect("secure Agent rotation PostgreSQL state");
    exercise_agent_binding_rotation_security(rotation_state).await;
    reset_postgres(&database_url).await;
    sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("release Hepta PostgreSQL onboarding replay lock");
}

#[tokio::test]
async fn postgres_review_flow_matches_memory_and_migration_is_repeatable() {
    let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
        eprintln!("HEPTA_TEST_DATABASE_URL unset; P5 PostgreSQL conformance skipped");
        return;
    };
    let mut lock = PgConnection::connect(&database_url)
        .await
        .expect("PostgreSQL P5 test lock");
    sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("serialize Hepta PostgreSQL tests");
    let state = AppState::connect(&database_url, security())
        .await
        .expect("P5 PostgreSQL state");
    let pool = state.pool.as_ref().expect("PostgreSQL pool");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0034_add_hepta_paper_review_appeal.sql"
    ))
    .execute(pool)
    .await
    .expect("0034 second application");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0034_add_hepta_paper_review_appeal.sql"
    ))
    .execute(pool)
    .await
    .expect("0034 third application");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0035_add_hepta_secure_onboarding.sql"
    ))
    .execute(pool)
    .await
    .expect("0035 second application");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0035_add_hepta_secure_onboarding.sql"
    ))
    .execute(pool)
    .await
    .expect("0035 third application");
    reset_postgres(&database_url).await;
    let postgres = run_review_flow(state).await;
    assert_eq!(postgres.contribution_room_event_count, 1);
    assert_eq!(postgres.evaluation_room_event_count, 2);
    assert_eq!(postgres.reproduction_room_event_count, 3);
    assert_eq!(postgres.appeal_room_event_count, 1);
    assert_eq!(postgres.resolution_room_event_count, 1);
    assert_eq!(postgres.appeal_outbox_event_count, 1);
    assert_eq!(postgres.resolution_outbox_event_count, 1);
    assert_eq!(postgres.appeal_room_settlement_state, "challenged");
    assert_eq!(postgres.resolution_room_settlement_state, "resolved");
    let memory = run_review_flow(AppState::new(security())).await;
    assert_eq!(postgres, memory);
    sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("release Hepta PostgreSQL test lock");
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
        four_state.clone(),
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
    let epoch_rows = sqlx::query(
        "select roster_version, record_json
         from hepta_research_session_authorization_sets order by roster_version",
    )
    .fetch_all(four_state.pool.as_ref().expect("four-author pool"))
    .await
    .expect("read immutable authorization epochs");
    assert_eq!(epoch_rows.len(), 2);
    let old_epoch: Value = epoch_rows[0].get("record_json");
    let new_epoch: Value = epoch_rows[1].get("record_json");
    assert_eq!(epoch_rows[0].get::<i64, _>("roster_version"), 1);
    assert_eq!(epoch_rows[1].get::<i64, _>("roster_version"), 2);
    assert_eq!(
        old_epoch["members"][0]["authorization"]["claim"]["agent_key_id"],
        sha256_digest(&actors(4)[0].agent_key.verifying_key().to_bytes())
    );
    assert_ne!(
        old_epoch["members"][0]["authorization"]["claim"]["agent_key_id"],
        new_epoch["members"][0]["authorization"]["claim"]["agent_key_id"]
    );
    let rotation_history: i64 = sqlx::query_scalar(
        "select count(*) from hepta_agent_binding_key_rotations where binding_id=$1",
    )
    .bind(actors(4)[0].binding_id)
    .fetch_one(four_state.pool.as_ref().expect("four-author pool"))
    .await
    .expect("read Agent binding rotation history");
    assert_eq!(rotation_history, 1);

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
