use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use sqlx::{Connection, PgConnection, Postgres, Row, Transaction};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
use tower::ServiceExt;
use uuid::Uuid;

use super::*;
use crate::{
    agent_proposal_v2_record_parity_definition_is_exact, app,
    paper_chain_finality_v1::{
        paper_finality_side_effect_snapshot, paper_trnm_submission_commitment_hash,
        PaperTrnmCommandBindingV1, PAPER_TRNM_COMMAND_BINDING_SCHEMA_V1,
    },
    paper_chain_finality_v2::{
        binding_commitment_id_v2, paper_scientific_finality_policy_hash_v1, validate_binding_v2,
        PaperTrnmAppealStatusV2, PaperTrnmChainTimeCheckpointV1, PaperTrnmFinalityPreparationV2,
        PaperTrnmFinalityWindowArmV2, PAPER_CHAIN_TIME_MAX_LAG_MS_V1,
    },
    paper_raid_contracts::{
        agent_binding_key_rotation_signing_bytes, agent_binding_proof_signing_bytes,
        agent_binding_proof_v3_signing_bytes, agent_capability_disclosure_hash,
        agent_proposal_v2_signing_bytes, canonical_json_bytes, canonical_json_sha256,
        human_decision_signing_bytes, human_evidence_verification_signing_bytes,
        human_key_registration_signing_bytes, human_key_revocation_signing_bytes,
        human_key_rotation_signing_bytes, paper_appeal_resolution_signing_bytes,
        paper_appeal_signing_bytes, paper_evaluation_signing_bytes, paper_release_candidate_hash,
        paper_reproduction_signing_bytes, paper_review_attestation_signing_bytes,
        paper_rework_signing_bytes, research_session_archive_hash, research_session_commitment_id,
        research_session_completion_signing_bytes, research_session_event_hash,
        research_session_event_id, research_session_event_root,
        research_session_terminal_facts_frame, section_merge_signing_bytes,
        section_review_signing_bytes, sha256_digest, sign_authorship_consent,
        sign_consumer_user_assertion, team_member_acceptance_signing_bytes,
        AgentBindingKeyRotationClaimV2, AgentBindingProofClaimV2, AgentBindingProofClaimV3,
        AgentCapabilityDisclosureAssuranceV1, AgentCapabilityDisclosureV1, AgentCapabilityV1,
        AgentProposalSigningV2, AgentResourceClassV1, AuthorshipConsentSigningV2,
        ConsumerUserAssertionClaimV2, HumanDecisionSigningV1, HumanEvidenceVerificationSigningV1,
        HumanKeyRegistrationClaimV2, HumanKeyRevocationClaimV2, HumanKeyRotationClaimV2,
        PaperAppealResolutionSigningV1, PaperAppealSigningV1, PaperEvaluationSigningV1,
        PaperReproductionSigningV1, PaperReviewAttestationSigningV1, PaperReworkSigningV1,
        ResearchSessionCompletionV1, ResearchSessionEventV1, ResearchSessionTerminalFactsV1,
        SectionMergeSigningV1, SectionReviewSigningV1, TeamMemberAcceptanceSigningV2,
        AGENT_BINDING_KEY_ROTATION_V2, AGENT_BINDING_PROOF_V2, AGENT_BINDING_PROOF_V3,
        AGENT_CAPABILITY_DISCLOSURE_V1, AGENT_PROPOSAL_V2, AUTHORSHIP_CONSENT_V2,
        CONSUMER_USER_ASSERTION_V2, HUMAN_DECISION_V1, HUMAN_EVIDENCE_VERIFICATION_V1,
        HUMAN_KEY_REGISTRATION_V2, HUMAN_KEY_REVOCATION_V2, HUMAN_KEY_ROTATION_V2,
        JSON_SAFE_U64_MAX, PAPER_APPEAL_RESOLUTION_V1, PAPER_APPEAL_V1, PAPER_EVALUATION_V1,
        PAPER_REPRODUCTION_V1, PAPER_REVIEW_ATTESTATION_V1, PAPER_REWORK_V1,
        RESEARCH_SESSION_COMPLETION_V1, RESEARCH_SESSION_EVENT_V1, SECTION_MATERIALIZATION_V1,
        SECTION_MERGE_V1, SECTION_REVIEW_V1, TEAM_MEMBER_ACCEPTANCE_V2,
    },
    verify_agent_proposal_v2_migration_catalog, verify_contribution_ledger_authority_catalog,
    verify_work_item_record_parity_catalog, AppState, ChallengeRulesetV1, SecurityConfig,
    AGENT_PROPOSAL_V2_RECORD_PARITY_CLAUSES, NAKAMA_TOKEN_HEADER, OPERATOR_TOKEN_HEADER,
    TRNM_TOKEN_HEADER, USER_ASSERTION_HEADER,
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

#[test]
fn agent_proposal_v2_record_parity_catalog_accepts_postgres_deparse_only_exactly() {
    let direct = format!(
        "CHECK ({})",
        AGENT_PROPOSAL_V2_RECORD_PARITY_CLAUSES
            .iter()
            .map(|(left, right)| format!("(({left}) IS NOT DISTINCT FROM ({right}))"))
            .collect::<Vec<_>>()
            .join(" AND ")
    );
    let postgres_deparsed = format!(
        "CHECK ((({})))",
        AGENT_PROPOSAL_V2_RECORD_PARITY_CLAUSES
            .iter()
            .map(|(left, right)| format!("(NOT (({left}) IS DISTINCT FROM ({right})))"))
            .collect::<Vec<_>>()
            .join(" AND ")
    );
    assert!(agent_proposal_v2_record_parity_definition_is_exact(&direct));
    assert!(agent_proposal_v2_record_parity_definition_is_exact(
        &postgres_deparsed
    ));

    let missing_signed_at = format!(
        "CHECK ({})",
        AGENT_PROPOSAL_V2_RECORD_PARITY_CLAUSES[..19]
            .iter()
            .map(|(left, right)| format!("NOT (({left}) IS DISTINCT FROM ({right}))"))
            .collect::<Vec<_>>()
            .join(" AND ")
    );
    let duplicate_field = postgres_deparsed.replacen(
        " AND ",
        &format!(
            " AND NOT (({}) IS DISTINCT FROM ({})) AND ",
            AGENT_PROPOSAL_V2_RECORD_PARITY_CLAUSES[0].0,
            AGENT_PROPOSAL_V2_RECORD_PARITY_CLAUSES[0].1,
        ),
        1,
    );
    let wrong_relational_operand = postgres_deparsed.replacen(
        "IS DISTINCT FROM (proposal_id)",
        "IS DISTINCT FROM (paper_project_id)",
        1,
    );
    let or_true = format!("{postgres_deparsed} OR TRUE");
    for hostile in [
        missing_signed_at,
        duplicate_field,
        wrong_relational_operand,
        or_true,
    ] {
        assert!(
            !agent_proposal_v2_record_parity_definition_is_exact(&hostile),
            "Agent proposal parity catalog must reject hostile definition: {hostile}"
        );
    }
}

#[test]
fn author_phase_transitions_are_server_gated_by_scientific_milestones() {
    let now = Utc::now();
    let mut paper = PaperProject {
        paper_project_id: Uuid::new_v4(),
        team_id: Uuid::new_v4(),
        challenge_id: Uuid::new_v4(),
        title: "Gate fixture".into(),
        target_format: "paper".into(),
        phase: PaperPhase::Preregistering,
        challenge_ruleset_snapshot: None,
        challenge_ruleset_snapshot_hash: None,
        deadline_at: None,
        grace_expires_at: None,
        active_rework_id: None,
        active_rework_cycle: None,
        rework_expires_at: None,
        outcome: PaperChallengeOutcomeV1::InProgress,
        outcome_reason: None,
        terminal_at: None,
        role_resources: None,
        current_revision_id: None,
        release_candidate_revision_id: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let empty = AuthorPhaseGateFacts::default();
    assert_eq!(
        author_phase_gate_blockers(&paper, PaperPhase::Researching, empty).expect("legacy gates"),
        vec![
            "work_item_required",
            "artifact_manifest_required",
            "experiment_plan_required"
        ]
    );

    let ready_preregistration = AuthorPhaseGateFacts {
        work_item: true,
        collaboration: collaboration_v3::CollaborationPhaseGateFacts {
            artifact_manifest: true,
            experiment_plan: true,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(
        author_phase_gate_blockers(&paper, PaperPhase::Researching, ready_preregistration)
            .expect("legacy gates")
            .is_empty()
    );

    paper.phase = PaperPhase::Drafting;
    paper.current_revision_id = Some(Uuid::new_v4());
    let drafting = AuthorPhaseGateFacts {
        work_item: true,
        all_work_items_terminal: false,
        paper_revision: true,
        paper_revision_covers_section_merges: false,
        collaboration: collaboration_v3::CollaborationPhaseGateFacts {
            section_revision: true,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        author_phase_gate_blockers(&paper, PaperPhase::IntegrityReview, drafting)
            .expect("legacy gates"),
        vec!["work_items_must_be_accepted_or_cancelled"]
    );

    paper.phase = PaperPhase::Reproducing;
    let stale_merged_revision = AuthorPhaseGateFacts {
        paper_revision: true,
        paper_revision_covers_section_merges: false,
        ..Default::default()
    };
    assert_eq!(
        author_phase_gate_blockers(&paper, PaperPhase::AuthorApproval, stale_merged_revision)
            .expect("legacy gates"),
        vec!["paper_revision_section_lineage_required"]
    );
    let merged_revision = AuthorPhaseGateFacts {
        paper_revision_covers_section_merges: true,
        ..stale_merged_revision
    };
    assert!(
        author_phase_gate_blockers(&paper, PaperPhase::AuthorApproval, merged_revision)
            .expect("legacy gates")
            .is_empty()
    );
}

fn requested_team_member(slot: u32, role: &str) -> CreateTeamMemberRequest {
    CreateTeamMemberRequest {
        participant_slot: slot,
        player_id: Uuid::new_v4(),
        binding_id: Uuid::new_v4(),
        role: role.to_string(),
    }
}

#[test]
fn new_team_rosters_fail_closed_without_the_canonical_role_contract() {
    let canonical = vec![
        requested_team_member(1, "captain"),
        requested_team_member(2, "evidence"),
        requested_team_member(3, "experiment"),
        requested_team_member(4, "support"),
    ];
    validate_team_members(&canonical).expect("canonical roster with support");

    let unknown = vec![
        requested_team_member(1, "captain"),
        requested_team_member(2, "evidence"),
        requested_team_member(3, "anything-goes"),
    ];
    assert_eq!(
        validate_team_members(&unknown)
            .expect_err("unknown role must fail")
            .code,
        "invalid_team_role_contract"
    );

    let duplicate = vec![
        requested_team_member(1, "captain"),
        requested_team_member(2, "captain"),
        requested_team_member(3, "experiment"),
    ];
    assert_eq!(
        validate_team_members(&duplicate)
            .expect_err("missing evidence seat must fail")
            .code,
        "invalid_team_role_contract"
    );

    let now = Utc::now();
    let legacy = ResearchTeam {
        team_id: Uuid::new_v4(),
        challenge_id: Uuid::new_v4(),
        collaboration_compact_hash: digest("legacy-role-contract"),
        status: TeamStatus::Locked,
        roster_version: 1,
        members: unknown
            .iter()
            .map(|member| TeamMember {
                participant_slot: member.participant_slot,
                player_id: member.player_id,
                binding_id: member.binding_id,
                agent_id: format!("agent-{}", member.participant_slot),
                role: member.role.clone(),
                joined_at: now,
            })
            .collect(),
        version: 1,
        created_at: now,
        updated_at: now,
    };
    assert_eq!(
        require_author_role(
            &legacy,
            legacy.members[0].player_id,
            "captain",
            "phase transition"
        )
        .expect_err("legacy noncanonical roster must not regain permissive mutation")
        .code,
        "team_role_contract_required"
    );
}

#[tokio::test]
async fn create_team_http_boundary_rejects_noncanonical_roles() {
    let state = AppState::new(security());
    let router = app(state);
    let actors = actors(3);
    let challenge_id = register_prerequisites(&router, &actors).await;
    create_players_and_bindings(&router, &actors).await;
    let key = "reject-noncanonical-team";
    let response = user_post(
        &router,
        &actors[0],
        "create_research_team_v2",
        "/v2/hepta/teams",
        key,
        json!({
            "team_id":Uuid::new_v4(),
            "challenge_id":challenge_id,
            "collaboration_compact_hash":digest("noncanonical-team"),
            "members":actors.iter().enumerate().map(|(index, actor)| json!({
                "participant_slot":index + 1,
                "player_id":actor.player_id,
                "binding_id":actor.binding_id,
                "role":(["foo", "bar", "baz"][index]),
            })).collect::<Vec<_>>(),
            "idempotency_key":key,
        }),
    )
    .await;
    assert_eq!(
        error_code(response, StatusCode::BAD_REQUEST),
        "invalid_team_role_contract"
    );
}

fn authoritative_benchmark_ruleset_json() -> Value {
    json!({
        "schema":"hepta.challenge.ruleset.v1",
        "template":"benchmark-ablation",
        "duration_seconds":5400,
        "grace_seconds":900,
        "phase_gates":[
            {"transition":"preregistering_to_researching","requirements":[
                {"kind":"work_items","minimum":1},
                {"kind":"artifact_manifests","minimum":1}
            ]},
            {"transition":"researching_to_experimenting","requirements":[
                {"kind":"evidence_cards","minimum":1},
                {"kind":"citations","minimum":1},
                {"kind":"claims","minimum":1}
            ]},
            {"transition":"experimenting_to_drafting","requirements":[
                {"kind":"artifact_manifests","minimum":1},
                {"kind":"experiment_plans","minimum":1},
                {"kind":"successful_runs","minimum":1},
                {"kind":"retained_failed_runs","minimum":1}
            ]},
            {"transition":"drafting_to_integrity_review","requirements":[
                {"kind":"section_revisions","minimum":1},
                {"kind":"paper_revisions","minimum":1},
                {"kind":"all_work_items_terminal","minimum":1}
            ]},
            {"transition":"integrity_review_to_reproducing","requirements":[
                {"kind":"approving_section_reviews","minimum":1},
                {"kind":"section_merges","minimum":1}
            ]},
            {"transition":"reproducing_to_author_approval","requirements":[
                {"kind":"paper_revision_covers_section_merges","minimum":1}
            ]}
        ],
        "victory_requirements":[
            {"kind":"accepted_work_items","minimum":1},
            {"kind":"retained_failed_runs","minimum":1},
            {"kind":"release_candidate","minimum":1},
            {"kind":"all_author_consents","minimum":1},
            {"kind":"paper_revision_covers_section_merges","minimum":1}
        ]
    })
}

#[tokio::test]
async fn challenge_http_boundary_recomputes_typed_ruleset_hash() {
    let router = app(AppState::new(security()));
    let ruleset = authoritative_benchmark_ruleset_json();
    let ruleset_hash = canonical_json_sha256(&ruleset).expect("canonical ruleset hash");
    let body = json!({
        "title":"Authoritative benchmark",
        "description":"Typed immutable benchmark rules",
        "ruleset_version":"hepta.challenge.ruleset.v1",
        "ruleset_hash":ruleset_hash,
        "dataset_manifest_hash":digest("authoritative-dataset"),
        "evaluator_manifest_hash":digest("authoritative-evaluator"),
        "ruleset":ruleset,
        "status":"open"
    });
    let (status, created) =
        request(&router, "POST", "/v1/hepta/challenges", body.clone(), None).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["ruleset_hash"], body["ruleset_hash"]);
    assert_eq!(created["ruleset"]["template"], "benchmark-ablation");

    let mut mismatched = body;
    mismatched["ruleset_hash"] = Value::String(digest("not-the-ruleset"));
    let response = request(&router, "POST", "/v1/hepta/challenges", mismatched, None).await;
    assert_eq!(
        error_code(response, StatusCode::BAD_REQUEST),
        "challenge_ruleset_hash_mismatch"
    );
}

#[test]
fn paper_creation_snapshot_freezes_typed_rules_and_deadlines() {
    let ruleset: ChallengeRulesetV1 =
        serde_json::from_value(authoritative_benchmark_ruleset_json()).expect("typed ruleset");
    let now = Utc::now();
    let challenge = ResearchChallenge {
        challenge_id: Uuid::new_v4(),
        title: "Snapshot fixture".into(),
        description: "Immutable gameplay rules".into(),
        ruleset_version: "paper-raid-benchmark-ablation-v1".into(),
        ruleset_hash: ruleset.canonical_hash().expect("ruleset hash"),
        dataset_manifest_hash: digest("snapshot-dataset"),
        evaluator_manifest_hash: digest("snapshot-evaluator"),
        ruleset: Some(ruleset.clone()),
        status: crate::ChallengeStatus::Open,
        created_at: now - chrono::Duration::minutes(5),
    };
    let (snapshot, snapshot_hash, deadline_at, grace_expires_at) =
        snapshot_challenge_ruleset(&challenge, now).expect("typed challenge snapshot");
    assert_eq!(
        snapshot.enforcement,
        ChallengeRulesetEnforcementV1::AuthoritativeV1
    );
    assert_eq!(snapshot.ruleset, Some(ruleset));
    assert_eq!(deadline_at, Some(now + chrono::Duration::seconds(5_400)));
    assert_eq!(
        grace_expires_at,
        Some(now + chrono::Duration::seconds(6_300))
    );
    assert_eq!(
        snapshot.canonical_hash().expect("snapshot hash"),
        snapshot_hash
    );

    let mut legacy = challenge;
    legacy.ruleset = None;
    legacy.ruleset_hash = digest("legacy-snapshot-rules");
    let (legacy_snapshot, _, legacy_deadline, legacy_grace) =
        snapshot_challenge_ruleset(&legacy, now).expect("legacy challenge snapshot");
    assert_eq!(
        legacy_snapshot.enforcement,
        ChallengeRulesetEnforcementV1::LegacyUnranked
    );
    assert!(legacy_snapshot.ruleset.is_none());
    assert!(legacy_deadline.is_none() && legacy_grace.is_none());
}

#[test]
fn typed_challenge_snapshot_overrides_legacy_phase_gate_and_deadline_is_fail_closed() {
    let ruleset: ChallengeRulesetV1 =
        serde_json::from_value(authoritative_benchmark_ruleset_json()).expect("typed ruleset");
    let ruleset_hash = ruleset.canonical_hash().expect("ruleset hash");
    let snapshot = PaperChallengeRulesetSnapshotV1 {
        schema: CHALLENGE_RULESET_SNAPSHOT_V1.to_string(),
        challenge_snapshot_hash: digest("typed-challenge-snapshot"),
        ruleset_version: "paper-raid-benchmark-ablation-v1".to_string(),
        ruleset_hash,
        enforcement: ChallengeRulesetEnforcementV1::AuthoritativeV1,
        ruleset: Some(ruleset),
    };
    let now = Utc::now();
    let mut paper = PaperProject {
        paper_project_id: Uuid::new_v4(),
        team_id: Uuid::new_v4(),
        challenge_id: Uuid::new_v4(),
        title: "Typed benchmark".into(),
        target_format: "paper".into(),
        phase: PaperPhase::Experimenting,
        challenge_ruleset_snapshot_hash: Some(snapshot.canonical_hash().expect("snapshot hash")),
        challenge_ruleset_snapshot: Some(snapshot),
        deadline_at: Some(now + chrono::Duration::minutes(90)),
        grace_expires_at: Some(now + chrono::Duration::minutes(105)),
        active_rework_id: None,
        active_rework_cycle: None,
        rework_expires_at: None,
        outcome: PaperChallengeOutcomeV1::InProgress,
        outcome_reason: None,
        terminal_at: None,
        role_resources: None,
        current_revision_id: None,
        release_candidate_revision_id: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let mut facts = AuthorPhaseGateFacts {
        collaboration_counts: collaboration_v3::CollaborationPhaseGateCounts {
            artifact_manifests: 1,
            experiment_plans: 1,
            successful_runs: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        author_phase_gate_blockers(&paper, PaperPhase::Drafting, facts).expect("typed gate"),
        vec!["minimum_retained_failed_runs_required:1:0"]
    );
    facts.collaboration_counts.retained_failed_runs = 1;
    assert!(
        author_phase_gate_blockers(&paper, PaperPhase::Drafting, facts)
            .expect("typed gate")
            .is_empty()
    );

    paper.grace_expires_at = Some(now);
    assert_eq!(
        ensure_paper_gameplay_active(&paper, now)
            .expect_err("mutation at the half-open grace boundary must fail")
            .code,
        "paper_challenge_deadline_elapsed"
    );
    validate_requested_terminal_outcome(&paper, PaperChallengeOutcomeV1::Expired, now)
        .expect("expired may be recorded at the grace boundary");
    assert_eq!(
        validate_requested_terminal_outcome(&paper, PaperChallengeOutcomeV1::Abandoned, now)
            .expect_err("deadline outcome cannot be disguised as abandonment")
            .code,
        "paper_challenge_deadline_elapsed"
    );
    assert_eq!(
        validate_requested_terminal_outcome(&paper, PaperChallengeOutcomeV1::SubmissionReady, now)
            .expect_err("submission_ready is not a manual outcome")
            .code,
        "invalid_paper_challenge_outcome"
    );
}

#[derive(Debug, Clone, Copy, Default)]
struct FlowOptions {
    rotate_human_after_acceptance: bool,
    replace_session_epoch: bool,
    revoke_author_before_finalize: bool,
    concurrent_finalize: bool,
    replay_0047_after_reservation: bool,
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
    for object in bundle["objects"].as_array_mut().expect("artifact objects") {
        let review_role = match object["logical_path"].as_str().expect("logical path") {
            "code/baseline.py" => Some("frozen_evaluator"),
            "data/synthetic-observations.csv" => Some("dataset"),
            "experiments/runs/baseline-seed-17/metrics.json" => Some("candidate"),
            _ => None,
        };
        if let Some(review_role) = review_role {
            object["role"] = json!(review_role);
        }
    }
    let frozen_roles = bundle["objects"]
        .as_array()
        .expect("artifact objects")
        .iter()
        .filter_map(|object| object["role"].as_str())
        .fold([0_usize; 3], |mut counts, role| {
            match role {
                "frozen_evaluator" => counts[0] += 1,
                "dataset" => counts[1] += 1,
                "candidate" => counts[2] += 1,
                _ => {}
            }
            counts
        });
    assert_eq!(
        frozen_roles,
        [1, 1, 1],
        "fixture must expose one frozen evaluator, one dataset, and one candidate"
    );
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
                "acl": "reviewers",
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
    let manifest_id = Uuid::new_v4();
    let key = format!("artifact-{paper_id}-{suffix}");
    let path = format!("/v2/hepta/papers/{paper_id}/artifact-manifests");
    let (full_bundle, _, _) = integration_artifact_bundle(challenge_id, suffix);
    let source_specs = [
        (
            "draft",
            vec!["paper_source", "bibliography", "claim_evidence_graph"],
        ),
        ("frozen-evaluator", vec!["frozen_evaluator"]),
        ("dataset", vec!["dataset"]),
        ("candidate", vec!["candidate"]),
    ];
    let mut registered_sources = Vec::new();
    for (source_name, roles) in source_specs {
        let mut objects = full_bundle["objects"]
            .as_array()
            .expect("full fixture objects")
            .iter()
            .filter(|object| {
                object["role"]
                    .as_str()
                    .is_some_and(|role| roles.contains(&role))
            })
            .cloned()
            .collect::<Vec<_>>();
        let source_paths = objects
            .iter()
            .map(|object| {
                object["logical_path"]
                    .as_str()
                    .expect("source logical path")
                    .to_string()
            })
            .collect::<Vec<_>>();
        for object in &mut objects {
            object["dependencies"]
                .as_array_mut()
                .expect("source dependencies")
                .retain(|dependency| {
                    dependency.as_str().is_some_and(|dependency| {
                        source_paths.iter().any(|path| path == dependency)
                    })
                });
        }
        let expected_count = if source_name == "draft" { 3 } else { 1 };
        assert_eq!(objects.len(), expected_count, "exact source role coverage");
        let source_manifest_id = Uuid::new_v4();
        let source_bundle = json!({
            "artifact_root": full_bundle["artifact_root"],
            "bundle_id": format!("review-source-{source_name}-{source_manifest_id}"),
            "challenge_id": challenge_id,
            "created_at": full_bundle["created_at"],
            "hepta_binding_status": "unbound",
            "human_authority_materialized": false,
            "object_count": objects.len(),
            "objects": objects,
            "required_run_ids": full_bundle["required_run_ids"],
            "schema": "paper-raid.artifact-bundle.v1",
        });
        let mut canonical = canonical_json_bytes(&source_bundle).expect("source bundle canonical");
        canonical.push(b'\n');
        let source_sha256 = sha256_digest(&canonical)
            .strip_prefix("sha256:")
            .expect("source hash prefix")
            .to_string();
        let source_locations = source_bundle["objects"]
            .as_array()
            .expect("source objects")
            .iter()
            .map(|object| {
                let sha256 = object["sha256"].as_str().expect("source digest");
                json!({
                    "logical_path": object["logical_path"],
                    "sha256": sha256,
                    "uri": format!("cas://sha256/{sha256}"),
                    "acl": "team",
                })
            })
            .collect::<Vec<_>>();
        let source_key = format!("{key}-source-{source_name}");
        let source_manifest = assert_status(
            user_post(
                router,
                actor,
                "create_artifact_manifest_v3",
                &path,
                &source_key,
                json!({
                    "manifest_id": source_manifest_id,
                    "expected_paper_version": expected_paper_version,
                    "expected_source_manifest_sha256": source_sha256,
                    "source_bundle": source_bundle,
                    "storage_locations": source_locations,
                    "idempotency_key": source_key,
                }),
            )
            .await,
            StatusCode::CREATED,
        );
        registered_sources.push((source_name, source_manifest));
    }
    let mut objects = registered_sources
        .iter()
        .flat_map(|(_, manifest)| {
            manifest["objects"]
                .as_array()
                .expect("registered source objects")
                .iter()
                .cloned()
        })
        .collect::<Vec<_>>();
    objects.sort_by(|left, right| {
        left["logical_path"]
            .as_str()
            .cmp(&right["logical_path"].as_str())
    });
    let mut required_run_ids = full_bundle["required_run_ids"]
        .as_array()
        .expect("required run IDs")
        .iter()
        .map(|run_id| run_id.as_str().expect("required run ID").to_string())
        .collect::<Vec<_>>();
    required_run_ids.sort();
    let source_bundle = json!({
        "artifact_root": full_bundle["artifact_root"],
        "bundle_id": format!("review-ready-{manifest_id}"),
        "challenge_id": challenge_id,
        "created_at": full_bundle["created_at"],
        "hepta_binding_status": "unbound",
        "human_authority_materialized": false,
        "object_count": objects.len(),
        "objects": objects,
        "required_run_ids": required_run_ids,
        "schema": "paper-raid.artifact-bundle.v1",
    });
    let mut canonical = canonical_json_bytes(&source_bundle).expect("Review bundle canonical");
    canonical.push(b'\n');
    let expected_source_manifest_sha256 = sha256_digest(&canonical)
        .strip_prefix("sha256:")
        .expect("Review bundle hash prefix")
        .to_string();
    let storage_locations = source_bundle["objects"]
        .as_array()
        .expect("Review bundle objects")
        .iter()
        .map(|object| {
            let sha256 = object["sha256"].as_str().expect("Review object digest");
            json!({
                "logical_path": object["logical_path"],
                "sha256": sha256,
                "uri": format!("cas://sha256/{sha256}"),
                "acl": "reviewers",
            })
        })
        .collect::<Vec<_>>();
    let pin = |name: &str| {
        let manifest = &registered_sources
            .iter()
            .find(|(source_name, _)| *source_name == name)
            .expect("registered Review source")
            .1;
        json!({
            "manifest_id": manifest["manifest_id"],
            "manifest_hash": manifest["manifest_hash"],
        })
    };
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
                "review_ready_assembly": {
                    "schema": "hepta.paper_raid.review_ready_artifact_assembly.v1",
                    "draft": pin("draft"),
                    "frozen_evaluator": pin("frozen-evaluator"),
                    "dataset": pin("dataset"),
                    "candidate": pin("candidate"),
                },
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

struct AuthorDraftingGateSeed {
    artifact: VerifiedArtifactHashes,
    work_item_id: Uuid,
}

struct ContributionGateFixture {
    accepted_artifact_manifest_id: Uuid,
    accepted_by_player_id: Uuid,
    accepted_section_review_id: Uuid,
    reviewed_by_player_id: Uuid,
}

async fn create_matchmaking_ticket_for(
    router: &Router,
    actor: &Actor,
    challenge_id: Uuid,
    requested_team_size: u32,
) -> (StatusCode, Value) {
    let availability_hash = digest("shared-alpha-availability-window");
    create_matchmaking_ticket_for_preferences(
        router,
        actor,
        challenge_id,
        requested_team_size,
        &[actor.role.as_str()],
        &availability_hash,
        None,
    )
    .await
}

async fn create_matchmaking_ticket_for_preferences(
    router: &Router,
    actor: &Actor,
    challenge_id: Uuid,
    requested_team_size: u32,
    roles: &[&str],
    availability_hash: &str,
    party_code_hash: Option<&str>,
) -> (StatusCode, Value) {
    let ticket_id = Uuid::new_v4();
    let key = format!("p3-ticket-{ticket_id}");
    let mut body = json!({
        "ticket_id":ticket_id,
        "challenge_id":challenge_id,
        "requested_team_size":requested_team_size,
        "roles":roles,
        "availability_hash":availability_hash,
        "idempotency_key":key,
    });
    if let Some(party_code_hash) = party_code_hash {
        body["party_code_hash"] = json!(party_code_hash);
    }
    user_post(
        router,
        actor,
        "create_matchmaking_ticket_v3",
        "/v2/hepta/matchmaking/tickets",
        &key,
        body,
    )
    .await
}

async fn cancel_matchmaking_ticket_for(
    router: &Router,
    actor: &Actor,
    ticket_id: Uuid,
    expected_version: u64,
    suffix: &str,
) -> (StatusCode, Value) {
    let path = format!("/v2/hepta/matchmaking/tickets/{ticket_id}/cancel");
    let key = format!("p3-ticket-cancel-{ticket_id}-{suffix}");
    user_post(
        router,
        actor,
        "cancel_matchmaking_ticket_v3",
        &path,
        &key,
        json!({
            "expected_version":expected_version,
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

async fn materialize_team_proposal_for(
    router: &Router,
    actor: &Actor,
    proposal_id: Uuid,
    expected_proposal_version: u64,
    suffix: &str,
) -> (StatusCode, Value) {
    let path = format!("/v2/hepta/team-proposals/{proposal_id}/materialize");
    let key = format!("p3-team-materialize-{proposal_id}-{suffix}");
    user_post(
        router,
        actor,
        "materialize_team_proposal_v1",
        &path,
        &key,
        json!({
            "expected_proposal_version":expected_proposal_version,
            "idempotency_key":key,
        }),
    )
    .await
}

async fn exercise_private_party_matchmaking(router: &Router, actors: &[Actor], challenge_id: Uuid) {
    let party_hash = digest("strict-private-party-affinity");
    let availability = digest("strict-private-party-window");
    let other_availability = digest("strict-private-party-other-window");

    let captain = assert_status(
        create_matchmaking_ticket_for_preferences(
            router,
            &actors[0],
            challenge_id,
            3,
            &["captain"],
            &availability,
            Some(&party_hash),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(captain["ticket"]["private_party"], true);
    assert!(captain["ticket"].get("party_code_hash").is_none());
    let captain_ticket = Uuid::parse_str(captain["ticket"]["ticket_id"].as_str().unwrap()).unwrap();

    assert_eq!(
        error_code(
            create_matchmaking_ticket_for_preferences(
                router,
                &actors[1],
                challenge_id,
                3,
                &["captain"],
                &availability,
                Some(&party_hash),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "party_role_conflict"
    );
    let evidence = assert_status(
        create_matchmaking_ticket_for_preferences(
            router,
            &actors[1],
            challenge_id,
            3,
            &["evidence"],
            &availability,
            Some(&party_hash),
        )
        .await,
        StatusCode::CREATED,
    );
    let evidence_ticket =
        Uuid::parse_str(evidence["ticket"]["ticket_id"].as_str().unwrap()).unwrap();

    assert_eq!(
        error_code(
            create_matchmaking_ticket_for_preferences(
                router,
                &actors[2],
                challenge_id,
                3,
                &["experiment"],
                &other_availability,
                Some(&party_hash),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "party_availability_conflict"
    );
    let experiment = assert_status(
        create_matchmaking_ticket_for_preferences(
            router,
            &actors[2],
            challenge_id,
            3,
            &["experiment"],
            &availability,
            Some(&party_hash),
        )
        .await,
        StatusCode::CREATED,
    );
    let experiment_ticket =
        Uuid::parse_str(experiment["ticket"]["ticket_id"].as_str().unwrap()).unwrap();
    let proposal_id = Uuid::parse_str(
        experiment["team_proposal"]["proposal_id"]
            .as_str()
            .expect("private proposal id"),
    )
    .unwrap();

    assert_eq!(
        error_code(
            create_matchmaking_ticket_for_preferences(
                router,
                &actors[3],
                challenge_id,
                3,
                &["captain"],
                &availability,
                Some(&party_hash),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "party_queue_full"
    );

    let declined = assert_status(
        decide_team_proposal(router, &actors[0], proposal_id, 1, "decline", "party").await,
        StatusCode::CREATED,
    );
    assert_eq!(declined["proposal"]["status"], "declined");
    assert!(declined["replacement_proposal"].is_null());

    let replacement = assert_status(
        create_matchmaking_ticket_for_preferences(
            router,
            &actors[3],
            challenge_id,
            3,
            &["captain"],
            &availability,
            Some(&party_hash),
        )
        .await,
        StatusCode::CREATED,
    );
    let replacement_ticket =
        Uuid::parse_str(replacement["ticket"]["ticket_id"].as_str().unwrap()).unwrap();
    assert!(replacement["team_proposal"].is_object());

    let withdrawn = assert_status(
        cancel_matchmaking_ticket_for(router, &actors[3], replacement_ticket, 2, "withdraw").await,
        StatusCode::OK,
    );
    assert_eq!(withdrawn["status"], "cancelled");
    assert_status(
        cancel_matchmaking_ticket_for(router, &actors[1], evidence_ticket, 5, "evidence").await,
        StatusCode::OK,
    );
    assert_status(
        cancel_matchmaking_ticket_for(router, &actors[2], experiment_ticket, 5, "experiment").await,
        StatusCode::OK,
    );

    let captain_path = format!("/v2/hepta/matchmaking/tickets/{captain_ticket}");
    let captain_read = assert_status(
        user_get(
            router,
            &actors[0],
            "get_matchmaking_ticket_v3",
            &captain_path,
            "private-party-captain-read",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(captain_read["status"], "cancelled");
    assert!(captain_read.get("party_code_hash").is_none());

    // The cancelled players can deliberately correct/change their role
    // preferences, rejoin the same private party, accept it, and materialize
    // the exact party-bound proposal without falling back to a public ticket.
    let changed_evidence = assert_status(
        create_matchmaking_ticket_for_preferences(
            router,
            &actors[0],
            challenge_id,
            3,
            &["evidence"],
            &availability,
            Some(&party_hash),
        )
        .await,
        StatusCode::CREATED,
    );
    assert!(changed_evidence["team_proposal"].is_null());
    let changed_experiment = assert_status(
        create_matchmaking_ticket_for_preferences(
            router,
            &actors[1],
            challenge_id,
            3,
            &["experiment"],
            &availability,
            Some(&party_hash),
        )
        .await,
        StatusCode::CREATED,
    );
    assert!(changed_experiment["team_proposal"].is_null());
    let changed_captain = assert_status(
        create_matchmaking_ticket_for_preferences(
            router,
            &actors[2],
            challenge_id,
            3,
            &["captain"],
            &availability,
            Some(&party_hash),
        )
        .await,
        StatusCode::CREATED,
    );
    let accepted_proposal_id = Uuid::parse_str(
        changed_captain["team_proposal"]["proposal_id"]
            .as_str()
            .expect("changed-role private proposal id"),
    )
    .unwrap();
    assert_status(
        decide_team_proposal(
            router,
            &actors[0],
            accepted_proposal_id,
            1,
            "accept",
            "changed-role-a",
        )
        .await,
        StatusCode::CREATED,
    );
    assert_status(
        decide_team_proposal(
            router,
            &actors[1],
            accepted_proposal_id,
            2,
            "accept",
            "changed-role-b",
        )
        .await,
        StatusCode::CREATED,
    );
    let accepted = assert_status(
        decide_team_proposal(
            router,
            &actors[2],
            accepted_proposal_id,
            3,
            "accept",
            "changed-role-c",
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(accepted["proposal"]["status"], "accepted");
    assert_eq!(accepted["proposal"]["version"], 4);

    let team = assert_status(
        materialize_team_proposal_for(
            router,
            &actors[0],
            accepted_proposal_id,
            4,
            "changed-role-party",
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(team["team_id"], accepted_proposal_id.to_string());
    let members = team["members"].as_array().expect("materialized members");
    assert_eq!(members.len(), 3);
    assert_eq!(members[0]["player_id"], actors[0].player_id.to_string());
    assert_eq!(members[0]["role"], "evidence");
    assert_eq!(members[1]["player_id"], actors[1].player_id.to_string());
    assert_eq!(members[1]["role"], "experiment");
    assert_eq!(members[2]["player_id"], actors[2].player_id.to_string());
    assert_eq!(members[2]["role"], "captain");
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

struct SignedAgentProposalBodyInput<'a> {
    proposal_id: Uuid,
    section_key: &'a str,
    parent_revision_id: Uuid,
    lease_id: Uuid,
    lease_fencing_token: u64,
    expected_work_version: u64,
    payload_hash: &'a str,
    idempotency_key: &'a str,
}

fn signed_agent_proposal_body(
    context: &DraftingPaperContext,
    input: SignedAgentProposalBodyInput<'_>,
) -> Value {
    let SignedAgentProposalBodyInput {
        proposal_id,
        section_key,
        parent_revision_id,
        lease_id,
        lease_fencing_token,
        expected_work_version,
        payload_hash,
        idempotency_key,
    } = input;
    signed_agent_proposal_body_for_actor(
        context,
        &context.actors[0],
        proposal_id,
        section_key,
        parent_revision_id,
        lease_id,
        lease_fencing_token,
        expected_work_version,
        payload_hash,
        idempotency_key,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn signed_agent_proposal_body_for_actor(
    context: &DraftingPaperContext,
    actor: &Actor,
    proposal_id: Uuid,
    section_key: &str,
    parent_revision_id: Uuid,
    lease_id: Uuid,
    lease_fencing_token: u64,
    expected_work_version: u64,
    payload_hash: &str,
    idempotency_key: &str,
    declared_agent_key_id: Option<String>,
) -> Value {
    signed_agent_proposal_body_for_actor_and_artifact(
        context,
        actor,
        &context.artifact,
        proposal_id,
        section_key,
        parent_revision_id,
        lease_id,
        lease_fencing_token,
        expected_work_version,
        payload_hash,
        idempotency_key,
        declared_agent_key_id,
    )
}

#[allow(clippy::too_many_arguments)]
fn signed_agent_proposal_body_for_actor_and_artifact(
    context: &DraftingPaperContext,
    actor: &Actor,
    artifact: &VerifiedArtifactHashes,
    proposal_id: Uuid,
    section_key: &str,
    parent_revision_id: Uuid,
    lease_id: Uuid,
    lease_fencing_token: u64,
    expected_work_version: u64,
    payload_hash: &str,
    idempotency_key: &str,
    declared_agent_key_id: Option<String>,
) -> Value {
    let signed_at_unix = Utc::now().timestamp();
    let agent_key_id = declared_agent_key_id
        .unwrap_or_else(|| sha256_digest(&actor.agent_key.verifying_key().to_bytes()));
    let signing = AgentProposalSigningV2 {
        schema: AGENT_PROPOSAL_V2.to_string(),
        proposal_id,
        paper_project_id: context.paper_id,
        work_item_id: context.work_item_id,
        section_key: section_key.to_string(),
        parent_revision_id,
        lease_id,
        lease_fencing_token,
        expected_work_version,
        proposal_kind: "delivery".to_string(),
        payload_hash: payload_hash.to_string(),
        artifact_manifest_id: artifact.manifest_id,
        artifact_manifest_hash: artifact.artifact_manifest_hash.clone(),
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
        "lease_id":lease_id,
        "lease_fencing_token":lease_fencing_token,
        "expected_work_version":expected_work_version,
        "proposal_kind":"delivery",
        "payload_hash":payload_hash,
        "artifact_manifest_id":artifact.manifest_id,
        "agent_id":actor.agent_id,
        "binding_id":actor.binding_id,
        "agent_key_id":agent_key_id,
        "signed_at_unix":signed_at_unix,
        "signature":BASE64.encode(actor.agent_key.sign(
            &agent_proposal_v2_signing_bytes(&signing).expect("Agent proposal V2 frame")
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

async fn transition_fixture_paper_phase(
    router: &Router,
    actor: &Actor,
    paper_id: Uuid,
    paper_version: &mut u64,
    next_phase: &str,
    suffix: &str,
) {
    let path = format!("/v2/hepta/papers/{paper_id}/transition");
    let key = format!("gate-phase-{paper_id}-{next_phase}-{suffix}");
    let paper = assert_status(
        user_post(
            router,
            actor,
            "transition_paper_project_v2",
            &path,
            &key,
            json!({
                "expected_version":*paper_version,
                "next_phase":next_phase,
                "idempotency_key":key,
            }),
        )
        .await,
        StatusCode::OK,
    );
    *paper_version += 1;
    assert_eq!(paper["version"], *paper_version);
}

#[allow(clippy::too_many_arguments)]
async fn seed_paper_to_drafting_through_scientific_gates(
    router: &Router,
    actors: &[Actor],
    paper_id: Uuid,
    challenge_id: Uuid,
    paper_version: &mut u64,
    work_item_id: Uuid,
    suffix: &str,
) -> AuthorDraftingGateSeed {
    assert!(
        actors.len() >= 3,
        "phase-gate fixture requires three authors"
    );
    let denied_phase_key = format!("gate-role-denied-{paper_id}-{suffix}");
    assert_eq!(
        error_code(
            user_post(
                router,
                &actors[1],
                "transition_paper_project_v2",
                &format!("/v2/hepta/papers/{paper_id}/transition"),
                &denied_phase_key,
                json!({
                    "expected_version":*paper_version,
                    "next_phase":"preregistering",
                    "idempotency_key":denied_phase_key,
                }),
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "author_role_duty_required"
    );
    transition_fixture_paper_phase(
        router,
        &actors[0],
        paper_id,
        paper_version,
        "preregistering",
        suffix,
    )
    .await;

    let artifact = create_verified_artifact_manifest(
        router,
        &actors[0],
        paper_id,
        challenge_id,
        *paper_version,
        &format!("{suffix}-primary"),
    )
    .await;
    let code_manifest = create_verified_artifact_manifest(
        router,
        &actors[0],
        paper_id,
        challenge_id,
        *paper_version,
        &format!("{suffix}-code"),
    )
    .await;
    let dataset_manifest = create_verified_artifact_manifest(
        router,
        &actors[1],
        paper_id,
        challenge_id,
        *paper_version,
        &format!("{suffix}-dataset"),
    )
    .await;
    let environment_manifest = create_verified_artifact_manifest(
        router,
        &actors[2],
        paper_id,
        challenge_id,
        *paper_version,
        &format!("{suffix}-environment"),
    )
    .await;

    let room_path = format!("/v2/hepta/papers/{paper_id}/room");
    let room_before_work = assert_status(
        user_get(
            router,
            &actors[0],
            "get_paper_room_v3",
            &room_path,
            &format!("gate-work-room-before-{paper_id}-{suffix}"),
        )
        .await,
        StatusCode::OK,
    );
    let cursor_before_work = room_before_work["last_event_cursor"]
        .as_u64()
        .expect("Paper Room cursor before work-item creation");
    let work_path = format!("/v2/hepta/papers/{paper_id}/work-items");
    let work_key = format!("gate-work-{paper_id}-{work_item_id}-{suffix}");
    let work_body = json!({
        "work_item_id":work_item_id,
        "expected_paper_version":*paper_version,
        "kind":"paper_section",
        "title":"Produce the gated research and paper section",
        "assigned_player_id":actors[0].player_id,
        "assigned_binding_id":actors[0].binding_id,
        "idempotency_key":work_key,
    });
    let work = assert_status(
        user_post(
            router,
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
    assert_eq!(
        assert_status(
            user_post(
                router,
                &actors[0],
                "create_paper_work_item_v2",
                &work_path,
                &work_key,
                work_body,
            )
            .await,
            StatusCode::CREATED,
        ),
        work,
        "work-item fixture replay must be exact",
    );
    let work_events_path =
        format!("/v2/hepta/papers/{paper_id}/events?after_cursor={cursor_before_work}");
    let work_events = assert_status(
        user_get(
            router,
            &actors[1],
            "list_paper_room_events_v3",
            &work_events_path,
            &format!("gate-work-events-after-{paper_id}-{suffix}"),
        )
        .await,
        StatusCode::OK,
    );
    let matching_work_events = work_events
        .as_array()
        .expect("Paper Room events after work-item creation")
        .iter()
        .filter(|event| {
            event["event_type"] == "hepta.paper_raid.work_item.created.v2"
                && event["aggregate_id"] == paper_id.to_string()
                && event["aggregate_version"] == *paper_version + 1
                && event["payload"]["paper_project_id"] == paper_id.to_string()
                && event["payload"]["work_item_id"] == work_item_id.to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        matching_work_events.len(),
        1,
        "work-item creation and exact replay must publish one live Paper Room event",
    );
    *paper_version += 1;

    let experiment_plan_id = Uuid::new_v4();
    let experiment_key = format!("gate-experiment-plan-{paper_id}-{suffix}");
    assert_status(
        user_post(
            router,
            &actors[2],
            "create_experiment_plan_v3",
            &format!("/v2/hepta/papers/{paper_id}/experiment-plans"),
            &experiment_key,
            json!({
                "experiment_plan_id":experiment_plan_id,
                "protocol_snapshot_hash":digest(&format!("gate-protocol-{paper_id}-{suffix}")),
                "code_manifest_id":code_manifest.manifest_id,
                "dataset_manifest_id":dataset_manifest.manifest_id,
                "environment_manifest_id":environment_manifest.manifest_id,
                "seed_policy_hash":digest(&format!("gate-seed-policy-{paper_id}-{suffix}")),
                "stopping_rule_hash":digest(&format!("gate-stopping-rule-{paper_id}-{suffix}")),
                "idempotency_key":experiment_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    transition_fixture_paper_phase(
        router,
        &actors[0],
        paper_id,
        paper_version,
        "researching",
        suffix,
    )
    .await;

    let evidence_actor = &actors[1];
    let evidence_id = Uuid::new_v4();
    let source_uri = format!("https://example.org/hepta/gate/{paper_id}/{suffix}");
    let source_hash = digest(&format!("gate-evidence-{paper_id}-{suffix}"));
    let locator = "fixture source, lines 1-8";
    let license = "CC-BY-4.0";
    let evidence_time = Utc::now().timestamp();
    let evidence_signature = human_verification_signature(
        evidence_actor,
        paper_id,
        "evidence_card",
        evidence_id,
        &format!("evidence-uri\n{source_uri}"),
        &source_hash,
        locator,
        license,
        evidence_time,
    );
    let evidence_key = format!("gate-evidence-{paper_id}-{suffix}");
    assert_status(
        user_post(
            router,
            evidence_actor,
            "create_evidence_card_v3",
            &format!("/v2/hepta/papers/{paper_id}/evidence-cards"),
            &evidence_key,
            json!({
                "evidence_card_id":evidence_id,
                "source_uri":source_uri,
                "source_hash":source_hash,
                "locator":locator,
                "license":license,
                "verification_key_id":evidence_actor.human_key_id,
                "verification_public_key":evidence_actor.human_public_key,
                "verification_public_key_hash":evidence_actor.human_public_key_hash,
                "signed_at_unix":evidence_time,
                "verification_signature":evidence_signature,
                "idempotency_key":evidence_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );

    let citation_id = Uuid::new_v4();
    let doi = format!("10.5555/hepta.{paper_id}");
    let canonical_url = format!("https://doi.org/{doi}");
    let citation_time = Utc::now().timestamp();
    let citation_signature = human_verification_signature(
        evidence_actor,
        paper_id,
        "citation",
        citation_id,
        &format!("citation-doi\n{doi}\ncitation-url\n{canonical_url}"),
        &source_hash,
        locator,
        license,
        citation_time,
    );
    let citation_key = format!("gate-citation-{paper_id}-{suffix}");
    assert_status(
        user_post(
            router,
            evidence_actor,
            "create_citation_record_v3",
            &format!("/v2/hepta/papers/{paper_id}/citations"),
            &citation_key,
            json!({
                "citation_id":citation_id,
                "evidence_card_id":evidence_id,
                "doi":doi,
                "canonical_url":canonical_url,
                "source_hash":source_hash,
                "locator":locator,
                "license":license,
                "verification_key_id":evidence_actor.human_key_id,
                "verification_public_key":evidence_actor.human_public_key,
                "verification_public_key_hash":evidence_actor.human_public_key_hash,
                "signed_at_unix":citation_time,
                "verification_signature":citation_signature,
                "idempotency_key":citation_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    let claim_key = format!("gate-claim-{paper_id}-{suffix}");
    assert_status(
        user_post(
            router,
            evidence_actor,
            "create_claim_record_v3",
            &format!("/v2/hepta/papers/{paper_id}/claims"),
            &claim_key,
            json!({
                "claim_id":Uuid::new_v4(),
                "claim_key":format!("gate-claim-{suffix}"),
                "claim_kind":"main",
                "statement_hash":digest(&format!("gate-claim-statement-{paper_id}-{suffix}")),
                "evidence_card_ids":[evidence_id],
                "run_record_ids":[],
                "figure_lineage_ids":[],
                "idempotency_key":claim_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    transition_fixture_paper_phase(
        router,
        &actors[0],
        paper_id,
        paper_version,
        "experimenting",
        suffix,
    )
    .await;

    let run_key = format!("gate-run-{paper_id}-{suffix}");
    assert_status(
        user_post(
            router,
            &actors[2],
            "create_run_record_v3",
            &format!("/v2/hepta/papers/{paper_id}/run-records"),
            &run_key,
            json!({
                "run_record_id":Uuid::new_v4(),
                "experiment_plan_id":experiment_plan_id,
                "status":"succeeded",
                "seed":17,
                "parameters_hash":digest(&format!("gate-parameters-{paper_id}-{suffix}")),
                "logs_manifest_id":artifact.manifest_id,
                "outputs_manifest_id":environment_manifest.manifest_id,
                "metrics_hash":digest(&format!("gate-metrics-{paper_id}-{suffix}")),
                "failure_hash":null,
                "idempotency_key":run_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    transition_fixture_paper_phase(
        router,
        &actors[0],
        paper_id,
        paper_version,
        "drafting",
        suffix,
    )
    .await;

    AuthorDraftingGateSeed {
        artifact,
        work_item_id,
    }
}

async fn transition_fixture_work_to_accepted(
    router: &Router,
    actor: &Actor,
    work_item_id: Uuid,
    artifact_manifest_hash: &str,
    suffix: &str,
) {
    let path = format!("/v2/hepta/work-items/{work_item_id}/transition");
    let mut work_version = 1_u64;
    for status in ["in_progress", "review", "accepted"] {
        let key = format!("gate-work-{work_item_id}-{status}-{suffix}");
        let item = assert_status(
            user_post(
                router,
                actor,
                "transition_paper_work_item_v2",
                &path,
                &key,
                json!({
                    "expected_version":work_version,
                    "next_status":status,
                    "artifact_manifest_hash":(status == "accepted")
                        .then(|| artifact_manifest_hash.to_string()),
                    "idempotency_key":key,
                }),
            )
            .await,
            StatusCode::OK,
        );
        work_version += 1;
        assert_eq!(item["version"], work_version);
    }
}

struct PostMergeWholePaperRevisionInput<'a> {
    paper_id: Uuid,
    expected_paper_version: u64,
    parent_revision_id: Uuid,
    revision_id: Uuid,
    artifact: &'a VerifiedArtifactHashes,
    suffix: &'a str,
    expected_section_count: usize,
}

async fn create_post_merge_whole_paper_revision(
    router: &Router,
    actor: &Actor,
    input: PostMergeWholePaperRevisionInput<'_>,
) -> Uuid {
    let PostMergeWholePaperRevisionInput {
        paper_id,
        expected_paper_version,
        parent_revision_id,
        revision_id,
        artifact,
        suffix,
        expected_section_count,
    } = input;
    let key = format!("post-merge-revision-{paper_id}-{suffix}");
    let revision = assert_status(
        user_post(
            router,
            actor,
            "create_paper_revision_v2",
            &format!("/v2/hepta/papers/{paper_id}/revisions"),
            &key,
            json!({
                "revision_id":revision_id,
                "expected_paper_version":expected_paper_version,
                "parent_revision_id":parent_revision_id,
                "source_manifest_hash":artifact.source_manifest_hash,
                "artifact_manifest_hash":artifact.artifact_manifest_hash,
                "bibliography_hash":artifact.bibliography_hash,
                "claim_evidence_graph_hash":artifact.claim_evidence_graph_hash,
                "idempotency_key":key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    let root = revision["section_materialization_root"]
        .as_str()
        .expect("post-merge revision must expose its materialization root");
    assert!(root.starts_with("sha256:") && root.len() == 71);
    let descriptor = revision["section_materialization"]
        .as_object()
        .expect("post-merge revision must expose its canonical materialization descriptor");
    assert_eq!(descriptor["schema"], SECTION_MATERIALIZATION_V1);
    assert_eq!(descriptor["paper_project_id"], paper_id.to_string());
    assert_eq!(descriptor["revision_id"], revision_id.to_string());
    assert_eq!(
        descriptor["parent_revision_id"],
        parent_revision_id.to_string()
    );
    let sections = descriptor["sections"]
        .as_array()
        .expect("materialized sections");
    assert_eq!(
        sections.len(),
        expected_section_count,
        "fixture materializes every terminal merged section head"
    );
    for section in sections {
        for field in [
            "section_key",
            "base_paper_revision_id",
            "head_section_revision_id",
            "merge_id",
            "patch_manifest_id",
            "patch_hash",
        ] {
            assert!(section.get(field).is_some(), "descriptor binds {field}");
        }
    }
    revision_id
}

async fn create_approved_section_gate_fixture(
    context: &DraftingPaperContext,
    suffix: &str,
) -> ContributionGateFixture {
    let actor = &context.actors[0];
    let decision_actor = &context.actors[1];
    let reviewer = &context.actors[2];
    let section_key = format!("gate-{suffix}");
    let lease_id = Uuid::new_v4();
    let lease_key = format!(
        "gate-lease-{context_id}-{suffix}",
        context_id = context.paper_id
    );
    assert_status(
        user_post(
            &context.router,
            actor,
            "acquire_section_lease_v3",
            &format!("/v2/hepta/papers/{}/section-leases", context.paper_id),
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

    let proposal_id = Uuid::new_v4();
    let payload_hash = digest(&format!(
        "gate-section-payload-{}-{suffix}",
        context.paper_id
    ));
    let proposal_key = format!("gate-proposal-{}-{suffix}", context.paper_id);
    let created_proposal = assert_status(
        request(
            &context.router,
            "POST",
            &format!("/v2/hepta/papers/{}/agent-proposals", context.paper_id),
            signed_agent_proposal_body(
                context,
                SignedAgentProposalBodyInput {
                    proposal_id,
                    section_key: &section_key,
                    parent_revision_id: context.revision_id,
                    lease_id,
                    lease_fencing_token: 1,
                    expected_work_version: 1,
                    payload_hash: &payload_hash,
                    idempotency_key: &proposal_key,
                },
            ),
            None,
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(created_proposal["lease_id"], lease_id.to_string());
    assert_eq!(created_proposal["lease_fencing_token"], 1);
    assert_eq!(created_proposal["expected_work_version"], 1);

    let decision_id = Uuid::new_v4();
    let decision_key = format!("gate-decision-{}-{suffix}", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            decision_actor,
            "create_human_decision_v3",
            &format!("/v2/hepta/papers/{}/human-decisions", context.paper_id),
            &decision_key,
            signed_human_decision_body(
                context,
                decision_actor,
                decision_id,
                proposal_id,
                1,
                "accept",
                &digest(&format!(
                    "gate-decision-reason-{}-{suffix}",
                    context.paper_id
                )),
                &decision_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );

    let section_revision_id = Uuid::new_v4();
    let section_revision_key = format!("gate-section-revision-{}-{suffix}", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            actor,
            "create_section_revision_v3",
            &format!("/v2/hepta/papers/{}/section-revisions", context.paper_id),
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

    let review_id = Uuid::new_v4();
    let review_hash = digest(&format!(
        "gate-section-review-{}-{suffix}",
        context.paper_id
    ));
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
    let review_key = format!("gate-section-review-{}-{suffix}", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            reviewer,
            "create_section_review_v3",
            &format!(
                "/v2/hepta/papers/{}/section-revisions/{section_revision_id}/reviews",
                context.paper_id
            ),
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
                    &section_review_signing_bytes(&review_signing)
                        .expect("gate section review frame")
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
        section_key: section_key.clone(),
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
    let merge_key = format!("gate-section-merge-{}-{suffix}", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            actor,
            "create_section_merge_v3",
            &format!("/v2/hepta/papers/{}/section-merges", context.paper_id),
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
                    &section_merge_signing_bytes(&merge_signing)
                        .expect("gate section merge frame")
                ).to_bytes()),
                "idempotency_key":merge_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );

    ContributionGateFixture {
        accepted_artifact_manifest_id: context.artifact.manifest_id,
        accepted_by_player_id: decision_actor.player_id,
        accepted_section_review_id: review_id,
        reviewed_by_player_id: reviewer.player_id,
    }
}

struct CompleteExistingSectionLeaseInput<'a> {
    section_key: &'a str,
    parent_revision_id: Uuid,
    lease_id: Uuid,
    fencing_token: u64,
    artifact: &'a VerifiedArtifactHashes,
    suffix: &'a str,
}

async fn complete_existing_section_lease_fixture(
    context: &DraftingPaperContext,
    input: CompleteExistingSectionLeaseInput<'_>,
) -> Uuid {
    let CompleteExistingSectionLeaseInput {
        section_key,
        parent_revision_id,
        lease_id,
        fencing_token,
        artifact,
        suffix,
    } = input;
    let actor = &context.actors[0];
    let decision_actor = &context.actors[1];
    let reviewer = &context.actors[2];
    let proposal_id = Uuid::new_v4();
    let payload_hash = digest(&format!(
        "terminal-section-payload-{}-{suffix}",
        context.paper_id
    ));
    let proposal_key = format!("terminal-section-proposal-{}-{suffix}", context.paper_id);
    assert_status(
        request(
            &context.router,
            "POST",
            &format!("/v2/hepta/papers/{}/agent-proposals", context.paper_id),
            signed_agent_proposal_body_for_actor_and_artifact(
                context,
                actor,
                artifact,
                proposal_id,
                section_key,
                parent_revision_id,
                lease_id,
                fencing_token,
                1,
                &payload_hash,
                &proposal_key,
                None,
            ),
            None,
        )
        .await,
        StatusCode::CREATED,
    );

    let decision_id = Uuid::new_v4();
    let decision_key = format!("terminal-section-decision-{}-{suffix}", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            decision_actor,
            "create_human_decision_v3",
            &format!("/v2/hepta/papers/{}/human-decisions", context.paper_id),
            &decision_key,
            signed_human_decision_body(
                context,
                decision_actor,
                decision_id,
                proposal_id,
                1,
                "accept",
                &digest(&format!(
                    "terminal-section-decision-{}-{suffix}",
                    context.paper_id
                )),
                &decision_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );

    let section_revision_id = Uuid::new_v4();
    let revision_key = format!("terminal-section-revision-{}-{suffix}", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            actor,
            "create_section_revision_v3",
            &format!("/v2/hepta/papers/{}/section-revisions", context.paper_id),
            &revision_key,
            json!({
                "section_revision_id":section_revision_id,
                "section_key":section_key,
                "parent_revision_id":parent_revision_id,
                "proposal_id":proposal_id,
                "lease_id":lease_id,
                "fencing_token":fencing_token,
                "patch_manifest_id":artifact.manifest_id,
                "patch_hash":payload_hash,
                "idempotency_key":revision_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );

    let review_id = Uuid::new_v4();
    let review_hash = digest(&format!(
        "terminal-section-review-{}-{suffix}",
        context.paper_id
    ));
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
    let review_key = format!("terminal-section-review-{}-{suffix}", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            reviewer,
            "create_section_review_v3",
            &format!(
                "/v2/hepta/papers/{}/section-revisions/{section_revision_id}/reviews",
                context.paper_id
            ),
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
                    &section_review_signing_bytes(&review_signing)
                        .expect("terminal section review frame")
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
        parent_revision_id,
        merged_section_revision_id: section_revision_id,
        lease_id,
        fencing_token,
        merged_by_player_id: actor.player_id,
        signing_key_id: actor.human_key_id.clone(),
        signing_public_key_hash: actor.human_public_key_hash.clone(),
        merged_at_unix: merge_time,
    };
    let merge_key = format!("terminal-section-merge-{}-{suffix}", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            actor,
            "create_section_merge_v3",
            &format!("/v2/hepta/papers/{}/section-merges", context.paper_id),
            &merge_key,
            json!({
                "merge_id":merge_id,
                "section_revision_id":section_revision_id,
                "expected_revision_version":2,
                "parent_revision_id":parent_revision_id,
                "merged_section_revision_id":section_revision_id,
                "lease_id":lease_id,
                "fencing_token":fencing_token,
                "signing_key_id":actor.human_key_id,
                "signing_public_key":actor.human_public_key,
                "signing_public_key_hash":actor.human_public_key_hash,
                "merged_at_unix":merge_time,
                "signature":BASE64.encode(actor.human_key.sign(
                    &section_merge_signing_bytes(&merge_signing)
                        .expect("terminal section merge frame")
                ).to_bytes()),
                "idempotency_key":merge_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    section_revision_id
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
    let mut paper_version = 1_u64;
    let gate_seed = seed_paper_to_drafting_through_scientific_gates(
        &router,
        &actors,
        paper_id,
        challenge_id,
        &mut paper_version,
        work_item_id,
        "collaboration",
    )
    .await;
    let artifact = gate_seed.artifact;
    assert_eq!(gate_seed.work_item_id, work_item_id);
    let revision_path = format!("/v2/hepta/papers/{paper_id}/revisions");
    let revision_key = format!("p3-revision-{revision_id}");
    let initial_revision = assert_status(
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
    assert_eq!(
        initial_revision["section_materialization"]["sections"],
        json!([]),
        "initial whole-Paper revision roots an explicit empty section set"
    );
    assert!(initial_revision["section_materialization_root"]
        .as_str()
        .is_some_and(|root| root.starts_with("sha256:") && root.len() == 71));
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
                role: if member_count == 7 {
                    ["captain", "evidence", "experiment"][index % 3].to_string()
                } else {
                    ["captain", "evidence", "experiment"]
                        .get(index)
                        .copied()
                        .unwrap_or("support")
                        .to_string()
                },
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
    // 0038, 0040, 0047, and 0050 intentionally make TRUNCATE impossible for
    // immutable evidence, evaluation drafts, attestations, contribution
    // ledgers, reservations, and their source tables.
    // These PostgreSQL tests run in a dedicated disposable database, so reset
    // removes only statement-level TRUNCATE guards, clears fixture data, and
    // then reapplies all five migrations to reconstruct the production guard
    // set and verifies the exact contribution-authority catalog.
    sqlx::raw_sql(
        "drop trigger if exists hepta_trnm_time_checkpoint_v1_truncate_guard
             on hepta_trnm_cometbft_time_checkpoints_v1;
         drop trigger if exists hepta_paper_finality_v2_window_arm_truncate_guard
             on hepta_paper_chain_finality_window_arms_v2;
         drop trigger if exists hepta_paper_finality_v2_preparation_truncate_guard
             on hepta_paper_chain_finality_preparations_v2;
         drop trigger if exists hepta_paper_projects_finality_v2_truncate_guard
             on hepta_paper_projects;
         drop trigger if exists hepta_paper_reworks_truncate_guard
             on hepta_paper_reworks;
         drop trigger if exists hepta_paper_rework_resubmissions_truncate_guard
             on hepta_paper_rework_resubmissions;
         drop trigger if exists hepta_joint_submissions_finality_v2_truncate_guard
             on hepta_joint_paper_submissions;
         drop trigger if exists hepta_paper_evaluations_finality_v2_truncate_guard
             on hepta_paper_evaluations;
         drop trigger if exists hepta_paper_reproductions_finality_v2_truncate_guard
             on hepta_paper_reproductions;
         drop trigger if exists hepta_paper_appeals_finality_v2_truncate_guard
             on hepta_paper_appeals;
         drop trigger if exists hepta_paper_resolutions_finality_v2_truncate_guard
             on hepta_paper_appeal_resolutions;
         drop trigger if exists hepta_research_auth_sets_finality_v2_truncate_guard
             on hepta_research_session_authorization_sets;
         drop trigger if exists hepta_nakama_completions_finality_v2_truncate_guard
             on hepta_nakama_research_session_completions;
         drop trigger if exists hepta_evaluation_draft_truncate_guard
             on hepta_paper_evaluation_drafts;
         drop trigger if exists hepta_evaluation_draft_attestation_truncate_guard
             on hepta_paper_evaluation_draft_attestations;
         drop trigger if exists hepta_contribution_ledger_reservation_truncate_guard
             on hepta_paper_contribution_ledger_reservations;
         drop trigger if exists hepta_paper_contribution_ledger_truncate_guard
             on hepta_paper_contribution_ledgers;",
    )
    .execute(&pool)
    .await
    .expect("drop test-only immutable TRUNCATE guards before reset");
    sqlx::raw_sql(
        "truncate table
           hepta_paper_rework_resubmissions,
           hepta_paper_reworks,
           hepta_paper_chain_finality_preparations_v2,
           hepta_paper_chain_finality_window_arms_v2,
           hepta_trnm_cometbft_time_checkpoints_v1,
           hepta_paper_chain_finality_projections,
           hepta_paper_chain_receipts,
           hepta_paper_chain_finality_inbox,
           hepta_trnm_cometbft_trust_anchors,
           hepta_paper_appeal_resolutions,
           hepta_paper_appeals,
           hepta_paper_reproductions,
           hepta_paper_evaluation_draft_attestations,
           hepta_paper_evaluation_drafts,
           hepta_paper_review_assignments,
           hepta_paper_raid_scores,
           hepta_paper_scores,
           hepta_paper_evaluation_panel_attestations,
           hepta_paper_evaluations,
           hepta_paper_contribution_ledgers,
           hepta_paper_contribution_ledger_reservations,
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
    sqlx::raw_sql(include_str!(
        "../../../migrations/0038_add_hepta_paper_chain_finality_v2.sql"
    ))
    .execute(&pool)
    .await
    .expect("restore V2 constraints and TRUNCATE guards after test reset");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0040_add_hepta_evaluation_draft_quorum.sql"
    ))
    .execute(&pool)
    .await
    .expect("restore evaluation draft quorum constraints and guards after test reset");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0047_add_hepta_contribution_ledger_authority.sql"
    ))
    .execute(&pool)
    .await
    .expect("restore contribution authority constraints and guards after test reset");
    verify_contribution_ledger_authority_catalog(&pool)
        .await
        .expect("verify exact contribution authority catalog after test reset");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0050_add_hepta_paper_rework.sql"
    ))
    .execute(&pool)
    .await
    .expect("restore Author rework constraints and guards after test reset");
    super::verify_rework_migration_catalog(&pool)
        .await
        .expect("verify exact Author rework catalog after test reset");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0051_bind_legacy_evaluation_panel_lifecycle.sql"
    ))
    .execute(&pool)
    .await
    .expect("restore legacy evaluation frozen-panel lifecycle after test reset");
    super::verify_legacy_evaluation_panel_lifecycle_catalog(&pool)
        .await
        .expect("verify exact legacy evaluation frozen-panel catalog after test reset");
}

fn postgres_review_assignment_probe(
    paper_project_id: Uuid,
    submission_id: Uuid,
    player_id: Uuid,
    review_round: u64,
    slot: ReviewAssignmentSlot,
    now: chrono::DateTime<Utc>,
) -> ReviewAssignment {
    ReviewAssignment {
        schema: REVIEW_ASSIGNMENT_SCHEMA_V1.to_string(),
        assignment_id: Uuid::new_v4(),
        paper_project_id,
        submission_id,
        player_id,
        review_round,
        slot,
        pinned_evaluation_id: None,
        status: ReviewAssignmentStatus::Claimed,
        version: 1,
        claimed_at: now,
        expires_at: now + chrono::Duration::hours(1),
        updated_at: now,
    }
}

async fn insert_postgres_review_assignment_probe(
    tx: &mut Transaction<'_, Postgres>,
    assignment: &ReviewAssignment,
) {
    let slot = serde_json::to_value(assignment.slot)
        .expect("serialize probe slot")
        .as_str()
        .expect("probe slot string")
        .to_string();
    let status = serde_json::to_value(assignment.status)
        .expect("serialize probe status")
        .as_str()
        .expect("probe status string")
        .to_string();
    sqlx::query(
        "insert into hepta_paper_review_assignments (
           assignment_id,paper_project_id,submission_id,player_id,review_round,slot,
           pinned_evaluation_id,status,version,record_json,created_at,expires_at,updated_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11,$12,$13)",
    )
    .bind(assignment.assignment_id)
    .bind(assignment.paper_project_id)
    .bind(assignment.submission_id)
    .bind(assignment.player_id)
    .bind(i64::try_from(assignment.review_round).expect("probe round fits i64"))
    .bind(slot)
    .bind(assignment.pinned_evaluation_id)
    .bind(status)
    .bind(i64::try_from(assignment.version).expect("probe version fits i64"))
    .bind(serde_json::to_value(assignment).expect("serialize probe assignment"))
    .bind(assignment.claimed_at)
    .bind(assignment.expires_at)
    .bind(assignment.updated_at)
    .execute(&mut **tx)
    .await
    .expect("insert Review assignment probe");
}

async fn update_postgres_review_assignment_probe(
    tx: &mut Transaction<'_, Postgres>,
    assignment: &ReviewAssignment,
    expected_version: u64,
    expected_status: ReviewAssignmentStatus,
) -> Result<u64, sqlx::Error> {
    let status = serde_json::to_value(assignment.status)
        .expect("serialize probe update status")
        .as_str()
        .expect("probe update status string")
        .to_string();
    let expected_status = serde_json::to_value(expected_status)
        .expect("serialize expected probe status")
        .as_str()
        .expect("expected probe status string")
        .to_string();
    sqlx::query(
        "update hepta_paper_review_assignments
         set pinned_evaluation_id=$1,status=$2,version=$3,record_json=$4::jsonb,updated_at=$5
         where assignment_id=$6 and version=$7 and status=$8",
    )
    .bind(assignment.pinned_evaluation_id)
    .bind(status)
    .bind(i64::try_from(assignment.version).expect("probe version fits i64"))
    .bind(serde_json::to_value(assignment).expect("serialize probe update"))
    .bind(assignment.updated_at)
    .bind(assignment.assignment_id)
    .bind(i64::try_from(expected_version).expect("expected probe version fits i64"))
    .bind(expected_status)
    .execute(&mut **tx)
    .await
    .map(|result| result.rows_affected())
}

async fn restore_legacy_panel_lifecycle_0051(pool: &sqlx::PgPool, label: &str) {
    sqlx::raw_sql(include_str!(
        "../../../migrations/0051_bind_legacy_evaluation_panel_lifecycle.sql"
    ))
    .execute(pool)
    .await
    .unwrap_or_else(|error| panic!("restore 0051 after {label}: {error}"));
    super::verify_legacy_evaluation_panel_lifecycle_catalog(pool)
        .await
        .unwrap_or_else(|error| panic!("verify restored 0051 after {label}: {error}"));
}

async fn register_prerequisites(router: &Router, _actors: &[Actor]) -> Uuid {
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

fn agent_capability_disclosure() -> AgentCapabilityDisclosureV1 {
    AgentCapabilityDisclosureV1 {
        schema: AGENT_CAPABILITY_DISCLOSURE_V1.to_string(),
        assurance: AgentCapabilityDisclosureAssuranceV1::SelfDeclaredUnverified,
        capabilities: vec![
            AgentCapabilityV1::ArtifactAnalysis,
            AgentCapabilityV1::EvidenceSearch,
            AgentCapabilityV1::ResearchSessionSigning,
            AgentCapabilityV1::SectionDrafting,
        ],
        resource_classes: vec![
            AgentResourceClassV1::ArtifactIo,
            AgentResourceClassV1::Cpu,
            AgentResourceClassV1::Sandbox,
        ],
        max_parallel_tasks: 2,
    }
}

fn agent_binding_v3_body(actor: &Actor, idempotency_key: &str) -> Value {
    let now = Utc::now().timestamp();
    let agent_public_key = BASE64.encode(actor.agent_key.verifying_key().to_bytes());
    let agent_key_id = sha256_digest(&actor.agent_key.verifying_key().to_bytes());
    let disclosure = agent_capability_disclosure();
    let disclosure_hash =
        agent_capability_disclosure_hash(&disclosure).expect("capability disclosure hash");
    let proof = AgentBindingProofClaimV3 {
        schema: AGENT_BINDING_PROOF_V3.to_string(),
        binding_id: actor.binding_id,
        agent_id: actor.agent_id.clone(),
        agent_key_id: agent_key_id.clone(),
        agent_public_key: agent_public_key.clone(),
        agent_public_key_hash: agent_key_id.clone(),
        capability_disclosure_hash: disclosure_hash.clone(),
        subject_id: actor.subject_id.clone(),
        player_id: actor.player_id,
        nonce: idempotency_key.to_string(),
        issued_at_unix: now - 1,
        expires_at_unix: now + 300,
    };
    let signature = BASE64.encode(
        actor
            .agent_key
            .sign(&agent_binding_proof_v3_signing_bytes(&proof).expect("Agent binding V3 frame"))
            .to_bytes(),
    );
    json!({
        "binding_id": actor.binding_id,
        "player_id": actor.player_id,
        "agent_id": actor.agent_id,
        "agent_key_id": agent_key_id,
        "agent_public_key": agent_public_key,
        "agent_proof_schema": AGENT_BINDING_PROOF_V3,
        "capability_disclosure": disclosure,
        "capability_disclosure_hash": disclosure_hash,
        "agent_proof_nonce": idempotency_key,
        "agent_proof_issued_at_unix": proof.issued_at_unix,
        "agent_proof_expires_at_unix": proof.expires_at_unix,
        "agent_proof_signature": signature,
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
        capability_disclosure: None,
        capability_disclosure_hash: None,
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

async fn exercise_agent_binding_v3_capability_disclosure(state: AppState) -> (Uuid, String) {
    let router = app(state);
    let actor = actors(1).remove(0);
    create_player_only(&router, &actor).await;
    let idempotency_key = format!("agent-binding-v3-{}", actor.binding_id);
    let valid_body = agent_binding_v3_body(&actor, &idempotency_key);

    let mut missing_disclosure = valid_body.clone();
    missing_disclosure
        .as_object_mut()
        .expect("V3 binding object")
        .remove("capability_disclosure");
    missing_disclosure
        .as_object_mut()
        .expect("V3 binding object")
        .remove("capability_disclosure_hash");
    assert_eq!(
        error_code(
            user_post(
                &router,
                &actor,
                "create_agent_binding_v3",
                "/v2/hepta/agent-bindings",
                &idempotency_key,
                missing_disclosure,
            )
            .await,
            StatusCode::BAD_REQUEST,
        ),
        "agent_binding_proof_version_mismatch"
    );

    let mut mismatched_hash = valid_body.clone();
    mismatched_hash["capability_disclosure_hash"] = json!(digest("wrong-disclosure"));
    assert_eq!(
        error_code(
            user_post(
                &router,
                &actor,
                "create_agent_binding_v3",
                "/v2/hepta/agent-bindings",
                &idempotency_key,
                mismatched_hash,
            )
            .await,
            StatusCode::BAD_REQUEST,
        ),
        "agent_capability_disclosure_hash_mismatch"
    );

    let mut tampered = valid_body.clone();
    tampered["capability_disclosure"]["max_parallel_tasks"] = json!(3);
    let tampered_disclosure: AgentCapabilityDisclosureV1 =
        serde_json::from_value(tampered["capability_disclosure"].clone())
            .expect("tampered capability disclosure");
    tampered["capability_disclosure_hash"] =
        json!(agent_capability_disclosure_hash(&tampered_disclosure)
            .expect("tampered capability disclosure hash"));
    assert_eq!(
        error_code(
            user_post(
                &router,
                &actor,
                "create_agent_binding_v3",
                "/v2/hepta/agent-bindings",
                &idempotency_key,
                tampered,
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "invalid_agent_binding_proof"
    );

    let created = assert_status(
        user_post(
            &router,
            &actor,
            "create_agent_binding_v3",
            "/v2/hepta/agent-bindings",
            &idempotency_key,
            valid_body.clone(),
        )
        .await,
        StatusCode::CREATED,
    );
    let disclosure_hash = created["capability_disclosure_hash"]
        .as_str()
        .expect("V3 disclosure hash")
        .to_string();
    assert_eq!(
        created["capability_disclosure"],
        valid_body["capability_disclosure"]
    );
    assert_eq!(
        created["capability_disclosure"]["assurance"],
        "self_declared_unverified"
    );
    for forbidden in [
        "score_eligible",
        "ranking_eligible",
        "reward_eligible",
        "economic_eligible",
        "scientific_finality",
    ] {
        assert!(created.get(forbidden).is_none());
    }
    let replayed = assert_status(
        user_post(
            &router,
            &actor,
            "create_agent_binding_v3",
            "/v2/hepta/agent-bindings",
            &idempotency_key,
            valid_body,
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(replayed, created);
    let self_read = assert_status(
        user_get(
            &router,
            &actor,
            "list_self_agent_bindings_v2",
            "/v2/hepta/agent-bindings",
            "agent-binding-v3-self-read",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(self_read, json!([created.clone()]));

    let rotated_key = SigningKey::from_bytes(&[0x7e; 32]);
    let rotation_key = format!("agent-binding-v3-rotation-{}", actor.binding_id);
    let rotation_path = format!("/v2/hepta/agent-bindings/{}/rotate-key", actor.binding_id);
    let rotated = assert_status(
        user_post(
            &router,
            &actor,
            "rotate_agent_binding_key_v2",
            &rotation_path,
            &rotation_key,
            agent_binding_rotation_body(
                &actor,
                actor.binding_id,
                1,
                &actor.agent_key,
                &rotated_key,
                &rotation_key,
            ),
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(rotated["version"], 2);
    assert_eq!(rotated["capability_disclosure_hash"], disclosure_hash);
    assert_eq!(
        rotated["capability_disclosure"],
        created["capability_disclosure"]
    );
    (actor.binding_id, disclosure_hash)
}

#[test]
fn capability_disclosure_is_bounded_sorted_and_explicitly_unverified() {
    let disclosure = agent_capability_disclosure();
    let hash = agent_capability_disclosure_hash(&disclosure).expect("valid disclosure");
    assert!(hash.starts_with("sha256:") && hash.len() == 71);
    assert_eq!(
        disclosure.assurance,
        AgentCapabilityDisclosureAssuranceV1::SelfDeclaredUnverified
    );

    let mut duplicate = disclosure.clone();
    duplicate
        .capabilities
        .insert(1, AgentCapabilityV1::ArtifactAnalysis);
    assert!(agent_capability_disclosure_hash(&duplicate)
        .expect_err("duplicate capabilities must fail")
        .contains("sorted and unique"));
    let mut unsorted = disclosure.clone();
    unsorted.capabilities.swap(0, 1);
    assert!(agent_capability_disclosure_hash(&unsorted)
        .expect_err("unsorted capabilities must fail")
        .contains("sorted and unique"));
    let mut no_capacity = disclosure;
    no_capacity.max_parallel_tasks = 0;
    assert!(agent_capability_disclosure_hash(&no_capacity)
        .expect_err("zero task capacity must fail")
        .contains("between 1 and 32"));
}

#[tokio::test]
async fn memory_agent_binding_v3_binds_unverified_capability_disclosure() {
    exercise_agent_binding_v3_capability_disclosure(AppState::new(security())).await;
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
    assert!(created.get("capability_disclosure").is_none());
    assert!(created.get("capability_disclosure_hash").is_none());

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

async fn legacy_league_state_snapshot_bytes(state: &AppState) -> Vec<u8> {
    let snapshot = if let Some(pool) = &state.pool {
        let row = sqlx::query(
            "select revision,state_json from hepta_league_state where state_key='primary'",
        )
        .fetch_one(pool)
        .await
        .expect("legacy league state snapshot");
        json!({
            "revision": row.get::<i64, _>("revision"),
            "state_json": row.get::<Value, _>("state_json"),
        })
    } else {
        state
            .inspect(|league| {
                serde_json::to_value(league)
                    .map_err(|error| ApiError::internal(format!("encode legacy state: {error}")))
            })
            .await
            .expect("memory legacy league state snapshot")
    };
    canonical_json_bytes(&snapshot).expect("canonical legacy league state snapshot")
}

async fn legacy_agent_count(state: &AppState) -> usize {
    state
        .inspect(|league| Ok(league.agents.len()))
        .await
        .expect("legacy Agent registry count")
}

async fn set_agent_binding_status_for_test(
    state: &AppState,
    binding_id: Uuid,
    status: AgentBindingStatus,
) {
    let now = Utc::now();
    if let Some(pool) = &state.pool {
        let record_json: Value =
            sqlx::query_scalar("select record_json from hepta_agent_bindings where binding_id=$1")
                .bind(binding_id)
                .fetch_one(pool)
                .await
                .expect("Agent binding record for authority test");
        let mut binding: AgentBinding =
            serde_json::from_value(record_json).expect("decode authority-test Agent binding");
        binding.status = status.clone();
        binding.updated_at = now;
        sqlx::query(
            "update hepta_agent_bindings set status=$1,record_json=$2::jsonb,updated_at=$3
             where binding_id=$4",
        )
        .bind(status.as_str())
        .bind(serde_json::to_value(binding).expect("encode authority-test Agent binding"))
        .bind(now)
        .bind(binding_id)
        .execute(pool)
        .await
        .expect("update authority-test Agent binding status");
    } else {
        let mut memory = state.paper_raid.write().await;
        let binding = memory
            .bindings
            .get_mut(&binding_id)
            .expect("memory Agent binding for authority test");
        binding.status = status;
        binding.updated_at = now;
    }
}

async fn set_work_item_status_for_agent_authority_test(
    state: &AppState,
    work_item_id: Uuid,
    status: WorkItemStatus,
    version: u64,
    artifact_manifest_hash: Option<String>,
) {
    let now = Utc::now();
    if let Some(pool) = &state.pool {
        let record_json: Value = sqlx::query_scalar(
            "select record_json from hepta_paper_work_items where work_item_id=$1",
        )
        .bind(work_item_id)
        .fetch_one(pool)
        .await
        .expect("work item record for Agent authority test");
        let mut work: WorkItem =
            serde_json::from_value(record_json).expect("decode Agent authority-test work item");
        work.status = status;
        work.version = version;
        work.artifact_manifest_hash = artifact_manifest_hash;
        work.updated_at = now;
        sqlx::query(
            "update hepta_paper_work_items
             set status=$1,version=$2,record_json=$3::jsonb,updated_at=$4
             where work_item_id=$5",
        )
        .bind(status.as_str())
        .bind(i64::try_from(version).expect("Agent authority-test work version fits bigint"))
        .bind(serde_json::to_value(work).expect("encode Agent authority-test work item"))
        .bind(now)
        .bind(work_item_id)
        .execute(pool)
        .await
        .expect("update Agent authority-test work status");
    } else {
        let mut memory = state.paper_raid.write().await;
        let work = memory
            .work_items
            .get_mut(&work_item_id)
            .expect("memory work item for Agent authority test");
        work.status = status;
        work.version = version;
        work.artifact_manifest_hash = artifact_manifest_hash;
        work.updated_at = now;
    }
}

async fn expire_section_lease_for_agent_authority_test(state: &AppState, lease_id: Uuid) {
    let now = Utc::now();
    let expires_at = now - chrono::Duration::seconds(1);
    if let Some(pool) = &state.pool {
        let record_json: Value =
            sqlx::query_scalar("select record_json from hepta_section_leases where lease_id=$1")
                .bind(lease_id)
                .fetch_one(pool)
                .await
                .expect("section lease record for Agent authority test");
        let mut lease: SectionLease =
            serde_json::from_value(record_json).expect("decode Agent authority-test section lease");
        lease.expires_at = expires_at;
        lease.updated_at = now;
        sqlx::query(
            "update hepta_section_leases
             set expires_at=$1,record_json=$2::jsonb,updated_at=$3
             where lease_id=$4",
        )
        .bind(expires_at)
        .bind(serde_json::to_value(lease).expect("encode Agent authority-test section lease"))
        .bind(now)
        .bind(lease_id)
        .execute(pool)
        .await
        .expect("expire Agent authority-test section lease");
    } else {
        let mut memory = state.paper_raid.write().await;
        memory
            .collaboration
            .expire_section_lease_for_agent_authority_test(lease_id, expires_at, now);
    }
}

async fn exercise_agent_proposal_binding_authority(state: AppState) {
    let router = app(state.clone());
    let group = actors(3);
    let challenge_id = register_prerequisites(&router, &group).await;
    create_players_and_bindings(&router, &group).await;
    assert_eq!(
        legacy_agent_count(&state).await,
        0,
        "secure Agent bindings must not require the legacy Agent registry",
    );
    let context = create_locked_drafting_paper(state.clone(), group, challenge_id).await;
    let actor = context.actors[0].clone();
    let section_key = "binding-authority";
    let lease_key = format!("binding-authority-lease-{}", context.paper_id);
    let lease_id = Uuid::new_v4();
    assert_status(
        user_post(
            &context.router,
            &actor,
            "acquire_section_lease_v3",
            &format!("/v2/hepta/papers/{}/section-leases", context.paper_id),
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
    let proposal_path = format!("/v2/hepta/papers/{}/agent-proposals", context.paper_id);

    let empty_registry_before = legacy_league_state_snapshot_bytes(&state).await;
    let no_registry_key = format!("binding-authority-no-registry-{}", context.paper_id);
    assert_status(
        request(
            &context.router,
            "POST",
            &proposal_path,
            signed_agent_proposal_body_for_actor(
                &context,
                &actor,
                Uuid::new_v4(),
                section_key,
                context.revision_id,
                lease_id,
                1,
                1,
                &digest("binding-authority-no-registry"),
                &no_registry_key,
                None,
            ),
            None,
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(
        legacy_league_state_snapshot_bytes(&state).await,
        empty_registry_before,
        "Agent proposal must not read-modify-write the empty legacy registry",
    );

    for (status, work_version) in [
        (WorkItemStatus::Cancelled, 2_u64),
        (WorkItemStatus::Accepted, 3_u64),
    ] {
        set_work_item_status_for_agent_authority_test(
            &state,
            context.work_item_id,
            status,
            work_version,
            (status == WorkItemStatus::Accepted)
                .then(|| context.artifact.artifact_manifest_hash.clone()),
        )
        .await;
        let status_key = format!(
            "binding-authority-work-{}-{}",
            status.as_str(),
            context.paper_id
        );
        assert_eq!(
            error_code(
                request(
                    &context.router,
                    "POST",
                    &proposal_path,
                    signed_agent_proposal_body_for_actor(
                        &context,
                        &actor,
                        Uuid::new_v4(),
                        section_key,
                        context.revision_id,
                        lease_id,
                        1,
                        work_version,
                        &digest(&format!("binding-authority-work-{}", status.as_str())),
                        &status_key,
                        None,
                    ),
                    None,
                )
                .await,
                StatusCode::CONFLICT,
            ),
            "agent_proposal_work_not_active",
            "terminal or cancelled work must not authorize a new Agent proposal",
        );
    }
    set_work_item_status_for_agent_authority_test(
        &state,
        context.work_item_id,
        WorkItemStatus::Planned,
        4,
        None,
    )
    .await;

    let old_lease_replay_key = format!(
        "binding-authority-old-lease-exact-replay-{}",
        context.paper_id
    );
    let old_lease_replay_body = signed_agent_proposal_body_for_actor(
        &context,
        &actor,
        Uuid::new_v4(),
        section_key,
        context.revision_id,
        lease_id,
        1,
        4,
        &digest("binding-authority-old-lease-exact-replay"),
        &old_lease_replay_key,
        None,
    );

    expire_section_lease_for_agent_authority_test(&state, lease_id).await;
    let expired_lease_key = format!("binding-authority-expired-lease-{}", context.paper_id);
    assert_eq!(
        error_code(
            request(
                &context.router,
                "POST",
                &proposal_path,
                signed_agent_proposal_body_for_actor(
                    &context,
                    &actor,
                    Uuid::new_v4(),
                    section_key,
                    context.revision_id,
                    lease_id,
                    1,
                    4,
                    &digest("binding-authority-expired-lease"),
                    &expired_lease_key,
                    None,
                ),
                None,
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "agent_proposal_requires_current_lease",
    );

    let replacement_actor = context.actors[1].clone();
    let replacement_lease_id = Uuid::new_v4();
    let replacement_lease_key = format!("binding-authority-replacement-{}", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            &replacement_actor,
            "acquire_section_lease_v3",
            &format!("/v2/hepta/papers/{}/section-leases", context.paper_id),
            &replacement_lease_key,
            json!({
                "lease_id":replacement_lease_id,
                "section_key":section_key,
                "holder_binding_id":replacement_actor.binding_id,
                "expected_previous_fencing_token":1,
                "ttl_seconds":3600,
                "idempotency_key":replacement_lease_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    let replaced_lease_key = format!("binding-authority-replaced-lease-{}", context.paper_id);
    assert_eq!(
        error_code(
            request(
                &context.router,
                "POST",
                &proposal_path,
                signed_agent_proposal_body_for_actor(
                    &context,
                    &actor,
                    Uuid::new_v4(),
                    section_key,
                    context.revision_id,
                    replacement_lease_id,
                    2,
                    4,
                    &digest("binding-authority-replaced-lease"),
                    &replaced_lease_key,
                    None,
                ),
                None,
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "agent_proposal_requires_current_lease",
        "a replaced lease must not authorize its previous holder",
    );

    expire_section_lease_for_agent_authority_test(&state, replacement_lease_id).await;
    let current_lease_key = format!("binding-authority-current-lease-{}", context.paper_id);
    let current_lease_id = Uuid::new_v4();
    assert_status(
        user_post(
            &context.router,
            &actor,
            "acquire_section_lease_v3",
            &format!("/v2/hepta/papers/{}/section-leases", context.paper_id),
            &current_lease_key,
            json!({
                "lease_id":current_lease_id,
                "section_key":section_key,
                "holder_binding_id":actor.binding_id,
                "expected_previous_fencing_token":2,
                "ttl_seconds":3600,
                "idempotency_key":current_lease_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );

    assert_eq!(
        error_code(
            request(
                &context.router,
                "POST",
                &proposal_path,
                old_lease_replay_body,
                None,
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "stale_agent_proposal_lease_epoch",
        "the exact old V2 body must fail after the same binding reacquires a new lease epoch",
    );

    let old_work_version_replay_key = format!(
        "binding-authority-old-work-version-exact-replay-{}",
        context.paper_id
    );
    let old_work_version_replay_body = signed_agent_proposal_body_for_actor(
        &context,
        &actor,
        Uuid::new_v4(),
        section_key,
        context.revision_id,
        current_lease_id,
        3,
        4,
        &digest("binding-authority-old-work-version-exact-replay"),
        &old_work_version_replay_key,
        None,
    );
    set_work_item_status_for_agent_authority_test(
        &state,
        context.work_item_id,
        WorkItemStatus::Rejected,
        5,
        None,
    )
    .await;
    set_work_item_status_for_agent_authority_test(
        &state,
        context.work_item_id,
        WorkItemStatus::InProgress,
        6,
        None,
    )
    .await;
    assert_eq!(
        error_code(
            request(
                &context.router,
                "POST",
                &proposal_path,
                old_work_version_replay_body,
                None,
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "aggregate_version_conflict",
        "the exact old V2 body must fail after work is rejected and reopened at a new version",
    );

    let current_lease_proposal_key = format!(
        "binding-authority-current-lease-proposal-{}",
        context.paper_id
    );
    assert_status(
        request(
            &context.router,
            "POST",
            &proposal_path,
            signed_agent_proposal_body_for_actor(
                &context,
                &actor,
                Uuid::new_v4(),
                section_key,
                context.revision_id,
                current_lease_id,
                3,
                6,
                &digest("binding-authority-current-lease-proposal"),
                &current_lease_proposal_key,
                None,
            ),
            None,
        )
        .await,
        StatusCode::CREATED,
    );

    let wrong_legacy_key = &context.actors[1].agent_key;
    assert_status(
        request(
            &context.router,
            "POST",
            "/v1/hepta/agents",
            json!({
                "agent_id":actor.agent_id,
                "owner_id":"untrusted-legacy-owner",
                "organization_id":"untrusted-legacy-registry",
                "protocol_version":"hepta_agent_protocol_v1",
                "public_key":BASE64.encode(wrong_legacy_key.verifying_key().to_bytes()),
                "capabilities":[],
            }),
            None,
        )
        .await,
        StatusCode::CREATED,
    );
    let mismatched_registry_before = legacy_league_state_snapshot_bytes(&state).await;

    let key_mismatch_key = format!("binding-authority-key-mismatch-{}", context.paper_id);
    let wrong_key_id = sha256_digest(&wrong_legacy_key.verifying_key().to_bytes());
    assert_eq!(
        error_code(
            request(
                &context.router,
                "POST",
                &proposal_path,
                signed_agent_proposal_body_for_actor(
                    &context,
                    &actor,
                    Uuid::new_v4(),
                    section_key,
                    context.revision_id,
                    current_lease_id,
                    3,
                    6,
                    &digest("binding-authority-key-mismatch"),
                    &key_mismatch_key,
                    Some(wrong_key_id),
                ),
                None,
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "agent_key_not_current",
    );

    set_agent_binding_status_for_test(&state, actor.binding_id, AgentBindingStatus::Revoked).await;
    let inactive_key = format!("binding-authority-inactive-{}", context.paper_id);
    assert_eq!(
        request(
            &context.router,
            "POST",
            &proposal_path,
            signed_agent_proposal_body_for_actor(
                &context,
                &actor,
                Uuid::new_v4(),
                section_key,
                context.revision_id,
                current_lease_id,
                3,
                6,
                &digest("binding-authority-inactive"),
                &inactive_key,
                None,
            ),
            None,
        )
        .await
        .0,
        StatusCode::FORBIDDEN,
        "an inactive binding must never authorize an Agent proposal",
    );
    set_agent_binding_status_for_test(&state, actor.binding_id, AgentBindingStatus::Active).await;

    let foreign = context.actors[1].clone();
    let foreign_key = format!("binding-authority-foreign-{}", context.paper_id);
    assert_eq!(
        request(
            &context.router,
            "POST",
            &proposal_path,
            signed_agent_proposal_body_for_actor(
                &context,
                &foreign,
                Uuid::new_v4(),
                section_key,
                context.revision_id,
                current_lease_id,
                3,
                6,
                &digest("binding-authority-foreign"),
                &foreign_key,
                None,
            ),
            None,
        )
        .await
        .0,
        StatusCode::FORBIDDEN,
        "a foreign team binding must not claim another binding's work item",
    );

    let rotated_key = SigningKey::from_bytes(&[0x7b; 32]);
    let rotation_key = format!("binding-authority-rotation-{}", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            &actor,
            "rotate_agent_binding_key_v2",
            &format!("/v2/hepta/agent-bindings/{}/rotate-key", actor.binding_id),
            &rotation_key,
            agent_binding_rotation_body(
                &actor,
                actor.binding_id,
                1,
                &actor.agent_key,
                &rotated_key,
                &rotation_key,
            ),
        )
        .await,
        StatusCode::OK,
    );

    let stale_key = format!("binding-authority-stale-key-{}", context.paper_id);
    assert_eq!(
        error_code(
            request(
                &context.router,
                "POST",
                &proposal_path,
                signed_agent_proposal_body_for_actor(
                    &context,
                    &actor,
                    Uuid::new_v4(),
                    section_key,
                    context.revision_id,
                    current_lease_id,
                    3,
                    6,
                    &digest("binding-authority-stale-key"),
                    &stale_key,
                    None,
                ),
                None,
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "agent_key_not_current",
    );

    let mut rotated_actor = actor.clone();
    rotated_actor.agent_key = rotated_key;
    let rotated_key_request = format!("binding-authority-current-key-{}", context.paper_id);
    assert_status(
        request(
            &context.router,
            "POST",
            &proposal_path,
            signed_agent_proposal_body_for_actor(
                &context,
                &rotated_actor,
                Uuid::new_v4(),
                section_key,
                context.revision_id,
                current_lease_id,
                3,
                6,
                &digest("binding-authority-current-key"),
                &rotated_key_request,
                None,
            ),
            None,
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(
        legacy_league_state_snapshot_bytes(&state).await,
        mismatched_registry_before,
        "Agent proposal and binding rotation must not mutate mismatched legacy authority state",
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CollaborationOutcome {
    revision_unknown_code: String,
    revision_cross_paper_code: String,
    revision_descriptor_code: String,
    materialization_in_flight_code: String,
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
    exercise_private_party_matchmaking(&router, &all_actors, challenge_id).await;
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
    let evidence_actor = &context.actors[1];
    let evidence_id = Uuid::new_v4();
    let source_uri = "https://example.org/public/paper-source";
    let source_hash = digest("public-paper-source");
    let locator = "page 4, table 2";
    let license = "CC-BY-4.0";
    let evidence_time = Utc::now().timestamp();
    let evidence_signature = human_verification_signature(
        evidence_actor,
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
        "verification_key_id":evidence_actor.human_key_id,
        "verification_public_key":evidence_actor.human_public_key,
        "verification_public_key_hash":evidence_actor.human_public_key_hash,
        "signed_at_unix":evidence_time,
        "verification_signature":evidence_signature,
        "idempotency_key":evidence_key,
    });
    assert_status(
        user_post(
            &context.router,
            evidence_actor,
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
            evidence_actor,
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
        evidence_actor,
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
        "verification_key_id":evidence_actor.human_key_id,
        "verification_public_key":evidence_actor.human_public_key,
        "verification_public_key_hash":evidence_actor.human_public_key_hash,
        "signed_at_unix":citation_time,
        "verification_signature":citation_signature,
        "idempotency_key":citation_key,
    });
    assert_status(
        user_post(
            &context.router,
            evidence_actor,
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
            evidence_actor,
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

    let in_flight_key = format!("p3-materialization-in-flight-{}", context.paper_id);
    let materialization_in_flight_code = error_code(
        user_post(
            &context.router,
            actor,
            "create_paper_revision_v2",
            &revision_path,
            &in_flight_key,
            json!({
                "revision_id":Uuid::new_v4(),
                "expected_paper_version":context.paper_version,
                "parent_revision_id":context.revision_id,
                "source_manifest_hash":context.artifact.source_manifest_hash,
                "artifact_manifest_hash":context.artifact.artifact_manifest_hash,
                "bibliography_hash":context.artifact.bibliography_hash,
                "claim_evidence_graph_hash":context.artifact.claim_evidence_graph_hash,
                "idempotency_key":in_flight_key,
            }),
        )
        .await,
        StatusCode::CONFLICT,
    );
    assert_eq!(
        materialization_in_flight_code,
        "section_materialization_in_flight"
    );

    let proposal_id = Uuid::new_v4();
    let payload_hash = digest("methods-section-patch");
    let proposal_key = format!("p3-proposal-{proposal_id}");
    let proposal_path = format!("/v2/hepta/papers/{}/agent-proposals", context.paper_id);
    let proposal_body = signed_agent_proposal_body(
        &context,
        SignedAgentProposalBodyInput {
            proposal_id,
            section_key,
            parent_revision_id: context.revision_id,
            lease_id,
            lease_fencing_token: 1,
            expected_work_version: 1,
            payload_hash: &payload_hash,
            idempotency_key: &proposal_key,
        },
    );
    assert_status(
        request(&context.router, "POST", &proposal_path, proposal_body, None).await,
        StatusCode::CREATED,
    );

    let before_duplicate = collaboration_snapshot(&context, "before-duplicate").await;
    let duplicate_key = format!("p3-proposal-duplicate-{proposal_id}");
    let duplicate_body = signed_agent_proposal_body(
        &context,
        SignedAgentProposalBodyInput {
            proposal_id,
            section_key,
            parent_revision_id: context.revision_id,
            lease_id,
            lease_fencing_token: 1,
            expected_work_version: 1,
            payload_hash: &digest("different-valid-payload"),
            idempotency_key: &duplicate_key,
        },
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
                SignedAgentProposalBodyInput {
                    proposal_id: Uuid::new_v4(),
                    section_key,
                    parent_revision_id: context.revision_id,
                    lease_id: next_lease_id,
                    lease_fencing_token: 2,
                    expected_work_version: 1,
                    payload_hash: &digest("stale-parent"),
                    idempotency_key: &stale_key,
                },
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
                SignedAgentProposalBodyInput {
                    proposal_id: context.revision_id,
                    section_key: "results",
                    parent_revision_id: context.revision_id,
                    lease_id: results_lease_id,
                    lease_fencing_token: 1,
                    expected_work_version: 1,
                    payload_hash: &digest("cyclic-section-parent"),
                    idempotency_key: &cyclic_parent_key,
                },
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
                SignedAgentProposalBodyInput {
                    proposal_id: Uuid::new_v4(),
                    section_key: "results",
                    parent_revision_id: section_revision_id,
                    lease_id: results_lease_id,
                    lease_fencing_token: 1,
                    expected_work_version: 1,
                    payload_hash: &digest("cross-section-parent"),
                    idempotency_key: &cross_section_key,
                },
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
                SignedAgentProposalBodyInput {
                    proposal_id: Uuid::new_v4(),
                    section_key: "results",
                    parent_revision_id: cross_context.revision_id,
                    lease_id: results_lease_id,
                    lease_fencing_token: 1,
                    expected_work_version: 1,
                    payload_hash: &digest("cross-paper-parent"),
                    idempotency_key: &cross_paper_key,
                },
            ),
            None,
        )
        .await,
        StatusCode::CONFLICT,
    );

    let methods_terminal_artifact = create_verified_artifact_manifest(
        &context.router,
        actor,
        context.paper_id,
        challenge_id,
        context.paper_version,
        "collaboration-methods-second-generation",
    )
    .await;
    let results_terminal_artifact = create_verified_artifact_manifest(
        &context.router,
        actor,
        context.paper_id,
        challenge_id,
        context.paper_version,
        "collaboration-results-first-generation",
    )
    .await;

    complete_existing_section_lease_fixture(
        &context,
        CompleteExistingSectionLeaseInput {
            section_key,
            parent_revision_id: section_revision_id,
            lease_id: next_lease_id,
            fencing_token: 2,
            artifact: &methods_terminal_artifact,
            suffix: "methods-second-generation",
        },
    )
    .await;
    complete_existing_section_lease_fixture(
        &context,
        CompleteExistingSectionLeaseInput {
            section_key: "results",
            parent_revision_id: context.revision_id,
            lease_id: results_lease_id,
            fencing_token: 1,
            artifact: &results_terminal_artifact,
            suffix: "results-first-generation",
        },
    )
    .await;

    transition_fixture_work_to_accepted(
        &context.router,
        actor,
        context.work_item_id,
        &context.artifact.artifact_manifest_hash,
        "collaboration",
    )
    .await;

    let transition_path = format!("/v2/hepta/papers/{}/transition", context.paper_id);
    for phase in ["integrity_review", "reproducing"] {
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
    let stale_revision_key = format!("p3-stale-whole-paper-revision-{}", context.paper_id);
    assert_eq!(
        error_code(
            user_post(
                &context.router,
                actor,
                "transition_paper_project_v2",
                &transition_path,
                &stale_revision_key,
                json!({
                    "expected_version":context.paper_version,
                    "next_phase":"author_approval",
                    "idempotency_key":stale_revision_key,
                }),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "paper_phase_gate_blocked"
    );
    let post_merge_revision_id = create_post_merge_whole_paper_revision(
        &context.router,
        actor,
        PostMergeWholePaperRevisionInput {
            paper_id: context.paper_id,
            expected_paper_version: context.paper_version,
            parent_revision_id: context.revision_id,
            revision_id: Uuid::new_v4(),
            artifact: &context.artifact,
            suffix: "collaboration-kernel",
            expected_section_count: 2,
        },
    )
    .await;
    context.revision_id = post_merge_revision_id;
    context.paper_version += 1;
    let author_approval_key = format!("p3-freeze-phase-{}-author-approval", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            actor,
            "transition_paper_project_v2",
            &transition_path,
            &author_approval_key,
            json!({
                "expected_version":context.paper_version,
                "next_phase":"author_approval",
                "idempotency_key":author_approval_key,
            }),
        )
        .await,
        StatusCode::OK,
    );
    context.paper_version += 1;
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

    let rebased = collaboration_snapshot(&context, "materialized-rebase").await;
    let rebased_head = rebased["room"]["section_heads"]
        .as_array()
        .expect("section heads")
        .iter()
        .find(|head| head["section_key"] == section_key)
        .expect("methods section head");
    assert_eq!(
        rebased_head["base_paper_revision_id"],
        post_merge_revision_id.to_string()
    );
    assert_eq!(
        rebased_head["current_head_revision_id"],
        post_merge_revision_id.to_string()
    );

    let rollback_key = format!("p3-materialized-rollback-{}", context.paper_id);
    assert_status(
        user_post(
            &context.router,
            actor,
            "transition_paper_project_v2",
            &transition_path,
            &rollback_key,
            json!({
                "expected_version":context.paper_version,
                "next_phase":"drafting",
                "idempotency_key":rollback_key,
            }),
        )
        .await,
        StatusCode::OK,
    );
    context.paper_version += 1;

    let continuation_work_item_id = Uuid::new_v4();
    let continuation_work_key =
        format!("p3-materialized-continuation-work-{continuation_work_item_id}");
    let continuation_work = assert_status(
        user_post(
            &context.router,
            actor,
            "create_paper_work_item_v2",
            &format!("/v2/hepta/papers/{}/work-items", context.paper_id),
            &continuation_work_key,
            json!({
                "work_item_id":continuation_work_item_id,
                "expected_paper_version":context.paper_version,
                "kind":"paper_section",
                "title":"Continue the methods section after materialization",
                "assigned_player_id":actor.player_id,
                "assigned_binding_id":actor.binding_id,
                "idempotency_key":continuation_work_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(continuation_work["version"], 1);
    context.work_item_id = continuation_work_item_id;
    context.paper_version += 1;

    let continuation_lease_id = Uuid::new_v4();
    let continuation_lease_key = format!("p3-materialized-continuation-{continuation_lease_id}");
    let continuation_lease = assert_status(
        user_post(
            &context.router,
            actor,
            "acquire_section_lease_v3",
            &lease_path,
            &continuation_lease_key,
            json!({
                "lease_id":continuation_lease_id,
                "section_key":section_key,
                "holder_binding_id":actor.binding_id,
                "expected_previous_fencing_token":2,
                "ttl_seconds":3600,
                "idempotency_key":continuation_lease_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(continuation_lease["fencing_token"], 3);
    let continuation_proposal_id = Uuid::new_v4();
    let continuation_proposal_key =
        format!("p3-materialized-continuation-proposal-{continuation_proposal_id}");
    assert_status(
        request(
            &context.router,
            "POST",
            &proposal_path,
            signed_agent_proposal_body(
                &context,
                SignedAgentProposalBodyInput {
                    proposal_id: continuation_proposal_id,
                    section_key,
                    parent_revision_id: post_merge_revision_id,
                    lease_id: continuation_lease_id,
                    lease_fencing_token: 3,
                    expected_work_version: 1,
                    payload_hash: &digest("methods-section-post-materialization-patch"),
                    idempotency_key: &continuation_proposal_key,
                },
            ),
            None,
        )
        .await,
        StatusCode::CREATED,
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
        materialization_in_flight_code,
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
    actors: &mut [Actor],
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
        actors[0].agent_key = new_agent_key;
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
    let mut revision_id = Uuid::from_u128(0x6000_0000_0000_4000_8000_0000_0000_0000 + base);
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
        &mut actors,
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
    let gate_seed = seed_paper_to_drafting_through_scientific_gates(
        &router,
        &actors,
        paper_id,
        challenge_id,
        &mut paper_version,
        work_item_id,
        &format!("authors-{member_count}"),
    )
    .await;
    let verified_artifact = gate_seed.artifact;
    assert_eq!(gate_seed.work_item_id, work_item_id);

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

    let mut section_actors = actors.clone();
    let section_work_item_id = if options.replace_session_epoch {
        // The session-replacement scenario intentionally rotates author 1's
        // binding key without rotating the separate League Agent
        // registration. Use another registered Agent for the section fixture
        // while retaining the rotated key for the authorization assertions.
        section_actors.swap(0, 1);
        let section_work_item_id = Uuid::new_v4();
        let key = format!("replacement-section-work-{member_count}");
        assert_status(
            user_post(
                &router,
                &section_actors[0],
                "create_paper_work_item_v2",
                &format!("/v2/hepta/papers/{paper_id}/work-items"),
                &key,
                json!({
                    "work_item_id":section_work_item_id,
                    "expected_paper_version":paper_version,
                    "kind":"paper_section",
                    "title":"Draft section after the roster-key replacement",
                    "assigned_player_id":section_actors[0].player_id,
                    "assigned_binding_id":section_actors[0].binding_id,
                    "idempotency_key":key,
                }),
            )
            .await,
            StatusCode::CREATED,
        );
        paper_version += 1;
        section_work_item_id
    } else {
        work_item_id
    };
    let section_gate_context = DraftingPaperContext {
        router: router.clone(),
        state: state.clone(),
        actors: section_actors,
        paper_id,
        work_item_id: section_work_item_id,
        revision_id,
        paper_version,
        artifact: verified_artifact.clone(),
    };
    let contribution_fixture = create_approved_section_gate_fixture(
        &section_gate_context,
        &format!("authors-{member_count}"),
    )
    .await;
    if section_work_item_id != work_item_id {
        transition_fixture_work_to_accepted(
            &router,
            &section_gate_context.actors[0],
            section_work_item_id,
            &verified_artifact.artifact_manifest_hash,
            &format!("authors-{member_count}-replacement"),
        )
        .await;
    }
    transition_fixture_work_to_accepted(
        &router,
        &actors[0],
        work_item_id,
        &verified_artifact.artifact_manifest_hash,
        &format!("authors-{member_count}"),
    )
    .await;

    for phase in ["integrity_review", "reproducing"] {
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

    let post_merge_revision_suffix = format!("authors-{member_count}");
    revision_id = create_post_merge_whole_paper_revision(
        &router,
        &actors[0],
        PostMergeWholePaperRevisionInput {
            paper_id,
            expected_paper_version: paper_version,
            parent_revision_id: revision_id,
            revision_id: Uuid::from_u128(0x6100_0000_0000_4000_8000_0000_0000_0000 + base),
            artifact: &verified_artifact,
            suffix: &post_merge_revision_suffix,
            expected_section_count: 1,
        },
    )
    .await;
    paper_version += 1;
    let author_approval_key = format!("phase-{member_count}-author_approval");
    assert_status(
        user_post(
            &router,
            &actors[0],
            "transition_paper_project_v2",
            &transition_path,
            &author_approval_key,
            json!({
                "expected_version":paper_version,
                "next_phase":"author_approval",
                "idempotency_key":author_approval_key,
            }),
        )
        .await,
        StatusCode::OK,
    );
    paper_version += 1;

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
            let accepted_artifact_manifest_ids = (actor.player_id
                == contribution_fixture.accepted_by_player_id)
                .then_some(contribution_fixture.accepted_artifact_manifest_id)
                .into_iter()
                .collect::<Vec<_>>();
            let accepted_section_review_ids = (actor.player_id
                == contribution_fixture.reviewed_by_player_id)
                .then_some(contribution_fixture.accepted_section_review_id)
                .into_iter()
                .collect::<Vec<_>>();
            let artifact_points = if actor.player_id == contribution_fixture.accepted_by_player_id {
                100_u64
            } else {
                0
            };
            let review_points = if actor.player_id == contribution_fixture.reviewed_by_player_id {
                150_u64
            } else {
                0
            };
            let contribution_points = artifact_points + review_points;
            json!({
                "player_id": actor.player_id,
                "credit_roles": ["methodology", "writing_review_editing"],
                "accepted_artifact_manifest_ids": accepted_artifact_manifest_ids,
                "accepted_section_review_ids": accepted_section_review_ids,
                "contribution_points": contribution_points,
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
    let mut omitted_contribution_entries = actors
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
    omitted_contribution_entries
        .sort_by_key(|entry| entry["player_id"].as_str().unwrap().to_string());
    let omitted_contribution_ledger_hash = canonical_json_sha256(&json!({
        "schema": CONTRIBUTION_LEDGER_SCHEMA_V1,
        "contribution_ledger_id": contribution_ledger_id,
        "paper_project_id": paper_id,
        "entries": omitted_contribution_entries,
    }))
    .expect("omitted contribution ledger preimage");
    let nil_promote_key = format!("promote-{member_count}-nil-ledger-id");
    assert_eq!(
        error_code(
            user_post(
                &router,
                &actors[0],
                "promote_paper_release_candidate_v2",
                &promote_path,
                &nil_promote_key,
                json!({
                    "expected_paper_version":paper_version,
                    "expected_revision_version":1,
                    "title":format!("Paper Raid {member_count}-author reproducible study"),
                    "abstract_text":"A complete reproducible research collaboration test.",
                    "collaboration_compact_hash":compact_hash,
                    "research_protocol_snapshot_hash":digest("research-protocol"),
                    "ethics_disclosure_hash":digest("ethics-disclosure"),
                    "coi_disclosure_hash":digest("coi-disclosure"),
                    "contribution_ledger_id":Uuid::nil(),
                    "contribution_ledger_hash":contribution_ledger_hash,
                    "ai_disclosure_hash":digest("ai-disclosure"),
                    "license":"CC-BY-4.0",
                    "authors":authors,
                    "idempotency_key":nil_promote_key,
                }),
            )
            .await,
            StatusCode::BAD_REQUEST,
        ),
        "nil_contribution_ledger_id",
        "promotion must reject the globally preemptable nil ledger ID",
    );
    let omitted_promote_key = format!("promote-{member_count}-omitted-contributions");
    assert_eq!(
        error_code(
            user_post(
                &router,
                &actors[0],
                "promote_paper_release_candidate_v2",
                &promote_path,
                &omitted_promote_key,
                json!({
                    "expected_paper_version":paper_version,
                    "expected_revision_version":1,
                    "title":format!("Paper Raid {member_count}-author reproducible study"),
                    "abstract_text":"A complete reproducible research collaboration test.",
                    "collaboration_compact_hash":compact_hash,
                    "research_protocol_snapshot_hash":digest("research-protocol"),
                    "ethics_disclosure_hash":digest("ethics-disclosure"),
                    "coi_disclosure_hash":digest("coi-disclosure"),
                    "contribution_ledger_id":contribution_ledger_id,
                    "contribution_ledger_hash":omitted_contribution_ledger_hash,
                    "ai_disclosure_hash":digest("ai-disclosure"),
                    "license":"CC-BY-4.0",
                    "authors":authors,
                    "idempotency_key":omitted_promote_key,
                }),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "contribution_ledger_hash_mismatch",
        "promotion must reject a ledger hash that omits authoritative contribution refs",
    );
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
                "contribution_ledger_id":contribution_ledger_id,
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
    assert_eq!(
        promoted["revision"]["release_candidate"]["section_materialization_root"],
        promoted["revision"]["section_materialization_root"],
        "release-candidate hash input must bind the exact revision materialization root"
    );
    let release_candidate_hash = promoted["release_candidate_hash"]
        .as_str()
        .expect("release candidate hash")
        .to_string();

    if options.replay_0047_after_reservation {
        let pool = state
            .pool
            .as_ref()
            .expect("0047 reservation-only replay requires PostgreSQL state");
        let frozen_ledger_count: i64 = sqlx::query_scalar(
            "select count(*)
               from hepta_paper_contribution_ledgers
              where contribution_ledger_id=$1
                and paper_project_id=$2
                and release_candidate_hash=$3",
        )
        .bind(contribution_ledger_id)
        .bind(paper_id)
        .bind(&release_candidate_hash)
        .fetch_one(pool)
        .await
        .expect("inspect reservation-only release candidate ledger state");
        assert_eq!(
            frozen_ledger_count, 0,
            "the 0047 replay probe must run before the contribution ledger is frozen"
        );
        let exact_reservation_count: i64 = sqlx::query_scalar(
            "select count(*)
               from hepta_paper_contribution_ledger_reservations
              where contribution_ledger_id=$1
                and paper_project_id=$2
                and release_candidate_hash=$3",
        )
        .bind(contribution_ledger_id)
        .bind(paper_id)
        .bind(&release_candidate_hash)
        .fetch_one(pool)
        .await
        .expect("inspect exact promotion-time contribution reservation");
        assert_eq!(
            exact_reservation_count, 1,
            "post-0047 promotion must own exactly one reservation triple"
        );
        sqlx::raw_sql(include_str!(
            "../../../migrations/0047_add_hepta_contribution_ledger_authority.sql"
        ))
        .execute(pool)
        .await
        .expect(
            "0047 replay must accept a live post-0047 candidate with its exact reservation and no frozen ledger yet",
        );
        verify_contribution_ledger_authority_catalog(pool)
            .await
            .expect("0047 reservation-only replay must preserve the exact authority catalog");
    }

    let ledger_path = format!("/v2/hepta/papers/{paper_id}/contribution-ledgers");
    let added_ledger_key = format!("contribution-ledger-{member_count}-added-ref");
    assert_eq!(
        error_code(
            user_post(
                &router,
                &actors[0],
                "create_contribution_ledger_v1",
                &ledger_path,
                &added_ledger_key,
                json!({
                    "contribution_ledger_id":contribution_ledger_id,
                    "expected_paper_version":paper_version,
                    "release_candidate_hash":release_candidate_hash,
                    "entries":actors.iter().map(|actor| json!({
                        "player_id":actor.player_id,
                        "credit_roles":["methodology","writing_review_editing"],
                        "accepted_artifact_manifest_ids":
                            (actor.player_id == contribution_fixture.accepted_by_player_id)
                                .then_some(vec![
                                    contribution_fixture.accepted_artifact_manifest_id,
                                    Uuid::new_v4(),
                                ])
                                .unwrap_or_default(),
                        "accepted_section_review_ids":
                            (actor.player_id == contribution_fixture.reviewed_by_player_id)
                                .then_some(contribution_fixture.accepted_section_review_id)
                                .into_iter()
                                .collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                    "idempotency_key":added_ledger_key,
                }),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "contribution_ledger_entries_mismatch",
        "ledger creation must reject an added non-authoritative contribution ref",
    );
    let duplicate_ledger_key = format!("contribution-ledger-{member_count}-duplicate-ref");
    assert_eq!(
        error_code(
            user_post(
                &router,
                &actors[0],
                "create_contribution_ledger_v1",
                &ledger_path,
                &duplicate_ledger_key,
                json!({
                    "contribution_ledger_id":contribution_ledger_id,
                    "expected_paper_version":paper_version,
                    "release_candidate_hash":release_candidate_hash,
                    "entries":actors.iter().map(|actor| json!({
                        "player_id":actor.player_id,
                        "credit_roles":["methodology","writing_review_editing"],
                        "accepted_artifact_manifest_ids":
                            (actor.player_id == contribution_fixture.accepted_by_player_id)
                                .then_some(vec![
                                    contribution_fixture.accepted_artifact_manifest_id,
                                    contribution_fixture.accepted_artifact_manifest_id,
                                ])
                                .unwrap_or_default(),
                        "accepted_section_review_ids":[],
                    })).collect::<Vec<_>>(),
                    "idempotency_key":duplicate_ledger_key,
                }),
            )
            .await,
            StatusCode::BAD_REQUEST,
        ),
        "duplicate_contribution_reference",
        "ledger creation must reject duplicate contribution refs",
    );
    let omitted_ledger_key = format!("contribution-ledger-{member_count}-omitted");
    assert_eq!(
        error_code(
            user_post(
                &router,
                &actors[0],
                "create_contribution_ledger_v1",
                &ledger_path,
                &omitted_ledger_key,
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
                    "idempotency_key":omitted_ledger_key,
                }),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "contribution_ledger_entries_mismatch",
        "ledger creation must reject an omitted authoritative contribution ref",
    );
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
                    "accepted_artifact_manifest_ids":
                        (actor.player_id == contribution_fixture.accepted_by_player_id)
                            .then_some(contribution_fixture.accepted_artifact_manifest_id)
                            .into_iter()
                            .collect::<Vec<_>>(),
                    "accepted_section_review_ids":
                        (actor.player_id == contribution_fixture.reviewed_by_player_id)
                            .then_some(contribution_fixture.accepted_section_review_id)
                            .into_iter()
                            .collect::<Vec<_>>(),
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

async fn claim_review_assignment_fixture(
    router: &Router,
    actor: &Actor,
    paper_id: Uuid,
    review_round: u64,
    slot: &str,
    suffix: &str,
) -> Value {
    let assignment_id = Uuid::new_v4();
    let key = format!("review-assignment-{paper_id}-{review_round}-{slot}-{suffix}");
    let assignment = assert_status(
        user_post(
            router,
            actor,
            "claim_paper_review_assignment_v1",
            &format!("/v2/hepta/papers/{paper_id}/review-assignments"),
            &key,
            json!({
                "assignment_id":assignment_id,
                "player_id":actor.player_id,
                "review_round":review_round,
                "slot":slot,
                "idempotency_key":key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(assignment["schema"], REVIEW_ASSIGNMENT_SCHEMA_V1);
    assert_eq!(assignment["assignment_id"], assignment_id.to_string());
    assert_eq!(assignment["player_id"], actor.player_id.to_string());
    assert_eq!(assignment["review_round"], review_round);
    assert_eq!(assignment["slot"], slot);
    assert_eq!(assignment["status"], "claimed");
    assert!(assignment["claimed_at"].is_string());
    assert!(assignment["expires_at"].is_string());
    assignment
}

async fn claim_evaluation_panel_fixture(
    router: &Router,
    paper_id: Uuid,
    review_round: u64,
    evaluator: &Actor,
    reviewers: [&Actor; 2],
    suffix: &str,
) {
    claim_review_assignment_fixture(
        router,
        evaluator,
        paper_id,
        review_round,
        "evaluator",
        suffix,
    )
    .await;
    claim_review_assignment_fixture(
        router,
        reviewers[0],
        paper_id,
        review_round,
        "reviewer_1",
        suffix,
    )
    .await;
    claim_review_assignment_fixture(
        router,
        reviewers[1],
        paper_id,
        review_round,
        "reviewer_2",
        suffix,
    )
    .await;
}

pub(super) async fn seed_three_member_postgres_flow_for_control_test(state: AppState) {
    let _ = run_full_flow(state, 3, FlowOptions::default()).await;
}

async fn seeded_chain_finality_authorization_set(
    state: &AppState,
    session_id: &str,
    roster_version: u64,
) -> ResearchSessionAuthorizationSetV1 {
    if let Some(pool) = &state.pool {
        let record_json: Value = sqlx::query_scalar(
            "select record_json from hepta_research_session_authorization_sets
             where session_id=$1 and roster_version=$2",
        )
        .bind(session_id)
        .bind(i64::try_from(roster_version).expect("fixture roster version fits PostgreSQL bigint"))
        .fetch_one(pool)
        .await
        .expect("seeded PostgreSQL Research Session authorization set");
        return serde_json::from_value(record_json)
            .expect("decode seeded PostgreSQL Research Session authorization set");
    }

    state
        .paper_raid
        .read()
        .await
        .research_session_authorization_sets
        .get(&(session_id.to_string(), roster_version))
        .cloned()
        .expect("seeded in-memory Research Session authorization set")
}

fn seeded_chain_finality_nakama_completion(
    authorization_set: &ResearchSessionAuthorizationSetV1,
    submission: &JointPaperSubmission,
) -> (ResearchSessionCompletionV1, Vec<ResearchSessionEventV1>) {
    assert_eq!(
        authorization_set.status,
        ResearchSessionAuthorizationSetStatus::Consumed,
        "the fixture must exercise the real consumed-to-completed transition"
    );
    let first_claim = &authorization_set
        .members
        .first()
        .expect("seeded Research Session has members")
        .authorization
        .claim;
    let terminal_facts = ResearchSessionTerminalFactsV1 {
        result_code: "paper_bundle_ready".to_string(),
        paper_bundle_hash: submission.paper_bundle_hash.clone(),
        paper_release_candidate_hash: submission.release_candidate_hash.clone(),
        contribution_ledger_hash: submission
            .paper_bundle
            .release_candidate
            .contribution_ledger_hash
            .clone(),
    };
    let terminal_frame = research_session_terminal_facts_frame(&terminal_facts)
        .expect("canonical terminal facts frame");
    let completed_at_unix = Utc::now().timestamp();
    let causation_id = sha256_digest(&terminal_frame);
    let mut terminal_event = ResearchSessionEventV1 {
        schema: RESEARCH_SESSION_EVENT_V1.to_string(),
        event_id: research_session_event_id(&authorization_set.session_id, 1, &causation_id)
            .expect("canonical terminal event ID"),
        event_type: "research_session_completed".to_string(),
        session_id: authorization_set.session_id.clone(),
        team_id: authorization_set.team_id.to_string(),
        paper_project_id: authorization_set.paper_project_id.to_string(),
        challenge_id: authorization_set.challenge_id.to_string(),
        roster_version: authorization_set.roster_version,
        sequence: 1,
        causation_id,
        occurred_at_unix: completed_at_unix,
        participant_slot: 0,
        session_version: 2,
        action_type: "server.complete".to_string(),
        payload_type: "trnm.research-session.terminal-facts.v1".to_string(),
        payload: BASE64.encode(&terminal_frame),
        payload_hash: sha256_digest(&terminal_frame),
        reference_hash: submission.release_candidate_hash.clone(),
        event_hash: String::new(),
    };
    terminal_event.event_hash =
        research_session_event_hash(&terminal_event).expect("canonical terminal event hash");
    let archive = vec![terminal_event];
    let event_root = research_session_event_root(&archive).expect("canonical event root");
    let archive_hash =
        research_session_archive_hash(&archive).expect("canonical Research Session archive hash");
    let commitment_id =
        research_session_commitment_id(&authorization_set.session_id, &event_root, &archive_hash)
            .expect("canonical MatchEvidence commitment ID");
    let mut completion = ResearchSessionCompletionV1 {
        schema: RESEARCH_SESSION_COMPLETION_V1.to_string(),
        commitment_id,
        session_id: authorization_set.session_id.clone(),
        team_id: authorization_set.team_id.to_string(),
        paper_project_id: authorization_set.paper_project_id.to_string(),
        challenge_id: authorization_set.challenge_id.to_string(),
        roster_version: authorization_set.roster_version,
        roster_root: authorization_set.roster_root.clone(),
        terminal_facts,
        event_count: u64::try_from(archive.len()).expect("fixture archive count fits u64"),
        event_root,
        archive_hash,
        ruleset_hash: first_claim.ruleset_hash.clone(),
        challenge_snapshot_hash: first_claim.challenge_snapshot_hash.clone(),
        completed_at_unix,
        authority_key_id: "nakama-paper-raid-test-v1".to_string(),
        signature: String::new(),
    };
    let signing_frame = research_session_completion_signing_bytes(&completion)
        .expect("canonical Nakama completion signing frame");
    completion.signature = BASE64.encode(
        SigningKey::from_bytes(&[0x75; 32])
            .sign(&signing_frame)
            .to_bytes(),
    );
    (completion, archive)
}

#[allow(dead_code)]
pub(crate) async fn seed_paper_chain_finality_test(state: AppState) -> PaperTrnmCommandBindingV1 {
    let _ = run_full_flow(state.clone(), 3, FlowOptions::default()).await;
    let router = app(state.clone());
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
    claim_evaluation_panel_fixture(
        &router,
        paper_id,
        1,
        &external[0],
        [&external[1], &external[2]],
        "chain-finality-round-1",
    )
    .await;
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
    claim_review_assignment_fixture(
        &router,
        &external[3],
        paper_id,
        1,
        "reproducer",
        "chain-finality-round-1",
    )
    .await;
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
    let research_session_id = "paper-raid-3-session";
    let research_session_roster_version = 1;
    let authorization_set = seeded_chain_finality_authorization_set(
        &state,
        research_session_id,
        research_session_roster_version,
    )
    .await;
    let (completion, archive) =
        seeded_chain_finality_nakama_completion(&authorization_set, &submission);
    let completion_commitment_id = completion.commitment_id.clone();
    let completion_key = "chain-finality-nakama-completion";
    let completion_receipt = assert_status(
        request(
            &router,
            "POST",
            "/v2/hepta/nakama/research-session-completions",
            json!({
                "schema":"hepta.paper_raid.nakama_completion_ingest.v1",
                "completion":completion,
                "archive":archive,
                "idempotency_key":completion_key,
            }),
            None,
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(
        completion_receipt["commitment_id"], completion_commitment_id,
        "Hepta must persist the exact archive-bound MatchEvidence commitment"
    );
    let mut binding = PaperTrnmCommandBindingV1 {
        schema: PAPER_TRNM_COMMAND_BINDING_SCHEMA_V1.to_string(),
        paper_project_id: paper_id,
        submission_id: submission.submission_id,
        evaluation_id,
        research_session_id: research_session_id.to_string(),
        research_session_roster_version,
        match_evidence_commitment_id: completion_commitment_id,
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

const PAPER_FINALITY_V2_ANCHOR_FILE: &[u8] =
    include_bytes!("../../../vendor/trnm-finality-verifier/fixtures/cometbft-trust-anchor-v1.json");
const PAPER_FINALITY_V2_RECEIPT_FILE: &[u8] = include_bytes!(
    "../../../vendor/trnm-finality-verifier/fixtures/cometbft-apphash-finality-receipt-v2.json"
);
const PAPER_FINALITY_V2_ANCHOR_HASH: &str =
    "88b73fc902dd554c35b9a44ff582ec6d76e59085a2e4fdf14292183f4b3846d5";
const PAPER_FINALITY_V2_FIXTURE_VERIFICATION_TIME_UNIX_S: u64 = 1_786_034_510;

pub(crate) fn paper_chain_finality_v2_security() -> SecurityConfig {
    security()
        .with_pinned_trnm_cometbft_trust_anchor_hash(PAPER_FINALITY_V2_ANCHOR_HASH)
        .expect("valid pinned Paper finality V2 trust anchor")
}

fn set_paper_chain_finality_v2_clock(state: &mut AppState, unix_ms: u64) {
    let instant = UNIX_EPOCH + Duration::from_millis(unix_ms);
    state.cometbft_local_verification_clock = Arc::new(move || instant);
}

async fn admit_paper_chain_finality_v2_anchor(router: &Router) {
    let canonical = PAPER_FINALITY_V2_ANCHOR_FILE
        .strip_suffix(b"\n")
        .expect("repository trust-anchor fixture has one transport newline");
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v2/hepta/operator/trnm/trust-anchors")
                .header("content-type", "application/json")
                .header(OPERATOR_TOKEN_HEADER, "operator")
                .body(Body::from(canonical.to_vec()))
                .expect("trust-anchor request"),
        )
        .await
        .expect("trust-anchor response");
    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn admit_paper_chain_finality_v2_start_checkpoint(
    router: &Router,
) -> PaperTrnmChainTimeCheckpointV1 {
    let receipt: Value = serde_json::from_slice(PAPER_FINALITY_V2_RECEIPT_FILE)
        .expect("decode CometBFT Receipt V2 fixture");
    let request_body = json!({
        "trust_anchor_hash":PAPER_FINALITY_V2_ANCHOR_HASH,
        "proof":receipt["commitment_light_proof"],
    });
    let path = "/v2/hepta/operator/trnm/time-checkpoints";
    let created = assert_status(
        request(router, "POST", path, request_body.clone(), None).await,
        StatusCode::CREATED,
    );
    assert_eq!(
        assert_status(
            request(router, "POST", path, request_body, None).await,
            StatusCode::OK,
        ),
        created,
        "exact checkpoint admission replay must preserve the authenticated tuple"
    );
    serde_json::from_value(created).expect("decode admitted Chain-time checkpoint")
}

fn synthetic_authenticated_checkpoint(
    start: &PaperTrnmChainTimeCheckpointV1,
    height: u64,
    consensus_time_unix_ms: u64,
    suffix: &str,
) -> (PaperTrnmChainTimeCheckpointV1, Value) {
    // The vendored CometBFT fixture above exercises the real admission and
    // verifier boundary, but it contains only one finalized height.  Later
    // heights are inserted as already-authenticated repository records so
    // this suite can isolate arm/deadline/seal semantics.  Verifier fixtures
    // separately reject unsigned or structurally invalid light proofs.
    let canonical_proof = json!({
        "schema":"hepta.paper_raid.test_authenticated_chain_time_checkpoint.v1",
        "source_checkpoint_hash":start.checkpoint_hash,
        "height":height,
        "consensus_time_unix_ms":consensus_time_unix_ms,
        "suffix":suffix,
    });
    let header_hash = digest(&format!("paper-finality-v2-header-{height}-{suffix}"))
        .strip_prefix("sha256:")
        .expect("digest prefix")
        .to_string();
    let checkpoint = PaperTrnmChainTimeCheckpointV1 {
        schema: crate::paper_chain_finality_v2::PAPER_TRNM_CHAIN_TIME_CHECKPOINT_SCHEMA_V1
            .to_string(),
        checkpoint_hash: digest(&format!(
            "paper-finality-v2-checkpoint-{height}-{consensus_time_unix_ms}-{suffix}"
        )),
        trust_anchor_hash: start.trust_anchor_hash.clone(),
        chain_id: start.chain_id.clone(),
        height,
        header_hash,
        consensus_time_unix_ms,
        canonical_proof_sha256: canonical_json_sha256(&canonical_proof)
            .expect("canonical synthetic proof hash"),
        locally_verified_at_unix_ms: consensus_time_unix_ms,
    };
    (checkpoint, canonical_proof)
}

async fn seed_authenticated_chain_time_checkpoint(
    state: &AppState,
    checkpoint: &PaperTrnmChainTimeCheckpointV1,
    canonical_proof: &Value,
) {
    if let Some(pool) = state.finality_pool.as_ref().or(state.pool.as_ref()) {
        sqlx::query(
            "insert into hepta_trnm_cometbft_time_checkpoints_v1 (
                checkpoint_hash,trust_anchor_hash,chain_id,height,header_hash,
                consensus_time_unix_ms,canonical_proof,canonical_proof_sha256,
                locally_verified_at_unix_ms,record_json
             ) values ($1,$2,$3,$4,$5,$6,$7::jsonb,$8,$9,$10::jsonb)",
        )
        .bind(&checkpoint.checkpoint_hash)
        .bind(&checkpoint.trust_anchor_hash)
        .bind(&checkpoint.chain_id)
        .bind(i64::try_from(checkpoint.height).expect("test checkpoint height fits i64"))
        .bind(&checkpoint.header_hash)
        .bind(
            i64::try_from(checkpoint.consensus_time_unix_ms)
                .expect("test checkpoint time fits i64"),
        )
        .bind(canonical_proof)
        .bind(&checkpoint.canonical_proof_sha256)
        .bind(
            i64::try_from(checkpoint.locally_verified_at_unix_ms)
                .expect("test local verification time fits i64"),
        )
        .bind(serde_json::to_value(checkpoint).expect("encode test checkpoint"))
        .execute(pool)
        .await
        .expect("seed authenticated synthetic Chain-time checkpoint");
    } else {
        let mut finality = state.paper_chain_finality.write().await;
        let height_key = (checkpoint.chain_id.clone(), checkpoint.height);
        assert!(
            finality
                .preparations_v2
                .time_checkpoint_hash_by_chain_height
                .insert(height_key, checkpoint.checkpoint_hash.clone())
                .is_none(),
            "synthetic checkpoint height must be new"
        );
        assert!(
            finality
                .preparations_v2
                .time_checkpoints_by_hash
                .insert(checkpoint.checkpoint_hash.clone(), checkpoint.clone())
                .is_none(),
            "synthetic checkpoint hash must be new"
        );
    }
}

struct ArmedPaperFinalityV2Test {
    state: AppState,
    legacy: PaperTrnmCommandBindingV1,
    arm: PaperTrnmFinalityWindowArmV2,
}

async fn arm_paper_chain_finality_v2_window(mut state: AppState) -> ArmedPaperFinalityV2Test {
    set_paper_chain_finality_v2_clock(
        &mut state,
        PAPER_FINALITY_V2_FIXTURE_VERIFICATION_TIME_UNIX_S * 1_000,
    );
    let router = app(state.clone());
    admit_paper_chain_finality_v2_anchor(&router).await;
    let start_checkpoint = admit_paper_chain_finality_v2_start_checkpoint(&router).await;
    let legacy = seed_paper_chain_finality_test(state.clone()).await;
    let source_safe_start_time = u64::try_from(Utc::now().timestamp_millis())
        .expect("positive preparation source time")
        .checked_add(1_000)
        .expect("preparation source time overflow");
    let (source_safe_start_checkpoint, source_safe_start_proof) =
        synthetic_authenticated_checkpoint(
            &start_checkpoint,
            start_checkpoint.height + 1,
            source_safe_start_time,
            "preparation-source-safe-start",
        );
    seed_authenticated_chain_time_checkpoint(
        &state,
        &source_safe_start_checkpoint,
        &source_safe_start_proof,
    )
    .await;
    let start_checkpoint = source_safe_start_checkpoint;
    set_paper_chain_finality_v2_clock(&mut state, source_safe_start_time);
    let router = app(state.clone());
    let arm_path = format!(
        "/v2/hepta/papers/{}/chain-finality-v2/arm",
        legacy.paper_project_id
    );
    let arm_body = json!({
        "submission_id":legacy.submission_id,
        "evaluation_id":legacy.evaluation_id,
        "latest_reproduction_id":legacy.reproduction_id,
        "research_session_id":legacy.research_session_id,
        "research_session_roster_version":legacy.research_session_roster_version,
        "start_checkpoint_hash":start_checkpoint.checkpoint_hash,
        "idempotency_key":"paper-finality-v2-window-arm",
    });
    let created = assert_status(
        request(&router, "POST", &arm_path, arm_body.clone(), None).await,
        StatusCode::CREATED,
    );
    assert_eq!(
        assert_status(
            request(&router, "POST", &arm_path, arm_body.clone(), None).await,
            StatusCode::OK,
        ),
        created,
        "window-arm replay must be byte-for-byte stable"
    );
    let arm: PaperTrnmFinalityWindowArmV2 =
        serde_json::from_value(created).expect("decode V2 window arm");
    assert_eq!(arm.max_chain_time_lag_ms, PAPER_CHAIN_TIME_MAX_LAG_MS_V1);
    ArmedPaperFinalityV2Test { state, legacy, arm }
}

#[derive(Debug, Clone, Copy)]
enum ResolvedAppealScenarioV2 {
    Denied,
    Upheld,
}

impl ResolvedAppealScenarioV2 {
    fn label(self) -> &'static str {
        match self {
            Self::Denied => "denied",
            Self::Upheld => "upheld",
        }
    }

    fn outcome(self) -> &'static str {
        match self {
            Self::Denied => "denied",
            Self::Upheld => "upheld",
        }
    }

    fn expected_status(self) -> PaperTrnmAppealStatusV2 {
        match self {
            Self::Denied => PaperTrnmAppealStatusV2::ResolvedDenied,
            Self::Upheld => PaperTrnmAppealStatusV2::ResolvedUpheld,
        }
    }
}

fn update_legacy_binding_for_latest_review(
    legacy: &mut PaperTrnmCommandBindingV1,
    evaluation: &PaperEvaluation,
    reproduction: &PaperReproduction,
) {
    legacy.evaluation_id = evaluation.evaluation_id;
    legacy.tolerance_policy_hash = evaluation.tolerance_policy_hash.clone();
    legacy.evaluation_signing_hash = evaluation.evaluation_signing_hash.clone();
    legacy.evaluation_score_bps = evaluation.paper_score.score_bps;
    legacy.evaluation_accepted = evaluation.status == PaperEvaluationStatus::Accepted;
    legacy.evaluation_completed_at_unix_s = u64::try_from(evaluation.created_at.timestamp())
        .expect("positive replacement evaluation time");
    legacy.reproduction_id = reproduction.reproduction_id;
    legacy.reproduction_report_hash = reproduction.report_hash.clone();
}

async fn create_replacement_evaluation_for_finality_v2(
    router: &Router,
    legacy: &PaperTrnmCommandBindingV1,
    submission: &Value,
    previous_evaluation: &Value,
    evaluator: &Actor,
    reviewers: [&Actor; 2],
    label: &str,
) -> Value {
    let review_round = previous_evaluation["version"]
        .as_u64()
        .expect("previous evaluation version")
        .checked_add(1)
        .expect("replacement review round overflow");
    claim_evaluation_panel_fixture(
        router,
        legacy.paper_project_id,
        review_round,
        evaluator,
        reviewers,
        &format!("{label}-replacement"),
    )
    .await;
    let evaluation_id = Uuid::new_v4();
    let evaluation_key = format!("paper-finality-v2-{label}-replacement-evaluation");
    let evaluation = assert_status(
        user_post(
            router,
            evaluator,
            "create_paper_evaluation_v1",
            &format!("/v2/hepta/papers/{}/evaluations", legacy.paper_project_id),
            &evaluation_key,
            evaluation_body(
                legacy.paper_project_id,
                submission,
                evaluator,
                reviewers,
                evaluation_id,
                Some(legacy.evaluation_id),
                &evaluation_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(
        evaluation["supersedes_evaluation_id"],
        previous_evaluation["evaluation_id"]
    );
    evaluation
}

async fn create_activated_reproduction_for_finality_v2(
    router: &Router,
    legacy: &mut PaperTrnmCommandBindingV1,
    evaluation: &Value,
    reproducer: &Actor,
    label: &str,
) -> Value {
    let review_round = evaluation["version"]
        .as_u64()
        .expect("replacement evaluation version");
    let evaluation_id = Uuid::parse_str(
        evaluation["evaluation_id"]
            .as_str()
            .expect("replacement evaluation ID"),
    )
    .expect("canonical replacement evaluation ID");
    claim_review_assignment_fixture(
        router,
        reproducer,
        legacy.paper_project_id,
        review_round,
        "reproducer",
        &format!("{label}-replacement"),
    )
    .await;
    let reproduction_id = Uuid::new_v4();
    let reproduction_key = format!("paper-finality-v2-{label}-replacement-reproduction");
    let reproduction = assert_status(
        user_post(
            router,
            reproducer,
            "create_paper_reproduction_v1",
            &format!(
                "/v2/hepta/papers/{}/evaluations/{evaluation_id}/reproductions",
                legacy.paper_project_id
            ),
            &reproduction_key,
            reproduction_body(
                legacy.paper_project_id,
                evaluation,
                reproducer,
                reproduction_id,
                None,
                true,
                &reproduction_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    let evaluation_record: PaperEvaluation =
        serde_json::from_value(evaluation.clone()).expect("decode replacement evaluation");
    let reproduction_record: PaperReproduction =
        serde_json::from_value(reproduction.clone()).expect("decode replacement reproduction");
    update_legacy_binding_for_latest_review(legacy, &evaluation_record, &reproduction_record);
    reproduction
}

async fn exercise_resolved_appeal_paper_chain_finality_v2(
    mut state: AppState,
    scenario: ResolvedAppealScenarioV2,
) -> PaperTrnmFinalityPreparationV2 {
    set_paper_chain_finality_v2_clock(
        &mut state,
        PAPER_FINALITY_V2_FIXTURE_VERIFICATION_TIME_UNIX_S * 1_000,
    );
    let router = app(state.clone());
    admit_paper_chain_finality_v2_anchor(&router).await;
    let start_checkpoint = admit_paper_chain_finality_v2_start_checkpoint(&router).await;
    let mut legacy = seed_paper_chain_finality_test(state.clone()).await;
    let authors = actors(3);
    let external = actors(11);
    let submission = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_joint_paper_submission_v2",
            &format!("/v2/hepta/papers/{}/submission", legacy.paper_project_id),
            &format!("paper-finality-v2-{}-submission", scenario.label()),
        )
        .await,
        StatusCode::OK,
    );
    let review = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_paper_review_state_v1",
            &format!("/v2/hepta/papers/{}/review-state", legacy.paper_project_id),
            &format!("paper-finality-v2-{}-review", scenario.label()),
        )
        .await,
        StatusCode::OK,
    );
    let appealed_evaluation = review["evaluations"]
        .as_array()
        .expect("evaluation array")
        .iter()
        .find(|record| record["evaluation_id"] == legacy.evaluation_id.to_string())
        .cloned()
        .expect("seeded evaluation");
    let appealed_evaluation_id = legacy.evaluation_id;
    let appeal_id = Uuid::new_v4();
    let appeal_key = format!("paper-finality-v2-{}-appeal", scenario.label());
    assert_status(
        user_post(
            &router,
            &authors[0],
            "create_paper_appeal_v1",
            &format!(
                "/v2/hepta/papers/{}/evaluations/{appealed_evaluation_id}/appeals",
                legacy.paper_project_id
            ),
            &appeal_key,
            appeal_body(
                legacy.paper_project_id,
                &appealed_evaluation,
                &authors[0],
                appeal_id,
                &appeal_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );

    let replacement_evaluation = if matches!(scenario, ResolvedAppealScenarioV2::Upheld) {
        Some(
            create_replacement_evaluation_for_finality_v2(
                &router,
                &legacy,
                &submission,
                &appealed_evaluation,
                &external[6],
                [&external[7], &external[8]],
                scenario.label(),
            )
            .await,
        )
    } else {
        None
    };
    let replacement_evaluation_id = replacement_evaluation.as_ref().map(|evaluation| {
        Uuid::parse_str(
            evaluation["evaluation_id"]
                .as_str()
                .expect("replacement evaluation ID"),
        )
        .expect("canonical replacement evaluation ID")
    });
    let resolution_id = Uuid::new_v4();
    let resolution_key = format!("paper-finality-v2-{}-resolution", scenario.label());
    assert_status(
        user_post(
            &router,
            &external[9],
            "resolve_paper_appeal_v1",
            &format!(
                "/v2/hepta/papers/{}/appeals/{appeal_id}/resolve",
                legacy.paper_project_id
            ),
            &resolution_key,
            resolution_body(
                legacy.paper_project_id,
                &appealed_evaluation,
                appeal_id,
                &external[9],
                ResolutionBody {
                    resolution_id,
                    outcome: scenario.outcome(),
                    superseding_evaluation_id: replacement_evaluation_id,
                    idempotency_key: &resolution_key,
                },
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    if let Some(replacement_evaluation) = replacement_evaluation.as_ref() {
        create_activated_reproduction_for_finality_v2(
            &router,
            &mut legacy,
            replacement_evaluation,
            &external[3],
            scenario.label(),
        )
        .await;
    }

    // The repository's real CometBFT proof predates these dynamically-created
    // review records.  Keep that proof as the admission boundary, then advance
    // to an already-authenticated repository checkpoint so the resolved-Appeal
    // branch is tested without allowing a Chain-time timestamp regression.
    let source_safe_start_time = u64::try_from(Utc::now().timestamp_millis())
        .expect("positive resolved-Appeal source time")
        .checked_add(1_000)
        .expect("resolved-Appeal source time overflow");
    let (source_safe_start_checkpoint, source_safe_start_proof) =
        synthetic_authenticated_checkpoint(
            &start_checkpoint,
            start_checkpoint.height + 1,
            source_safe_start_time,
            &format!("{}-source-safe-start", scenario.label()),
        );
    seed_authenticated_chain_time_checkpoint(
        &state,
        &source_safe_start_checkpoint,
        &source_safe_start_proof,
    )
    .await;
    let start_checkpoint = source_safe_start_checkpoint;
    set_paper_chain_finality_v2_clock(&mut state, source_safe_start_time);
    let router = app(state.clone());

    let arm_path = format!(
        "/v2/hepta/papers/{}/chain-finality-v2/arm",
        legacy.paper_project_id
    );
    let arm_body = json!({
        "submission_id":legacy.submission_id,
        "evaluation_id":legacy.evaluation_id,
        "latest_reproduction_id":legacy.reproduction_id,
        "research_session_id":legacy.research_session_id,
        "research_session_roster_version":legacy.research_session_roster_version,
        "start_checkpoint_hash":start_checkpoint.checkpoint_hash,
        "idempotency_key":format!(
            "paper-finality-v2-{}-window-arm",
            scenario.label()
        ),
    });
    let created_arm = assert_status(
        request(&router, "POST", &arm_path, arm_body.clone(), None).await,
        StatusCode::CREATED,
    );
    assert_eq!(
        assert_status(
            request(&router, "POST", &arm_path, arm_body, None).await,
            StatusCode::OK,
        ),
        created_arm,
        "resolved-Appeal window-arm replay must be byte-for-byte stable"
    );
    let arm: PaperTrnmFinalityWindowArmV2 =
        serde_json::from_value(created_arm).expect("decode resolved-Appeal V2 window arm");
    assert_eq!(arm.appeal_status, scenario.expected_status());
    assert_eq!(arm.appeal_id, Some(appeal_id));
    assert_eq!(arm.appeal_resolution_id, Some(resolution_id));

    let (final_checkpoint, final_proof) = synthetic_authenticated_checkpoint(
        &arm.start_checkpoint,
        arm.start_checkpoint.height + 1,
        arm.earliest_final_checkpoint_time_unix_ms,
        scenario.label(),
    );
    seed_authenticated_chain_time_checkpoint(&state, &final_checkpoint, &final_proof).await;
    let preparation_path = format!(
        "/v2/hepta/papers/{}/chain-finality-v2/prepare",
        legacy.paper_project_id
    );
    let preparation_body = json!({
        "arm_id":arm.arm_id,
        "submission_id":legacy.submission_id,
        "evaluation_id":legacy.evaluation_id,
        "latest_reproduction_id":legacy.reproduction_id,
        "research_session_id":legacy.research_session_id,
        "research_session_roster_version":legacy.research_session_roster_version,
        "final_checkpoint_hash":final_checkpoint.checkpoint_hash,
        "idempotency_key":format!(
            "paper-finality-v2-{}-preparation",
            scenario.label()
        ),
    });
    let created_preparation = assert_status(
        request(
            &router,
            "POST",
            &preparation_path,
            preparation_body.clone(),
            None,
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(
        assert_status(
            request(&router, "POST", &preparation_path, preparation_body, None,).await,
            StatusCode::OK,
        ),
        created_preparation,
        "resolved-Appeal preparation replay must be byte-for-byte stable"
    );
    let preparation: PaperTrnmFinalityPreparationV2 =
        serde_json::from_value(created_preparation).expect("decode resolved-Appeal V2 preparation");
    assert_eq!(
        preparation.binding.appeal_status,
        scenario.expected_status()
    );
    assert_eq!(preparation.binding.appeal_id, Some(appeal_id));
    assert_eq!(
        preparation.binding.appealed_evaluation_id,
        Some(appealed_evaluation_id)
    );
    assert_eq!(
        preparation.binding.appeal_resolution_id,
        Some(resolution_id)
    );
    assert!(preparation.binding.scientific_finality);
    assert_eq!(
        (
            preparation.binding.score_eligible,
            preparation.binding.ranking_eligible,
            preparation.binding.reward_eligible,
            preparation.binding.economic_eligible,
        ),
        (false, false, false, false)
    );
    match scenario {
        ResolvedAppealScenarioV2::Denied => {
            assert_eq!(preparation.binding.evaluation_id, appealed_evaluation_id);
            assert_eq!(
                preparation.binding.evaluation_supersedes_evaluation_id,
                None
            );
        }
        ResolvedAppealScenarioV2::Upheld => {
            assert_ne!(preparation.binding.evaluation_id, appealed_evaluation_id);
            assert_eq!(
                preparation.binding.evaluation_supersedes_evaluation_id,
                Some(appealed_evaluation_id)
            );
        }
    }
    preparation
}

pub(crate) async fn exercise_paper_chain_finality_v2_preparation(
    state: AppState,
) -> PaperTrnmFinalityPreparationV2 {
    let armed = arm_paper_chain_finality_v2_window(state).await;
    let mut state = armed.state;
    let legacy = armed.legacy;
    let arm = armed.arm;
    let path = format!(
        "/v2/hepta/papers/{}/chain-finality-v2/prepare",
        legacy.paper_project_id
    );
    let early_time = arm
        .earliest_final_checkpoint_time_unix_ms
        .checked_sub(1)
        .expect("positive V2 Chain-time deadline");
    let (early_checkpoint, early_proof) = synthetic_authenticated_checkpoint(
        &arm.start_checkpoint,
        arm.start_checkpoint.height + 1,
        early_time,
        "deadline-minus-one",
    );
    seed_authenticated_chain_time_checkpoint(&state, &early_checkpoint, &early_proof).await;
    let mut body = json!({
        "arm_id":arm.arm_id,
        "submission_id":legacy.submission_id,
        "evaluation_id":legacy.evaluation_id,
        "latest_reproduction_id":legacy.reproduction_id,
        "research_session_id":legacy.research_session_id,
        "research_session_roster_version":legacy.research_session_roster_version,
        "final_checkpoint_hash":early_checkpoint.checkpoint_hash,
        "idempotency_key":"paper-finality-v2-preparation",
    });
    let router = app(state.clone());
    let unauthenticated = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&path)
                .header("content-type", "application/json")
                .body(Body::from("{"))
                .expect("malformed unauthenticated request"),
        )
        .await
        .expect("unauthenticated response");
    assert_eq!(
        unauthenticated.status(),
        StatusCode::FORBIDDEN,
        "operator authentication must run before malformed JSON is read"
    );
    let unauthenticated_body = to_bytes(unauthenticated.into_body(), usize::MAX)
        .await
        .expect("unauthenticated response body");
    let unauthenticated_json: Value =
        serde_json::from_slice(&unauthenticated_body).expect("unauthenticated JSON response");
    assert_eq!(unauthenticated_json["code"], "operator_auth_failed");

    set_paper_chain_finality_v2_clock(
        &mut state,
        (PAPER_FINALITY_V2_FIXTURE_VERIFICATION_TIME_UNIX_S + 30 * 24 * 60 * 60) * 1_000,
    );
    let forward_clock_router = app(state.clone());
    assert_eq!(
        error_code(
            request(&forward_clock_router, "POST", &path, body.clone(), None).await,
            StatusCode::CONFLICT,
        ),
        "paper_chain_finality_appeal_window_open",
        "a host-clock jump must not close a Chain-time Appeal window at deadline - 1ms"
    );

    let (final_checkpoint, final_proof) = synthetic_authenticated_checkpoint(
        &arm.start_checkpoint,
        arm.start_checkpoint.height + 2,
        arm.earliest_final_checkpoint_time_unix_ms,
        "deadline",
    );
    seed_authenticated_chain_time_checkpoint(&state, &final_checkpoint, &final_proof).await;
    body["final_checkpoint_hash"] = json!(final_checkpoint.checkpoint_hash);
    set_paper_chain_finality_v2_clock(&mut state, 1);
    let router = app(state.clone());
    let created = assert_status(
        request(&router, "POST", &path, body.clone(), None).await,
        StatusCode::CREATED,
    );
    assert_eq!(
        created["binding"]["final_checkpoint_consensus_time_unix_ms"],
        arm.earliest_final_checkpoint_time_unix_ms,
        "deadline Chain time must be accepted even after the host clock rolls backward"
    );
    assert_eq!(
        created["binding"]["appeal_window_closes_at_unix_ms"],
        arm.earliest_final_checkpoint_time_unix_ms
    );
    assert_eq!(
        created["status"], "awaiting_chain_verifier_upgrade",
        "preparation must not masquerade as a queued or verified Chain command"
    );
    assert_eq!(created["binding"]["scientific_finality"], true);
    for field in [
        "score_eligible",
        "ranking_eligible",
        "reward_eligible",
        "economic_eligible",
    ] {
        assert_eq!(created["binding"][field], false, "{field} must fail closed");
    }
    assert_eq!(
        created["binding"]["settlement_policy_hash"],
        paper_scientific_finality_policy_hash_v1().expect("frozen policy hash")
    );
    assert_eq!(
        assert_status(
            request(&router, "POST", &path, body.clone(), None).await,
            StatusCode::OK,
        ),
        created,
        "preparation replay must be byte-for-byte stable"
    );
    let mut duplicate_facts = body.clone();
    duplicate_facts["idempotency_key"] = json!("paper-finality-v2-preparation-duplicate");
    let expected_duplicate_code = if state.pool.is_some() {
        "paper_chain_finality_v2_source_sealed"
    } else {
        "paper_trnm_v2_preparation_exists"
    };
    assert_eq!(
        error_code(
            request(&router, "POST", &path, duplicate_facts, None).await,
            StatusCode::CONFLICT,
        ),
        expected_duplicate_code,
        "a new idempotency key must not mint another commitment for the same final facts"
    );
    let mut conflict = body.clone();
    conflict["latest_reproduction_id"] = json!(Uuid::new_v4());
    assert_eq!(
        error_code(
            request(&router, "POST", &path, conflict, None).await,
            StatusCode::CONFLICT,
        ),
        "paper_trnm_v2_idempotency_conflict"
    );

    let preparation: PaperTrnmFinalityPreparationV2 =
        serde_json::from_value(created.clone()).expect("decode V2 preparation");
    let mut zero_score = preparation.binding.clone();
    zero_score.evaluation_accepted = true;
    zero_score.evaluation_score_bps = 0;
    assert_eq!(
        validate_binding_v2(&zero_score)
            .expect_err("accepted evaluation with zero score must fail closed")
            .code,
        "paper_trnm_v2_finality_invariant_failed"
    );
    let mut independent_scientific_facts = preparation.binding.clone();
    independent_scientific_facts.evaluation_accepted = false;
    independent_scientific_facts.latest_reproduction_accepted = true;
    independent_scientific_facts.commitment_id =
        binding_commitment_id_v2(&independent_scientific_facts)
            .expect("derive independent-facts commitment id");
    validate_binding_v2(&independent_scientific_facts)
        .expect("evaluation acceptance and reproduction success are independent scientific facts");

    let authors = actors(3);
    let external = actors(11);
    let submission = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_joint_paper_submission_v2",
            &format!("/v2/hepta/papers/{}/submission", legacy.paper_project_id),
            "paper-finality-v2-sealed-submission",
        )
        .await,
        StatusCode::OK,
    );
    let review = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_paper_review_state_v1",
            &format!("/v2/hepta/papers/{}/review-state", legacy.paper_project_id),
            "paper-finality-v2-sealed-review",
        )
        .await,
        StatusCode::OK,
    );
    let evaluation = review["evaluations"]
        .as_array()
        .expect("evaluation array")
        .iter()
        .find(|record| record["evaluation_id"] == legacy.evaluation_id.to_string())
        .cloned()
        .expect("seeded evaluation in review state");
    let late_evaluation_key = "paper-finality-v2-late-evaluation";
    assert_eq!(
        error_code(
            user_post(
                &router,
                &external[6],
                "create_paper_evaluation_v1",
                &format!("/v2/hepta/papers/{}/evaluations", legacy.paper_project_id),
                late_evaluation_key,
                evaluation_body(
                    legacy.paper_project_id,
                    &submission,
                    &external[6],
                    [&external[7], &external[8]],
                    Uuid::new_v4(),
                    Some(legacy.evaluation_id),
                    late_evaluation_key,
                ),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "paper_chain_finality_v2_source_sealed"
    );
    let late_reproduction_key = "paper-finality-v2-late-reproduction";
    assert_eq!(
        error_code(
            user_post(
                &router,
                &external[3],
                "create_paper_reproduction_v1",
                &format!(
                    "/v2/hepta/papers/{}/evaluations/{}/reproductions",
                    legacy.paper_project_id, legacy.evaluation_id
                ),
                late_reproduction_key,
                reproduction_body(
                    legacy.paper_project_id,
                    &evaluation,
                    &external[3],
                    Uuid::new_v4(),
                    Some(legacy.reproduction_id),
                    true,
                    late_reproduction_key,
                ),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "paper_chain_finality_v2_source_sealed"
    );
    let late_appeal_key = "paper-finality-v2-late-appeal";
    assert_eq!(
        error_code(
            user_post(
                &router,
                &authors[0],
                "create_paper_appeal_v1",
                &format!(
                    "/v2/hepta/papers/{}/evaluations/{}/appeals",
                    legacy.paper_project_id, legacy.evaluation_id
                ),
                late_appeal_key,
                appeal_body(
                    legacy.paper_project_id,
                    &evaluation,
                    &authors[0],
                    Uuid::new_v4(),
                    late_appeal_key,
                ),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "paper_chain_finality_v2_source_sealed"
    );
    assert_eq!(
        assert_status(
            request(&router, "POST", &path, body, None).await,
            StatusCode::OK,
        ),
        created,
        "sealed source mutations must not make exact preparation replay stale"
    );
    preparation
}

#[tokio::test]
async fn memory_paper_chain_finality_v2_preparation_enforces_policy_and_fail_closed_status() {
    let preparation = exercise_paper_chain_finality_v2_preparation(AppState::new(
        paper_chain_finality_v2_security(),
    ))
    .await;
    assert_eq!(
        preparation.binding.appeal_status,
        PaperTrnmAppealStatusV2::ClosedNoAppeal
    );
    assert_eq!(
        preparation.binding.evaluation_superseded_by_evaluation_id,
        None
    );
    assert_eq!(
        preparation
            .binding
            .reproduction_superseded_by_reproduction_id,
        None
    );
}

#[tokio::test]
async fn memory_paper_chain_finality_v2_resolved_appeal_branches_reach_preparation() {
    for scenario in [
        ResolvedAppealScenarioV2::Denied,
        ResolvedAppealScenarioV2::Upheld,
    ] {
        exercise_resolved_appeal_paper_chain_finality_v2(
            AppState::new(paper_chain_finality_v2_security()),
            scenario,
        )
        .await;
    }
}

#[derive(Debug, Clone, Copy)]
enum AppealLineageScenarioV2 {
    AncestorOpen,
    TerminalDeniedAfterUpheld,
    MultiGenerationUpheld,
}

impl AppealLineageScenarioV2 {
    fn label(self) -> &'static str {
        match self {
            Self::AncestorOpen => "ancestor-open",
            Self::TerminalDeniedAfterUpheld => "terminal-denied-after-upheld",
            Self::MultiGenerationUpheld => "multi-generation-upheld",
        }
    }
}

async fn exercise_paper_chain_finality_v2_appeal_lineage(
    mut state: AppState,
    scenario: AppealLineageScenarioV2,
) {
    set_paper_chain_finality_v2_clock(
        &mut state,
        PAPER_FINALITY_V2_FIXTURE_VERIFICATION_TIME_UNIX_S * 1_000,
    );
    let router = app(state.clone());
    admit_paper_chain_finality_v2_anchor(&router).await;
    let start_checkpoint = admit_paper_chain_finality_v2_start_checkpoint(&router).await;
    let mut legacy = seed_paper_chain_finality_test(state.clone()).await;
    let authors = actors(3);
    let external = actors(15);
    create_players_and_bindings(&router, &external).await;
    let submission = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_joint_paper_submission_v2",
            &format!("/v2/hepta/papers/{}/submission", legacy.paper_project_id),
            &format!("paper-finality-v2-{}-submission", scenario.label()),
        )
        .await,
        StatusCode::OK,
    );
    let review = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_paper_review_state_v1",
            &format!("/v2/hepta/papers/{}/review-state", legacy.paper_project_id),
            &format!("paper-finality-v2-{}-review", scenario.label()),
        )
        .await,
        StatusCode::OK,
    );
    let original_evaluation = review["evaluations"]
        .as_array()
        .expect("evaluation array")
        .iter()
        .find(|record| record["evaluation_id"] == legacy.evaluation_id.to_string())
        .cloned()
        .expect("seeded evaluation");
    let original_evaluation_id = legacy.evaluation_id;
    let original_appeal_id = Uuid::new_v4();
    let original_appeal_key = format!("paper-finality-v2-{}-appeal-1", scenario.label());
    assert_status(
        user_post(
            &router,
            &authors[0],
            "create_paper_appeal_v1",
            &format!(
                "/v2/hepta/papers/{}/evaluations/{original_evaluation_id}/appeals",
                legacy.paper_project_id
            ),
            &original_appeal_key,
            appeal_body(
                legacy.paper_project_id,
                &original_evaluation,
                &authors[0],
                original_appeal_id,
                &original_appeal_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    let mut expected_positive_lineage = None;
    let replacement_evaluation = if !matches!(scenario, AppealLineageScenarioV2::AncestorOpen) {
        Some(
            create_replacement_evaluation_for_finality_v2(
                &router,
                &legacy,
                &submission,
                &original_evaluation,
                &external[6],
                [&external[7], &external[8]],
                scenario.label(),
            )
            .await,
        )
    } else {
        None
    };

    if !matches!(scenario, AppealLineageScenarioV2::AncestorOpen) {
        let replacement_evaluation = replacement_evaluation
            .as_ref()
            .expect("resolved-lineage fixture prepares a replacement evaluation");
        let replacement_evaluation_id = Uuid::parse_str(
            replacement_evaluation["evaluation_id"]
                .as_str()
                .expect("replacement evaluation ID"),
        )
        .expect("canonical replacement evaluation ID");
        let premature_appeal_key = format!(
            "paper-finality-v2-{}-premature-child-appeal",
            scenario.label()
        );
        let premature_appeal_error = error_code(
            user_post(
                &router,
                &authors[1],
                "create_paper_appeal_v1",
                &format!(
                    "/v2/hepta/papers/{}/evaluations/{replacement_evaluation_id}/appeals",
                    legacy.paper_project_id
                ),
                &premature_appeal_key,
                appeal_body(
                    legacy.paper_project_id,
                    replacement_evaluation,
                    &authors[1],
                    Uuid::new_v4(),
                    &premature_appeal_key,
                ),
            )
            .await,
            StatusCode::CONFLICT,
        );
        assert_eq!(
            premature_appeal_error, "paper_evaluation_not_activated",
            "a prepared child must not acquire its own Appeal before the exact parent uphold activates it"
        );
        let first_resolution_key = format!("paper-finality-v2-{}-resolution-1", scenario.label());
        assert_status(
            user_post(
                &router,
                &external[9],
                "resolve_paper_appeal_v1",
                &format!(
                    "/v2/hepta/papers/{}/appeals/{original_appeal_id}/resolve",
                    legacy.paper_project_id
                ),
                &first_resolution_key,
                resolution_body(
                    legacy.paper_project_id,
                    &original_evaluation,
                    original_appeal_id,
                    &external[9],
                    ResolutionBody {
                        resolution_id: Uuid::new_v4(),
                        outcome: "upheld",
                        superseding_evaluation_id: Some(replacement_evaluation_id),
                        idempotency_key: &first_resolution_key,
                    },
                ),
            )
            .await,
            StatusCode::CREATED,
        );
        create_activated_reproduction_for_finality_v2(
            &router,
            &mut legacy,
            replacement_evaluation,
            &external[3],
            scenario.label(),
        )
        .await;
        let second_appeal_id = Uuid::new_v4();
        let second_appeal_key = format!("paper-finality-v2-{}-appeal-2", scenario.label());
        assert_status(
            user_post(
                &router,
                &authors[1],
                "create_paper_appeal_v1",
                &format!(
                    "/v2/hepta/papers/{}/evaluations/{}/appeals",
                    legacy.paper_project_id, legacy.evaluation_id
                ),
                &second_appeal_key,
                appeal_body(
                    legacy.paper_project_id,
                    replacement_evaluation,
                    &authors[1],
                    second_appeal_id,
                    &second_appeal_key,
                ),
            )
            .await,
            StatusCode::CREATED,
        );
        let second_replacement =
            if matches!(scenario, AppealLineageScenarioV2::MultiGenerationUpheld) {
                Some(
                    create_replacement_evaluation_for_finality_v2(
                        &router,
                        &legacy,
                        &submission,
                        replacement_evaluation,
                        &external[10],
                        [&external[11], &external[12]],
                        &format!("{}-generation-3", scenario.label()),
                    )
                    .await,
                )
            } else {
                None
            };
        let second_replacement_id = second_replacement.as_ref().map(|record| {
            Uuid::parse_str(
                record["evaluation_id"]
                    .as_str()
                    .expect("generation-3 evaluation ID"),
            )
            .expect("canonical generation-3 evaluation ID")
        });
        let second_resolution_id = Uuid::new_v4();
        let second_resolution_key = format!("paper-finality-v2-{}-resolution-2", scenario.label());
        assert_status(
            user_post(
                &router,
                &external[9],
                "resolve_paper_appeal_v1",
                &format!(
                    "/v2/hepta/papers/{}/appeals/{second_appeal_id}/resolve",
                    legacy.paper_project_id
                ),
                &second_resolution_key,
                resolution_body(
                    legacy.paper_project_id,
                    replacement_evaluation,
                    second_appeal_id,
                    &external[9],
                    ResolutionBody {
                        resolution_id: second_resolution_id,
                        outcome: if second_replacement_id.is_some() {
                            "upheld"
                        } else {
                            "denied"
                        },
                        superseding_evaluation_id: second_replacement_id,
                        idempotency_key: &second_resolution_key,
                    },
                ),
            )
            .await,
            StatusCode::CREATED,
        );
        if let Some(second_replacement) = second_replacement.as_ref() {
            create_activated_reproduction_for_finality_v2(
                &router,
                &mut legacy,
                second_replacement,
                &external[13],
                &format!("{}-generation-3", scenario.label()),
            )
            .await;
        }
        expected_positive_lineage = Some((
            if second_replacement.is_some() {
                PaperTrnmAppealStatusV2::ResolvedUpheld
            } else {
                PaperTrnmAppealStatusV2::ResolvedDenied
            },
            second_appeal_id,
            second_resolution_id,
        ));
    }

    if let Some((expected_status, expected_appeal_id, expected_resolution_id)) =
        expected_positive_lineage
    {
        let source_safe_start_time = u64::try_from(Utc::now().timestamp_millis())
            .expect("positive multi-generation source time")
            .checked_add(1_000)
            .expect("multi-generation source time overflow");
        let (source_safe_start_checkpoint, source_safe_start_proof) =
            synthetic_authenticated_checkpoint(
                &start_checkpoint,
                start_checkpoint.height + 1,
                source_safe_start_time,
                &format!("{}-source-safe-start", scenario.label()),
            );
        seed_authenticated_chain_time_checkpoint(
            &state,
            &source_safe_start_checkpoint,
            &source_safe_start_proof,
        )
        .await;
        set_paper_chain_finality_v2_clock(&mut state, source_safe_start_time);
        let router = app(state.clone());
        let arm = assert_status(
            request(
                &router,
                "POST",
                &format!(
                    "/v2/hepta/papers/{}/chain-finality-v2/arm",
                    legacy.paper_project_id
                ),
                json!({
                    "submission_id":legacy.submission_id,
                    "evaluation_id":legacy.evaluation_id,
                    "latest_reproduction_id":legacy.reproduction_id,
                    "research_session_id":legacy.research_session_id,
                    "research_session_roster_version":legacy.research_session_roster_version,
                    "start_checkpoint_hash":source_safe_start_checkpoint.checkpoint_hash,
                    "idempotency_key":format!(
                        "paper-finality-v2-{}-window-arm",
                        scenario.label()
                    ),
                }),
                None,
            )
            .await,
            StatusCode::CREATED,
        );
        assert_eq!(
            serde_json::from_value::<PaperTrnmAppealStatusV2>(arm["appeal_status"].clone())
                .expect("decode multi-generation Appeal status"),
            expected_status
        );
        let expected_appeal_id = expected_appeal_id.to_string();
        let expected_resolution_id = expected_resolution_id.to_string();
        assert_eq!(arm["appeal_id"].as_str(), Some(expected_appeal_id.as_str()));
        assert_eq!(
            arm["appeal_resolution_id"].as_str(),
            Some(expected_resolution_id.as_str())
        );
        return;
    }

    let before = paper_finality_side_effect_snapshot(&state).await;
    let error = error_code(
        request(
            &router,
            "POST",
            &format!(
                "/v2/hepta/papers/{}/chain-finality-v2/arm",
                legacy.paper_project_id
            ),
            json!({
                "submission_id":legacy.submission_id,
                "evaluation_id":legacy.evaluation_id,
                "latest_reproduction_id":legacy.reproduction_id,
                "research_session_id":legacy.research_session_id,
                "research_session_roster_version":legacy.research_session_roster_version,
                "start_checkpoint_hash":start_checkpoint.checkpoint_hash,
                "idempotency_key":format!(
                    "paper-finality-v2-{}-window-arm",
                    scenario.label()
                ),
            }),
            None,
        )
        .await,
        StatusCode::CONFLICT,
    );
    assert_eq!(error, "paper_chain_finality_open_appeal");
    let after = paper_finality_side_effect_snapshot(&state).await;
    assert_eq!(
        after, before,
        "Appeal lineage rejection must leave every reachable memory/PG side-effect surface unchanged"
    );
}

#[tokio::test]
async fn memory_paper_chain_finality_v2_appeal_lineage_outcomes_are_strict() {
    for scenario in [
        AppealLineageScenarioV2::AncestorOpen,
        AppealLineageScenarioV2::TerminalDeniedAfterUpheld,
        AppealLineageScenarioV2::MultiGenerationUpheld,
    ] {
        exercise_paper_chain_finality_v2_appeal_lineage(
            AppState::new(paper_chain_finality_v2_security()),
            scenario,
        )
        .await;
    }
}

async fn exercise_paper_chain_finality_v2_arm_source_drift(state: AppState) {
    let armed = arm_paper_chain_finality_v2_window(state).await;
    let state = armed.state;
    let legacy = armed.legacy;
    let arm = armed.arm;
    let router = app(state.clone());
    let authors = actors(3);
    let review = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_paper_review_state_v1",
            &format!("/v2/hepta/papers/{}/review-state", legacy.paper_project_id),
            "paper-finality-v2-arm-drift-review",
        )
        .await,
        StatusCode::OK,
    );
    let evaluation = review["evaluations"]
        .as_array()
        .expect("evaluation array")
        .iter()
        .find(|record| record["evaluation_id"] == legacy.evaluation_id.to_string())
        .cloned()
        .expect("armed evaluation in review state");
    let appeal_key = "paper-finality-v2-arm-drift-appeal";
    assert_status(
        user_post(
            &router,
            &authors[0],
            "create_paper_appeal_v1",
            &format!(
                "/v2/hepta/papers/{}/evaluations/{}/appeals",
                legacy.paper_project_id, legacy.evaluation_id
            ),
            appeal_key,
            appeal_body(
                legacy.paper_project_id,
                &evaluation,
                &authors[0],
                Uuid::new_v4(),
                appeal_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );

    let (final_checkpoint, final_proof) = synthetic_authenticated_checkpoint(
        &arm.start_checkpoint,
        arm.start_checkpoint.height + 1,
        arm.earliest_final_checkpoint_time_unix_ms,
        "source-drift",
    );
    seed_authenticated_chain_time_checkpoint(&state, &final_checkpoint, &final_proof).await;
    let path = format!(
        "/v2/hepta/papers/{}/chain-finality-v2/prepare",
        legacy.paper_project_id
    );
    let before = paper_finality_side_effect_snapshot(&state).await;
    assert_eq!(
        error_code(
            request(
                &router,
                "POST",
                &path,
                json!({
                    "arm_id":arm.arm_id,
                    "submission_id":legacy.submission_id,
                    "evaluation_id":legacy.evaluation_id,
                    "latest_reproduction_id":legacy.reproduction_id,
                    "research_session_id":legacy.research_session_id,
                    "research_session_roster_version":legacy.research_session_roster_version,
                    "final_checkpoint_hash":final_checkpoint.checkpoint_hash,
                    "idempotency_key":"paper-finality-v2-source-drift-prepare",
                }),
                None,
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "paper_trnm_v2_window_arm_stale",
        "an Appeal opened after arm must invalidate the observed source tuple"
    );
    let after = paper_finality_side_effect_snapshot(&state).await;
    assert_eq!(
        after, before,
        "source drift must not mutate any reachable memory/PG side-effect surface"
    );
    assert!(
        !state
            .paper_chain_finality
            .read()
            .await
            .preparations_v2
            .by_paper_id
            .contains_key(&legacy.paper_project_id),
        "source drift must not leave a partial immutable preparation"
    );
}

#[tokio::test]
async fn memory_paper_chain_finality_v2_arm_is_invalidated_by_source_drift() {
    exercise_paper_chain_finality_v2_arm_source_drift(AppState::new(
        paper_chain_finality_v2_security(),
    ))
    .await;
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
    evaluation_body_with_acceptance(
        paper_id,
        submission,
        evaluator,
        reviewers,
        evaluation_id,
        supersedes_evaluation_id,
        idempotency_key,
        true,
    )
}

fn rejected_evaluation_body(
    paper_id: Uuid,
    submission: &Value,
    evaluator: &Actor,
    reviewers: [&Actor; 2],
    evaluation_id: Uuid,
    idempotency_key: &str,
) -> Value {
    evaluation_body_with_acceptance(
        paper_id,
        submission,
        evaluator,
        reviewers,
        evaluation_id,
        None,
        idempotency_key,
        false,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "the test fixture mirrors every exact signed Paper Rework V1 field"
)]
fn paper_rework_body(
    paper_id: Uuid,
    rejected_submission_id: Uuid,
    rejected_evaluation_id: Uuid,
    expected_paper_version: u64,
    rework_id: Uuid,
    rework_cycle: u64,
    author: &Actor,
    reason_hash: String,
    signed_at_unix: i64,
    idempotency_key: &str,
) -> Value {
    let signing = PaperReworkSigningV1 {
        schema: PAPER_REWORK_V1.to_string(),
        rework_id,
        paper_project_id: paper_id,
        rejected_evaluation_id,
        rejected_submission_id,
        expected_paper_version,
        rework_cycle,
        author_player_id: author.player_id,
        signing_key_id: author.human_key_id.clone(),
        signing_public_key_hash: author.human_public_key_hash.clone(),
        reason_hash: reason_hash.clone(),
        signed_at_unix,
    };
    let frame = paper_rework_signing_bytes(&signing).expect("Paper rework signing frame");
    json!({
        "rework_id":rework_id,
        "rejected_evaluation_id":rejected_evaluation_id,
        "rejected_submission_id":rejected_submission_id,
        "expected_paper_version":expected_paper_version,
        "rework_cycle":rework_cycle,
        "author_player_id":author.player_id,
        "signing_key_id":author.human_key_id,
        "signing_public_key":author.human_public_key,
        "signing_public_key_hash":author.human_public_key_hash,
        "reason_hash":reason_hash,
        "signed_at_unix":signed_at_unix,
        "signature":BASE64.encode(author.human_key.sign(&frame).to_bytes()),
        "idempotency_key":idempotency_key,
    })
}

async fn seed_rework_release_candidate_memory(
    state: &AppState,
    rejected_submission: &JointPaperSubmission,
    authors: &[Actor],
    change_scientific_content: bool,
) -> (Uuid, String, u64) {
    let now = Utc::now();
    let mut memory = state.paper_raid.write().await;
    let rejected_revision = memory
        .revisions
        .get(&rejected_submission.revision_id)
        .cloned()
        .expect("rejected revision fixture");
    let revision_id = Uuid::new_v4();
    let mut candidate = rejected_submission.paper_bundle.release_candidate.clone();
    candidate.revision_id = revision_id;
    if change_scientific_content {
        candidate.claim_evidence_graph_hash = digest("reworked-claim-evidence-graph");
    }
    let release_candidate_hash =
        paper_release_candidate_hash(&candidate).expect("rework release candidate hash");
    let revision = PaperRevision {
        revision_id,
        paper_project_id: rejected_submission.paper_project_id,
        parent_revision_id: Some(rejected_submission.revision_id),
        revision_number: rejected_revision
            .revision_number
            .checked_add(1)
            .expect("fixture revision number"),
        source_manifest_hash: candidate.source_manifest_hash.clone(),
        artifact_manifest_hash: candidate.artifact_manifest_hash.clone(),
        bibliography_hash: candidate.bibliography_hash.clone(),
        claim_evidence_graph_hash: candidate.claim_evidence_graph_hash.clone(),
        section_materialization: rejected_revision.section_materialization.clone(),
        section_materialization_root: rejected_revision.section_materialization_root.clone(),
        status: PaperRevisionStatus::ReleaseCandidate,
        release_candidate: Some(candidate),
        release_candidate_hash: Some(release_candidate_hash.clone()),
        version: 2,
        created_at: now,
        updated_at: now,
    };
    memory.revisions.insert(revision_id, revision);
    for author in authors {
        let consent_id = Uuid::new_v4();
        let signing = AuthorshipConsentSigningV2 {
            schema: AUTHORSHIP_CONSENT_V2.to_string(),
            consent_id,
            paper_project_id: rejected_submission.paper_project_id,
            revision_id,
            player_id: author.player_id,
            signing_key_id: author.human_key_id.clone(),
            signing_public_key: author.human_public_key.clone(),
            signing_public_key_hash: author.human_public_key_hash.clone(),
            release_candidate_hash: release_candidate_hash.clone(),
            signed_at_unix: now.timestamp(),
        };
        memory.consents.insert(
            consent_id,
            AuthorshipConsent {
                consent_id,
                paper_project_id: rejected_submission.paper_project_id,
                revision_id,
                player_id: author.player_id,
                signing_key_id: author.human_key_id.clone(),
                signing_public_key: author.human_public_key.clone(),
                signing_public_key_hash: author.human_public_key_hash.clone(),
                release_candidate_hash: release_candidate_hash.clone(),
                signed_at: now,
                signature: sign_authorship_consent(&signing, &author.human_key)
                    .expect("rework authorship consent signature"),
            },
        );
    }
    let paper = memory
        .papers
        .get_mut(&rejected_submission.paper_project_id)
        .expect("rework Paper fixture");
    paper.current_revision_id = Some(revision_id);
    paper.release_candidate_revision_id = Some(revision_id);
    paper.phase = PaperPhase::AuthorApproval;
    paper.version = paper.version.checked_add(1).expect("fixture Paper version");
    paper.updated_at = now;
    (revision_id, release_candidate_hash, paper.version)
}

#[allow(clippy::too_many_arguments)]
fn evaluation_body_with_acceptance(
    paper_id: Uuid,
    submission: &Value,
    evaluator: &Actor,
    reviewers: [&Actor; 2],
    evaluation_id: Uuid,
    supersedes_evaluation_id: Option<Uuid>,
    idempotency_key: &str,
    accepted: bool,
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
        .enumerate()
        .map(|(index, reviewer)| {
            let attestation_id = Uuid::new_v4();
            let reviewer_signed_at = Utc::now().timestamp();
            let coi = digest(&format!("coi-reviewer-{}", reviewer.player_id));
            let verdict = if accepted || index > 0 {
                "approve"
            } else {
                "reject"
            };
            let signing = PaperReviewAttestationSigningV1 {
                schema: PAPER_REVIEW_ATTESTATION_V1.to_string(),
                attestation_id,
                evaluation_id,
                evaluation_signing_hash: evaluation_signing_hash.clone(),
                reviewer_player_id: reviewer.player_id,
                verdict: verdict.to_string(),
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
                "verdict":verdict,
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

fn evaluation_draft_body(
    paper_id: Uuid,
    submission: &Value,
    evaluator: &Actor,
    reviewers: [&Actor; 2],
    evaluation_id: Uuid,
    supersedes_evaluation_id: Option<Uuid>,
    idempotency_key: &str,
) -> Value {
    let mut body = evaluation_body(
        paper_id,
        submission,
        evaluator,
        reviewers,
        evaluation_id,
        supersedes_evaluation_id,
        idempotency_key,
    );
    body.as_object_mut()
        .expect("evaluation fixture object")
        .remove("reviewer_attestations");
    body
}

fn evaluation_draft_attestation_body(
    draft: &Value,
    reviewer: &Actor,
    attestation_id: Uuid,
    idempotency_key: &str,
) -> Value {
    let evaluation_id = Uuid::parse_str(
        draft["evaluation_id"]
            .as_str()
            .expect("draft evaluation id"),
    )
    .expect("draft evaluation UUID");
    let evaluation_signing_hash = draft["evaluation_signing_hash"]
        .as_str()
        .expect("draft evaluation signing hash")
        .to_string();
    let signed_at_unix = Utc::now().timestamp();
    let coi_attestation_hash = digest(&format!("coi-reviewer-{}", reviewer.player_id));
    let signing = PaperReviewAttestationSigningV1 {
        schema: PAPER_REVIEW_ATTESTATION_V1.to_string(),
        attestation_id,
        evaluation_id,
        evaluation_signing_hash,
        reviewer_player_id: reviewer.player_id,
        verdict: "approve".to_string(),
        signing_key_id: reviewer.human_key_id.clone(),
        signing_public_key_hash: reviewer.human_public_key_hash.clone(),
        coi_attestation_hash: coi_attestation_hash.clone(),
        signed_at_unix,
    };
    let frame = paper_review_attestation_signing_bytes(&signing)
        .expect("evaluation draft attestation frame");
    json!({
        "attestation_id":attestation_id,
        "draft_hash":draft["draft_hash"].clone(),
        "reviewer_player_id":reviewer.player_id,
        "verdict":"approve",
        "signing_key_id":reviewer.human_key_id,
        "signing_public_key":reviewer.human_public_key,
        "signing_public_key_hash":reviewer.human_public_key_hash,
        "coi_attestation_hash":coi_attestation_hash,
        "signed_at_unix":signed_at_unix,
        "signature":BASE64.encode(reviewer.human_key.sign(&frame).to_bytes()),
        "idempotency_key":idempotency_key,
    })
}

async fn age_evaluation_draft_authority_for_recovery_test(
    state: &AppState,
    evaluation_id: Uuid,
) -> Value {
    let now = Utc::now();
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            "select record_json from hepta_paper_evaluation_drafts where evaluation_id=$1",
        )
        .bind(evaluation_id)
        .fetch_one(pool)
        .await
        .expect("load draft before synthetic lease expiry");
        let mut draft: PaperEvaluationDraft =
            serde_json::from_value(row.get("record_json")).expect("decode draft for expiry test");
        review_v4::age_evaluation_draft_for_recovery_test(&mut draft, now)
            .expect("advance synthetic PostgreSQL draft deadline");
        sqlx::raw_sql(
            "alter table hepta_paper_evaluation_drafts
             disable trigger hepta_evaluation_draft_lifecycle_guard;",
        )
        .execute(pool)
        .await
        .expect("disable immutable draft trigger for test clock injection");
        let update_result = sqlx::query(
            "update hepta_paper_evaluation_drafts
             set expires_at=$1,draft_hash=$2,record_json=$3::jsonb
             where evaluation_id=$4 and status='open'",
        )
        .bind(draft.expires_at)
        .bind(&draft.draft_hash)
        .bind(serde_json::to_value(&draft).expect("encode aged PostgreSQL draft"))
        .bind(evaluation_id)
        .execute(pool)
        .await;
        sqlx::raw_sql(
            "alter table hepta_paper_evaluation_drafts
             enable trigger hepta_evaluation_draft_lifecycle_guard;",
        )
        .execute(pool)
        .await
        .expect("restore immutable draft trigger after test clock injection");
        assert_eq!(
            update_result
                .expect("inject synthetic PostgreSQL draft deadline")
                .rows_affected(),
            1
        );
        return serde_json::to_value(draft).expect("encode aged draft response");
    }
    let mut memory = state.paper_raid.write().await;
    serde_json::to_value(
        review_v4::age_memory_evaluation_draft_for_recovery_test(&mut memory, evaluation_id, now)
            .expect("advance synthetic memory draft deadline"),
    )
    .expect("encode aged memory draft")
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct DraftLeaseRecoveryOutcome {
    stale_attestation_code: String,
    stale_finalize_code: String,
    expired_status: String,
    replacement_evaluation_status: String,
    expiry_event_count: usize,
}

async fn run_draft_lease_recovery_flow(state: AppState) -> DraftLeaseRecoveryOutcome {
    let _ = run_full_flow(state.clone(), 3, FlowOptions::default()).await;
    let router = app(state.clone());
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
            "draft-lease-recovery-submission",
        )
        .await,
        StatusCode::OK,
    );
    claim_evaluation_panel_fixture(
        &router,
        paper_id,
        1,
        &external[0],
        [&external[1], &external[2]],
        "draft-lease-recovery-initial",
    )
    .await;
    let stale_evaluation_id = Uuid::new_v4();
    let stale_draft_key = "draft-lease-recovery-stale";
    let draft_path = format!("/v2/hepta/papers/{paper_id}/evaluation-drafts");
    let stale_draft = assert_status(
        user_post(
            &router,
            &external[0],
            "create_paper_evaluation_draft_v1",
            &draft_path,
            stale_draft_key,
            evaluation_draft_body(
                paper_id,
                &submission,
                &external[0],
                [&external[1], &external[2]],
                stale_evaluation_id,
                None,
                stale_draft_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(stale_draft["schema"], EVALUATION_DRAFT_SCHEMA_V2);
    assert!(stale_draft["expires_at"].is_string());
    if let Some(pool) = &state.pool {
        assert!(
            sqlx::query(
                "update hepta_paper_review_assignments
                 set status='expired',version=version+1,updated_at=now(),
                     record_json=jsonb_set(
                       jsonb_set(
                         jsonb_set(record_json,'{status}','\"expired\"'::jsonb,true),
                         '{version}',to_jsonb(version+1),true
                       ),
                       '{updated_at}',to_jsonb(now()),true
                     )
                 where paper_project_id=$1 and review_round=1
                   and slot='evaluator' and status='pinned'"
            )
            .bind(paper_id)
            .execute(pool)
            .await
            .is_err(),
            "PostgreSQL must reject actor-driven pinned release while its draft lease is live"
        );
    }
    let _aged_draft =
        age_evaluation_draft_authority_for_recovery_test(&state, stale_evaluation_id).await;

    let stale_attestation_key = "draft-lease-recovery-old-attestation";
    let stale_attestation_code = error_code(
        user_post(
            &router,
            &external[1],
            "submit_paper_evaluation_draft_attestation_v1",
            &format!(
                "/v2/hepta/papers/{paper_id}/evaluation-drafts/{stale_evaluation_id}/attestations"
            ),
            stale_attestation_key,
            evaluation_draft_attestation_body(
                &stale_draft,
                &external[1],
                Uuid::new_v4(),
                stale_attestation_key,
            ),
        )
        .await,
        StatusCode::CONFLICT,
    );
    let stale_finalize_key = "draft-lease-recovery-old-finalize";
    let stale_finalize_code = error_code(
        user_post(
            &router,
            &external[0],
            "finalize_paper_evaluation_draft_v1",
            &format!(
                "/v2/hepta/papers/{paper_id}/evaluation-drafts/{stale_evaluation_id}/finalize"
            ),
            stale_finalize_key,
            json!({
                "expected_draft_version":1,
                "idempotency_key":stale_finalize_key,
            }),
        )
        .await,
        StatusCode::CONFLICT,
    );
    assert_eq!(stale_attestation_code, "evaluation_draft_lease_expired");
    assert_eq!(stale_finalize_code, "evaluation_draft_lease_expired");

    claim_review_assignment_fixture(
        &router,
        &external[0],
        paper_id,
        1,
        "evaluator",
        "draft-lease-recovery-reassigned",
    )
    .await;
    let expired = assert_status(
        user_get(
            &router,
            &external[0],
            "get_paper_evaluation_draft_v1",
            &format!("/v2/hepta/papers/{paper_id}/evaluation-drafts/{stale_evaluation_id}"),
            "draft-lease-recovery-read-expired",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(expired["draft"]["status"], "expired");
    assert_eq!(expired["draft"]["version"], 2);
    assert_eq!(expired["ready_to_finalize"], false);

    let replacement_evaluation_id = Uuid::new_v4();
    let replacement_draft_key = "draft-lease-recovery-replacement";
    let replacement = assert_status(
        user_post(
            &router,
            &external[0],
            "create_paper_evaluation_draft_v1",
            &draft_path,
            replacement_draft_key,
            evaluation_draft_body(
                paper_id,
                &submission,
                &external[0],
                [&external[1], &external[2]],
                replacement_evaluation_id,
                None,
                replacement_draft_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    for (index, reviewer) in [&external[1], &external[2]].into_iter().enumerate() {
        let key = format!("draft-lease-recovery-replacement-reviewer-{index}");
        assert_status(
            user_post(
                &router,
                reviewer,
                "submit_paper_evaluation_draft_attestation_v1",
                &format!(
                    "/v2/hepta/papers/{paper_id}/evaluation-drafts/{replacement_evaluation_id}/attestations"
                ),
                &key,
                evaluation_draft_attestation_body(
                    &replacement,
                    reviewer,
                    Uuid::new_v4(),
                    &key,
                ),
            )
            .await,
            StatusCode::CREATED,
        );
    }
    let replacement_finalize_key = "draft-lease-recovery-replacement-finalize";
    let replacement_evaluation = assert_status(
        user_post(
            &router,
            &external[0],
            "finalize_paper_evaluation_draft_v1",
            &format!(
                "/v2/hepta/papers/{paper_id}/evaluation-drafts/{replacement_evaluation_id}/finalize"
            ),
            replacement_finalize_key,
            json!({
                "expected_draft_version":1,
                "idempotency_key":replacement_finalize_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    let event_types = paper_raid_event_types(&state).await;
    let expiry_event_count = event_types
        .iter()
        .filter(|event_type| *event_type == "hepta.paper_raid.evaluation_draft.expired.v1")
        .count();
    assert_eq!(expiry_event_count, 1);
    DraftLeaseRecoveryOutcome {
        stale_attestation_code,
        stale_finalize_code,
        expired_status: expired["draft"]["status"]
            .as_str()
            .expect("expired draft status")
            .to_string(),
        replacement_evaluation_status: replacement_evaluation["status"]
            .as_str()
            .expect("replacement evaluation status")
            .to_string(),
        expiry_event_count,
    }
}

async fn run_review_flow(state: AppState) -> ReviewFlowOutcome {
    let replay_0047_after_reservation = state.pool.is_some();
    let _ = run_full_flow(
        state.clone(),
        3,
        FlowOptions {
            replay_0047_after_reservation,
            ..FlowOptions::default()
        },
    )
    .await;
    let paper_id = Uuid::from_u128(0x5000_0000_0000_4000_8000_0000_0000_0000 + 3 * 0x100);
    if let Some(pool) = &state.pool {
        let challenge_id: Uuid = sqlx::query_scalar(
            "select challenge_id from hepta_paper_projects where paper_project_id=$1",
        )
        .bind(paper_id)
        .fetch_one(pool)
        .await
        .expect("read PostgreSQL review fixture challenge ID");
        assert!(
            !state
                .inner
                .read()
                .await
                .challenges
                .contains_key(&challenge_id),
            "PostgreSQL review regression must not accidentally hydrate the in-memory challenge cache"
        );
        let durable_hashes = state
            .inspect(|league| {
                league
                    .challenges
                    .get(&challenge_id)
                    .map(|challenge| {
                        (
                            challenge.evaluator_manifest_hash.clone(),
                            challenge.dataset_manifest_hash.clone(),
                        )
                    })
                    .ok_or_else(|| {
                        ApiError::internal("durable PostgreSQL review fixture challenge is missing")
                    })
            })
            .await
            .expect("read durable PostgreSQL review fixture challenge");
        assert_eq!(durable_hashes.0, digest("paper-raid-evaluator"));
        assert_eq!(durable_hashes.1, digest("paper-raid-dataset"));
    }
    let router = app(state.clone());
    let authors = actors(3);
    let external = actors(11);
    register_prerequisites(&router, &external).await;
    create_players_and_bindings(&router, &external).await;
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
    claim_evaluation_panel_fixture(
        &router,
        paper_id,
        1,
        &external[0],
        [&external[1], &external[2]],
        "review-flow-round-1",
    )
    .await;
    let evaluation_id = Uuid::new_v4();
    let draft_key = "p5-evaluation-draft";
    let draft_path = format!("/v2/hepta/papers/{paper_id}/evaluation-drafts");
    let draft = assert_status(
        user_post(
            &router,
            &external[0],
            "create_paper_evaluation_draft_v1",
            &draft_path,
            draft_key,
            evaluation_draft_body(
                paper_id,
                &submission,
                &external[0],
                [&external[1], &external[2]],
                evaluation_id,
                None,
                draft_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(draft["status"], "open");
    assert_eq!(draft["version"], 1);

    let reviewer_bundle = assert_status(
        user_get(
            &router,
            &external[1],
            "get_paper_review_bundle_v1",
            &format!("/v2/hepta/papers/{paper_id}/review-bundle"),
            "p5-reviewer-bundle-draft",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(
        reviewer_bundle["evaluation_quorum"]["draft"]["evaluation_id"],
        evaluation_id.to_string()
    );
    assert_eq!(
        reviewer_bundle["evaluation_quorum"]["draft"]["draft_hash"],
        draft["draft_hash"]
    );
    assert!(reviewer_bundle["evaluation"].is_null());

    let attestation_path =
        format!("/v2/hepta/papers/{paper_id}/evaluation-drafts/{evaluation_id}/attestations");
    let wrong_slot_key = "p5-evaluation-draft-wrong-slot";
    assert_eq!(
        error_code(
            user_post(
                &router,
                &external[0],
                "submit_paper_evaluation_draft_attestation_v1",
                &attestation_path,
                wrong_slot_key,
                evaluation_draft_attestation_body(
                    &draft,
                    &external[0],
                    Uuid::new_v4(),
                    wrong_slot_key,
                ),
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "evaluation_draft_reviewer_assignment_mismatch"
    );

    let concurrent_key_a = "p5-evaluation-draft-reviewer-1-a";
    let concurrent_key_b = "p5-evaluation-draft-reviewer-1-b";
    let concurrent_a = user_post(
        &router,
        &external[1],
        "submit_paper_evaluation_draft_attestation_v1",
        &attestation_path,
        concurrent_key_a,
        evaluation_draft_attestation_body(&draft, &external[1], Uuid::new_v4(), concurrent_key_a),
    );
    let concurrent_b = user_post(
        &router,
        &external[1],
        "submit_paper_evaluation_draft_attestation_v1",
        &attestation_path,
        concurrent_key_b,
        evaluation_draft_attestation_body(&draft, &external[1], Uuid::new_v4(), concurrent_key_b),
    );
    let (concurrent_a, concurrent_b) = tokio::join!(concurrent_a, concurrent_b);
    let mut concurrent_statuses = [concurrent_a.0.as_u16(), concurrent_b.0.as_u16()];
    concurrent_statuses.sort();
    assert_eq!(
        concurrent_statuses,
        [StatusCode::CREATED.as_u16(), StatusCode::CONFLICT.as_u16()],
        "one immutable reviewer slot write must win a concurrent race"
    );

    let reviewer_2_key = "p5-evaluation-draft-reviewer-2";
    assert_status(
        user_post(
            &router,
            &external[2],
            "submit_paper_evaluation_draft_attestation_v1",
            &attestation_path,
            reviewer_2_key,
            evaluation_draft_attestation_body(&draft, &external[2], Uuid::new_v4(), reviewer_2_key),
        )
        .await,
        StatusCode::CREATED,
    );
    let quorum = assert_status(
        user_get(
            &router,
            &external[0],
            "get_paper_evaluation_draft_v1",
            &format!("/v2/hepta/papers/{paper_id}/evaluation-drafts/{evaluation_id}"),
            "p5-evaluation-draft-quorum",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(quorum["attestations"].as_array().unwrap().len(), 2);
    assert_eq!(quorum["missing_slots"], json!([]));
    assert_eq!(quorum["assignments_active"], true);
    assert_eq!(quorum["ready_to_finalize"], true);

    let finalize_key = "p5-evaluation-draft-finalize";
    let evaluation = assert_status(
        user_post(
            &router,
            &external[0],
            "finalize_paper_evaluation_draft_v1",
            &format!("/v2/hepta/papers/{paper_id}/evaluation-drafts/{evaluation_id}/finalize"),
            finalize_key,
            json!({
                "expected_draft_version":1,
                "idempotency_key":finalize_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(evaluation["paper_score"]["score_bps"], 8_500);
    assert_eq!(evaluation["settlement_state"], "pending_finality");
    claim_review_assignment_fixture(
        &router,
        &external[3],
        paper_id,
        1,
        "reproducer",
        "review-flow-round-1",
    )
    .await;
    let reproducer_bundle = assert_status(
        user_get(
            &router,
            &external[3],
            "get_paper_review_bundle_v1",
            &format!("/v2/hepta/papers/{paper_id}/review-bundle"),
            "p5-reproducer-bundle-evaluation",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(
        reproducer_bundle["evaluation"]["evaluation_id"],
        evaluation_id.to_string()
    );
    assert_eq!(
        reproducer_bundle["evaluation"]["tolerance_policy"],
        evaluation["tolerance_policy"]
    );
    assert_eq!(
        reproducer_bundle["evaluation"]["reference_metrics_micros"],
        evaluation["reference_metrics_micros"]
    );
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
            &external[3],
            "create_paper_reproduction_v1",
            &reproduction_path,
            failing_key,
            reproduction_body(
                paper_id,
                &evaluation,
                &external[3],
                failing_id,
                Some(passing_id),
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
                Some(failing_id),
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
    claim_evaluation_panel_fixture(
        &router,
        paper_id,
        2,
        &external[6],
        [&external[7], &external[8]],
        "review-flow-round-2",
    )
    .await;
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
async fn memory_legacy_evaluation_consumes_exact_panel_atomically_and_replays() {
    let state = AppState::new(security());
    let _ = run_full_flow(state.clone(), 3, FlowOptions::default()).await;
    let router = app(state.clone());
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
            "legacy-panel-submission",
        )
        .await,
        StatusCode::OK,
    );
    let submission_id = Uuid::parse_str(
        submission["submission_id"]
            .as_str()
            .expect("legacy panel submission ID"),
    )
    .expect("legacy panel submission UUID");
    claim_evaluation_panel_fixture(
        &router,
        paper_id,
        1,
        &external[0],
        [&external[1], &external[2]],
        "legacy-panel-round-1",
    )
    .await;
    let evaluation_path = format!("/v2/hepta/papers/{paper_id}/evaluations");
    let evaluation_id = Uuid::new_v4();

    let failed_key = "legacy-panel-signature-failure";
    let mut failed_body = evaluation_body(
        paper_id,
        &submission,
        &external[0],
        [&external[1], &external[2]],
        evaluation_id,
        None,
        failed_key,
    );
    failed_body["evaluator_signature"] = json!(BASE64.encode([0_u8; 64]));
    assert_eq!(
        error_code(
            user_post(
                &router,
                &external[0],
                "create_paper_evaluation_v1",
                &evaluation_path,
                failed_key,
                failed_body,
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "evaluation_signature_failed"
    );
    {
        let memory = state.paper_raid.read().await;
        let claimed = memory
            .review
            .assignments
            .values()
            .filter(|assignment| assignment.submission_id == submission_id)
            .collect::<Vec<_>>();
        assert_eq!(claimed.len(), 3);
        assert!(claimed.iter().all(|assignment| {
            assignment.status == ReviewAssignmentStatus::Claimed
                && assignment.pinned_evaluation_id.is_none()
                && assignment.version == 1
        }));
        assert!(!memory.review.evaluations.contains_key(&evaluation_id));
    }

    let key = "legacy-panel-evaluation";
    let body = evaluation_body(
        paper_id,
        &submission,
        &external[0],
        [&external[1], &external[2]],
        evaluation_id,
        None,
        key,
    );
    let first = user_post(
        &router,
        &external[0],
        "create_paper_evaluation_v1",
        &evaluation_path,
        key,
        body.clone(),
    );
    let lost_response_retry = user_post(
        &router,
        &external[0],
        "create_paper_evaluation_v1",
        &evaluation_path,
        key,
        body.clone(),
    );
    let (first, retry) = tokio::join!(first, lost_response_retry);
    let first = assert_status(first, StatusCode::CREATED);
    let retry = assert_status(retry, StatusCode::CREATED);
    assert_eq!(
        retry, first,
        "concurrent lost-response retry must replay exactly"
    );
    assert_eq!(
        assert_status(
            user_post(
                &router,
                &external[0],
                "create_paper_evaluation_v1",
                &evaluation_path,
                key,
                body,
            )
            .await,
            StatusCode::CREATED,
        ),
        first
    );

    let drift = evaluation_body(
        paper_id,
        &submission,
        &external[0],
        [&external[1], &external[2]],
        Uuid::new_v4(),
        None,
        key,
    );
    assert_eq!(
        error_code(
            user_post(
                &router,
                &external[0],
                "create_paper_evaluation_v1",
                &evaluation_path,
                key,
                drift,
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "idempotency_key_conflict"
    );
    {
        let memory = state.paper_raid.read().await;
        let consumed = memory
            .review
            .assignments
            .values()
            .filter(|assignment| assignment.submission_id == submission_id)
            .collect::<Vec<_>>();
        assert_eq!(consumed.len(), 3);
        assert!(consumed.iter().all(|assignment| {
            assignment.status == ReviewAssignmentStatus::Consumed
                && assignment.pinned_evaluation_id == Some(evaluation_id)
                && assignment.version == 3
        }));
        assert_eq!(
            memory
                .review
                .evaluations
                .values()
                .filter(|evaluation| evaluation.evaluation_id == evaluation_id)
                .count(),
            1
        );
    }
    assert_eq!(
        paper_raid_event_types(&state)
            .await
            .iter()
            .filter(|event_type| *event_type == "hepta.paper_raid.evaluation.recorded.v1")
            .count(),
        1
    );
}

#[tokio::test]
async fn memory_author_rework_is_signed_scientific_and_resets_review_root() {
    let state = AppState::new(security());
    let _ = run_full_flow(state.clone(), 3, FlowOptions::default()).await;
    let router = app(state.clone());
    let mut authors = actors(3);
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
            "rework-read-rejected-submission",
        )
        .await,
        StatusCode::OK,
    );
    let rejected_submission: JointPaperSubmission =
        serde_json::from_value(submission.clone()).expect("decode rejected submission");
    claim_evaluation_panel_fixture(
        &router,
        paper_id,
        1,
        &external[0],
        [&external[1], &external[2]],
        "rework-rejected-round-1",
    )
    .await;
    let rejected_evaluation_id = Uuid::new_v4();
    let rejected_key = "rework-rejected-evaluation";
    let rejected_evaluation = assert_status(
        user_post(
            &router,
            &external[0],
            "create_paper_evaluation_v1",
            &format!("/v2/hepta/papers/{paper_id}/evaluations"),
            rejected_key,
            rejected_evaluation_body(
                paper_id,
                &submission,
                &external[0],
                [&external[1], &external[2]],
                rejected_evaluation_id,
                rejected_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(rejected_evaluation["status"], "rejected");
    let rejected_submission_id = rejected_submission.submission_id;
    {
        let memory = state.paper_raid.read().await;
        let consumed_panel = memory
            .review
            .assignments
            .values()
            .filter(|assignment| {
                assignment.submission_id == rejected_submission_id
                    && assignment.review_round == 1
                    && assignment.slot != ReviewAssignmentSlot::Reproducer
            })
            .collect::<Vec<_>>();
        assert_eq!(consumed_panel.len(), 3);
        assert!(consumed_panel.iter().all(|assignment| {
            assignment.status == ReviewAssignmentStatus::Consumed
                && assignment.pinned_evaluation_id == Some(rejected_evaluation_id)
                && assignment.version == 3
        }));
    }
    let rework_path = format!("/v2/hepta/papers/{paper_id}/reworks");
    let paper_version = state
        .paper_raid
        .read()
        .await
        .papers
        .get(&paper_id)
        .expect("Paper fixture")
        .version;

    let expired_key = "rework-expired-signature";
    let expired_body = paper_rework_body(
        paper_id,
        rejected_submission_id,
        rejected_evaluation_id,
        paper_version,
        Uuid::new_v4(),
        2,
        &authors[0],
        digest("rework-expired"),
        (Utc::now() - chrono::Duration::hours(25)).timestamp(),
        expired_key,
    );
    assert_eq!(
        error_code(
            user_post(
                &router,
                &authors[0],
                "start_paper_rework_v1",
                &rework_path,
                expired_key,
                expired_body,
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "signed_at_outside_window"
    );

    let active_assignment_id = {
        let mut memory = state.paper_raid.write().await;
        let mut assignment = memory
            .review
            .assignments
            .values()
            .find(|assignment| assignment.submission_id == rejected_submission_id)
            .cloned()
            .expect("consumed rejected Review assignment");
        assignment.assignment_id = Uuid::new_v4();
        assignment.status = ReviewAssignmentStatus::Claimed;
        assignment.pinned_evaluation_id = None;
        assignment.expires_at = Utc::now() + chrono::Duration::hours(1);
        assignment.updated_at = Utc::now();
        let assignment_id = assignment.assignment_id;
        memory
            .review
            .assignments
            .insert(assignment.assignment_id, assignment);
        assignment_id
    };
    let assignment_key = "rework-active-assignment";
    assert_eq!(
        error_code(
            user_post(
                &router,
                &authors[0],
                "start_paper_rework_v1",
                &rework_path,
                assignment_key,
                paper_rework_body(
                    paper_id,
                    rejected_submission_id,
                    rejected_evaluation_id,
                    paper_version,
                    Uuid::new_v4(),
                    2,
                    &authors[0],
                    digest("rework-active-assignment"),
                    Utc::now().timestamp(),
                    assignment_key,
                ),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "paper_rework_review_assignment_active"
    );
    {
        let mut memory = state.paper_raid.write().await;
        memory
            .review
            .assignments
            .remove(&active_assignment_id)
            .expect("remove only the synthetic active assignment");
        assert!(memory.review.assignments.values().all(|assignment| {
            assignment.submission_id != rejected_submission_id
                || matches!(
                    assignment.status,
                    ReviewAssignmentStatus::Consumed | ReviewAssignmentStatus::Expired
                )
        }));
    }

    let appeal_id = Uuid::new_v4();
    let appeal_key = "rework-open-appeal";
    assert_status(
        user_post(
            &router,
            &authors[0],
            "create_paper_appeal_v1",
            &format!("/v2/hepta/papers/{paper_id}/evaluations/{rejected_evaluation_id}/appeals"),
            appeal_key,
            appeal_body(
                paper_id,
                &rejected_evaluation,
                &authors[0],
                appeal_id,
                appeal_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    let open_appeal_key = "rework-blocked-open-appeal";
    assert_eq!(
        error_code(
            user_post(
                &router,
                &authors[0],
                "start_paper_rework_v1",
                &rework_path,
                open_appeal_key,
                paper_rework_body(
                    paper_id,
                    rejected_submission_id,
                    rejected_evaluation_id,
                    paper_version,
                    Uuid::new_v4(),
                    2,
                    &authors[0],
                    digest("rework-open-appeal-block"),
                    Utc::now().timestamp(),
                    open_appeal_key,
                ),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "paper_rework_open_appeal"
    );
    state
        .paper_raid
        .write()
        .await
        .review
        .appeals
        .remove(&appeal_id);

    let superseding_evaluation_id = {
        let mut memory = state.paper_raid.write().await;
        let mut child = memory
            .review
            .evaluations
            .get(&rejected_evaluation_id)
            .cloned()
            .expect("rejected evaluation fixture");
        child.evaluation_id = Uuid::new_v4();
        child.supersedes_evaluation_id = Some(rejected_evaluation_id);
        child.version = child
            .version
            .checked_add(1)
            .expect("fixture evaluation version");
        child.created_at = Utc::now();
        let child_id = child.evaluation_id;
        memory.review.evaluations.insert(child_id, child);
        child_id
    };
    let superseded_key = "rework-superseded-evaluation";
    assert_eq!(
        error_code(
            user_post(
                &router,
                &authors[0],
                "start_paper_rework_v1",
                &rework_path,
                superseded_key,
                paper_rework_body(
                    paper_id,
                    rejected_submission_id,
                    rejected_evaluation_id,
                    paper_version,
                    Uuid::new_v4(),
                    2,
                    &authors[0],
                    digest("rework-superseded"),
                    Utc::now().timestamp(),
                    superseded_key,
                ),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "paper_rework_evaluation_not_final_rejected"
    );
    state
        .paper_raid
        .write()
        .await
        .review
        .evaluations
        .remove(&superseding_evaluation_id);

    let frozen_consents = {
        let mut memory = state.paper_raid.write().await;
        let submission = memory
            .submissions
            .get_mut(&rejected_submission_id)
            .expect("rejected submission fixture");
        let frozen = submission.paper_bundle.author_consents.clone();
        submission
            .paper_bundle
            .author_consents
            .retain(|consent| consent.player_id != authors[0].player_id);
        frozen
    };
    let non_frozen_key = "rework-non-frozen-author";
    assert_eq!(
        error_code(
            user_post(
                &router,
                &authors[0],
                "start_paper_rework_v1",
                &rework_path,
                non_frozen_key,
                paper_rework_body(
                    paper_id,
                    rejected_submission_id,
                    rejected_evaluation_id,
                    paper_version,
                    Uuid::new_v4(),
                    2,
                    &authors[0],
                    digest("rework-non-frozen"),
                    Utc::now().timestamp(),
                    non_frozen_key,
                ),
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "paper_rework_author_not_frozen"
    );
    state
        .paper_raid
        .write()
        .await
        .submissions
        .get_mut(&rejected_submission_id)
        .expect("rejected submission fixture")
        .paper_bundle
        .author_consents = frozen_consents;

    let retired_author = authors[0].clone();
    rotate_human_key(&router, &mut authors[0]).await;
    let retired_key = "rework-retired-key";
    assert_eq!(
        error_code(
            user_post(
                &router,
                &retired_author,
                "start_paper_rework_v1",
                &rework_path,
                retired_key,
                paper_rework_body(
                    paper_id,
                    rejected_submission_id,
                    rejected_evaluation_id,
                    paper_version,
                    Uuid::new_v4(),
                    2,
                    &retired_author,
                    digest("rework-retired-key"),
                    Utc::now().timestamp(),
                    retired_key,
                ),
            )
            .await,
            StatusCode::FORBIDDEN,
        ),
        "paper_rework_author_key_inactive"
    );

    let rework_id = Uuid::new_v4();
    let start_key = "rework-start-valid";
    let start_body = paper_rework_body(
        paper_id,
        rejected_submission_id,
        rejected_evaluation_id,
        paper_version,
        rework_id,
        2,
        &authors[0],
        digest("rework-valid-reason"),
        Utc::now().timestamp(),
        start_key,
    );
    let started = assert_status(
        user_post(
            &router,
            &authors[0],
            "start_paper_rework_v1",
            &rework_path,
            start_key,
            start_body.clone(),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_ne!(
        started["rejected_rework_content_commitment_sha256"],
        format!("sha256:{}", "0".repeat(64))
    );
    assert_eq!(
        assert_status(
            user_post(
                &router,
                &authors[0],
                "start_paper_rework_v1",
                &rework_path,
                start_key,
                start_body,
            )
            .await,
            StatusCode::CREATED,
        ),
        started
    );
    let drift_body = paper_rework_body(
        paper_id,
        rejected_submission_id,
        rejected_evaluation_id,
        paper_version,
        rework_id,
        2,
        &authors[0],
        digest("rework-drifted-reason"),
        Utc::now().timestamp(),
        start_key,
    );
    assert_eq!(
        error_code(
            user_post(
                &router,
                &authors[0],
                "start_paper_rework_v1",
                &rework_path,
                start_key,
                drift_body,
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "idempotency_key_conflict"
    );
    {
        let memory = state.paper_raid.read().await;
        assert_eq!(
            memory
                .submissions
                .get(&rejected_submission_id)
                .expect("withdrawn rejected submission")
                .status,
            JointSubmissionStatus::Withdrawn
        );
    }
    let active_lineage = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_paper_reworks_v1",
            &rework_path,
            "rework-get-active-lineage",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(active_lineage.as_array().unwrap().len(), 1);
    assert!(active_lineage[0]["resubmission"].is_null());

    let (identity_revision_id, identity_release_hash, identity_paper_version) =
        seed_rework_release_candidate_memory(&state, &rejected_submission, &authors, false).await;
    let identity_key = "rework-finalize-identity-only";
    assert_eq!(
        error_code(
            user_post(
                &router,
                &authors[0],
                "finalize_joint_paper_submission_v2",
                &format!("/v2/hepta/papers/{paper_id}/finalize"),
                identity_key,
                json!({
                    "submission_id":Uuid::new_v4(),
                    "expected_paper_version":identity_paper_version,
                    "revision_id":identity_revision_id,
                    "release_candidate_hash":identity_release_hash,
                    "idempotency_key":identity_key,
                }),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "paper_rework_replacement_not_new"
    );

    let (replacement_revision_id, replacement_release_hash, replacement_paper_version) =
        seed_rework_release_candidate_memory(&state, &rejected_submission, &authors, true).await;
    let replacement_submission_id = Uuid::new_v4();
    let replacement_key = "rework-finalize-scientific-change";
    let replacement = assert_status(
        user_post(
            &router,
            &authors[0],
            "finalize_joint_paper_submission_v2",
            &format!("/v2/hepta/papers/{paper_id}/finalize"),
            replacement_key,
            json!({
                "submission_id":replacement_submission_id,
                "expected_paper_version":replacement_paper_version,
                "revision_id":replacement_revision_id,
                "release_candidate_hash":replacement_release_hash,
                "idempotency_key":replacement_key,
            }),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(replacement["status"], "submission_ready");
    let closed_lineage = assert_status(
        user_get(
            &router,
            &authors[0],
            "get_paper_reworks_v1",
            &rework_path,
            "rework-get-closed-lineage",
        )
        .await,
        StatusCode::OK,
    );
    assert_eq!(
        closed_lineage[0]["resubmission"]["replacement_submission_id"],
        replacement_submission_id.to_string()
    );
    assert_eq!(
        closed_lineage[0]["resubmission"]["replacement_review_round"],
        1
    );
    assert_ne!(
        closed_lineage[0]["rework"]["rejected_rework_content_commitment_sha256"],
        closed_lineage[0]["resubmission"]["replacement_rework_content_commitment_sha256"]
    );
    {
        let memory = state.paper_raid.read().await;
        let paper = memory
            .papers
            .get(&paper_id)
            .expect("completed rework Paper");
        assert_eq!(paper.phase, PaperPhase::SubmissionReady);
        assert!(paper.active_rework_id.is_none());
        assert!(paper.active_rework_cycle.is_none());
        assert!(paper.rework_expires_at.is_none());
    }

    let queue = assert_status(
        user_get(
            &router,
            &external[0],
            "get_review_queue_v1",
            "/v2/hepta/review-queue",
            "rework-review-queue",
        )
        .await,
        StatusCode::OK,
    );
    let paper_items = queue
        .as_array()
        .expect("Review queue")
        .iter()
        .filter(|item| item["paper_project_id"] == paper_id.to_string())
        .collect::<Vec<_>>();
    assert_eq!(paper_items.len(), 1);
    assert_eq!(
        paper_items[0]["submission_id"],
        replacement_submission_id.to_string()
    );
    assert!(paper_items[0]["open_slots"]
        .as_array()
        .expect("replacement open slots")
        .iter()
        .all(|slot| slot["review_round"] == 1));
    let assignment = claim_review_assignment_fixture(
        &router,
        &external[0],
        paper_id,
        1,
        "evaluator",
        "rework-replacement-round-1",
    )
    .await;
    assert_eq!(
        assignment["submission_id"],
        replacement_submission_id.to_string()
    );
}

#[tokio::test]
async fn memory_expired_evaluation_draft_reassigns_and_finalizes_without_revival() {
    let outcome = run_draft_lease_recovery_flow(AppState::new(security())).await;
    assert_eq!(
        outcome.stale_attestation_code,
        "evaluation_draft_lease_expired"
    );
    assert_eq!(
        outcome.stale_finalize_code,
        "evaluation_draft_lease_expired"
    );
    assert_eq!(outcome.expired_status, "expired");
    assert_eq!(outcome.replacement_evaluation_status, "accepted");
    assert_eq!(outcome.expiry_event_count, 1);
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
    assert_eq!(outcome.revision_binding_count, 2);
    assert_eq!(outcome.section_merge_count, 3);
    assert!(outcome.room_event_count >= 11);
    assert!(outcome.room_last_cursor >= outcome.room_event_count as u64);
}

#[tokio::test]
async fn memory_agent_proposals_trust_only_the_active_secure_binding() {
    exercise_agent_proposal_binding_authority(AppState::new(security())).await;
}

async fn assert_postgres_agent_proposal_v2_identity_parity(pool: &sqlx::PgPool) {
    let proposal_count: i64 = sqlx::query_scalar("select count(*) from hepta_agent_proposals")
        .fetch_one(pool)
        .await
        .expect("count persisted Agent proposal V2 rows");
    assert!(
        proposal_count > 0,
        "Agent proposal V2 parity proof needs rows"
    );
    let parity_violations: i64 = sqlx::query_scalar(
        "select count(*)
         from hepta_agent_proposals
         where not (
             (record_json->>'proposal_id') is not distinct from proposal_id::text
             and (record_json->>'paper_project_id') is not distinct from paper_project_id::text
             and (record_json->>'work_item_id') is not distinct from work_item_id::text
             and (record_json->>'section_key') is not distinct from section_key
             and (record_json->>'parent_revision_id') is not distinct from parent_revision_id::text
             and (record_json->>'lease_id') is not distinct from lease_id::text
             and (record_json->>'lease_fencing_token') is not distinct from lease_fencing_token::text
             and (record_json->>'expected_work_version') is not distinct from expected_work_version::text
             and (record_json->>'proposal_kind') is not distinct from proposal_kind
             and (record_json->>'payload_hash') is not distinct from payload_hash
             and (record_json->>'artifact_manifest_id') is not distinct from artifact_manifest_id::text
             and (record_json->>'artifact_manifest_hash') is not distinct from artifact_manifest_hash
             and (record_json->>'agent_id') is not distinct from agent_id
             and (record_json->>'binding_id') is not distinct from binding_id::text
             and (record_json->>'agent_key_id') is not distinct from agent_key_id
             and (record_json->>'agent_public_key') is not distinct from agent_public_key
             and (record_json->>'signature') is not distinct from signature
             and (record_json->>'status') is not distinct from status
             and (record_json->>'version') is not distinct from version::text
             and signed_at is not distinct from to_timestamp((record_json->>'signed_at_unix')::double precision)
         )",
    )
    .fetch_one(pool)
    .await
    .expect("verify persisted Agent proposal V2 relational/JSON identity parity");
    assert_eq!(parity_violations, 0);

    let mutation = sqlx::query(
        "update hepta_agent_proposals
         set artifact_manifest_hash=$1
         where proposal_id=(select proposal_id from hepta_agent_proposals order by proposal_id limit 1)",
    )
    .bind(digest("tampered-relational-artifact-manifest-hash"))
    .execute(pool)
    .await;
    assert!(
        mutation.is_err(),
        "full signed Agent proposal identity parity must reject relational-only mutation"
    );

    for (column_name, record_path) in [
        ("payload_hash", "payload_hash"),
        ("artifact_manifest_hash", "artifact_manifest_hash"),
    ] {
        let statement = format!(
            "update hepta_agent_proposals
             set {column_name}='not-a-sha256-digest',
                 record_json=jsonb_set(
                     record_json,
                     '{{{record_path}}}',
                     to_jsonb('not-a-sha256-digest'::text)
                 )
             where proposal_id=(select proposal_id from hepta_agent_proposals order by proposal_id limit 1)"
        );
        let digest_mutation = sqlx::query(&statement).execute(pool).await;
        assert!(
            digest_mutation.is_err(),
            "Agent proposal V2 {column_name} must remain a canonical SHA-256 digest even when record_json matches"
        );
    }
}

async fn assert_postgres_work_item_record_parity(pool: &sqlx::PgPool) {
    let work_item_count: i64 = sqlx::query_scalar("select count(*) from hepta_paper_work_items")
        .fetch_one(pool)
        .await
        .expect("count persisted WorkItem rows");
    assert!(
        work_item_count > 0,
        "WorkItem relational/JSON parity proof needs rows"
    );
    let parity_violations: i64 = sqlx::query_scalar(
        "select count(*)
         from hepta_paper_work_items
         where not (
             (record_json->>'work_item_id') is not distinct from work_item_id::text
             and (record_json->>'paper_project_id') is not distinct from paper_project_id::text
             and (record_json->>'assigned_player_id') is not distinct from assigned_player_id::text
             and (record_json->>'assigned_binding_id') is not distinct from assigned_binding_id::text
             and (record_json->>'status') is not distinct from status
             and (record_json->>'version') is not distinct from version::text
         )",
    )
    .fetch_one(pool)
    .await
    .expect("verify persisted WorkItem relational/JSON identity parity");
    assert_eq!(parity_violations, 0);

    let relational_only_mutation = sqlx::query(
        "update hepta_paper_work_items
         set status=case when status='cancelled' then 'planned' else 'cancelled' end
         where work_item_id=(
             select work_item_id from hepta_paper_work_items order by work_item_id limit 1
         )",
    )
    .execute(pool)
    .await;
    assert!(
        relational_only_mutation.is_err(),
        "WorkItem parity must reject a relational-only status mutation"
    );

    for json_field in [
        "work_item_id",
        "paper_project_id",
        "assigned_player_id",
        "assigned_binding_id",
        "status",
        "version",
    ] {
        let json_only_mutation = sqlx::query(
            "update hepta_paper_work_items
             set record_json=jsonb_set(
                 record_json,
                 array[$1]::text[],
                 to_jsonb('__work_item_record_drift__'::text),
                 true
             )
             where work_item_id=(
                 select work_item_id from hepta_paper_work_items order by work_item_id limit 1
             )",
        )
        .bind(json_field)
        .execute(pool)
        .await;
        assert!(
            json_only_mutation.is_err(),
            "WorkItem parity must reject a record_json-only {json_field} mutation"
        );
    }
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
async fn postgres_agent_proposals_trust_only_the_active_secure_binding() {
    let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
        eprintln!("HEPTA_TEST_DATABASE_URL unset; Agent binding authority PostgreSQL test skipped");
        return;
    };
    let mut lock = PgConnection::connect(&database_url)
        .await
        .expect("PostgreSQL Agent binding authority test lock");
    sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("serialize Hepta PostgreSQL tests");
    let state = AppState::connect(&database_url, security())
        .await
        .expect("Agent binding authority PostgreSQL state");
    let pool = state
        .pool
        .as_ref()
        .expect("Agent authority PostgreSQL pool");
    for application in ["second", "third"] {
        sqlx::raw_sql(include_str!(
            "../../../migrations/0044_add_hepta_agent_proposal_v2_epoch.sql"
        ))
        .execute(pool)
        .await
        .unwrap_or_else(|error| panic!("0044 {application} application: {error}"));
        sqlx::raw_sql(include_str!(
            "../../../migrations/0045_add_hepta_work_item_record_parity.sql"
        ))
        .execute(pool)
        .await
        .unwrap_or_else(|error| panic!("0045 {application} application: {error}"));
    }
    verify_agent_proposal_v2_migration_catalog(pool)
        .await
        .expect("0044 exact runtime catalog readiness");
    verify_work_item_record_parity_catalog(pool)
        .await
        .expect("0045 exact runtime catalog readiness");
    sqlx::query("drop index hepta_agent_proposals_lease_epoch_v2_idx")
        .execute(pool)
        .await
        .expect("remove Agent proposal V2 lease-epoch index for readiness probe");
    let missing_index = verify_agent_proposal_v2_migration_catalog(pool)
        .await
        .expect_err("runtime readiness must reject a missing Agent proposal V2 index");
    assert!(missing_index.contains("hepta_agent_proposals_lease_epoch_v2_idx"));
    sqlx::raw_sql(include_str!(
        "../../../migrations/0044_add_hepta_agent_proposal_v2_epoch.sql"
    ))
    .execute(pool)
    .await
    .expect("0044 repairs the deliberately removed lease-epoch index");
    verify_agent_proposal_v2_migration_catalog(pool)
        .await
        .expect("0044 repaired exact runtime catalog readiness");
    sqlx::query(
        "alter table hepta_paper_work_items
         drop constraint hepta_paper_work_items_record_json_parity_check",
    )
    .execute(pool)
    .await
    .expect("remove WorkItem parity constraint for readiness probe");
    let missing_parity = verify_work_item_record_parity_catalog(pool)
        .await
        .expect_err("runtime readiness must reject a missing WorkItem parity constraint");
    assert!(missing_parity.contains("hepta_paper_work_items_record_json_parity_check"));
    sqlx::raw_sql(
        "alter table hepta_paper_work_items
         add constraint hepta_paper_work_items_record_json_parity_check
         check (
             (record_json->>'work_item_id') is not distinct from work_item_id::text
             and (record_json->>'paper_project_id') is not distinct from paper_project_id::text
             and (record_json->>'assigned_player_id') is not distinct from assigned_player_id::text
             and (record_json->>'assigned_binding_id') is not distinct from assigned_binding_id::text
             and (record_json->>'status') is not distinct from status
             and (record_json->>'version') is not distinct from version::text
         ) not valid",
    )
    .execute(pool)
    .await
    .expect("install unvalidated WorkItem parity constraint for readiness probe");
    let unvalidated_parity = verify_work_item_record_parity_catalog(pool)
        .await
        .expect_err("runtime readiness must reject an unvalidated WorkItem parity constraint");
    assert!(unvalidated_parity.contains("unvalidated"));
    sqlx::raw_sql(include_str!(
        "../../../migrations/0045_add_hepta_work_item_record_parity.sql"
    ))
    .execute(pool)
    .await
    .expect("0045 repairs the deliberately removed WorkItem parity constraint");
    verify_work_item_record_parity_catalog(pool)
        .await
        .expect("0045 repaired exact runtime catalog readiness");
    reset_postgres(&database_url).await;
    exercise_agent_proposal_binding_authority(state.clone()).await;
    assert_postgres_agent_proposal_v2_identity_parity(pool).await;
    assert_postgres_work_item_record_parity(pool).await;
    sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("release Hepta PostgreSQL test lock");
}

#[tokio::test]
async fn postgres_agent_binding_v3_migration_is_repeatable_and_disclosure_is_immutable() {
    let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
        eprintln!(
            "HEPTA_TEST_DATABASE_URL unset; Agent capability disclosure PostgreSQL test skipped"
        );
        return;
    };
    let mut lock = PgConnection::connect(&database_url)
        .await
        .expect("PostgreSQL Agent capability disclosure test lock");
    sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("serialize Hepta PostgreSQL tests");
    let state = AppState::connect(&database_url, security())
        .await
        .expect("Agent capability disclosure PostgreSQL state");
    let pool = state.pool.as_ref().expect("PostgreSQL pool");
    for application in ["second", "third"] {
        sqlx::raw_sql(include_str!(
            "../../../migrations/0041_add_hepta_agent_capability_disclosure.sql"
        ))
        .execute(pool)
        .await
        .unwrap_or_else(|error| panic!("0041 {application} application: {error}"));
    }
    reset_postgres(&database_url).await;
    let (binding_id, disclosure_hash) =
        exercise_agent_binding_v3_capability_disclosure(state.clone()).await;
    let row = sqlx::query(
        "select capability_disclosure_hash, record_json
         from hepta_agent_bindings where binding_id=$1",
    )
    .bind(binding_id)
    .fetch_one(pool)
    .await
    .expect("stored V3 Agent binding");
    let stored_hash: String = row.get("capability_disclosure_hash");
    let record: Value = row.get("record_json");
    assert_eq!(stored_hash, disclosure_hash);
    assert_eq!(record["capability_disclosure_hash"], disclosure_hash);
    assert_eq!(
        record["capability_disclosure"]["assurance"],
        "self_declared_unverified"
    );
    let mutation = sqlx::query(
        "update hepta_agent_bindings
         set record_json=jsonb_set(
             record_json,
             '{capability_disclosure,max_parallel_tasks}',
             '3'::jsonb
         )
         where binding_id=$1",
    )
    .bind(binding_id)
    .execute(pool)
    .await;
    assert!(mutation.is_err(), "capability disclosure must be immutable");
    let scientific_rows: i64 = sqlx::query_scalar(
        "select
            (select count(*) from hepta_paper_projects)
          + (select count(*) from hepta_paper_scores)
          + (select count(*) from hepta_paper_raid_scores)
          + (select count(*) from hepta_paper_chain_finality_projections)",
    )
    .fetch_one(pool)
    .await
    .expect("scientific and eligibility row count");
    assert_eq!(scientific_rows, 0);
    reset_postgres(&database_url).await;
    sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("release Agent capability disclosure PostgreSQL test lock");
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
    sqlx::raw_sql(include_str!(
        "../../../migrations/0039_add_hepta_review_assignments.sql"
    ))
    .execute(pool)
    .await
    .expect("0039 second application");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0039_add_hepta_review_assignments.sql"
    ))
    .execute(pool)
    .await
    .expect("0039 third application");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0040_add_hepta_evaluation_draft_quorum.sql"
    ))
    .execute(pool)
    .await
    .expect("0040 second application");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0040_add_hepta_evaluation_draft_quorum.sql"
    ))
    .execute(pool)
    .await
    .expect("0040 third application");
    let draft_only_fk: i64 = sqlx::query_scalar(
        "select count(*)::bigint from pg_constraint
         where conrelid='public.hepta_paper_review_assignments'::regclass
           and conname='hepta_paper_review_assignments_pinned_evaluation_fkey'",
    )
    .fetch_one(pool)
    .await
    .expect("inspect pre-0051 draft-only pinned authority");
    assert_eq!(draft_only_fk, 1);
    let pre_0051_definition: String = sqlx::query_scalar(
        "select pg_get_functiondef(
           'public.hepta_guard_review_assignment_draft_lifecycle_v1()'::regprocedure
         )",
    )
    .fetch_one(pool)
    .await
    .expect("inspect pre-0051 assignment lifecycle");
    assert!(!pre_0051_definition.contains("hepta_paper_evaluation_panel_attestations"));
    let failing_0051 =
        include_str!("../../../migrations/0051_bind_legacy_evaluation_panel_lifecycle.sql")
            .replacen("\ncommit;", "\nselect 1 / 0;\ncommit;", 1);
    let mut atomicity_connection = PgConnection::connect(&database_url)
        .await
        .expect("dedicated 0051 atomicity connection");
    sqlx::raw_sql(&failing_0051)
        .execute(&mut atomicity_connection)
        .await
        .expect_err("synthetic 0051 failure must abort the whole migration transaction");
    sqlx::query("rollback")
        .execute(&mut atomicity_connection)
        .await
        .expect("rollback expected-failing 0051 probe");
    atomicity_connection
        .close()
        .await
        .expect("close 0051 atomicity connection");
    let rollback_fk: i64 = sqlx::query_scalar(
        "select count(*)::bigint from pg_constraint
         where conrelid='public.hepta_paper_review_assignments'::regclass
           and conname='hepta_paper_review_assignments_pinned_evaluation_fkey'",
    )
    .fetch_one(pool)
    .await
    .expect("inspect 0051 rollback constraint state");
    assert_eq!(
        rollback_fk, 1,
        "failed 0051 must not leave a partial FK drop"
    );
    let rollback_definition: String = sqlx::query_scalar(
        "select pg_get_functiondef(
           'public.hepta_guard_review_assignment_draft_lifecycle_v1()'::regprocedure
         )",
    )
    .fetch_one(pool)
    .await
    .expect("inspect 0051 rollback function state");
    assert!(
        !rollback_definition.contains("hepta_paper_evaluation_panel_attestations"),
        "failed 0051 must not leave a partial function upgrade"
    );
    for ordinal in 1..=2 {
        sqlx::raw_sql(include_str!(
            "../../../migrations/0051_bind_legacy_evaluation_panel_lifecycle.sql"
        ))
        .execute(pool)
        .await
        .unwrap_or_else(|error| panic!("0051 repeat application {ordinal}: {error}"));
        super::verify_legacy_evaluation_panel_lifecycle_catalog(pool)
            .await
            .unwrap_or_else(|error| panic!("0051 catalog after application {ordinal}: {error}"));
    }
    sqlx::raw_sql(include_str!(
        "../../../migrations/0047_add_hepta_contribution_ledger_authority.sql"
    ))
    .execute(pool)
    .await
    .expect("0047 second application");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0047_add_hepta_contribution_ledger_authority.sql"
    ))
    .execute(pool)
    .await
    .expect("0047 third application");
    verify_contribution_ledger_authority_catalog(pool)
        .await
        .expect("0047 exact runtime catalog readiness");
    reset_postgres(&database_url).await;
    let postgres = run_review_flow(state.clone()).await;
    assert_eq!(postgres.contribution_room_event_count, 1);
    assert_eq!(postgres.evaluation_room_event_count, 2);
    assert_eq!(postgres.reproduction_room_event_count, 3);
    assert_eq!(postgres.appeal_room_event_count, 1);
    assert_eq!(postgres.resolution_room_event_count, 1);
    assert_eq!(postgres.appeal_outbox_event_count, 1);
    assert_eq!(postgres.resolution_outbox_event_count, 1);
    assert_eq!(postgres.appeal_room_settlement_state, "challenged");
    assert_eq!(postgres.resolution_room_settlement_state, "resolved");
    let reservation_count: i64 =
        sqlx::query_scalar("select count(*) from hepta_paper_contribution_ledger_reservations")
            .fetch_one(pool)
            .await
            .expect("promotion-time contribution ledger reservation count");
    assert_eq!(
        reservation_count, 1,
        "release promotion must reserve exactly one global ledger ID before ledger creation"
    );
    let reservation = sqlx::query(
        "select contribution_ledger_id,paper_project_id,release_candidate_hash
         from hepta_paper_contribution_ledger_reservations",
    )
    .fetch_one(pool)
    .await
    .expect("promotion-time contribution reservation");
    let mut reservation_replay = pool.begin().await.expect("reservation replay transaction");
    super::reserve_contribution_ledger_id_postgres(
        &mut reservation_replay,
        reservation.get("contribution_ledger_id"),
        reservation.get("paper_project_id"),
        &reservation.get::<String, _>("release_candidate_hash"),
        Utc::now(),
    )
    .await
    .expect("the exact PostgreSQL reservation replay must match memory idempotency");
    reservation_replay
        .rollback()
        .await
        .expect("rollback reservation replay probe");

    let drift_rejected = sqlx::query(
        "update hepta_paper_contribution_ledgers
         set record_json=jsonb_set(record_json,'{entries,0,contribution_points}','999'::jsonb)",
    )
    .execute(pool)
    .await
    .expect_err("frozen contribution ledger record_json drift must be rejected");
    assert!(
        drift_rejected
            .as_database_error()
            .and_then(|database| database
                .message()
                .contains("frozen_contribution_authority_is_immutable")
                .then_some(()))
            .is_some(),
        "record_json drift must fail at frozen contribution authority"
    );
    for statement in [
        "update hepta_paper_contribution_ledger_reservations set created_at=created_at",
        "truncate table hepta_paper_contribution_ledgers",
        "truncate table hepta_paper_contribution_ledger_reservations cascade",
    ] {
        let immutable = sqlx::query(statement)
            .execute(pool)
            .await
            .expect_err("row and statement authority mutation must be rejected");
        assert!(
            immutable
                .as_database_error()
                .is_some_and(|database| database
                    .message()
                    .contains("frozen_contribution_authority_is_immutable")),
            "immutable contribution authority rejected {statement:?} with the wrong error"
        );
    }

    sqlx::raw_sql(
        "alter table hepta_paper_contribution_ledgers
           drop constraint hepta_paper_contribution_ledgers_entries_json_check;
         alter table hepta_paper_contribution_ledgers
           add constraint hepta_paper_contribution_ledgers_entries_json_check
           check (jsonb_typeof(entries_json) is not distinct from 'array' or true);",
    )
    .execute(pool)
    .await
    .expect("install permissive OR TRUE constraint probe");
    let permissive_check = verify_contribution_ledger_authority_catalog(pool)
        .await
        .expect_err("runtime readiness must reject OR TRUE authority checks");
    assert!(permissive_check.contains("non-canonical exact definition"));
    sqlx::raw_sql(include_str!(
        "../../../migrations/0047_add_hepta_contribution_ledger_authority.sql"
    ))
    .execute(pool)
    .await
    .expect("0047 repairs permissive contribution checks");

    sqlx::raw_sql(
        "create table hepta_contribution_authority_decoy (contribution_ledger_id uuid);
         alter table hepta_contribution_authority_decoy
           add constraint hepta_contribution_ledger_reservations_non_nil_id_check
           check (contribution_ledger_id <> '00000000-0000-0000-0000-000000000000'::uuid);",
    )
    .execute(pool)
    .await
    .expect("install wrong-table duplicate constraint probe");
    let wrong_table = verify_contribution_ledger_authority_catalog(pool)
        .await
        .expect_err("runtime readiness must reject duplicate names on wrong tables");
    assert!(wrong_table.contains("globally unique managed names"));
    sqlx::query("drop table hepta_contribution_authority_decoy")
        .execute(pool)
        .await
        .expect("remove wrong-table constraint probe");

    sqlx::raw_sql(
        "create table hepta_contribution_index_decoy (
             artifact_manifest_id uuid,
             status text not null
         );
         drop index hepta_agent_proposals_one_accepted_artifact_manifest_idx;
         create unique index hepta_agent_proposals_one_accepted_artifact_manifest_idx
           on hepta_contribution_index_decoy (artifact_manifest_id)
           where status='accepted';",
    )
    .execute(pool)
    .await
    .expect("install wrong-table IF NOT EXISTS index probe");
    let wrong_index = verify_contribution_ledger_authority_catalog(pool)
        .await
        .expect_err("runtime readiness must reject a same-name index on the wrong table");
    assert!(wrong_index.contains("missing the accepted artifact unique index"));
    sqlx::raw_sql(include_str!(
        "../../../migrations/0047_add_hepta_contribution_ledger_authority.sql"
    ))
    .execute(pool)
    .await
    .expect("0047 repairs a same-name wrong-table authority index");
    sqlx::query("drop table hepta_contribution_index_decoy")
        .execute(pool)
        .await
        .expect("remove wrong-table index probe");

    sqlx::query(
        "alter table hepta_paper_contribution_ledgers
         drop constraint hepta_paper_contribution_ledgers_record_json_parity_check",
    )
    .execute(pool)
    .await
    .expect("remove contribution parity constraint for readiness negative evidence");
    let missing_parity = verify_contribution_ledger_authority_catalog(pool)
        .await
        .expect_err(
            "runtime readiness must reject a missing contribution ledger parity constraint",
        );
    assert!(missing_parity.contains("constraint catalog"));
    sqlx::raw_sql(include_str!(
        "../../../migrations/0047_add_hepta_contribution_ledger_authority.sql"
    ))
    .execute(pool)
    .await
    .expect("0047 repairs contribution ledger parity catalog");
    verify_contribution_ledger_authority_catalog(pool)
        .await
        .expect("0047 repaired exact contribution authority readiness");
    let draft_count: i64 = sqlx::query_scalar("select count(*) from hepta_paper_evaluation_drafts")
        .fetch_one(pool)
        .await
        .expect("evaluation draft count");
    assert!(
        draft_count >= 1,
        "review flow must persist an evaluation draft"
    );
    let consumed_panels = sqlx::query(
        "select a.pinned_evaluation_id,
                bool_or(d.evaluation_id is not null) as draft_backed,
                count(*) as assignment_count,
                count(distinct a.slot) as slot_count,
                min(a.version) as minimum_version,
                max(a.version) as maximum_version
         from hepta_paper_review_assignments a
         left join hepta_paper_evaluation_drafts d
           on d.evaluation_id=a.pinned_evaluation_id
         where a.status='consumed'
         group by a.pinned_evaluation_id
         order by draft_backed",
    )
    .fetch_all(pool)
    .await
    .expect("consumed evaluation panel assignment groups");
    assert_eq!(
        consumed_panels.len(),
        2,
        "the review flow must consume exactly one draft-backed panel and one legacy superseding panel"
    );
    assert_eq!(
        consumed_panels
            .iter()
            .filter(|row| row.get::<bool, _>("draft_backed"))
            .count(),
        1,
        "exactly one consumed panel must be backed by the finalized evaluation draft"
    );
    for panel in consumed_panels {
        assert!(
            panel
                .get::<Option<Uuid>, _>("pinned_evaluation_id")
                .is_some(),
            "every consumed panel must retain its immutable evaluation identity"
        );
        assert_eq!(
            panel.get::<i64, _>("assignment_count"),
            3,
            "each consumed panel must contain exactly its evaluator and two reviewers"
        );
        assert_eq!(
            panel.get::<i64, _>("slot_count"),
            3,
            "each consumed panel must contain three distinct frozen slots"
        );
        assert_eq!(panel.get::<i64, _>("minimum_version"), 3);
        assert_eq!(panel.get::<i64, _>("maximum_version"), 3);
    }
    let pinned_panel_assignments: i64 = sqlx::query_scalar(
        "select count(*) from hepta_paper_review_assignments where status='pinned'",
    )
    .fetch_one(pool)
    .await
    .expect("pinned evaluation panel assignment count");
    assert_eq!(
        pinned_panel_assignments, 0,
        "a finalized draft must leave no pinned panel assignments"
    );
    assert!(
        sqlx::query(
            "update hepta_paper_review_assignments
             set status='claimed',version=version+1
             where assignment_id=(
               select assignment_id from hepta_paper_review_assignments
               where status='consumed' limit 1
             )",
        )
        .execute(pool)
        .await
        .is_err(),
        "PostgreSQL must reject reopening a consumed draft panel assignment"
    );
    assert!(
        sqlx::query("update hepta_paper_evaluation_drafts set draft_hash=$1")
            .bind(digest("immutable-draft-tamper"))
            .execute(pool)
            .await
            .is_err(),
        "PostgreSQL must reject immutable evaluation draft field changes"
    );
    assert!(
        sqlx::query("update hepta_paper_evaluation_draft_attestations set record_json=record_json")
            .execute(pool)
            .await
            .is_err(),
        "PostgreSQL must reject every evaluation draft attestation update"
    );
    let memory = run_review_flow(AppState::new(security())).await;
    assert_eq!(postgres, memory);

    // Model the first-upgrade ambiguity in the disposable database: the
    // release candidate remains live, but neither the pre-0047 frozen ledger
    // nor the post-0047 reservation that retains its caller-owned ID exists.
    // Removing the row guards is test-only setup; reset_postgres immediately
    // rebuilds the exact production catalog after the fail-closed assertion.
    sqlx::raw_sql(
        "drop trigger hepta_paper_contribution_ledger_immutable_guard
             on hepta_paper_contribution_ledgers;
         drop trigger hepta_contribution_ledger_reservation_immutable_guard
             on hepta_paper_contribution_ledger_reservations;
         delete from hepta_paper_contribution_ledgers;
         delete from hepta_paper_contribution_ledger_reservations;",
    )
    .execute(pool)
    .await
    .expect("construct a pre-0047 release candidate with no recoverable ledger ID");
    let mut legacy_upgrade_connection = PgConnection::connect(&database_url)
        .await
        .expect("dedicated 0047 legacy-upgrade probe connection");
    let legacy_upgrade_error = sqlx::raw_sql(include_str!(
        "../../../migrations/0047_add_hepta_contribution_ledger_authority.sql"
    ))
    .execute(&mut legacy_upgrade_connection)
    .await
    .expect_err("0047 first upgrade must reject a live candidate missing both authorities");
    let legacy_upgrade_database_error = legacy_upgrade_error
        .as_database_error()
        .expect("legacy release-candidate rejection must be a PostgreSQL error");
    assert_eq!(
        legacy_upgrade_database_error.code().as_deref(),
        Some("23514")
    );
    assert_eq!(
        legacy_upgrade_database_error.message(),
        "legacy_release_candidate_missing_contribution_ledger"
    );
    sqlx::query("rollback")
        .execute(&mut legacy_upgrade_connection)
        .await
        .expect("rollback expected-failing 0047 legacy-upgrade probe");
    legacy_upgrade_connection
        .close()
        .await
        .expect("close dedicated 0047 legacy-upgrade probe connection");
    reset_postgres(&database_url).await;
    let postgres_recovery = run_draft_lease_recovery_flow(state).await;
    let memory_recovery = run_draft_lease_recovery_flow(AppState::new(security())).await;
    assert_eq!(
        postgres_recovery, memory_recovery,
        "SIGKILL-equivalent draft expiry, reassignment and finalization must match memory"
    );
    sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("release Hepta PostgreSQL test lock");
}

#[tokio::test]
async fn postgres_legacy_panel_and_rework_commitment_hostile_guards_are_atomic() {
    let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
        eprintln!(
            "HEPTA_TEST_DATABASE_URL unset; legacy panel/rework PostgreSQL hostile test skipped"
        );
        return;
    };
    let mut lock = PgConnection::connect(&database_url)
        .await
        .expect("PostgreSQL legacy panel/rework test lock");
    sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("serialize Hepta PostgreSQL tests");
    let state = AppState::connect(&database_url, security())
        .await
        .expect("legacy panel/rework PostgreSQL state");
    reset_postgres(&database_url).await;
    let pool = state.pool.as_ref().expect("PostgreSQL pool");
    sqlx::raw_sql(
        "drop trigger if exists hepta_paper_rework_finality_v2_lineage_guard
           on hepta_paper_chain_finality_preparations_v2;
         drop trigger if exists hepta_joint_submission_rework_withdrawal_trigger
           on hepta_joint_paper_submissions;
         drop table if exists hepta_paper_rework_resubmissions cascade;
         drop table if exists hepta_paper_reworks cascade;
         alter table hepta_paper_projects
           drop constraint if exists hepta_paper_projects_active_rework_shape_check,
           drop column if exists active_rework_id,
           drop column if exists active_rework_cycle,
           drop column if exists rework_expires_at;
         drop function if exists hepta_validate_paper_rework_insert();
         drop function if exists hepta_validate_paper_rework_resubmission_insert();
         drop function if exists hepta_guard_joint_submission_rework_withdrawal();
         drop function if exists hepta_reject_paper_rework_mutation();
         drop function if exists hepta_validate_paper_rework_finality_lineage();
         drop function if exists hepta_paper_rework_content_commitment_sha256(jsonb);
         drop function if exists hepta_paper_rework_content_projection(jsonb);",
    )
    .execute(pool)
    .await
    .expect("construct pre-0050 schema for atomicity proof");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0043_add_hepta_challenge_ruleset_v1.sql"
    ))
    .execute(pool)
    .await
    .expect("restore pre-0050 ChallengeRuleset guard");
    let failing_0050 = include_str!("../../../migrations/0050_add_hepta_paper_rework.sql")
        .replacen("\ncommit;", "\nselect 1 / 0;\ncommit;", 1);
    let mut rework_atomicity_connection = PgConnection::connect(&database_url)
        .await
        .expect("dedicated 0050 atomicity connection");
    sqlx::raw_sql(&failing_0050)
        .execute(&mut rework_atomicity_connection)
        .await
        .expect_err("synthetic 0050 failure must abort the whole migration transaction");
    sqlx::query("rollback")
        .execute(&mut rework_atomicity_connection)
        .await
        .expect("rollback expected-failing 0050 probe");
    rework_atomicity_connection
        .close()
        .await
        .expect("close 0050 atomicity connection");
    let partial_0050_tables: i64 = sqlx::query_scalar(
        "select count(*)::bigint from pg_class
         where oid in (
           to_regclass('public.hepta_paper_reworks'),
           to_regclass('public.hepta_paper_rework_resubmissions')
         ) and relkind='r'",
    )
    .fetch_one(pool)
    .await
    .expect("inspect failed 0050 tables");
    let partial_0050_columns: i64 = sqlx::query_scalar(
        "select count(*)::bigint from pg_attribute
         where attrelid='public.hepta_paper_projects'::regclass
           and attname in ('active_rework_id','active_rework_cycle','rework_expires_at')
           and attnum>0 and not attisdropped",
    )
    .fetch_one(pool)
    .await
    .expect("inspect failed 0050 columns");
    assert_eq!(partial_0050_tables, 0);
    assert_eq!(partial_0050_columns, 0);
    sqlx::raw_sql(include_str!(
        "../../../migrations/0050_add_hepta_paper_rework.sql"
    ))
    .execute(pool)
    .await
    .expect("apply 0050 after atomicity proof");
    super::verify_rework_migration_catalog(pool)
        .await
        .expect("verify 0050 after atomicity proof");

    sqlx::raw_sql(
        "alter table hepta_paper_review_assignments
           disable trigger hepta_review_assignment_draft_lifecycle_guard;",
    )
    .execute(pool)
    .await
    .expect("disable 0051 lifecycle guard hostile");
    assert!(
        super::verify_legacy_evaluation_panel_lifecycle_catalog(pool)
            .await
            .expect_err("disabled 0051 trigger must fail readiness")
            .contains("globally unique, exact, and ALWAYS")
    );
    restore_legacy_panel_lifecycle_0051(pool, "disabled trigger").await;

    sqlx::raw_sql(
        "create or replace function hepta_test_permissive_panel_lifecycle()
         returns trigger language plpgsql as $$ begin return new; end; $$;
         drop trigger hepta_review_assignment_draft_lifecycle_guard
           on hepta_paper_review_assignments;
         create trigger hepta_review_assignment_draft_lifecycle_guard
           before update on hepta_paper_review_assignments
           for each row execute function hepta_test_permissive_panel_lifecycle();
         alter table hepta_paper_review_assignments
           enable always trigger hepta_review_assignment_draft_lifecycle_guard;",
    )
    .execute(pool)
    .await
    .expect("install wrong-function 0051 trigger hostile");
    assert!(
        super::verify_legacy_evaluation_panel_lifecycle_catalog(pool)
            .await
            .expect_err("wrong-function 0051 trigger must fail readiness")
            .contains("globally unique, exact, and ALWAYS")
    );
    restore_legacy_panel_lifecycle_0051(pool, "wrong trigger function").await;
    sqlx::query("drop function hepta_test_permissive_panel_lifecycle()")
        .execute(pool)
        .await
        .expect("remove wrong-function hostile helper");

    sqlx::raw_sql(
        "drop trigger hepta_review_assignment_draft_lifecycle_guard
           on hepta_paper_review_assignments;
         create trigger hepta_review_assignment_draft_lifecycle_guard
           before update on hepta_paper_projects
           for each row execute function hepta_guard_review_assignment_draft_lifecycle_v1();
         alter table hepta_paper_projects
           enable always trigger hepta_review_assignment_draft_lifecycle_guard;",
    )
    .execute(pool)
    .await
    .expect("install wrong-relation 0051 trigger hostile");
    assert!(
        super::verify_legacy_evaluation_panel_lifecycle_catalog(pool)
            .await
            .expect_err("wrong-relation 0051 trigger must fail readiness")
            .contains("globally unique, exact, and ALWAYS")
    );
    sqlx::query(
        "drop trigger hepta_review_assignment_draft_lifecycle_guard on hepta_paper_projects",
    )
    .execute(pool)
    .await
    .expect("remove wrong-relation trigger hostile");
    restore_legacy_panel_lifecycle_0051(pool, "wrong trigger relation").await;

    sqlx::raw_sql(
        "create or replace function hepta_guard_review_assignment_draft_lifecycle_v1()
         returns trigger language plpgsql
         set search_path = pg_catalog, public
         as $$ begin if true or false then return new; end if; return new; end; $$;",
    )
    .execute(pool)
    .await
    .expect("install permissive OR TRUE lifecycle hostile");
    assert!(
        super::verify_legacy_evaluation_panel_lifecycle_catalog(pool)
            .await
            .expect_err("permissive 0051 function must fail readiness")
            .contains("not the exact 0051 authority")
    );
    restore_legacy_panel_lifecycle_0051(pool, "permissive function body").await;

    sqlx::raw_sql(
        "alter table hepta_paper_review_assignments
         add constraint hepta_paper_review_assignments_pinned_evaluation_fkey
         foreign key (pinned_evaluation_id)
         references hepta_paper_evaluation_drafts(evaluation_id);",
    )
    .execute(pool)
    .await
    .expect("install stale draft-only FK hostile");
    assert!(
        super::verify_legacy_evaluation_panel_lifecycle_catalog(pool)
            .await
            .expect_err("stale draft-only FK must fail readiness")
            .contains("draft-only authority")
    );
    restore_legacy_panel_lifecycle_0051(pool, "stale draft-only FK").await;

    let _ = run_full_flow(state.clone(), 3, FlowOptions::default()).await;
    let router = app(state.clone());
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
            "pg-rework-submission",
        )
        .await,
        StatusCode::OK,
    );
    let rejected_submission: JointPaperSubmission =
        serde_json::from_value(submission.clone()).expect("decode PostgreSQL rejected submission");
    claim_evaluation_panel_fixture(
        &router,
        paper_id,
        1,
        &external[0],
        [&external[1], &external[2]],
        "pg-rework-rejected-round-1",
    )
    .await;
    let rejected_evaluation_id = Uuid::new_v4();
    let rejected_key = "pg-rework-rejected-evaluation";
    let rejected_body = rejected_evaluation_body(
        paper_id,
        &submission,
        &external[0],
        [&external[1], &external[2]],
        rejected_evaluation_id,
        rejected_key,
    );

    sqlx::raw_sql(
        "create or replace function hepta_test_fail_legacy_panel_consume()
         returns trigger language plpgsql as $$
         begin
           if new.status='consumed' and new.slot='reviewer_1' then
             raise exception 'hepta_test_fail_legacy_panel_consume' using errcode='55000';
           end if;
           return new;
         end;
         $$;
         drop trigger if exists aaa_hepta_test_fail_legacy_panel_consume
           on hepta_paper_review_assignments;
         create trigger aaa_hepta_test_fail_legacy_panel_consume
           before update on hepta_paper_review_assignments
           for each row execute function hepta_test_fail_legacy_panel_consume();",
    )
    .execute(pool)
    .await
    .expect("install deterministic second-panel-transition failure");
    let failed = user_post(
        &router,
        &external[0],
        "create_paper_evaluation_v1",
        &format!("/v2/hepta/papers/{paper_id}/evaluations"),
        rejected_key,
        rejected_body.clone(),
    )
    .await;
    assert_eq!(failed.0, StatusCode::INTERNAL_SERVER_ERROR, "{}", failed.1);
    sqlx::raw_sql(
        "drop trigger aaa_hepta_test_fail_legacy_panel_consume
           on hepta_paper_review_assignments;
         drop function hepta_test_fail_legacy_panel_consume();",
    )
    .execute(pool)
    .await
    .expect("remove deterministic panel transition failure");
    let failed_evaluation_count: i64 = sqlx::query_scalar(
        "select count(*)::bigint from hepta_paper_evaluations where evaluation_id=$1",
    )
    .bind(rejected_evaluation_id)
    .fetch_one(pool)
    .await
    .expect("count rolled-back evaluation");
    assert_eq!(failed_evaluation_count, 0);
    let rolled_back_panel = sqlx::query(
        "select status,version,pinned_evaluation_id
         from hepta_paper_review_assignments
         where submission_id=$1 and review_round=1 and slot<>'reproducer'
         order by slot",
    )
    .bind(rejected_submission.submission_id)
    .fetch_all(pool)
    .await
    .expect("inspect rolled-back exact panel");
    assert_eq!(rolled_back_panel.len(), 3);
    for row in &rolled_back_panel {
        assert_eq!(row.get::<String, _>("status"), "claimed");
        assert_eq!(row.get::<i64, _>("version"), 1);
        assert_eq!(row.get::<Option<Uuid>, _>("pinned_evaluation_id"), None);
    }
    let failed_idempotency_count: i64 = sqlx::query_scalar(
        "select count(*)::bigint from hepta_paper_raid_idempotency
         where operation='create_paper_evaluation_v1' and idempotency_key=$1",
    )
    .bind(rejected_key)
    .fetch_one(pool)
    .await
    .expect("count rolled-back evaluation idempotency");
    assert_eq!(failed_idempotency_count, 0);

    let rejected = assert_status(
        user_post(
            &router,
            &external[0],
            "create_paper_evaluation_v1",
            &format!("/v2/hepta/papers/{paper_id}/evaluations"),
            rejected_key,
            rejected_body,
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(rejected["status"], "rejected");
    let consumed_panel = sqlx::query(
        "select status,version,pinned_evaluation_id
         from hepta_paper_review_assignments
         where submission_id=$1 and review_round=1 and slot<>'reproducer'
         order by slot",
    )
    .bind(rejected_submission.submission_id)
    .fetch_all(pool)
    .await
    .expect("inspect consumed exact panel");
    assert_eq!(consumed_panel.len(), 3);
    for row in &consumed_panel {
        assert_eq!(row.get::<String, _>("status"), "consumed");
        assert_eq!(row.get::<i64, _>("version"), 3);
        assert_eq!(
            row.get::<Option<Uuid>, _>("pinned_evaluation_id"),
            Some(rejected_evaluation_id)
        );
    }

    let probe_now: chrono::DateTime<Utc> = sqlx::query_scalar("select clock_timestamp()")
        .fetch_one(pool)
        .await
        .expect("PostgreSQL panel probe clock");
    for (label, player_id, review_round, slot) in [
        (
            "wrong-player",
            external[3].player_id,
            1_u64,
            ReviewAssignmentSlot::Reviewer1,
        ),
        (
            "wrong-slot",
            external[0].player_id,
            1_u64,
            ReviewAssignmentSlot::Reviewer1,
        ),
        (
            "wrong-round",
            external[0].player_id,
            2_u64,
            ReviewAssignmentSlot::Evaluator,
        ),
    ] {
        let mut tx = pool.begin().await.expect("begin hostile panel probe");
        let claimed = postgres_review_assignment_probe(
            paper_id,
            rejected_submission.submission_id,
            player_id,
            review_round,
            slot,
            probe_now,
        );
        insert_postgres_review_assignment_probe(&mut tx, &claimed).await;
        let mut pinned = claimed.clone();
        pinned.status = ReviewAssignmentStatus::Pinned;
        pinned.pinned_evaluation_id = Some(rejected_evaluation_id);
        pinned.version = 2;
        pinned.updated_at = probe_now;
        let error = update_postgres_review_assignment_probe(
            &mut tx,
            &pinned,
            1,
            ReviewAssignmentStatus::Claimed,
        )
        .await
        .unwrap_err();
        assert!(
            error.as_database_error().is_some_and(|database| database
                .message()
                .contains("review assignment identity or frozen panel lifecycle changed")),
            "{label} pin failed with unexpected error: {error}"
        );
        tx.rollback()
            .await
            .unwrap_or_else(|rollback| panic!("rollback {label} panel probe: {rollback}"));
    }

    let mut exact_tx = pool
        .begin()
        .await
        .expect("begin exact panel lifecycle probe");
    let mut exact_panel = vec![
        postgres_review_assignment_probe(
            paper_id,
            rejected_submission.submission_id,
            external[0].player_id,
            1,
            ReviewAssignmentSlot::Evaluator,
            probe_now,
        ),
        postgres_review_assignment_probe(
            paper_id,
            rejected_submission.submission_id,
            external[1].player_id,
            1,
            ReviewAssignmentSlot::Reviewer1,
            probe_now,
        ),
        postgres_review_assignment_probe(
            paper_id,
            rejected_submission.submission_id,
            external[2].player_id,
            1,
            ReviewAssignmentSlot::Reviewer2,
            probe_now,
        ),
    ];
    for assignment in &exact_panel {
        insert_postgres_review_assignment_probe(&mut exact_tx, assignment).await;
    }
    for assignment in &mut exact_panel {
        assignment.status = ReviewAssignmentStatus::Pinned;
        assignment.pinned_evaluation_id = Some(rejected_evaluation_id);
        assignment.version = 2;
        assignment.updated_at = probe_now;
        assert_eq!(
            update_postgres_review_assignment_probe(
                &mut exact_tx,
                assignment,
                1,
                ReviewAssignmentStatus::Claimed,
            )
            .await
            .expect("exact claimed-to-pinned transition"),
            1
        );
    }
    let pinned_versions = sqlx::query(
        "select status,version,pinned_evaluation_id
         from hepta_paper_review_assignments
         where assignment_id = any($1) order by slot",
    )
    .bind(
        exact_panel
            .iter()
            .map(|assignment| assignment.assignment_id)
            .collect::<Vec<_>>(),
    )
    .fetch_all(&mut *exact_tx)
    .await
    .expect("inspect exact pinned panel");
    assert_eq!(pinned_versions.len(), 3);
    assert!(pinned_versions.iter().all(|row| {
        row.get::<String, _>("status") == "pinned"
            && row.get::<i64, _>("version") == 2
            && row.get::<Option<Uuid>, _>("pinned_evaluation_id") == Some(rejected_evaluation_id)
    }));
    for assignment in &mut exact_panel {
        assignment.status = ReviewAssignmentStatus::Consumed;
        assignment.version = 3;
        assignment.updated_at = probe_now;
        assert_eq!(
            update_postgres_review_assignment_probe(
                &mut exact_tx,
                assignment,
                2,
                ReviewAssignmentStatus::Pinned,
            )
            .await
            .expect("exact pinned-to-consumed transition"),
            1
        );
    }
    let consumed_versions = sqlx::query(
        "select status,version,pinned_evaluation_id
         from hepta_paper_review_assignments
         where assignment_id = any($1) order by slot",
    )
    .bind(
        exact_panel
            .iter()
            .map(|assignment| assignment.assignment_id)
            .collect::<Vec<_>>(),
    )
    .fetch_all(&mut *exact_tx)
    .await
    .expect("inspect exact consumed panel");
    assert_eq!(consumed_versions.len(), 3);
    assert!(consumed_versions.iter().all(|row| {
        row.get::<String, _>("status") == "consumed"
            && row.get::<i64, _>("version") == 3
            && row.get::<Option<Uuid>, _>("pinned_evaluation_id") == Some(rejected_evaluation_id)
    }));
    exact_tx
        .rollback()
        .await
        .expect("rollback exact panel lifecycle probe");

    let _ = run_full_flow(state.clone(), 4, FlowOptions::default()).await;
    let other_paper_id = Uuid::from_u128(0x5000_0000_0000_4000_8000_0000_0000_0000 + 4 * 0x100);
    let other_submission_id: Uuid = sqlx::query_scalar(
        "select submission_id from hepta_joint_paper_submissions
         where paper_project_id=$1 and status='submission_ready'",
    )
    .bind(other_paper_id)
    .fetch_one(pool)
    .await
    .expect("load other Paper submission probe");
    let mut cross_tx = pool.begin().await.expect("begin cross-Paper panel probe");
    let cross_claimed = postgres_review_assignment_probe(
        other_paper_id,
        other_submission_id,
        external[4].player_id,
        1,
        ReviewAssignmentSlot::Evaluator,
        probe_now,
    );
    insert_postgres_review_assignment_probe(&mut cross_tx, &cross_claimed).await;
    let mut cross_pinned = cross_claimed.clone();
    cross_pinned.status = ReviewAssignmentStatus::Pinned;
    cross_pinned.pinned_evaluation_id = Some(rejected_evaluation_id);
    cross_pinned.version = 2;
    cross_pinned.updated_at = probe_now;
    let cross_error = update_postgres_review_assignment_probe(
        &mut cross_tx,
        &cross_pinned,
        1,
        ReviewAssignmentStatus::Claimed,
    )
    .await
    .expect_err("cross-Paper/submission evaluation pin must fail");
    assert!(cross_error
        .as_database_error()
        .is_some_and(|database| database
            .message()
            .contains("review assignment identity or frozen panel lifecycle changed")));
    cross_tx
        .rollback()
        .await
        .expect("rollback cross-Paper/submission panel probe");

    let rust_commitment = rework_content_commitment_sha256(&rejected_submission)
        .expect("Rust rejected scientific commitment");
    let sql_commitment: String = sqlx::query_scalar(
        "select hepta_paper_rework_content_commitment_sha256(record_json)
         from hepta_joint_paper_submissions where submission_id=$1",
    )
    .bind(rejected_submission.submission_id)
    .fetch_one(pool)
    .await
    .expect("PostgreSQL rejected scientific commitment");
    assert_eq!(sql_commitment, rust_commitment);

    let paper_version: i64 =
        sqlx::query_scalar("select version from hepta_paper_projects where paper_project_id=$1")
            .bind(paper_id)
            .fetch_one(pool)
            .await
            .expect("load rejected Paper version");
    let active_probe = postgres_review_assignment_probe(
        paper_id,
        rejected_submission.submission_id,
        external[0].player_id,
        1,
        ReviewAssignmentSlot::Evaluator,
        probe_now,
    );
    let mut active_tx = pool
        .begin()
        .await
        .expect("begin partial active panel probe");
    insert_postgres_review_assignment_probe(&mut active_tx, &active_probe).await;
    active_tx
        .commit()
        .await
        .expect("commit partial active panel probe");
    let partial_key = "pg-rework-partial-active-panel";
    assert_eq!(
        error_code(
            user_post(
                &router,
                &authors[0],
                "start_paper_rework_v1",
                &format!("/v2/hepta/papers/{paper_id}/reworks"),
                partial_key,
                paper_rework_body(
                    paper_id,
                    rejected_submission.submission_id,
                    rejected_evaluation_id,
                    u64::try_from(paper_version).expect("Paper version fits u64"),
                    Uuid::new_v4(),
                    2,
                    &authors[0],
                    digest("pg-rework-partial-panel"),
                    Utc::now().timestamp(),
                    partial_key,
                ),
            )
            .await,
            StatusCode::CONFLICT,
        ),
        "paper_rework_review_assignment_active"
    );
    assert_eq!(
        sqlx::query("delete from hepta_paper_review_assignments where assignment_id=$1")
            .bind(active_probe.assignment_id)
            .execute(pool)
            .await
            .expect("remove partial active panel probe")
            .rows_affected(),
        1
    );

    let forged_created_at: chrono::DateTime<Utc> = sqlx::query_scalar("select clock_timestamp()")
        .fetch_one(pool)
        .await
        .expect("PostgreSQL forged rework clock");
    let forged_commitment = digest("forged-rework-content-commitment");
    assert_ne!(forged_commitment, rust_commitment);
    let forged_rework = PaperReworkRecordV1 {
        schema: "hepta.paper_raid.rework_record.v1".to_string(),
        rework_id: Uuid::new_v4(),
        paper_project_id: paper_id,
        rejected_evaluation_id,
        rejected_submission_id: rejected_submission.submission_id,
        rejected_revision_id: rejected_submission.revision_id,
        rejected_release_candidate_hash: rejected_submission.release_candidate_hash.clone(),
        rejected_paper_bundle_hash: rejected_submission.paper_bundle_hash.clone(),
        rejected_rework_content_commitment_sha256: forged_commitment,
        rework_cycle: 2,
        author_player_id: authors[0].player_id,
        signing_key_id: authors[0].human_key_id.clone(),
        signing_public_key: authors[0].human_public_key.clone(),
        signing_public_key_hash: authors[0].human_public_key_hash.clone(),
        reason_hash: digest("forged-rework-reason"),
        signed_at_unix: forged_created_at.timestamp(),
        signature: BASE64.encode([0_u8; 64]),
        request_hash: digest("forged-rework-request"),
        rework_expires_at: forged_created_at + chrono::Duration::hours(24),
        version: 1,
        created_at: forged_created_at,
    };
    let forged_error = sqlx::query(
        "insert into hepta_paper_reworks (
           rework_id,paper_project_id,rejected_evaluation_id,rejected_submission_id,
           rejected_revision_id,rejected_release_candidate_hash,rejected_paper_bundle_hash,
           rejected_rework_content_commitment_sha256,rework_cycle,author_player_id,
           signing_key_id,signing_public_key,signing_public_key_hash,reason_hash,
           signed_at_unix,request_hash,signature,rework_expires_at,version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20::jsonb,$21)",
    )
    .bind(forged_rework.rework_id)
    .bind(forged_rework.paper_project_id)
    .bind(forged_rework.rejected_evaluation_id)
    .bind(forged_rework.rejected_submission_id)
    .bind(forged_rework.rejected_revision_id)
    .bind(&forged_rework.rejected_release_candidate_hash)
    .bind(&forged_rework.rejected_paper_bundle_hash)
    .bind(&forged_rework.rejected_rework_content_commitment_sha256)
    .bind(i64::try_from(forged_rework.rework_cycle).expect("rework cycle fits i64"))
    .bind(forged_rework.author_player_id)
    .bind(&forged_rework.signing_key_id)
    .bind(&forged_rework.signing_public_key)
    .bind(&forged_rework.signing_public_key_hash)
    .bind(&forged_rework.reason_hash)
    .bind(forged_rework.signed_at_unix)
    .bind(&forged_rework.request_hash)
    .bind(&forged_rework.signature)
    .bind(forged_rework.rework_expires_at)
    .bind(i64::try_from(forged_rework.version).expect("rework version fits i64"))
    .bind(serde_json::to_value(&forged_rework).expect("serialize forged rework"))
    .bind(forged_rework.created_at)
    .execute(pool)
    .await
    .expect_err("direct forged scientific commitment must fail at the database");
    assert!(
        forged_error
            .as_database_error()
            .is_some_and(|database| database
                .message()
                .contains("hepta_paper_rework_state_invalid")),
        "forged commitment failed with unexpected error: {forged_error}"
    );
    let forged_count: i64 = sqlx::query_scalar("select count(*)::bigint from hepta_paper_reworks")
        .fetch_one(pool)
        .await
        .expect("count rejected forged reworks");
    assert_eq!(forged_count, 0);

    let valid_key = "pg-rework-valid-after-hostile";
    let valid = assert_status(
        user_post(
            &router,
            &authors[0],
            "start_paper_rework_v1",
            &format!("/v2/hepta/papers/{paper_id}/reworks"),
            valid_key,
            paper_rework_body(
                paper_id,
                rejected_submission.submission_id,
                rejected_evaluation_id,
                u64::try_from(paper_version).expect("Paper version fits u64"),
                Uuid::new_v4(),
                2,
                &authors[0],
                digest("pg-rework-valid-after-hostile"),
                Utc::now().timestamp(),
                valid_key,
            ),
        )
        .await,
        StatusCode::CREATED,
    );
    assert_eq!(
        valid["rejected_rework_content_commitment_sha256"],
        rust_commitment
    );

    reset_postgres(&database_url).await;
    sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("release Hepta PostgreSQL legacy panel/rework lock");
}

async fn assert_paper_finality_v2_trigger_replay_repairs_tampering(pool: &sqlx::PgPool) {
    sqlx::raw_sql(
        "alter table hepta_paper_evaluations
             disable trigger hepta_paper_evaluations_finality_v2_source_guard;
         drop trigger hepta_paper_reproductions_finality_v2_source_guard
             on hepta_paper_reproductions;
         create trigger hepta_paper_reproductions_finality_v2_source_guard
             before update on hepta_paper_reproductions
             for each row execute function
                 hepta_reject_paper_finality_v2_evidence_mutation();",
    )
    .execute(pool)
    .await
    .expect("tamper V2 source triggers before migration replay");

    let tampered = sqlx::query(
        "select relation.relname as relation_name,
                trigger.tgname as trigger_name,
                procedure.proname as function_name,
                trigger.tgtype::integer as trigger_type,
                trigger.tgenabled::text as enabled
         from pg_trigger as trigger
         join pg_class as relation on relation.oid=trigger.tgrelid
         join pg_proc as procedure on procedure.oid=trigger.tgfoid
         where not trigger.tgisinternal
           and trigger.tgname in (
                'hepta_paper_evaluations_finality_v2_source_guard',
                'hepta_paper_reproductions_finality_v2_source_guard'
           )
         order by trigger.tgname",
    )
    .fetch_all(pool)
    .await
    .expect("inspect deliberately tampered V2 source triggers");
    assert_eq!(tampered.len(), 2);
    assert_eq!(tampered[0].get::<String, _>("enabled"), "D");
    assert_eq!(tampered[1].get::<i32, _>("trigger_type"), 19);
    assert_eq!(
        tampered[1].get::<String, _>("function_name"),
        "hepta_reject_paper_finality_v2_evidence_mutation"
    );

    sqlx::raw_sql(include_str!(
        "../../../migrations/0038_add_hepta_paper_chain_finality_v2.sql"
    ))
    .execute(pool)
    .await
    .expect("0038 must repair disabled and wrong-function source triggers");

    let repaired = sqlx::query(
        "select relation.relname as relation_name,
                trigger.tgname as trigger_name,
                procedure.proname as function_name,
                trigger.tgtype::integer as trigger_type,
                trigger.tgenabled::text as enabled
         from pg_trigger as trigger
         join pg_class as relation on relation.oid=trigger.tgrelid
         join pg_proc as procedure on procedure.oid=trigger.tgfoid
         where not trigger.tgisinternal
           and trigger.tgname in (
                'hepta_paper_evaluations_finality_v2_source_guard',
                'hepta_paper_reproductions_finality_v2_source_guard'
           )
         order by trigger.tgname",
    )
    .fetch_all(pool)
    .await
    .expect("inspect repaired V2 source triggers");
    assert_eq!(repaired.len(), 2);
    for row in repaired {
        let trigger_name = row.get::<String, _>("trigger_name");
        let expected_relation =
            if trigger_name == "hepta_paper_evaluations_finality_v2_source_guard" {
                "hepta_paper_evaluations"
            } else {
                assert_eq!(
                    trigger_name,
                    "hepta_paper_reproductions_finality_v2_source_guard"
                );
                "hepta_paper_reproductions"
            };
        assert_eq!(row.get::<String, _>("relation_name"), expected_relation);
        assert_eq!(
            row.get::<String, _>("function_name"),
            "hepta_reject_paper_finality_v2_source_mutation"
        );
        assert_eq!(
            row.get::<i32, _>("trigger_type"),
            31,
            "source guard must be BEFORE ROW INSERT OR UPDATE OR DELETE"
        );
        assert_eq!(
            row.get::<String, _>("enabled"),
            "A",
            "source guard must be ENABLE ALWAYS"
        );
    }
}

async fn assert_paper_finality_v2_constraint_replay_rejects_same_name_tampering(
    pool: &sqlx::PgPool,
) {
    let baseline = sqlx::query(
        "select constraint_count, catalog_sha256
         from public.hepta_paper_finality_v2_constraint_catalog_fingerprint()",
    )
    .fetch_one(pool)
    .await
    .expect("read canonical V2 constraint catalog fingerprint");
    assert_eq!(baseline.get::<i64, _>("constraint_count"), 77);
    assert_eq!(
        baseline.get::<String, _>("catalog_sha256"),
        "910d4454106f5722ad44c6c9095bf48d585dfaa9501fc40d9ef377fd57c3f3ba"
    );

    sqlx::raw_sql(
        "alter table hepta_paper_chain_finality_window_arms_v2
             drop constraint hepta_paper_finality_v2_arm_values_check;
         alter table hepta_paper_chain_finality_window_arms_v2
             add constraint hepta_paper_finality_v2_arm_values_check check (true);",
    )
    .execute(pool)
    .await
    .expect("replace a V2 CHECK with a weaker same-name constraint");
    let tampered_sha256: String = sqlx::query_scalar(
        "select catalog_sha256
         from public.hepta_paper_finality_v2_constraint_catalog_fingerprint()",
    )
    .fetch_one(pool)
    .await
    .expect("fingerprint deliberately weakened V2 constraint catalog");
    assert_ne!(
        tampered_sha256,
        "910d4454106f5722ad44c6c9095bf48d585dfaa9501fc40d9ef377fd57c3f3ba"
    );

    let replay_error = sqlx::raw_sql(include_str!(
        "../../../migrations/0038_add_hepta_paper_chain_finality_v2.sql"
    ))
    .execute(pool)
    .await
    .expect_err("0038 replay must reject a weaker same-name constraint");
    let replay_database_error = replay_error
        .as_database_error()
        .expect("constraint-catalog rejection must be a PostgreSQL error");
    assert_eq!(replay_database_error.code().as_deref(), Some("55000"));
    assert_eq!(
        replay_database_error.message(),
        "hepta_paper_chain_finality_v2_constraint_catalog_mismatch"
    );

    sqlx::query(
        "alter table hepta_paper_chain_finality_window_arms_v2
         drop constraint hepta_paper_finality_v2_arm_values_check",
    )
    .execute(pool)
    .await
    .expect("remove deliberately weakened V2 CHECK");
    sqlx::raw_sql(include_str!(
        "../../../migrations/0038_add_hepta_paper_chain_finality_v2.sql"
    ))
    .execute(pool)
    .await
    .expect("0038 replay must restore a missing canonical V2 CHECK");

    let repaired = sqlx::query(
        "select constraint_count, catalog_sha256
         from public.hepta_paper_finality_v2_constraint_catalog_fingerprint()",
    )
    .fetch_one(pool)
    .await
    .expect("read repaired V2 constraint catalog fingerprint");
    assert_eq!(repaired.get::<i64, _>("constraint_count"), 77);
    assert_eq!(
        repaired.get::<String, _>("catalog_sha256"),
        "910d4454106f5722ad44c6c9095bf48d585dfaa9501fc40d9ef377fd57c3f3ba"
    );
}

async fn prepare_paper_finality_v2_against_stale_repeatable_read_writer(
    state: AppState,
) -> PaperTrnmFinalityPreparationV2 {
    let armed = arm_paper_chain_finality_v2_window(state).await;
    let state = armed.state;
    let legacy = armed.legacy;
    let arm = armed.arm;
    let pool = state.pool.as_ref().expect("PostgreSQL pool").clone();
    let (final_checkpoint, final_proof) = synthetic_authenticated_checkpoint(
        &arm.start_checkpoint,
        arm.start_checkpoint.height + 1,
        arm.earliest_final_checkpoint_time_unix_ms,
        "repeatable-read-deadline",
    );
    seed_authenticated_chain_time_checkpoint(&state, &final_checkpoint, &final_proof).await;

    let mut stale_tx = pool.begin().await.expect("begin stale writer transaction");
    sqlx::query("set transaction isolation level repeatable read")
        .execute(&mut *stale_tx)
        .await
        .expect("set stale writer isolation level");
    let stale_snapshot = sqlx::query(
        "select paper.finality_v2_seal_epoch as seal_epoch,
                evaluation.record_json as evaluation_json
         from hepta_paper_projects as paper
         join hepta_paper_evaluations as evaluation
           on evaluation.paper_project_id=paper.paper_project_id
         where paper.paper_project_id=$1 and evaluation.evaluation_id=$2",
    )
    .bind(legacy.paper_project_id)
    .bind(legacy.evaluation_id)
    .fetch_one(&mut *stale_tx)
    .await
    .expect("establish stale Paper and source snapshot");
    assert_eq!(stale_snapshot.get::<i16, _>("seal_epoch"), 0);
    let evaluation_json_before = stale_snapshot.get::<Value, _>("evaluation_json");
    let rollback_operation = "paper-finality-v2-repeatable-read-rollback-probe";
    let rollback_key = "paper-finality-v2-repeatable-read-rollback-probe";
    sqlx::query(
        "insert into hepta_paper_raid_idempotency (
            operation,idempotency_key,request_hash,aggregate_id,response_status,response_json
         ) values ($1,$2,$3,$4,201,$5::jsonb)",
    )
    .bind(rollback_operation)
    .bind(rollback_key)
    .bind(sha256_digest(b"repeatable-read-side-effect-must-roll-back"))
    .bind(legacy.paper_project_id)
    .bind(json!({"result":"must_roll_back_with_stale_writer"}))
    .execute(&mut *stale_tx)
    .await
    .expect("write stale writer side effect before preparation commits");

    let path = format!(
        "/v2/hepta/papers/{}/chain-finality-v2/prepare",
        legacy.paper_project_id
    );
    let body = json!({
        "arm_id":arm.arm_id,
        "submission_id":legacy.submission_id,
        "evaluation_id":legacy.evaluation_id,
        "latest_reproduction_id":legacy.reproduction_id,
        "research_session_id":legacy.research_session_id,
        "research_session_roster_version":legacy.research_session_roster_version,
        "final_checkpoint_hash":final_checkpoint.checkpoint_hash,
        "idempotency_key":"paper-finality-v2-repeatable-read-preparation",
    });
    let router = app(state);
    let created = assert_status(
        request(&router, "POST", &path, body, None).await,
        StatusCode::CREATED,
    );
    let preparation: PaperTrnmFinalityPreparationV2 =
        serde_json::from_value(created).expect("decode RR-race V2 preparation");

    let stale_error = sqlx::query(
        "update hepta_paper_evaluations
         set record_json=record_json
         where evaluation_id=$1",
    )
    .bind(legacy.evaluation_id)
    .execute(&mut *stale_tx)
    .await
    .expect_err("stale RR source writer must lose to the committed Paper anchor update");
    let stale_database_error = stale_error
        .as_database_error()
        .expect("stale writer failure must be a PostgreSQL error");
    assert_eq!(
        stale_database_error.code().as_deref(),
        Some("40001"),
        "the pre-existing Paper anchor must turn an old RR snapshot into a serialization failure"
    );
    stale_tx
        .rollback()
        .await
        .expect("rollback aborted stale writer transaction");

    let rollback_rows = sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from hepta_paper_raid_idempotency
         where operation=$1 and idempotency_key=$2",
    )
    .bind(rollback_operation)
    .bind(rollback_key)
    .fetch_one(&pool)
    .await
    .expect("inspect rolled-back stale writer side effect");
    assert_eq!(
        rollback_rows, 0,
        "40001 must abort every side effect written earlier by the stale transaction"
    );
    let evaluation_json_after = sqlx::query_scalar::<_, Value>(
        "select record_json from hepta_paper_evaluations where evaluation_id=$1",
    )
    .bind(legacy.evaluation_id)
    .fetch_one(&pool)
    .await
    .expect("read source after stale writer rollback");
    assert_eq!(evaluation_json_after, evaluation_json_before);
    preparation
}

async fn assert_paper_finality_v2_cross_paper_updates_are_deadlock_free(
    pool: &sqlx::PgPool,
    sealed_paper_id: Uuid,
) {
    let sealed = sqlx::query(
        "select paper.team_id, paper.challenge_id, auth_set.authorization_set_id
         from hepta_paper_projects as paper
         join hepta_research_session_authorization_sets as auth_set
           on auth_set.paper_project_id=paper.paper_project_id
         where paper.paper_project_id=$1
         order by auth_set.roster_version desc
         limit 1",
    )
    .bind(sealed_paper_id)
    .fetch_one(pool)
    .await
    .expect("load sealed Paper authorization set");
    let sealed_team_id = sealed.get::<Uuid, _>("team_id");
    let challenge_id = sealed.get::<Uuid, _>("challenge_id");
    let sealed_authorization_set_id = sealed.get::<Uuid, _>("authorization_set_id");
    let unsealed_team_id = Uuid::new_v4();
    let unsealed_paper_id = Uuid::new_v4();
    let unsealed_authorization_set_id = Uuid::new_v4();
    sqlx::query(
        "insert into hepta_research_teams (
            team_id,challenge_id,collaboration_compact_hash,status,
            roster_version,version,record_json,created_at,updated_at
         ) values ($1,$2,$3,'archived',1,1,$4::jsonb,now(),now())",
    )
    .bind(unsealed_team_id)
    .bind(challenge_id)
    .bind(digest("paper-finality-v2-cross-paper-team"))
    .bind(json!({"team_id":unsealed_team_id,"status":"archived"}))
    .execute(pool)
    .await
    .expect("seed unsealed cross-Paper team");
    let unsealed_terminal_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("select clock_timestamp()")
            .fetch_one(pool)
            .await
            .expect("read cross-Paper terminal timestamp");
    sqlx::query(
        "insert into hepta_paper_projects (
            paper_project_id,team_id,challenge_id,phase,outcome,outcome_reason,
            terminal_at,version,record_json,created_at,updated_at
         ) values (
            $1,$2,$3,'submission_ready','submission_ready',null,
            $4,1,$5::jsonb,$4,$4
         )",
    )
    .bind(unsealed_paper_id)
    .bind(unsealed_team_id)
    .bind(challenge_id)
    .bind(unsealed_terminal_at)
    .bind(json!({
        "paper_project_id":unsealed_paper_id,
        "team_id":unsealed_team_id,
        "phase":"submission_ready",
        "outcome":"submission_ready",
        "outcome_reason":null,
        "terminal_at":unsealed_terminal_at,
    }))
    .execute(pool)
    .await
    .expect("seed unsealed cross-Paper anchor");
    sqlx::query(
        "insert into hepta_research_session_authorization_sets (
            authorization_set_id,session_id,team_id,paper_project_id,challenge_id,
            team_roster_version,roster_version,roster_root,
            supersedes_roster_version,replaced_participant_slot,status,version,
            record_json,issued_at,expires_at,consumed_at
         ) values (
            $1,$2,$3,$4,$5,1,1,$6,null,null,'completed',1,
            $7::jsonb,now(),now()+interval '1 day',now()
         )",
    )
    .bind(unsealed_authorization_set_id)
    .bind(format!("paper-finality-v2-cross-paper-{unsealed_paper_id}"))
    .bind(unsealed_team_id)
    .bind(unsealed_paper_id)
    .bind(challenge_id)
    .bind(digest("paper-finality-v2-cross-paper-roster"))
    .bind(json!({
        "authorization_set_id":unsealed_authorization_set_id,
        "paper_project_id":unsealed_paper_id,
        "status":"completed",
    }))
    .execute(pool)
    .await
    .expect("seed unsealed cross-Paper source row");

    let opposite_updates = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            sqlx::query(
                "update hepta_research_session_authorization_sets
                 set paper_project_id=$1,team_id=$2
                 where authorization_set_id=$3",
            )
            .bind(unsealed_paper_id)
            .bind(unsealed_team_id)
            .bind(sealed_authorization_set_id)
            .execute(pool),
            sqlx::query(
                "update hepta_research_session_authorization_sets
                 set paper_project_id=$1,team_id=$2
                 where authorization_set_id=$3",
            )
            .bind(sealed_paper_id)
            .bind(sealed_team_id)
            .bind(unsealed_authorization_set_id)
            .execute(pool),
        )
    })
    .await
    .expect("opposite cross-Paper updates must not deadlock");
    for result in [opposite_updates.0, opposite_updates.1] {
        let error = result.expect_err("sealed/unsealed cross-Paper update must fail closed");
        let database_error = error
            .as_database_error()
            .expect("cross-Paper rejection must be a PostgreSQL error");
        assert_eq!(database_error.code().as_deref(), Some("55000"));
        assert_eq!(
            database_error.message(),
            "hepta_paper_chain_finality_v2_source_sealed"
        );
    }
    let source_scopes = sqlx::query(
        "select authorization_set_id,paper_project_id
         from hepta_research_session_authorization_sets
         where authorization_set_id=any($1)
         order by authorization_set_id",
    )
    .bind(vec![
        sealed_authorization_set_id,
        unsealed_authorization_set_id,
    ])
    .fetch_all(pool)
    .await
    .expect("inspect cross-Paper source scopes after rejection");
    assert_eq!(source_scopes.len(), 2);
    for row in source_scopes {
        let authorization_set_id = row.get::<Uuid, _>("authorization_set_id");
        let expected_paper_id = if authorization_set_id == sealed_authorization_set_id {
            sealed_paper_id
        } else {
            assert_eq!(authorization_set_id, unsealed_authorization_set_id);
            unsealed_paper_id
        };
        assert_eq!(row.get::<Uuid, _>("paper_project_id"), expected_paper_id);
    }
}

#[tokio::test]
async fn postgres_paper_chain_finality_v2_preparation_matches_memory_and_is_atomic() {
    let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
        eprintln!(
            "HEPTA_TEST_DATABASE_URL unset; Paper finality V2 PostgreSQL conformance skipped"
        );
        return;
    };
    let mut lock = PgConnection::connect(&database_url)
        .await
        .expect("PostgreSQL Paper finality V2 test lock");
    sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("serialize Hepta PostgreSQL tests");
    let state = AppState::connect(&database_url, paper_chain_finality_v2_security())
        .await
        .expect("Paper finality V2 PostgreSQL state");
    let pool = state.pool.as_ref().expect("PostgreSQL pool").clone();
    sqlx::raw_sql(include_str!(
        "../../../migrations/0038_add_hepta_paper_chain_finality_v2.sql"
    ))
    .execute(&pool)
    .await
    .expect("0038 second application");
    reset_postgres(&database_url).await;
    assert_paper_finality_v2_trigger_replay_repairs_tampering(&pool).await;
    assert_paper_finality_v2_constraint_replay_rejects_same_name_tampering(&pool).await;

    for scenario in [
        ResolvedAppealScenarioV2::Denied,
        ResolvedAppealScenarioV2::Upheld,
    ] {
        reset_postgres(&database_url).await;
        exercise_resolved_appeal_paper_chain_finality_v2(state.clone(), scenario).await;
    }
    for scenario in [
        AppealLineageScenarioV2::AncestorOpen,
        AppealLineageScenarioV2::TerminalDeniedAfterUpheld,
        AppealLineageScenarioV2::MultiGenerationUpheld,
    ] {
        reset_postgres(&database_url).await;
        exercise_paper_chain_finality_v2_appeal_lineage(state.clone(), scenario).await;
    }
    reset_postgres(&database_url).await;
    exercise_paper_chain_finality_v2_arm_source_drift(state.clone()).await;

    reset_postgres(&database_url).await;
    let stale_writer_preparation =
        prepare_paper_finality_v2_against_stale_repeatable_read_writer(state.clone()).await;
    assert_paper_finality_v2_cross_paper_updates_are_deadlock_free(
        &pool,
        stale_writer_preparation.binding.paper_project_id,
    )
    .await;

    reset_postgres(&database_url).await;
    let postgres = exercise_paper_chain_finality_v2_preparation(state).await;
    let row = sqlx::query(
        "select count(*)::bigint as rows,
                min(status) as status,
                min(commitment_id) as commitment_id
         from hepta_paper_chain_finality_preparations_v2",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect V2 preparation rows");
    assert_eq!(row.get::<i64, _>("rows"), 1);
    assert_eq!(
        row.get::<String, _>("status"),
        "awaiting_chain_verifier_upgrade"
    );
    assert_eq!(
        row.get::<String, _>("commitment_id"),
        postgres.binding.commitment_id
    );
    let protocol_counts = sqlx::query(
        "select
            (select count(*) from hepta_trnm_cometbft_time_checkpoints_v1)::bigint
                as checkpoints,
            (select count(*) from hepta_paper_chain_finality_window_arms_v2)::bigint
                as arms",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect V2 checkpoint and arm rows");
    assert_eq!(protocol_counts.get::<i64, _>("checkpoints"), 4);
    assert_eq!(protocol_counts.get::<i64, _>("arms"), 1);

    let evaluation_json_before = sqlx::query_scalar::<_, Value>(
        "select record_json from hepta_paper_evaluations where evaluation_id=$1",
    )
    .bind(postgres.binding.evaluation_id)
    .fetch_one(&pool)
    .await
    .expect("read sealed evaluation before rollback probe");
    let rollback_operation = "paper-finality-v2-old-binary-rollback-probe";
    let rollback_key = "paper-finality-v2-old-binary-rollback-probe";
    let mut rollback_tx = pool.begin().await.expect("begin rollback probe");
    sqlx::query(
        "insert into hepta_paper_raid_idempotency (
            operation,idempotency_key,request_hash,aggregate_id,response_status,response_json
         ) values ($1,$2,$3,$4,201,$5::jsonb)",
    )
    .bind(rollback_operation)
    .bind(rollback_key)
    .bind(sha256_digest(b"must-roll-back"))
    .bind(postgres.binding.paper_project_id)
    .bind(json!({"result":"must_roll_back"}))
    .execute(&mut *rollback_tx)
    .await
    .expect("write old-binary side effect before sealed source mutation");
    let sealed_error = sqlx::query(
        "update hepta_paper_evaluations
         set record_json=record_json
         where evaluation_id=$1",
    )
    .bind(postgres.binding.evaluation_id)
    .execute(&mut *rollback_tx)
    .await
    .expect_err("database trigger must reject an old-binary source write");
    let sealed_database_error = sealed_error
        .as_database_error()
        .expect("sealed source write must be a PostgreSQL error");
    assert_eq!(sealed_database_error.code().as_deref(), Some("55000"));
    assert_eq!(
        sealed_database_error.message(),
        "hepta_paper_chain_finality_v2_source_sealed"
    );
    rollback_tx
        .rollback()
        .await
        .expect("rollback aborted old-binary transaction");
    let rollback_rows = sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from hepta_paper_raid_idempotency
         where operation=$1 and idempotency_key=$2",
    )
    .bind(rollback_operation)
    .bind(rollback_key)
    .fetch_one(&pool)
    .await
    .expect("inspect rolled-back old-binary side effect");
    assert_eq!(
        rollback_rows, 0,
        "a rejected old-binary source write must roll back every earlier transaction side effect"
    );
    let evaluation_json_after = sqlx::query_scalar::<_, Value>(
        "select record_json from hepta_paper_evaluations where evaluation_id=$1",
    )
    .bind(postgres.binding.evaluation_id)
    .fetch_one(&pool)
    .await
    .expect("read sealed evaluation after rollback probe");
    assert_eq!(evaluation_json_after, evaluation_json_before);

    let immutable_error = sqlx::query(
        "update hepta_paper_chain_finality_preparations_v2
         set record_json=record_json
         where preparation_id=$1",
    )
    .bind(postgres.preparation_id)
    .execute(&pool)
    .await
    .expect_err("database trigger must make the preparation immutable");
    let immutable_database_error = immutable_error
        .as_database_error()
        .expect("immutable preparation write must be a PostgreSQL error");
    assert_eq!(immutable_database_error.code().as_deref(), Some("55000"));
    assert_eq!(
        immutable_database_error.message(),
        "hepta_paper_chain_finality_v2_preparation_immutable"
    );

    let truncate_error = sqlx::query("truncate hepta_paper_chain_finality_window_arms_v2 cascade")
        .execute(&pool)
        .await
        .expect_err("production V2 TRUNCATE guard must remain enabled after test reset");
    let truncate_database_error = truncate_error
        .as_database_error()
        .expect("V2 TRUNCATE rejection must be a PostgreSQL error");
    assert_eq!(truncate_database_error.code().as_deref(), Some("55000"));
    assert_eq!(
        truncate_database_error.message(),
        "hepta_paper_chain_finality_v2_truncate_forbidden"
    );

    let memory = exercise_paper_chain_finality_v2_preparation(AppState::new(
        paper_chain_finality_v2_security(),
    ))
    .await;
    assert_eq!(postgres.binding.appeal_status, memory.binding.appeal_status);
    assert_eq!(
        postgres.binding.settlement_policy_hash,
        memory.binding.settlement_policy_hash
    );
    assert_eq!(
        (
            postgres.binding.scientific_finality,
            postgres.binding.score_eligible,
            postgres.binding.ranking_eligible,
            postgres.binding.reward_eligible,
            postgres.binding.economic_eligible,
        ),
        (true, false, false, false, false)
    );
    reset_postgres(&database_url).await;
    sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
        .execute(&mut lock)
        .await
        .expect("release Hepta PostgreSQL Paper finality V2 test lock");
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
    let pg_pool = pg_state.pool.as_ref().expect("Paper Raid PostgreSQL pool");
    for application in ["second", "third"] {
        sqlx::raw_sql(include_str!(
            "../../../migrations/0043_add_hepta_challenge_ruleset_v1.sql"
        ))
        .execute(pg_pool)
        .await
        .unwrap_or_else(|error| panic!("0043 {application} application: {error}"));
        sqlx::raw_sql(include_str!(
            "../../../migrations/0044_add_hepta_agent_proposal_v2_epoch.sql"
        ))
        .execute(pg_pool)
        .await
        .unwrap_or_else(|error| panic!("0044 {application} application: {error}"));
        sqlx::raw_sql(include_str!(
            "../../../migrations/0045_add_hepta_work_item_record_parity.sql"
        ))
        .execute(pg_pool)
        .await
        .unwrap_or_else(|error| panic!("0045 {application} application: {error}"));
    }
    reset_postgres(&database_url).await;
    let postgres = run_full_flow(pg_state.clone(), 3, FlowOptions::default()).await;
    assert_eq!(postgres, memory);
    let challenge_projection = sqlx::query(
        "select challenge_ruleset_snapshot_hash,deadline_at,grace_expires_at,
                outcome,outcome_reason,terminal_at,record_json
         from hepta_paper_projects",
    )
    .fetch_one(pg_pool)
    .await
    .expect("Paper ChallengeRuleset projection");
    let challenge_record: Value = challenge_projection.get("record_json");
    assert!(challenge_projection
        .get::<Option<String>, _>("challenge_ruleset_snapshot_hash")
        .is_some());
    assert!(challenge_projection
        .get::<Option<chrono::DateTime<Utc>>, _>("deadline_at")
        .is_none());
    assert!(challenge_projection
        .get::<Option<chrono::DateTime<Utc>>, _>("grace_expires_at")
        .is_none());
    assert_eq!(
        challenge_projection.get::<String, _>("outcome"),
        "submission_ready"
    );
    assert!(challenge_projection
        .get::<Option<String>, _>("outcome_reason")
        .is_none());
    assert!(challenge_projection
        .get::<Option<chrono::DateTime<Utc>>, _>("terminal_at")
        .is_some());
    assert_eq!(
        challenge_record["challenge_ruleset_snapshot"]["enforcement"],
        "legacy_unranked"
    );
    assert_eq!(challenge_record["outcome"], "submission_ready");
    assert!(sqlx::query(
        "update hepta_paper_projects
         set terminal_at=terminal_at + interval '1 second'"
    )
    .execute(pg_pool)
    .await
    .is_err());
    assert!(sqlx::query(
        "update hepta_paper_projects
         set challenge_ruleset_snapshot_hash=$1"
    )
    .bind(digest("tampered-ruleset-snapshot"))
    .execute(pg_pool)
    .await
    .is_err());

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
