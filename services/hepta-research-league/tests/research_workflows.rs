use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use hepta_research_league::{
    app, submission_signing_message,
    trnm_v1::{
        object_leaf_hash, quorum_signing_bytes, receipt_hash, AuthorityRole, ChallengeReason,
        ChallengeResearchClaimV1, ClaimResolutionDecision, ClaimResolutionV1, ClaimShareV1,
        ContributionRole, ContributorWorkV1, CreateResearchClaimV1, DeclareLicenseV1,
        EvaluationCommitmentV1, ExternalKey, FinalityReceiptV1, IssueWorkloadReceiptV1,
        LicenseScope, ObjectInclusionProofV1, ObjectRefV1, QuorumCertificateV1, ResearchCommandV1,
        ResearchObjectKind, SignedResearchCommandV1, TrustedValidatorSetV1, TrustedValidatorV1,
        ValidatorSignatureV1, FINALITY_RECEIPT_V1, OBJECT_INCLUSION_PROOF_V1,
        QUORUM_CERTIFICATE_V1,
    },
    AppState, SecurityConfig, SubmitArtifactRequest,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use uuid::Uuid;

const HASH_A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const HASH_C: &str = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn protocol_key(byte: u8) -> ExternalKey {
    ExternalKey::from_bytes([byte; 32])
}

fn object_ref(kind: ResearchObjectKind, byte: u8) -> ObjectRefV1 {
    ObjectRefV1::new(kind, protocol_key(byte), 1)
}

fn signed_hepta_command(
    command_id: ExternalKey,
    nonce: u64,
    command: ResearchCommandV1,
) -> SignedResearchCommandV1 {
    SignedResearchCommandV1::sign(
        "trnm-devnet-v1".to_string(),
        command_id,
        "did:trnm:hepta-authority".to_string(),
        AuthorityRole::HeptaAuthority,
        nonce,
        command,
        &SigningKey::from_bytes(&[0x44; 32]),
    )
    .expect("valid signed Hepta research command")
}

async fn request(app: axum::Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-hepta-operator-token", "hepta-test-operator-token")
                .header("x-hepta-nakama-token", "hepta-test-nakama-token")
                .header("x-hepta-trnm-token", "hepta-test-trnm-token")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"text": String::from_utf8_lossy(&bytes)}));
    (status, value)
}

fn decode_hash(value: &str) -> [u8; 32] {
    let hex = value.strip_prefix("sha256:").unwrap();
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
    }
    output
}

fn match_event_root(event_hashes: &[&str]) -> String {
    let mut level = event_hashes
        .iter()
        .enumerate()
        .map(|(index, event_hash)| {
            let mut hasher = Sha256::new();
            hasher.update(b"trnm_match_event_leaf_v1\0");
            hasher.update(((index + 1) as u64).to_be_bytes());
            hasher.update(decode_hash(event_hash));
            <[u8; 32]>::from(hasher.finalize())
        })
        .collect::<Vec<_>>();
    while level.len() > 1 {
        level = level
            .chunks(2)
            .map(|pair| {
                let left = pair[0];
                let right = pair.get(1).copied().unwrap_or(left);
                let mut hasher = Sha256::new();
                hasher.update(b"trnm_binary_merkle_node_v1\0");
                hasher.update(left);
                hasher.update(right);
                <[u8; 32]>::from(hasher.finalize())
            })
            .collect();
    }
    format!(
        "sha256:{}",
        level[0]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn finality_receipt(
    command_id: Uuid,
    command_fingerprint: &str,
    source_event_id: Uuid,
    validator_keys: &[SigningKey],
    confirmations: u64,
) -> FinalityReceiptV1 {
    let mut receipt = FinalityReceiptV1 {
        protocol: FINALITY_RECEIPT_V1.to_string(),
        source_event_id,
        command_id,
        command_fingerprint: command_fingerprint.to_string(),
        chain_id: "trnm-devnet-v1".to_string(),
        tx_hash: HASH_B.to_string(),
        tx_index: 0,
        block_height: 42,
        block_hash: HASH_A.to_string(),
        state_root: HASH_C.to_string(),
        object_ref: ObjectRefV1 {
            kind: ResearchObjectKind::WorkloadReceipt,
            key: ExternalKey::from_bytes([0xcc; 32]),
            object_version: 1,
        },
        inclusion_proof: ObjectInclusionProofV1 {
            protocol: OBJECT_INCLUSION_PROOF_V1.to_string(),
            leaf_index: 0,
            sibling_hashes: Vec::new(),
        },
        validator_set_id: "validator-set-42".to_string(),
        quorum_certificate: QuorumCertificateV1 {
            protocol: QUORUM_CERTIFICATE_V1.to_string(),
            chain_id: "trnm-devnet-v1".to_string(),
            validator_set_id: "validator-set-42".to_string(),
            block_height: 42,
            block_hash: HASH_A.to_string(),
            state_root: HASH_C.to_string(),
            signed_voting_power: 3,
            total_voting_power: 3,
            signatures: Vec::new(),
        },
        confirmations,
        receipt_hash: String::new(),
    };
    receipt.state_root = object_leaf_hash(&receipt).unwrap();
    receipt.quorum_certificate.state_root = receipt.state_root.clone();
    let signing_bytes = quorum_signing_bytes(&receipt.quorum_certificate).unwrap();
    receipt.quorum_certificate.signatures = validator_keys
        .iter()
        .enumerate()
        .map(|(index, key)| ValidatorSignatureV1 {
            validator_id: format!("validator-{}", index + 1),
            key_id: format!("validator-key-{}", index + 1),
            signature_base64: BASE64.encode(key.sign(&signing_bytes).to_bytes()),
        })
        .collect();
    receipt.receipt_hash = receipt_hash(&receipt).unwrap();
    receipt
}

#[tokio::test]
async fn evaluation_nakama_and_trnm_flow_is_deterministic_and_finality_gated() {
    let validator_keys = vec![
        SigningKey::from_bytes(&[81; 32]),
        SigningKey::from_bytes(&[82; 32]),
        SigningKey::from_bytes(&[83; 32]),
    ];
    let trusted_set = TrustedValidatorSetV1 {
        chain_id: "trnm-devnet-v1".to_string(),
        validator_set_id: "validator-set-42".to_string(),
        validators: validator_keys
            .iter()
            .enumerate()
            .map(|(index, key)| TrustedValidatorV1 {
                validator_id: format!("validator-{}", index + 1),
                key_id: format!("validator-key-{}", index + 1),
                public_key_base64: BASE64.encode(key.verifying_key().to_bytes()),
                voting_power: 1,
            })
            .collect(),
    };
    let security = SecurityConfig::new("hepta-test-operator-token", "hepta-test-nakama-token")
        .with_trnm_token("hepta-test-trnm-token")
        .with_trusted_trnm_validator_set(trusted_set)
        .unwrap();
    let state = AppState::new(security);
    let router = app(state);
    let signing_key = SigningKey::from_bytes(&[44; 32]);
    let agent_id = "did:trnm:workflow-agent";

    assert_eq!(
        request(
            router.clone(),
            "POST",
            "/v1/hepta/agents",
            json!({
                "agent_id":agent_id,"owner_id":"workflow-owner",
                "protocol_version":"hepta_agent_protocol_v1",
                "public_key":BASE64.encode(signing_key.verifying_key().to_bytes())
            })
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (status, challenge) = request(
        router.clone(),
        "POST",
        "/v1/hepta/challenges",
        json!({
            "title":"Deterministic Research","description":"workflow",
            "ruleset_version":"research-v1","ruleset_hash":HASH_A,
            "dataset_manifest_hash":HASH_B,"evaluator_manifest_hash":HASH_C,
            "status":"open"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let challenge_id = challenge["challenge_id"].as_str().unwrap();
    assert_eq!(
        request(
            router.clone(),
            "POST",
            &format!("/v1/hepta/challenges/{challenge_id}/enrollments"),
            json!({"agent_id":agent_id})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (_, authorization) = request(
        router.clone(),
        "POST",
        "/v1/hepta/match-authorizations",
        json!({
            "challenge_id":challenge_id,"agent_id":agent_id,
            "subject_user_id":"nakama-workflow-user",
            "participant_slot":1,"role":"challenger","ttl_seconds":300
        }),
    )
    .await;
    let match_id = authorization["claim"]["match_id"].as_str().unwrap();
    assert_eq!(
        request(
            router.clone(),
            "POST",
            "/v1/hepta/nakama/match-authorizations/consumed",
            json!({
                "authorization_id":authorization["claim"]["authorization_id"],
                "match_id":match_id,"agent_id":agent_id,
            })
        )
        .await
        .0,
        StatusCode::OK
    );
    let submission_id = Uuid::new_v4();
    let unsigned = SubmitArtifactRequest {
        protocol_version: "hepta_agent_protocol_v1".into(),
        submission_id,
        challenge_id: Uuid::parse_str(challenge_id).unwrap(),
        match_id: Uuid::parse_str(match_id).unwrap(),
        agent_id: agent_id.into(),
        artifact_hash: HASH_A.into(),
        evidence_manifest_hash: HASH_B.into(),
        nonce: "workflow-submit-1".into(),
        signature: String::new(),
    };
    let signature = BASE64.encode(
        signing_key
            .sign(submission_signing_message(&unsigned).as_bytes())
            .to_bytes(),
    );
    assert_eq!(
        request(
            router.clone(),
            "POST",
            "/v1/hepta/submissions",
            json!({
                "protocol_version":unsigned.protocol_version,
                "submission_id":unsigned.submission_id,
                "challenge_id":unsigned.challenge_id,
                "match_id":unsigned.match_id,
                "agent_id":unsigned.agent_id,
                "artifact_hash":unsigned.artifact_hash,
                "evidence_manifest_hash":unsigned.evidence_manifest_hash,
                "nonce":unsigned.nonce,"signature":signature
            })
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );

    let (_, manifest) = request(
        router.clone(),
        "POST",
        "/v1/hepta/evaluator-manifests",
        json!({
            "challenge_id":challenge_id,"version":"eval-v1",
            "criteria":[
                {"metric":"quality","weight_bps":7500,"minimum_micros":500000},
                {"metric":"efficiency","weight_bps":2500,"minimum_micros":null}
            ]
        }),
    )
    .await;
    let manifest_id = manifest["evaluator_manifest_id"].as_str().unwrap();
    let evaluation_request = json!({
        "submission_id":submission_id,"evaluator_manifest_id":manifest_id,
        "observed_metrics_micros":{"efficiency":800000,"quality":600000}
    });
    let (status, evaluation) = request(
        router.clone(),
        "POST",
        "/v1/hepta/evaluations",
        evaluation_request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(evaluation["score_micros"], 650000);
    let (status, retry) = request(
        router.clone(),
        "POST",
        "/v1/hepta/evaluations",
        evaluation_request,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(retry["report_hash"], evaluation["report_hash"]);

    let evaluation_id = evaluation["evaluation_report_id"].as_str().unwrap();
    let (status, reproduction) = request(
        router.clone(),
        "POST",
        "/v1/hepta/reproductions",
        json!({
            "evaluation_report_id":evaluation_id,"reproducer_id":"lab-reproducer",
            "environment_hash":HASH_C,
            "observed_metrics_micros":{"quality":600000,"efficiency":800000}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(reproduction["reproduced"], true);
    let (_, appeal) = request(
        router.clone(),
        "POST",
        "/v1/hepta/appeals",
        json!({
            "evaluation_report_id":evaluation_id,"appellant_id":"workflow-owner",
            "grounds_hash":HASH_A
        }),
    )
    .await;
    assert_eq!(
        request(
            router.clone(),
            "POST",
            &format!(
                "/v1/hepta/appeals/{}/resolve",
                appeal["appeal_id"].as_str().unwrap()
            ),
            json!({"outcome":"denied","resolution_hash":HASH_B,"adjusted_score_micros":null})
        )
        .await
        .0,
        StatusCode::OK
    );

    let event_hashes = [HASH_A, HASH_B, HASH_C];
    for (index, event_type) in [
        "nakama.match.started.v1",
        "nakama.round.closed.v1",
        "nakama.match.completed.v1",
    ]
    .iter()
    .enumerate()
    {
        let event_root = (index == 2).then(|| match_event_root(&event_hashes));
        if index == 2 {
            let (status, error) = request(
                router.clone(),
                "POST",
                "/v1/hepta/nakama/events",
                json!({
                    "protocol":"hepta_nakama_match_event_v2","event_id":Uuid::new_v4(),
                    "event_type":event_type,"match_id":match_id,"challenge_id":challenge_id,
                    "sequence":index + 1,"event_hash":event_hashes[index],
                    "event_root":HASH_A,"archive_uri":"s3://archive/tampered",
                    "occurred_at":"2026-07-25T10:00:00Z"
                }),
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(error["code"], "nakama_event_root_mismatch");
        }
        let (status, _) = request(
            router.clone(),
            "POST",
            "/v1/hepta/nakama/events",
            json!({
                "protocol":"hepta_nakama_match_event_v2","event_id":Uuid::new_v4(),
                "event_type":event_type,"match_id":match_id,"challenge_id":challenge_id,
                "sequence":index + 1,"event_hash":event_hashes[index],
                "event_root":event_root,"archive_uri":if index == 2 {Some("s3://archive/match")} else {None},
                "occurred_at":"2026-07-25T10:00:00Z"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
    }
    let (status, reconciliation) = request(
        router.clone(),
        "GET",
        &format!("/v1/hepta/nakama/matches/{match_id}/reconciliation"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reconciliation["reconciled"], true);

    let evaluation_payload = EvaluationCommitmentV1 {
        evaluation_id: protocol_key(0x21),
        match_evidence_ref: object_ref(ResearchObjectKind::MatchEvidence, 0x20),
        submission_hash: [0x22; 32],
        rubric_hash: [0x23; 32],
        evaluation_hash: [0x24; 32],
        reproduction_hash: Some([0x25; 32]),
        score_bps: 9_000,
        accepted: true,
        completed_at_unix_s: 1_753_449_700,
    };
    let workload_payload = IssueWorkloadReceiptV1 {
        receipt_id: protocol_key(0x31),
        evaluation_ref: object_ref(ResearchObjectKind::EvaluationCommitment, 0x21),
        contributors: vec![ContributorWorkV1 {
            contributor: protocol_key(0x32),
            role: ContributionRole::Researcher,
            accepted_work_units: 1_000_000,
            contribution_hash: [0x33; 32],
        }],
        total_accepted_work_units: 1_000_000,
        policy_hash: [0x34; 32],
        issued_at_unix_s: 1_753_449_800,
    };
    let claim_payload = CreateResearchClaimV1 {
        claim_id: protocol_key(0x41),
        workload_receipt_ref: object_ref(ResearchObjectKind::WorkloadReceipt, 0x31),
        evidence_refs: vec![object_ref(ResearchObjectKind::EvaluationCommitment, 0x21)],
        artifact_hash: [0x42; 32],
        claim_scope_hash: [0x43; 32],
        claimants: vec![ClaimShareV1 {
            contributor: protocol_key(0x32),
            share_bps: 10_000,
        }],
        created_at_unix_s: 1_753_449_900,
    };
    let license_payload = DeclareLicenseV1 {
        declaration_id: protocol_key(0x51),
        claim_ref: object_ref(ResearchObjectKind::ResearchClaim, 0x41),
        licensor: protocol_key(0x32),
        scope: LicenseScope::AllClaimedMaterial,
        spdx_expression: "Apache-2.0".to_string(),
        additional_terms_hash: None,
        effective_at_unix_s: 1_753_450_000,
    };
    let challenge_payload = ChallengeResearchClaimV1 {
        challenge_id: protocol_key(0x61),
        claim_ref: object_ref(ResearchObjectKind::ResearchClaim, 0x41),
        challenger: protocol_key(0x62),
        reason: ChallengeReason::EvidenceIntegrity,
        evidence_hash: [0x63; 32],
        opened_at_unix_s: 1_753_450_100,
    };
    let resolution_payload = ClaimResolutionV1 {
        resolution_id: protocol_key(0x71),
        challenge_ref: object_ref(ResearchObjectKind::ClaimChallenge, 0x61),
        decision: ClaimResolutionDecision::Reject,
        resolution_hash: [0x72; 32],
        amended_claimants: Vec::new(),
        decided_at_unix_s: 1_753_450_200,
    };
    let typed_commands = [
        (
            "/v1/hepta/trnm/commitments",
            signed_hepta_command(
                protocol_key(0x81),
                1,
                ResearchCommandV1::EvaluationCommitment(evaluation_payload),
            ),
        ),
        (
            "/v1/hepta/trnm/research-claims",
            signed_hepta_command(
                protocol_key(0x82),
                2,
                ResearchCommandV1::CreateResearchClaim(claim_payload),
            ),
        ),
        (
            "/v1/hepta/trnm/license-declarations",
            signed_hepta_command(
                protocol_key(0x83),
                3,
                ResearchCommandV1::DeclareLicense(license_payload),
            ),
        ),
        (
            "/v1/hepta/trnm/claim-challenges",
            signed_hepta_command(
                protocol_key(0x84),
                4,
                ResearchCommandV1::ChallengeResearchClaim(challenge_payload),
            ),
        ),
        (
            "/v1/hepta/trnm/challenge-resolutions",
            signed_hepta_command(
                protocol_key(0x85),
                5,
                ResearchCommandV1::ResolveResearchClaim(resolution_payload),
            ),
        ),
    ];
    for (path, signed_command) in typed_commands {
        let (status, command) = request(
            router.clone(),
            "POST",
            path,
            json!({"signed_command":signed_command}),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED, "{command}");
        assert_eq!(command["status"], "pending_finality");
        assert!(command["command_fingerprint"]
            .as_str()
            .is_some_and(|value| value.starts_with("sha256:")));
        assert!(command.get("payload").is_none());
    }

    let workload_command_id = protocol_key(0x86);
    let workload_request = json!({
        "signed_command":signed_hepta_command(
            workload_command_id,
            6,
            ResearchCommandV1::IssueWorkloadReceipt(workload_payload.clone()),
        )
    });
    let (status, command) = request(
        router.clone(),
        "POST",
        "/v1/hepta/trnm/workload-receipts",
        workload_request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(command["status"], "pending_finality");
    let command_id = command["command_id"].as_str().unwrap();
    let command_fingerprint = command["command_fingerprint"].as_str().unwrap();
    let (status, retry) = request(
        router.clone(),
        "POST",
        "/v1/hepta/trnm/workload-receipts",
        workload_request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(retry["command_id"], command["command_id"]);
    let mut altered_payload = workload_payload;
    altered_payload.policy_hash = [0x35; 32];
    let altered_workload = json!({
        "signed_command":signed_hepta_command(
            workload_command_id,
            6,
            ResearchCommandV1::IssueWorkloadReceipt(altered_payload),
        )
    });
    let (status, error) = request(
        router.clone(),
        "POST",
        "/v1/hepta/trnm/workload-receipts",
        altered_workload,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "trnm_idempotency_conflict");
    assert_eq!(
        request(
            router.clone(),
            "GET",
            &format!("/v1/hepta/trnm/finality/{command_id}"),
            json!({})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let receipt_event_id = Uuid::new_v4();
    let command_id = Uuid::parse_str(command_id).unwrap();
    let receipt = finality_receipt(
        command_id,
        command_fingerprint,
        receipt_event_id,
        &validator_keys,
        12,
    );
    let mut tampered_receipt = receipt.clone();
    tampered_receipt.quorum_certificate.signatures[0].signature_base64 = BASE64.encode([0_u8; 64]);
    tampered_receipt.receipt_hash = receipt_hash(&tampered_receipt).unwrap();
    let (status, error) = request(
        router.clone(),
        "POST",
        "/v1/hepta/trnm/finality",
        serde_json::to_value(tampered_receipt).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], "trnm_finality_verification_failed");
    let (status, verification) = request(
        router.clone(),
        "POST",
        "/v1/hepta/trnm/finality/verify",
        serde_json::to_value(&receipt).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(verification["valid"], true);
    let (status, finality) = request(
        router.clone(),
        "POST",
        "/v1/hepta/trnm/finality",
        serde_json::to_value(&receipt).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(finality["status"], "finalized");
    assert_eq!(finality["verified"], true);
    assert_eq!(finality["chain_id"], "trnm-devnet-v1");
    assert_eq!(finality["block_height"], 42);
    assert!(finality["quorum_certificate"]["signatures"]
        .as_array()
        .is_some_and(|signatures| signatures.len() == 3));
    assert_eq!(
        request(
            router.clone(),
            "POST",
            "/v1/hepta/trnm/finality",
            serde_json::to_value(&receipt).unwrap()
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut altered_receipt = receipt;
    altered_receipt.confirmations += 1;
    altered_receipt.receipt_hash = receipt_hash(&altered_receipt).unwrap();
    let (status, error) = request(
        router.clone(),
        "POST",
        "/v1/hepta/trnm/finality",
        serde_json::to_value(altered_receipt).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "inbox_event_conflict");

    let (status, task_package) = request(
        router.clone(),
        "GET",
        &format!("/v1/hepta/research-terminal/challenges/{challenge_id}/task-package"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task_package["agent_execution_mode"], "external_only");
    assert!(task_package["platform_model_credentials"].is_null());

    let (status, ready) = request(router.clone(), "GET", "/ready", json!({})).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(ready["ready"], false);
    assert_eq!(ready["database"], "not_configured");
    assert!(ready["failures"]
        .as_array()
        .is_some_and(|failures| failures.contains(&json!("database_pool_missing"))));
    assert_eq!(
        ready["top_level_modules"],
        json!(["hepta", "nakama", "trnm"])
    );
    assert_eq!(ready["agent_execution_mode"], "external_only");
    let (status, metrics) = request(router.clone(), "GET", "/metrics", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(metrics["text"]
        .as_str()
        .is_some_and(|text| text.contains("hepta_trnm_pending_finality")));
    assert!(metrics["text"]
        .as_str()
        .is_some_and(|text| text.contains("hepta_paper_raid_pending_control_commands")));
    assert!(metrics["text"]
        .as_str()
        .is_some_and(|text| text.contains("backend=\"memory\"")));
    let (status, openapi) = request(router, "GET", "/v1/hepta/openapi.yaml", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(openapi["text"]
        .as_str()
        .is_some_and(|text| text.contains("openapi: 3.1.0")));
}
