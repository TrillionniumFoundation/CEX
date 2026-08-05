use anyhow::{anyhow, ensure, Result};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

use super::crypto::{
    decode_hash32, hash_domain, public_key_hex, put_bytes, put_str, put_u64, sign_hex, verify_hex,
    Hash32,
};

pub const SIGNED_COMMAND_SCHEMA_V1: &str = "trnm_signed_command_envelope_v1";
pub const BLOCK_HEADER_SCHEMA_V1: &str = "trnm_block_header_v1";
pub const VALIDATOR_VOTE_SCHEMA_V1: &str = "trnm_validator_precommit_v1";
pub const FINALITY_RECEIPT_SCHEMA_V1: &str = "trnm_finality_receipt_v1";

fn ensure_token(label: &str, value: &str, max: usize) -> Result<()> {
    ensure!(!value.is_empty(), "{label} must not be empty");
    ensure!(value.len() <= max, "{label} exceeds {max} bytes");
    ensure!(value == value.trim(), "{label} must be trim-canonical");
    ensure!(
        value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        }),
        "{label} contains non-canonical characters"
    );
    ensure!(
        !value.chars().any(char::is_control),
        "{label} contains control characters"
    );
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedCommandEnvelopeV1 {
    pub schema: String,
    pub chain_id: String,
    pub command_id: String,
    pub signer_id: String,
    pub signer_role: String,
    pub public_key_hex: String,
    pub nonce: u64,
    pub issued_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub payload_type: String,
    pub payload_hex: String,
    pub payload_hash_hex: String,
    pub signature_hex: String,
}

impl SignedCommandEnvelopeV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        chain_id: impl Into<String>,
        command_id: impl Into<String>,
        signer_id: impl Into<String>,
        signer_role: impl Into<String>,
        nonce: u64,
        issued_at_unix_ms: u64,
        expires_at_unix_ms: u64,
        payload_type: impl Into<String>,
        payload: &[u8],
        signing_key: &SigningKey,
    ) -> Result<Self> {
        let mut envelope = Self {
            schema: SIGNED_COMMAND_SCHEMA_V1.to_string(),
            chain_id: chain_id.into(),
            command_id: command_id.into(),
            signer_id: signer_id.into(),
            signer_role: signer_role.into(),
            public_key_hex: public_key_hex(signing_key),
            nonce,
            issued_at_unix_ms,
            expires_at_unix_ms,
            payload_type: payload_type.into(),
            payload_hex: hex::encode(payload),
            payload_hash_hex: hex::encode(hash_domain("trnm.command.payload.v1", &[payload])),
            signature_hex: String::new(),
        };
        envelope.validate_shape()?;
        envelope.signature_hex = sign_hex(signing_key, &envelope.signing_bytes()?);
        Ok(envelope)
    }

    pub fn payload_bytes(&self) -> Result<Vec<u8>> {
        let bytes = hex::decode(&self.payload_hex)
            .map_err(|_| anyhow!("payload_hex must be lowercase hex"))?;
        ensure!(
            hex::encode(&bytes) == self.payload_hex,
            "payload_hex must use canonical lowercase hex"
        );
        Ok(bytes)
    }

    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.schema == SIGNED_COMMAND_SCHEMA_V1,
            "unsupported command envelope schema"
        );
        ensure_token("chain_id", &self.chain_id, 128)?;
        ensure_token("command_id", &self.command_id, 160)?;
        ensure_token("signer_id", &self.signer_id, 256)?;
        ensure_token("signer_role", &self.signer_role, 64)?;
        ensure_token("payload_type", &self.payload_type, 128)?;
        ensure!(self.nonce > 0, "nonce must be at least 1");
        ensure!(
            self.expires_at_unix_ms > self.issued_at_unix_ms,
            "expires_at_unix_ms must be after issued_at_unix_ms"
        );
        let payload = self.payload_bytes()?;
        ensure!(
            payload.len() <= 1024 * 1024,
            "payload exceeds the 1 MiB command limit"
        );
        let expected = hash_domain("trnm.command.payload.v1", &[&payload]);
        ensure!(
            self.payload_hash_hex == hex::encode(expected),
            "payload_hash_hex does not match payload"
        );
        let _ = decode_hash32("public_key_hex", &self.public_key_hex)?;
        if !self.signature_hex.is_empty() {
            let bytes = hex::decode(&self.signature_hex)
                .map_err(|_| anyhow!("signature_hex must be lowercase hex"))?;
            ensure!(
                bytes.len() == 64 && hex::encode(bytes) == self.signature_hex,
                "signature_hex must encode 64 bytes in canonical lowercase hex"
            );
        }
        Ok(())
    }

    pub fn validate_at(&self, expected_chain_id: &str, now_unix_ms: u64) -> Result<()> {
        self.validate_shape()?;
        ensure!(self.chain_id == expected_chain_id, "chain_id mismatch");
        ensure!(
            self.issued_at_unix_ms <= now_unix_ms.saturating_add(60_000),
            "issued_at_unix_ms is too far in the future"
        );
        ensure!(
            now_unix_ms <= self.expires_at_unix_ms,
            "command envelope has expired"
        );
        verify_hex(
            &self.public_key_hex,
            &self.signing_bytes()?,
            &self.signature_hex,
        )
    }

    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate_shape()?;
        let mut out = Vec::with_capacity(512 + self.payload_hex.len());
        put_str(&mut out, SIGNED_COMMAND_SCHEMA_V1);
        put_str(&mut out, &self.chain_id);
        put_str(&mut out, &self.command_id);
        put_str(&mut out, &self.signer_id);
        put_str(&mut out, &self.signer_role);
        put_str(&mut out, &self.public_key_hex);
        put_u64(&mut out, self.nonce);
        put_u64(&mut out, self.issued_at_unix_ms);
        put_u64(&mut out, self.expires_at_unix_ms);
        put_str(&mut out, &self.payload_type);
        put_bytes(&mut out, &self.payload_bytes()?);
        put_str(&mut out, &self.payload_hash_hex);
        Ok(out)
    }

    pub fn fingerprint(&self) -> Result<Hash32> {
        Ok(hash_domain(
            "trnm.command.fingerprint.v1",
            &[&self.signing_bytes()?],
        ))
    }

    pub fn tx_hash(&self) -> Result<Hash32> {
        Ok(hash_domain(
            "trnm.transaction.v1",
            &[&self.signing_bytes()?, self.signature_hex.as_bytes()],
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidatorDescriptorV1 {
    pub validator_id: String,
    pub public_key_hex: String,
    pub vote_endpoint: String,
    pub voting_power: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidatorSetV1 {
    pub validator_set_id: String,
    pub validators: Vec<ValidatorDescriptorV1>,
    pub quorum_power: u64,
}

impl ValidatorSetV1 {
    pub fn validate(&self) -> Result<()> {
        ensure_token("validator_set_id", &self.validator_set_id, 128)?;
        ensure!(
            !self.validators.is_empty(),
            "validator set must not be empty"
        );
        let mut ids = std::collections::BTreeSet::new();
        let mut keys = std::collections::BTreeSet::new();
        let mut total = 0u64;
        for validator in &self.validators {
            ensure_token("validator_id", &validator.validator_id, 128)?;
            ensure!(
                ids.insert(validator.validator_id.clone()),
                "duplicate validator_id"
            );
            let _ = decode_hash32("validator public key", &validator.public_key_hex)?;
            ensure!(
                keys.insert(validator.public_key_hex.clone()),
                "duplicate validator public key"
            );
            ensure!(validator.voting_power > 0, "voting_power must be positive");
            total = total
                .checked_add(validator.voting_power)
                .ok_or_else(|| anyhow!("validator voting power overflow"))?;
            ensure!(
                validator.vote_endpoint.starts_with("http://127.0.0.1:")
                    || validator.vote_endpoint.starts_with("http://[::1]:"),
                "devnet validator vote_endpoint must be explicit loopback HTTP"
            );
        }
        let byzantine_quorum = total
            .checked_mul(2)
            .ok_or_else(|| anyhow!("validator voting power overflow"))?
            / 3
            + 1;
        ensure!(
            self.quorum_power >= byzantine_quorum && self.quorum_power <= total,
            "quorum_power must be at least 2/3+1 and no more than total power"
        );
        Ok(())
    }

    pub fn descriptor(&self, validator_id: &str) -> Option<&ValidatorDescriptorV1> {
        self.validators
            .iter()
            .find(|validator| validator.validator_id == validator_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockHeaderV1 {
    pub schema: String,
    pub chain_id: String,
    pub height: u64,
    pub previous_block_hash_hex: String,
    pub transaction_root_hex: String,
    pub state_root_hex: String,
    pub validator_set_id: String,
    pub timestamp_unix_ms: u64,
}

impl BlockHeaderV1 {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == BLOCK_HEADER_SCHEMA_V1,
            "unsupported block schema"
        );
        ensure_token("chain_id", &self.chain_id, 128)?;
        ensure!(self.height > 0, "height must be at least 1");
        let _ = decode_hash32("previous_block_hash_hex", &self.previous_block_hash_hex)?;
        let _ = decode_hash32("transaction_root_hex", &self.transaction_root_hex)?;
        let _ = decode_hash32("state_root_hex", &self.state_root_hex)?;
        ensure_token("validator_set_id", &self.validator_set_id, 128)?;
        Ok(())
    }

    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut out = Vec::with_capacity(256);
        put_str(&mut out, BLOCK_HEADER_SCHEMA_V1);
        put_str(&mut out, &self.chain_id);
        put_u64(&mut out, self.height);
        put_str(&mut out, &self.previous_block_hash_hex);
        put_str(&mut out, &self.transaction_root_hex);
        put_str(&mut out, &self.state_root_hex);
        put_str(&mut out, &self.validator_set_id);
        put_u64(&mut out, self.timestamp_unix_ms);
        Ok(out)
    }

    pub fn block_hash(&self) -> Result<Hash32> {
        Ok(hash_domain(
            "trnm.block.header.v1",
            &[&self.signing_bytes()?],
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidatorVoteRequestV1 {
    pub schema: String,
    pub header: BlockHeaderV1,
    pub commands: Vec<SignedCommandEnvelopeV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidatorVoteV1 {
    pub schema: String,
    pub validator_id: String,
    pub validator_set_id: String,
    pub height: u64,
    pub block_hash_hex: String,
    pub public_key_hex: String,
    pub signature_hex: String,
}

impl ValidatorVoteV1 {
    pub fn signing_bytes(
        chain_id: &str,
        validator_set_id: &str,
        height: u64,
        block_hash_hex: &str,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(192);
        put_str(&mut out, VALIDATOR_VOTE_SCHEMA_V1);
        put_str(&mut out, chain_id);
        put_str(&mut out, validator_set_id);
        put_u64(&mut out, height);
        put_str(&mut out, block_hash_hex);
        out
    }

    pub fn verify(&self, chain_id: &str, set: &ValidatorSetV1) -> Result<()> {
        ensure!(
            self.schema == VALIDATOR_VOTE_SCHEMA_V1,
            "unsupported validator vote schema"
        );
        ensure!(
            self.validator_set_id == set.validator_set_id,
            "validator_set_id mismatch"
        );
        let descriptor = set
            .descriptor(&self.validator_id)
            .ok_or_else(|| anyhow!("vote signer is not in validator set"))?;
        ensure!(
            descriptor.public_key_hex == self.public_key_hex,
            "validator public key mismatch"
        );
        let _ = decode_hash32("block_hash_hex", &self.block_hash_hex)?;
        verify_hex(
            &self.public_key_hex,
            &Self::signing_bytes(
                chain_id,
                &self.validator_set_id,
                self.height,
                &self.block_hash_hex,
            ),
            &self.signature_hex,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuorumCertificateV1 {
    pub validator_set_id: String,
    pub height: u64,
    pub block_hash_hex: String,
    pub signatures: Vec<ValidatorVoteV1>,
}

impl QuorumCertificateV1 {
    pub fn verify(&self, chain_id: &str, set: &ValidatorSetV1) -> Result<()> {
        set.validate()?;
        ensure!(
            self.validator_set_id == set.validator_set_id,
            "quorum certificate validator_set_id mismatch"
        );
        let _ = decode_hash32("block_hash_hex", &self.block_hash_hex)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut power = 0u64;
        for vote in &self.signatures {
            ensure!(vote.height == self.height, "vote height mismatch");
            ensure!(
                vote.block_hash_hex == self.block_hash_hex,
                "vote block hash mismatch"
            );
            ensure!(
                seen.insert(vote.validator_id.clone()),
                "duplicate validator vote"
            );
            vote.verify(chain_id, set)?;
            power = power
                .checked_add(
                    set.descriptor(&vote.validator_id)
                        .expect("verified descriptor")
                        .voting_power,
                )
                .ok_or_else(|| anyhow!("quorum voting power overflow"))?;
        }
        ensure!(
            power >= set.quorum_power,
            "insufficient quorum voting power: {power} < {}",
            set.quorum_power
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerkleProofStepV1 {
    pub sibling_hash_hex: String,
    pub sibling_on_left: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerkleProofV1 {
    pub tree_domain: String,
    pub leaf_hash_hex: String,
    pub leaf_index: u64,
    pub leaf_count: u64,
    pub steps: Vec<MerkleProofStepV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectRefV1 {
    pub object_key_hex: String,
    pub object_type: String,
    pub version: u64,
    pub value_hash_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalityReceiptV1 {
    pub schema: String,
    pub chain_id: String,
    pub command_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_command_fingerprint_hex: Option<String>,
    pub transaction_hash_hex: String,
    pub transaction_index: u64,
    pub block_height: u64,
    pub block_hash_hex: String,
    pub block_header: BlockHeaderV1,
    pub state_root_hex: String,
    pub transaction_root_hex: String,
    pub object_ref: Option<ObjectRefV1>,
    pub transaction_inclusion_proof: MerkleProofV1,
    pub object_inclusion_proof: Option<MerkleProofV1>,
    pub validator_set_id: String,
    pub quorum_certificate: QuorumCertificateV1,
    pub receipt_hash_hex: String,
}

impl FinalityReceiptV1 {
    pub fn unsigned_bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.schema == FINALITY_RECEIPT_SCHEMA_V1,
            "unsupported finality receipt schema"
        );
        let mut copy = self.clone();
        copy.receipt_hash_hex.clear();
        serde_json::to_vec(&copy).map_err(Into::into)
    }

    pub fn compute_receipt_hash(&self) -> Result<Hash32> {
        Ok(hash_domain(
            "trnm.finality.receipt.v1",
            &[&self.unsigned_bytes()?],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_signature_binds_every_security_field() {
        let key = SigningKey::from_bytes(&[9u8; 32]);
        let mut envelope = SignedCommandEnvelopeV1::sign(
            "trnm-devnet-1",
            "cmd-1",
            "did:key:hepta-1",
            "hepta",
            1,
            1_000,
            2_000,
            "evaluation_commitment_v1",
            b"canonical-payload",
            &key,
        )
        .unwrap();
        envelope.validate_at("trnm-devnet-1", 1_500).unwrap();

        envelope.command_id = "cmd-2".to_string();
        assert!(envelope.validate_at("trnm-devnet-1", 1_500).is_err());
    }

    #[test]
    fn altered_payload_is_rejected_before_signature_verification() {
        let key = SigningKey::from_bytes(&[10u8; 32]);
        let mut envelope = SignedCommandEnvelopeV1::sign(
            "trnm-devnet-1",
            "cmd-1",
            "did:key:nakama-1",
            "nakama",
            1,
            1_000,
            2_000,
            "match_evidence_commitment_v1",
            b"payload-a",
            &key,
        )
        .unwrap();
        envelope.payload_hex = hex::encode(b"payload-b");
        assert!(envelope.validate_at("trnm-devnet-1", 1_500).is_err());
    }

    #[test]
    fn validator_set_requires_byzantine_quorum() {
        let validators = (1..=3)
            .map(|index| ValidatorDescriptorV1 {
                validator_id: format!("validator-{index}"),
                public_key_hex: hex::encode([index as u8; 32]),
                vote_endpoint: format!("http://127.0.0.1:{}/v1/vote", 27_000 + index),
                voting_power: 1,
            })
            .collect::<Vec<_>>();
        assert!(ValidatorSetV1 {
            validator_set_id: "validators-v1".to_string(),
            validators: validators.clone(),
            quorum_power: 2,
        }
        .validate()
        .is_err());
        ValidatorSetV1 {
            validator_set_id: "validators-v1".to_string(),
            validators,
            quorum_power: 3,
        }
        .validate()
        .unwrap();
    }
}
