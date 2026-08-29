use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::VerifyingKey;
use hepta_research_league::paper_raid_contracts::{
    paper_appeal_resolution_signing_bytes, paper_appeal_signing_bytes,
    paper_evaluation_signing_bytes, paper_reproduction_signing_bytes,
    paper_review_attestation_signing_bytes, sha256_digest,
    verify_paper_appeal_resolution_signature, verify_paper_appeal_signature,
    verify_paper_evaluation_signature, verify_paper_reproduction_signature,
    verify_paper_review_attestation_signature, PaperAppealResolutionSigningV1,
    PaperAppealSigningV1, PaperEvaluationSigningV1, PaperReproductionSigningV1,
    PaperReviewAttestationSigningV1,
};
use serde::Deserialize;

const FIXTURE: &[u8] = include_bytes!("../../../docs/sdk-fixtures/hepta-paper-review-v4.json");
const FIXTURE_SHA256: &str =
    "sha256:b25dcbfcf3f9d5830ab8d2b32bdd36b2c073bca8a0bff1da6ba05fb85f6f17b4";

#[derive(Debug, Deserialize)]
struct KeyVector {
    public_key_base64: String,
    public_key_hash: String,
}

#[derive(Debug, Deserialize)]
struct Keys {
    evaluator: KeyVector,
    reviewer: KeyVector,
    reproducer: KeyVector,
    author: KeyVector,
    resolver: KeyVector,
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
    evaluation: SignedVector<PaperEvaluationSigningV1>,
    review_attestation: SignedVector<PaperReviewAttestationSigningV1>,
    reproduction: SignedVector<PaperReproductionSigningV1>,
    appeal: SignedVector<PaperAppealSigningV1>,
    appeal_resolution: SignedVector<PaperAppealResolutionSigningV1>,
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
fn rust_verifies_review_frames_signatures_and_tamper_negatives() {
    assert_eq!(sha256_digest(FIXTURE), FIXTURE_SHA256);
    let fixture: Fixture = serde_json::from_slice(FIXTURE).expect("review fixture");
    assert_eq!(fixture.schema, "hepta.paper_raid.review.golden_vectors.v4");
    assert_eq!(fixture.protocol, "hepta.paper_raid.review.v4");
    assert_eq!(
        fixture.negative_cases,
        [
            "evaluation_score_tamper",
            "review_verdict_tamper",
            "reproduction_seed_tamper",
            "appeal_evidence_tamper",
            "resolution_outcome_tamper",
        ]
    );

    let evaluator_key = key(&fixture.keys.evaluator);
    let reviewer_key = key(&fixture.keys.reviewer);
    let reproducer_key = key(&fixture.keys.reproducer);
    let author_key = key(&fixture.keys.author);
    let resolver_key = key(&fixture.keys.resolver);

    assert_eq!(
        paper_evaluation_signing_bytes(&fixture.evaluation.signing).unwrap(),
        decode_hex(&fixture.evaluation.signing_frame_hex)
    );
    verify_paper_evaluation_signature(
        &fixture.evaluation.signing,
        &fixture.evaluation.signature,
        &evaluator_key,
    )
    .unwrap();

    assert_eq!(
        paper_review_attestation_signing_bytes(&fixture.review_attestation.signing).unwrap(),
        decode_hex(&fixture.review_attestation.signing_frame_hex)
    );
    verify_paper_review_attestation_signature(
        &fixture.review_attestation.signing,
        &fixture.review_attestation.signature,
        &reviewer_key,
    )
    .unwrap();

    assert_eq!(
        paper_reproduction_signing_bytes(&fixture.reproduction.signing).unwrap(),
        decode_hex(&fixture.reproduction.signing_frame_hex)
    );
    verify_paper_reproduction_signature(
        &fixture.reproduction.signing,
        &fixture.reproduction.signature,
        &reproducer_key,
    )
    .unwrap();

    assert_eq!(
        paper_appeal_signing_bytes(&fixture.appeal.signing).unwrap(),
        decode_hex(&fixture.appeal.signing_frame_hex)
    );
    verify_paper_appeal_signature(
        &fixture.appeal.signing,
        &fixture.appeal.signature,
        &author_key,
    )
    .unwrap();

    assert_eq!(
        paper_appeal_resolution_signing_bytes(&fixture.appeal_resolution.signing).unwrap(),
        decode_hex(&fixture.appeal_resolution.signing_frame_hex)
    );
    verify_paper_appeal_resolution_signature(
        &fixture.appeal_resolution.signing,
        &fixture.appeal_resolution.signature,
        &resolver_key,
    )
    .unwrap();

    let mut evaluation_tamper = fixture.evaluation.signing.clone();
    evaluation_tamper.paper_score_hash = sha256_digest(b"tampered");
    assert!(verify_paper_evaluation_signature(
        &evaluation_tamper,
        &fixture.evaluation.signature,
        &evaluator_key,
    )
    .is_err());

    let mut review_tamper = fixture.review_attestation.signing.clone();
    review_tamper.verdict = "reject".to_string();
    assert!(verify_paper_review_attestation_signature(
        &review_tamper,
        &fixture.review_attestation.signature,
        &reviewer_key,
    )
    .is_err());

    let mut reproduction_tamper = fixture.reproduction.signing.clone();
    reproduction_tamper.seed_set_hash = sha256_digest(b"tampered");
    assert!(verify_paper_reproduction_signature(
        &reproduction_tamper,
        &fixture.reproduction.signature,
        &reproducer_key,
    )
    .is_err());

    let mut appeal_tamper = fixture.appeal.signing.clone();
    appeal_tamper.evidence_manifest_hash = sha256_digest(b"tampered");
    assert!(
        verify_paper_appeal_signature(&appeal_tamper, &fixture.appeal.signature, &author_key,)
            .is_err()
    );

    let mut resolution_tamper = fixture.appeal_resolution.signing.clone();
    resolution_tamper.outcome = "upheld".to_string();
    assert!(verify_paper_appeal_resolution_signature(
        &resolution_tamper,
        &fixture.appeal_resolution.signature,
        &resolver_key,
    )
    .is_err());
}
