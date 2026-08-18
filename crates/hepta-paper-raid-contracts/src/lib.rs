//! Frozen, language-neutral contracts used by Paper Raid.
//!
//! The `trnm_research_session_*_v1` frames are shared with the authoritative
//! Nakama research-session runtime. They deliberately do not widen or alter
//! any legacy `trnm.match.*.v1` contract.

use std::collections::{BTreeMap, HashMap, HashSet};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const PAPER_RAID_PROTOCOL_V2: &str = "hepta.paper_raid.v2";
pub const JSON_SAFE_U64_MAX: u64 = 9_007_199_254_740_991;
pub const PAPER_RELEASE_CANDIDATE_V2: &str = "hepta.paper_raid.release_candidate.v2";
pub const SECTION_MATERIALIZATION_V1: &str = "hepta.paper_raid.section_materialization.v1";
pub const PAPER_BUNDLE_V2: &str = "hepta.paper_raid.paper_bundle.v2";
pub const PAPER_RAID_EVIDENCE_ENVELOPE_V1: &str = "hepta.paper_raid.evidence_envelope.v1";
pub const PUBLICATION_RELEASE_V1: &str = "hepta.paper_raid.publication_release.v1";
pub const AUTHORSHIP_CONSENT_V2: &str = "hepta.paper_raid.authorship_consent.v2";
pub const TEAM_MEMBER_ACCEPTANCE_V2: &str = "hepta.paper_raid.team_member_acceptance.v2";
pub const NAKAMA_COMPLETION_RECEIPT_V1: &str = "hepta.paper_raid.nakama_completion_receipt.v1";
pub const AUTHORIZATION_SET_CONSUMPTION_RECEIPT_V1: &str =
    "hepta.paper_raid.authorization_set_consumption_receipt.v1";
pub const HUMAN_KEY_REGISTRATION_V2: &str = "hepta.paper_raid.human_key_registration.v2";
pub const AGENT_BINDING_PROOF_V2: &str = "hepta.paper_raid.agent_binding_proof.v2";
pub const AGENT_BINDING_PROOF_V3: &str = "hepta.paper_raid.agent_binding_proof.v3";
pub const AGENT_CAPABILITY_DISCLOSURE_V1: &str = "hepta.paper_raid.agent_capability_disclosure.v1";
pub const AGENT_BRIDGE_REQUEST_PROOF_V1: &str = "hepta.paper_raid.agent_bridge_request_proof.v1";
pub const AGENT_BRIDGE_REQUEST_PROOF_MAX_LIFETIME_SECONDS: i64 = 60;
pub const AGENT_BINDING_KEY_ROTATION_V2: &str = "hepta.paper_raid.agent_binding_key_rotation.v2";
pub const HUMAN_KEY_ROTATION_V2: &str = "hepta.paper_raid.human_key_rotation.v2";
pub const HUMAN_KEY_REVOCATION_V2: &str = "hepta.paper_raid.human_key_revocation.v2";
pub const CONSUMER_USER_ASSERTION_V2: &str = "hepta.consumer-edge.user-assertion.v2";
pub const AGENT_PROPOSAL_V1: &str = "hepta.paper_raid.agent_proposal.v1";
pub const AGENT_PROPOSAL_V2: &str = "hepta.paper_raid.agent_proposal.v2";
pub const HUMAN_DECISION_V1: &str = "hepta.paper_raid.human_decision.v1";
pub const HUMAN_EVIDENCE_VERIFICATION_V1: &str = "hepta.paper_raid.human_evidence_verification.v1";
pub const SECTION_REVIEW_V1: &str = "hepta.paper_raid.section_review.v1";
pub const SECTION_MERGE_V1: &str = "hepta.paper_raid.section_merge.v1";
pub const PAPER_EVALUATION_V1: &str = "hepta.paper_raid.evaluation.v1";
pub const PAPER_REVIEW_ATTESTATION_V1: &str = "hepta.paper_raid.review_attestation.v1";
pub const PAPER_REPRODUCTION_V1: &str = "hepta.paper_raid.reproduction.v1";
pub const PAPER_APPEAL_V1: &str = "hepta.paper_raid.appeal.v1";
pub const PAPER_APPEAL_RESOLUTION_V1: &str = "hepta.paper_raid.appeal_resolution.v1";
pub const PAPER_REWORK_V1: &str = "hepta.paper_raid.rework.v1";
pub const FROZEN_REVIEW_AUTHORITY_V1: &str = "hepta.paper_raid.frozen_review_authority.v1";
pub const RESOLVED_FROZEN_REVIEW_BUNDLE_V1: &str =
    "hepta.paper_raid.resolved_frozen_review_bundle.v1";
pub const FROZEN_REVIEW_BUNDLE_V1: &str = RESOLVED_FROZEN_REVIEW_BUNDLE_V1;
pub const REVIEW_EXECUTION_RECEIPT_V1: &str = "hepta.paper_raid.review_execution_receipt.v1";
pub const REVIEW_EXECUTION_RECEIPT_ID_DOMAIN_V1: &str =
    "hepta.paper_raid.review_execution_receipt_id.v1";
pub const REVIEW_OBJECT_DOWNLOAD_PATH_V1: &str = "/api/agent-bridge/review-objects";
pub const FROZEN_CHALLENGE_MATERIAL_AUTHORITY_V1: &str =
    "hepta.paper_raid.frozen_challenge_material_authority.v1";
pub const LEGACY_GOLDEN_QUALIFICATION_MATERIAL_AUTHORITY_V1: &str =
    "hepta.paper_raid.legacy_golden_qualification_material_authority.v1";
pub const ASSIGNED_CHALLENGE_MATERIAL_BUNDLE_V1: &str =
    "hepta.paper_raid.assigned_challenge_material_bundle.v1";
pub const CHALLENGE_MATERIAL_OBJECT_DOWNLOAD_PATH_V1: &str = "/api/agent-bridge/challenge-objects";
pub const LEGACY_GOLDEN_QUALIFICATION_ID: &str = "paper-raid-golden-v2-strict-review-v1";
pub const LEGACY_GOLDEN_CHALLENGE_TITLE: &str = "Paper Raid: Reproducible Synthetic Ablation";
pub const LEGACY_GOLDEN_CHALLENGE_DESCRIPTION: &str = "Reproduce a public deterministic baseline, retain the failed run, and deliver one evidence-bound ablation as a short paper. paper-raid-alpha-evaluator-sha256:805ee4ad69fa1e56cebc0721711419d211cf0fe2090e294db25bf712f10c74f8";
pub const LEGACY_GOLDEN_CHALLENGE_RULESET_VERSION: &str = "paper-raid-golden-v2";
pub const LEGACY_GOLDEN_CHALLENGE_RULESET_HASH: &str =
    "sha256:39cfc6a5c883e49b78bf315b53079336539c932ca48ff5a040415d7e5dd9b2c0";
pub const LEGACY_GOLDEN_DATASET_MANIFEST_HASH: &str =
    "sha256:6c0494ec10383018b4a938528d179d16fcb6dd8961b8294ff6f4093dda42aeb4";
pub const LEGACY_GOLDEN_EVALUATOR_MANIFEST_HASH: &str =
    "sha256:805ee4ad69fa1e56cebc0721711419d211cf0fe2090e294db25bf712f10c74f8";
pub const LEGACY_GOLDEN_QUALIFICATION_BRIEF_BYTES: &[u8] =
    include_bytes!("../assets/legacy-golden-qualification-brief.md");
pub const LEGACY_GOLDEN_QUALIFICATION_BRIEF_DIGEST: &str =
    "sha256:b926b4c868af652b2fed4671efc07f9bbc021c65ac188c971c2a2b67c365a9a3";
pub const LEGACY_GOLDEN_QUALIFICATION_BRIEF_SIZE: u64 = 913;
pub const LEGACY_GOLDEN_DATASET_DIGEST: &str =
    "sha256:b002e6297f6fd781742866533b89bf781c7f21d5cfa7b42b5c97a9ecd5821314";
pub const LEGACY_GOLDEN_DATASET_SIZE: u64 = 230;
pub const LEGACY_GOLDEN_BASELINE_DIGEST: &str =
    "sha256:059717cc82d10cee0504ed6af3fa81121d7a7645c53d8e8a788a35f63ae644ba";
pub const LEGACY_GOLDEN_BASELINE_SIZE: u64 = 1_071;
pub const LEGACY_GOLDEN_EVALUATOR_DIGEST: &str =
    "sha256:63971194ab97e1d14752795ff1ff8c39a44d1a7782ffbb9459ec2210fe8a8e3d";
pub const LEGACY_GOLDEN_EVALUATOR_SIZE: u64 = 3_483;

pub const RESEARCH_SESSION_AUTHORIZATION_V1: &str = "trnm.research-session.authorization.v1";
pub const RESEARCH_SESSION_ACTION_V1: &str = "trnm.research-session.action.v1";
pub const RESEARCH_SESSION_EVENT_V1: &str = "trnm.research-session.event.v1";
pub const RESEARCH_SESSION_COMPLETION_V1: &str = "trnm.research-session.completed.v1";
pub const RESEARCH_CONTROL_CLAIM_V2: &str = "trnm.nakama.research-control.claim.v2";
pub const RESEARCH_CONTROL_AUDIENCE_V2: &str = "trnm:nakama:research-control:v2";
pub const RESEARCH_CONTROL_CREATE_REQUEST_V2: &str = "trnm.nakama.research-session.create.v2";
pub const RESEARCH_CONTROL_RESUME_REQUEST_V2: &str = "trnm.nakama.research-session.resume.v2";
pub const RESEARCH_CONTROL_REPLACE_REQUEST_V2: &str =
    "trnm.nakama.research-session.replace-roster.v2";
pub const RESEARCH_CONTROL_COMPLETE_REQUEST_V2: &str = "trnm.nakama.research-session.complete.v2";
pub const RESEARCH_CONTROL_RESULT_V2: &str = "trnm.nakama.research-control.result.v2";
pub const RESEARCH_CONTROL_RPC_CREATE_V2: &str = "trnm_research_session_create_v2";
pub const RESEARCH_CONTROL_RPC_RESUME_V2: &str = "trnm_research_session_resume_v2";
pub const RESEARCH_CONTROL_RPC_REPLACE_V2: &str = "trnm_research_session_replace_roster_v2";
pub const RESEARCH_CONTROL_RPC_COMPLETE_V2: &str = "trnm_research_session_complete_v2";
pub const RESEARCH_CONTROL_MAXIMUM_LIFETIME_SECONDS: i64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CanonicalFrame {
    bytes: Vec<u8>,
}

impl CanonicalFrame {
    fn new(domain: &str) -> Self {
        let mut bytes = Vec::with_capacity(domain.len() + 1);
        bytes.extend_from_slice(domain.as_bytes());
        bytes.push(0);
        Self { bytes }
    }

    fn string(self, value: &str) -> Result<Self, String> {
        self.bytes(value.as_bytes())
    }

    fn bytes(mut self, value: &[u8]) -> Result<Self, String> {
        let length = u32::try_from(value.len())
            .map_err(|_| "canonical field exceeds uint32 length".to_string())?;
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(value);
        Ok(self)
    }

    fn u32(mut self, value: u32) -> Self {
        self.bytes.extend_from_slice(&value.to_be_bytes());
        self
    }

    fn u64(mut self, value: u64) -> Self {
        self.bytes.extend_from_slice(&value.to_be_bytes());
        self
    }

    fn i64(mut self, value: i64) -> Self {
        self.bytes.extend_from_slice(&value.to_be_bytes());
        self
    }

    fn digest(mut self, value: &str) -> Result<Self, String> {
        self.bytes.extend_from_slice(&decode_digest(value)?);
        Ok(self)
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

fn validate_text(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{field} is required"));
    }
    if value.chars().count() > 512 {
        return Err(format!("{field} exceeds 512 Unicode code points"));
    }
    if value.as_bytes().contains(&0) {
        return Err(format!("{field} contains a NUL byte"));
    }
    Ok(())
}

fn validate_logical_id(field: &str, value: &str) -> Result<(), String> {
    validate_text(field, value)?;
    let mut bytes = value.bytes();
    let first = bytes.next().ok_or_else(|| format!("{field} is required"))?;
    if value.len() > 128
        || !first.is_ascii_alphanumeric()
        || !bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(format!(
            "{field} must match [A-Za-z0-9][A-Za-z0-9._:-]{{0,127}}"
        ));
    }
    Ok(())
}

fn validate_key_id(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(format!(
            "{field} must contain 1-128 ASCII alphanumeric, dot, underscore, colon, or dash bytes"
        ));
    }
    Ok(())
}

pub fn decode_digest(value: &str) -> Result<[u8; 32], String> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| "digest must start with sha256:".to_string())?;
    if hex.len() != 64
        || hex.bytes().any(|byte| {
            !byte.is_ascii_hexdigit() || (byte.is_ascii_alphabetic() && !byte.is_ascii_lowercase())
        })
    {
        return Err("digest must contain 64 lowercase hexadecimal characters".to_string());
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| "digest contains invalid hexadecimal".to_string())?;
    }
    Ok(output)
}

fn decode_base64_exact<const N: usize>(field: &str, value: &str) -> Result<[u8; N], String> {
    let decoded = BASE64
        .decode(value)
        .map_err(|_| format!("{field} must be canonical padded base64"))?;
    if BASE64.encode(&decoded) != value {
        return Err(format!("{field} must be canonical padded base64"));
    }
    decoded
        .try_into()
        .map_err(|_| format!("{field} must decode to exactly {N} bytes"))
}

fn decode_base64(field: &str, value: &str) -> Result<Vec<u8>, String> {
    let decoded = BASE64
        .decode(value)
        .map_err(|_| format!("{field} must be canonical padded base64"))?;
    if BASE64.encode(&decoded) != value {
        return Err(format!("{field} must be canonical padded base64"));
    }
    Ok(decoded)
}

pub fn sha256_digest(bytes: &[u8]) -> String {
    let hex: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("sha256:{hex}")
}

/// Canonical JSON used by Consumer Edge assertions and SDK fixtures.
///
/// Paper Raid request contracts contain integers, strings, booleans, nulls,
/// arrays, and objects (no floating-point values). Object keys are sorted by
/// UTF-8/Unicode scalar order through `serde_json::Map` before compact JSON
/// encoding; arrays retain their contract order.
pub fn canonical_json_bytes<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, String> {
    fn sort_value(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sort_value).collect())
            }
            serde_json::Value::Object(values) => {
                let mut entries: Vec<_> = values.into_iter().collect();
                entries.sort_by(|left, right| left.0.cmp(&right.0));
                serde_json::Value::Object(
                    entries
                        .into_iter()
                        .map(|(key, value)| (key, sort_value(value)))
                        .collect(),
                )
            }
            primitive => primitive,
        }
    }
    let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
    serde_json::to_vec(&sort_value(value)).map_err(|error| error.to_string())
}

pub fn canonical_json_sha256<T: Serialize + ?Sized>(value: &T) -> Result<String, String> {
    canonical_json_bytes(value).map(|bytes| sha256_digest(&bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConsumerUserAssertionClaimV2 {
    pub schema: String,
    pub assertion_id: Uuid,
    pub issuer: String,
    pub audience: String,
    pub subject_id: String,
    pub nakama_user_id: Uuid,
    pub player_id: Uuid,
    pub operation: String,
    pub http_method: String,
    pub canonical_path: String,
    pub idempotency_key: String,
    pub body_hash: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedConsumerUserAssertionV2 {
    pub claim: ConsumerUserAssertionClaimV2,
    pub issuer_key_id: String,
    pub signature: String,
}

pub fn consumer_user_assertion_signing_bytes(
    assertion: &ConsumerUserAssertionClaimV2,
    issuer_key_id: &str,
) -> Result<Vec<u8>, String> {
    if assertion.schema != CONSUMER_USER_ASSERTION_V2 {
        return Err(format!(
            "unsupported user assertion schema {}",
            assertion.schema
        ));
    }
    for (field, value) in [
        ("issuer", assertion.issuer.as_str()),
        ("audience", assertion.audience.as_str()),
        ("subject_id", assertion.subject_id.as_str()),
        ("operation", assertion.operation.as_str()),
        ("http_method", assertion.http_method.as_str()),
        ("canonical_path", assertion.canonical_path.as_str()),
        ("idempotency_key", assertion.idempotency_key.as_str()),
        ("nonce", assertion.nonce.as_str()),
        ("issuer_key_id", issuer_key_id),
    ] {
        validate_text(field, value)?;
    }
    if assertion.nonce != assertion.idempotency_key {
        return Err("assertion nonce must equal the idempotency key".to_string());
    }
    if assertion.issued_at_unix < 0 || assertion.expires_at_unix <= assertion.issued_at_unix {
        return Err("user assertion validity interval is invalid".to_string());
    }
    Ok(CanonicalFrame::new("hepta_consumer_edge_user_assertion_v2")
        .string(&assertion.schema)?
        .string(&assertion.assertion_id.to_string())?
        .string(&assertion.issuer)?
        .string(&assertion.audience)?
        .string(&assertion.subject_id)?
        .string(&assertion.nakama_user_id.to_string())?
        .string(&assertion.player_id.to_string())?
        .string(&assertion.operation)?
        .string(&assertion.http_method)?
        .string(&assertion.canonical_path)?
        .string(&assertion.idempotency_key)?
        .digest(&assertion.body_hash)?
        .i64(assertion.issued_at_unix)
        .i64(assertion.expires_at_unix)
        .string(&assertion.nonce)?
        .string(issuer_key_id)?
        .finish())
}

pub fn sign_consumer_user_assertion(
    claim: ConsumerUserAssertionClaimV2,
    issuer_key_id: &str,
    signing_key: &SigningKey,
) -> Result<SignedConsumerUserAssertionV2, String> {
    let bytes = consumer_user_assertion_signing_bytes(&claim, issuer_key_id)?;
    Ok(SignedConsumerUserAssertionV2 {
        claim,
        issuer_key_id: issuer_key_id.to_string(),
        signature: BASE64.encode(signing_key.sign(&bytes).to_bytes()),
    })
}

pub fn verify_consumer_user_assertion_signature(
    assertion: &SignedConsumerUserAssertionV2,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let bytes = consumer_user_assertion_signing_bytes(&assertion.claim, &assertion.issuer_key_id)?;
    let signature = decode_base64_exact::<64>("signature", &assertion.signature)?;
    verifying_key
        .verify(&bytes, &Signature::from_bytes(&signature))
        .map_err(|_| "Consumer Edge assertion signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HumanKeyRegistrationClaimV2 {
    pub schema: String,
    pub player_id: Uuid,
    pub subject_id: String,
    pub nakama_user_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub nonce: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
}

pub fn human_key_registration_signing_bytes(
    claim: &HumanKeyRegistrationClaimV2,
) -> Result<Vec<u8>, String> {
    if claim.schema != HUMAN_KEY_REGISTRATION_V2 {
        return Err(format!(
            "unsupported human key registration schema {}",
            claim.schema
        ));
    }
    for (field, value) in [
        ("subject_id", claim.subject_id.as_str()),
        ("signing_key_id", claim.signing_key_id.as_str()),
        ("nonce", claim.nonce.as_str()),
    ] {
        validate_text(field, value)?;
    }
    if claim.issued_at_unix < 0 || claim.expires_at_unix <= claim.issued_at_unix {
        return Err("human key registration validity interval is invalid".to_string());
    }
    let key = decode_base64_exact::<32>("signing_public_key", &claim.signing_public_key)?;
    if sha256_digest(&key) != claim.signing_public_key_hash {
        return Err("signing_public_key_hash does not match key".to_string());
    }
    Ok(
        CanonicalFrame::new("hepta_paper_raid_human_key_registration_v2")
            .string(&claim.schema)?
            .string(&claim.player_id.to_string())?
            .string(&claim.subject_id)?
            .string(&claim.nakama_user_id.to_string())?
            .string(&claim.signing_key_id)?
            .bytes(&key)?
            .digest(&claim.signing_public_key_hash)?
            .string(&claim.nonce)?
            .i64(claim.issued_at_unix)
            .i64(claim.expires_at_unix)
            .finish(),
    )
}

pub fn verify_human_key_registration_pop(
    claim: &HumanKeyRegistrationClaimV2,
    signature: &str,
) -> Result<(), String> {
    let message = human_key_registration_signing_bytes(claim)?;
    let key = decode_base64_exact::<32>("signing_public_key", &claim.signing_public_key)?;
    let key = VerifyingKey::from_bytes(&key)
        .map_err(|_| "human signing public key is not valid Ed25519".to_string())?;
    let signature = decode_base64_exact::<64>("proof_signature", signature)?;
    key.verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| "human signing-key proof-of-possession failed".to_string())
}

/// Agent-owned proof that binds one external Ed25519 key to the exact human
/// subject and Paper Raid player selected by Consumer Edge. Legacy v1
/// `owner_id` registration is intentionally absent from this frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentBindingProofClaimV2 {
    pub schema: String,
    pub binding_id: Uuid,
    pub agent_id: String,
    pub agent_key_id: String,
    pub agent_public_key: String,
    pub agent_public_key_hash: String,
    pub subject_id: String,
    pub player_id: Uuid,
    pub nonce: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
}

pub fn agent_binding_proof_signing_bytes(
    claim: &AgentBindingProofClaimV2,
) -> Result<Vec<u8>, String> {
    if claim.schema != AGENT_BINDING_PROOF_V2 {
        return Err(format!(
            "unsupported Agent binding proof schema {}",
            claim.schema
        ));
    }
    for (field, value) in [
        ("agent_id", claim.agent_id.as_str()),
        ("agent_key_id", claim.agent_key_id.as_str()),
        ("subject_id", claim.subject_id.as_str()),
        ("nonce", claim.nonce.as_str()),
    ] {
        validate_text(field, value)?;
    }
    if claim.issued_at_unix < 0 || claim.expires_at_unix <= claim.issued_at_unix {
        return Err("Agent binding proof validity interval is invalid".to_string());
    }
    let key = decode_base64_exact::<32>("agent_public_key", &claim.agent_public_key)?;
    if sha256_digest(&key) != claim.agent_public_key_hash {
        return Err("agent_public_key_hash does not match key".to_string());
    }
    if claim.agent_key_id != claim.agent_public_key_hash {
        return Err("agent_key_id must equal agent_public_key_hash".to_string());
    }
    Ok(
        CanonicalFrame::new("hepta_paper_raid_agent_binding_proof_v2")
            .string(&claim.schema)?
            .string(&claim.binding_id.to_string())?
            .string(&claim.agent_id)?
            .string(&claim.agent_key_id)?
            .bytes(&key)?
            .digest(&claim.agent_public_key_hash)?
            .string(&claim.subject_id)?
            .string(&claim.player_id.to_string())?
            .string(&claim.nonce)?
            .i64(claim.issued_at_unix)
            .i64(claim.expires_at_unix)
            .finish(),
    )
}

pub fn verify_agent_binding_proof(
    claim: &AgentBindingProofClaimV2,
    signature: &str,
) -> Result<(), String> {
    let message = agent_binding_proof_signing_bytes(claim)?;
    let key = decode_base64_exact::<32>("agent_public_key", &claim.agent_public_key)?;
    let key = VerifyingKey::from_bytes(&key)
        .map_err(|_| "Agent public key is not valid Ed25519".to_string())?;
    let signature = decode_base64_exact::<64>("agent_proof_signature", signature)?;
    key.verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| "Agent binding proof-of-possession failed".to_string())
}

/// A bounded, Agent-signed statement of supported task classes.  It is a
/// self-declaration, not an attestation by Hepta and never grants authority or
/// scientific, score, ranking, reward, or economic eligibility.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentCapabilityDisclosureAssuranceV1 {
    SelfDeclaredUnverified,
}

impl AgentCapabilityDisclosureAssuranceV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SelfDeclaredUnverified => "self_declared_unverified",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentCapabilityV1 {
    ArtifactAnalysis,
    CitationVerification,
    EvidenceSearch,
    ExperimentExecution,
    ExperimentPlanning,
    Reproduction,
    ResearchSessionSigning,
    SectionDrafting,
}

impl AgentCapabilityV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ArtifactAnalysis => "artifact_analysis",
            Self::CitationVerification => "citation_verification",
            Self::EvidenceSearch => "evidence_search",
            Self::ExperimentExecution => "experiment_execution",
            Self::ExperimentPlanning => "experiment_planning",
            Self::Reproduction => "reproduction",
            Self::ResearchSessionSigning => "research_session_signing",
            Self::SectionDrafting => "section_drafting",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentResourceClassV1 {
    ArtifactIo,
    Browser,
    CodeExecution,
    Cpu,
    Gpu,
    Network,
    Sandbox,
}

impl AgentResourceClassV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ArtifactIo => "artifact_io",
            Self::Browser => "browser",
            Self::CodeExecution => "code_execution",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Network => "network",
            Self::Sandbox => "sandbox",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentCapabilityDisclosureV1 {
    pub schema: String,
    pub assurance: AgentCapabilityDisclosureAssuranceV1,
    pub capabilities: Vec<AgentCapabilityV1>,
    pub resource_classes: Vec<AgentResourceClassV1>,
    pub max_parallel_tasks: u32,
}

pub fn validate_agent_capability_disclosure(
    disclosure: &AgentCapabilityDisclosureV1,
) -> Result<(), String> {
    if disclosure.schema != AGENT_CAPABILITY_DISCLOSURE_V1 {
        return Err(format!(
            "unsupported Agent capability disclosure schema {}",
            disclosure.schema
        ));
    }
    if disclosure.capabilities.is_empty() || disclosure.capabilities.len() > 16 {
        return Err("Agent capability disclosure must contain 1 to 16 capabilities".to_string());
    }
    if disclosure.resource_classes.len() > 16 {
        return Err(
            "Agent capability disclosure must contain at most 16 resource classes".to_string(),
        );
    }
    if !(1..=32).contains(&disclosure.max_parallel_tasks) {
        return Err("max_parallel_tasks must be between 1 and 32".to_string());
    }
    if disclosure
        .capabilities
        .windows(2)
        .any(|pair| pair[0].as_str() >= pair[1].as_str())
    {
        return Err("Agent capabilities must be lexicographically sorted and unique".to_string());
    }
    if disclosure
        .resource_classes
        .windows(2)
        .any(|pair| pair[0].as_str() >= pair[1].as_str())
    {
        return Err(
            "Agent resource classes must be lexicographically sorted and unique".to_string(),
        );
    }
    Ok(())
}

/// Frozen language-neutral frame for one bounded self-declaration.
pub fn agent_capability_disclosure_frame(
    disclosure: &AgentCapabilityDisclosureV1,
) -> Result<Vec<u8>, String> {
    validate_agent_capability_disclosure(disclosure)?;
    let mut frame = CanonicalFrame::new("hepta_paper_raid_agent_capability_disclosure_v1")
        .string(&disclosure.schema)?
        .string(disclosure.assurance.as_str())?
        .u32(
            u32::try_from(disclosure.capabilities.len())
                .map_err(|_| "Agent capability count exceeds uint32".to_string())?,
        );
    for capability in &disclosure.capabilities {
        frame = frame.string(capability.as_str())?;
    }
    frame = frame.u32(
        u32::try_from(disclosure.resource_classes.len())
            .map_err(|_| "Agent resource-class count exceeds uint32".to_string())?,
    );
    for resource_class in &disclosure.resource_classes {
        frame = frame.string(resource_class.as_str())?;
    }
    Ok(frame.u32(disclosure.max_parallel_tasks).finish())
}

pub fn agent_capability_disclosure_hash(
    disclosure: &AgentCapabilityDisclosureV1,
) -> Result<String, String> {
    agent_capability_disclosure_frame(disclosure).map(|frame| sha256_digest(&frame))
}

/// V3 preserves the V2 identity/key/owner proof and additionally binds the
/// hash of the exact bounded capability/resource self-declaration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentBindingProofClaimV3 {
    pub schema: String,
    pub binding_id: Uuid,
    pub agent_id: String,
    pub agent_key_id: String,
    pub agent_public_key: String,
    pub agent_public_key_hash: String,
    pub capability_disclosure_hash: String,
    pub subject_id: String,
    pub player_id: Uuid,
    pub nonce: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
}

pub fn agent_binding_proof_v3_signing_bytes(
    claim: &AgentBindingProofClaimV3,
) -> Result<Vec<u8>, String> {
    if claim.schema != AGENT_BINDING_PROOF_V3 {
        return Err(format!(
            "unsupported Agent binding proof schema {}",
            claim.schema
        ));
    }
    for (field, value) in [
        ("agent_id", claim.agent_id.as_str()),
        ("agent_key_id", claim.agent_key_id.as_str()),
        ("subject_id", claim.subject_id.as_str()),
        ("nonce", claim.nonce.as_str()),
    ] {
        validate_text(field, value)?;
    }
    if claim.issued_at_unix < 0 || claim.expires_at_unix <= claim.issued_at_unix {
        return Err("Agent binding proof validity interval is invalid".to_string());
    }
    let key = decode_base64_exact::<32>("agent_public_key", &claim.agent_public_key)?;
    if sha256_digest(&key) != claim.agent_public_key_hash {
        return Err("agent_public_key_hash does not match key".to_string());
    }
    if claim.agent_key_id != claim.agent_public_key_hash {
        return Err("agent_key_id must equal agent_public_key_hash".to_string());
    }
    Ok(
        CanonicalFrame::new("hepta_paper_raid_agent_binding_proof_v3")
            .string(&claim.schema)?
            .string(&claim.binding_id.to_string())?
            .string(&claim.agent_id)?
            .string(&claim.agent_key_id)?
            .bytes(&key)?
            .digest(&claim.agent_public_key_hash)?
            .digest(&claim.capability_disclosure_hash)?
            .string(&claim.subject_id)?
            .string(&claim.player_id.to_string())?
            .string(&claim.nonce)?
            .i64(claim.issued_at_unix)
            .i64(claim.expires_at_unix)
            .finish(),
    )
}

pub fn verify_agent_binding_proof_v3(
    claim: &AgentBindingProofClaimV3,
    signature: &str,
) -> Result<(), String> {
    let message = agent_binding_proof_v3_signing_bytes(claim)?;
    let key = decode_base64_exact::<32>("agent_public_key", &claim.agent_public_key)?;
    let key = VerifyingKey::from_bytes(&key)
        .map_err(|_| "Agent public key is not valid Ed25519".to_string())?;
    let signature = decode_base64_exact::<64>("agent_proof_signature", signature)?;
    key.verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| "Agent binding proof-of-possession failed".to_string())
}

/// Per-request proof used only by the dedicated Consumer Edge Agent Bridge
/// surface. It authenticates one already-bound external Agent key; it is not
/// a browser session, bearer credential, capability grant, or scientific fact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentBridgeRequestProofV1 {
    pub schema: String,
    pub binding_id: Uuid,
    pub agent_id: String,
    pub agent_key_id: String,
    pub http_method: String,
    pub canonical_path: String,
    pub canonical_query: String,
    pub body_hash: String,
    pub nonce: Uuid,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
}

fn agent_bridge_path_method_allowed(method: &str, path: &str) -> bool {
    matches!(
        (method, path),
        ("GET", "/api/agent-bridge/binding")
            | ("POST", "/api/agent-bridge/health")
            | ("POST", "/api/agent-bridge/inbox")
            | ("POST", "/api/agent-bridge/practice-tasks")
            | ("POST", "/api/agent-bridge/practice-claims")
            | ("POST", "/api/agent-bridge/practice-results")
            | ("POST", "/api/agent-bridge/delivery-drafts")
            | ("POST", "/api/agent-bridge/proposals")
            | ("GET", REVIEW_OBJECT_DOWNLOAD_PATH_V1)
            | ("GET", CHALLENGE_MATERIAL_OBJECT_DOWNLOAD_PATH_V1)
            | ("POST", "/api/agent-bridge/review-receipts")
    )
}

fn canonical_query_hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn agent_bridge_query_component(value: &str, field: &str) -> Result<String, String> {
    let encoded = value.as_bytes();
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut cursor = 0;
    while cursor < encoded.len() {
        let byte = encoded[cursor];
        if byte == b'%' {
            if cursor + 2 >= encoded.len() {
                return Err(format!("{field} contains a truncated percent escape"));
            }
            let high = canonical_query_hex_value(encoded[cursor + 1])
                .ok_or_else(|| format!("{field} percent escapes must use uppercase hex"))?;
            let low = canonical_query_hex_value(encoded[cursor + 2])
                .ok_or_else(|| format!("{field} percent escapes must use uppercase hex"))?;
            let decoded_byte = (high << 4) | low;
            if decoded_byte.is_ascii_alphanumeric()
                || matches!(decoded_byte, b'-' | b'.' | b'_' | b'~')
            {
                return Err(format!("{field} percent-encodes an unreserved byte"));
            }
            decoded.push(decoded_byte);
            cursor += 3;
            continue;
        }
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')) {
            return Err(format!("{field} contains a non-canonical query byte"));
        }
        decoded.push(byte);
        cursor += 1;
    }
    String::from_utf8(decoded).map_err(|_| format!("{field} is not valid UTF-8"))
}

/// Validates the exact RFC3986-style representation signed by both Rust and
/// non-Rust Bridge implementations. Keys are unique and encoded pairs are
/// strictly ascending; `+`, lowercase escapes, redundant escapes, fragments,
/// and a leading `?` are rejected.
pub fn validate_agent_bridge_canonical_query(value: &str) -> Result<(), String> {
    if value.len() > 2_048 {
        return Err("canonical Agent Bridge query exceeds 2048 bytes".to_string());
    }
    if value.is_empty() {
        return Ok(());
    }
    let mut keys = HashSet::new();
    let mut previous: Option<&str> = None;
    let mut count = 0_usize;
    for pair in value.split('&') {
        count += 1;
        if count > 32 {
            return Err("canonical Agent Bridge query exceeds 32 pairs".to_string());
        }
        let (encoded_key, encoded_value) = pair
            .split_once('=')
            .ok_or_else(|| "canonical Agent Bridge query pairs require '='".to_string())?;
        if encoded_key.is_empty() || encoded_value.contains('=') {
            return Err("canonical Agent Bridge query pair is malformed".to_string());
        }
        let key = agent_bridge_query_component(encoded_key, "query key")?;
        let decoded_value = agent_bridge_query_component(encoded_value, "query value")?;
        if key.chars().count() > 64 || decoded_value.chars().count() > 512 {
            return Err("canonical Agent Bridge query key or value is too long".to_string());
        }
        if !keys.insert(key) {
            return Err("canonical Agent Bridge query keys must be unique".to_string());
        }
        if previous.is_some_and(|prior| prior.as_bytes() >= pair.as_bytes()) {
            return Err(
                "canonical Agent Bridge query pairs must be strictly lexicographically sorted"
                    .to_string(),
            );
        }
        previous = Some(pair);
    }
    Ok(())
}

pub fn agent_bridge_request_proof_signing_bytes(
    claim: &AgentBridgeRequestProofV1,
) -> Result<Vec<u8>, String> {
    if claim.schema != AGENT_BRIDGE_REQUEST_PROOF_V1 {
        return Err(format!(
            "unsupported Agent Bridge request proof schema {}",
            claim.schema
        ));
    }
    validate_text("agent_id", &claim.agent_id)?;
    decode_digest(&claim.agent_key_id)?;
    decode_digest(&claim.body_hash)?;
    if !agent_bridge_path_method_allowed(&claim.http_method, &claim.canonical_path) {
        return Err("Agent Bridge request method/path is not allowed".to_string());
    }
    validate_agent_bridge_canonical_query(&claim.canonical_query)?;
    if claim.http_method == "GET" && claim.body_hash != sha256_digest(&[]) {
        return Err("Agent Bridge GET body hash must be SHA-256(empty)".to_string());
    }
    if claim.issued_at_unix < 0
        || claim.expires_at_unix <= claim.issued_at_unix
        || claim.expires_at_unix - claim.issued_at_unix
            > AGENT_BRIDGE_REQUEST_PROOF_MAX_LIFETIME_SECONDS
    {
        return Err("Agent Bridge request proof validity interval is invalid".to_string());
    }
    Ok(
        CanonicalFrame::new("hepta_paper_raid_agent_bridge_request_proof_v1")
            .string(&claim.schema)?
            .string(&claim.binding_id.to_string())?
            .string(&claim.agent_id)?
            // `agent_key_id` is validated as a SHA-256 identifier above, but the frozen
            // cross-language request-proof frame binds its canonical `sha256:<hex>` text.
            .string(&claim.agent_key_id)?
            .string(&claim.http_method)?
            .string(&claim.canonical_path)?
            .bytes(claim.canonical_query.as_bytes())?
            .digest(&claim.body_hash)?
            .string(&claim.nonce.to_string())?
            .i64(claim.issued_at_unix)
            .i64(claim.expires_at_unix)
            .finish(),
    )
}

pub fn agent_bridge_request_proof_hash(
    claim: &AgentBridgeRequestProofV1,
) -> Result<String, String> {
    agent_bridge_request_proof_signing_bytes(claim).map(|frame| sha256_digest(&frame))
}

pub fn verify_agent_bridge_request_proof(
    claim: &AgentBridgeRequestProofV1,
    agent_public_key: &str,
    signature: &str,
) -> Result<(), String> {
    let message = agent_bridge_request_proof_signing_bytes(claim)?;
    let key = decode_base64_exact::<32>("agent_public_key", agent_public_key)?;
    if sha256_digest(&key) != claim.agent_key_id {
        return Err("Agent Bridge proof key does not match agent_key_id".to_string());
    }
    let key = VerifyingKey::from_bytes(&key)
        .map_err(|_| "Agent Bridge public key is not valid Ed25519".to_string())?;
    let signature = decode_base64_exact::<64>("Agent Bridge signature", signature)?;
    key.verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| "Agent Bridge request proof-of-possession failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentBindingKeyRotationClaimV2 {
    pub schema: String,
    pub rotation_id: Uuid,
    pub binding_id: Uuid,
    pub expected_binding_version: u64,
    pub player_id: Uuid,
    pub subject_id: String,
    pub agent_id: String,
    pub old_agent_key_id: String,
    pub old_agent_public_key: String,
    pub old_agent_public_key_hash: String,
    pub new_agent_key_id: String,
    pub new_agent_public_key: String,
    pub new_agent_public_key_hash: String,
    pub nonce: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
}

pub fn agent_binding_key_rotation_signing_bytes(
    claim: &AgentBindingKeyRotationClaimV2,
) -> Result<Vec<u8>, String> {
    if claim.schema != AGENT_BINDING_KEY_ROTATION_V2
        || claim.expected_binding_version == 0
        || claim.expected_binding_version > JSON_SAFE_U64_MAX
    {
        return Err("Agent binding key rotation schema/version is invalid".to_string());
    }
    for (field, value) in [
        ("subject_id", claim.subject_id.as_str()),
        ("agent_id", claim.agent_id.as_str()),
        ("old_agent_key_id", claim.old_agent_key_id.as_str()),
        ("new_agent_key_id", claim.new_agent_key_id.as_str()),
        ("nonce", claim.nonce.as_str()),
    ] {
        validate_text(field, value)?;
    }
    if claim.issued_at_unix < 0 || claim.expires_at_unix <= claim.issued_at_unix {
        return Err("Agent binding key rotation validity interval is invalid".to_string());
    }
    let old_key = decode_base64_exact::<32>("old_agent_public_key", &claim.old_agent_public_key)?;
    let new_key = decode_base64_exact::<32>("new_agent_public_key", &claim.new_agent_public_key)?;
    if sha256_digest(&old_key) != claim.old_agent_public_key_hash
        || claim.old_agent_key_id != claim.old_agent_public_key_hash
    {
        return Err("old Agent key ID/hash does not match its public key".to_string());
    }
    if sha256_digest(&new_key) != claim.new_agent_public_key_hash
        || claim.new_agent_key_id != claim.new_agent_public_key_hash
    {
        return Err("new Agent key ID/hash does not match its public key".to_string());
    }
    if old_key == new_key {
        return Err("Agent binding key rotation must change the public key".to_string());
    }
    Ok(
        CanonicalFrame::new("hepta_paper_raid_agent_binding_key_rotation_v2")
            .string(&claim.schema)?
            .string(&claim.rotation_id.to_string())?
            .string(&claim.binding_id.to_string())?
            .u64(claim.expected_binding_version)
            .string(&claim.player_id.to_string())?
            .string(&claim.subject_id)?
            .string(&claim.agent_id)?
            .string(&claim.old_agent_key_id)?
            .bytes(&old_key)?
            .digest(&claim.old_agent_public_key_hash)?
            .string(&claim.new_agent_key_id)?
            .bytes(&new_key)?
            .digest(&claim.new_agent_public_key_hash)?
            .string(&claim.nonce)?
            .i64(claim.issued_at_unix)
            .i64(claim.expires_at_unix)
            .finish(),
    )
}

pub fn verify_agent_binding_key_rotation_signatures(
    claim: &AgentBindingKeyRotationClaimV2,
    old_key_signature: &str,
    new_key_signature: &str,
) -> Result<(), String> {
    let message = agent_binding_key_rotation_signing_bytes(claim)?;
    for (label, public_key, signature) in [
        (
            "old",
            claim.old_agent_public_key.as_str(),
            old_key_signature,
        ),
        (
            "new",
            claim.new_agent_public_key.as_str(),
            new_key_signature,
        ),
    ] {
        let key = decode_base64_exact::<32>("agent_public_key", public_key)?;
        let key = VerifyingKey::from_bytes(&key)
            .map_err(|_| format!("{label} Agent public key is not valid Ed25519"))?;
        let signature = decode_base64_exact::<64>("signature", signature)?;
        key.verify(&message, &Signature::from_bytes(&signature))
            .map_err(|_| format!("{label} Agent key rotation signature failed"))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HumanKeyRotationClaimV2 {
    pub schema: String,
    pub rotation_id: Uuid,
    pub player_id: Uuid,
    pub subject_id: String,
    pub nakama_user_id: Uuid,
    pub old_signing_key_id: String,
    pub old_signing_public_key_hash: String,
    pub new_signing_key_id: String,
    pub new_signing_public_key: String,
    pub new_signing_public_key_hash: String,
    pub nonce: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
}

pub fn human_key_rotation_signing_bytes(
    claim: &HumanKeyRotationClaimV2,
) -> Result<Vec<u8>, String> {
    if claim.schema != HUMAN_KEY_ROTATION_V2 {
        return Err(format!(
            "unsupported human key rotation schema {}",
            claim.schema
        ));
    }
    for (field, value) in [
        ("subject_id", claim.subject_id.as_str()),
        ("old_signing_key_id", claim.old_signing_key_id.as_str()),
        ("new_signing_key_id", claim.new_signing_key_id.as_str()),
        ("nonce", claim.nonce.as_str()),
    ] {
        validate_text(field, value)?;
    }
    if claim.issued_at_unix < 0 || claim.expires_at_unix <= claim.issued_at_unix {
        return Err("human key rotation validity interval is invalid".to_string());
    }
    decode_digest(&claim.old_signing_public_key_hash)?;
    let new_key =
        decode_base64_exact::<32>("new_signing_public_key", &claim.new_signing_public_key)?;
    if sha256_digest(&new_key) != claim.new_signing_public_key_hash {
        return Err("new_signing_public_key_hash does not match key".to_string());
    }
    Ok(
        CanonicalFrame::new("hepta_paper_raid_human_key_rotation_v2")
            .string(&claim.schema)?
            .string(&claim.rotation_id.to_string())?
            .string(&claim.player_id.to_string())?
            .string(&claim.subject_id)?
            .string(&claim.nakama_user_id.to_string())?
            .string(&claim.old_signing_key_id)?
            .digest(&claim.old_signing_public_key_hash)?
            .string(&claim.new_signing_key_id)?
            .bytes(&new_key)?
            .digest(&claim.new_signing_public_key_hash)?
            .string(&claim.nonce)?
            .i64(claim.issued_at_unix)
            .i64(claim.expires_at_unix)
            .finish(),
    )
}

pub fn verify_human_key_rotation_signatures(
    claim: &HumanKeyRotationClaimV2,
    old_public_key: &str,
    old_signature: &str,
    new_signature: &str,
) -> Result<(), String> {
    let message = human_key_rotation_signing_bytes(claim)?;
    let old_key = decode_base64_exact::<32>("old_signing_public_key", old_public_key)?;
    if sha256_digest(&old_key) != claim.old_signing_public_key_hash {
        return Err("old_signing_public_key_hash does not match stored key".to_string());
    }
    let new_key =
        decode_base64_exact::<32>("new_signing_public_key", &claim.new_signing_public_key)?;
    for (label, key, signature) in [
        ("old", old_key, old_signature),
        ("new", new_key, new_signature),
    ] {
        let key = VerifyingKey::from_bytes(&key)
            .map_err(|_| format!("{label} human key is not valid Ed25519"))?;
        let signature = decode_base64_exact::<64>(&format!("{label}_signature"), signature)?;
        key.verify(&message, &Signature::from_bytes(&signature))
            .map_err(|_| format!("{label} human key rotation signature failed"))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HumanKeyRevocationClaimV2 {
    pub schema: String,
    pub revocation_id: Uuid,
    pub player_id: Uuid,
    pub subject_id: String,
    pub nakama_user_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key_hash: String,
    pub reason_hash: String,
    pub nonce: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
}

pub fn human_key_revocation_signing_bytes(
    claim: &HumanKeyRevocationClaimV2,
) -> Result<Vec<u8>, String> {
    if claim.schema != HUMAN_KEY_REVOCATION_V2 {
        return Err(format!(
            "unsupported human key revocation schema {}",
            claim.schema
        ));
    }
    for (field, value) in [
        ("subject_id", claim.subject_id.as_str()),
        ("signing_key_id", claim.signing_key_id.as_str()),
        ("nonce", claim.nonce.as_str()),
    ] {
        validate_text(field, value)?;
    }
    if claim.issued_at_unix < 0 || claim.expires_at_unix <= claim.issued_at_unix {
        return Err("human key revocation validity interval is invalid".to_string());
    }
    Ok(
        CanonicalFrame::new("hepta_paper_raid_human_key_revocation_v2")
            .string(&claim.schema)?
            .string(&claim.revocation_id.to_string())?
            .string(&claim.player_id.to_string())?
            .string(&claim.subject_id)?
            .string(&claim.nakama_user_id.to_string())?
            .string(&claim.signing_key_id)?
            .digest(&claim.signing_public_key_hash)?
            .digest(&claim.reason_hash)?
            .string(&claim.nonce)?
            .i64(claim.issued_at_unix)
            .i64(claim.expires_at_unix)
            .finish(),
    )
}

pub fn verify_human_key_revocation_signature(
    claim: &HumanKeyRevocationClaimV2,
    public_key: &str,
    signature: &str,
) -> Result<(), String> {
    let message = human_key_revocation_signing_bytes(claim)?;
    let key = decode_base64_exact::<32>("signing_public_key", public_key)?;
    if sha256_digest(&key) != claim.signing_public_key_hash {
        return Err("signing_public_key_hash does not match stored key".to_string());
    }
    let key = VerifyingKey::from_bytes(&key)
        .map_err(|_| "human signing key is not valid Ed25519".to_string())?;
    let signature = decode_base64_exact::<64>("signature", signature)?;
    key.verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| "human key revocation signature failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchSessionAuthorizationClaimV1 {
    pub schema: String,
    pub authorization_id: String,
    pub session_id: String,
    pub team_id: String,
    pub paper_project_id: String,
    pub challenge_id: String,
    pub agent_id: String,
    pub agent_did: String,
    pub agent_key_id: String,
    pub agent_public_key: String,
    pub subject_user_id: String,
    pub participant_slot: u32,
    pub role: String,
    pub roster_version: u64,
    pub roster_root: String,
    pub ruleset_hash: String,
    pub challenge_snapshot_hash: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedResearchSessionAuthorizationV1 {
    pub claim: ResearchSessionAuthorizationClaimV1,
    pub issuer_key_id: String,
    pub signature: String,
}

pub fn research_session_authorization_claim_frame(
    claim: &ResearchSessionAuthorizationClaimV1,
) -> Result<Vec<u8>, String> {
    if claim.schema != RESEARCH_SESSION_AUTHORIZATION_V1 {
        return Err(format!("unsupported authorization schema {}", claim.schema));
    }
    validate_logical_id("session_id", &claim.session_id)?;
    for (field, value) in [
        ("authorization_id", claim.authorization_id.as_str()),
        ("session_id", claim.session_id.as_str()),
        ("team_id", claim.team_id.as_str()),
        ("paper_project_id", claim.paper_project_id.as_str()),
        ("challenge_id", claim.challenge_id.as_str()),
        ("agent_id", claim.agent_id.as_str()),
        ("agent_did", claim.agent_did.as_str()),
        ("agent_key_id", claim.agent_key_id.as_str()),
        ("subject_user_id", claim.subject_user_id.as_str()),
        ("role", claim.role.as_str()),
    ] {
        validate_text(field, value)?;
    }
    if !(1..=5).contains(&claim.participant_slot) {
        return Err("participant_slot must be between 1 and 5".to_string());
    }
    if claim.roster_version == 0 {
        return Err("roster_version must be positive".to_string());
    }
    if claim.issued_at_unix < 0 || claim.expires_at_unix <= claim.issued_at_unix {
        return Err("authorization validity interval is invalid".to_string());
    }
    let key = decode_base64_exact::<32>("agent_public_key", &claim.agent_public_key)?;
    Ok(
        CanonicalFrame::new("trnm_research_session_authorization_claim_v1")
            .string(&claim.schema)?
            .string(&claim.authorization_id)?
            .string(&claim.session_id)?
            .string(&claim.team_id)?
            .string(&claim.paper_project_id)?
            .string(&claim.challenge_id)?
            .string(&claim.agent_id)?
            .string(&claim.agent_did)?
            .string(&claim.agent_key_id)?
            .bytes(&key)?
            .string(&claim.subject_user_id)?
            .u32(claim.participant_slot)
            .string(&claim.role)?
            .u64(claim.roster_version)
            .digest(&claim.roster_root)?
            .digest(&claim.ruleset_hash)?
            .digest(&claim.challenge_snapshot_hash)?
            .i64(claim.issued_at_unix)
            .i64(claim.expires_at_unix)
            .finish(),
    )
}

pub fn research_session_authorization_signing_bytes(
    claim: &ResearchSessionAuthorizationClaimV1,
    issuer_key_id: &str,
) -> Result<Vec<u8>, String> {
    validate_text("issuer_key_id", issuer_key_id)?;
    let claim_frame = research_session_authorization_claim_frame(claim)?;
    Ok(
        CanonicalFrame::new("trnm_research_session_authorization_signature_v1")
            .string(issuer_key_id)?
            .bytes(&claim_frame)?
            .finish(),
    )
}

pub fn sign_research_session_authorization(
    claim: ResearchSessionAuthorizationClaimV1,
    issuer_key_id: &str,
    signing_key: &SigningKey,
) -> Result<SignedResearchSessionAuthorizationV1, String> {
    let bytes = research_session_authorization_signing_bytes(&claim, issuer_key_id)?;
    Ok(SignedResearchSessionAuthorizationV1 {
        claim,
        issuer_key_id: issuer_key_id.to_string(),
        signature: BASE64.encode(signing_key.sign(&bytes).to_bytes()),
    })
}

pub fn verify_research_session_authorization(
    signed: &SignedResearchSessionAuthorizationV1,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let bytes = research_session_authorization_signing_bytes(&signed.claim, &signed.issuer_key_id)?;
    let signature = decode_base64_exact::<64>("signature", &signed.signature)?;
    verifying_key
        .verify(&bytes, &Signature::from_bytes(&signature))
        .map_err(|_| "research session authorization signature verification failed".to_string())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchControlOperationV2 {
    Create,
    Resume,
    ReplaceRoster,
    Complete,
}

impl ResearchControlOperationV2 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Resume => "resume",
            Self::ReplaceRoster => "replace_roster",
            Self::Complete => "complete",
        }
    }

    pub fn target_rpc(self) -> &'static str {
        match self {
            Self::Create => RESEARCH_CONTROL_RPC_CREATE_V2,
            Self::Resume => RESEARCH_CONTROL_RPC_RESUME_V2,
            Self::ReplaceRoster => RESEARCH_CONTROL_RPC_REPLACE_V2,
            Self::Complete => RESEARCH_CONTROL_RPC_COMPLETE_V2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchControlClaimV2 {
    pub schema: String,
    pub command_id: Uuid,
    pub operation: ResearchControlOperationV2,
    pub target_rpc: String,
    pub session_id: String,
    pub session_roster_version: u64,
    pub authorization_set_id: Uuid,
    pub payload_hash: String,
    pub audience: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
    pub issuer_key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedResearchControlV2 {
    pub claim: ResearchControlClaimV2,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchControlCreateRequestV2 {
    pub schema: String,
    pub authorization_set_id: Uuid,
    pub authorizations: Vec<SignedResearchSessionAuthorizationV1>,
    pub control: SignedResearchControlV2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchControlResumeRequestV2 {
    pub schema: String,
    pub logical_session_id: String,
    pub authorization_set_id: Uuid,
    pub control: SignedResearchControlV2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchControlReplaceRequestV2 {
    pub schema: String,
    pub logical_session_id: String,
    pub authorization_set_id: Uuid,
    pub authorizations: Vec<SignedResearchSessionAuthorizationV1>,
    pub control: SignedResearchControlV2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchControlCompleteRequestV2 {
    pub schema: String,
    pub logical_session_id: String,
    pub authorization_set_id: Uuid,
    pub facts: ResearchSessionTerminalFactsV1,
    pub control: SignedResearchControlV2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchControlRuntimeResultV1 {
    pub schema: String,
    pub logical_session_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub external_match_id: String,
    pub runtime_generation: u64,
    pub status: String,
    pub session_version: u64,
    pub roster_version: u64,
    pub roster_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchControlEvidenceResultV1 {
    pub schema: String,
    pub logical_session_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub external_match_id: String,
    pub runtime_generation: u64,
    pub completion: ResearchSessionCompletionV1,
    pub authority_public_key_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchControlResultV2<T> {
    pub schema: String,
    pub command_id: Uuid,
    pub operation: ResearchControlOperationV2,
    pub target_rpc: String,
    pub result: T,
}

fn research_control_authorization_envelope_v2(
    authorization: &SignedResearchSessionAuthorizationV1,
) -> Result<Vec<u8>, String> {
    let signing = research_session_authorization_signing_bytes(
        &authorization.claim,
        &authorization.issuer_key_id,
    )?;
    let signature = decode_base64_exact::<64>("authorization_signature", &authorization.signature)?;
    Ok(
        CanonicalFrame::new("trnm_research_control_authorization_envelope_v2")
            .bytes(&signing)?
            .bytes(&signature)?
            .finish(),
    )
}

fn research_control_authorization_set_business_v2(
    domain: &str,
    schema: &str,
    session_id: &str,
    authorization_set_id: Uuid,
    authorizations: &[SignedResearchSessionAuthorizationV1],
) -> Result<Vec<u8>, String> {
    validate_logical_id("session_id", session_id)?;
    if !(3..=5).contains(&authorizations.len()) {
        return Err("research control authorization set must contain 3-5 entries".to_string());
    }
    let roster_version = authorizations[0].claim.roster_version;
    if roster_version == 0 || roster_version > JSON_SAFE_U64_MAX {
        return Err("research control authorization roster version is invalid".to_string());
    }
    let mut frame = CanonicalFrame::new(domain)
        .string(schema)?
        .string(session_id)?
        .string(&authorization_set_id.to_string())?
        .u32(u32::try_from(authorizations.len()).map_err(|_| "authorization set too large")?);
    for (index, authorization) in authorizations.iter().enumerate() {
        if authorization.claim.participant_slot
            != u32::try_from(index + 1).map_err(|_| "participant slot overflow")?
            || authorization.claim.session_id != session_id
            || authorization.claim.roster_version != roster_version
        {
            return Err(
                "research control authorizations must be ordered and bind one session and roster version"
                    .to_string(),
            );
        }
        frame = frame.bytes(&research_control_authorization_envelope_v2(authorization)?)?;
    }
    Ok(frame.finish())
}

pub fn research_control_create_business_v2(
    request: &ResearchControlCreateRequestV2,
) -> Result<Vec<u8>, String> {
    if request.schema != RESEARCH_CONTROL_CREATE_REQUEST_V2 || request.authorizations.is_empty() {
        return Err("invalid research control create request".to_string());
    }
    research_control_authorization_set_business_v2(
        "trnm_research_control_create_business_v2",
        &request.schema,
        &request.authorizations[0].claim.session_id,
        request.authorization_set_id,
        &request.authorizations,
    )
}

pub fn research_control_resume_business_v2(
    request: &ResearchControlResumeRequestV2,
) -> Result<Vec<u8>, String> {
    if request.schema != RESEARCH_CONTROL_RESUME_REQUEST_V2 {
        return Err("invalid research control resume request".to_string());
    }
    validate_logical_id("logical_session_id", &request.logical_session_id)?;
    Ok(
        CanonicalFrame::new("trnm_research_control_resume_business_v2")
            .string(&request.schema)?
            .string(&request.logical_session_id)?
            .string(&request.authorization_set_id.to_string())?
            .finish(),
    )
}

pub fn research_control_replace_business_v2(
    request: &ResearchControlReplaceRequestV2,
) -> Result<Vec<u8>, String> {
    if request.schema != RESEARCH_CONTROL_REPLACE_REQUEST_V2 {
        return Err("invalid research control replacement request".to_string());
    }
    research_control_authorization_set_business_v2(
        "trnm_research_control_replace_business_v2",
        &request.schema,
        &request.logical_session_id,
        request.authorization_set_id,
        &request.authorizations,
    )
}

pub fn research_control_complete_business_v2(
    request: &ResearchControlCompleteRequestV2,
) -> Result<Vec<u8>, String> {
    if request.schema != RESEARCH_CONTROL_COMPLETE_REQUEST_V2 {
        return Err("invalid research control completion request".to_string());
    }
    validate_logical_id("logical_session_id", &request.logical_session_id)?;
    let facts = research_session_terminal_facts_frame(&request.facts)?;
    Ok(
        CanonicalFrame::new("trnm_research_control_complete_business_v2")
            .string(&request.schema)?
            .string(&request.logical_session_id)?
            .string(&request.authorization_set_id.to_string())?
            .bytes(&facts)?
            .finish(),
    )
}

pub fn research_control_claim_frame_v2(claim: &ResearchControlClaimV2) -> Result<Vec<u8>, String> {
    if claim.schema != RESEARCH_CONTROL_CLAIM_V2
        || claim.target_rpc != claim.operation.target_rpc()
        || claim.audience != RESEARCH_CONTROL_AUDIENCE_V2
        || claim.session_roster_version == 0
        || claim.session_roster_version > JSON_SAFE_U64_MAX
    {
        return Err(
            "research control claim schema, operation, audience, or version is invalid".to_string(),
        );
    }
    validate_logical_id("session_id", &claim.session_id)?;
    validate_key_id("issuer_key_id", &claim.issuer_key_id)?;
    decode_digest(&claim.payload_hash)?;
    if claim.issued_at_unix < 0
        || u64::try_from(claim.issued_at_unix).map_or(true, |value| value > JSON_SAFE_U64_MAX)
        || claim.expires_at_unix <= claim.issued_at_unix
        || u64::try_from(claim.expires_at_unix).map_or(true, |value| value > JSON_SAFE_U64_MAX)
        || claim.expires_at_unix - claim.issued_at_unix > RESEARCH_CONTROL_MAXIMUM_LIFETIME_SECONDS
    {
        return Err("research control validity interval is invalid".to_string());
    }
    Ok(CanonicalFrame::new("trnm_research_control_claim_v2")
        .string(&claim.schema)?
        .string(&claim.command_id.to_string())?
        .string(claim.operation.as_str())?
        .string(&claim.target_rpc)?
        .string(&claim.session_id)?
        .u64(claim.session_roster_version)
        .string(&claim.authorization_set_id.to_string())?
        .digest(&claim.payload_hash)?
        .string(&claim.audience)?
        .i64(claim.issued_at_unix)
        .i64(claim.expires_at_unix)
        .string(&claim.issuer_key_id)?
        .finish())
}

pub fn research_control_signing_bytes_v2(
    claim: &ResearchControlClaimV2,
) -> Result<Vec<u8>, String> {
    let claim_frame = research_control_claim_frame_v2(claim)?;
    Ok(CanonicalFrame::new("trnm_research_control_signature_v2")
        .bytes(&claim_frame)?
        .finish())
}

pub fn sign_research_control_v2(
    claim: ResearchControlClaimV2,
    signing_key: &SigningKey,
) -> Result<SignedResearchControlV2, String> {
    let message = research_control_signing_bytes_v2(&claim)?;
    Ok(SignedResearchControlV2 {
        claim,
        signature: BASE64.encode(signing_key.sign(&message).to_bytes()),
    })
}

pub fn verify_research_control_v2(
    control: &SignedResearchControlV2,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let message = research_control_signing_bytes_v2(&control.claim)?;
    let signature = decode_base64_exact::<64>("control_signature", &control.signature)?;
    verifying_key
        .verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| "research control signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchSessionRosterMemberV1 {
    pub participant_slot: u32,
    pub authorization_id: String,
    pub subject_user_id: String,
    pub agent_id: String,
    pub agent_did: String,
    pub agent_key_id: String,
    pub agent_key_hash: String,
    pub role: String,
}

pub fn research_session_roster_frame(
    session_id: &str,
    team_id: &str,
    paper_project_id: &str,
    roster_version: u64,
    members: &[ResearchSessionRosterMemberV1],
) -> Result<Vec<u8>, String> {
    for (field, value) in [
        ("session_id", session_id),
        ("team_id", team_id),
        ("paper_project_id", paper_project_id),
    ] {
        validate_text(field, value)?;
    }
    validate_logical_id("session_id", session_id)?;
    if roster_version == 0 {
        return Err("roster_version must be positive".to_string());
    }
    if !(3..=5).contains(&members.len()) {
        return Err("research session roster must contain 3 to 5 members".to_string());
    }
    let mut ordered = members.to_vec();
    ordered.sort_by_key(|member| member.participant_slot);
    let mut authorization_ids = std::collections::HashSet::new();
    let mut subject_user_ids = std::collections::HashSet::new();
    let mut agent_ids = std::collections::HashSet::new();
    let mut agent_dids = std::collections::HashSet::new();
    let mut agent_key_ids = std::collections::HashSet::new();
    let mut agent_key_hashes = std::collections::HashSet::new();
    for (index, member) in ordered.iter().enumerate() {
        let expected_slot = u32::try_from(index + 1).expect("five slots fit u32");
        if member.participant_slot != expected_slot {
            return Err("participant slots must be unique and gapless from 1".to_string());
        }
        for (field, value) in [
            ("authorization_id", member.authorization_id.as_str()),
            ("subject_user_id", member.subject_user_id.as_str()),
            ("agent_id", member.agent_id.as_str()),
            ("agent_did", member.agent_did.as_str()),
            ("agent_key_id", member.agent_key_id.as_str()),
            ("role", member.role.as_str()),
        ] {
            validate_text(field, value)?;
        }
        decode_digest(&member.agent_key_hash)?;
        if !authorization_ids.insert(member.authorization_id.clone())
            || !subject_user_ids.insert(member.subject_user_id.clone())
            || !agent_ids.insert(member.agent_id.clone())
            || !agent_dids.insert(member.agent_did.clone())
            || !agent_key_ids.insert(member.agent_key_id.clone())
            || !agent_key_hashes.insert(member.agent_key_hash.clone())
        {
            return Err(
                "roster authorization, user, Agent, DID, key ID, and key hash values must be unique"
                    .to_string(),
            );
        }
    }
    let mut frame = CanonicalFrame::new("trnm_research_session_roster_v1")
        .string(session_id)?
        .string(team_id)?
        .string(paper_project_id)?
        .u64(roster_version)
        .u32(u32::try_from(ordered.len()).expect("five members fit u32"));
    for member in &ordered {
        frame = frame
            .u32(member.participant_slot)
            .string(&member.authorization_id)?
            .string(&member.subject_user_id)?
            .string(&member.agent_id)?
            .string(&member.agent_did)?
            .string(&member.agent_key_id)?
            .digest(&member.agent_key_hash)?
            .string(&member.role)?;
    }
    Ok(frame.finish())
}

pub fn research_session_roster_root(
    session_id: &str,
    team_id: &str,
    paper_project_id: &str,
    roster_version: u64,
    members: &[ResearchSessionRosterMemberV1],
) -> Result<String, String> {
    research_session_roster_frame(
        session_id,
        team_id,
        paper_project_id,
        roster_version,
        members,
    )
    .map(|frame| sha256_digest(&frame))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchSessionActionV1 {
    pub schema: String,
    pub action_id: String,
    pub authorization_id: String,
    pub session_id: String,
    pub team_id: String,
    pub paper_project_id: String,
    pub challenge_id: String,
    pub roster_version: u64,
    pub participant_slot: u32,
    pub participant_sequence: u64,
    pub expected_session_version: u64,
    pub issued_at_unix: i64,
    pub action_type: String,
    pub payload_type: String,
    pub payload: String,
    pub payload_hash: String,
    pub reference_hash: String,
    pub agent_key_id: String,
    pub signature: String,
}

pub fn research_session_action_signing_bytes(
    action: &ResearchSessionActionV1,
) -> Result<Vec<u8>, String> {
    if action.schema != RESEARCH_SESSION_ACTION_V1 {
        return Err(format!("unsupported action schema {}", action.schema));
    }
    if !(1..=5).contains(&action.participant_slot) {
        return Err("participant_slot must be between 1 and 5".to_string());
    }
    if action.roster_version == 0 || action.participant_sequence == 0 {
        return Err("roster_version and participant_sequence must be positive".to_string());
    }
    if action.expected_session_version == 0 || action.issued_at_unix < 0 {
        return Err(
            "expected_session_version must be positive and issued_at_unix non-negative".to_string(),
        );
    }
    validate_logical_id("session_id", &action.session_id)?;
    for (field, value) in [
        ("action_id", action.action_id.as_str()),
        ("authorization_id", action.authorization_id.as_str()),
        ("team_id", action.team_id.as_str()),
        ("paper_project_id", action.paper_project_id.as_str()),
        ("challenge_id", action.challenge_id.as_str()),
        ("action_type", action.action_type.as_str()),
        ("payload_type", action.payload_type.as_str()),
        ("agent_key_id", action.agent_key_id.as_str()),
    ] {
        validate_text(field, value)?;
    }
    let payload = decode_base64("payload", &action.payload)?;
    if payload.is_empty() || payload.len() > 65_536 {
        return Err("payload must contain 1 to 65536 bytes".to_string());
    }
    let valid_type_pair = matches!(
        (action.action_type.as_str(), action.payload_type.as_str()),
        ("participant.ready", "trnm.research-session.ready.v1")
            | ("research.task.claimed", "trnm.paper-raid.task-claim.v1")
            | (
                "agent.proposal.submitted",
                "trnm.paper-raid.agent-proposal.v1"
            )
            | (
                "artifact.manifest.published",
                "trnm.paper-raid.artifact-manifest.v1"
            )
            | ("review.submitted", "trnm.paper-raid.review.v1")
            | ("checkpoint.recorded", "trnm.paper-raid.checkpoint.v1")
            | (
                "paper.release.acknowledged",
                "trnm.paper-raid.release-acknowledgement.v1"
            )
    );
    if !valid_type_pair {
        return Err("action_type and payload_type are not an allowed pair".to_string());
    }
    if sha256_digest(&payload) != action.payload_hash {
        return Err("payload_hash does not match payload bytes".to_string());
    }
    decode_digest(&action.reference_hash)?;
    Ok(
        CanonicalFrame::new("trnm_research_session_action_signature_v1")
            .string(&action.schema)?
            .string(&action.action_id)?
            .string(&action.authorization_id)?
            .string(&action.session_id)?
            .string(&action.team_id)?
            .string(&action.paper_project_id)?
            .string(&action.challenge_id)?
            .u64(action.roster_version)
            .u32(action.participant_slot)
            .u64(action.participant_sequence)
            .u64(action.expected_session_version)
            .i64(action.issued_at_unix)
            .string(&action.action_type)?
            .string(&action.payload_type)?
            .bytes(&payload)?
            .digest(&action.payload_hash)?
            .digest(&action.reference_hash)?
            .string(&action.agent_key_id)?
            .finish(),
    )
}

pub fn verify_research_session_action(
    action: &ResearchSessionActionV1,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let bytes = research_session_action_signing_bytes(action)?;
    let signature = decode_base64_exact::<64>("signature", &action.signature)?;
    verifying_key
        .verify(&bytes, &Signature::from_bytes(&signature))
        .map_err(|_| "research session action signature verification failed".to_string())
}

pub fn research_session_action_fingerprint(
    action: &ResearchSessionActionV1,
) -> Result<String, String> {
    let signing = research_session_action_signing_bytes(action)?;
    let signature = decode_base64_exact::<64>("signature", &action.signature)?;
    let frame = CanonicalFrame::new("trnm_research_session_action_fingerprint_v1")
        .bytes(&signing)?
        .bytes(&signature)?
        .finish();
    Ok(sha256_digest(&frame))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchSessionEventV1 {
    pub schema: String,
    pub event_id: String,
    pub event_type: String,
    pub session_id: String,
    pub team_id: String,
    pub paper_project_id: String,
    pub challenge_id: String,
    pub roster_version: u64,
    pub sequence: u64,
    pub causation_id: String,
    pub occurred_at_unix: i64,
    pub participant_slot: u32,
    pub session_version: u64,
    pub action_type: String,
    pub payload_type: String,
    pub payload: String,
    pub payload_hash: String,
    pub reference_hash: String,
    pub event_hash: String,
}

pub fn research_session_event_facts_frame(
    event: &ResearchSessionEventV1,
) -> Result<Vec<u8>, String> {
    if event.schema != RESEARCH_SESSION_EVENT_V1 {
        return Err(format!("unsupported event schema {}", event.schema));
    }
    validate_logical_id("session_id", &event.session_id)?;
    for (field, value) in [
        ("event_id", event.event_id.as_str()),
        ("event_type", event.event_type.as_str()),
        ("team_id", event.team_id.as_str()),
        ("paper_project_id", event.paper_project_id.as_str()),
        ("challenge_id", event.challenge_id.as_str()),
        ("causation_id", event.causation_id.as_str()),
        ("action_type", event.action_type.as_str()),
        ("payload_type", event.payload_type.as_str()),
    ] {
        validate_text(field, value)?;
    }
    if event.roster_version == 0 || event.sequence == 0 || event.session_version == 0 {
        return Err("roster, event, and session versions must be positive".to_string());
    }
    if event.participant_slot > 5 {
        return Err("event participant_slot is outside 0..5".to_string());
    }
    if event.occurred_at_unix < 0 {
        return Err("event time must be non-negative".to_string());
    }
    let payload = decode_base64("payload", &event.payload)?;
    if payload.is_empty() || payload.len() > 65_536 {
        return Err("event payload must contain 1 to 65536 bytes".to_string());
    }
    if sha256_digest(&payload) != event.payload_hash {
        return Err("event payload_hash does not match payload bytes".to_string());
    }
    decode_digest(&event.reference_hash)?;
    Ok(CanonicalFrame::new("trnm_research_session_event_v1")
        .string(&event.schema)?
        .string(&event.event_id)?
        .string(&event.event_type)?
        .string(&event.session_id)?
        .string(&event.team_id)?
        .string(&event.paper_project_id)?
        .string(&event.challenge_id)?
        .u64(event.roster_version)
        .u64(event.sequence)
        .string(&event.causation_id)?
        .i64(event.occurred_at_unix)
        .u32(event.participant_slot)
        .u64(event.session_version)
        .string(&event.action_type)?
        .string(&event.payload_type)?
        .bytes(&payload)?
        .digest(&event.payload_hash)?
        .digest(&event.reference_hash)?
        .finish())
}

pub fn research_session_event_hash(event: &ResearchSessionEventV1) -> Result<String, String> {
    research_session_event_facts_frame(event).map(|frame| sha256_digest(&frame))
}

pub fn research_session_event_id(
    session_id: &str,
    sequence: u64,
    causation_id: &str,
) -> Result<String, String> {
    validate_logical_id("session_id", session_id)?;
    if sequence == 0 {
        return Err("event sequence must be positive".to_string());
    }
    validate_text("causation_id", causation_id)?;
    let frame = CanonicalFrame::new("trnm_research_session_event_id_v1")
        .string(session_id)?
        .string(causation_id)?
        .u64(sequence)
        .finish();
    Ok(sha256_digest(&frame))
}

pub fn validate_research_session_archive(events: &[ResearchSessionEventV1]) -> Result<(), String> {
    let Some(first) = events.first() else {
        return Err("event archive is empty".to_string());
    };
    let mut seen_ids = std::collections::HashSet::new();
    let mut seen_hashes = std::collections::HashSet::new();
    let mut previous_time = 0_i64;
    for (index, event) in events.iter().enumerate() {
        if event.session_id != first.session_id
            || event.team_id != first.team_id
            || event.paper_project_id != first.paper_project_id
            || event.challenge_id != first.challenge_id
        {
            return Err(format!("event {index} has a different session identity"));
        }
        let expected_sequence = u64::try_from(index + 1).map_err(|_| "archive too large")?;
        if event.sequence != expected_sequence || event.session_version != event.sequence + 1 {
            return Err(format!(
                "event {index} has a non-contiguous sequence or version"
            ));
        }
        if index > 0 && event.occurred_at_unix < previous_time {
            return Err(format!("event {index} time moves backwards"));
        }
        previous_time = event.occurred_at_unix;
        if !matches!(
            event.event_type.as_str(),
            "participant_joined"
                | "participant_disconnected"
                | "participant_reconnected"
                | "research_action_applied"
                | "roster_replaced"
                | "research_session_completed"
        ) {
            return Err(format!(
                "event {index} has unsupported type {}",
                event.event_type
            ));
        }
        let terminal = event.event_type == "research_session_completed";
        if terminal && index + 1 != events.len() {
            return Err(format!("event {index} completion is not terminal"));
        }
        if (terminal && event.participant_slot != 0) || (!terminal && event.participant_slot == 0) {
            return Err(format!("event {index} has an invalid participant slot"));
        }
        let expected_hash = research_session_event_hash(event)?;
        if event.event_hash != expected_hash {
            return Err(format!(
                "event {index} event_hash does not match event facts"
            ));
        }
        decode_digest(&event.event_hash)?;
        let expected_id =
            research_session_event_id(&event.session_id, event.sequence, &event.causation_id)?;
        if event.event_id != expected_id {
            return Err(format!("event {index} event_id is not canonical"));
        }
        if !seen_ids.insert(event.event_id.clone()) {
            return Err(format!("event {index} duplicates event_id"));
        }
        if !seen_hashes.insert(event.event_hash.clone()) {
            return Err(format!("event {index} duplicates event_hash"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchSessionEventCommitmentV1 {
    pub sequence: u64,
    pub event_hash: String,
}

pub fn research_session_event_root_from_commitments(
    commitments: &[ResearchSessionEventCommitmentV1],
) -> Result<String, String> {
    if commitments.is_empty() {
        return Err("cannot compute event root for empty archive".to_string());
    }
    let mut level = Vec::<[u8; 32]>::with_capacity(commitments.len());
    for (index, commitment) in commitments.iter().enumerate() {
        let expected_sequence = u64::try_from(index + 1).map_err(|_| "archive too large")?;
        if commitment.sequence != expected_sequence {
            return Err(format!("event sequence gap at {index}"));
        }
        let mut leaf = Vec::with_capacity(48 + 8 + 32);
        leaf.extend_from_slice(b"trnm_research_session_event_leaf_v1\0");
        leaf.extend_from_slice(&commitment.sequence.to_be_bytes());
        leaf.extend_from_slice(&decode_digest(&commitment.event_hash)?);
        level.push(Sha256::digest(&leaf).into());
    }
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let left = pair[0];
            let right = pair.get(1).copied().unwrap_or(left);
            let mut node = Vec::with_capacity(48 + 64);
            node.extend_from_slice(b"trnm_research_session_merkle_node_v1\0");
            node.extend_from_slice(&left);
            node.extend_from_slice(&right);
            next.push(Sha256::digest(&node).into());
        }
        level = next;
    }
    Ok(format!(
        "sha256:{}",
        level[0]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

pub fn research_session_event_root(events: &[ResearchSessionEventV1]) -> Result<String, String> {
    validate_research_session_archive(events)?;
    research_session_event_root_from_commitments(
        &events
            .iter()
            .map(|event| ResearchSessionEventCommitmentV1 {
                sequence: event.sequence,
                event_hash: event.event_hash.clone(),
            })
            .collect::<Vec<_>>(),
    )
}

pub fn research_session_archive_frame(
    events: &[ResearchSessionEventV1],
) -> Result<Vec<u8>, String> {
    validate_research_session_archive(events)?;
    let mut frame = CanonicalFrame::new("trnm_research_session_event_archive_v1")
        .u64(u64::try_from(events.len()).map_err(|_| "archive too large")?);
    for event in events {
        let facts = research_session_event_facts_frame(event)?;
        frame = frame
            .bytes(&facts)?
            .bytes(&decode_digest(&event.event_hash)?)?;
    }
    Ok(frame.finish())
}

pub fn research_session_archive_hash(events: &[ResearchSessionEventV1]) -> Result<String, String> {
    research_session_archive_frame(events).map(|frame| sha256_digest(&frame))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchSessionTerminalFactsV1 {
    pub result_code: String,
    pub paper_bundle_hash: String,
    pub paper_release_candidate_hash: String,
    pub contribution_ledger_hash: String,
}

pub fn research_session_terminal_facts_frame(
    facts: &ResearchSessionTerminalFactsV1,
) -> Result<Vec<u8>, String> {
    validate_text("result_code", &facts.result_code)?;
    Ok(
        CanonicalFrame::new("trnm_research_session_terminal_facts_v1")
            .string(&facts.result_code)?
            .digest(&facts.paper_bundle_hash)?
            .digest(&facts.paper_release_candidate_hash)?
            .digest(&facts.contribution_ledger_hash)?
            .finish(),
    )
}

pub fn research_session_commitment_id(
    session_id: &str,
    event_root: &str,
    archive_hash: &str,
) -> Result<String, String> {
    validate_logical_id("session_id", session_id)?;
    let frame = CanonicalFrame::new("trnm_research_session_commitment_id_v1")
        .string(session_id)?
        .digest(event_root)?
        .digest(archive_hash)?
        .finish();
    Ok(sha256_digest(&frame))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchSessionCompletionV1 {
    pub schema: String,
    pub commitment_id: String,
    pub session_id: String,
    pub team_id: String,
    pub paper_project_id: String,
    pub challenge_id: String,
    pub roster_version: u64,
    pub roster_root: String,
    pub terminal_facts: ResearchSessionTerminalFactsV1,
    pub event_count: u64,
    pub event_root: String,
    pub archive_hash: String,
    pub ruleset_hash: String,
    pub challenge_snapshot_hash: String,
    pub completed_at_unix: i64,
    pub authority_key_id: String,
    pub signature: String,
}

pub fn research_session_completion_signing_bytes(
    completion: &ResearchSessionCompletionV1,
) -> Result<Vec<u8>, String> {
    if completion.schema != RESEARCH_SESSION_COMPLETION_V1 {
        return Err(format!(
            "unsupported completion schema {}",
            completion.schema
        ));
    }
    if completion.roster_version == 0 || completion.event_count == 0 {
        return Err("roster_version and event_count must be positive".to_string());
    }
    for (field, value) in [
        ("commitment_id", completion.commitment_id.as_str()),
        ("session_id", completion.session_id.as_str()),
        ("team_id", completion.team_id.as_str()),
        ("paper_project_id", completion.paper_project_id.as_str()),
        ("challenge_id", completion.challenge_id.as_str()),
        ("authority_key_id", completion.authority_key_id.as_str()),
    ] {
        validate_text(field, value)?;
    }
    validate_logical_id("session_id", &completion.session_id)?;
    if completion.completed_at_unix < 0 {
        return Err("completed_at_unix must be non-negative".to_string());
    }
    if !completion.signature.is_empty() {
        decode_base64_exact::<64>("signature", &completion.signature)?;
    }
    let expected_commitment = research_session_commitment_id(
        &completion.session_id,
        &completion.event_root,
        &completion.archive_hash,
    )?;
    if completion.commitment_id != expected_commitment {
        return Err("commitment_id does not match session/event/archive roots".to_string());
    }
    let terminal = research_session_terminal_facts_frame(&completion.terminal_facts)?;
    Ok(
        CanonicalFrame::new("trnm_research_session_completed_signature_v1")
            .string(&completion.schema)?
            .digest(&completion.commitment_id)?
            .string(&completion.session_id)?
            .string(&completion.team_id)?
            .string(&completion.paper_project_id)?
            .string(&completion.challenge_id)?
            .u64(completion.roster_version)
            .digest(&completion.roster_root)?
            .bytes(&terminal)?
            .u64(completion.event_count)
            .digest(&completion.event_root)?
            .digest(&completion.archive_hash)?
            .digest(&completion.ruleset_hash)?
            .digest(&completion.challenge_snapshot_hash)?
            .i64(completion.completed_at_unix)
            .string(&completion.authority_key_id)?
            .finish(),
    )
}

pub fn verify_research_session_completion(
    completion: &ResearchSessionCompletionV1,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let bytes = research_session_completion_signing_bytes(completion)?;
    let signature = decode_base64_exact::<64>("signature", &completion.signature)?;
    verifying_key
        .verify(&bytes, &Signature::from_bytes(&signature))
        .map_err(|_| "research session completion signature verification failed".to_string())
}

pub fn verify_research_session_completion_against_archive(
    completion: &ResearchSessionCompletionV1,
    events: &[ResearchSessionEventV1],
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    verify_research_session_completion(completion, verifying_key)?;
    validate_research_session_archive(events)?;
    if completion.event_count != u64::try_from(events.len()).map_err(|_| "archive too large")? {
        return Err("completion event_count does not cover the archive".to_string());
    }
    let last = events
        .last()
        .ok_or_else(|| "event archive is empty".to_string())?;
    if last.event_type != "research_session_completed"
        || last.participant_slot != 0
        || last.action_type != "server.complete"
        || last.payload_type != "trnm.research-session.terminal-facts.v1"
    {
        return Err("archive has no canonical terminal completion event".to_string());
    }
    if last.session_id != completion.session_id
        || last.team_id != completion.team_id
        || last.paper_project_id != completion.paper_project_id
        || last.challenge_id != completion.challenge_id
        || last.roster_version != completion.roster_version
        || last.reference_hash != completion.terminal_facts.paper_release_candidate_hash
        || last.occurred_at_unix != completion.completed_at_unix
    {
        return Err("terminal completion event identity differs from credential".to_string());
    }
    let terminal = research_session_terminal_facts_frame(&completion.terminal_facts)?;
    let last_payload = decode_base64("payload", &last.payload)?;
    if last_payload != terminal || last.causation_id != sha256_digest(&terminal) {
        return Err("terminal completion event payload differs from credential".to_string());
    }
    let event_root = research_session_event_root(events)?;
    if event_root != completion.event_root {
        return Err("completion event_root does not cover the archive".to_string());
    }
    let archive_hash = research_session_archive_hash(events)?;
    if archive_hash != completion.archive_hash {
        return Err("completion archive_hash does not cover the archive".to_string());
    }
    let commitment_id =
        research_session_commitment_id(&completion.session_id, &event_root, &archive_hash)?;
    if commitment_id != completion.commitment_id {
        return Err("completion commitment_id does not bind the archive".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedNakamaCompletionReceiptV1 {
    pub schema: String,
    pub commitment_id: String,
    pub session_id: String,
    pub team_id: Uuid,
    pub paper_project_id: Uuid,
    pub challenge_id: Uuid,
    pub roster_version: u64,
    pub roster_root: String,
    pub event_count: u64,
    pub event_root: String,
    pub archive_hash: String,
    pub ruleset_hash: String,
    pub challenge_snapshot_hash: String,
    pub nakama_authority_key_id: String,
    pub terminal_facts: ResearchSessionTerminalFactsV1,
    pub verified_at_unix: i64,
    pub issuer_key_id: String,
    pub signature: String,
}

pub fn nakama_completion_receipt_signing_bytes(
    receipt: &SignedNakamaCompletionReceiptV1,
) -> Result<Vec<u8>, String> {
    if receipt.schema != NAKAMA_COMPLETION_RECEIPT_V1
        || receipt.roster_version == 0
        || receipt.event_count == 0
        || receipt.verified_at_unix < 0
    {
        return Err(
            "Nakama completion receipt schema, version, count, or time is invalid".to_string(),
        );
    }
    validate_logical_id("session_id", &receipt.session_id)?;
    validate_text("nakama_authority_key_id", &receipt.nakama_authority_key_id)?;
    validate_text("issuer_key_id", &receipt.issuer_key_id)?;
    if !receipt.signature.is_empty() {
        decode_base64_exact::<64>("signature", &receipt.signature)?;
    }
    let terminal = research_session_terminal_facts_frame(&receipt.terminal_facts)?;
    Ok(
        CanonicalFrame::new("hepta_nakama_research_session_completion_receipt_v1")
            .string(&receipt.schema)?
            .digest(&receipt.commitment_id)?
            .string(&receipt.session_id)?
            .string(&receipt.team_id.to_string())?
            .string(&receipt.paper_project_id.to_string())?
            .string(&receipt.challenge_id.to_string())?
            .u64(receipt.roster_version)
            .digest(&receipt.roster_root)?
            .u64(receipt.event_count)
            .digest(&receipt.event_root)?
            .digest(&receipt.archive_hash)?
            .digest(&receipt.ruleset_hash)?
            .digest(&receipt.challenge_snapshot_hash)?
            .string(&receipt.nakama_authority_key_id)?
            .bytes(&terminal)?
            .i64(receipt.verified_at_unix)
            .string(&receipt.issuer_key_id)?
            .finish(),
    )
}

pub fn sign_nakama_completion_receipt(
    mut receipt: SignedNakamaCompletionReceiptV1,
    signing_key: &SigningKey,
) -> Result<SignedNakamaCompletionReceiptV1, String> {
    receipt.signature.clear();
    let bytes = nakama_completion_receipt_signing_bytes(&receipt)?;
    receipt.signature = BASE64.encode(signing_key.sign(&bytes).to_bytes());
    Ok(receipt)
}

pub fn verify_nakama_completion_receipt(
    receipt: &SignedNakamaCompletionReceiptV1,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let bytes = nakama_completion_receipt_signing_bytes(receipt)?;
    let signature = decode_base64_exact::<64>("signature", &receipt.signature)?;
    verifying_key
        .verify(&bytes, &Signature::from_bytes(&signature))
        .map_err(|_| "Hepta Nakama completion receipt signature verification failed".to_string())
}

/// Hepta's signed acknowledgement that one complete, ordered authorization
/// epoch was atomically consumed by Nakama.  Returning the unsigned mutable
/// authorization set is deliberately insufficient: the signature binds the
/// state transition and its timestamp so Nakama can safely persist and
/// re-verify the acknowledgement after restart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedAuthorizationSetConsumptionReceiptV1 {
    pub schema: String,
    pub session_id: String,
    pub team_id: Uuid,
    pub paper_project_id: Uuid,
    pub challenge_id: Uuid,
    pub session_roster_version: u64,
    pub roster_root: String,
    pub authorization_ids: Vec<Uuid>,
    pub consumed_at_unix: i64,
    pub issuer_key_id: String,
    pub signature: String,
}

pub fn authorization_set_consumption_receipt_signing_bytes(
    receipt: &SignedAuthorizationSetConsumptionReceiptV1,
) -> Result<Vec<u8>, String> {
    if receipt.schema != AUTHORIZATION_SET_CONSUMPTION_RECEIPT_V1
        || receipt.session_roster_version == 0
        || receipt.consumed_at_unix < 0
        || !(3..=5).contains(&receipt.authorization_ids.len())
    {
        return Err(
            "authorization consumption receipt schema, epoch, member count, or time is invalid"
                .to_string(),
        );
    }
    validate_logical_id("session_id", &receipt.session_id)?;
    validate_text("issuer_key_id", &receipt.issuer_key_id)?;
    let mut unique = std::collections::HashSet::new();
    if receipt
        .authorization_ids
        .iter()
        .any(|authorization_id| !unique.insert(*authorization_id))
    {
        return Err("authorization_ids must be unique and ordered".to_string());
    }
    if !receipt.signature.is_empty() {
        decode_base64_exact::<64>("signature", &receipt.signature)?;
    }

    let mut frame =
        CanonicalFrame::new("hepta_research_session_authorization_set_consumption_receipt_v1")
            .string(&receipt.schema)?
            .string(&receipt.session_id)?
            .string(&receipt.team_id.to_string())?
            .string(&receipt.paper_project_id.to_string())?
            .string(&receipt.challenge_id.to_string())?
            .u64(receipt.session_roster_version)
            .digest(&receipt.roster_root)?
            .u32(
                u32::try_from(receipt.authorization_ids.len())
                    .map_err(|_| "authorization_ids exceeds uint32 length".to_string())?,
            );
    for authorization_id in &receipt.authorization_ids {
        frame = frame.string(&authorization_id.to_string())?;
    }
    Ok(frame
        .i64(receipt.consumed_at_unix)
        .string(&receipt.issuer_key_id)?
        .finish())
}

pub fn sign_authorization_set_consumption_receipt(
    mut receipt: SignedAuthorizationSetConsumptionReceiptV1,
    signing_key: &SigningKey,
) -> Result<SignedAuthorizationSetConsumptionReceiptV1, String> {
    receipt.signature.clear();
    let bytes = authorization_set_consumption_receipt_signing_bytes(&receipt)?;
    receipt.signature = BASE64.encode(signing_key.sign(&bytes).to_bytes());
    Ok(receipt)
}

pub fn verify_authorization_set_consumption_receipt(
    receipt: &SignedAuthorizationSetConsumptionReceiptV1,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let bytes = authorization_set_consumption_receipt_signing_bytes(receipt)?;
    let signature = decode_base64_exact::<64>("signature", &receipt.signature)?;
    verifying_key
        .verify(&bytes, &Signature::from_bytes(&signature))
        .map_err(|_| {
            "Hepta authorization consumption receipt signature verification failed".to_string()
        })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperReleaseAuthorV2 {
    pub author_order: u32,
    pub participant_slot: u32,
    pub player_id: Uuid,
    pub display_name: String,
    pub credit_roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct SectionMaterializationEntryV1 {
    pub section_key: String,
    pub base_paper_revision_id: Uuid,
    pub head_section_revision_id: Uuid,
    pub merge_id: Uuid,
    pub patch_manifest_id: Uuid,
    pub patch_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SectionMaterializationDescriptorV1 {
    pub schema: String,
    pub paper_project_id: Uuid,
    pub revision_id: Uuid,
    pub parent_revision_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_materialization_root: Option<String>,
    pub sections: Vec<SectionMaterializationEntryV1>,
}

pub fn section_materialization_root(
    descriptor: &SectionMaterializationDescriptorV1,
) -> Result<String, String> {
    if descriptor.schema != SECTION_MATERIALIZATION_V1 {
        return Err(format!(
            "unsupported section materialization schema {}",
            descriptor.schema
        ));
    }
    if descriptor.paper_project_id.is_nil() || descriptor.revision_id.is_nil() {
        return Err("section materialization paper and revision ids must be non-nil".to_string());
    }
    if descriptor.parent_revision_id == Some(descriptor.revision_id) {
        return Err("section materialization revision cannot parent itself".to_string());
    }
    if descriptor.parent_revision_id.is_none() && descriptor.parent_materialization_root.is_some() {
        return Err(
            "root section materialization cannot name a parent materialization root".to_string(),
        );
    }
    if let Some(root) = descriptor.parent_materialization_root.as_deref() {
        decode_digest(root)?;
    }
    let mut canonical_sections = descriptor.sections.clone();
    canonical_sections.sort();
    if canonical_sections != descriptor.sections {
        return Err(
            "section materialization entries must be canonically sorted and unique".to_string(),
        );
    }
    for pair in canonical_sections.windows(2) {
        if pair[0].section_key == pair[1].section_key {
            return Err("section materialization section keys must be unique".to_string());
        }
    }
    for entry in &canonical_sections {
        validate_logical_id("section_key", &entry.section_key)?;
        if entry.base_paper_revision_id.is_nil()
            || entry.head_section_revision_id.is_nil()
            || entry.merge_id.is_nil()
            || entry.patch_manifest_id.is_nil()
        {
            return Err("section materialization ids must be non-nil".to_string());
        }
        decode_digest(&entry.patch_hash)?;
    }
    canonical_json_sha256(descriptor)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperReleaseCandidateV2 {
    pub schema: String,
    pub paper_project_id: Uuid,
    pub revision_id: Uuid,
    pub team_id: Uuid,
    pub challenge_id: Uuid,
    pub ruleset_hash: String,
    pub challenge_snapshot_hash: String,
    pub roster_version: u64,
    pub title: String,
    pub abstract_text: String,
    pub target_format: String,
    pub source_manifest_hash: String,
    pub artifact_manifest_hash: String,
    pub bibliography_hash: String,
    pub claim_evidence_graph_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_materialization_root: Option<String>,
    pub collaboration_compact_hash: String,
    pub research_protocol_snapshot_hash: String,
    pub ethics_disclosure_hash: String,
    pub coi_disclosure_hash: String,
    pub contribution_ledger_hash: String,
    pub ai_disclosure_hash: String,
    pub license: String,
    pub authors: Vec<PaperReleaseAuthorV2>,
}

pub fn paper_release_candidate_frame(
    candidate: &PaperReleaseCandidateV2,
) -> Result<Vec<u8>, String> {
    if candidate.schema != PAPER_RELEASE_CANDIDATE_V2 {
        return Err(format!(
            "unsupported release candidate schema {}",
            candidate.schema
        ));
    }
    if candidate.roster_version == 0 || !(3..=5).contains(&candidate.authors.len()) {
        return Err("release candidate must bind a positive 3-5 member roster".to_string());
    }
    for (field, value) in [
        ("title", candidate.title.as_str()),
        ("abstract_text", candidate.abstract_text.as_str()),
        ("target_format", candidate.target_format.as_str()),
        ("license", candidate.license.as_str()),
    ] {
        validate_text(field, value)?;
    }
    let mut authors = candidate.authors.clone();
    authors.sort_by_key(|author| author.author_order);
    let mut participant_slots = std::collections::HashSet::new();
    let mut frame = CanonicalFrame::new("hepta_paper_raid_release_candidate_v2")
        .string(&candidate.schema)?
        .string(&candidate.paper_project_id.to_string())?
        .string(&candidate.revision_id.to_string())?
        .string(&candidate.team_id.to_string())?
        .string(&candidate.challenge_id.to_string())?
        .digest(&candidate.ruleset_hash)?
        .digest(&candidate.challenge_snapshot_hash)?
        .u64(candidate.roster_version)
        .string(&candidate.title)?
        .string(&candidate.abstract_text)?
        .string(&candidate.target_format)?
        .digest(&candidate.source_manifest_hash)?
        .digest(&candidate.artifact_manifest_hash)?
        .digest(&candidate.bibliography_hash)?
        .digest(&candidate.claim_evidence_graph_hash)?
        .digest(&candidate.collaboration_compact_hash)?
        .digest(&candidate.research_protocol_snapshot_hash)?
        .digest(&candidate.ethics_disclosure_hash)?
        .digest(&candidate.coi_disclosure_hash)?
        .digest(&candidate.contribution_ledger_hash)?
        .digest(&candidate.ai_disclosure_hash)?
        .string(&candidate.license)?
        .u32(u32::try_from(authors.len()).expect("five authors fit u32"));
    for (index, author) in authors.iter().enumerate() {
        let expected_order = u32::try_from(index + 1).expect("five authors fit u32");
        if author.author_order != expected_order || !(1..=5).contains(&author.participant_slot) {
            return Err(
                "release candidate author_order must be gapless and roster slots valid".to_string(),
            );
        }
        if !participant_slots.insert(author.participant_slot) {
            return Err("release candidate participant slots must be unique".to_string());
        }
        validate_text("display_name", &author.display_name)?;
        if author.credit_roles.is_empty() {
            return Err("every author must have at least one CRediT role".to_string());
        }
        let mut roles = author.credit_roles.clone();
        roles.sort();
        roles.dedup();
        if roles.len() != author.credit_roles.len() {
            return Err("author CRediT roles must be unique".to_string());
        }
        frame = frame
            .u32(author.author_order)
            .u32(author.participant_slot)
            .string(&author.player_id.to_string())?
            .string(&author.display_name)?
            .u32(u32::try_from(roles.len()).map_err(|_| "too many CRediT roles")?);
        for role in roles {
            validate_text("credit_role", &role)?;
            if !matches!(
                role.as_str(),
                "conceptualization"
                    | "data_curation"
                    | "formal_analysis"
                    | "funding_acquisition"
                    | "investigation"
                    | "methodology"
                    | "project_administration"
                    | "resources"
                    | "software"
                    | "supervision"
                    | "validation"
                    | "visualization"
                    | "writing_original_draft"
                    | "writing_review_editing"
            ) {
                return Err(format!("unsupported CRediT role {role}"));
            }
            frame = frame.string(&role)?;
        }
    }
    if let Some(root) = candidate.section_materialization_root.as_deref() {
        frame = frame.string(SECTION_MATERIALIZATION_V1)?.digest(root)?;
    }
    Ok(frame.finish())
}

pub fn paper_release_candidate_hash(candidate: &PaperReleaseCandidateV2) -> Result<String, String> {
    paper_release_candidate_frame(candidate).map(|bytes| sha256_digest(&bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthorshipConsentSigningV2 {
    pub schema: String,
    pub consent_id: Uuid,
    pub paper_project_id: Uuid,
    pub revision_id: Uuid,
    pub player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub release_candidate_hash: String,
    pub signed_at_unix: i64,
}

pub fn authorship_consent_signing_bytes(
    consent: &AuthorshipConsentSigningV2,
) -> Result<Vec<u8>, String> {
    if consent.schema != AUTHORSHIP_CONSENT_V2 {
        return Err(format!("unsupported consent schema {}", consent.schema));
    }
    if consent.signed_at_unix < 0 {
        return Err("signed_at_unix must be non-negative".to_string());
    }
    validate_text("signing_key_id", &consent.signing_key_id)?;
    let public_key = decode_base64_exact::<32>("signing_public_key", &consent.signing_public_key)?;
    if sha256_digest(&public_key) != consent.signing_public_key_hash {
        return Err("signing_public_key_hash does not match key snapshot".to_string());
    }
    Ok(
        CanonicalFrame::new("hepta_paper_raid_authorship_consent_v2")
            .string(&consent.schema)?
            .string(&consent.consent_id.to_string())?
            .string(&consent.paper_project_id.to_string())?
            .string(&consent.revision_id.to_string())?
            .string(&consent.player_id.to_string())?
            .string(&consent.signing_key_id)?
            .bytes(&public_key)?
            .digest(&consent.signing_public_key_hash)?
            .digest(&consent.release_candidate_hash)?
            .i64(consent.signed_at_unix)
            .finish(),
    )
}

pub fn sign_authorship_consent(
    consent: &AuthorshipConsentSigningV2,
    signing_key: &SigningKey,
) -> Result<String, String> {
    authorship_consent_signing_bytes(consent)
        .map(|bytes| BASE64.encode(signing_key.sign(&bytes).to_bytes()))
}

pub fn verify_authorship_consent_signature(
    consent: &AuthorshipConsentSigningV2,
    signature: &str,
) -> Result<(), String> {
    let bytes = authorship_consent_signing_bytes(consent)?;
    let public_key = decode_base64_exact::<32>("signing_public_key", &consent.signing_public_key)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| "authorship public key is not valid Ed25519".to_string())?;
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(&bytes, &Signature::from_bytes(&signature))
        .map_err(|_| "authorship consent signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TeamMemberAcceptanceSigningV2 {
    pub schema: String,
    pub acceptance_id: Uuid,
    pub team_id: Uuid,
    pub challenge_id: Uuid,
    pub roster_version: u64,
    pub participant_slot: u32,
    pub player_id: Uuid,
    pub binding_id: Uuid,
    pub agent_id: String,
    pub role: String,
    pub collaboration_compact_hash: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub accepted_at_unix: i64,
}

pub fn team_member_acceptance_signing_bytes(
    acceptance: &TeamMemberAcceptanceSigningV2,
) -> Result<Vec<u8>, String> {
    if acceptance.schema != TEAM_MEMBER_ACCEPTANCE_V2 {
        return Err(format!(
            "unsupported team acceptance schema {}",
            acceptance.schema
        ));
    }
    if acceptance.roster_version == 0
        || !(1..=5).contains(&acceptance.participant_slot)
        || acceptance.accepted_at_unix < 0
    {
        return Err("team acceptance roster, slot, or time is invalid".to_string());
    }
    for (field, value) in [
        ("agent_id", acceptance.agent_id.as_str()),
        ("role", acceptance.role.as_str()),
        ("signing_key_id", acceptance.signing_key_id.as_str()),
    ] {
        validate_text(field, value)?;
    }
    let public_key =
        decode_base64_exact::<32>("signing_public_key", &acceptance.signing_public_key)?;
    if sha256_digest(&public_key) != acceptance.signing_public_key_hash {
        return Err("signing_public_key_hash does not match key snapshot".to_string());
    }
    Ok(
        CanonicalFrame::new("hepta_paper_raid_team_member_acceptance_v2")
            .string(&acceptance.schema)?
            .string(&acceptance.acceptance_id.to_string())?
            .string(&acceptance.team_id.to_string())?
            .string(&acceptance.challenge_id.to_string())?
            .u64(acceptance.roster_version)
            .u32(acceptance.participant_slot)
            .string(&acceptance.player_id.to_string())?
            .string(&acceptance.binding_id.to_string())?
            .string(&acceptance.agent_id)?
            .string(&acceptance.role)?
            .digest(&acceptance.collaboration_compact_hash)?
            .string(&acceptance.signing_key_id)?
            .bytes(&public_key)?
            .digest(&acceptance.signing_public_key_hash)?
            .i64(acceptance.accepted_at_unix)
            .finish(),
    )
}

pub fn verify_team_member_acceptance_signature(
    acceptance: &TeamMemberAcceptanceSigningV2,
    signature: &str,
) -> Result<(), String> {
    let bytes = team_member_acceptance_signing_bytes(acceptance)?;
    let public_key =
        decode_base64_exact::<32>("signing_public_key", &acceptance.signing_public_key)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| "team acceptance public key is not valid Ed25519".to_string())?;
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(&bytes, &Signature::from_bytes(&signature))
        .map_err(|_| "team member acceptance signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperBundleAuthorConsentV2 {
    pub author_order: u32,
    pub participant_slot: u32,
    pub player_id: Uuid,
    pub consent_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperBundleV2 {
    pub schema: String,
    pub release_candidate: PaperReleaseCandidateV2,
    pub release_candidate_hash: String,
    pub author_consents: Vec<PaperBundleAuthorConsentV2>,
    pub paper_bundle_hash: String,
}

/// One immutable CAS member disclosed to an independently assigned Review Raid actor.
///
/// `download_path` is deliberately a fixed local route rather than an arbitrary URI.  The
/// assignment, bundle, object and task bindings are carried in the signed Agent request query;
/// consumers must never treat this value as a general-purpose URL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct FrozenReviewObjectV1 {
    pub object_key: String,
    pub logical_path: String,
    pub role: String,
    pub digest: String,
    pub size_bytes: u64,
    pub media_type: String,
    pub download_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenReviewExecutionPlanV1 {
    pub schema: String,
    pub kind: String,
    pub adapter: String,
    pub evaluator_version: String,
    pub entrypoint: String,
    pub timeout_ms: u64,
    pub seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenReviewExecutionPolicyV1 {
    pub schema: String,
    pub kind: String,
    pub adapter: String,
    pub timeout_ms: u64,
    pub seed: u64,
}

/// The scientific authority returned by Hepta before the Consumer BFF reads any CAS bytes.
///
/// This record deliberately is not an executable bundle.  It binds one live independent
/// assignment to the release ArtifactManifest and to the two challenge-manifest digests.  A BFF
/// may resolve those digests, but may not add, remove, or rewrite any ArtifactManifest member.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenReviewAuthorityV1 {
    pub schema: String,
    pub authority_hash: String,
    pub assignment_id: Uuid,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub review_round: u64,
    pub slot: String,
    pub assignment_version: u64,
    pub expires_at: String,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub artifact_manifest_hash: String,
    pub evaluator_manifest_hash: String,
    pub dataset_manifest_hash: String,
    pub artifact_objects: Vec<FrozenReviewObjectV1>,
    pub execution_policy: FrozenReviewExecutionPolicyV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengeManifestObjectV1 {
    pub cas_uri: String,
    pub media_type: String,
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengeEvaluatorManifestV1 {
    pub entrypoint: String,
    pub frozen: bool,
    pub objects: Vec<ChallengeManifestObjectV1>,
    pub pack_id: String,
    pub runtime: String,
    pub schema: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengeDatasetManifestV1 {
    pub objects: Vec<ChallengeManifestObjectV1>,
    pub pack_id: String,
    pub schema: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengePackContentContractV1 {
    pub difficulty: String,
    pub duration_seconds: u64,
    pub modifiers: Vec<String>,
    pub objective: String,
    pub risks: Vec<String>,
    pub victory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengePackDeploymentV1 {
    pub blocker: String,
    pub cas_seeded: bool,
    pub open_status_allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengePackObjectV1 {
    pub cas_uri: String,
    pub media_type: String,
    pub path: String,
    pub role: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengePackManifestV1 {
    pub content_contract: ChallengePackContentContractV1,
    pub dataset_manifest_sha256: String,
    pub deployment: ChallengePackDeploymentV1,
    pub evaluator_manifest_sha256: String,
    pub objects: Vec<ChallengePackObjectV1>,
    pub pack_id: String,
    pub ruleset_version: String,
    pub schema: String,
    pub seed: u64,
    pub template: String,
}

/// Immutable material authority copied from the append-only Challenge Pack activation record
/// when a Paper starts.  It carries only digest pins; the Consumer BFF must independently resolve
/// the exact CAS bytes and may not infer a pack from a mutable catalog or from player input.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenChallengeMaterialAuthorityV1 {
    pub schema: String,
    pub authority_hash: String,
    pub activation_id: Uuid,
    pub activation_request_sha256: String,
    pub challenge_id: Uuid,
    pub challenge_snapshot_hash: String,
    pub template: String,
    pub pack_id: String,
    pub pack_manifest_hash: String,
    pub ruleset_version: String,
    pub ruleset_hash: String,
    pub dataset_manifest_hash: String,
    pub evaluator_manifest_hash: String,
}

/// Explicit, non-activation authority for the one historical golden qualification Challenge.
///
/// The historical Challenge predates Challenge Pack activation.  This contract therefore binds
/// the exact qualifying Challenge fields and the four exact player-facing objects directly; it
/// never invents an activation id or claims that a pack activation occurred.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LegacyGoldenQualificationMaterialAuthorityV1 {
    pub schema: String,
    pub authority_hash: String,
    pub qualification_id: String,
    pub challenge_id: Uuid,
    pub challenge_snapshot_hash: String,
    pub challenge_title: String,
    pub challenge_description: String,
    pub challenge_status: String,
    pub ruleset_version: String,
    pub ruleset_hash: String,
    pub ruleset_absent: bool,
    pub dataset_manifest_hash: String,
    pub evaluator_manifest_hash: String,
    pub objects: Vec<AssignedChallengeMaterialObjectV1>,
}

/// The Paper snapshot carries exactly one explicit provenance kind.  The untagged representation
/// preserves the established activation JSON byte shape while the two strict schemas and
/// `deny_unknown_fields` keep the variants disjoint and fail closed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum FrozenChallengeMaterialAuthorityBindingV1 {
    PackActivation(FrozenChallengeMaterialAuthorityV1),
    LegacyGoldenQualification(LegacyGoldenQualificationMaterialAuthorityV1),
}

impl FrozenChallengeMaterialAuthorityBindingV1 {
    pub fn authority_hash(&self) -> &str {
        match self {
            Self::PackActivation(authority) => &authority.authority_hash,
            Self::LegacyGoldenQualification(authority) => &authority.authority_hash,
        }
    }

    pub fn challenge_id(&self) -> Uuid {
        match self {
            Self::PackActivation(authority) => authority.challenge_id,
            Self::LegacyGoldenQualification(authority) => authority.challenge_id,
        }
    }

    pub fn challenge_snapshot_hash(&self) -> &str {
        match self {
            Self::PackActivation(authority) => &authority.challenge_snapshot_hash,
            Self::LegacyGoldenQualification(authority) => &authority.challenge_snapshot_hash,
        }
    }

    pub fn ruleset_version(&self) -> &str {
        match self {
            Self::PackActivation(authority) => &authority.ruleset_version,
            Self::LegacyGoldenQualification(authority) => &authority.ruleset_version,
        }
    }

    pub fn ruleset_hash(&self) -> &str {
        match self {
            Self::PackActivation(authority) => &authority.ruleset_hash,
            Self::LegacyGoldenQualification(authority) => &authority.ruleset_hash,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignedChallengeMaterialObjectV1 {
    pub object_key: String,
    pub source_path: String,
    pub logical_path: String,
    pub role: String,
    pub digest: String,
    pub size_bytes: u64,
    pub media_type: String,
    pub download_path: String,
}

/// A transport-only projection for one current Author work-item assignment.  Possession of this
/// descriptor is not authorization: every object read must re-fetch the Paper Room and reproduce
/// this bundle from the current binding/player/work-item tuple before CAS bytes are returned.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignedChallengeMaterialBundleV1 {
    pub schema: String,
    pub bundle_hash: String,
    pub authority: FrozenChallengeMaterialAuthorityBindingV1,
    pub authority_hash: String,
    pub paper_project_id: Uuid,
    pub challenge_ruleset_snapshot_hash: String,
    pub binding_id: Uuid,
    pub player_id: Uuid,
    pub work_item_id: Uuid,
    pub work_item_version: u64,
    pub objects: Vec<AssignedChallengeMaterialObjectV1>,
}

pub fn parse_challenge_evaluator_manifest(
    bytes: &[u8],
    expected_digest: &str,
) -> Result<ChallengeEvaluatorManifestV1, String> {
    if sha256_digest(bytes) != expected_digest {
        return Err("evaluator manifest bytes do not match the Hepta digest pin".to_string());
    }
    let manifest: ChallengeEvaluatorManifestV1 = serde_json::from_slice(bytes)
        .map_err(|_| "evaluator manifest is not strict JSON v1".to_string())?;
    if manifest.schema != "hepta.challenge_pack.evaluator_manifest.v1"
        || !manifest.frozen
        || manifest.runtime != "python3-stdlib"
        || !safe_review_path(&manifest.entrypoint)
    {
        return Err("evaluator manifest authority fields are invalid".to_string());
    }
    validate_challenge_manifest_objects(&manifest.objects)?;
    if manifest
        .objects
        .iter()
        .filter(|object| object.path == manifest.entrypoint)
        .count()
        != 1
    {
        return Err("evaluator manifest entrypoint must name exactly one member".to_string());
    }
    validate_logical_id("pack_id", &manifest.pack_id)?;
    Ok(manifest)
}

pub fn parse_challenge_dataset_manifest(
    bytes: &[u8],
    expected_digest: &str,
) -> Result<ChallengeDatasetManifestV1, String> {
    if sha256_digest(bytes) != expected_digest {
        return Err("dataset manifest bytes do not match the Hepta digest pin".to_string());
    }
    let manifest: ChallengeDatasetManifestV1 = serde_json::from_slice(bytes)
        .map_err(|_| "dataset manifest is not strict JSON v1".to_string())?;
    if manifest.schema != "hepta.challenge_pack.dataset_manifest.v1" {
        return Err("dataset manifest schema is invalid".to_string());
    }
    validate_logical_id("pack_id", &manifest.pack_id)?;
    validate_challenge_manifest_objects(&manifest.objects)?;
    Ok(manifest)
}

pub fn parse_challenge_pack_manifest(
    bytes: &[u8],
    expected_digest: &str,
) -> Result<ChallengePackManifestV1, String> {
    if sha256_digest(bytes) != expected_digest {
        return Err("pack manifest bytes do not match the Hepta activation digest pin".to_string());
    }
    let manifest: ChallengePackManifestV1 = serde_json::from_slice(bytes)
        .map_err(|_| "pack manifest is not strict JSON v1".to_string())?;
    if manifest.schema != "hepta.challenge_pack.v1"
        || manifest.seed > JSON_SAFE_U64_MAX
        || manifest.content_contract.duration_seconds == 0
        || manifest.content_contract.duration_seconds > 7 * 24 * 60 * 60
    {
        return Err("pack manifest authority fields are invalid".to_string());
    }
    for (field, value) in [
        ("pack_id", manifest.pack_id.as_str()),
        ("ruleset_version", manifest.ruleset_version.as_str()),
        ("template", manifest.template.as_str()),
        ("difficulty", manifest.content_contract.difficulty.as_str()),
        ("deployment.blocker", manifest.deployment.blocker.as_str()),
    ] {
        validate_logical_id(field, value)?;
    }
    for (field, value) in [
        ("objective", manifest.content_contract.objective.as_str()),
        ("victory", manifest.content_contract.victory.as_str()),
    ] {
        validate_text(field, value)?;
    }
    validate_pack_labels("modifier", &manifest.content_contract.modifiers)?;
    validate_pack_labels("risk", &manifest.content_contract.risks)?;
    decode_digest(&manifest.dataset_manifest_sha256)?;
    decode_digest(&manifest.evaluator_manifest_sha256)?;
    validate_challenge_pack_objects(
        &manifest.objects,
        &manifest.dataset_manifest_sha256,
        &manifest.evaluator_manifest_sha256,
    )?;
    Ok(manifest)
}

fn validate_pack_labels(field: &str, values: &[String]) -> Result<(), String> {
    if values.is_empty() || values.len() > 8 {
        return Err(format!("pack {field} count is invalid"));
    }
    let mut unique = HashSet::new();
    for value in values {
        validate_logical_id(field, value)?;
        if !unique.insert(value) {
            return Err(format!("pack contains a duplicate {field}"));
        }
    }
    Ok(())
}

fn validate_challenge_pack_objects(
    objects: &[ChallengePackObjectV1],
    dataset_manifest_hash: &str,
    evaluator_manifest_hash: &str,
) -> Result<(), String> {
    const ROLES: [&str; 8] = [
        "license",
        "baseline_code",
        "playable_brief",
        "dataset_manifest",
        "dataset",
        "evaluator_manifest",
        "frozen_evaluator",
        "result_explanation",
    ];
    if objects.len() < ROLES.len() || objects.len() > 32 {
        return Err("pack manifest member count is invalid".to_string());
    }
    let mut paths = HashSet::new();
    let mut digests = HashSet::new();
    for object in objects {
        if !safe_review_path(&object.path)
            || object.size == 0
            || object.size > 16 * 1024 * 1024
            || object.media_type.is_empty()
            || object.media_type.len() > 128
            || object
                .media_type
                .bytes()
                .any(|byte| byte.is_ascii_control())
            || !ROLES.contains(&object.role.as_str())
            || !paths.insert(&object.path)
            || !digests.insert(&object.sha256)
        {
            return Err(
                "pack manifest contains an unsafe, unknown, or duplicate member".to_string(),
            );
        }
        decode_digest(&object.sha256)?;
        let raw = object
            .sha256
            .strip_prefix("sha256:")
            .ok_or_else(|| "pack manifest member digest is invalid".to_string())?;
        if object.cas_uri != format!("cas://sha256/{raw}") {
            return Err("pack manifest CAS URI does not match its member digest".to_string());
        }
    }
    for role in ROLES {
        if objects.iter().filter(|object| object.role == role).count() != 1 {
            return Err(format!(
                "pack manifest must contain exactly one {role} member"
            ));
        }
    }
    for (role, digest) in [
        ("dataset_manifest", dataset_manifest_hash),
        ("evaluator_manifest", evaluator_manifest_hash),
    ] {
        let object = objects
            .iter()
            .find(|object| object.role == role)
            .expect("role count checked above");
        if object.sha256 != digest || object.media_type != "application/json" {
            return Err(format!(
                "pack {role} member disagrees with its top-level pin"
            ));
        }
    }
    Ok(())
}

fn pack_object_matches_manifest_member(
    object: &ChallengePackObjectV1,
    member: &ChallengeManifestObjectV1,
    role: &str,
) -> bool {
    object.path == member.path
        && object.role == role
        && object.sha256 == member.sha256
        && object.size == member.size
        && object.media_type == member.media_type
        && object.cas_uri == member.cas_uri
}

/// Resolve exactly the four player-facing inputs from the frozen pack/manifests.
///
/// Current P0 packs deliberately contain one dataset, one baseline, one brief and one evaluator.
/// Rejecting wider shapes prevents an apparently harmless manifest expansion from silently
/// changing what the Browser or Bridge receives.
pub fn resolve_challenge_material_objects(
    pack: &ChallengePackManifestV1,
    evaluator: &ChallengeEvaluatorManifestV1,
    dataset: &ChallengeDatasetManifestV1,
) -> Result<Vec<AssignedChallengeMaterialObjectV1>, String> {
    if pack.pack_id != evaluator.pack_id
        || pack.pack_id != dataset.pack_id
        || dataset.objects.len() != 1
        || evaluator.objects.len() > 2
    {
        return Err("pack, evaluator, and dataset manifests do not form one supported pack".into());
    }
    let brief = pack
        .objects
        .iter()
        .filter(|object| object.role == "playable_brief")
        .collect::<Vec<_>>();
    let baseline = pack
        .objects
        .iter()
        .filter(|object| object.role == "baseline_code")
        .collect::<Vec<_>>();
    let frozen_evaluator = pack
        .objects
        .iter()
        .filter(|object| object.role == "frozen_evaluator")
        .collect::<Vec<_>>();
    let dataset_objects = pack
        .objects
        .iter()
        .filter(|object| object.role == "dataset")
        .collect::<Vec<_>>();
    if brief.len() != 1
        || baseline.len() != 1
        || frozen_evaluator.len() != 1
        || dataset_objects.len() != 1
        || brief[0].media_type != "text/markdown; charset=utf-8"
        || baseline[0].media_type != "text/x-python; charset=utf-8"
        || frozen_evaluator[0].media_type != "text/x-python; charset=utf-8"
        || !matches!(
            dataset_objects[0].media_type.as_str(),
            "application/json" | "text/csv; charset=utf-8"
        )
    {
        return Err("pack player-facing material roles or media types are invalid".into());
    }
    if !pack_object_matches_manifest_member(dataset_objects[0], &dataset.objects[0], "dataset") {
        return Err("pack dataset object disagrees with the dataset manifest".into());
    }
    let entrypoint = evaluator
        .objects
        .iter()
        .find(|member| member.path == evaluator.entrypoint)
        .ok_or_else(|| "evaluator entrypoint is absent".to_string())?;
    if !pack_object_matches_manifest_member(frozen_evaluator[0], entrypoint, "frozen_evaluator") {
        return Err("pack evaluator object disagrees with the evaluator manifest".into());
    }
    let support = evaluator
        .objects
        .iter()
        .filter(|member| member.path != evaluator.entrypoint)
        .collect::<Vec<_>>();
    if support.len() > 1
        || support.first().is_some_and(|member| {
            !pack_object_matches_manifest_member(baseline[0], member, "baseline_code")
        })
    {
        return Err("pack baseline object disagrees with evaluator support authority".into());
    }
    let material = |object: &ChallengePackObjectV1,
                    object_key: &str,
                    logical_path: &str|
     -> AssignedChallengeMaterialObjectV1 {
        AssignedChallengeMaterialObjectV1 {
            object_key: object_key.to_string(),
            source_path: object.path.clone(),
            logical_path: logical_path.to_string(),
            role: object.role.clone(),
            digest: object.sha256.clone(),
            size_bytes: object.size,
            media_type: object.media_type.clone(),
            download_path: CHALLENGE_MATERIAL_OBJECT_DOWNLOAD_PATH_V1.to_string(),
        }
    };
    Ok(vec![
        material(brief[0], "brief", "challenge/brief.md"),
        material(
            dataset_objects[0],
            "dataset",
            if dataset_objects[0].media_type == "application/json" {
                "challenge/dataset.json"
            } else {
                "challenge/dataset.csv"
            },
        ),
        material(baseline[0], "baseline", "challenge/baseline.py"),
        material(frozen_evaluator[0], "evaluator", "challenge/evaluator.py"),
    ])
}

fn validate_challenge_manifest_objects(
    objects: &[ChallengeManifestObjectV1],
) -> Result<(), String> {
    if objects.is_empty() || objects.len() > 32 {
        return Err("challenge manifest member count is invalid".to_string());
    }
    let mut paths = HashSet::new();
    let mut digests = HashSet::new();
    for object in objects {
        if !safe_review_path(&object.path)
            || object.size == 0
            || object.size > 16 * 1024 * 1024
            || object.media_type.is_empty()
            || object.media_type.len() > 128
            || object
                .media_type
                .bytes()
                .any(|byte| byte.is_ascii_control())
            || !paths.insert(&object.path)
            || !digests.insert(&object.sha256)
        {
            return Err("challenge manifest contains an unsafe or duplicate member".to_string());
        }
        decode_digest(&object.sha256)?;
        let raw = object
            .sha256
            .strip_prefix("sha256:")
            .ok_or_else(|| "challenge manifest member digest is invalid".to_string())?;
        if object.cas_uri != format!("cas://sha256/{raw}") {
            return Err("challenge manifest CAS URI does not match its member digest".to_string());
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct FrozenChallengeMaterialAuthorityHashFrameV1<'a> {
    schema: &'a str,
    activation_id: Uuid,
    activation_request_sha256: &'a str,
    challenge_id: Uuid,
    challenge_snapshot_hash: &'a str,
    template: &'a str,
    pack_id: &'a str,
    pack_manifest_hash: &'a str,
    ruleset_version: &'a str,
    ruleset_hash: &'a str,
    dataset_manifest_hash: &'a str,
    evaluator_manifest_hash: &'a str,
}

pub fn frozen_challenge_material_authority_hash(
    authority: &FrozenChallengeMaterialAuthorityV1,
) -> Result<String, String> {
    validate_frozen_challenge_material_authority(authority, false)?;
    canonical_json_sha256(&FrozenChallengeMaterialAuthorityHashFrameV1 {
        schema: &authority.schema,
        activation_id: authority.activation_id,
        activation_request_sha256: &authority.activation_request_sha256,
        challenge_id: authority.challenge_id,
        challenge_snapshot_hash: &authority.challenge_snapshot_hash,
        template: &authority.template,
        pack_id: &authority.pack_id,
        pack_manifest_hash: &authority.pack_manifest_hash,
        ruleset_version: &authority.ruleset_version,
        ruleset_hash: &authority.ruleset_hash,
        dataset_manifest_hash: &authority.dataset_manifest_hash,
        evaluator_manifest_hash: &authority.evaluator_manifest_hash,
    })
}

pub fn verify_frozen_challenge_material_authority(
    authority: &FrozenChallengeMaterialAuthorityV1,
) -> Result<(), String> {
    validate_frozen_challenge_material_authority(authority, true)
}

fn validate_frozen_challenge_material_authority(
    authority: &FrozenChallengeMaterialAuthorityV1,
    verify_hash: bool,
) -> Result<(), String> {
    if authority.schema != FROZEN_CHALLENGE_MATERIAL_AUTHORITY_V1
        || authority.activation_id.is_nil()
        || authority.challenge_id.is_nil()
    {
        return Err("frozen challenge material activation binding is invalid".to_string());
    }
    for (field, value) in [
        ("template", authority.template.as_str()),
        ("pack_id", authority.pack_id.as_str()),
        ("ruleset_version", authority.ruleset_version.as_str()),
    ] {
        validate_logical_id(field, value)?;
    }
    for digest in [
        &authority.activation_request_sha256,
        &authority.challenge_snapshot_hash,
        &authority.pack_manifest_hash,
        &authority.ruleset_hash,
        &authority.dataset_manifest_hash,
        &authority.evaluator_manifest_hash,
    ] {
        decode_digest(digest)?;
    }
    if verify_hash {
        decode_digest(&authority.authority_hash)?;
        if frozen_challenge_material_authority_hash(authority)? != authority.authority_hash {
            return Err("frozen challenge material authority hash mismatch".to_string());
        }
    }
    Ok(())
}

pub fn legacy_golden_qualification_material_objects() -> Vec<AssignedChallengeMaterialObjectV1> {
    let object = |object_key: &str,
                  source_path: &str,
                  logical_path: &str,
                  role: &str,
                  digest: &str,
                  size_bytes: u64,
                  media_type: &str| AssignedChallengeMaterialObjectV1 {
        object_key: object_key.to_string(),
        source_path: source_path.to_string(),
        logical_path: logical_path.to_string(),
        role: role.to_string(),
        digest: digest.to_string(),
        size_bytes,
        media_type: media_type.to_string(),
        download_path: CHALLENGE_MATERIAL_OBJECT_DOWNLOAD_PATH_V1.to_string(),
    };
    vec![
        object(
            "brief",
            "qualification/legacy-golden/brief.md",
            "challenge/brief.md",
            "playable_brief",
            LEGACY_GOLDEN_QUALIFICATION_BRIEF_DIGEST,
            LEGACY_GOLDEN_QUALIFICATION_BRIEF_SIZE,
            "text/markdown; charset=utf-8",
        ),
        object(
            "dataset",
            "data/synthetic-observations.csv",
            "challenge/dataset.csv",
            "dataset",
            LEGACY_GOLDEN_DATASET_DIGEST,
            LEGACY_GOLDEN_DATASET_SIZE,
            "text/csv; charset=utf-8",
        ),
        object(
            "baseline",
            "code/baseline.py",
            "challenge/baseline.py",
            "baseline_code",
            LEGACY_GOLDEN_BASELINE_DIGEST,
            LEGACY_GOLDEN_BASELINE_SIZE,
            "text/x-python; charset=utf-8",
        ),
        object(
            "evaluator",
            "evaluator/legacy-golden-evaluator.py",
            "challenge/evaluator.py",
            "frozen_evaluator",
            LEGACY_GOLDEN_EVALUATOR_DIGEST,
            LEGACY_GOLDEN_EVALUATOR_SIZE,
            "text/x-python; charset=utf-8",
        ),
    ]
}

#[derive(Serialize)]
struct LegacyGoldenQualificationMaterialAuthorityHashFrameV1<'a> {
    schema: &'a str,
    qualification_id: &'a str,
    challenge_id: Uuid,
    challenge_snapshot_hash: &'a str,
    challenge_title: &'a str,
    challenge_description: &'a str,
    challenge_status: &'a str,
    ruleset_version: &'a str,
    ruleset_hash: &'a str,
    ruleset_absent: bool,
    dataset_manifest_hash: &'a str,
    evaluator_manifest_hash: &'a str,
    objects: &'a [AssignedChallengeMaterialObjectV1],
}

pub fn legacy_golden_qualification_material_authority_hash(
    authority: &LegacyGoldenQualificationMaterialAuthorityV1,
) -> Result<String, String> {
    validate_legacy_golden_qualification_material_authority(authority, false)?;
    canonical_json_sha256(&LegacyGoldenQualificationMaterialAuthorityHashFrameV1 {
        schema: &authority.schema,
        qualification_id: &authority.qualification_id,
        challenge_id: authority.challenge_id,
        challenge_snapshot_hash: &authority.challenge_snapshot_hash,
        challenge_title: &authority.challenge_title,
        challenge_description: &authority.challenge_description,
        challenge_status: &authority.challenge_status,
        ruleset_version: &authority.ruleset_version,
        ruleset_hash: &authority.ruleset_hash,
        ruleset_absent: authority.ruleset_absent,
        dataset_manifest_hash: &authority.dataset_manifest_hash,
        evaluator_manifest_hash: &authority.evaluator_manifest_hash,
        objects: &authority.objects,
    })
}

pub fn verify_legacy_golden_qualification_material_authority(
    authority: &LegacyGoldenQualificationMaterialAuthorityV1,
) -> Result<(), String> {
    validate_legacy_golden_qualification_material_authority(authority, true)
}

fn validate_legacy_golden_qualification_material_authority(
    authority: &LegacyGoldenQualificationMaterialAuthorityV1,
    verify_hash: bool,
) -> Result<(), String> {
    if authority.schema != LEGACY_GOLDEN_QUALIFICATION_MATERIAL_AUTHORITY_V1
        || authority.qualification_id != LEGACY_GOLDEN_QUALIFICATION_ID
        || authority.challenge_id.is_nil()
        || authority.challenge_title != LEGACY_GOLDEN_CHALLENGE_TITLE
        || authority.challenge_description != LEGACY_GOLDEN_CHALLENGE_DESCRIPTION
        || authority.challenge_status != "open"
        || authority.ruleset_version != LEGACY_GOLDEN_CHALLENGE_RULESET_VERSION
        || authority.ruleset_hash != LEGACY_GOLDEN_CHALLENGE_RULESET_HASH
        || !authority.ruleset_absent
        || authority.dataset_manifest_hash != LEGACY_GOLDEN_DATASET_MANIFEST_HASH
        || authority.evaluator_manifest_hash != LEGACY_GOLDEN_EVALUATOR_MANIFEST_HASH
        || authority.objects != legacy_golden_qualification_material_objects()
    {
        return Err("legacy golden qualification material authority is not exact".to_string());
    }
    for digest in [
        &authority.challenge_snapshot_hash,
        &authority.ruleset_hash,
        &authority.dataset_manifest_hash,
        &authority.evaluator_manifest_hash,
    ] {
        decode_digest(digest)?;
    }
    if sha256_digest(LEGACY_GOLDEN_QUALIFICATION_BRIEF_BYTES)
        != LEGACY_GOLDEN_QUALIFICATION_BRIEF_DIGEST
        || u64::try_from(LEGACY_GOLDEN_QUALIFICATION_BRIEF_BYTES.len())
            .map_err(|_| "legacy golden qualification brief size overflow".to_string())?
            != LEGACY_GOLDEN_QUALIFICATION_BRIEF_SIZE
    {
        return Err("legacy golden qualification brief bytes drifted".to_string());
    }
    if verify_hash {
        decode_digest(&authority.authority_hash)?;
        if legacy_golden_qualification_material_authority_hash(authority)?
            != authority.authority_hash
        {
            return Err("legacy golden qualification material authority hash mismatch".to_string());
        }
    }
    Ok(())
}

pub fn verify_frozen_challenge_material_authority_binding(
    authority: &FrozenChallengeMaterialAuthorityBindingV1,
) -> Result<(), String> {
    match authority {
        FrozenChallengeMaterialAuthorityBindingV1::PackActivation(authority) => {
            verify_frozen_challenge_material_authority(authority)
        }
        FrozenChallengeMaterialAuthorityBindingV1::LegacyGoldenQualification(authority) => {
            verify_legacy_golden_qualification_material_authority(authority)
        }
    }
}

#[derive(Serialize)]
struct AssignedChallengeMaterialBundleHashFrameV1<'a> {
    schema: &'a str,
    authority: &'a FrozenChallengeMaterialAuthorityBindingV1,
    authority_hash: &'a str,
    paper_project_id: Uuid,
    challenge_ruleset_snapshot_hash: &'a str,
    binding_id: Uuid,
    player_id: Uuid,
    work_item_id: Uuid,
    work_item_version: u64,
    objects: &'a [AssignedChallengeMaterialObjectV1],
}

pub fn assigned_challenge_material_bundle_hash(
    bundle: &AssignedChallengeMaterialBundleV1,
) -> Result<String, String> {
    validate_assigned_challenge_material_bundle(bundle, false)?;
    canonical_json_sha256(&AssignedChallengeMaterialBundleHashFrameV1 {
        schema: &bundle.schema,
        authority: &bundle.authority,
        authority_hash: &bundle.authority_hash,
        paper_project_id: bundle.paper_project_id,
        challenge_ruleset_snapshot_hash: &bundle.challenge_ruleset_snapshot_hash,
        binding_id: bundle.binding_id,
        player_id: bundle.player_id,
        work_item_id: bundle.work_item_id,
        work_item_version: bundle.work_item_version,
        objects: &bundle.objects,
    })
}

pub fn verify_assigned_challenge_material_bundle(
    bundle: &AssignedChallengeMaterialBundleV1,
) -> Result<(), String> {
    validate_assigned_challenge_material_bundle(bundle, true)
}

fn validate_assigned_challenge_material_bundle(
    bundle: &AssignedChallengeMaterialBundleV1,
    verify_hash: bool,
) -> Result<(), String> {
    if bundle.schema != ASSIGNED_CHALLENGE_MATERIAL_BUNDLE_V1
        || bundle.paper_project_id.is_nil()
        || bundle.binding_id.is_nil()
        || bundle.player_id.is_nil()
        || bundle.work_item_id.is_nil()
        || bundle.work_item_version == 0
        || bundle.work_item_version > JSON_SAFE_U64_MAX
    {
        return Err("assigned challenge material work-item binding is invalid".to_string());
    }
    verify_frozen_challenge_material_authority_binding(&bundle.authority)?;
    if bundle.authority_hash != bundle.authority.authority_hash() {
        return Err("assigned challenge material authority hash mismatch".to_string());
    }
    decode_digest(&bundle.challenge_ruleset_snapshot_hash)?;
    if bundle.objects.len() != 4 {
        return Err("assigned challenge material bundle must contain exactly four objects".into());
    }
    let expected = [
        (
            "brief",
            "playable_brief",
            "challenge/brief.md",
            "text/markdown; charset=utf-8",
        ),
        (
            "dataset",
            "dataset",
            "",
            bundle.objects[1].media_type.as_str(),
        ),
        (
            "baseline",
            "baseline_code",
            "challenge/baseline.py",
            "text/x-python; charset=utf-8",
        ),
        (
            "evaluator",
            "frozen_evaluator",
            "challenge/evaluator.py",
            "text/x-python; charset=utf-8",
        ),
    ];
    let mut source_paths = HashSet::new();
    let mut digests = HashSet::new();
    for (index, object) in bundle.objects.iter().enumerate() {
        let (object_key, role, logical_path, media_type) = expected[index];
        let dataset_path_valid = index != 1
            || matches!(
                (object.logical_path.as_str(), object.media_type.as_str()),
                ("challenge/dataset.json", "application/json")
                    | ("challenge/dataset.csv", "text/csv; charset=utf-8")
            );
        if object.object_key != object_key
            || object.role != role
            || (index != 1 && object.logical_path != logical_path)
            || object.media_type != media_type
            || !dataset_path_valid
            || !safe_review_path(&object.source_path)
            || object.size_bytes == 0
            || object.size_bytes > 16 * 1024 * 1024
            || object.download_path != CHALLENGE_MATERIAL_OBJECT_DOWNLOAD_PATH_V1
            || !source_paths.insert(&object.source_path)
            || !digests.insert(&object.digest)
        {
            return Err("assigned challenge material object mapping is invalid".to_string());
        }
        decode_digest(&object.digest)?;
    }
    if let FrozenChallengeMaterialAuthorityBindingV1::LegacyGoldenQualification(authority) =
        &bundle.authority
    {
        if bundle.objects != authority.objects {
            return Err(
                "legacy golden qualification bundle differs from frozen authority".to_string(),
            );
        }
    }
    if verify_hash {
        decode_digest(&bundle.bundle_hash)?;
        if assigned_challenge_material_bundle_hash(bundle)? != bundle.bundle_hash {
            return Err("assigned challenge material bundle hash mismatch".to_string());
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct FrozenReviewAuthorityHashFrameV1<'a> {
    schema: &'a str,
    assignment_id: Uuid,
    paper_project_id: Uuid,
    submission_id: Uuid,
    review_round: u64,
    slot: &'a str,
    assignment_version: u64,
    expires_at: &'a str,
    release_candidate_hash: &'a str,
    paper_bundle_hash: &'a str,
    artifact_manifest_hash: &'a str,
    evaluator_manifest_hash: &'a str,
    dataset_manifest_hash: &'a str,
    artifact_objects: &'a [FrozenReviewObjectV1],
    execution_policy: &'a FrozenReviewExecutionPolicyV1,
}

pub fn frozen_review_authority_hash(authority: &FrozenReviewAuthorityV1) -> Result<String, String> {
    validate_frozen_review_authority(authority, false)?;
    canonical_json_sha256(&FrozenReviewAuthorityHashFrameV1 {
        schema: &authority.schema,
        assignment_id: authority.assignment_id,
        paper_project_id: authority.paper_project_id,
        submission_id: authority.submission_id,
        review_round: authority.review_round,
        slot: &authority.slot,
        assignment_version: authority.assignment_version,
        expires_at: &authority.expires_at,
        release_candidate_hash: &authority.release_candidate_hash,
        paper_bundle_hash: &authority.paper_bundle_hash,
        artifact_manifest_hash: &authority.artifact_manifest_hash,
        evaluator_manifest_hash: &authority.evaluator_manifest_hash,
        dataset_manifest_hash: &authority.dataset_manifest_hash,
        artifact_objects: &authority.artifact_objects,
        execution_policy: &authority.execution_policy,
    })
}

pub fn verify_frozen_review_authority(authority: &FrozenReviewAuthorityV1) -> Result<(), String> {
    validate_frozen_review_authority(authority, true)
}

/// Assignment-scoped, immutable projection of the exact review bytes.
///
/// This is intentionally not a Paper Room membership token.  Its sole authority is the exact
/// active assignment and the exact object descriptors committed by `bundle_hash`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenReviewBundleV1 {
    pub schema: String,
    pub bundle_hash: String,
    pub authority: FrozenReviewAuthorityV1,
    pub authority_hash: String,
    pub assignment_id: Uuid,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub review_round: u64,
    pub slot: String,
    pub assignment_version: u64,
    pub expires_at: String,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub artifact_manifest_hash: String,
    pub evaluator_manifest_hash: String,
    pub dataset_manifest_hash: String,
    pub objects: Vec<FrozenReviewObjectV1>,
    pub execution: FrozenReviewExecutionPlanV1,
}

#[derive(Serialize)]
struct FrozenReviewBundleHashFrameV1<'a> {
    schema: &'a str,
    authority: &'a FrozenReviewAuthorityV1,
    authority_hash: &'a str,
    assignment_id: Uuid,
    paper_project_id: Uuid,
    submission_id: Uuid,
    review_round: u64,
    slot: &'a str,
    assignment_version: u64,
    expires_at: &'a str,
    release_candidate_hash: &'a str,
    paper_bundle_hash: &'a str,
    artifact_manifest_hash: &'a str,
    evaluator_manifest_hash: &'a str,
    dataset_manifest_hash: &'a str,
    objects: &'a [FrozenReviewObjectV1],
    execution: &'a FrozenReviewExecutionPlanV1,
}

pub fn frozen_review_bundle_hash(bundle: &FrozenReviewBundleV1) -> Result<String, String> {
    validate_frozen_review_bundle(bundle, false)?;
    canonical_json_sha256(&FrozenReviewBundleHashFrameV1 {
        schema: &bundle.schema,
        authority: &bundle.authority,
        authority_hash: &bundle.authority_hash,
        assignment_id: bundle.assignment_id,
        paper_project_id: bundle.paper_project_id,
        submission_id: bundle.submission_id,
        review_round: bundle.review_round,
        slot: &bundle.slot,
        assignment_version: bundle.assignment_version,
        expires_at: &bundle.expires_at,
        release_candidate_hash: &bundle.release_candidate_hash,
        paper_bundle_hash: &bundle.paper_bundle_hash,
        artifact_manifest_hash: &bundle.artifact_manifest_hash,
        evaluator_manifest_hash: &bundle.evaluator_manifest_hash,
        dataset_manifest_hash: &bundle.dataset_manifest_hash,
        objects: &bundle.objects,
        execution: &bundle.execution,
    })
}

pub fn verify_frozen_review_bundle(bundle: &FrozenReviewBundleV1) -> Result<(), String> {
    validate_frozen_review_bundle(bundle, true)
}

fn validate_frozen_review_bundle(
    bundle: &FrozenReviewBundleV1,
    verify_hash: bool,
) -> Result<(), String> {
    if bundle.schema != RESOLVED_FROZEN_REVIEW_BUNDLE_V1 {
        return Err("frozen review bundle schema is invalid".to_string());
    }
    verify_frozen_review_authority(&bundle.authority)?;
    if bundle.authority_hash != bundle.authority.authority_hash
        || bundle.assignment_id != bundle.authority.assignment_id
        || bundle.paper_project_id != bundle.authority.paper_project_id
        || bundle.submission_id != bundle.authority.submission_id
        || bundle.review_round != bundle.authority.review_round
        || bundle.slot != bundle.authority.slot
        || bundle.assignment_version != bundle.authority.assignment_version
        || bundle.expires_at != bundle.authority.expires_at
        || bundle.release_candidate_hash != bundle.authority.release_candidate_hash
        || bundle.paper_bundle_hash != bundle.authority.paper_bundle_hash
        || bundle.artifact_manifest_hash != bundle.authority.artifact_manifest_hash
        || bundle.evaluator_manifest_hash != bundle.authority.evaluator_manifest_hash
        || bundle.dataset_manifest_hash != bundle.authority.dataset_manifest_hash
    {
        return Err("resolved review bundle disagrees with Hepta authority".to_string());
    }
    validate_resolved_review_object_mapping(&bundle.authority.artifact_objects, &bundle.objects)?;
    if bundle.assignment_id.is_nil()
        || bundle.paper_project_id.is_nil()
        || bundle.submission_id.is_nil()
        || bundle.review_round == 0
        || bundle.assignment_version == 0
    {
        return Err("frozen review bundle assignment binding is invalid".to_string());
    }
    if !matches!(
        bundle.slot.as_str(),
        "evaluator" | "reviewer_1" | "reviewer_2" | "reproducer"
    ) {
        return Err("frozen review bundle slot is invalid".to_string());
    }
    for digest in [
        &bundle.authority_hash,
        &bundle.release_candidate_hash,
        &bundle.paper_bundle_hash,
        &bundle.artifact_manifest_hash,
        &bundle.evaluator_manifest_hash,
        &bundle.dataset_manifest_hash,
    ] {
        decode_digest(digest)?;
    }
    let expected_kind = match bundle.slot.as_str() {
        "evaluator" => "evaluate",
        "reviewer_1" | "reviewer_2" => "review",
        "reproducer" => "reproduce",
        _ => unreachable!(),
    };
    if bundle.execution.schema != "hepta.paper_raid.review_execution_plan.v1"
        || bundle.execution.kind != expected_kind
        || bundle.execution.adapter != "python3-stdlib-v1"
        || !safe_review_path(&bundle.execution.entrypoint)
        || !(500..=30_000).contains(&bundle.execution.timeout_ms)
        || bundle.execution.seed > JSON_SAFE_U64_MAX
    {
        return Err(
            "frozen review execution plan is not an allowed fixed adapter plan".to_string(),
        );
    }
    if bundle.execution.kind != bundle.authority.execution_policy.kind
        || bundle.execution.adapter != bundle.authority.execution_policy.adapter
        || bundle.execution.timeout_ms != bundle.authority.execution_policy.timeout_ms
        || bundle.execution.seed != bundle.authority.execution_policy.seed
    {
        return Err("resolved execution plan disagrees with Hepta authority".to_string());
    }
    validate_text("evaluator_version", &bundle.execution.evaluator_version)?;
    if !bundle.expires_at.ends_with('Z')
        || bundle.expires_at.len() < 20
        || bundle.expires_at.len() > 40
    {
        return Err("frozen review bundle expiry must be canonical UTC RFC3339".to_string());
    }
    if !(3..=4).contains(&bundle.objects.len()) {
        return Err("frozen review executable bundle must contain 3-4 objects".to_string());
    }
    let mut previous: Option<(&str, &str)> = None;
    let mut keys = HashSet::new();
    let mut paths = HashSet::new();
    let mut total_size = 0_u64;
    for object in &bundle.objects {
        validate_logical_id("object_key", &object.object_key)?;
        validate_text("logical_path", &object.logical_path)?;
        validate_text("role", &object.role)?;
        validate_text("media_type", &object.media_type)?;
        if object.logical_path.starts_with('/')
            || object
                .logical_path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || object.logical_path.len() > 192
            || object
                .media_type
                .bytes()
                .any(|byte| byte.is_ascii_control())
            || object.download_path != REVIEW_OBJECT_DOWNLOAD_PATH_V1
            || !matches!(
                object.role.as_str(),
                "candidate" | "dataset" | "evaluator_support" | "frozen_evaluator"
            )
            || object.size_bytes == 0
            || object.size_bytes > 16 * 1024 * 1024
        {
            return Err("frozen review object descriptor is unsafe".to_string());
        }
        decode_digest(&object.digest)?;
        let ordering = (object.object_key.as_str(), object.logical_path.as_str());
        if previous.is_some_and(|prior| prior >= ordering)
            || !keys.insert(&object.object_key)
            || !paths.insert(&object.logical_path)
        {
            return Err(
                "frozen review objects must be strictly sorted by unique object_key/path"
                    .to_string(),
            );
        }
        total_size = total_size
            .checked_add(object.size_bytes)
            .ok_or_else(|| "frozen review object size overflow".to_string())?;
        previous = Some(ordering);
    }
    if total_size > 64 * 1024 * 1024 {
        return Err("frozen review bundle exceeds 64 MiB".to_string());
    }
    if bundle
        .objects
        .iter()
        .filter(|object| {
            object.role == "frozen_evaluator"
                && object.logical_path == bundle.execution.entrypoint
                && object.digest == bundle.execution.evaluator_version
        })
        .count()
        != 1
        || bundle
            .objects
            .iter()
            .filter(|object| object.role == "candidate")
            .count()
            != 1
        || bundle
            .objects
            .iter()
            .filter(|object| object.role == "dataset")
            .count()
            != 1
        || bundle
            .objects
            .iter()
            .filter(|object| object.role == "evaluator_support")
            .count()
            > 1
    {
        return Err(
            "resolved review bundle must bind one entrypoint, one candidate, and dataset members"
                .to_string(),
        );
    }
    if verify_hash {
        decode_digest(&bundle.bundle_hash)?;
        if frozen_review_bundle_hash(bundle)? != bundle.bundle_hash {
            return Err("frozen review bundle hash mismatch".to_string());
        }
    }
    Ok(())
}

fn validate_resolved_review_object_mapping(
    authority_objects: &[FrozenReviewObjectV1],
    resolved_objects: &[FrozenReviewObjectV1],
) -> Result<(), String> {
    let executable_authority = authority_objects
        .iter()
        .filter(|object| review_object_role_is_executable(&object.role))
        .collect::<Vec<_>>();
    if executable_authority.len() != resolved_objects.len() {
        return Err(
            "resolved review executable count differs from Hepta ArtifactManifest authority"
                .to_string(),
        );
    }
    let mut projected_keys = HashSet::new();
    for resolved in resolved_objects {
        if !review_object_role_is_executable(&resolved.role) {
            return Err(
                "human-readable review authority escaped into the executable bundle".to_string(),
            );
        }
        let matches = executable_authority
            .iter()
            .filter(|authority| {
                resolved.object_key == authority.object_key
                    && resolved.role == authority.role
                    && resolved.digest == authority.digest
                    && resolved.size_bytes == authority.size_bytes
                    && resolved.media_type == authority.media_type
                    && resolved.download_path == authority.download_path
            })
            .count();
        if matches != 1 || !projected_keys.insert(resolved.object_key.as_str()) {
            return Err(
                "resolved review object differs from Hepta ArtifactManifest authority".to_string(),
            );
        }
        let expected_transport_path = match (resolved.role.as_str(), resolved.media_type.as_str()) {
            ("frozen_evaluator", "text/x-python; charset=utf-8") => "evaluator/main.py",
            ("evaluator_support", "text/x-python; charset=utf-8") => "evaluator/baseline.py",
            ("candidate", "application/json") => "inputs/candidate.json",
            ("dataset", "application/json") => "inputs/dataset.json",
            ("dataset", "text/csv; charset=utf-8") => "inputs/dataset.csv",
            _ => {
                return Err(
                    "frozen Python adapter object role or media type is unsupported".to_string(),
                );
            }
        };
        if resolved.logical_path != expected_transport_path {
            return Err("resolved review object transport path is not deterministic".to_string());
        }
    }
    if executable_authority
        .iter()
        .any(|authority| !projected_keys.contains(authority.object_key.as_str()))
    {
        return Err("one executable authority object was not projected".to_string());
    }
    Ok(())
}

fn review_object_role_is_executable(role: &str) -> bool {
    matches!(
        role,
        "candidate" | "dataset" | "evaluator_support" | "frozen_evaluator"
    )
}

fn safe_review_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 192
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains('\0')
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn validate_frozen_review_authority(
    authority: &FrozenReviewAuthorityV1,
    verify_hash: bool,
) -> Result<(), String> {
    if authority.schema != FROZEN_REVIEW_AUTHORITY_V1
        || authority.assignment_id.is_nil()
        || authority.paper_project_id.is_nil()
        || authority.submission_id.is_nil()
        || authority.review_round == 0
        || authority.assignment_version == 0
        || !matches!(
            authority.slot.as_str(),
            "evaluator" | "reviewer_1" | "reviewer_2" | "reproducer"
        )
    {
        return Err("frozen review authority assignment binding is invalid".to_string());
    }
    for digest in [
        &authority.release_candidate_hash,
        &authority.paper_bundle_hash,
        &authority.artifact_manifest_hash,
        &authority.evaluator_manifest_hash,
        &authority.dataset_manifest_hash,
    ] {
        decode_digest(digest)?;
    }
    if !authority.expires_at.ends_with('Z')
        || authority.expires_at.len() < 20
        || authority.expires_at.len() > 40
        || !(6..=7).contains(&authority.artifact_objects.len())
    {
        return Err("frozen review authority expiry or object set is invalid".to_string());
    }
    validate_frozen_review_objects(&authority.artifact_objects)?;
    let expected_kind = match authority.slot.as_str() {
        "evaluator" => "evaluate",
        "reviewer_1" | "reviewer_2" => "review",
        "reproducer" => "reproduce",
        _ => unreachable!(),
    };
    if authority.execution_policy.schema != "hepta.paper_raid.review_execution_policy.v1"
        || authority.execution_policy.kind != expected_kind
        || authority.execution_policy.adapter != "python3-stdlib-v1"
        || !(500..=30_000).contains(&authority.execution_policy.timeout_ms)
        || authority.execution_policy.seed > JSON_SAFE_U64_MAX
    {
        return Err("frozen review execution policy is invalid".to_string());
    }
    if verify_hash {
        decode_digest(&authority.authority_hash)?;
        if frozen_review_authority_hash(authority)? != authority.authority_hash {
            return Err("frozen review authority hash mismatch".to_string());
        }
    }
    Ok(())
}

fn validate_frozen_review_objects(objects: &[FrozenReviewObjectV1]) -> Result<(), String> {
    let mut previous: Option<(&str, &str)> = None;
    let mut keys = HashSet::new();
    let mut paths = HashSet::new();
    let mut digests = HashSet::new();
    let mut roles = HashMap::<&str, usize>::new();
    let mut total_size = 0_u64;
    for object in objects {
        validate_logical_id("object_key", &object.object_key)?;
        validate_text("logical_path", &object.logical_path)?;
        validate_text("role", &object.role)?;
        validate_text("media_type", &object.media_type)?;
        if !safe_review_path(&object.logical_path)
            || object
                .media_type
                .bytes()
                .any(|byte| byte.is_ascii_control())
            || object.download_path != REVIEW_OBJECT_DOWNLOAD_PATH_V1
            || !matches!(
                (object.role.as_str(), object.media_type.as_str()),
                (
                    "paper_source",
                    "text/markdown; charset=utf-8" | "application/pdf"
                ) | (
                    "bibliography",
                    "application/x-bibtex" | "text/plain; charset=utf-8"
                ) | ("claim_evidence_graph", "application/json")
                    | ("candidate", "application/json")
                    | ("dataset", "application/json" | "text/csv; charset=utf-8")
                    | (
                        "frozen_evaluator" | "evaluator_support",
                        "text/x-python; charset=utf-8"
                    )
            )
            || object.size_bytes == 0
            || object.size_bytes > 16 * 1024 * 1024
        {
            return Err("frozen review object descriptor is unsafe".to_string());
        }
        decode_digest(&object.digest)?;
        let ordering = (object.object_key.as_str(), object.logical_path.as_str());
        if previous.is_some_and(|prior| prior >= ordering)
            || !keys.insert(&object.object_key)
            || !paths.insert(&object.logical_path)
            || !digests.insert(&object.digest)
        {
            return Err(
                "frozen review objects must be strictly sorted by unique object_key/path"
                    .to_string(),
            );
        }
        *roles.entry(object.role.as_str()).or_default() += 1;
        total_size = total_size
            .checked_add(object.size_bytes)
            .ok_or_else(|| "frozen review object size overflow".to_string())?;
        previous = Some(ordering);
    }
    if total_size > 64 * 1024 * 1024 {
        return Err("frozen review bundle exceeds 64 MiB".to_string());
    }
    let exactly_one = |role: &str| roles.get(role).copied().unwrap_or_default() == 1;
    if !exactly_one("paper_source")
        || !exactly_one("bibliography")
        || !exactly_one("claim_evidence_graph")
        || !exactly_one("frozen_evaluator")
        || !exactly_one("dataset")
        || !exactly_one("candidate")
        || roles.get("evaluator_support").copied().unwrap_or_default() > 1
        || roles.len() != 6 + usize::from(roles.contains_key("evaluator_support"))
    {
        return Err(
            "frozen review authority lacks exact human-readable and executable role coverage"
                .to_string(),
        );
    }
    Ok(())
}

/// Human-authored evaluation scores. These are deliberately not part of the frozen evaluator
/// output and are collected only when the assigned evaluator confirms the execution receipt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewExecutionScoreV1 {
    pub method_rigor_bps: u16,
    pub experiment_statistics_bps: u16,
    pub reproducibility_bps: u16,
    pub evidence_citations_bps: u16,
    pub value_originality_bps: u16,
    pub argument_expression_bps: u16,
    pub ethics_transparency_bps: u16,
}

/// Human-confirmed evaluation gates. These are not trusted from the frozen evaluator process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewExecutionHardGatesV1 {
    pub citations_and_data_authentic: bool,
    pub failed_runs_disclosed: bool,
    pub all_authors_consented: bool,
    pub core_claims_have_evidence: bool,
    pub artifact_lineage_complete: bool,
    pub license_ethics_coi_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewExecutionToleranceRuleV1 {
    Absolute {
        metric: String,
        max_delta_micros: i64,
    },
    Relative {
        metric: String,
        max_delta_bps: u16,
    },
    Statistical {
        metric: String,
        minimum_interval_overlap_bps: u16,
        maximum_effect_delta_micros: i64,
        minimum_p_value_micros: u32,
    },
    Seed {
        expected_seed_set_hash: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewEvaluationExecutionResultV1 {
    pub reference_metrics_micros: BTreeMap<String, i64>,
    pub tolerance_policy_version: String,
    pub tolerance_rules: Vec<ReviewExecutionToleranceRuleV1>,
    pub candidate_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewExecutionStatisticalEvidenceV1 {
    pub interval_overlap_bps: u16,
    pub effect_delta_micros: i64,
    pub p_value_micros: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewReproductionExecutionResultV1 {
    pub observed_metrics_micros: BTreeMap<String, i64>,
    pub statistical_evidence: BTreeMap<String, ReviewExecutionStatisticalEvidenceV1>,
}

/// Agent-signed execution result.  Human review signatures consume this receipt; they do not
/// retype metrics, seeds or environment statements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewExecutionReceiptV1 {
    pub schema: String,
    pub receipt_id: Uuid,
    pub task_id: Uuid,
    pub binding_id: Uuid,
    pub assignment_id: Uuid,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub evaluation_id: Uuid,
    pub kind: String,
    pub attempt: u64,
    pub fencing_token: u64,
    pub bundle_hash: String,
    pub evaluator_version: String,
    pub input_root: String,
    pub output_root: String,
    pub metrics_hash: String,
    pub observed_metrics_micros: BTreeMap<String, i64>,
    pub statistical_evidence: serde_json::Value,
    pub candidate_passed: Option<bool>,
    pub seed_set_hash: String,
    pub environment_hash: String,
    pub run_manifest_hash: String,
    pub logs_hash: String,
    pub started_at_unix: i64,
    pub completed_at_unix: i64,
    pub agent_id: String,
    pub agent_key_id: String,
    pub signing_public_key_hash: String,
    pub signature: String,
}

pub fn review_execution_receipt_id(
    binding_id: Uuid,
    task_id: Uuid,
    assignment_id: Uuid,
    bundle_hash: &str,
    attempt: u64,
    fencing_token: u64,
) -> Result<Uuid, String> {
    if binding_id.is_nil()
        || task_id.is_nil()
        || assignment_id.is_nil()
        || attempt == 0
        || attempt > JSON_SAFE_U64_MAX
        || fencing_token == 0
        || fencing_token > JSON_SAFE_U64_MAX
    {
        return Err("review execution receipt ID authority is invalid".to_string());
    }
    decode_digest(bundle_hash)?;
    let fields = [
        binding_id.to_string(),
        task_id.to_string(),
        assignment_id.to_string(),
        bundle_hash.to_string(),
        attempt.to_string(),
        fencing_token.to_string(),
    ];
    let mut digest = Sha256::new();
    digest.update(REVIEW_EXECUTION_RECEIPT_ID_DOMAIN_V1.as_bytes());
    digest.update([0]);
    for (index, field) in fields.iter().enumerate() {
        if index != 0 {
            digest.update([0]);
        }
        digest.update(field.as_bytes());
    }
    let mut bytes: [u8; 16] = digest.finalize()[..16]
        .try_into()
        .expect("sha256 prefix has fixed length");
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(Uuid::from_bytes(bytes))
}

pub fn review_execution_receipt_signing_bytes(
    receipt: &ReviewExecutionReceiptV1,
) -> Result<Vec<u8>, String> {
    validate_review_execution_receipt(receipt)?;
    Ok(
        CanonicalFrame::new("hepta_paper_raid_review_execution_receipt_v1")
            .string(&receipt.schema)?
            .string(&receipt.receipt_id.to_string())?
            .string(&receipt.task_id.to_string())?
            .string(&receipt.assignment_id.to_string())?
            .string(&receipt.binding_id.to_string())?
            .string(&receipt.paper_project_id.to_string())?
            .string(&receipt.submission_id.to_string())?
            .string(&receipt.evaluation_id.to_string())?
            .string(&receipt.kind)?
            .u64(receipt.attempt)
            .u64(receipt.fencing_token)
            .digest(&receipt.bundle_hash)?
            .string(&receipt.evaluator_version)?
            .digest(&receipt.input_root)?
            .digest(&receipt.output_root)?
            .digest(&receipt.metrics_hash)?
            .u32(match receipt.candidate_passed {
                None => 0,
                Some(false) => 1,
                Some(true) => 2,
            })
            .digest(&receipt.seed_set_hash)?
            .digest(&receipt.environment_hash)?
            .digest(&receipt.run_manifest_hash)?
            .digest(&receipt.logs_hash)?
            .i64(receipt.started_at_unix)
            .i64(receipt.completed_at_unix)
            .string(&receipt.agent_id)?
            .digest(&receipt.agent_key_id)?
            .digest(&receipt.signing_public_key_hash)?
            .finish(),
    )
}

pub fn review_execution_receipt_hash(receipt: &ReviewExecutionReceiptV1) -> Result<String, String> {
    review_execution_receipt_signing_bytes(receipt).map(|bytes| sha256_digest(&bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenReviewInputObjectV1 {
    pub object_key: String,
    pub logical_path: String,
    pub role: String,
    pub digest: String,
    pub size_bytes: u64,
}

pub fn frozen_review_input_root(objects: &[FrozenReviewInputObjectV1]) -> Result<String, String> {
    if !(3..=64).contains(&objects.len()) {
        return Err("frozen review input root requires 3-64 objects".to_string());
    }
    canonical_json_sha256(objects)
}

pub fn review_execution_metrics_hash(receipt: &ReviewExecutionReceiptV1) -> Result<String, String> {
    canonical_json_sha256(&serde_json::json!({
        "observed_metrics_micros": receipt.observed_metrics_micros,
        "statistical_evidence": receipt.statistical_evidence,
        "candidate_passed": receipt.candidate_passed,
    }))
}

pub fn verify_review_execution_receipt_signature(
    receipt: &ReviewExecutionReceiptV1,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", &receipt.signature)?;
    verifying_key
        .verify(
            &review_execution_receipt_signing_bytes(receipt)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "review execution receipt signature verification failed".to_string())
}

fn validate_review_execution_receipt(receipt: &ReviewExecutionReceiptV1) -> Result<(), String> {
    if receipt.schema != REVIEW_EXECUTION_RECEIPT_V1
        || receipt.receipt_id.is_nil()
        || receipt.task_id.is_nil()
        || receipt.binding_id.is_nil()
        || receipt.assignment_id.is_nil()
        || receipt.paper_project_id.is_nil()
        || receipt.submission_id.is_nil()
        || receipt.evaluation_id.is_nil()
        || receipt.attempt == 0
        || receipt.fencing_token == 0
        || receipt.started_at_unix < 0
        || receipt.completed_at_unix < receipt.started_at_unix
    {
        return Err("review execution receipt identity, attempt, or time is invalid".to_string());
    }
    if !matches!(receipt.kind.as_str(), "evaluate" | "reproduce") {
        return Err("review execution receipt kind is invalid".to_string());
    }
    match (receipt.kind.as_str(), receipt.candidate_passed) {
        ("evaluate", Some(_)) | ("reproduce", None) => {}
        ("evaluate", None) => {
            return Err("evaluation receipt must bind candidate_passed".to_string());
        }
        ("reproduce", Some(_)) => {
            return Err("reproduction receipt must not bind candidate_passed".to_string());
        }
        _ => unreachable!("receipt kind was validated above"),
    }
    for digest in [
        &receipt.bundle_hash,
        &receipt.input_root,
        &receipt.output_root,
        &receipt.metrics_hash,
        &receipt.seed_set_hash,
        &receipt.environment_hash,
        &receipt.run_manifest_hash,
        &receipt.logs_hash,
        &receipt.signing_public_key_hash,
    ] {
        decode_digest(digest)?;
    }
    validate_text("evaluator_version", &receipt.evaluator_version)?;
    validate_logical_id("agent_id", &receipt.agent_id)?;
    if receipt.agent_key_id != receipt.signing_public_key_hash {
        return Err("review receipt signing key hashes differ".to_string());
    }
    if receipt.observed_metrics_micros.len() > 256 {
        return Err("review receipt metrics are oversized".to_string());
    }
    Ok(())
}

pub fn paper_bundle_frame(bundle: &PaperBundleV2) -> Result<Vec<u8>, String> {
    if bundle.schema != PAPER_BUNDLE_V2 {
        return Err(format!("unsupported PaperBundle schema {}", bundle.schema));
    }
    let release_frame = paper_release_candidate_frame(&bundle.release_candidate)?;
    let release_hash = sha256_digest(&release_frame);
    if release_hash != bundle.release_candidate_hash {
        return Err("release_candidate_hash does not match release candidate bytes".to_string());
    }
    if bundle.author_consents.len() != bundle.release_candidate.authors.len()
        || !(3..=5).contains(&bundle.author_consents.len())
    {
        return Err("PaperBundle must contain one consent for every 3-5 roster author".to_string());
    }
    let mut candidate_authors = bundle.release_candidate.authors.clone();
    candidate_authors.sort_by_key(|author| author.author_order);
    let mut consents = bundle.author_consents.clone();
    consents.sort_by_key(|consent| consent.author_order);
    let mut frame = CanonicalFrame::new("hepta_paper_raid_paper_bundle_v2")
        .string(&bundle.schema)?
        .bytes(&release_frame)?
        .digest(&bundle.release_candidate_hash)?
        .u32(u32::try_from(consents.len()).expect("five consents fit u32"));
    for (author, consent) in candidate_authors.iter().zip(consents.iter()) {
        if consent.author_order != author.author_order
            || consent.participant_slot != author.participant_slot
            || consent.player_id != author.player_id
            || consent.signed_at_unix < 0
        {
            return Err(
                "PaperBundle consent does not match release candidate author order/roster"
                    .to_string(),
            );
        }
        validate_text("signing_key_id", &consent.signing_key_id)?;
        let public_key =
            decode_base64_exact::<32>("signing_public_key", &consent.signing_public_key)?;
        if sha256_digest(&public_key) != consent.signing_public_key_hash {
            return Err("signing_public_key_hash does not match key snapshot".to_string());
        }
        let signature = decode_base64_exact::<64>("signature", &consent.signature)?;
        verify_authorship_consent_signature(
            &AuthorshipConsentSigningV2 {
                schema: AUTHORSHIP_CONSENT_V2.to_string(),
                consent_id: consent.consent_id,
                paper_project_id: bundle.release_candidate.paper_project_id,
                revision_id: bundle.release_candidate.revision_id,
                player_id: consent.player_id,
                signing_key_id: consent.signing_key_id.clone(),
                signing_public_key: consent.signing_public_key.clone(),
                signing_public_key_hash: consent.signing_public_key_hash.clone(),
                release_candidate_hash: bundle.release_candidate_hash.clone(),
                signed_at_unix: consent.signed_at_unix,
            },
            &consent.signature,
        )?;
        frame = frame
            .u32(consent.author_order)
            .u32(consent.participant_slot)
            .string(&consent.player_id.to_string())?
            .string(&consent.consent_id.to_string())?
            .string(&consent.signing_key_id)?
            .bytes(&public_key)?
            .digest(&consent.signing_public_key_hash)?
            .i64(consent.signed_at_unix)
            .bytes(&signature)?;
    }
    Ok(frame.finish())
}

pub fn paper_bundle_hash(bundle: &PaperBundleV2) -> Result<String, String> {
    let hash = paper_bundle_frame(bundle).map(|bytes| sha256_digest(&bytes))?;
    if !bundle.paper_bundle_hash.is_empty() && bundle.paper_bundle_hash != hash {
        return Err("paper_bundle_hash does not match canonical PaperBundle bytes".to_string());
    }
    Ok(hash)
}

/// Outer post-session evidence. `PaperBundleV2` intentionally remains an
/// acyclic, jointly signed research artifact; real-time roots, evaluation,
/// appeal and finality facts are added only here after Nakama has completed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedPaperRaidEvidenceEnvelopeV1 {
    pub schema: String,
    pub evidence_envelope_id: Uuid,
    pub paper_bundle_hash: String,
    pub nakama_completion_receipt_hash: String,
    pub session_id: String,
    pub session_roster_version: u64,
    pub roster_root: String,
    pub event_root: String,
    pub archive_hash: String,
    pub ruleset_hash: String,
    pub challenge_snapshot_hash: String,
    pub evaluation_report_hash: String,
    pub reproduction_report_hash: String,
    pub appeal_resolution_hash: Option<String>,
    pub finality_receipt_hash: Option<String>,
    pub created_at_unix: i64,
    pub issuer_key_id: String,
    pub signature: String,
}

fn optional_digest(
    mut frame: CanonicalFrame,
    value: &Option<String>,
) -> Result<CanonicalFrame, String> {
    match value {
        Some(value) => {
            frame = frame.u32(1).digest(value)?;
        }
        None => frame = frame.u32(0),
    }
    Ok(frame)
}

pub fn paper_raid_evidence_envelope_signing_bytes(
    envelope: &SignedPaperRaidEvidenceEnvelopeV1,
) -> Result<Vec<u8>, String> {
    if envelope.schema != PAPER_RAID_EVIDENCE_ENVELOPE_V1
        || envelope.session_roster_version == 0
        || envelope.created_at_unix < 0
    {
        return Err("evidence envelope schema, session epoch, or time is invalid".to_string());
    }
    validate_logical_id("session_id", &envelope.session_id)?;
    validate_text("issuer_key_id", &envelope.issuer_key_id)?;
    if !envelope.signature.is_empty() {
        decode_base64_exact::<64>("signature", &envelope.signature)?;
    }
    let frame = CanonicalFrame::new("hepta_paper_raid_evidence_envelope_v1")
        .string(&envelope.schema)?
        .string(&envelope.evidence_envelope_id.to_string())?
        .digest(&envelope.paper_bundle_hash)?
        .digest(&envelope.nakama_completion_receipt_hash)?
        .string(&envelope.session_id)?
        .u64(envelope.session_roster_version)
        .digest(&envelope.roster_root)?
        .digest(&envelope.event_root)?
        .digest(&envelope.archive_hash)?
        .digest(&envelope.ruleset_hash)?
        .digest(&envelope.challenge_snapshot_hash)?
        .digest(&envelope.evaluation_report_hash)?
        .digest(&envelope.reproduction_report_hash)?;
    let frame = optional_digest(frame, &envelope.appeal_resolution_hash)?;
    let frame = optional_digest(frame, &envelope.finality_receipt_hash)?;
    Ok(frame
        .i64(envelope.created_at_unix)
        .string(&envelope.issuer_key_id)?
        .finish())
}

pub fn sign_paper_raid_evidence_envelope(
    mut envelope: SignedPaperRaidEvidenceEnvelopeV1,
    signing_key: &SigningKey,
) -> Result<SignedPaperRaidEvidenceEnvelopeV1, String> {
    envelope.signature.clear();
    let bytes = paper_raid_evidence_envelope_signing_bytes(&envelope)?;
    envelope.signature = BASE64.encode(signing_key.sign(&bytes).to_bytes());
    Ok(envelope)
}

pub fn verify_paper_raid_evidence_envelope(
    envelope: &SignedPaperRaidEvidenceEnvelopeV1,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let bytes = paper_raid_evidence_envelope_signing_bytes(envelope)?;
    let signature = decode_base64_exact::<64>("signature", &envelope.signature)?;
    verifying_key
        .verify(&bytes, &Signature::from_bytes(&signature))
        .map_err(|_| "Paper Raid evidence envelope signature verification failed".to_string())
}

pub fn paper_raid_evidence_envelope_hash(
    envelope: &SignedPaperRaidEvidenceEnvelopeV1,
) -> Result<String, String> {
    let signing_bytes = paper_raid_evidence_envelope_signing_bytes(envelope)?;
    let signature = decode_base64_exact::<64>("signature", &envelope.signature)?;
    Ok(sha256_digest(
        &CanonicalFrame::new("hepta_paper_raid_evidence_envelope_record_v1")
            .bytes(&signing_bytes)?
            .bytes(&signature)?
            .finish(),
    ))
}

/// Verifies both signatures and the complete cross-record binding. Signature
/// verification alone is insufficient: a correctly signed envelope that
/// names a different receipt/root/reference set must never be accepted for a
/// PaperBundle.
#[allow(clippy::too_many_arguments)]
pub fn verify_paper_raid_evidence_envelope_against(
    envelope: &SignedPaperRaidEvidenceEnvelopeV1,
    bundle: &PaperBundleV2,
    completion_receipt: &SignedNakamaCompletionReceiptV1,
    evaluation_report_hash: &str,
    reproduction_report_hash: &str,
    appeal_resolution_hash: Option<&str>,
    finality_receipt_hash: Option<&str>,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let expected_bundle_hash = paper_bundle_hash(bundle)?;
    let expected_receipt_hash = canonical_json_sha256(completion_receipt)?;
    if envelope.paper_bundle_hash != expected_bundle_hash {
        return Err("evidence envelope paper_bundle_hash differs from PaperBundle".to_string());
    }
    if envelope.nakama_completion_receipt_hash != expected_receipt_hash {
        return Err(
            "evidence envelope receipt hash differs from signed completion receipt".to_string(),
        );
    }
    if envelope.session_id != completion_receipt.session_id
        || envelope.session_roster_version != completion_receipt.roster_version
        || envelope.roster_root != completion_receipt.roster_root
        || envelope.event_root != completion_receipt.event_root
        || envelope.archive_hash != completion_receipt.archive_hash
        || envelope.ruleset_hash != completion_receipt.ruleset_hash
        || envelope.challenge_snapshot_hash != completion_receipt.challenge_snapshot_hash
    {
        return Err(
            "evidence envelope session epoch or authoritative roots differ from completion receipt"
                .to_string(),
        );
    }
    if envelope.evaluation_report_hash != evaluation_report_hash
        || envelope.reproduction_report_hash != reproduction_report_hash
        || envelope.appeal_resolution_hash.as_deref() != appeal_resolution_hash
        || envelope.finality_receipt_hash.as_deref() != finality_receipt_hash
    {
        return Err(
            "evidence envelope review/finality references differ from supplied facts".to_string(),
        );
    }
    if envelope.issuer_key_id != completion_receipt.issuer_key_id {
        return Err("evidence envelope and completion receipt use different issuers".to_string());
    }
    verify_nakama_completion_receipt(completion_receipt, verifying_key)?;
    verify_paper_raid_evidence_envelope(envelope, verifying_key)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublicationReleaseAuthorConsentV1 {
    pub author_order: u32,
    pub participant_slot: u32,
    pub player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublicationReleaseV1 {
    pub schema: String,
    pub publication_release_id: Uuid,
    pub paper_bundle_hash: String,
    pub evidence_envelope_hash: String,
    pub destination: String,
    pub release_manifest_hash: String,
    pub license: String,
    pub released_at_unix: i64,
    pub author_consents: Vec<PublicationReleaseAuthorConsentV1>,
    pub publication_release_hash: String,
}

pub fn publication_release_author_signing_bytes(
    release: &PublicationReleaseV1,
    consent: &PublicationReleaseAuthorConsentV1,
) -> Result<Vec<u8>, String> {
    if release.schema != PUBLICATION_RELEASE_V1
        || release.released_at_unix < 0
        || !(1..=5).contains(&consent.author_order)
        || !(1..=5).contains(&consent.participant_slot)
    {
        return Err(
            "publication release schema, time, author order, or slot is invalid".to_string(),
        );
    }
    validate_text("destination", &release.destination)?;
    validate_text("license", &release.license)?;
    validate_text("signing_key_id", &consent.signing_key_id)?;
    let key = decode_base64_exact::<32>("signing_public_key", &consent.signing_public_key)?;
    if sha256_digest(&key) != consent.signing_public_key_hash {
        return Err("publication release signing key hash does not match key".to_string());
    }
    Ok(
        CanonicalFrame::new("hepta_paper_raid_publication_release_author_v1")
            .string(&release.schema)?
            .string(&release.publication_release_id.to_string())?
            .digest(&release.paper_bundle_hash)?
            .digest(&release.evidence_envelope_hash)?
            .string(&release.destination)?
            .digest(&release.release_manifest_hash)?
            .string(&release.license)?
            .i64(release.released_at_unix)
            .u32(consent.author_order)
            .u32(consent.participant_slot)
            .string(&consent.player_id.to_string())?
            .string(&consent.signing_key_id)?
            .bytes(&key)?
            .digest(&consent.signing_public_key_hash)?
            .finish(),
    )
}

pub fn publication_release_frame(release: &PublicationReleaseV1) -> Result<Vec<u8>, String> {
    if !(3..=5).contains(&release.author_consents.len()) {
        return Err("publication release requires 3-5 explicit human consents".to_string());
    }
    let mut consents = release.author_consents.clone();
    consents.sort_by_key(|consent| consent.author_order);
    let mut slots = std::collections::HashSet::new();
    let mut players = std::collections::HashSet::new();
    let mut frame = CanonicalFrame::new("hepta_paper_raid_publication_release_v1")
        .string(&release.schema)?
        .string(&release.publication_release_id.to_string())?
        .digest(&release.paper_bundle_hash)?
        .digest(&release.evidence_envelope_hash)?
        .string(&release.destination)?
        .digest(&release.release_manifest_hash)?
        .string(&release.license)?
        .i64(release.released_at_unix)
        .u32(u32::try_from(consents.len()).expect("five release consents fit u32"));
    for (index, consent) in consents.iter().enumerate() {
        if consent.author_order != u32::try_from(index + 1).expect("five authors fit u32")
            || !slots.insert(consent.participant_slot)
            || !players.insert(consent.player_id)
        {
            return Err(
                "publication release author order must be gapless and slots/players unique"
                    .to_string(),
            );
        }
        let signing_bytes = publication_release_author_signing_bytes(release, consent)?;
        let key = decode_base64_exact::<32>("signing_public_key", &consent.signing_public_key)?;
        let key = VerifyingKey::from_bytes(&key)
            .map_err(|_| "publication release key is not valid Ed25519".to_string())?;
        let signature = decode_base64_exact::<64>("signature", &consent.signature)?;
        key.verify(&signing_bytes, &Signature::from_bytes(&signature))
            .map_err(|_| "publication release author signature verification failed".to_string())?;
        frame = frame.bytes(&signing_bytes)?.bytes(&signature)?;
    }
    Ok(frame.finish())
}

pub fn publication_release_hash(release: &PublicationReleaseV1) -> Result<String, String> {
    let hash = publication_release_frame(release).map(|bytes| sha256_digest(&bytes))?;
    if !release.publication_release_hash.is_empty() && release.publication_release_hash != hash {
        return Err("publication_release_hash does not match canonical release bytes".to_string());
    }
    Ok(hash)
}

/// Verifies that a publication action releases exactly the jointly signed
/// PaperBundle and its post-session evidence, without substituting authors.
/// Service code additionally requires each signing key to be the author's
/// current active key at release time.
pub fn verify_publication_release_against(
    release: &PublicationReleaseV1,
    bundle: &PaperBundleV2,
    evidence_envelope: &SignedPaperRaidEvidenceEnvelopeV1,
) -> Result<(), String> {
    let expected_bundle_hash = paper_bundle_hash(bundle)?;
    let expected_evidence_hash = paper_raid_evidence_envelope_hash(evidence_envelope)?;
    if release.paper_bundle_hash != expected_bundle_hash
        || release.evidence_envelope_hash != expected_evidence_hash
    {
        return Err(
            "publication release does not bind the supplied PaperBundle/evidence envelope"
                .to_string(),
        );
    }
    if release.license != bundle.release_candidate.license {
        return Err("publication release license differs from PaperBundle".to_string());
    }
    let mut expected_authors = bundle.release_candidate.authors.clone();
    expected_authors.sort_by_key(|author| author.author_order);
    let mut release_authors = release.author_consents.clone();
    release_authors.sort_by_key(|author| author.author_order);
    if expected_authors.len() != release_authors.len()
        || expected_authors
            .iter()
            .zip(release_authors.iter())
            .any(|(expected, actual)| {
                expected.author_order != actual.author_order
                    || expected.participant_slot != actual.participant_slot
                    || expected.player_id != actual.player_id
            })
    {
        return Err(
            "publication release author order/slot/player roster differs from PaperBundle"
                .to_string(),
        );
    }
    publication_release_hash(release).map(|_| ())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentProposalSigningV1 {
    pub schema: String,
    pub proposal_id: Uuid,
    pub paper_project_id: Uuid,
    pub work_item_id: Uuid,
    pub section_key: String,
    pub parent_revision_id: Uuid,
    pub proposal_kind: String,
    pub payload_hash: String,
    pub artifact_manifest_hash: String,
    pub agent_id: String,
    pub binding_id: Uuid,
    pub agent_key_id: String,
    pub signed_at_unix: i64,
}

pub fn agent_proposal_signing_bytes(proposal: &AgentProposalSigningV1) -> Result<Vec<u8>, String> {
    if proposal.schema != AGENT_PROPOSAL_V1
        || proposal.signed_at_unix < 0
        || !matches!(proposal.proposal_kind.as_str(), "proposal" | "delivery")
    {
        return Err("Agent proposal schema, kind, or signing time is invalid".to_string());
    }
    validate_logical_id("section_key", &proposal.section_key)?;
    validate_text("agent_id", &proposal.agent_id)?;
    validate_text("agent_key_id", &proposal.agent_key_id)?;
    Ok(CanonicalFrame::new("hepta_paper_raid_agent_proposal_v1")
        .string(&proposal.schema)?
        .string(&proposal.proposal_id.to_string())?
        .string(&proposal.paper_project_id.to_string())?
        .string(&proposal.work_item_id.to_string())?
        .string(&proposal.section_key)?
        .string(&proposal.parent_revision_id.to_string())?
        .string(&proposal.proposal_kind)?
        .digest(&proposal.payload_hash)?
        .digest(&proposal.artifact_manifest_hash)?
        .string(&proposal.agent_id)?
        .string(&proposal.binding_id.to_string())?
        .string(&proposal.agent_key_id)?
        .i64(proposal.signed_at_unix)
        .finish())
}

pub fn sign_agent_proposal(
    proposal: &AgentProposalSigningV1,
    signing_key: &SigningKey,
) -> Result<String, String> {
    Ok(BASE64.encode(
        signing_key
            .sign(&agent_proposal_signing_bytes(proposal)?)
            .to_bytes(),
    ))
}

pub fn verify_agent_proposal_signature(
    proposal: &AgentProposalSigningV1,
    signature: &str,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(
            &agent_proposal_signing_bytes(proposal)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "Agent proposal signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentProposalSigningV2 {
    pub schema: String,
    pub proposal_id: Uuid,
    pub paper_project_id: Uuid,
    pub work_item_id: Uuid,
    pub section_key: String,
    pub parent_revision_id: Uuid,
    pub lease_id: Uuid,
    pub lease_fencing_token: u64,
    pub expected_work_version: u64,
    pub proposal_kind: String,
    pub payload_hash: String,
    pub artifact_manifest_id: Uuid,
    pub artifact_manifest_hash: String,
    pub agent_id: String,
    pub binding_id: Uuid,
    pub agent_key_id: String,
    pub signed_at_unix: i64,
}

pub fn agent_proposal_v2_signing_bytes(
    proposal: &AgentProposalSigningV2,
) -> Result<Vec<u8>, String> {
    if proposal.schema != AGENT_PROPOSAL_V2
        || proposal.signed_at_unix < 0
        || proposal.lease_fencing_token == 0
        || proposal.lease_fencing_token > JSON_SAFE_U64_MAX
        || proposal.expected_work_version == 0
        || proposal.expected_work_version > JSON_SAFE_U64_MAX
        || !matches!(proposal.proposal_kind.as_str(), "proposal" | "delivery")
    {
        return Err(
            "Agent proposal V2 schema, kind, lease fence, work version, or signing time is invalid"
                .to_string(),
        );
    }
    validate_logical_id("section_key", &proposal.section_key)?;
    validate_text("agent_id", &proposal.agent_id)?;
    validate_text("agent_key_id", &proposal.agent_key_id)?;
    Ok(CanonicalFrame::new("hepta_paper_raid_agent_proposal_v2")
        .string(&proposal.schema)?
        .string(&proposal.proposal_id.to_string())?
        .string(&proposal.paper_project_id.to_string())?
        .string(&proposal.work_item_id.to_string())?
        .string(&proposal.section_key)?
        .string(&proposal.parent_revision_id.to_string())?
        .string(&proposal.lease_id.to_string())?
        .u64(proposal.lease_fencing_token)
        .u64(proposal.expected_work_version)
        .string(&proposal.proposal_kind)?
        .digest(&proposal.payload_hash)?
        .string(&proposal.artifact_manifest_id.to_string())?
        .digest(&proposal.artifact_manifest_hash)?
        .string(&proposal.agent_id)?
        .string(&proposal.binding_id.to_string())?
        .string(&proposal.agent_key_id)?
        .i64(proposal.signed_at_unix)
        .finish())
}

pub fn sign_agent_proposal_v2(
    proposal: &AgentProposalSigningV2,
    signing_key: &SigningKey,
) -> Result<String, String> {
    Ok(BASE64.encode(
        signing_key
            .sign(&agent_proposal_v2_signing_bytes(proposal)?)
            .to_bytes(),
    ))
}

pub fn verify_agent_proposal_v2_signature(
    proposal: &AgentProposalSigningV2,
    signature: &str,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(
            &agent_proposal_v2_signing_bytes(proposal)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "Agent proposal V2 signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HumanDecisionSigningV1 {
    pub schema: String,
    pub decision_id: Uuid,
    pub paper_project_id: Uuid,
    pub proposal_id: Uuid,
    pub player_id: Uuid,
    pub decision: String,
    pub reason_hash: String,
    pub expected_proposal_version: u64,
    pub signing_key_id: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
}

pub fn human_decision_signing_bytes(decision: &HumanDecisionSigningV1) -> Result<Vec<u8>, String> {
    if decision.schema != HUMAN_DECISION_V1
        || decision.signed_at_unix < 0
        || decision.expected_proposal_version == 0
        || !matches!(decision.decision.as_str(), "accept" | "rework" | "reject")
    {
        return Err("human decision schema, decision, version, or time is invalid".to_string());
    }
    validate_text("signing_key_id", &decision.signing_key_id)?;
    Ok(CanonicalFrame::new("hepta_paper_raid_human_decision_v1")
        .string(&decision.schema)?
        .string(&decision.decision_id.to_string())?
        .string(&decision.paper_project_id.to_string())?
        .string(&decision.proposal_id.to_string())?
        .string(&decision.player_id.to_string())?
        .string(&decision.decision)?
        .digest(&decision.reason_hash)?
        .u64(decision.expected_proposal_version)
        .string(&decision.signing_key_id)?
        .digest(&decision.signing_public_key_hash)?
        .i64(decision.signed_at_unix)
        .finish())
}

pub fn sign_human_decision(
    decision: &HumanDecisionSigningV1,
    signing_key: &SigningKey,
) -> Result<String, String> {
    Ok(BASE64.encode(
        signing_key
            .sign(&human_decision_signing_bytes(decision)?)
            .to_bytes(),
    ))
}

pub fn verify_human_decision_signature(
    decision: &HumanDecisionSigningV1,
    signature: &str,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(
            &human_decision_signing_bytes(decision)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "human decision signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HumanEvidenceVerificationSigningV1 {
    pub schema: String,
    pub verification_id: Uuid,
    pub paper_project_id: Uuid,
    pub record_kind: String,
    pub record_id: Uuid,
    pub source_identifier: String,
    pub source_hash: String,
    pub locator: String,
    pub license: String,
    pub player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
}

pub fn human_evidence_verification_signing_bytes(
    verification: &HumanEvidenceVerificationSigningV1,
) -> Result<Vec<u8>, String> {
    if verification.schema != HUMAN_EVIDENCE_VERIFICATION_V1
        || verification.signed_at_unix < 0
        || !matches!(
            verification.record_kind.as_str(),
            "evidence_card" | "citation"
        )
    {
        return Err("human evidence verification schema, kind, or time is invalid".to_string());
    }
    validate_text("locator", &verification.locator)?;
    validate_text("license", &verification.license)?;
    validate_text("source_identifier", &verification.source_identifier)?;
    validate_text("signing_key_id", &verification.signing_key_id)?;
    Ok(
        CanonicalFrame::new("hepta_paper_raid_human_evidence_verification_v1")
            .string(&verification.schema)?
            .string(&verification.verification_id.to_string())?
            .string(&verification.paper_project_id.to_string())?
            .string(&verification.record_kind)?
            .string(&verification.record_id.to_string())?
            .string(&verification.source_identifier)?
            .digest(&verification.source_hash)?
            .string(&verification.locator)?
            .string(&verification.license)?
            .string(&verification.player_id.to_string())?
            .string(&verification.signing_key_id)?
            .digest(&verification.signing_public_key_hash)?
            .i64(verification.signed_at_unix)
            .finish(),
    )
}

pub fn sign_human_evidence_verification(
    verification: &HumanEvidenceVerificationSigningV1,
    signing_key: &SigningKey,
) -> Result<String, String> {
    Ok(BASE64.encode(
        signing_key
            .sign(&human_evidence_verification_signing_bytes(verification)?)
            .to_bytes(),
    ))
}

pub fn verify_human_evidence_verification_signature(
    verification: &HumanEvidenceVerificationSigningV1,
    signature: &str,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(
            &human_evidence_verification_signing_bytes(verification)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "human evidence verification signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SectionReviewSigningV1 {
    pub schema: String,
    pub review_id: Uuid,
    pub paper_project_id: Uuid,
    pub section_revision_id: Uuid,
    pub reviewer_player_id: Uuid,
    pub verdict: String,
    pub review_hash: String,
    pub expected_revision_version: u64,
    pub signing_key_id: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
}

pub fn section_review_signing_bytes(review: &SectionReviewSigningV1) -> Result<Vec<u8>, String> {
    if review.schema != SECTION_REVIEW_V1
        || review.signed_at_unix < 0
        || review.expected_revision_version == 0
        || !matches!(review.verdict.as_str(), "approve" | "rework" | "reject")
    {
        return Err("section review schema, verdict, version, or time is invalid".to_string());
    }
    validate_text("signing_key_id", &review.signing_key_id)?;
    Ok(CanonicalFrame::new("hepta_paper_raid_section_review_v1")
        .string(&review.schema)?
        .string(&review.review_id.to_string())?
        .string(&review.paper_project_id.to_string())?
        .string(&review.section_revision_id.to_string())?
        .string(&review.reviewer_player_id.to_string())?
        .string(&review.verdict)?
        .digest(&review.review_hash)?
        .u64(review.expected_revision_version)
        .string(&review.signing_key_id)?
        .digest(&review.signing_public_key_hash)?
        .i64(review.signed_at_unix)
        .finish())
}

pub fn sign_section_review(
    review: &SectionReviewSigningV1,
    signing_key: &SigningKey,
) -> Result<String, String> {
    Ok(BASE64.encode(
        signing_key
            .sign(&section_review_signing_bytes(review)?)
            .to_bytes(),
    ))
}

pub fn verify_section_review_signature(
    review: &SectionReviewSigningV1,
    signature: &str,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(
            &section_review_signing_bytes(review)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "section review signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SectionMergeSigningV1 {
    pub schema: String,
    pub merge_id: Uuid,
    pub paper_project_id: Uuid,
    pub section_key: String,
    pub section_revision_id: Uuid,
    pub parent_revision_id: Uuid,
    pub merged_section_revision_id: Uuid,
    pub lease_id: Uuid,
    pub fencing_token: u64,
    pub merged_by_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key_hash: String,
    pub merged_at_unix: i64,
}

pub fn section_merge_signing_bytes(merge: &SectionMergeSigningV1) -> Result<Vec<u8>, String> {
    if merge.schema != SECTION_MERGE_V1 || merge.merged_at_unix < 0 || merge.fencing_token == 0 {
        return Err("section merge schema, fencing token, or time is invalid".to_string());
    }
    validate_logical_id("section_key", &merge.section_key)?;
    validate_text("signing_key_id", &merge.signing_key_id)?;
    Ok(CanonicalFrame::new("hepta_paper_raid_section_merge_v1")
        .string(&merge.schema)?
        .string(&merge.merge_id.to_string())?
        .string(&merge.paper_project_id.to_string())?
        .string(&merge.section_key)?
        .string(&merge.section_revision_id.to_string())?
        .string(&merge.parent_revision_id.to_string())?
        .string(&merge.merged_section_revision_id.to_string())?
        .string(&merge.lease_id.to_string())?
        .u64(merge.fencing_token)
        .string(&merge.merged_by_player_id.to_string())?
        .string(&merge.signing_key_id)?
        .digest(&merge.signing_public_key_hash)?
        .i64(merge.merged_at_unix)
        .finish())
}

pub fn sign_section_merge(
    merge: &SectionMergeSigningV1,
    signing_key: &SigningKey,
) -> Result<String, String> {
    Ok(BASE64.encode(
        signing_key
            .sign(&section_merge_signing_bytes(merge)?)
            .to_bytes(),
    ))
}

pub fn verify_section_merge_signature(
    merge: &SectionMergeSigningV1,
    signature: &str,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(
            &section_merge_signing_bytes(merge)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "section merge signature verification failed".to_string())
}

fn optional_uuid(frame: CanonicalFrame, value: Option<Uuid>) -> Result<CanonicalFrame, String> {
    match value {
        Some(value) => frame.u32(1).string(&value.to_string()),
        None => Ok(frame.u32(0)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperEvaluationSigningV1 {
    pub schema: String,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub supersedes_evaluation_id: Option<Uuid>,
    pub tolerance_policy_hash: String,
    pub paper_score_hash: String,
    pub reference_metrics_hash: String,
    pub hard_gates_hash: String,
    pub evaluator_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key_hash: String,
    pub coi_attestation_hash: String,
    pub signed_at_unix: i64,
}

pub fn paper_evaluation_signing_bytes(
    evaluation: &PaperEvaluationSigningV1,
) -> Result<Vec<u8>, String> {
    if evaluation.schema != PAPER_EVALUATION_V1 || evaluation.signed_at_unix < 0 {
        return Err("paper evaluation schema or signing time is invalid".to_string());
    }
    validate_text("signing_key_id", &evaluation.signing_key_id)?;
    let frame = CanonicalFrame::new("hepta_paper_raid_evaluation_v1")
        .string(&evaluation.schema)?
        .string(&evaluation.evaluation_id.to_string())?
        .string(&evaluation.paper_project_id.to_string())?
        .string(&evaluation.submission_id.to_string())?
        .digest(&evaluation.release_candidate_hash)?
        .digest(&evaluation.paper_bundle_hash)?;
    Ok(optional_uuid(frame, evaluation.supersedes_evaluation_id)?
        .digest(&evaluation.tolerance_policy_hash)?
        .digest(&evaluation.paper_score_hash)?
        .digest(&evaluation.reference_metrics_hash)?
        .digest(&evaluation.hard_gates_hash)?
        .string(&evaluation.evaluator_player_id.to_string())?
        .string(&evaluation.signing_key_id)?
        .digest(&evaluation.signing_public_key_hash)?
        .digest(&evaluation.coi_attestation_hash)?
        .i64(evaluation.signed_at_unix)
        .finish())
}

pub fn sign_paper_evaluation(
    evaluation: &PaperEvaluationSigningV1,
    signing_key: &SigningKey,
) -> Result<String, String> {
    Ok(BASE64.encode(
        signing_key
            .sign(&paper_evaluation_signing_bytes(evaluation)?)
            .to_bytes(),
    ))
}

pub fn verify_paper_evaluation_signature(
    evaluation: &PaperEvaluationSigningV1,
    signature: &str,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(
            &paper_evaluation_signing_bytes(evaluation)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "paper evaluation signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperReviewAttestationSigningV1 {
    pub schema: String,
    pub attestation_id: Uuid,
    pub evaluation_id: Uuid,
    pub evaluation_signing_hash: String,
    pub reviewer_player_id: Uuid,
    pub verdict: String,
    pub signing_key_id: String,
    pub signing_public_key_hash: String,
    pub coi_attestation_hash: String,
    pub signed_at_unix: i64,
}

pub fn paper_review_attestation_signing_bytes(
    review: &PaperReviewAttestationSigningV1,
) -> Result<Vec<u8>, String> {
    if review.schema != PAPER_REVIEW_ATTESTATION_V1
        || review.signed_at_unix < 0
        || !matches!(review.verdict.as_str(), "approve" | "reject")
    {
        return Err("paper review schema, verdict, or signing time is invalid".to_string());
    }
    validate_text("signing_key_id", &review.signing_key_id)?;
    Ok(
        CanonicalFrame::new("hepta_paper_raid_review_attestation_v1")
            .string(&review.schema)?
            .string(&review.attestation_id.to_string())?
            .string(&review.evaluation_id.to_string())?
            .digest(&review.evaluation_signing_hash)?
            .string(&review.reviewer_player_id.to_string())?
            .string(&review.verdict)?
            .string(&review.signing_key_id)?
            .digest(&review.signing_public_key_hash)?
            .digest(&review.coi_attestation_hash)?
            .i64(review.signed_at_unix)
            .finish(),
    )
}

pub fn verify_paper_review_attestation_signature(
    review: &PaperReviewAttestationSigningV1,
    signature: &str,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(
            &paper_review_attestation_signing_bytes(review)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "paper review attestation signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperReproductionSigningV1 {
    pub schema: String,
    pub reproduction_id: Uuid,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub tolerance_policy_hash: String,
    pub observed_metrics_hash: String,
    pub statistical_evidence_hash: String,
    pub seed_set_hash: String,
    pub environment_hash: String,
    pub run_manifest_hash: String,
    pub supersedes_reproduction_id: Option<Uuid>,
    pub reproducer_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key_hash: String,
    pub coi_attestation_hash: String,
    pub signed_at_unix: i64,
}

pub fn paper_reproduction_signing_bytes(
    reproduction: &PaperReproductionSigningV1,
) -> Result<Vec<u8>, String> {
    if reproduction.schema != PAPER_REPRODUCTION_V1 || reproduction.signed_at_unix < 0 {
        return Err("paper reproduction schema or signing time is invalid".to_string());
    }
    validate_text("signing_key_id", &reproduction.signing_key_id)?;
    let frame = CanonicalFrame::new("hepta_paper_raid_reproduction_v1")
        .string(&reproduction.schema)?
        .string(&reproduction.reproduction_id.to_string())?
        .string(&reproduction.evaluation_id.to_string())?
        .string(&reproduction.paper_project_id.to_string())?
        .digest(&reproduction.release_candidate_hash)?
        .digest(&reproduction.paper_bundle_hash)?
        .digest(&reproduction.tolerance_policy_hash)?
        .digest(&reproduction.observed_metrics_hash)?
        .digest(&reproduction.statistical_evidence_hash)?
        .digest(&reproduction.seed_set_hash)?
        .digest(&reproduction.environment_hash)?
        .digest(&reproduction.run_manifest_hash)?;
    Ok(
        optional_uuid(frame, reproduction.supersedes_reproduction_id)?
            .string(&reproduction.reproducer_player_id.to_string())?
            .string(&reproduction.signing_key_id)?
            .digest(&reproduction.signing_public_key_hash)?
            .digest(&reproduction.coi_attestation_hash)?
            .i64(reproduction.signed_at_unix)
            .finish(),
    )
}

pub fn verify_paper_reproduction_signature(
    reproduction: &PaperReproductionSigningV1,
    signature: &str,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(
            &paper_reproduction_signing_bytes(reproduction)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "paper reproduction signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperAppealSigningV1 {
    pub schema: String,
    pub appeal_id: Uuid,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub release_candidate_hash: String,
    pub appellant_player_id: Uuid,
    pub grounds_hash: String,
    pub evidence_manifest_hash: String,
    pub signing_key_id: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
}

pub fn paper_appeal_signing_bytes(appeal: &PaperAppealSigningV1) -> Result<Vec<u8>, String> {
    if appeal.schema != PAPER_APPEAL_V1 || appeal.signed_at_unix < 0 {
        return Err("paper appeal schema or signing time is invalid".to_string());
    }
    validate_text("signing_key_id", &appeal.signing_key_id)?;
    Ok(CanonicalFrame::new("hepta_paper_raid_appeal_v1")
        .string(&appeal.schema)?
        .string(&appeal.appeal_id.to_string())?
        .string(&appeal.evaluation_id.to_string())?
        .string(&appeal.paper_project_id.to_string())?
        .digest(&appeal.release_candidate_hash)?
        .string(&appeal.appellant_player_id.to_string())?
        .digest(&appeal.grounds_hash)?
        .digest(&appeal.evidence_manifest_hash)?
        .string(&appeal.signing_key_id)?
        .digest(&appeal.signing_public_key_hash)?
        .i64(appeal.signed_at_unix)
        .finish())
}

pub fn verify_paper_appeal_signature(
    appeal: &PaperAppealSigningV1,
    signature: &str,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(
            &paper_appeal_signing_bytes(appeal)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "paper appeal signature verification failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperAppealResolutionSigningV1 {
    pub schema: String,
    pub resolution_id: Uuid,
    pub appeal_id: Uuid,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub release_candidate_hash: String,
    pub outcome: String,
    pub superseding_evaluation_id: Option<Uuid>,
    pub decision_hash: String,
    pub resolver_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
}

/// Human-author authorization for reopening a terminally rejected Paper.
///
/// The frame deliberately binds the rejected immutable evaluation and
/// submission, plus the exact optimistic Paper version.  The replacement
/// revision and PaperBundle do not exist yet; they are bound later by a
/// separate immutable resubmission record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperReworkSigningV1 {
    pub schema: String,
    pub rework_id: Uuid,
    pub paper_project_id: Uuid,
    pub rejected_evaluation_id: Uuid,
    pub rejected_submission_id: Uuid,
    pub expected_paper_version: u64,
    pub rework_cycle: u64,
    pub author_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key_hash: String,
    pub reason_hash: String,
    pub signed_at_unix: i64,
}

pub fn paper_rework_signing_bytes(rework: &PaperReworkSigningV1) -> Result<Vec<u8>, String> {
    if rework.schema != PAPER_REWORK_V1
        || rework.expected_paper_version == 0
        || rework.expected_paper_version > JSON_SAFE_U64_MAX
        || rework.rework_cycle < 2
        || rework.rework_cycle > JSON_SAFE_U64_MAX
        || rework.signed_at_unix < 0
    {
        return Err("Paper rework schema, version, cycle, or timestamp is invalid".to_string());
    }
    validate_key_id("signing_key_id", &rework.signing_key_id)?;
    decode_digest(&rework.signing_public_key_hash)?;
    decode_digest(&rework.reason_hash)?;
    Ok(CanonicalFrame::new("hepta_paper_raid_rework_v1")
        .string(&rework.schema)?
        .string(&rework.rework_id.to_string())?
        .string(&rework.paper_project_id.to_string())?
        .string(&rework.rejected_evaluation_id.to_string())?
        .string(&rework.rejected_submission_id.to_string())?
        .u64(rework.expected_paper_version)
        .u64(rework.rework_cycle)
        .string(&rework.author_player_id.to_string())?
        .string(&rework.signing_key_id)?
        .digest(&rework.signing_public_key_hash)?
        .digest(&rework.reason_hash)?
        .i64(rework.signed_at_unix)
        .finish())
}

pub fn verify_paper_rework_signature(
    rework: &PaperReworkSigningV1,
    signature: &str,
    key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("Paper rework signature", signature)?;
    key.verify(
        &paper_rework_signing_bytes(rework)?,
        &Signature::from_bytes(&signature),
    )
    .map_err(|_| "Paper rework signature verification failed".to_string())
}

pub fn paper_appeal_resolution_signing_bytes(
    resolution: &PaperAppealResolutionSigningV1,
) -> Result<Vec<u8>, String> {
    if resolution.schema != PAPER_APPEAL_RESOLUTION_V1
        || resolution.signed_at_unix < 0
        || !matches!(resolution.outcome.as_str(), "upheld" | "denied")
    {
        return Err("paper appeal resolution schema, outcome, or time is invalid".to_string());
    }
    validate_text("signing_key_id", &resolution.signing_key_id)?;
    let frame = CanonicalFrame::new("hepta_paper_raid_appeal_resolution_v1")
        .string(&resolution.schema)?
        .string(&resolution.resolution_id.to_string())?
        .string(&resolution.appeal_id.to_string())?
        .string(&resolution.evaluation_id.to_string())?
        .string(&resolution.paper_project_id.to_string())?
        .digest(&resolution.release_candidate_hash)?
        .string(&resolution.outcome)?;
    Ok(optional_uuid(frame, resolution.superseding_evaluation_id)?
        .digest(&resolution.decision_hash)?
        .string(&resolution.resolver_player_id.to_string())?
        .string(&resolution.signing_key_id)?
        .digest(&resolution.signing_public_key_hash)?
        .i64(resolution.signed_at_unix)
        .finish())
}

pub fn verify_paper_appeal_resolution_signature(
    resolution: &PaperAppealResolutionSigningV1,
    signature: &str,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = decode_base64_exact::<64>("signature", signature)?;
    verifying_key
        .verify(
            &paper_appeal_resolution_signing_bytes(resolution)?,
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "paper appeal resolution signature verification failed".to_string())
}

#[cfg(test)]
mod frozen_review_manifest_tests {
    use super::*;

    fn evaluator_bytes() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "entrypoint":"evaluator.py",
            "frozen":true,
            "objects":[{
                "cas_uri":format!("cas://sha256/{}", "a".repeat(64)),
                "media_type":"text/x-python; charset=utf-8",
                "path":"evaluator.py",
                "sha256":format!("sha256:{}", "a".repeat(64)),
                "size":7
            }],
            "pack_id":"pack-fixture-v1",
            "runtime":"python3-stdlib",
            "schema":"hepta.challenge_pack.evaluator_manifest.v1"
        }))
        .unwrap()
    }

    #[test]
    fn strict_manifest_parser_binds_bytes_and_rejects_hostile_shapes() {
        let bytes = evaluator_bytes();
        let digest = sha256_digest(&bytes);
        let parsed = parse_challenge_evaluator_manifest(&bytes, &digest).unwrap();
        assert_eq!(parsed.entrypoint, "evaluator.py");
        assert!(
            parse_challenge_evaluator_manifest(&bytes, &format!("sha256:{}", "b".repeat(64)))
                .is_err()
        );

        let traversal = serde_json::to_vec(&serde_json::json!({
            "entrypoint":"../evaluator.py",
            "frozen":true,
            "objects":[{
                "cas_uri":format!("cas://sha256/{}", "a".repeat(64)),
                "media_type":"text/x-python; charset=utf-8",
                "path":"../evaluator.py",
                "sha256":format!("sha256:{}", "a".repeat(64)),
                "size":7
            }],
            "pack_id":"pack-fixture-v1",
            "runtime":"python3-stdlib",
            "schema":"hepta.challenge_pack.evaluator_manifest.v1"
        }))
        .unwrap();
        let traversal_digest = sha256_digest(&traversal);
        assert!(parse_challenge_evaluator_manifest(&traversal, &traversal_digest).is_err());

        let extra = serde_json::to_vec(&serde_json::json!({
            "entrypoint":"evaluator.py",
            "extra_authority":"forbidden",
            "frozen":true,
            "objects":[{
                "cas_uri":format!("cas://sha256/{}", "a".repeat(64)),
                "media_type":"text/x-python; charset=utf-8",
                "path":"evaluator.py",
                "sha256":format!("sha256:{}", "a".repeat(64)),
                "size":7
            }],
            "pack_id":"pack-fixture-v1",
            "runtime":"python3-stdlib",
            "schema":"hepta.challenge_pack.evaluator_manifest.v1"
        }))
        .unwrap();
        let extra_digest = sha256_digest(&extra);
        assert!(parse_challenge_evaluator_manifest(&extra, &extra_digest).is_err());
    }

    fn challenge_material_fixture() -> (
        ChallengePackManifestV1,
        ChallengeEvaluatorManifestV1,
        ChallengeDatasetManifestV1,
    ) {
        let digest = |byte: char| format!("sha256:{}", byte.to_string().repeat(64));
        let member =
            |path: &str, media_type: &str, byte: char, size: u64| ChallengeManifestObjectV1 {
                cas_uri: format!("cas://sha256/{}", byte.to_string().repeat(64)),
                media_type: media_type.to_string(),
                path: path.to_string(),
                sha256: digest(byte),
                size,
            };
        let evaluator = ChallengeEvaluatorManifestV1 {
            entrypoint: "evaluator.py".to_string(),
            frozen: true,
            objects: vec![
                member("baseline.py", "text/x-python; charset=utf-8", 'b', 11),
                member("evaluator.py", "text/x-python; charset=utf-8", 'e', 13),
            ],
            pack_id: "pack-fixture-v1".to_string(),
            runtime: "python3-stdlib".to_string(),
            schema: "hepta.challenge_pack.evaluator_manifest.v1".to_string(),
        };
        let dataset = ChallengeDatasetManifestV1 {
            objects: vec![member("dataset/claims.json", "application/json", 'd', 17)],
            pack_id: evaluator.pack_id.clone(),
            schema: "hepta.challenge_pack.dataset_manifest.v1".to_string(),
        };
        let evaluator_manifest_hash = sha256_digest(&serde_json::to_vec(&evaluator).unwrap());
        let dataset_manifest_hash = sha256_digest(&serde_json::to_vec(&dataset).unwrap());
        let pack_member = |path: &str, role: &str, media_type: &str, sha256: String, size: u64| {
            ChallengePackObjectV1 {
                cas_uri: format!("cas://sha256/{}", sha256.strip_prefix("sha256:").unwrap()),
                media_type: media_type.to_string(),
                path: path.to_string(),
                role: role.to_string(),
                sha256,
                size,
            }
        };
        let pack = ChallengePackManifestV1 {
            content_contract: ChallengePackContentContractV1 {
                difficulty: "introductory".to_string(),
                duration_seconds: 2_700,
                modifiers: vec!["frozen-evaluator".to_string()],
                objective: "Audit the frozen claims.".to_string(),
                risks: vec!["citation-mismatch".to_string()],
                victory: "Every claim is resolved.".to_string(),
            },
            dataset_manifest_sha256: dataset_manifest_hash.clone(),
            deployment: ChallengePackDeploymentV1 {
                blocker: "challenge_pack_cas_objects_not_seeded".to_string(),
                cas_seeded: false,
                open_status_allowed: false,
            },
            evaluator_manifest_sha256: evaluator_manifest_hash.clone(),
            objects: vec![
                pack_member(
                    "LICENSE.txt",
                    "license",
                    "text/plain; charset=utf-8",
                    digest('1'),
                    5,
                ),
                pack_member(
                    "baseline.py",
                    "baseline_code",
                    "text/x-python; charset=utf-8",
                    digest('b'),
                    11,
                ),
                pack_member(
                    "brief.md",
                    "playable_brief",
                    "text/markdown; charset=utf-8",
                    digest('2'),
                    7,
                ),
                pack_member(
                    "dataset.manifest.json",
                    "dataset_manifest",
                    "application/json",
                    dataset_manifest_hash,
                    19,
                ),
                pack_member(
                    "dataset/claims.json",
                    "dataset",
                    "application/json",
                    digest('d'),
                    17,
                ),
                pack_member(
                    "evaluator.manifest.json",
                    "evaluator_manifest",
                    "application/json",
                    evaluator_manifest_hash,
                    23,
                ),
                pack_member(
                    "evaluator.py",
                    "frozen_evaluator",
                    "text/x-python; charset=utf-8",
                    digest('e'),
                    13,
                ),
                pack_member(
                    "result-explanation.md",
                    "result_explanation",
                    "text/markdown; charset=utf-8",
                    digest('3'),
                    29,
                ),
            ],
            pack_id: evaluator.pack_id.clone(),
            ruleset_version: "paper-raid-evidence-audit-v1".to_string(),
            schema: "hepta.challenge_pack.v1".to_string(),
            seed: 1_701,
            template: "evidence-audit".to_string(),
        };
        (pack, evaluator, dataset)
    }

    #[test]
    fn frozen_pack_parser_and_material_projection_reject_substitution() {
        let (pack, evaluator, dataset) = challenge_material_fixture();
        let bytes = serde_json::to_vec(&pack).unwrap();
        let digest = sha256_digest(&bytes);
        let parsed = parse_challenge_pack_manifest(&bytes, &digest).unwrap();
        let objects = resolve_challenge_material_objects(&parsed, &evaluator, &dataset).unwrap();
        assert_eq!(objects.len(), 4);
        assert_eq!(objects[0].role, "playable_brief");
        assert_eq!(objects[1].logical_path, "challenge/dataset.json");
        assert_eq!(objects[2].role, "baseline_code");
        assert_eq!(objects[3].role, "frozen_evaluator");

        let mut substituted = parsed.clone();
        substituted
            .objects
            .iter_mut()
            .find(|object| object.role == "dataset")
            .unwrap()
            .sha256 = format!("sha256:{}", "9".repeat(64));
        assert!(resolve_challenge_material_objects(&substituted, &evaluator, &dataset).is_err());

        let mut extra = serde_json::to_value(pack).unwrap();
        extra["unfrozen_catalog_hint"] = serde_json::json!(true);
        let extra_bytes = serde_json::to_vec(&extra).unwrap();
        let extra_digest = sha256_digest(&extra_bytes);
        assert!(parse_challenge_pack_manifest(&extra_bytes, &extra_digest).is_err());
    }

    #[test]
    fn assigned_material_bundle_hash_binds_activation_and_work_item_assignment() {
        let (pack, evaluator, dataset) = challenge_material_fixture();
        let mut authority = FrozenChallengeMaterialAuthorityV1 {
            schema: FROZEN_CHALLENGE_MATERIAL_AUTHORITY_V1.to_string(),
            authority_hash: String::new(),
            activation_id: Uuid::from_u128(1),
            activation_request_sha256: format!("sha256:{}", "1".repeat(64)),
            challenge_id: Uuid::from_u128(2),
            challenge_snapshot_hash: format!("sha256:{}", "2".repeat(64)),
            template: pack.template.clone(),
            pack_id: pack.pack_id.clone(),
            pack_manifest_hash: format!("sha256:{}", "3".repeat(64)),
            ruleset_version: pack.ruleset_version.clone(),
            ruleset_hash: format!("sha256:{}", "4".repeat(64)),
            dataset_manifest_hash: pack.dataset_manifest_sha256.clone(),
            evaluator_manifest_hash: pack.evaluator_manifest_sha256.clone(),
        };
        authority.authority_hash = frozen_challenge_material_authority_hash(&authority).unwrap();
        verify_frozen_challenge_material_authority(&authority).unwrap();
        for (field, value) in [
            (
                "qualification_id",
                serde_json::json!(LEGACY_GOLDEN_QUALIFICATION_ID),
            ),
            ("unknown_authority", serde_json::json!(true)),
        ] {
            let mut mixed = serde_json::to_value(&authority).unwrap();
            mixed[field] = value;
            assert!(
                serde_json::from_value::<FrozenChallengeMaterialAuthorityBindingV1>(mixed).is_err()
            );
        }
        let mut bundle = AssignedChallengeMaterialBundleV1 {
            schema: ASSIGNED_CHALLENGE_MATERIAL_BUNDLE_V1.to_string(),
            bundle_hash: String::new(),
            authority: FrozenChallengeMaterialAuthorityBindingV1::PackActivation(authority.clone()),
            authority_hash: authority.authority_hash.clone(),
            paper_project_id: Uuid::from_u128(3),
            challenge_ruleset_snapshot_hash: format!("sha256:{}", "5".repeat(64)),
            binding_id: Uuid::from_u128(4),
            player_id: Uuid::from_u128(5),
            work_item_id: Uuid::from_u128(6),
            work_item_version: 1,
            objects: resolve_challenge_material_objects(&pack, &evaluator, &dataset).unwrap(),
        };
        bundle.bundle_hash = assigned_challenge_material_bundle_hash(&bundle).unwrap();
        verify_assigned_challenge_material_bundle(&bundle).unwrap();

        let mut foreign_assignment = bundle.clone();
        foreign_assignment.work_item_id = Uuid::from_u128(7);
        assert_ne!(
            assigned_challenge_material_bundle_hash(&foreign_assignment).unwrap(),
            bundle.bundle_hash
        );
        assert!(verify_assigned_challenge_material_bundle(&foreign_assignment).is_err());
        let mut unfrozen = bundle;
        let FrozenChallengeMaterialAuthorityBindingV1::PackActivation(authority) =
            &mut unfrozen.authority
        else {
            panic!("activation fixture changed provenance")
        };
        authority.authority_hash = format!("sha256:{}", "8".repeat(64));
        assert!(assigned_challenge_material_bundle_hash(&unfrozen).is_err());
    }

    #[test]
    fn legacy_golden_qualification_binds_exact_contract_without_activation_claim() {
        let mut authority = LegacyGoldenQualificationMaterialAuthorityV1 {
            schema: LEGACY_GOLDEN_QUALIFICATION_MATERIAL_AUTHORITY_V1.to_string(),
            authority_hash: String::new(),
            qualification_id: LEGACY_GOLDEN_QUALIFICATION_ID.to_string(),
            challenge_id: Uuid::from_u128(20),
            challenge_snapshot_hash: format!("sha256:{}", "a".repeat(64)),
            challenge_title: LEGACY_GOLDEN_CHALLENGE_TITLE.to_string(),
            challenge_description: LEGACY_GOLDEN_CHALLENGE_DESCRIPTION.to_string(),
            challenge_status: "open".to_string(),
            ruleset_version: LEGACY_GOLDEN_CHALLENGE_RULESET_VERSION.to_string(),
            ruleset_hash: LEGACY_GOLDEN_CHALLENGE_RULESET_HASH.to_string(),
            ruleset_absent: true,
            dataset_manifest_hash: LEGACY_GOLDEN_DATASET_MANIFEST_HASH.to_string(),
            evaluator_manifest_hash: LEGACY_GOLDEN_EVALUATOR_MANIFEST_HASH.to_string(),
            objects: legacy_golden_qualification_material_objects(),
        };
        authority.authority_hash =
            legacy_golden_qualification_material_authority_hash(&authority).unwrap();
        verify_legacy_golden_qualification_material_authority(&authority).unwrap();

        let serialized = serde_json::to_value(
            FrozenChallengeMaterialAuthorityBindingV1::LegacyGoldenQualification(authority.clone()),
        )
        .unwrap();
        assert!(serialized.get("activation_id").is_none());
        assert_eq!(
            serialized
                .get("qualification_id")
                .and_then(serde_json::Value::as_str),
            Some(LEGACY_GOLDEN_QUALIFICATION_ID)
        );
        for (field, value) in [
            ("activation_id", serde_json::json!(Uuid::from_u128(21))),
            ("unknown_authority", serde_json::json!(true)),
        ] {
            let mut mixed = serialized.clone();
            mixed[field] = value;
            assert!(
                serde_json::from_value::<FrozenChallengeMaterialAuthorityBindingV1>(mixed).is_err()
            );
        }

        for field in [
            "challenge_title",
            "challenge_description",
            "ruleset_version",
            "ruleset_hash",
            "dataset_manifest_hash",
            "evaluator_manifest_hash",
        ] {
            let mut mutant = serde_json::to_value(&authority).unwrap();
            mutant[field] = serde_json::json!("tampered");
            let mutant: LegacyGoldenQualificationMaterialAuthorityV1 =
                serde_json::from_value(mutant).unwrap();
            assert!(verify_legacy_golden_qualification_material_authority(&mutant).is_err());
        }
        for index in 0..4 {
            let mut mutant = authority.clone();
            mutant.objects[index].digest = format!("sha256:{}", index.to_string().repeat(64));
            assert!(verify_legacy_golden_qualification_material_authority(&mutant).is_err());
        }
    }

    #[test]
    fn authority_and_resolved_bundle_hashes_bind_every_manifest_pin_and_member() {
        let object = FrozenReviewObjectV1 {
            object_key: "object-0004".to_string(),
            logical_path: "evaluator.py".to_string(),
            role: "frozen_evaluator".to_string(),
            digest: format!("sha256:{}", "a".repeat(64)),
            size_bytes: 7,
            media_type: "text/x-python; charset=utf-8".to_string(),
            download_path: REVIEW_OBJECT_DOWNLOAD_PATH_V1.to_string(),
        };
        let mut authority = FrozenReviewAuthorityV1 {
            schema: FROZEN_REVIEW_AUTHORITY_V1.to_string(),
            authority_hash: String::new(),
            assignment_id: Uuid::from_u128(1),
            paper_project_id: Uuid::from_u128(2),
            submission_id: Uuid::from_u128(3),
            review_round: 1,
            slot: "evaluator".to_string(),
            assignment_version: 1,
            expires_at: "2026-08-12T00:00:00Z".to_string(),
            release_candidate_hash: format!("sha256:{}", "1".repeat(64)),
            paper_bundle_hash: format!("sha256:{}", "2".repeat(64)),
            artifact_manifest_hash: format!("sha256:{}", "3".repeat(64)),
            evaluator_manifest_hash: format!("sha256:{}", "4".repeat(64)),
            dataset_manifest_hash: format!("sha256:{}", "5".repeat(64)),
            artifact_objects: vec![
                FrozenReviewObjectV1 {
                    object_key: "object-0000".to_string(),
                    logical_path: "paper/references.bib".to_string(),
                    role: "bibliography".to_string(),
                    digest: format!("sha256:{}", "d".repeat(64)),
                    media_type: "application/x-bibtex".to_string(),
                    ..object.clone()
                },
                FrozenReviewObjectV1 {
                    object_key: "object-0001".to_string(),
                    logical_path: "candidate.json".to_string(),
                    role: "candidate".to_string(),
                    digest: format!("sha256:{}", "c".repeat(64)),
                    media_type: "application/json".to_string(),
                    ..object.clone()
                },
                FrozenReviewObjectV1 {
                    object_key: "object-0002".to_string(),
                    logical_path: "paper/claim-evidence.json".to_string(),
                    role: "claim_evidence_graph".to_string(),
                    digest: format!("sha256:{}", "e".repeat(64)),
                    media_type: "application/json".to_string(),
                    ..object.clone()
                },
                FrozenReviewObjectV1 {
                    object_key: "object-0003".to_string(),
                    logical_path: "dataset.json".to_string(),
                    role: "dataset".to_string(),
                    digest: format!("sha256:{}", "b".repeat(64)),
                    media_type: "application/json".to_string(),
                    ..object.clone()
                },
                object.clone(),
                FrozenReviewObjectV1 {
                    object_key: "object-0005".to_string(),
                    logical_path: "paper/paper.md".to_string(),
                    role: "paper_source".to_string(),
                    digest: format!("sha256:{}", "f".repeat(64)),
                    media_type: "text/markdown; charset=utf-8".to_string(),
                    ..object.clone()
                },
            ],
            execution_policy: FrozenReviewExecutionPolicyV1 {
                schema: "hepta.paper_raid.review_execution_policy.v1".to_string(),
                kind: "evaluate".to_string(),
                adapter: "python3-stdlib-v1".to_string(),
                timeout_ms: 30_000,
                seed: 7,
            },
        };
        authority.authority_hash = frozen_review_authority_hash(&authority).unwrap();
        verify_frozen_review_authority(&authority).unwrap();
        let mut transport_objects = authority
            .artifact_objects
            .iter()
            .filter(|candidate| review_object_role_is_executable(&candidate.role))
            .cloned()
            .collect::<Vec<_>>();
        for transport in &mut transport_objects {
            transport.logical_path = match transport.role.as_str() {
                "frozen_evaluator" => "evaluator/main.py",
                "dataset" => "inputs/dataset.json",
                "candidate" => "inputs/candidate.json",
                _ => unreachable!(),
            }
            .to_string();
        }
        transport_objects.sort_by(|left, right| {
            (&left.object_key, &left.logical_path).cmp(&(&right.object_key, &right.logical_path))
        });
        let mut bundle = FrozenReviewBundleV1 {
            schema: RESOLVED_FROZEN_REVIEW_BUNDLE_V1.to_string(),
            bundle_hash: String::new(),
            authority: authority.clone(),
            authority_hash: authority.authority_hash.clone(),
            assignment_id: authority.assignment_id,
            paper_project_id: authority.paper_project_id,
            submission_id: authority.submission_id,
            review_round: authority.review_round,
            slot: authority.slot.clone(),
            assignment_version: authority.assignment_version,
            expires_at: authority.expires_at.clone(),
            release_candidate_hash: authority.release_candidate_hash.clone(),
            paper_bundle_hash: authority.paper_bundle_hash.clone(),
            artifact_manifest_hash: authority.artifact_manifest_hash.clone(),
            evaluator_manifest_hash: authority.evaluator_manifest_hash.clone(),
            dataset_manifest_hash: authority.dataset_manifest_hash.clone(),
            objects: transport_objects,
            execution: FrozenReviewExecutionPlanV1 {
                schema: "hepta.paper_raid.review_execution_plan.v1".to_string(),
                kind: "evaluate".to_string(),
                adapter: "python3-stdlib-v1".to_string(),
                evaluator_version: object.digest.clone(),
                entrypoint: "evaluator/main.py".to_string(),
                timeout_ms: 30_000,
                seed: 7,
            },
        };
        bundle.bundle_hash = frozen_review_bundle_hash(&bundle).unwrap();
        verify_frozen_review_bundle(&bundle).unwrap();
        let mut path_tamper = bundle.clone();
        path_tamper.objects[0].logical_path = "evaluator/other.py".to_string();
        assert!(frozen_review_bundle_hash(&path_tamper).is_err());
        let mut authority_member_tamper = bundle;
        authority_member_tamper.objects[0].digest = format!("sha256:{}", "9".repeat(64));
        assert!(frozen_review_bundle_hash(&authority_member_tamper).is_err());

        let original = authority.authority_hash.clone();
        authority.dataset_manifest_hash = format!("sha256:{}", "6".repeat(64));
        assert_ne!(frozen_review_authority_hash(&authority).unwrap(), original);
        assert!(verify_frozen_review_authority(&authority).is_err());
    }

    fn review_receipt(kind: &str, candidate_passed: Option<bool>) -> ReviewExecutionReceiptV1 {
        let digest = format!("sha256:{}", "a".repeat(64));
        ReviewExecutionReceiptV1 {
            schema: REVIEW_EXECUTION_RECEIPT_V1.to_string(),
            receipt_id: Uuid::from_u128(1),
            task_id: Uuid::from_u128(2),
            binding_id: Uuid::from_u128(3),
            assignment_id: Uuid::from_u128(4),
            paper_project_id: Uuid::from_u128(5),
            submission_id: Uuid::from_u128(6),
            evaluation_id: Uuid::from_u128(7),
            kind: kind.to_string(),
            attempt: 1,
            fencing_token: 1,
            bundle_hash: digest.clone(),
            evaluator_version: "fixture-v1".to_string(),
            input_root: digest.clone(),
            output_root: digest.clone(),
            metrics_hash: digest.clone(),
            observed_metrics_micros: BTreeMap::from([("score".to_string(), 1)]),
            statistical_evidence: serde_json::json!({
                "schema": "hepta.paper_raid.statistical_evidence.none.v1",
                "reason": "evaluation_has_no_observed_statistical_evidence"
            }),
            candidate_passed,
            seed_set_hash: digest.clone(),
            environment_hash: digest.clone(),
            run_manifest_hash: digest.clone(),
            logs_hash: digest.clone(),
            started_at_unix: 1,
            completed_at_unix: 2,
            agent_id: "did:trnm:fixture-agent".to_string(),
            agent_key_id: digest.clone(),
            signing_public_key_hash: digest,
            signature: String::new(),
        }
    }

    #[test]
    fn execution_receipt_kind_strictly_binds_candidate_passed() {
        let failed = review_receipt("evaluate", Some(false));
        let passed = review_receipt("evaluate", Some(true));
        assert_ne!(
            review_execution_receipt_signing_bytes(&failed).unwrap(),
            review_execution_receipt_signing_bytes(&passed).unwrap()
        );
        assert_ne!(
            review_execution_metrics_hash(&failed).unwrap(),
            review_execution_metrics_hash(&passed).unwrap()
        );
        assert!(review_execution_receipt_signing_bytes(&review_receipt("evaluate", None)).is_err());
        assert!(
            review_execution_receipt_signing_bytes(&review_receipt("reproduce", Some(true)))
                .is_err()
        );
        assert!(review_execution_receipt_signing_bytes(&review_receipt("reproduce", None)).is_ok());
    }

    #[test]
    fn review_execution_receipt_id_matches_the_bridge_golden_vector() {
        let id = review_execution_receipt_id(
            Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
            &format!("sha256:{}", "a".repeat(64)),
            2,
            7,
        )
        .unwrap();
        assert_eq!(id.to_string(), "445f1306-eda6-5a10-bcd9-bbc15e349bce");
    }
}

#[cfg(test)]
mod agent_proposal_v2_frozen_tests {
    use super::*;

    fn vector() -> AgentProposalSigningV2 {
        AgentProposalSigningV2 {
            schema: AGENT_PROPOSAL_V2.to_string(),
            proposal_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            paper_project_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            work_item_id: Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
            section_key: "results.main".to_string(),
            parent_revision_id: Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap(),
            lease_id: Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap(),
            lease_fencing_token: 7,
            expected_work_version: 11,
            proposal_kind: "delivery".to_string(),
            payload_hash: "sha256:7917212537bd6e80eb59be660839509f2c0319c23e7236ab589e1bf6868e598b"
                .to_string(),
            artifact_manifest_id: Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap(),
            artifact_manifest_hash:
                "sha256:fad5d89eff2f29912c8c10f4cb411fbc87b0dbb8d916258c741edb39575c388c"
                    .to_string(),
            agent_id: "did:trnm:agent-proposal-v2".to_string(),
            binding_id: Uuid::parse_str("77777777-7777-4777-8777-777777777777").unwrap(),
            agent_key_id: "sha256:3097e2dee2cb4a34b53840cdb705aed71067c36f68db0e0f559c3f3fa043315f"
                .to_string(),
            signed_at_unix: 1_770_000_000,
        }
    }

    #[test]
    fn agent_proposal_v2_frozen_frame_and_signature() {
        let proposal = vector();
        let signing_key = SigningKey::from_bytes(&[0x42; 32]);
        assert_eq!(
            BASE64.encode(signing_key.verifying_key().to_bytes()),
            "IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI="
        );
        let frame = agent_proposal_v2_signing_bytes(&proposal).unwrap();
        assert_eq!(
            sha256_digest(&frame),
            "sha256:b285cf8c2b1609a73afea9dfb7c4af2ef5720c82eded01e793e6516df4d694be"
        );
        let signature = sign_agent_proposal_v2(&proposal, &signing_key).unwrap();
        assert_eq!(
            signature,
            "tPgBHp6Aypntpam1qkp8h14l1x4wRWn+mEfsQsqb3N79YIiKExuQEG2mEgUgxkR1HiWsV7I/daUXzXxfuZ6uCw=="
        );
        verify_agent_proposal_v2_signature(&proposal, &signature, &signing_key.verifying_key())
            .unwrap();
    }

    #[test]
    fn agent_proposal_v2_binds_epoch_work_version_and_manifest_identity() {
        let proposal = vector();
        let signing_key = SigningKey::from_bytes(&[0x42; 32]);
        let signature = sign_agent_proposal_v2(&proposal, &signing_key).unwrap();
        for tampered in [
            AgentProposalSigningV2 {
                lease_id: Uuid::new_v4(),
                ..proposal.clone()
            },
            AgentProposalSigningV2 {
                lease_fencing_token: proposal.lease_fencing_token + 1,
                ..proposal.clone()
            },
            AgentProposalSigningV2 {
                expected_work_version: proposal.expected_work_version + 1,
                ..proposal.clone()
            },
            AgentProposalSigningV2 {
                artifact_manifest_id: Uuid::new_v4(),
                ..proposal.clone()
            },
        ] {
            assert!(verify_agent_proposal_v2_signature(
                &tampered,
                &signature,
                &signing_key.verifying_key()
            )
            .is_err());
        }
    }
}

#[cfg(test)]
mod agent_bridge_challenge_request_tests {
    use super::*;

    fn challenge_get_claim() -> AgentBridgeRequestProofV1 {
        AgentBridgeRequestProofV1 {
            schema: AGENT_BRIDGE_REQUEST_PROOF_V1.to_string(),
            binding_id: Uuid::from_u128(1),
            agent_id: "did:trnm:challenge-material-agent".to_string(),
            agent_key_id: format!("sha256:{}", "a".repeat(64)),
            http_method: "GET".to_string(),
            canonical_path: CHALLENGE_MATERIAL_OBJECT_DOWNLOAD_PATH_V1.to_string(),
            canonical_query: format!(
                "bundle_hash=sha256%3A{}&digest=sha256%3A{}&object_key=brief&paper_id={}&work_item_id={}",
                "b".repeat(64),
                "c".repeat(64),
                Uuid::from_u128(2),
                Uuid::from_u128(3),
            ),
            body_hash: sha256_digest(&[]),
            nonce: Uuid::from_u128(4),
            issued_at_unix: 1_000,
            expires_at_unix: 1_060,
        }
    }

    #[test]
    fn challenge_get_route_and_empty_body_are_frozen_into_request_proof() {
        let claim = challenge_get_claim();
        assert!(agent_bridge_request_proof_signing_bytes(&claim).is_ok());

        let mut nonempty = claim.clone();
        nonempty.body_hash = sha256_digest(b"not empty");
        assert!(agent_bridge_request_proof_signing_bytes(&nonempty).is_err());

        let mut wrong_method = claim.clone();
        wrong_method.http_method = "POST".to_string();
        assert!(agent_bridge_request_proof_signing_bytes(&wrong_method).is_err());

        let mut arbitrary_path = claim;
        arbitrary_path.canonical_path = "/api/agent-bridge/arbitrary".to_string();
        assert!(agent_bridge_request_proof_signing_bytes(&arbitrary_path).is_err());
    }

    #[test]
    fn challenge_query_encoding_rejects_order_duplicate_and_percent_aliases() {
        let valid = challenge_get_claim().canonical_query;
        assert!(validate_agent_bridge_canonical_query(&valid).is_ok());
        for hostile in [
            "digest=sha256%3Acc&bundle_hash=sha256%3Abb",
            "bundle_hash=sha256%3Abb&bundle_hash=sha256%3Acc",
            "bundle_hash=sha256%3abb",
            "%62undle_hash=sha256%3Abb",
            "bundle_hash=sha256%3Aab+cd",
            "?bundle_hash=sha256%3Abb",
        ] {
            assert!(
                validate_agent_bridge_canonical_query(hostile).is_err(),
                "hostile canonical query unexpectedly accepted: {hostile}"
            );
        }
    }

    #[test]
    fn practice_post_routes_are_exactly_allowlisted_in_request_proofs() {
        let mut claim = challenge_get_claim();
        claim.http_method = "POST".to_string();
        claim.canonical_query.clear();
        claim.body_hash = sha256_digest(br#"{"schema":"bounded-practice-fixture"}"#);
        for path in [
            "/api/agent-bridge/practice-tasks",
            "/api/agent-bridge/practice-claims",
            "/api/agent-bridge/practice-results",
        ] {
            claim.canonical_path = path.to_string();
            assert!(
                agent_bridge_request_proof_signing_bytes(&claim).is_ok(),
                "exact practice POST path was rejected: {path}"
            );

            let mut wrong_method = claim.clone();
            wrong_method.http_method = "GET".to_string();
            wrong_method.body_hash = sha256_digest(&[]);
            assert!(agent_bridge_request_proof_signing_bytes(&wrong_method).is_err());

            let mut aliased_path = claim.clone();
            aliased_path.canonical_path = format!("{path}/");
            assert!(agent_bridge_request_proof_signing_bytes(&aliased_path).is_err());
        }
    }
}
