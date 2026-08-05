//! Frozen, language-neutral contracts used by Paper Raid.
//!
//! The `trnm_research_session_*_v1` frames are shared with the authoritative
//! Nakama research-session runtime. They deliberately do not widen or alter
//! any legacy `trnm.match.*.v1` contract.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const PAPER_RAID_PROTOCOL_V2: &str = "hepta.paper_raid.v2";
pub const PAPER_RELEASE_CANDIDATE_V2: &str = "hepta.paper_raid.release_candidate.v2";
pub const PAPER_BUNDLE_V2: &str = "hepta.paper_raid.paper_bundle.v2";
pub const PAPER_RAID_EVIDENCE_ENVELOPE_V1: &str = "hepta.paper_raid.evidence_envelope.v1";
pub const PUBLICATION_RELEASE_V1: &str = "hepta.paper_raid.publication_release.v1";
pub const AUTHORSHIP_CONSENT_V2: &str = "hepta.paper_raid.authorship_consent.v2";
pub const TEAM_MEMBER_ACCEPTANCE_V2: &str = "hepta.paper_raid.team_member_acceptance.v2";
pub const NAKAMA_COMPLETION_RECEIPT_V1: &str = "hepta.paper_raid.nakama_completion_receipt.v1";
pub const AUTHORIZATION_SET_CONSUMPTION_RECEIPT_V1: &str =
    "hepta.paper_raid.authorization_set_consumption_receipt.v1";
pub const HUMAN_KEY_REGISTRATION_V2: &str = "hepta.paper_raid.human_key_registration.v2";
pub const HUMAN_KEY_ROTATION_V2: &str = "hepta.paper_raid.human_key_rotation.v2";
pub const HUMAN_KEY_REVOCATION_V2: &str = "hepta.paper_raid.human_key_revocation.v2";
pub const CONSUMER_USER_ASSERTION_V2: &str = "hepta.consumer-edge.user-assertion.v2";
pub const AGENT_PROPOSAL_V1: &str = "hepta.paper_raid.agent_proposal.v1";
pub const HUMAN_DECISION_V1: &str = "hepta.paper_raid.human_decision.v1";
pub const HUMAN_EVIDENCE_VERIFICATION_V1: &str = "hepta.paper_raid.human_evidence_verification.v1";
pub const SECTION_REVIEW_V1: &str = "hepta.paper_raid.section_review.v1";
pub const SECTION_MERGE_V1: &str = "hepta.paper_raid.section_merge.v1";
pub const PAPER_EVALUATION_V1: &str = "hepta.paper_raid.evaluation.v1";
pub const PAPER_REVIEW_ATTESTATION_V1: &str = "hepta.paper_raid.review_attestation.v1";
pub const PAPER_REPRODUCTION_V1: &str = "hepta.paper_raid.reproduction.v1";
pub const PAPER_APPEAL_V1: &str = "hepta.paper_raid.appeal.v1";
pub const PAPER_APPEAL_RESOLUTION_V1: &str = "hepta.paper_raid.appeal_resolution.v1";

pub const RESEARCH_SESSION_AUTHORIZATION_V1: &str = "trnm.research-session.authorization.v1";
pub const RESEARCH_SESSION_ACTION_V1: &str = "trnm.research-session.action.v1";
pub const RESEARCH_SESSION_EVENT_V1: &str = "trnm.research-session.event.v1";
pub const RESEARCH_SESSION_COMPLETION_V1: &str = "trnm.research-session.completed.v1";

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
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
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

pub fn canonical_json_sha256<T: Serialize>(value: &T) -> Result<String, String> {
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
