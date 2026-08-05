use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::VerifyingKey;
use hepta_research_league::paper_raid_contracts::{
    agent_proposal_signing_bytes, human_decision_signing_bytes,
    human_evidence_verification_signing_bytes, section_merge_signing_bytes,
    section_review_signing_bytes, sha256_digest, verify_agent_proposal_signature,
    verify_human_decision_signature, verify_human_evidence_verification_signature,
    verify_section_merge_signature, verify_section_review_signature, AgentProposalSigningV1,
    HumanDecisionSigningV1, HumanEvidenceVerificationSigningV1, SectionMergeSigningV1,
    SectionReviewSigningV1,
};
use serde::Deserialize;

const FIXTURE: &[u8] =
    include_bytes!("../../../docs/sdk-fixtures/hepta-paper-collaboration-v3.json");
const FIXTURE_SHA256: &str =
    "sha256:6a8c20dabaf2ff723a1db7e9742bcbd24f4d18bb17938f3695cac099c29d84ce";

#[derive(Debug, Deserialize)]
struct KeyVector {
    public_key_base64: String,
    public_key_hash: String,
}

#[derive(Debug, Deserialize)]
struct Keys {
    agent: KeyVector,
    human: KeyVector,
    reviewer: KeyVector,
}

#[derive(Debug, Deserialize)]
struct SignedVector<T> {
    signing: T,
    signing_frame_hex: String,
    signature: String,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    schema: String,
    protocol: String,
    keys: Keys,
    agent_proposal: SignedVector<AgentProposalSigningV1>,
    human_decision: SignedVector<HumanDecisionSigningV1>,
    human_evidence_verification: SignedVector<HumanEvidenceVerificationSigningV1>,
    section_review: SignedVector<SectionReviewSigningV1>,
    section_merge: SignedVector<SectionMergeSigningV1>,
    negative_cases: Vec<String>,
}

fn decode_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0, "fixture hex must be even length");
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).expect("fixture hex"))
        .collect()
}

fn key(vector: &KeyVector) -> VerifyingKey {
    let public: [u8; 32] = BASE64
        .decode(&vector.public_key_base64)
        .expect("public key base64")
        .try_into()
        .expect("public key length");
    assert_eq!(sha256_digest(&public), vector.public_key_hash);
    VerifyingKey::from_bytes(&public).expect("Ed25519 public key")
}

#[test]
fn rust_verifies_collaboration_frames_signatures_and_tamper_negatives() {
    assert_eq!(sha256_digest(FIXTURE), FIXTURE_SHA256);
    let fixture: Fixture = serde_json::from_slice(FIXTURE).expect("collaboration fixture");
    assert_eq!(
        fixture.schema,
        "hepta.paper_raid.collaboration.golden_vectors.v3"
    );
    assert_eq!(fixture.protocol, "hepta.paper_raid.collaboration.v3");
    assert_eq!(fixture.negative_cases.len(), 5);

    let agent_key = key(&fixture.keys.agent);
    let human_key = key(&fixture.keys.human);
    let reviewer_key = key(&fixture.keys.reviewer);

    assert_eq!(
        agent_proposal_signing_bytes(&fixture.agent_proposal.signing).unwrap(),
        decode_hex(&fixture.agent_proposal.signing_frame_hex)
    );
    verify_agent_proposal_signature(
        &fixture.agent_proposal.signing,
        &fixture.agent_proposal.signature,
        &agent_key,
    )
    .unwrap();

    assert_eq!(
        human_decision_signing_bytes(&fixture.human_decision.signing).unwrap(),
        decode_hex(&fixture.human_decision.signing_frame_hex)
    );
    verify_human_decision_signature(
        &fixture.human_decision.signing,
        &fixture.human_decision.signature,
        &human_key,
    )
    .unwrap();

    assert_eq!(
        human_evidence_verification_signing_bytes(&fixture.human_evidence_verification.signing)
            .unwrap(),
        decode_hex(&fixture.human_evidence_verification.signing_frame_hex)
    );
    verify_human_evidence_verification_signature(
        &fixture.human_evidence_verification.signing,
        &fixture.human_evidence_verification.signature,
        &human_key,
    )
    .unwrap();

    assert_eq!(
        section_review_signing_bytes(&fixture.section_review.signing).unwrap(),
        decode_hex(&fixture.section_review.signing_frame_hex)
    );
    verify_section_review_signature(
        &fixture.section_review.signing,
        &fixture.section_review.signature,
        &reviewer_key,
    )
    .unwrap();

    assert_eq!(
        section_merge_signing_bytes(&fixture.section_merge.signing).unwrap(),
        decode_hex(&fixture.section_merge.signing_frame_hex)
    );
    verify_section_merge_signature(
        &fixture.section_merge.signing,
        &fixture.section_merge.signature,
        &human_key,
    )
    .unwrap();

    let mut agent_tamper = fixture.agent_proposal.signing.clone();
    agent_tamper.payload_hash = sha256_digest(b"tampered");
    assert!(verify_agent_proposal_signature(
        &agent_tamper,
        &fixture.agent_proposal.signature,
        &agent_key
    )
    .is_err());

    let mut decision_tamper = fixture.human_decision.signing.clone();
    decision_tamper.reason_hash = sha256_digest(b"tampered");
    assert!(verify_human_decision_signature(
        &decision_tamper,
        &fixture.human_decision.signature,
        &human_key
    )
    .is_err());

    let mut evidence_tamper = fixture.human_evidence_verification.signing.clone();
    evidence_tamper.source_identifier.push_str("?tampered");
    assert!(verify_human_evidence_verification_signature(
        &evidence_tamper,
        &fixture.human_evidence_verification.signature,
        &human_key
    )
    .is_err());

    let mut review_tamper = fixture.section_review.signing.clone();
    review_tamper.verdict = "reject".to_string();
    assert!(verify_section_review_signature(
        &review_tamper,
        &fixture.section_review.signature,
        &reviewer_key
    )
    .is_err());

    let mut merge_tamper = fixture.section_merge.signing.clone();
    merge_tamper.fencing_token += 1;
    assert!(verify_section_merge_signature(
        &merge_tamper,
        &fixture.section_merge.signature,
        &human_key
    )
    .is_err());
}
