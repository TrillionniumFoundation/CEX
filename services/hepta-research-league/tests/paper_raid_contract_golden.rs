use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::VerifyingKey;
use hepta_research_league::paper_raid_contracts::{
    agent_binding_key_rotation_signing_bytes, agent_binding_proof_signing_bytes,
    agent_binding_proof_v3_signing_bytes, agent_capability_disclosure_frame,
    agent_capability_disclosure_hash, authorization_set_consumption_receipt_signing_bytes,
    authorship_consent_signing_bytes, canonical_json_sha256, consumer_user_assertion_signing_bytes,
    nakama_completion_receipt_signing_bytes, paper_bundle_frame, paper_bundle_hash,
    paper_raid_evidence_envelope_hash, paper_raid_evidence_envelope_signing_bytes,
    paper_release_candidate_frame, paper_release_candidate_hash, publication_release_frame,
    publication_release_hash, sha256_digest, verify_agent_binding_key_rotation_signatures,
    verify_agent_binding_proof, verify_agent_binding_proof_v3,
    verify_authorization_set_consumption_receipt, verify_authorship_consent_signature,
    verify_consumer_user_assertion_signature, verify_nakama_completion_receipt,
    verify_paper_raid_evidence_envelope, verify_paper_raid_evidence_envelope_against,
    verify_publication_release_against, AgentBindingKeyRotationClaimV2, AgentBindingProofClaimV2,
    AgentBindingProofClaimV3, AgentCapabilityDisclosureAssuranceV1, AgentCapabilityDisclosureV1,
    AgentCapabilityV1, AgentResourceClassV1, AuthorshipConsentSigningV2, PaperBundleV2,
    PaperReleaseCandidateV2, PublicationReleaseV1, SignedAuthorizationSetConsumptionReceiptV1,
    SignedConsumerUserAssertionV2, SignedNakamaCompletionReceiptV1,
    SignedPaperRaidEvidenceEnvelopeV1, AGENT_BINDING_PROOF_V3, AGENT_CAPABILITY_DISCLOSURE_V1,
    JSON_SAFE_U64_MAX,
};
use serde::Deserialize;

const FIXTURE: &[u8] = include_bytes!("../../../docs/sdk-fixtures/hepta-paper-raid-v2.json");
const FIXTURE_SHA256: &str =
    "sha256:c9f25b1f68fb4ccdd31f4eb4c113292dc5dbaa9b9ddbb3cd6a168655854d808c";

#[derive(Deserialize)]
struct KeyVector {
    public_key_base64: String,
}

#[derive(Deserialize)]
struct AssertionVector {
    value: SignedConsumerUserAssertionV2,
    signing_frame_hex: String,
}

#[derive(Deserialize)]
struct AgentBindingVector {
    claim: AgentBindingProofClaimV2,
    signing_frame_hex: String,
    signature: String,
}

#[derive(Deserialize)]
struct AgentBindingRotationVector {
    claim: AgentBindingKeyRotationClaimV2,
    signing_frame_hex: String,
    old_key_signature: String,
    new_key_signature: String,
}

#[derive(Deserialize)]
struct ReleaseVector {
    value: PaperReleaseCandidateV2,
    frame_hex: String,
    hash: String,
}

#[derive(Deserialize)]
struct ConsentVector {
    signing: AuthorshipConsentSigningV2,
    signing_frame_hex: String,
    signature: String,
}

#[derive(Deserialize)]
struct BundleVector {
    value: PaperBundleV2,
    frame_hex: String,
}

#[derive(Deserialize)]
struct ConsumptionVector {
    value: SignedAuthorizationSetConsumptionReceiptV1,
    signing_frame_hex: String,
}

#[derive(Deserialize)]
struct CompletionVector {
    value: SignedNakamaCompletionReceiptV1,
    signing_frame_hex: String,
}

#[derive(Deserialize)]
struct EvidenceVector {
    value: SignedPaperRaidEvidenceEnvelopeV1,
    signing_frame_hex: String,
    hash: String,
}

#[derive(Deserialize)]
struct PublicationVector {
    value: PublicationReleaseV1,
    frame_hex: String,
}

#[derive(Deserialize)]
struct Fixture {
    schema: String,
    keys: serde_json::Map<String, serde_json::Value>,
    consumer_assertion: AssertionVector,
    agent_binding_proof: AgentBindingVector,
    agent_binding_key_rotation: AgentBindingRotationVector,
    release_candidate: ReleaseVector,
    authorship_consents: Vec<ConsentVector>,
    paper_bundle: BundleVector,
    authorization_consumption_receipt: ConsumptionVector,
    nakama_completion_receipt: CompletionVector,
    paper_raid_evidence_envelope: EvidenceVector,
    publication_release: PublicationVector,
    negative_cases: Vec<String>,
}

fn decode_hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).expect("fixture hex"))
        .collect()
}

#[test]
fn agent_binding_v3_capability_disclosure_has_frozen_cross_language_bytes() {
    const DISCLOSURE_FRAME_HEX: &str = "68657074615f70617065725f726169645f6167656e745f6361706162696c6974795f646973636c6f737572655f7631000000002f68657074612e70617065725f726169642e6167656e745f6361706162696c6974795f646973636c6f737572652e76310000001873656c665f6465636c617265645f756e7665726966696564000000030000001161727469666163745f616e616c797369730000000f65766964656e63655f7365617263680000001073656374696f6e5f6472616674696e67000000030000000b61727469666163745f696f000000036370750000000773616e64626f7800000002";
    const PROOF_FRAME_HEX: &str = "68657074615f70617065725f726169645f6167656e745f62696e64696e675f70726f6f665f7633000000002768657074612e70617065725f726169642e6167656e745f62696e64696e675f70726f6f662e76330000002433313030303030302d303030302d343030302d383030302d303030303030303030303033000000236469643a74726e6d3a70617065722d726169642d676f6c64656e2d6167656e742d7633000000477368613235363a6430326662626562373665323164663962636538306537626362663838316637313963386565366437626131646331656437376532326234363865336664633200000020b8fc9d70b330b5d377a521047bce772144a747caa203c453a91385de86e177ccd02fbbeb76e21df9bce80e7bcbf881f719c8ee6d7ba1dc1ed77e22b468e3fdc222aeaec9954e5774212ab9103e28d7e3e52c827c8d976839715d9b3b1cc41fec000000206f6964637c70617065722d726169642d676f6c64656e2d617574686f722d76330000002433303030303030302d303030302d343030302d383030302d3030303030303030303030330000002670617065722d726169642d676f6c64656e2d6167656e742d62696e64696e672d76332d303031000000006b4b58a0000000006b4b59cc";
    const SIGNATURE: &str =
        "asP/gO6V+ntcYVGQaarHmVpCKqpXLm7kOyvCspAFBSRcl6WeV2lI4YMgbmOgpbOGZqOXzXbyBSl/nShkrvWYBg==";
    let disclosure = AgentCapabilityDisclosureV1 {
        schema: AGENT_CAPABILITY_DISCLOSURE_V1.to_string(),
        assurance: AgentCapabilityDisclosureAssuranceV1::SelfDeclaredUnverified,
        capabilities: vec![
            AgentCapabilityV1::ArtifactAnalysis,
            AgentCapabilityV1::EvidenceSearch,
            AgentCapabilityV1::SectionDrafting,
        ],
        resource_classes: vec![
            AgentResourceClassV1::ArtifactIo,
            AgentResourceClassV1::Cpu,
            AgentResourceClassV1::Sandbox,
        ],
        max_parallel_tasks: 2,
    };
    assert_eq!(
        agent_capability_disclosure_frame(&disclosure).unwrap(),
        decode_hex(DISCLOSURE_FRAME_HEX)
    );
    let disclosure_hash = agent_capability_disclosure_hash(&disclosure).unwrap();
    assert_eq!(
        disclosure_hash,
        "sha256:22aeaec9954e5774212ab9103e28d7e3e52c827c8d976839715d9b3b1cc41fec"
    );
    let claim = AgentBindingProofClaimV3 {
        schema: AGENT_BINDING_PROOF_V3.to_string(),
        binding_id: "31000000-0000-4000-8000-000000000003".parse().unwrap(),
        agent_id: "did:trnm:paper-raid-golden-agent-v3".into(),
        agent_key_id: "sha256:d02fbbeb76e21df9bce80e7bcbf881f719c8ee6d7ba1dc1ed77e22b468e3fdc2"
            .into(),
        agent_public_key: "uPydcLMwtdN3pSEEe853IUSnR8qiA8RTqROF3obhd8w=".into(),
        agent_public_key_hash:
            "sha256:d02fbbeb76e21df9bce80e7bcbf881f719c8ee6d7ba1dc1ed77e22b468e3fdc2".into(),
        capability_disclosure_hash: disclosure_hash,
        subject_id: "oidc|paper-raid-golden-author-v3".into(),
        player_id: "30000000-0000-4000-8000-000000000003".parse().unwrap(),
        nonce: "paper-raid-golden-agent-binding-v3-001".into(),
        issued_at_unix: 1_800_100_000,
        expires_at_unix: 1_800_100_300,
    };
    assert_eq!(
        agent_binding_proof_v3_signing_bytes(&claim).unwrap(),
        decode_hex(PROOF_FRAME_HEX)
    );
    verify_agent_binding_proof_v3(&claim, SIGNATURE).unwrap();
    let mut tampered = claim;
    tampered.capability_disclosure_hash = sha256_digest(b"tampered capability disclosure");
    assert!(verify_agent_binding_proof_v3(&tampered, SIGNATURE).is_err());
}

#[test]
fn rust_verifies_hepta_owned_cross_language_vectors_and_tamper_negatives() {
    assert_eq!(sha256_digest(FIXTURE), FIXTURE_SHA256);
    let fixture: Fixture = serde_json::from_slice(FIXTURE).expect("Paper Raid fixture");
    assert_eq!(fixture.schema, "hepta.paper_raid.golden_vectors.v2");
    assert_eq!(fixture.authorship_consents.len(), 3);
    assert_eq!(fixture.negative_cases.len(), 16);

    let consumer: KeyVector = serde_json::from_value(
        fixture
            .keys
            .get("consumer_edge")
            .expect("consumer key")
            .clone(),
    )
    .expect("consumer key vector");
    let public_key: [u8; 32] = BASE64
        .decode(consumer.public_key_base64)
        .expect("consumer key base64")
        .try_into()
        .expect("consumer key length");
    let consumer_key = VerifyingKey::from_bytes(&public_key).expect("consumer key");
    let assertion_frame = consumer_user_assertion_signing_bytes(
        &fixture.consumer_assertion.value.claim,
        &fixture.consumer_assertion.value.issuer_key_id,
    )
    .unwrap();
    assert_eq!(
        assertion_frame,
        decode_hex(&fixture.consumer_assertion.signing_frame_hex)
    );
    verify_consumer_user_assertion_signature(&fixture.consumer_assertion.value, &consumer_key)
        .unwrap();

    let agent_frame = agent_binding_proof_signing_bytes(&fixture.agent_binding_proof.claim)
        .expect("Agent binding frame");
    assert_eq!(
        agent_frame,
        decode_hex(&fixture.agent_binding_proof.signing_frame_hex)
    );
    verify_agent_binding_proof(
        &fixture.agent_binding_proof.claim,
        &fixture.agent_binding_proof.signature,
    )
    .expect("Agent binding proof");
    let rotation_frame =
        agent_binding_key_rotation_signing_bytes(&fixture.agent_binding_key_rotation.claim)
            .expect("Agent binding rotation frame");
    assert_eq!(
        rotation_frame,
        decode_hex(&fixture.agent_binding_key_rotation.signing_frame_hex)
    );
    verify_agent_binding_key_rotation_signatures(
        &fixture.agent_binding_key_rotation.claim,
        &fixture.agent_binding_key_rotation.old_key_signature,
        &fixture.agent_binding_key_rotation.new_key_signature,
    )
    .expect("Agent binding rotation signatures");

    let release_frame = paper_release_candidate_frame(&fixture.release_candidate.value).unwrap();
    assert_eq!(
        release_frame,
        decode_hex(&fixture.release_candidate.frame_hex)
    );
    assert_eq!(
        paper_release_candidate_hash(&fixture.release_candidate.value).unwrap(),
        fixture.release_candidate.hash
    );
    assert!(
        fixture
            .release_candidate
            .value
            .section_materialization_root
            .is_none(),
        "legacy fixture must exercise the byte-for-byte compatible unextended v2 frame"
    );
    let mut rooted_release = fixture.release_candidate.value.clone();
    rooted_release.section_materialization_root = Some(sha256_digest(b"materialized-sections"));
    let rooted_frame = paper_release_candidate_frame(&rooted_release).unwrap();
    assert!(rooted_frame.starts_with(&release_frame));
    assert_ne!(
        paper_release_candidate_hash(&rooted_release).unwrap(),
        fixture.release_candidate.hash
    );
    for consent in &fixture.authorship_consents {
        assert_eq!(
            authorship_consent_signing_bytes(&consent.signing).unwrap(),
            decode_hex(&consent.signing_frame_hex)
        );
        verify_authorship_consent_signature(&consent.signing, &consent.signature).unwrap();
    }
    let bundle_frame = paper_bundle_frame(&fixture.paper_bundle.value).unwrap();
    assert_eq!(bundle_frame, decode_hex(&fixture.paper_bundle.frame_hex));
    assert_eq!(
        paper_bundle_hash(&fixture.paper_bundle.value).unwrap(),
        fixture.paper_bundle.value.paper_bundle_hash
    );

    let receipt_issuer: KeyVector = serde_json::from_value(
        fixture
            .keys
            .get("hepta_receipt_issuer")
            .expect("Hepta receipt issuer key")
            .clone(),
    )
    .expect("Hepta receipt issuer key vector");
    let receipt_key: [u8; 32] = BASE64
        .decode(receipt_issuer.public_key_base64)
        .expect("receipt key base64")
        .try_into()
        .expect("receipt key length");
    let receipt_key = VerifyingKey::from_bytes(&receipt_key).expect("receipt key");

    assert_eq!(
        authorization_set_consumption_receipt_signing_bytes(
            &fixture.authorization_consumption_receipt.value,
        )
        .unwrap(),
        decode_hex(&fixture.authorization_consumption_receipt.signing_frame_hex,)
    );
    verify_authorization_set_consumption_receipt(
        &fixture.authorization_consumption_receipt.value,
        &receipt_key,
    )
    .unwrap();

    assert_eq!(
        nakama_completion_receipt_signing_bytes(&fixture.nakama_completion_receipt.value).unwrap(),
        decode_hex(&fixture.nakama_completion_receipt.signing_frame_hex)
    );
    verify_nakama_completion_receipt(&fixture.nakama_completion_receipt.value, &receipt_key)
        .unwrap();
    assert_eq!(
        canonical_json_sha256(&fixture.nakama_completion_receipt.value).unwrap(),
        fixture
            .paper_raid_evidence_envelope
            .value
            .nakama_completion_receipt_hash
    );

    assert_eq!(
        paper_raid_evidence_envelope_signing_bytes(&fixture.paper_raid_evidence_envelope.value)
            .unwrap(),
        decode_hex(&fixture.paper_raid_evidence_envelope.signing_frame_hex)
    );
    verify_paper_raid_evidence_envelope(&fixture.paper_raid_evidence_envelope.value, &receipt_key)
        .unwrap();
    assert_eq!(
        paper_raid_evidence_envelope_hash(&fixture.paper_raid_evidence_envelope.value).unwrap(),
        fixture.paper_raid_evidence_envelope.hash
    );
    verify_paper_raid_evidence_envelope_against(
        &fixture.paper_raid_evidence_envelope.value,
        &fixture.paper_bundle.value,
        &fixture.nakama_completion_receipt.value,
        &fixture
            .paper_raid_evidence_envelope
            .value
            .evaluation_report_hash,
        &fixture
            .paper_raid_evidence_envelope
            .value
            .reproduction_report_hash,
        fixture
            .paper_raid_evidence_envelope
            .value
            .appeal_resolution_hash
            .as_deref(),
        fixture
            .paper_raid_evidence_envelope
            .value
            .finality_receipt_hash
            .as_deref(),
        &receipt_key,
    )
    .unwrap();

    assert_eq!(
        publication_release_frame(&fixture.publication_release.value).unwrap(),
        decode_hex(&fixture.publication_release.frame_hex)
    );
    assert_eq!(
        publication_release_hash(&fixture.publication_release.value).unwrap(),
        fixture.publication_release.value.publication_release_hash
    );
    verify_publication_release_against(
        &fixture.publication_release.value,
        &fixture.paper_bundle.value,
        &fixture.paper_raid_evidence_envelope.value,
    )
    .unwrap();

    let mut assertion_tamper = fixture.consumer_assertion.value.clone();
    assertion_tamper.claim.body_hash = sha256_digest(b"tampered");
    assert!(verify_consumer_user_assertion_signature(&assertion_tamper, &consumer_key).is_err());
    let mut agent_binding_tamper = fixture.agent_binding_proof.claim.clone();
    agent_binding_tamper.subject_id.push_str("-tampered");
    assert!(verify_agent_binding_proof(
        &agent_binding_tamper,
        &fixture.agent_binding_proof.signature,
    )
    .is_err());
    assert!(verify_agent_binding_key_rotation_signatures(
        &fixture.agent_binding_key_rotation.claim,
        &fixture.agent_binding_key_rotation.new_key_signature,
        &fixture.agent_binding_key_rotation.old_key_signature,
    )
    .is_err());
    let mut no_op_rotation = fixture.agent_binding_key_rotation.claim.clone();
    no_op_rotation.new_agent_key_id = no_op_rotation.old_agent_key_id.clone();
    no_op_rotation.new_agent_public_key = no_op_rotation.old_agent_public_key.clone();
    no_op_rotation.new_agent_public_key_hash = no_op_rotation.old_agent_public_key_hash.clone();
    assert!(agent_binding_key_rotation_signing_bytes(&no_op_rotation).is_err());
    let mut overflow_rotation = fixture.agent_binding_key_rotation.claim.clone();
    overflow_rotation.expected_binding_version = JSON_SAFE_U64_MAX + 1;
    assert!(agent_binding_key_rotation_signing_bytes(&overflow_rotation).is_err());
    let mut release_tamper = fixture.release_candidate.value.clone();
    release_tamper.title.push_str(" tampered");
    assert_ne!(
        paper_release_candidate_hash(&release_tamper).unwrap(),
        fixture.release_candidate.hash
    );
    let mut consent_tamper = fixture.authorship_consents[0].signing.clone();
    consent_tamper.release_candidate_hash = sha256_digest(b"tampered");
    assert!(verify_authorship_consent_signature(
        &consent_tamper,
        &fixture.authorship_consents[0].signature,
    )
    .is_err());
    let mut bundle_tamper = fixture.paper_bundle.value.clone();
    bundle_tamper.author_consents[0].signature = BASE64.encode([0_u8; 64]);
    assert!(paper_bundle_frame(&bundle_tamper).is_err());

    let mut consumption_tamper = fixture.authorization_consumption_receipt.value.clone();
    consumption_tamper.consumed_at_unix += 1;
    assert!(
        verify_authorization_set_consumption_receipt(&consumption_tamper, &receipt_key).is_err()
    );
    let mut completion_tamper = fixture.nakama_completion_receipt.value.clone();
    completion_tamper.ruleset_hash = sha256_digest(b"tampered ruleset");
    assert!(verify_nakama_completion_receipt(&completion_tamper, &receipt_key).is_err());
    let mut evidence_tamper = fixture.paper_raid_evidence_envelope.value.clone();
    evidence_tamper.event_root = sha256_digest(b"tampered event root");
    assert!(verify_paper_raid_evidence_envelope(&evidence_tamper, &receipt_key).is_err());
    let mut publication_tamper = fixture.publication_release.value.clone();
    publication_tamper.destination.push_str("-tampered");
    assert!(publication_release_frame(&publication_tamper).is_err());

    let mut receipt_root_mismatch = fixture.nakama_completion_receipt.value.clone();
    receipt_root_mismatch.event_root = sha256_digest(b"different signed receipt root");
    assert!(verify_paper_raid_evidence_envelope_against(
        &fixture.paper_raid_evidence_envelope.value,
        &fixture.paper_bundle.value,
        &receipt_root_mismatch,
        &fixture
            .paper_raid_evidence_envelope
            .value
            .evaluation_report_hash,
        &fixture
            .paper_raid_evidence_envelope
            .value
            .reproduction_report_hash,
        None,
        None,
        &receipt_key,
    )
    .is_err());
    let mut receipt_hash_tamper = fixture.paper_raid_evidence_envelope.value.clone();
    receipt_hash_tamper.nakama_completion_receipt_hash = sha256_digest(b"wrong receipt");
    assert!(verify_paper_raid_evidence_envelope_against(
        &receipt_hash_tamper,
        &fixture.paper_bundle.value,
        &fixture.nakama_completion_receipt.value,
        &receipt_hash_tamper.evaluation_report_hash,
        &receipt_hash_tamper.reproduction_report_hash,
        None,
        None,
        &receipt_key,
    )
    .is_err());
    let mut review_reference_tamper = fixture.paper_raid_evidence_envelope.value.clone();
    review_reference_tamper.evaluation_report_hash = sha256_digest(b"wrong evaluation");
    assert!(verify_paper_raid_evidence_envelope_against(
        &review_reference_tamper,
        &fixture.paper_bundle.value,
        &fixture.nakama_completion_receipt.value,
        &fixture
            .paper_raid_evidence_envelope
            .value
            .evaluation_report_hash,
        &fixture
            .paper_raid_evidence_envelope
            .value
            .reproduction_report_hash,
        None,
        None,
        &receipt_key,
    )
    .is_err());
    let mut author_replacement = fixture.publication_release.value.clone();
    author_replacement.author_consents[0].player_id = uuid::Uuid::new_v4();
    assert!(verify_publication_release_against(
        &author_replacement,
        &fixture.paper_bundle.value,
        &fixture.paper_raid_evidence_envelope.value,
    )
    .is_err());
}
