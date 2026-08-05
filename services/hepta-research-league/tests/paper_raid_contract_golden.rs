use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::VerifyingKey;
use hepta_research_league::paper_raid_contracts::{
    authorization_set_consumption_receipt_signing_bytes, authorship_consent_signing_bytes,
    canonical_json_sha256, consumer_user_assertion_signing_bytes,
    nakama_completion_receipt_signing_bytes, paper_bundle_frame, paper_bundle_hash,
    paper_raid_evidence_envelope_hash, paper_raid_evidence_envelope_signing_bytes,
    paper_release_candidate_frame, paper_release_candidate_hash, publication_release_frame,
    publication_release_hash, sha256_digest, verify_authorization_set_consumption_receipt,
    verify_authorship_consent_signature, verify_consumer_user_assertion_signature,
    verify_nakama_completion_receipt, verify_paper_raid_evidence_envelope,
    verify_paper_raid_evidence_envelope_against, verify_publication_release_against,
    AuthorshipConsentSigningV2, PaperBundleV2, PaperReleaseCandidateV2, PublicationReleaseV1,
    SignedAuthorizationSetConsumptionReceiptV1, SignedConsumerUserAssertionV2,
    SignedNakamaCompletionReceiptV1, SignedPaperRaidEvidenceEnvelopeV1,
};
use serde::Deserialize;

const FIXTURE: &[u8] = include_bytes!("../../../docs/sdk-fixtures/hepta-paper-raid-v2.json");
const FIXTURE_SHA256: &str =
    "sha256:309584cc21a7169473a7bd37b93528edce4a3b248b313238cd81f6a7c3cad19d";

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
fn rust_verifies_hepta_owned_cross_language_vectors_and_tamper_negatives() {
    assert_eq!(sha256_digest(FIXTURE), FIXTURE_SHA256);
    let fixture: Fixture = serde_json::from_slice(FIXTURE).expect("Paper Raid fixture");
    assert_eq!(fixture.schema, "hepta.paper_raid.golden_vectors.v2");
    assert_eq!(fixture.authorship_consents.len(), 3);
    assert_eq!(fixture.negative_cases.len(), 12);

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

    let release_frame = paper_release_candidate_frame(&fixture.release_candidate.value).unwrap();
    assert_eq!(
        release_frame,
        decode_hex(&fixture.release_candidate.frame_hex)
    );
    assert_eq!(
        paper_release_candidate_hash(&fixture.release_candidate.value).unwrap(),
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
