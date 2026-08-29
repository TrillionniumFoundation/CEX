use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use trnm_research_protocol::*;

const NAKAMA_SEED: [u8; 32] = [0x11; 32];
const HEPTA_SEED: [u8; 32] = [0x22; 32];

fn digest(byte: u8) -> Digest32 {
    [byte; 32]
}

fn key(namespace: &str, id: &str) -> ExternalKey {
    ExternalKey::from_external_id(namespace, id).unwrap()
}

fn fixture_commands() -> Vec<ResearchCommandV1> {
    let match_payload = MatchEvidenceCommitmentV1 {
        commitment_id: key("nakama.commitment", "commitment-001"),
        match_id: key("nakama.match", "match-001"),
        challenge_id: key("hepta.challenge", "research-challenge-001"),
        event_root: digest(0x10),
        roster_root: digest(0x11),
        ruleset_hash: digest(0x12),
        dataset_hash: digest(0x13),
        archive_hash: digest(0x14),
        event_count: 42,
        completed_at_unix_s: 1_753_449_600,
    };
    let evaluation_payload = EvaluationCommitmentV1 {
        evaluation_id: key("hepta.evaluation", "evaluation-001"),
        match_evidence_ref: match_payload.object_ref(),
        submission_hash: digest(0x20),
        rubric_hash: digest(0x21),
        evaluation_hash: digest(0x22),
        reproduction_hash: Some(digest(0x23)),
        score_bps: 9_250,
        accepted: true,
        completed_at_unix_s: 1_753_449_700,
    };

    let researcher = key("hepta.contributor", "researcher-001");
    let reproducer = key("hepta.contributor", "reproducer-001");
    let mut contributors = vec![
        ContributorWorkV1 {
            contributor: researcher,
            role: ContributionRole::Researcher,
            accepted_work_units: 700,
            contribution_hash: digest(0x30),
        },
        ContributorWorkV1 {
            contributor: reproducer,
            role: ContributionRole::Reproducer,
            accepted_work_units: 300,
            contribution_hash: digest(0x31),
        },
    ];
    contributors.sort_by_key(|entry| entry.contributor);
    let workload_payload = IssueWorkloadReceiptV1 {
        receipt_id: key("hepta.workload", "workload-001"),
        evaluation_ref: evaluation_payload.object_ref(),
        contributors,
        total_accepted_work_units: 1_000,
        policy_hash: digest(0x32),
        issued_at_unix_s: 1_753_449_800,
    };

    let mut claimants = vec![
        ClaimShareV1 {
            contributor: researcher,
            share_bps: 7_000,
        },
        ClaimShareV1 {
            contributor: reproducer,
            share_bps: 3_000,
        },
    ];
    claimants.sort_by_key(|entry| entry.contributor);
    let claim_payload = CreateResearchClaimV1 {
        claim_id: key("hepta.claim", "claim-001"),
        workload_receipt_ref: workload_payload.object_ref(),
        evidence_refs: vec![match_payload.object_ref(), evaluation_payload.object_ref()],
        artifact_hash: digest(0x40),
        claim_scope_hash: digest(0x41),
        claimants: claimants.clone(),
        created_at_unix_s: 1_753_449_900,
    };
    let license_payload = DeclareLicenseV1 {
        declaration_id: key("hepta.license", "license-001"),
        claim_ref: claim_payload.object_ref(),
        licensor: researcher,
        scope: LicenseScope::AllClaimedMaterial,
        spdx_expression: "Apache-2.0".into(),
        additional_terms_hash: Some(digest(0x50)),
        effective_at_unix_s: 1_753_450_000,
    };
    let challenge_payload = ChallengeResearchClaimV1 {
        challenge_id: key("hepta.claim-challenge", "claim-challenge-001"),
        claim_ref: claim_payload.object_ref(),
        challenger: key("hepta.contributor", "challenger-001"),
        reason: ChallengeReason::ContributionAllocation,
        evidence_hash: digest(0x60),
        opened_at_unix_s: 1_753_450_100,
    };
    let resolution_payload = ResolveResearchClaimV1 {
        resolution_id: key("hepta.claim-resolution", "claim-resolution-001"),
        challenge_ref: challenge_payload.object_ref(),
        decision: ClaimResolutionDecision::AmendContributorShares,
        resolution_hash: digest(0x70),
        amended_claimants: claimants,
        decided_at_unix_s: 1_753_450_200,
    };

    vec![
        ResearchCommandV1::MatchEvidenceCommitment(match_payload),
        ResearchCommandV1::EvaluationCommitment(evaluation_payload),
        ResearchCommandV1::IssueWorkloadReceipt(workload_payload),
        ResearchCommandV1::CreateResearchClaim(claim_payload),
        ResearchCommandV1::DeclareLicense(license_payload),
        ResearchCommandV1::ChallengeResearchClaim(challenge_payload),
        ResearchCommandV1::ResolveResearchClaim(resolution_payload),
    ]
}

fn sign(
    index: usize,
    command: ResearchCommandV1,
) -> Result<SignedResearchCommandV1, SignedResearchCommandValidationError> {
    let (role, did, key_seed) = if matches!(command, ResearchCommandV1::MatchEvidenceCommitment(_))
    {
        (
            AuthorityRole::NakamaAuthority,
            "did:trnm:nakama-authority",
            NAKAMA_SEED,
        )
    } else {
        (
            AuthorityRole::HeptaAuthority,
            "did:trnm:hepta-authority",
            HEPTA_SEED,
        )
    };
    SignedResearchCommandV1::sign(
        "trnm-devnet-v1".into(),
        key("trnm.command", &format!("command-{index:03}")),
        did.into(),
        role,
        index as u64 + 1,
        command,
        &SigningKey::from_bytes(&key_seed),
    )
}

fn test_authorities() -> AuthoritySetV1 {
    AuthoritySetV1::new(
        vec![AuthorityIdentityV1::new(
            "did:trnm:nakama-authority".into(),
            SigningKey::from_bytes(&NAKAMA_SEED)
                .verifying_key()
                .to_bytes(),
        )
        .unwrap()],
        vec![AuthorityIdentityV1::new(
            "did:trnm:hepta-authority".into(),
            SigningKey::from_bytes(&HEPTA_SEED)
                .verifying_key()
                .to_bytes(),
        )
        .unwrap()],
    )
    .unwrap()
}

fn test_state() -> ResearchProtocolState {
    ResearchProtocolState::with_authorities(test_authorities()).unwrap()
}

#[test]
fn every_typed_command_strictly_round_trips() {
    for command in fixture_commands() {
        command.validate().unwrap();
        let encoded = command.canonical_bytes();
        let decoded = ResearchCommandV1::from_canonical_bytes(&encoded).unwrap();
        assert_eq!(decoded, command);
        assert_eq!(decoded.canonical_bytes(), encoded);
    }
}

#[test]
fn every_signed_envelope_strictly_round_trips() {
    for (index, command) in fixture_commands().into_iter().enumerate() {
        let signed = sign(index, command).unwrap();
        let encoded = signed.canonical_bytes();
        let decoded = SignedResearchCommandV1::from_canonical_bytes(&encoded).unwrap();
        assert_eq!(decoded, signed);

        let mut trailing = encoded;
        trailing.push(0);
        assert!(matches!(
            SignedResearchCommandV1::from_canonical_bytes(&trailing),
            Err(SignedResearchCommandValidationError::CanonicalDecode(
                CanonicalDecodeError::TrailingBytes
            ))
        ));
    }
}

#[test]
fn strict_decoder_rejects_noncanonical_unknown_and_trailing_forms() {
    let encoded = fixture_commands()[0].canonical_bytes();

    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(matches!(
        ResearchCommandV1::from_canonical_bytes(&trailing),
        Err(ResearchCommandDecodeError::Canonical(
            CanonicalDecodeError::TrailingBytes
        ))
    ));

    let mut wrong_version = encoded.clone();
    wrong_version[1] = 2;
    assert!(matches!(
        ResearchCommandV1::from_canonical_bytes(&wrong_version),
        Err(ResearchCommandDecodeError::Canonical(
            CanonicalDecodeError::UnsupportedVersion(2)
        ))
    ));

    let mut unknown_tag = encoded.clone();
    unknown_tag[2] = 23;
    assert!(matches!(
        ResearchCommandV1::from_canonical_bytes(&unknown_tag),
        Err(ResearchCommandDecodeError::Canonical(
            CanonicalDecodeError::UnknownDiscriminant {
                name: "ResearchCommandV1",
                value: 23
            }
        ))
    ));

    let mut nonminimal_version = vec![encoded[0], 0x18, 0x01];
    nonminimal_version.extend_from_slice(&encoded[2..]);
    assert!(matches!(
        ResearchCommandV1::from_canonical_bytes(&nonminimal_version),
        Err(ResearchCommandDecodeError::Canonical(
            CanonicalDecodeError::NonMinimalEncoding
        ))
    ));

    let mut wrong_array_len = encoded;
    wrong_array_len[0] = 0x84;
    assert!(matches!(
        ResearchCommandV1::from_canonical_bytes(&wrong_array_len),
        Err(ResearchCommandDecodeError::Canonical(
            CanonicalDecodeError::ArrayLengthMismatch {
                expected: 3,
                got: 4
            }
        ))
    ));
}

#[test]
fn nakama_is_limited_to_match_fact_commitments() {
    let evaluation = fixture_commands()[1].clone();
    let result = SignedResearchCommandV1::sign(
        "trnm-devnet-v1".into(),
        key("trnm.command", "unauthorized-nakama"),
        "did:trnm:nakama-authority".into(),
        AuthorityRole::NakamaAuthority,
        1,
        evaluation,
        &SigningKey::from_bytes(&NAKAMA_SEED),
    );
    assert!(matches!(
        result,
        Err(SignedResearchCommandValidationError::UnauthorizedCommand {
            role: AuthorityRole::NakamaAuthority,
            command_type: "evaluation_commitment_v1"
        })
    ));
}

#[test]
fn protocol_state_rejects_self_asserted_unregistered_authorities() {
    let command = fixture_commands()[0].clone();
    let attacker_seed = [0x77; 32];
    let forged = SignedResearchCommandV1::sign(
        "trnm-devnet-v1".into(),
        key("trnm.command", "forged-authority"),
        "did:trnm:nakama-authority".into(),
        AuthorityRole::NakamaAuthority,
        1,
        command,
        &SigningKey::from_bytes(&attacker_seed),
    )
    .unwrap();
    assert!(matches!(
        test_state().apply(&forged),
        Err(ProtocolStateError::UnauthorizedAuthority {
            role: AuthorityRole::NakamaAuthority,
            ..
        })
    ));
}

#[test]
fn signed_command_detects_payload_and_identity_tampering() {
    let mut signed = sign(0, fixture_commands()[0].clone()).unwrap();
    signed.validate().unwrap();
    signed.signer_did.push_str("-tampered");
    assert_eq!(
        signed.validate().unwrap_err(),
        SignedResearchCommandValidationError::InvalidSignature
    );
}

#[test]
fn workload_requires_sorted_unique_contributors_and_exact_accepted_units() {
    let ResearchCommandV1::IssueWorkloadReceipt(mut workload) = fixture_commands()[2].clone()
    else {
        unreachable!()
    };
    workload.total_accepted_work_units += 1;
    assert!(matches!(
        workload.validate(),
        Err(ResearchPayloadValidationError::WorkUnitTotalMismatch { .. })
    ));

    workload.total_accepted_work_units -= 1;
    workload.contributors.reverse();
    assert_eq!(
        workload.validate().unwrap_err(),
        ResearchPayloadValidationError::NonCanonicalOrdering("contributors")
    );
}

#[test]
fn complete_state_machine_handles_idempotency_challenge_and_resolution() {
    let commands = fixture_commands();
    let signed: Vec<_> = commands
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, command)| sign(index, command).unwrap())
        .collect();
    let mut state = test_state();

    assert!(matches!(
        state.apply(&signed[0]).unwrap(),
        ApplyOutcome::Applied { .. }
    ));
    assert!(matches!(
        state.apply(&signed[0]).unwrap(),
        ApplyOutcome::Idempotent { .. }
    ));
    for (index, command) in signed[1..].iter().enumerate() {
        let outcome = state.apply(command).unwrap();
        let ApplyOutcome::Applied {
            changed_object_refs,
            ..
        } = outcome
        else {
            unreachable!()
        };
        let command_index = index + 1;
        let expected_changed = match command_index {
            5 => 2,
            6 => 3,
            _ => 1,
        };
        assert_eq!(changed_object_refs.len(), expected_changed);
        for object_ref in changed_object_refs {
            assert!(!state.object_canonical_bytes(object_ref).unwrap().is_empty());
            assert_ne!(state.object_leaf_hash(object_ref).unwrap(), [0; 32]);
        }
    }

    let claim_key = commands[3].primary_object_ref().key;
    let claim = state.get_claim(claim_key).unwrap();
    assert_eq!(claim.status, ClaimStatus::Amended);
    assert_eq!(claim.object_ref.object_version, 3);
    assert_eq!(claim.active_challenge, None);

    let challenge_key = commands[5].primary_object_ref().key;
    let challenge = state.get_challenge(challenge_key).unwrap();
    assert_eq!(challenge.status, ClaimChallengeStatus::Resolved);
    assert_eq!(challenge.object_ref.object_version, 2);
    assert!(challenge.resolution_ref.is_some());
}

#[test]
fn altered_replay_is_rejected_even_when_new_payload_is_validly_signed() {
    let original = fixture_commands()[0].clone();
    let signed = sign(0, original.clone()).unwrap();
    let mut state = test_state();
    state.apply(&signed).unwrap();

    let ResearchCommandV1::MatchEvidenceCommitment(mut altered) = original else {
        unreachable!()
    };
    altered.event_count += 1;
    let altered = SignedResearchCommandV1::sign(
        signed.chain_id.clone(),
        signed.command_id,
        signed.signer_did.clone(),
        signed.signer_role,
        signed.nonce,
        ResearchCommandV1::MatchEvidenceCommitment(altered),
        &SigningKey::from_bytes(&NAKAMA_SEED),
    )
    .unwrap();
    assert_ne!(signed.command_fingerprint(), altered.command_fingerprint());
    assert!(matches!(
        state.apply(&altered),
        Err(ProtocolStateError::AlteredReplay { .. })
    ));
}

#[test]
fn state_machine_rejects_missing_and_stale_references() {
    let commands = fixture_commands();
    let mut state = test_state();
    let evaluation = sign(1, commands[1].clone()).unwrap();
    assert!(matches!(
        state.apply(&evaluation),
        Err(ProtocolStateError::MissingReferencedObject(_))
    ));

    for (index, command) in commands[..6].iter().cloned().enumerate() {
        state.apply(&sign(index, command).unwrap()).unwrap();
    }
    let ResearchCommandV1::ChallengeResearchClaim(mut second_challenge) = commands[5].clone()
    else {
        unreachable!()
    };
    second_challenge.challenge_id = key("hepta.claim-challenge", "claim-challenge-002");
    let second = sign(
        20,
        ResearchCommandV1::ChallengeResearchClaim(second_challenge),
    )
    .unwrap();
    assert!(matches!(
        state.apply(&second),
        Err(ProtocolStateError::ObjectVersionMismatch { actual: 2, .. })
    ));
}

#[test]
fn rejected_evaluation_cannot_mint_accepted_workload() {
    let mut commands = fixture_commands();
    let ResearchCommandV1::EvaluationCommitment(evaluation) = &mut commands[1] else {
        unreachable!()
    };
    evaluation.accepted = false;

    let mut state = test_state();
    state.apply(&sign(0, commands[0].clone()).unwrap()).unwrap();
    state.apply(&sign(1, commands[1].clone()).unwrap()).unwrap();
    assert_eq!(
        state
            .apply(&sign(2, commands[2].clone()).unwrap())
            .unwrap_err(),
        ProtocolStateError::RejectedEvaluationCannotIssueWorkload
    );
}

#[test]
fn snapshots_round_trip_and_fail_closed_on_corruption() {
    let mut state = test_state();
    for (index, command) in fixture_commands().into_iter().enumerate() {
        state.apply(&sign(index, command).unwrap()).unwrap();
    }
    let snapshot = state.export_snapshot();
    let json = serde_json::to_vec(&snapshot).unwrap();
    let decoded: ResearchProtocolSnapshotV1 = serde_json::from_slice(&json).unwrap();
    let restored = ResearchProtocolState::from_snapshot(decoded).unwrap();
    assert_eq!(restored, state);
    assert_eq!(
        restored.canonical_snapshot_bytes(),
        state.canonical_snapshot_bytes()
    );
    assert_eq!(
        restored.canonical_snapshot_hash(),
        state.canonical_snapshot_hash()
    );
    assert_eq!(restored.current_object_refs().len(), 7);

    let claim = restored
        .current_object_refs()
        .into_iter()
        .find(|object_ref| object_ref.kind == ResearchObjectKind::ResearchClaim)
        .unwrap();
    let mut stale_claim = claim;
    stale_claim.object_version -= 1;
    assert!(matches!(
        restored.object_canonical_bytes(stale_claim),
        Err(ProtocolStateError::ObjectVersionMismatch { .. })
    ));

    let mut corrupted = snapshot;
    corrupted.protocol_version = 99;
    assert_eq!(
        ResearchProtocolState::from_snapshot(corrupted).unwrap_err(),
        ProtocolStateError::UnsupportedSnapshotVersion(99)
    );
}

#[derive(Debug, Serialize, Deserialize)]
struct GoldenFixture {
    protocol: String,
    canonical_encoding: String,
    nakama_test_seed_hex: String,
    hepta_test_seed_hex: String,
    external_keys: Vec<GoldenExternalKey>,
    commands: Vec<GoldenCommand>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GoldenExternalKey {
    namespace: String,
    external_id: String,
    key_hex: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GoldenCommand {
    command_type: String,
    canonical_cbor_hex: String,
    payload_hash_hex: String,
    command_fingerprint_hex: String,
    signature_hex: String,
}

fn generated_golden_fixture() -> GoldenFixture {
    let commands = fixture_commands();
    GoldenFixture {
        protocol: "trnm-research-protocol-v1".into(),
        canonical_encoding: CANONICAL_ENCODING.into(),
        nakama_test_seed_hex: hex::encode(NAKAMA_SEED),
        hepta_test_seed_hex: hex::encode(HEPTA_SEED),
        external_keys: vec![
            (
                "nakama.match",
                "match-001",
                key("nakama.match", "match-001"),
            ),
            ("hepta.claim", "claim-001", key("hepta.claim", "claim-001")),
            (
                "hepta.match",
                "550e8400-e29b-41d4-a716-446655440000",
                ExternalKey::from_uuid("hepta.match", "550e8400-e29b-41d4-a716-446655440000")
                    .unwrap(),
            ),
        ]
        .into_iter()
        .map(|(namespace, external_id, key)| GoldenExternalKey {
            namespace: namespace.into(),
            external_id: external_id.into(),
            key_hex: key.to_hex(),
        })
        .collect(),
        commands: commands
            .into_iter()
            .enumerate()
            .map(|(index, command)| {
                let signed = sign(index, command.clone()).unwrap();
                GoldenCommand {
                    command_type: command.command_type().into(),
                    canonical_cbor_hex: hex::encode(command.canonical_bytes()),
                    payload_hash_hex: hex::encode(signed.payload_hash()),
                    command_fingerprint_hex: hex::encode(signed.command_fingerprint()),
                    signature_hex: hex::encode(signed.signature),
                }
            })
            .collect(),
    }
}

#[test]
fn canonical_bytes_match_cross_implementation_golden_fixture() {
    let expected: GoldenFixture =
        serde_json::from_str(include_str!("../fixtures/protocol-v1-golden.json")).unwrap();
    let actual = generated_golden_fixture();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
}

#[test]
#[ignore = "fixture regeneration helper"]
fn print_golden_fixture_for_regeneration() {
    println!(
        "{}",
        serde_json::to_string_pretty(&generated_golden_fixture()).unwrap()
    );
}
