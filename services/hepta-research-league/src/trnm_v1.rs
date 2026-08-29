use std::collections::{BTreeMap, BTreeSet};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub use trnm_research_protocol::{
    AuthorityRole, CanonicalCbor, ChallengeReason, ChallengeResearchClaimV1,
    ClaimResolutionDecision, ClaimResolutionV1, ClaimShareV1, ContributionRole, ContributorWorkV1,
    CreateResearchClaimV1, DeclareLicenseV1, Digest32, EvaluationCommitmentV1, ExternalKey,
    IssueWorkloadReceiptV1, LicenseScope, MatchEvidenceCommitmentV1, ObjectRefV1,
    ResearchCommandV1, ResearchObjectKind, SignedResearchCommandV1, CANONICAL_ENCODING,
};

pub const FINALITY_RECEIPT_V1: &str = "trnm_finality_receipt_v1";
pub const OBJECT_INCLUSION_PROOF_V1: &str = "trnm_object_inclusion_proof_v1";
pub const QUORUM_CERTIFICATE_V1: &str = "trnm_quorum_certificate_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrnmCommandKind {
    EvaluationCommitment,
    WorkloadReceipt,
    ResearchClaim,
    LicenseDeclaration,
    ClaimChallenge,
    ClaimResolution,
}

pub fn command_kind(command: &ResearchCommandV1) -> Option<TrnmCommandKind> {
    match command {
        ResearchCommandV1::MatchEvidenceCommitment(_) => None,
        ResearchCommandV1::EvaluationCommitment(_) => Some(TrnmCommandKind::EvaluationCommitment),
        ResearchCommandV1::IssueWorkloadReceipt(_) => Some(TrnmCommandKind::WorkloadReceipt),
        ResearchCommandV1::CreateResearchClaim(_) => Some(TrnmCommandKind::ResearchClaim),
        ResearchCommandV1::DeclareLicense(_) => Some(TrnmCommandKind::LicenseDeclaration),
        ResearchCommandV1::ChallengeResearchClaim(_) => Some(TrnmCommandKind::ClaimChallenge),
        ResearchCommandV1::ResolveResearchClaim(_) => Some(TrnmCommandKind::ClaimResolution),
    }
}

pub fn format_digest(digest: &Digest32) -> String {
    format!("sha256:{}", hex_bytes(digest))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedValidatorV1 {
    pub validator_id: String,
    pub key_id: String,
    pub public_key_base64: String,
    pub voting_power: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedValidatorSetV1 {
    pub chain_id: String,
    pub validator_set_id: String,
    pub validators: Vec<TrustedValidatorV1>,
}

impl TrustedValidatorSetV1 {
    pub fn validate(&self) -> Result<(), String> {
        require_non_empty("chain_id", &self.chain_id)?;
        require_non_empty("validator_set_id", &self.validator_set_id)?;
        if self.validators.is_empty() {
            return Err("trusted validator set must not be empty".to_string());
        }
        let mut ids = BTreeSet::new();
        let mut key_ids = BTreeSet::new();
        let mut total = 0_u64;
        for validator in &self.validators {
            require_non_empty("validator_id", &validator.validator_id)?;
            require_non_empty("key_id", &validator.key_id)?;
            if !ids.insert(validator.validator_id.as_str()) {
                return Err("trusted validator IDs must be unique".to_string());
            }
            if !key_ids.insert(validator.key_id.as_str()) {
                return Err("trusted validator key IDs must be unique".to_string());
            }
            decode_verifying_key(&validator.public_key_base64)?;
            if validator.voting_power == 0 {
                return Err("validator voting power must be greater than zero".to_string());
            }
            total = total
                .checked_add(validator.voting_power)
                .ok_or_else(|| "validator voting power overflow".to_string())?;
        }
        if total == 0 {
            return Err("trusted validator set total power must be positive".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectInclusionProofV1 {
    pub protocol: String,
    pub leaf_index: u64,
    #[serde(default)]
    pub sibling_hashes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidatorSignatureV1 {
    pub validator_id: String,
    pub key_id: String,
    pub signature_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuorumCertificateV1 {
    pub protocol: String,
    pub chain_id: String,
    pub validator_set_id: String,
    pub block_height: u64,
    pub block_hash: String,
    pub state_root: String,
    pub signed_voting_power: u64,
    pub total_voting_power: u64,
    pub signatures: Vec<ValidatorSignatureV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FinalityReceiptV1 {
    pub protocol: String,
    pub source_event_id: Uuid,
    pub command_id: Uuid,
    pub command_fingerprint: String,
    pub chain_id: String,
    pub tx_hash: String,
    pub tx_index: u64,
    pub block_height: u64,
    pub block_hash: String,
    pub state_root: String,
    pub object_ref: ObjectRefV1,
    pub inclusion_proof: ObjectInclusionProofV1,
    pub validator_set_id: String,
    pub quorum_certificate: QuorumCertificateV1,
    pub confirmations: u64,
    pub receipt_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiedFinalityV1 {
    pub valid: bool,
    pub protocol: String,
    pub command_id: Uuid,
    pub command_fingerprint: String,
    pub receipt_hash: String,
    pub validator_set_id: String,
    pub signed_voting_power: u64,
    pub total_voting_power: u64,
}

#[derive(Serialize)]
struct ObjectLeafV1<'a> {
    protocol: &'static str,
    chain_id: &'a str,
    tx_hash: &'a str,
    tx_index: u64,
    command_id: Uuid,
    command_fingerprint: &'a str,
    object_ref: &'a ObjectRefV1,
}

#[derive(Serialize)]
struct QuorumSigningPayloadV1<'a> {
    protocol: &'static str,
    chain_id: &'a str,
    validator_set_id: &'a str,
    block_height: u64,
    block_hash: &'a str,
    state_root: &'a str,
}

#[derive(Serialize)]
struct ReceiptHashPayloadV1<'a> {
    protocol: &'a str,
    source_event_id: Uuid,
    command_id: Uuid,
    command_fingerprint: &'a str,
    chain_id: &'a str,
    tx_hash: &'a str,
    tx_index: u64,
    block_height: u64,
    block_hash: &'a str,
    state_root: &'a str,
    object_ref: &'a ObjectRefV1,
    inclusion_proof: &'a ObjectInclusionProofV1,
    validator_set_id: &'a str,
    quorum_certificate: &'a QuorumCertificateV1,
    confirmations: u64,
}

pub fn object_leaf_hash(receipt: &FinalityReceiptV1) -> Result<String, String> {
    canonical_json_sha256(&ObjectLeafV1 {
        protocol: "trnm_finality_object_leaf_v1",
        chain_id: &receipt.chain_id,
        tx_hash: &receipt.tx_hash,
        tx_index: receipt.tx_index,
        command_id: receipt.command_id,
        command_fingerprint: &receipt.command_fingerprint,
        object_ref: &receipt.object_ref,
    })
}

pub fn quorum_signing_bytes(certificate: &QuorumCertificateV1) -> Result<Vec<u8>, String> {
    canonical_json_bytes(&QuorumSigningPayloadV1 {
        protocol: QUORUM_CERTIFICATE_V1,
        chain_id: &certificate.chain_id,
        validator_set_id: &certificate.validator_set_id,
        block_height: certificate.block_height,
        block_hash: &certificate.block_hash,
        state_root: &certificate.state_root,
    })
}

pub fn receipt_hash(receipt: &FinalityReceiptV1) -> Result<String, String> {
    canonical_json_sha256(&ReceiptHashPayloadV1 {
        protocol: &receipt.protocol,
        source_event_id: receipt.source_event_id,
        command_id: receipt.command_id,
        command_fingerprint: &receipt.command_fingerprint,
        chain_id: &receipt.chain_id,
        tx_hash: &receipt.tx_hash,
        tx_index: receipt.tx_index,
        block_height: receipt.block_height,
        block_hash: &receipt.block_hash,
        state_root: &receipt.state_root,
        object_ref: &receipt.object_ref,
        inclusion_proof: &receipt.inclusion_proof,
        validator_set_id: &receipt.validator_set_id,
        quorum_certificate: &receipt.quorum_certificate,
        confirmations: receipt.confirmations,
    })
}

pub fn verify_finality_receipt(
    receipt: &FinalityReceiptV1,
    expected_command_fingerprint: Option<&str>,
    trusted_sets: &[TrustedValidatorSetV1],
) -> Result<VerifiedFinalityV1, String> {
    require_protocol(&receipt.protocol, FINALITY_RECEIPT_V1)?;
    require_non_empty("chain_id", &receipt.chain_id)?;
    validate_hash("command_fingerprint", &receipt.command_fingerprint)?;
    validate_hash("tx_hash", &receipt.tx_hash)?;
    validate_hash("block_hash", &receipt.block_hash)?;
    validate_hash("state_root", &receipt.state_root)?;
    validate_hash("receipt_hash", &receipt.receipt_hash)?;
    if receipt.object_ref.key.as_bytes() == &[0; 32] {
        return Err("object_ref.key cannot be zero".to_string());
    }
    if receipt.object_ref.object_version == 0 {
        return Err("object_ref.object_version must be greater than zero".to_string());
    }
    if expected_command_fingerprint
        .is_some_and(|fingerprint| fingerprint != receipt.command_fingerprint)
    {
        return Err("receipt command fingerprint does not match queued command".to_string());
    }
    let expected_receipt_hash = receipt_hash(receipt)?;
    if expected_receipt_hash != receipt.receipt_hash {
        return Err("receipt_hash does not match canonical receipt bytes".to_string());
    }

    require_protocol(&receipt.inclusion_proof.protocol, OBJECT_INCLUSION_PROOF_V1)?;
    let mut current = decode_hash(&object_leaf_hash(receipt)?)?;
    let mut index = receipt.inclusion_proof.leaf_index;
    for sibling in &receipt.inclusion_proof.sibling_hashes {
        let sibling = decode_hash(sibling)?;
        current = if index & 1 == 0 {
            merkle_parent(&current, &sibling)
        } else {
            merkle_parent(&sibling, &current)
        };
        index >>= 1;
    }
    if index != 0 {
        return Err("inclusion proof is too short for leaf_index".to_string());
    }
    if format_digest(&current) != receipt.state_root {
        return Err("object inclusion proof does not resolve to state_root".to_string());
    }

    let certificate = &receipt.quorum_certificate;
    require_protocol(&certificate.protocol, QUORUM_CERTIFICATE_V1)?;
    if certificate.chain_id != receipt.chain_id
        || certificate.validator_set_id != receipt.validator_set_id
        || certificate.block_height != receipt.block_height
        || certificate.block_hash != receipt.block_hash
        || certificate.state_root != receipt.state_root
    {
        return Err("quorum certificate is not bound to the receipt block".to_string());
    }
    let trusted_set = trusted_sets
        .iter()
        .find(|set| {
            set.chain_id == receipt.chain_id && set.validator_set_id == receipt.validator_set_id
        })
        .ok_or_else(|| "receipt validator set is not locally trusted".to_string())?;
    trusted_set.validate()?;
    let configured_total = trusted_set
        .validators
        .iter()
        .try_fold(0_u64, |total, validator| {
            total
                .checked_add(validator.voting_power)
                .ok_or_else(|| "validator voting power overflow".to_string())
        })?;
    if certificate.total_voting_power != configured_total {
        return Err("certificate total voting power differs from trusted set".to_string());
    }
    let signing_bytes = quorum_signing_bytes(certificate)?;
    let trusted_by_id = trusted_set
        .validators
        .iter()
        .map(|validator| (validator.validator_id.as_str(), validator))
        .collect::<BTreeMap<_, _>>();
    let mut signed_ids = BTreeSet::new();
    let mut signed_power = 0_u64;
    let mut previous_validator_id: Option<&str> = None;
    for signature in &certificate.signatures {
        if previous_validator_id.is_some_and(|previous| previous >= signature.validator_id.as_str())
        {
            return Err(
                "quorum signatures must be sorted by validator_id without duplicates".to_string(),
            );
        }
        previous_validator_id = Some(&signature.validator_id);
        if !signed_ids.insert(signature.validator_id.as_str()) {
            return Err("quorum certificate contains a duplicate validator".to_string());
        }
        let validator = trusted_by_id
            .get(signature.validator_id.as_str())
            .ok_or_else(|| "quorum certificate contains an untrusted validator".to_string())?;
        if validator.key_id != signature.key_id {
            return Err("quorum signature key_id does not match trusted validator".to_string());
        }
        let verifying_key = decode_verifying_key(&validator.public_key_base64)?;
        let signature_bytes = BASE64
            .decode(&signature.signature_base64)
            .map_err(|error| format!("decode quorum signature: {error}"))?;
        let signature = Signature::from_slice(&signature_bytes)
            .map_err(|error| format!("decode Ed25519 quorum signature: {error}"))?;
        verifying_key
            .verify(&signing_bytes, &signature)
            .map_err(|_| "quorum signature verification failed".to_string())?;
        signed_power = signed_power
            .checked_add(validator.voting_power)
            .ok_or_else(|| "signed validator power overflow".to_string())?;
    }
    if certificate.signed_voting_power != signed_power {
        return Err("certificate signed voting power is incorrect".to_string());
    }
    if u128::from(signed_power) * 3 <= u128::from(configured_total) * 2 {
        return Err("quorum certificate does not exceed two-thirds voting power".to_string());
    }
    Ok(VerifiedFinalityV1 {
        valid: true,
        protocol: FINALITY_RECEIPT_V1.to_string(),
        command_id: receipt.command_id,
        command_fingerprint: receipt.command_fingerprint.clone(),
        receipt_hash: receipt.receipt_hash.clone(),
        validator_set_id: receipt.validator_set_id.clone(),
        signed_voting_power: signed_power,
        total_voting_power: configured_total,
    })
}

fn canonical_json_sha256(value: &impl Serialize) -> Result<String, String> {
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(canonical_json_bytes(value)?)
    ))
}

fn canonical_json_bytes(value: &impl Serialize) -> Result<Vec<u8>, String> {
    let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
    let mut output = Vec::new();
    write_canonical_json(&value, &mut output)?;
    Ok(output)
}

fn write_canonical_json(value: &Value, output: &mut Vec<u8>) -> Result<(), String> {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        Value::Number(value) => output.extend_from_slice(value.to_string().as_bytes()),
        Value::String(value) => output.extend_from_slice(
            serde_json::to_string(value)
                .map_err(|error| error.to_string())?
                .as_bytes(),
        ),
        Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                write_canonical_json(value, output)?;
            }
            output.push(b']');
        }
        Value::Object(values) => {
            output.push(b'{');
            let sorted = values.iter().collect::<BTreeMap<_, _>>();
            for (index, (key, value)) in sorted.into_iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                output.extend_from_slice(
                    serde_json::to_string(key)
                        .map_err(|error| error.to_string())?
                        .as_bytes(),
                );
                output.push(b':');
                write_canonical_json(value, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

fn require_protocol(actual: &str, expected: &str) -> Result<(), String> {
    if actual != expected {
        return Err(format!(
            "unsupported protocol {actual:?}; expected {expected}"
        ));
    }
    Ok(())
}

fn require_non_empty(name: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    Ok(())
}

fn validate_hash(name: &str, value: &str) -> Result<(), String> {
    decode_hash(value)
        .map(|_| ())
        .map_err(|error| format!("{name}: {error}"))
}

fn decode_hash(value: &str) -> Result<[u8; 32], String> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| "hash must use sha256:<64 lowercase hex> encoding".to_string())?;
    if hex.len() != 64
        || !hex
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err("hash must use sha256:<64 lowercase hex> encoding".to_string());
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| "hash contains invalid hex".to_string())?;
    }
    Ok(output)
}

fn merkle_parent(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"trnm_binary_merkle_node_v1\0");
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

fn decode_verifying_key(value: &str) -> Result<VerifyingKey, String> {
    let bytes = BASE64
        .decode(value)
        .map_err(|error| format!("decode Ed25519 public key: {error}"))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "Ed25519 public key must contain exactly 32 bytes".to_string())?;
    VerifyingKey::from_bytes(&bytes).map_err(|error| format!("invalid Ed25519 public key: {error}"))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
