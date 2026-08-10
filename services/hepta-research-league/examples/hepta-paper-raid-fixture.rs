use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use hepta_research_league::paper_raid_contracts::{
    agent_binding_key_rotation_signing_bytes, agent_binding_proof_signing_bytes,
    authorization_set_consumption_receipt_signing_bytes, authorship_consent_signing_bytes,
    canonical_json_sha256, consumer_user_assertion_signing_bytes,
    nakama_completion_receipt_signing_bytes, paper_bundle_frame, paper_bundle_hash,
    paper_raid_evidence_envelope_hash, paper_raid_evidence_envelope_signing_bytes,
    paper_release_candidate_frame, paper_release_candidate_hash,
    publication_release_author_signing_bytes, publication_release_frame, publication_release_hash,
    sha256_digest, sign_authorization_set_consumption_receipt, sign_authorship_consent,
    sign_consumer_user_assertion, sign_nakama_completion_receipt,
    sign_paper_raid_evidence_envelope, AgentBindingKeyRotationClaimV2, AgentBindingProofClaimV2,
    AuthorshipConsentSigningV2, ConsumerUserAssertionClaimV2, PaperBundleAuthorConsentV2,
    PaperBundleV2, PaperReleaseAuthorV2, PaperReleaseCandidateV2,
    PublicationReleaseAuthorConsentV1, PublicationReleaseV1, ResearchSessionTerminalFactsV1,
    SignedAuthorizationSetConsumptionReceiptV1, SignedNakamaCompletionReceiptV1,
    SignedPaperRaidEvidenceEnvelopeV1, AGENT_BINDING_KEY_ROTATION_V2, AGENT_BINDING_PROOF_V2,
    AUTHORIZATION_SET_CONSUMPTION_RECEIPT_V1, AUTHORSHIP_CONSENT_V2, CONSUMER_USER_ASSERTION_V2,
    NAKAMA_COMPLETION_RECEIPT_V1, PAPER_BUNDLE_V2, PAPER_RAID_EVIDENCE_ENVELOPE_V1,
    PAPER_RELEASE_CANDIDATE_V2, PUBLICATION_RELEASE_V1,
};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize)]
struct KeyVector {
    seed_hex: String,
    public_key_base64: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn digest(label: &str) -> String {
    sha256_digest(label.as_bytes())
}

fn key(seed_byte: u8) -> (SigningKey, KeyVector) {
    let seed = [seed_byte; 32];
    let key = SigningKey::from_bytes(&seed);
    let vector = KeyVector {
        seed_hex: hex(&seed),
        public_key_base64: BASE64.encode(key.verifying_key().to_bytes()),
    };
    (key, vector)
}

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("fixture UUID")
}

fn main() {
    let (consumer_key, consumer_vector) = key(0xa1);
    let (agent_key, agent_vector) = key(0xf1);
    let (replacement_agent_key, replacement_agent_vector) = key(0xf2);
    let (hepta_issuer_key, hepta_issuer_vector) = key(0xe1);
    let author_material = [key(0xb1), key(0xc1), key(0xd1)];
    let mut keys = BTreeMap::new();
    keys.insert("consumer_edge", consumer_vector);
    keys.insert("agent_binding", agent_vector);
    keys.insert("agent_binding_replacement", replacement_agent_vector);
    keys.insert("hepta_receipt_issuer", hepta_issuer_vector);
    for (index, (_, vector)) in author_material.iter().enumerate() {
        keys.insert(
            match index {
                0 => "author_1",
                1 => "author_2",
                _ => "author_3",
            },
            KeyVector {
                seed_hex: vector.seed_hex.clone(),
                public_key_base64: vector.public_key_base64.clone(),
            },
        );
    }

    let assertion_claim = ConsumerUserAssertionClaimV2 {
        schema: CONSUMER_USER_ASSERTION_V2.to_string(),
        assertion_id: id("10000000-0000-4000-8000-000000000001"),
        issuer: "consumer-edge-golden".to_string(),
        audience: "hepta-paper-raid-v2".to_string(),
        subject_id: "oidc|paper-raid-golden-author-1".to_string(),
        nakama_user_id: id("20000000-0000-4000-8000-000000000001"),
        player_id: id("30000000-0000-4000-8000-000000000001"),
        operation: "create_paper_project_v2".to_string(),
        http_method: "POST".to_string(),
        canonical_path: "/v2/hepta/papers".to_string(),
        idempotency_key: "paper-raid-golden-create-paper-001".to_string(),
        body_hash: digest("consumer-request-body:golden:v2"),
        issued_at_unix: 1_800_100_000,
        expires_at_unix: 1_800_100_120,
        nonce: "paper-raid-golden-create-paper-001".to_string(),
    };
    let assertion = sign_consumer_user_assertion(
        assertion_claim,
        "consumer-edge-key-golden-v2",
        &consumer_key,
    )
    .expect("sign assertion");
    let assertion_frame =
        consumer_user_assertion_signing_bytes(&assertion.claim, &assertion.issuer_key_id)
            .expect("assertion frame");

    let agent_public_key = BASE64.encode(agent_key.verifying_key().to_bytes());
    let agent_key_id = sha256_digest(&agent_key.verifying_key().to_bytes());
    let agent_binding_proof = AgentBindingProofClaimV2 {
        schema: AGENT_BINDING_PROOF_V2.to_string(),
        binding_id: id("31000000-0000-4000-8000-000000000001"),
        agent_id: "did:trnm:paper-raid-golden-agent-1".to_string(),
        agent_key_id: agent_key_id.clone(),
        agent_public_key: agent_public_key.clone(),
        agent_public_key_hash: agent_key_id,
        subject_id: assertion.claim.subject_id.clone(),
        player_id: assertion.claim.player_id,
        nonce: "paper-raid-golden-agent-binding-001".to_string(),
        issued_at_unix: 1_800_100_000,
        expires_at_unix: 1_800_100_300,
    };
    let agent_binding_frame =
        agent_binding_proof_signing_bytes(&agent_binding_proof).expect("Agent binding frame");
    let agent_binding_signature = BASE64.encode(agent_key.sign(&agent_binding_frame).to_bytes());
    let replacement_public_key = BASE64.encode(replacement_agent_key.verifying_key().to_bytes());
    let replacement_key_id = sha256_digest(&replacement_agent_key.verifying_key().to_bytes());
    let agent_binding_rotation = AgentBindingKeyRotationClaimV2 {
        schema: AGENT_BINDING_KEY_ROTATION_V2.to_string(),
        rotation_id: id("32000000-0000-4000-8000-000000000001"),
        binding_id: agent_binding_proof.binding_id,
        expected_binding_version: 1,
        player_id: agent_binding_proof.player_id,
        subject_id: agent_binding_proof.subject_id.clone(),
        agent_id: agent_binding_proof.agent_id.clone(),
        old_agent_key_id: agent_binding_proof.agent_key_id.clone(),
        old_agent_public_key: agent_binding_proof.agent_public_key.clone(),
        old_agent_public_key_hash: agent_binding_proof.agent_public_key_hash.clone(),
        new_agent_key_id: replacement_key_id.clone(),
        new_agent_public_key: replacement_public_key,
        new_agent_public_key_hash: replacement_key_id,
        nonce: "paper-raid-golden-agent-rotation-001".to_string(),
        issued_at_unix: 1_800_100_400,
        expires_at_unix: 1_800_100_700,
    };
    let agent_binding_rotation_frame =
        agent_binding_key_rotation_signing_bytes(&agent_binding_rotation)
            .expect("Agent binding rotation frame");
    let agent_binding_rotation_old_signature =
        BASE64.encode(agent_key.sign(&agent_binding_rotation_frame).to_bytes());
    let agent_binding_rotation_new_signature = BASE64.encode(
        replacement_agent_key
            .sign(&agent_binding_rotation_frame)
            .to_bytes(),
    );

    let player_ids = [
        id("30000000-0000-4000-8000-000000000001"),
        id("30000000-0000-4000-8000-000000000002"),
        id("30000000-0000-4000-8000-000000000003"),
    ];
    let release_candidate = PaperReleaseCandidateV2 {
        schema: PAPER_RELEASE_CANDIDATE_V2.to_string(),
        paper_project_id: id("40000000-0000-4000-8000-000000000001"),
        revision_id: id("50000000-0000-4000-8000-000000000001"),
        team_id: id("60000000-0000-4000-8000-000000000001"),
        challenge_id: id("70000000-0000-4000-8000-000000000001"),
        ruleset_hash: digest("paper-raid-ruleset:golden:v2"),
        challenge_snapshot_hash: digest("paper-raid-challenge-snapshot:golden:v2"),
        roster_version: 1,
        title: "Reproducing a Public Baseline with a Small Ablation".to_string(),
        abstract_text: "A deterministic Paper Raid golden release candidate.".to_string(),
        target_format: "workshop-short-paper-v1".to_string(),
        source_manifest_hash: digest("source-manifest:golden:v2"),
        artifact_manifest_hash: digest("artifact-manifest:golden:v2"),
        bibliography_hash: digest("bibliography:golden:v2"),
        claim_evidence_graph_hash: digest("claim-evidence:golden:v2"),
        section_materialization_root: None,
        collaboration_compact_hash: digest("collaboration-compact:golden:v2"),
        research_protocol_snapshot_hash: digest("research-protocol:golden:v2"),
        ethics_disclosure_hash: digest("ethics-disclosure:golden:v2"),
        coi_disclosure_hash: digest("coi-disclosure:golden:v2"),
        contribution_ledger_hash: digest("contribution-ledger:golden:v2"),
        ai_disclosure_hash: digest("ai-disclosure:golden:v2"),
        license: "CC-BY-4.0".to_string(),
        authors: vec![
            PaperReleaseAuthorV2 {
                author_order: 1,
                participant_slot: 2,
                player_id: player_ids[1],
                display_name: "Method Author".to_string(),
                credit_roles: vec!["methodology".to_string(), "software".to_string()],
            },
            PaperReleaseAuthorV2 {
                author_order: 2,
                participant_slot: 1,
                player_id: player_ids[0],
                display_name: "Captain Author".to_string(),
                credit_roles: vec![
                    "conceptualization".to_string(),
                    "writing_review_editing".to_string(),
                ],
            },
            PaperReleaseAuthorV2 {
                author_order: 3,
                participant_slot: 3,
                player_id: player_ids[2],
                display_name: "Evidence Author".to_string(),
                credit_roles: vec!["data_curation".to_string(), "validation".to_string()],
            },
        ],
    };
    let release_frame = paper_release_candidate_frame(&release_candidate).expect("release frame");
    let release_hash = paper_release_candidate_hash(&release_candidate).expect("release hash");

    let ordered_material = [
        (
            &author_material[1].0,
            &author_material[1].1,
            1_u32,
            2_u32,
            player_ids[1],
        ),
        (
            &author_material[0].0,
            &author_material[0].1,
            2_u32,
            1_u32,
            player_ids[0],
        ),
        (
            &author_material[2].0,
            &author_material[2].1,
            3_u32,
            3_u32,
            player_ids[2],
        ),
    ];
    let mut consent_vectors = Vec::new();
    let mut bundle_consents = Vec::new();
    for (index, (signing_key, key_vector, author_order, slot, player_id)) in
        ordered_material.iter().enumerate()
    {
        let public_key = BASE64
            .decode(&key_vector.public_key_base64)
            .expect("public key");
        let consent = AuthorshipConsentSigningV2 {
            schema: AUTHORSHIP_CONSENT_V2.to_string(),
            consent_id: Uuid::from_u128(0x80000000000040008000000000000001 + index as u128),
            paper_project_id: release_candidate.paper_project_id,
            revision_id: release_candidate.revision_id,
            player_id: *player_id,
            signing_key_id: format!("human-author-key-golden-{}", index + 1),
            signing_public_key: key_vector.public_key_base64.clone(),
            signing_public_key_hash: sha256_digest(&public_key),
            release_candidate_hash: release_hash.clone(),
            signed_at_unix: 1_800_100_200 + index as i64,
        };
        let signing_frame = authorship_consent_signing_bytes(&consent).expect("consent frame");
        let signature = sign_authorship_consent(&consent, signing_key).expect("consent signature");
        consent_vectors.push(json!({
            "signing": consent,
            "signing_frame_hex": hex(&signing_frame),
            "signature": signature,
        }));
        bundle_consents.push(PaperBundleAuthorConsentV2 {
            author_order: *author_order,
            participant_slot: *slot,
            player_id: *player_id,
            consent_id: consent.consent_id,
            signing_key_id: consent.signing_key_id,
            signing_public_key: consent.signing_public_key,
            signing_public_key_hash: consent.signing_public_key_hash,
            signed_at_unix: consent.signed_at_unix,
            signature,
        });
    }
    let mut bundle = PaperBundleV2 {
        schema: PAPER_BUNDLE_V2.to_string(),
        release_candidate: release_candidate.clone(),
        release_candidate_hash: release_hash.clone(),
        author_consents: bundle_consents,
        paper_bundle_hash: String::new(),
    };
    let bundle_frame = paper_bundle_frame(&bundle).expect("PaperBundle frame");
    bundle.paper_bundle_hash = paper_bundle_hash(&bundle).expect("PaperBundle hash");

    let consumption_receipt = sign_authorization_set_consumption_receipt(
        SignedAuthorizationSetConsumptionReceiptV1 {
            schema: AUTHORIZATION_SET_CONSUMPTION_RECEIPT_V1.to_string(),
            session_id: "paper-raid-golden-session-001".to_string(),
            team_id: release_candidate.team_id,
            paper_project_id: release_candidate.paper_project_id,
            challenge_id: release_candidate.challenge_id,
            session_roster_version: 2,
            roster_root: "sha256:96b6646d5f0274f42eb51b1d714a444eba1412a747af3dfa1c22a6eb8fd83fb4"
                .to_string(),
            authorization_ids: vec![
                id("90000000-0000-4000-8000-000000000001"),
                id("90000000-0000-4000-8000-000000000002"),
                id("90000000-0000-4000-8000-000000000003"),
            ],
            consumed_at_unix: 1_800_100_300,
            issuer_key_id: "hepta-receipt-issuer-golden-v1".to_string(),
            signature: String::new(),
        },
        &hepta_issuer_key,
    )
    .expect("sign consumption receipt");
    let consumption_frame =
        authorization_set_consumption_receipt_signing_bytes(&consumption_receipt)
            .expect("consumption receipt frame");

    let completion_receipt = sign_nakama_completion_receipt(
        SignedNakamaCompletionReceiptV1 {
            schema: NAKAMA_COMPLETION_RECEIPT_V1.to_string(),
            commitment_id:
                "sha256:f12c79c3b10f057d8f4122d546ebd279d65d4a66938710c3f057a53d698dcd18"
                    .to_string(),
            session_id: consumption_receipt.session_id.clone(),
            team_id: release_candidate.team_id,
            paper_project_id: release_candidate.paper_project_id,
            challenge_id: release_candidate.challenge_id,
            roster_version: consumption_receipt.session_roster_version,
            roster_root: consumption_receipt.roster_root.clone(),
            event_count: 11,
            event_root: "sha256:29d382fe2557015927e4d55adf9493978309a6cd1db3a2784492978ee50194a0"
                .to_string(),
            archive_hash: "sha256:aab201a90dae59db6c35823d0db4ef616137246ba9db89af1fb8e1d1810e4103"
                .to_string(),
            ruleset_hash: release_candidate.ruleset_hash.clone(),
            challenge_snapshot_hash: release_candidate.challenge_snapshot_hash.clone(),
            nakama_authority_key_id: "nakama-authority-golden-v1".to_string(),
            terminal_facts: ResearchSessionTerminalFactsV1 {
                result_code: "paper_bundle_ready".to_string(),
                paper_bundle_hash: bundle.paper_bundle_hash.clone(),
                paper_release_candidate_hash: release_hash.clone(),
                contribution_ledger_hash: release_candidate.contribution_ledger_hash.clone(),
            },
            verified_at_unix: 1_800_100_400,
            issuer_key_id: "hepta-receipt-issuer-golden-v1".to_string(),
            signature: String::new(),
        },
        &hepta_issuer_key,
    )
    .expect("sign completion receipt");
    let completion_frame = nakama_completion_receipt_signing_bytes(&completion_receipt)
        .expect("completion receipt frame");
    let completion_receipt_hash =
        canonical_json_sha256(&completion_receipt).expect("completion receipt JSON hash");

    let evidence_envelope = sign_paper_raid_evidence_envelope(
        SignedPaperRaidEvidenceEnvelopeV1 {
            schema: PAPER_RAID_EVIDENCE_ENVELOPE_V1.to_string(),
            evidence_envelope_id: id("a0000000-0000-4000-8000-000000000001"),
            paper_bundle_hash: bundle.paper_bundle_hash.clone(),
            nakama_completion_receipt_hash: completion_receipt_hash,
            session_id: completion_receipt.session_id.clone(),
            session_roster_version: completion_receipt.roster_version,
            roster_root: completion_receipt.roster_root.clone(),
            event_root: completion_receipt.event_root.clone(),
            archive_hash: completion_receipt.archive_hash.clone(),
            ruleset_hash: completion_receipt.ruleset_hash.clone(),
            challenge_snapshot_hash: completion_receipt.challenge_snapshot_hash.clone(),
            evaluation_report_hash: digest("evaluation-report:golden:v1"),
            reproduction_report_hash: digest("reproduction-report:golden:v1"),
            appeal_resolution_hash: None,
            finality_receipt_hash: None,
            created_at_unix: 1_800_100_500,
            issuer_key_id: "hepta-receipt-issuer-golden-v1".to_string(),
            signature: String::new(),
        },
        &hepta_issuer_key,
    )
    .expect("sign evidence envelope");
    let evidence_frame = paper_raid_evidence_envelope_signing_bytes(&evidence_envelope)
        .expect("evidence envelope frame");
    let evidence_hash =
        paper_raid_evidence_envelope_hash(&evidence_envelope).expect("evidence envelope hash");

    let mut publication_release = PublicationReleaseV1 {
        schema: PUBLICATION_RELEASE_V1.to_string(),
        publication_release_id: id("b0000000-0000-4000-8000-000000000001"),
        paper_bundle_hash: bundle.paper_bundle_hash.clone(),
        evidence_envelope_hash: evidence_hash.clone(),
        destination: "manual-workshop-submission".to_string(),
        release_manifest_hash: digest("publication-release-manifest:golden:v1"),
        license: release_candidate.license.clone(),
        released_at_unix: 1_800_100_600,
        author_consents: ordered_material
            .iter()
            .map(|(_, vector, author_order, slot, player_id)| {
                let public_key = BASE64
                    .decode(&vector.public_key_base64)
                    .expect("publication release public key");
                PublicationReleaseAuthorConsentV1 {
                    author_order: *author_order,
                    participant_slot: *slot,
                    player_id: *player_id,
                    signing_key_id: format!("human-author-key-golden-{author_order}"),
                    signing_public_key: vector.public_key_base64.clone(),
                    signing_public_key_hash: sha256_digest(&public_key),
                    signature: String::new(),
                }
            })
            .collect(),
        publication_release_hash: String::new(),
    };
    for (index, (signing_key, _, _, _, _)) in ordered_material.iter().enumerate() {
        let signing_frame = publication_release_author_signing_bytes(
            &publication_release,
            &publication_release.author_consents[index],
        )
        .expect("publication release author frame");
        publication_release.author_consents[index].signature =
            BASE64.encode(signing_key.sign(&signing_frame).to_bytes());
    }
    let publication_frame =
        publication_release_frame(&publication_release).expect("publication release frame");
    publication_release.publication_release_hash =
        publication_release_hash(&publication_release).expect("publication release hash");

    let output = json!({
        "schema": "hepta.paper_raid.golden_vectors.v2",
        "fixture_notice": "TEST ONLY. Deterministic seeds and private keys MUST NEVER be used in production.",
        "keys": keys,
        "source_digests": {
            "consumer_request_body": {"utf8": "consumer-request-body:golden:v2", "digest": digest("consumer-request-body:golden:v2")},
            "source_manifest": {"utf8": "source-manifest:golden:v2", "digest": digest("source-manifest:golden:v2")},
            "artifact_manifest": {"utf8": "artifact-manifest:golden:v2", "digest": digest("artifact-manifest:golden:v2")}
        },
        "consumer_assertion": {
            "value": assertion,
            "signing_frame_hex": hex(&assertion_frame),
        },
        "agent_binding_proof": {
            "claim": agent_binding_proof,
            "signing_frame_hex": hex(&agent_binding_frame),
            "signature": agent_binding_signature,
        },
        "agent_binding_key_rotation": {
            "claim": agent_binding_rotation,
            "signing_frame_hex": hex(&agent_binding_rotation_frame),
            "old_key_signature": agent_binding_rotation_old_signature,
            "new_key_signature": agent_binding_rotation_new_signature,
        },
        "release_candidate": {
            "value": release_candidate,
            "frame_hex": hex(&release_frame),
            "hash": release_hash,
        },
        "authorship_consents": consent_vectors,
        "paper_bundle": {
            "value": bundle,
            "frame_hex": hex(&bundle_frame),
        },
        "authorization_consumption_receipt": {
            "value": consumption_receipt,
            "signing_frame_hex": hex(&consumption_frame),
        },
        "nakama_completion_receipt": {
            "value": completion_receipt,
            "signing_frame_hex": hex(&completion_frame),
        },
        "paper_raid_evidence_envelope": {
            "value": evidence_envelope,
            "signing_frame_hex": hex(&evidence_frame),
            "hash": evidence_hash,
        },
        "publication_release": {
            "value": publication_release,
            "frame_hex": hex(&publication_frame),
        },
        "negative_cases": [
            "consumer_assertion_body_hash_tamper",
            "agent_binding_subject_tamper",
            "agent_binding_rotation_signature_substitution",
            "agent_binding_rotation_noop",
            "agent_binding_rotation_version_overflow",
            "release_candidate_title_tamper",
            "authorship_consent_release_hash_tamper",
            "paper_bundle_signature_tamper",
            "authorization_consumption_time_tamper",
            "completion_ruleset_hash_tamper",
            "evidence_event_root_tamper",
            "publication_release_destination_tamper",
            "evidence_receipt_root_mismatch",
            "evidence_completion_receipt_hash_tamper",
            "evidence_review_reference_tamper",
            "publication_release_author_replacement"
        ]
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).expect("fixture JSON")
    );
}
