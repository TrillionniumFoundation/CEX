use std::{fs, path::PathBuf};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::VerifyingKey;
use hepta_research_league::paper_raid_contracts::{
    research_control_claim_frame_v2, research_control_complete_business_v2,
    research_control_create_business_v2, research_control_replace_business_v2,
    research_control_resume_business_v2, research_control_signing_bytes_v2, sha256_digest,
    verify_research_control_v2, ResearchControlCompleteRequestV2, ResearchControlCreateRequestV2,
    ResearchControlOperationV2, ResearchControlReplaceRequestV2, ResearchControlResumeRequestV2,
    SignedResearchControlV2, JSON_SAFE_U64_MAX,
};
use serde::Deserialize;
use serde_json::Value;

const FIXTURE_BYTES: &[u8] = include_bytes!(
    "../../../docs/sdk-fixtures/trnm-nakama-research-control-golden-vectors-v2.json"
);
const FIXTURE_SHA256: &str =
    "sha256:65f7869261f452dadbba9228dccafcba2fe4e6fa12875f96628d0395021197ea";

#[derive(Debug, Deserialize)]
struct KeyVector {
    public_key_base64: String,
}

#[derive(Debug, Deserialize)]
struct Vector {
    operation: ResearchControlOperationV2,
    target_rpc: String,
    business_frame_base64: String,
    payload_hash: String,
    control_signing_frame_base64: String,
    canonical_request_body_base64: String,
    request: Value,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    schema: String,
    keys: std::collections::HashMap<String, KeyVector>,
    vectors: Vec<Vector>,
}

fn verifying_key(vector: &KeyVector) -> VerifyingKey {
    let bytes: [u8; 32] = BASE64
        .decode(&vector.public_key_base64)
        .expect("fixture public key base64")
        .try_into()
        .expect("fixture public key length");
    VerifyingKey::from_bytes(&bytes).expect("fixture Ed25519 public key")
}

fn decode_base64(value: &str) -> Vec<u8> {
    BASE64.decode(value).expect("fixture base64")
}

fn verify_vector(vector: &Vector, key: &VerifyingKey) -> SignedResearchControlV2 {
    let (business, control, request_bytes) = match vector.operation {
        ResearchControlOperationV2::Create => {
            let request: ResearchControlCreateRequestV2 =
                serde_json::from_value(vector.request.clone()).expect("create request");
            (
                research_control_create_business_v2(&request).expect("create business frame"),
                request.control.clone(),
                serde_json::to_vec(&request).expect("create request JSON"),
            )
        }
        ResearchControlOperationV2::Resume => {
            let request: ResearchControlResumeRequestV2 =
                serde_json::from_value(vector.request.clone()).expect("resume request");
            (
                research_control_resume_business_v2(&request).expect("resume business frame"),
                request.control.clone(),
                serde_json::to_vec(&request).expect("resume request JSON"),
            )
        }
        ResearchControlOperationV2::ReplaceRoster => {
            let request: ResearchControlReplaceRequestV2 =
                serde_json::from_value(vector.request.clone()).expect("replace request");
            (
                research_control_replace_business_v2(&request).expect("replace business frame"),
                request.control.clone(),
                serde_json::to_vec(&request).expect("replace request JSON"),
            )
        }
        ResearchControlOperationV2::Complete => {
            let request: ResearchControlCompleteRequestV2 =
                serde_json::from_value(vector.request.clone()).expect("complete request");
            (
                research_control_complete_business_v2(&request).expect("complete business frame"),
                request.control.clone(),
                serde_json::to_vec(&request).expect("complete request JSON"),
            )
        }
    };
    assert_eq!(business, decode_base64(&vector.business_frame_base64));
    assert_eq!(sha256_digest(&business), vector.payload_hash);
    assert_eq!(control.claim.payload_hash, vector.payload_hash);
    assert_eq!(control.claim.operation, vector.operation);
    assert_eq!(control.claim.target_rpc, vector.target_rpc);
    assert_eq!(control.claim.target_rpc, vector.operation.target_rpc());
    assert_eq!(
        research_control_signing_bytes_v2(&control.claim).expect("control signing frame"),
        decode_base64(&vector.control_signing_frame_base64)
    );
    assert_eq!(
        request_bytes,
        decode_base64(&vector.canonical_request_body_base64)
    );
    verify_research_control_v2(&control, key).expect("control signature");
    control
}

#[test]
fn rust_matches_nakama_signed_control_v2_for_all_four_operations() {
    assert_eq!(sha256_digest(FIXTURE_BYTES), FIXTURE_SHA256);
    let fixture: Fixture = serde_json::from_slice(FIXTURE_BYTES).expect("control fixture");
    assert_eq!(
        fixture.schema,
        "trnm.nakama.research_control.golden_vectors.v2"
    );
    assert_eq!(fixture.vectors.len(), 4);
    assert_ne!(
        fixture.keys["control_issuer"].public_key_base64,
        fixture.keys["authorization_issuer"].public_key_base64
    );
    let control_key = verifying_key(&fixture.keys["control_issuer"]);
    let controls = fixture
        .vectors
        .iter()
        .map(|vector| verify_vector(vector, &control_key))
        .collect::<Vec<_>>();

    let mut payload_tamper = controls[0].clone();
    payload_tamper.claim.payload_hash = sha256_digest(b"tampered business frame");
    assert!(verify_research_control_v2(&payload_tamper, &control_key).is_err());

    let mut operation_tamper = controls[0].clone();
    operation_tamper.claim.operation = ResearchControlOperationV2::Resume;
    assert!(research_control_claim_frame_v2(&operation_tamper.claim).is_err());

    let mut version_overflow = controls[0].clone();
    version_overflow.claim.session_roster_version = JSON_SAFE_U64_MAX + 1;
    assert!(research_control_claim_frame_v2(&version_overflow.claim).is_err());
}

#[test]
fn vendored_control_contract_is_byte_identical_when_nakama_sibling_is_present() {
    let canonical = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../trillionnium-nakama/contracts/research-control-golden-vectors.json");
    if canonical.exists() {
        assert_eq!(
            fs::read(canonical).expect("read Nakama control fixture"),
            FIXTURE_BYTES
        );
    }
}
