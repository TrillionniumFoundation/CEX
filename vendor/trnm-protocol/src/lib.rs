use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use trnm_research_protocol::{
    canonical_hash, CanonicalCbor, ExternalKey, ResearchObjectKind,
    SignedPaperRaidFinalityCommandV2, SignedPaperRaidFinalityCommandV3, SignedResearchCommandV1,
};

pub const CANONICAL_TX_SCHEMA_V1: &str = "trnm_canonical_tx_v1";
pub const CANONICAL_TX_PAYLOAD_TYPE_V1: &str = "trnm.canonical.tx.v1";
pub const CANONICAL_RESEARCH_TX_SCHEMA_V1: &str = "trnm_canonical_research_tx_v1";
pub const CANONICAL_RESEARCH_TX_PAYLOAD_TYPE_V1: &str = "trnm.canonical.research.tx.v1";
pub const CANONICAL_PAPER_RAID_FINALITY_TX_SCHEMA_V2: &str =
    "trnm_canonical_paper_raid_finality_tx_v2";
pub const CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V2: &str =
    "trnm.canonical.paper-raid-finality.tx.v2";
pub const CANONICAL_PAPER_RAID_FINALITY_TX_SCHEMA_V3: &str =
    "trnm_canonical_paper_raid_finality_tx_v3";
pub const CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V3: &str =
    "trnm.canonical.paper-raid-finality.tx.v3";
pub const ACCOUNT_OBJECT_TYPE_V1: &str = "trnm.account.v1";
pub const TASK_OBJECT_TYPE_V1: &str = "trnm.poco.task.v1";
pub const FEE_POLICY_OBJECT_TYPE_V1: &str = "trnm.fee-policy.v1";
pub const MONETARY_STATE_OBJECT_TYPE_V1: &str = "trnm.monetary-state.v1";
pub const RESEARCH_AUTHORITY_SET_OBJECT_TYPE_V1: &str = "trnm.research.authority-set.v1";
pub const RESEARCH_SNAPSHOT_OBJECT_TYPE_V1: &str = "trnm.research.snapshot.v1";
pub const RESEARCH_APPLIED_COMMAND_OBJECT_TYPE_V1: &str = "trnm.research.applied-command.v1";
pub const RESEARCH_DOMAIN_OBJECT_TYPE_V1: &str = "trnm.research.domain-object.v1";
pub const PAPER_RAID_FINALITY_APPLIED_COMMAND_OBJECT_TYPE_V2: &str =
    "trnm.paper-raid.finality-applied-command.v2";
pub const PAPER_RAID_FINALITY_APPLIED_RECORD_SCHEMA_V2: &str =
    "trnm_paper_raid_finality_applied_record_v2";
pub const PAPER_RAID_FINALITY_APPLIED_COMMAND_OBJECT_TYPE_V3: &str =
    "trnm.paper-raid.finality-applied-command.v3";
pub const PAPER_RAID_FINALITY_APPLIED_RECORD_SCHEMA_V3: &str =
    "trnm_paper_raid_finality_applied_record_v3";
pub const FEE_COLLECTOR_ACCOUNT_V1: &str = "trnm:fee:collector";

const MAX_ID_BYTES: usize = 160;
const MAX_HASH_HEX_BYTES: usize = 64;
const MAX_SIGNED_RESEARCH_COMMAND_CBOR_BYTES: usize = 256 * 1024;
const MAX_CANONICAL_RESEARCH_TX_BYTES: usize = 2 * MAX_SIGNED_RESEARCH_COMMAND_CBOR_BYTES + 1024;
const MAX_SIGNED_PAPER_RAID_FINALITY_COMMAND_CBOR_BYTES: usize = 256 * 1024;
const MAX_CANONICAL_PAPER_RAID_FINALITY_TX_BYTES: usize =
    2 * MAX_SIGNED_PAPER_RAID_FINALITY_COMMAND_CBOR_BYTES + 1024;
const MAX_PAPER_RAID_FINALITY_APPLIED_RECORD_BYTES: usize = 8 * 1024;
const MAX_CHALLENGE_WINDOW_BLOCKS: u64 = 1_000_000;
const MAX_GAS_PRICE: u128 = 1_000_000_000_000;
const MAX_BASE_GAS: u64 = 10_000_000_000;
const MAX_BYTE_GAS: u64 = 1_000_000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("unsupported canonical transaction schema")]
    UnsupportedSchema,
    #[error("unsupported canonical transaction payload type")]
    UnsupportedPayloadType,
    #[error("{0} is not canonical")]
    NonCanonical(&'static str),
    #[error("{0} must be positive")]
    NonPositive(&'static str),
    #[error("invalid task deadline")]
    InvalidDeadline,
    #[error("{0} is outside the supported range")]
    OutOfRange(&'static str),
    #[error("signed research command is invalid")]
    InvalidSignedResearchCommand,
    #[error("{0} does not match the signed research command")]
    ResearchBindingMismatch(&'static str),
    #[error("signed Paper Raid finality command is invalid")]
    InvalidSignedPaperRaidFinalityCommand,
    #[error("{0} does not match the signed Paper Raid finality command")]
    PaperRaidFinalityBindingMismatch(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalTxV1 {
    pub schema: String,
    pub sender: String,
    pub nonce: u64,
    pub max_gas: u64,
    #[serde(with = "u128_decimal")]
    pub fee_limit: u128,
    pub command: CanonicalCommandV1,
}

impl CanonicalTxV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema != CANONICAL_TX_SCHEMA_V1 {
            return Err(ProtocolError::UnsupportedSchema);
        }
        validate_id("sender", &self.sender)?;
        if self.nonce == 0 {
            return Err(ProtocolError::NonPositive("nonce"));
        }
        if self.max_gas == 0 {
            return Err(ProtocolError::NonPositive("max_gas"));
        }
        self.command.validate()
    }
}

/// Versioned consensus ingress for a complete, already-signed Research
/// command.
///
/// This is deliberately a separate payload from [`CanonicalTxV1`]. The
/// signed deterministic-CBOR command remains the Research protocol source of
/// truth, while these outer fields make gas, fee, replay, and signer binding
/// explicit to consensus routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalResearchTxV1 {
    pub schema: String,
    pub payload_type: String,
    pub command_id: String,
    pub sender: String,
    pub nonce: u64,
    pub max_gas: u64,
    #[serde(with = "u128_decimal")]
    pub fee_limit: u128,
    pub signed_research_command_cbor_hex: String,
}

impl CanonicalResearchTxV1 {
    pub fn from_signed_command(
        signed: &SignedResearchCommandV1,
        max_gas: u64,
        fee_limit: u128,
    ) -> Result<Self, ProtocolError> {
        let tx = Self {
            schema: CANONICAL_RESEARCH_TX_SCHEMA_V1.to_string(),
            payload_type: CANONICAL_RESEARCH_TX_PAYLOAD_TYPE_V1.to_string(),
            command_id: signed.command_id.to_hex(),
            sender: signed.signer_did.clone(),
            nonce: signed.nonce,
            max_gas,
            fee_limit,
            signed_research_command_cbor_hex: hex::encode(signed.canonical_bytes()),
        };
        tx.validate()?;
        Ok(tx)
    }

    /// Encode the one accepted JSON representation. Field order, decimal
    /// spelling, escaping, and absence of whitespace are consensus-facing.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| ProtocolError::NonCanonical("canonical_research_tx"))
    }

    /// Decode only the exact JSON representation emitted by
    /// [`Self::canonical_bytes`]. This rejects duplicate and unknown fields
    /// via serde, then rejects alternate field order, whitespace, escaping,
    /// and number spelling by byte-for-byte re-encoding.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.is_empty() || bytes.len() > MAX_CANONICAL_RESEARCH_TX_BYTES {
            return Err(ProtocolError::OutOfRange("canonical_research_tx"));
        }
        let tx: Self = serde_json::from_slice(bytes)
            .map_err(|_| ProtocolError::NonCanonical("canonical_research_tx"))?;
        tx.validate()?;
        let canonical = serde_json::to_vec(&tx)
            .map_err(|_| ProtocolError::NonCanonical("canonical_research_tx"))?;
        if canonical != bytes {
            return Err(ProtocolError::NonCanonical("canonical_research_tx"));
        }
        Ok(tx)
    }

    pub fn signed_research_command(&self) -> Result<SignedResearchCommandV1, ProtocolError> {
        let bytes = decode_lower_hex(
            "signed_research_command_cbor_hex",
            &self.signed_research_command_cbor_hex,
            None,
            MAX_SIGNED_RESEARCH_COMMAND_CBOR_BYTES,
        )?;
        SignedResearchCommandV1::from_canonical_bytes(&bytes)
            .map_err(|_| ProtocolError::InvalidSignedResearchCommand)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema != CANONICAL_RESEARCH_TX_SCHEMA_V1 {
            return Err(ProtocolError::UnsupportedSchema);
        }
        if self.payload_type != CANONICAL_RESEARCH_TX_PAYLOAD_TYPE_V1 {
            return Err(ProtocolError::UnsupportedPayloadType);
        }
        validate_hash("command_id", &self.command_id)?;
        if self.nonce == 0 {
            return Err(ProtocolError::NonPositive("nonce"));
        }
        if self.max_gas == 0 {
            return Err(ProtocolError::NonPositive("max_gas"));
        }

        let signed = self.signed_research_command()?;
        if self.command_id != signed.command_id.to_hex() {
            return Err(ProtocolError::ResearchBindingMismatch("command_id"));
        }
        if self.sender != signed.signer_did {
            return Err(ProtocolError::ResearchBindingMismatch("sender"));
        }
        if self.nonce != signed.nonce {
            return Err(ProtocolError::ResearchBindingMismatch("nonce"));
        }
        Ok(())
    }
}

/// Consensus-facing JSON wrapper for the independent, signed Paper Raid V2
/// finality command. The V1 Research transaction remains byte-for-byte frozen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalPaperRaidFinalityTxV2 {
    pub schema: String,
    pub payload_type: String,
    pub command_id: String,
    pub sender: String,
    pub nonce: u64,
    pub max_gas: u64,
    #[serde(with = "u128_decimal")]
    pub fee_limit: u128,
    pub signed_paper_raid_finality_command_cbor_hex: String,
}

/// Canonical authenticated-state replay record written by App v6 for one
/// successfully executed Paper Raid finality command. Keeping this wire in
/// the protocol crate lets the runtime and an independent Receipt V2 verifier
/// derive and compare exactly the same bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperRaidFinalityAppliedRecordV2 {
    pub schema: String,
    pub command_id: String,
    pub command_fingerprint_hex: String,
    pub commitment_id: String,
    pub commitment_object_key_hex: String,
    pub payload_hash_hex: String,
}

impl PaperRaidFinalityAppliedRecordV2 {
    pub fn from_signed(signed: &SignedPaperRaidFinalityCommandV2) -> Result<Self, ProtocolError> {
        signed
            .validate()
            .map_err(|_| ProtocolError::InvalidSignedPaperRaidFinalityCommand)?;
        let record = Self {
            schema: PAPER_RAID_FINALITY_APPLIED_RECORD_SCHEMA_V2.to_string(),
            command_id: signed.command_id.to_hex(),
            command_fingerprint_hex: hex::encode(signed.command_fingerprint()),
            commitment_id: signed.commitment.commitment_id.to_hex(),
            commitment_object_key_hex: paper_raid_finality_commitment_key(
                signed.commitment.commitment_id,
            )?,
            payload_hash_hex: hex::encode(signed.payload_hash()),
        };
        record.validate()?;
        Ok(record)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|_| ProtocolError::NonCanonical("paper_raid_finality_applied_record"))?;
        if bytes.is_empty() || bytes.len() > MAX_PAPER_RAID_FINALITY_APPLIED_RECORD_BYTES {
            return Err(ProtocolError::OutOfRange(
                "paper_raid_finality_applied_record",
            ));
        }
        Ok(bytes)
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.is_empty() || bytes.len() > MAX_PAPER_RAID_FINALITY_APPLIED_RECORD_BYTES {
            return Err(ProtocolError::OutOfRange(
                "paper_raid_finality_applied_record",
            ));
        }
        let record: Self = serde_json::from_slice(bytes)
            .map_err(|_| ProtocolError::NonCanonical("paper_raid_finality_applied_record"))?;
        record.validate()?;
        if record.canonical_bytes()? != bytes {
            return Err(ProtocolError::NonCanonical(
                "paper_raid_finality_applied_record",
            ));
        }
        Ok(record)
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema != PAPER_RAID_FINALITY_APPLIED_RECORD_SCHEMA_V2 {
            return Err(ProtocolError::UnsupportedSchema);
        }
        for (label, value) in [
            ("paper_raid_finality_command_id", self.command_id.as_str()),
            (
                "paper_raid_finality_command_fingerprint",
                self.command_fingerprint_hex.as_str(),
            ),
            (
                "paper_raid_finality_commitment_id",
                self.commitment_id.as_str(),
            ),
            (
                "paper_raid_finality_commitment_object_key",
                self.commitment_object_key_hex.as_str(),
            ),
            (
                "paper_raid_finality_payload_hash",
                self.payload_hash_hex.as_str(),
            ),
        ] {
            decode_lower_hex(label, value, Some(32), 32)?;
        }
        Ok(())
    }
}

impl CanonicalPaperRaidFinalityTxV2 {
    pub fn from_signed_command(
        signed: &SignedPaperRaidFinalityCommandV2,
        max_gas: u64,
        fee_limit: u128,
    ) -> Result<Self, ProtocolError> {
        let tx = Self {
            schema: CANONICAL_PAPER_RAID_FINALITY_TX_SCHEMA_V2.to_string(),
            payload_type: CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V2.to_string(),
            command_id: signed.command_id.to_hex(),
            sender: signed.signer_did.clone(),
            nonce: signed.nonce,
            max_gas,
            fee_limit,
            signed_paper_raid_finality_command_cbor_hex: hex::encode(signed.canonical_bytes()),
        };
        tx.validate()?;
        Ok(tx)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|_| ProtocolError::NonCanonical("canonical_paper_raid_finality_tx"))
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.is_empty() || bytes.len() > MAX_CANONICAL_PAPER_RAID_FINALITY_TX_BYTES {
            return Err(ProtocolError::OutOfRange(
                "canonical_paper_raid_finality_tx",
            ));
        }
        let tx: Self = serde_json::from_slice(bytes)
            .map_err(|_| ProtocolError::NonCanonical("canonical_paper_raid_finality_tx"))?;
        tx.validate()?;
        let canonical = serde_json::to_vec(&tx)
            .map_err(|_| ProtocolError::NonCanonical("canonical_paper_raid_finality_tx"))?;
        if canonical != bytes {
            return Err(ProtocolError::NonCanonical(
                "canonical_paper_raid_finality_tx",
            ));
        }
        Ok(tx)
    }

    pub fn signed_paper_raid_finality_command(
        &self,
    ) -> Result<SignedPaperRaidFinalityCommandV2, ProtocolError> {
        let bytes = decode_lower_hex(
            "signed_paper_raid_finality_command_cbor_hex",
            &self.signed_paper_raid_finality_command_cbor_hex,
            None,
            MAX_SIGNED_PAPER_RAID_FINALITY_COMMAND_CBOR_BYTES,
        )?;
        SignedPaperRaidFinalityCommandV2::from_canonical_bytes(&bytes)
            .map_err(|_| ProtocolError::InvalidSignedPaperRaidFinalityCommand)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema != CANONICAL_PAPER_RAID_FINALITY_TX_SCHEMA_V2 {
            return Err(ProtocolError::UnsupportedSchema);
        }
        if self.payload_type != CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V2 {
            return Err(ProtocolError::UnsupportedPayloadType);
        }
        validate_hash("command_id", &self.command_id)?;
        if self.nonce == 0 {
            return Err(ProtocolError::NonPositive("nonce"));
        }
        if self.max_gas == 0 {
            return Err(ProtocolError::NonPositive("max_gas"));
        }
        let signed = self.signed_paper_raid_finality_command()?;
        if self.command_id != signed.command_id.to_hex() {
            return Err(ProtocolError::PaperRaidFinalityBindingMismatch(
                "command_id",
            ));
        }
        if self.sender != signed.signer_did {
            return Err(ProtocolError::PaperRaidFinalityBindingMismatch("sender"));
        }
        if self.nonce != signed.nonce {
            return Err(ProtocolError::PaperRaidFinalityBindingMismatch("nonce"));
        }
        Ok(())
    }
}

/// Consensus-facing JSON wrapper for the signed Paper Raid V3 finality
/// command. The schema and payload type are independent from frozen V2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalPaperRaidFinalityTxV3 {
    pub schema: String,
    pub payload_type: String,
    pub command_id: String,
    pub sender: String,
    pub nonce: u64,
    pub max_gas: u64,
    #[serde(with = "u128_decimal")]
    pub fee_limit: u128,
    pub signed_paper_raid_finality_command_cbor_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperRaidFinalityAppliedRecordV3 {
    pub schema: String,
    pub command_id: String,
    pub command_fingerprint_hex: String,
    pub commitment_id: String,
    pub commitment_object_key_hex: String,
    pub payload_hash_hex: String,
}

impl PaperRaidFinalityAppliedRecordV3 {
    pub fn from_signed(signed: &SignedPaperRaidFinalityCommandV3) -> Result<Self, ProtocolError> {
        signed
            .validate()
            .map_err(|_| ProtocolError::InvalidSignedPaperRaidFinalityCommand)?;
        let record = Self {
            schema: PAPER_RAID_FINALITY_APPLIED_RECORD_SCHEMA_V3.to_string(),
            command_id: signed.command_id.to_hex(),
            command_fingerprint_hex: hex::encode(signed.command_fingerprint()),
            commitment_id: signed.commitment.commitment_id.to_hex(),
            commitment_object_key_hex: paper_raid_finality_commitment_key_v3(
                signed.commitment.commitment_id,
            )?,
            payload_hash_hex: hex::encode(signed.payload_hash()),
        };
        record.validate()?;
        Ok(record)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|_| ProtocolError::NonCanonical("paper_raid_finality_applied_record_v3"))?;
        if bytes.is_empty() || bytes.len() > MAX_PAPER_RAID_FINALITY_APPLIED_RECORD_BYTES {
            return Err(ProtocolError::OutOfRange(
                "paper_raid_finality_applied_record_v3",
            ));
        }
        Ok(bytes)
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.is_empty() || bytes.len() > MAX_PAPER_RAID_FINALITY_APPLIED_RECORD_BYTES {
            return Err(ProtocolError::OutOfRange(
                "paper_raid_finality_applied_record_v3",
            ));
        }
        let record: Self = serde_json::from_slice(bytes)
            .map_err(|_| ProtocolError::NonCanonical("paper_raid_finality_applied_record_v3"))?;
        record.validate()?;
        if record.canonical_bytes()? != bytes {
            return Err(ProtocolError::NonCanonical(
                "paper_raid_finality_applied_record_v3",
            ));
        }
        Ok(record)
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema != PAPER_RAID_FINALITY_APPLIED_RECORD_SCHEMA_V3 {
            return Err(ProtocolError::UnsupportedSchema);
        }
        for (label, value) in [
            (
                "paper_raid_finality_v3_command_id",
                self.command_id.as_str(),
            ),
            (
                "paper_raid_finality_v3_command_fingerprint",
                self.command_fingerprint_hex.as_str(),
            ),
            (
                "paper_raid_finality_v3_commitment_id",
                self.commitment_id.as_str(),
            ),
            (
                "paper_raid_finality_v3_commitment_object_key",
                self.commitment_object_key_hex.as_str(),
            ),
            (
                "paper_raid_finality_v3_payload_hash",
                self.payload_hash_hex.as_str(),
            ),
        ] {
            decode_lower_hex(label, value, Some(32), 32)?;
        }
        Ok(())
    }
}

impl CanonicalPaperRaidFinalityTxV3 {
    pub fn from_signed_command(
        signed: &SignedPaperRaidFinalityCommandV3,
        max_gas: u64,
        fee_limit: u128,
    ) -> Result<Self, ProtocolError> {
        let tx = Self {
            schema: CANONICAL_PAPER_RAID_FINALITY_TX_SCHEMA_V3.to_string(),
            payload_type: CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V3.to_string(),
            command_id: signed.command_id.to_hex(),
            sender: signed.signer_did.clone(),
            nonce: signed.nonce,
            max_gas,
            fee_limit,
            signed_paper_raid_finality_command_cbor_hex: hex::encode(signed.canonical_bytes()),
        };
        tx.validate()?;
        Ok(tx)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|_| ProtocolError::NonCanonical("canonical_paper_raid_finality_tx_v3"))
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.is_empty() || bytes.len() > MAX_CANONICAL_PAPER_RAID_FINALITY_TX_BYTES {
            return Err(ProtocolError::OutOfRange(
                "canonical_paper_raid_finality_tx_v3",
            ));
        }
        let tx: Self = serde_json::from_slice(bytes)
            .map_err(|_| ProtocolError::NonCanonical("canonical_paper_raid_finality_tx_v3"))?;
        tx.validate()?;
        let canonical = serde_json::to_vec(&tx)
            .map_err(|_| ProtocolError::NonCanonical("canonical_paper_raid_finality_tx_v3"))?;
        if canonical != bytes {
            return Err(ProtocolError::NonCanonical(
                "canonical_paper_raid_finality_tx_v3",
            ));
        }
        Ok(tx)
    }

    pub fn signed_paper_raid_finality_command(
        &self,
    ) -> Result<SignedPaperRaidFinalityCommandV3, ProtocolError> {
        let bytes = decode_lower_hex(
            "signed_paper_raid_finality_command_cbor_hex",
            &self.signed_paper_raid_finality_command_cbor_hex,
            None,
            MAX_SIGNED_PAPER_RAID_FINALITY_COMMAND_CBOR_BYTES,
        )?;
        SignedPaperRaidFinalityCommandV3::from_canonical_bytes(&bytes)
            .map_err(|_| ProtocolError::InvalidSignedPaperRaidFinalityCommand)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema != CANONICAL_PAPER_RAID_FINALITY_TX_SCHEMA_V3 {
            return Err(ProtocolError::UnsupportedSchema);
        }
        if self.payload_type != CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V3 {
            return Err(ProtocolError::UnsupportedPayloadType);
        }
        validate_hash("command_id", &self.command_id)?;
        if self.nonce == 0 {
            return Err(ProtocolError::NonPositive("nonce"));
        }
        if self.max_gas == 0 {
            return Err(ProtocolError::NonPositive("max_gas"));
        }
        let signed = self.signed_paper_raid_finality_command()?;
        if self.command_id != signed.command_id.to_hex() {
            return Err(ProtocolError::PaperRaidFinalityBindingMismatch(
                "command_id",
            ));
        }
        if self.sender != signed.signer_did {
            return Err(ProtocolError::PaperRaidFinalityBindingMismatch("sender"));
        }
        if self.nonce != signed.nonce {
            return Err(ProtocolError::PaperRaidFinalityBindingMismatch("nonce"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CanonicalCommandV1 {
    CreditAccount {
        account: String,
        #[serde(with = "u128_decimal")]
        amount: u128,
    },
    Transfer {
        to: String,
        #[serde(with = "u128_decimal")]
        amount: u128,
    },
    CreateTask {
        task_id: String,
        #[serde(with = "u128_decimal")]
        reward: u128,
        #[serde(with = "u128_decimal")]
        worker_stake: u128,
        result_deadline_height: u64,
        challenge_window_blocks: u64,
    },
    AssignTask {
        task_id: String,
        worker: String,
    },
    CommitResult {
        task_id: String,
        commitment_hex: String,
    },
    RevealResult {
        task_id: String,
        result_hash_hex: String,
        reveal_salt_hex: String,
    },
    RecordConsumption {
        task_id: String,
        units: u64,
        #[serde(with = "u128_decimal")]
        payment: u128,
        receipt_hash_hex: String,
    },
    OpenChallenge {
        task_id: String,
        #[serde(with = "u128_decimal")]
        bond: u128,
        evidence_hash_hex: String,
    },
    ResolveChallenge {
        task_id: String,
        accept_challenge: bool,
    },
    SettleTask {
        task_id: String,
    },
    ExpireTask {
        task_id: String,
    },
    SetFeePolicy {
        #[serde(with = "u128_decimal")]
        gas_price: u128,
        base_gas: u64,
        byte_gas: u64,
    },
    DistributeFees {
        to: String,
        #[serde(with = "u128_decimal")]
        amount: u128,
    },
}

impl CanonicalCommandV1 {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::CreditAccount { account, amount } => {
                validate_id("account", account)?;
                positive("amount", *amount)
            }
            Self::Transfer { to, amount } => {
                validate_id("to", to)?;
                positive("amount", *amount)
            }
            Self::CreateTask {
                task_id,
                reward,
                worker_stake,
                result_deadline_height,
                challenge_window_blocks,
            } => {
                validate_id("task_id", task_id)?;
                positive("reward", *reward)?;
                positive("worker_stake", *worker_stake)?;
                if *result_deadline_height == 0 {
                    return Err(ProtocolError::InvalidDeadline);
                }
                if *challenge_window_blocks == 0
                    || *challenge_window_blocks > MAX_CHALLENGE_WINDOW_BLOCKS
                {
                    return Err(ProtocolError::OutOfRange("challenge_window_blocks"));
                }
                Ok(())
            }
            Self::AssignTask { task_id, worker } => {
                validate_id("task_id", task_id)?;
                validate_id("worker", worker)
            }
            Self::CommitResult {
                task_id,
                commitment_hex,
            } => {
                validate_id("task_id", task_id)?;
                validate_hash("commitment_hex", commitment_hex)
            }
            Self::RevealResult {
                task_id,
                result_hash_hex,
                reveal_salt_hex,
            } => {
                validate_id("task_id", task_id)?;
                validate_hash("result_hash_hex", result_hash_hex)?;
                validate_hash("reveal_salt_hex", reveal_salt_hex)
            }
            Self::RecordConsumption {
                task_id,
                units,
                payment,
                receipt_hash_hex,
            } => {
                validate_id("task_id", task_id)?;
                if *units == 0 {
                    return Err(ProtocolError::NonPositive("units"));
                }
                positive("payment", *payment)?;
                validate_hash("receipt_hash_hex", receipt_hash_hex)
            }
            Self::OpenChallenge {
                task_id,
                bond,
                evidence_hash_hex,
            } => {
                validate_id("task_id", task_id)?;
                positive("bond", *bond)?;
                validate_hash("evidence_hash_hex", evidence_hash_hex)
            }
            Self::ResolveChallenge { task_id, .. }
            | Self::SettleTask { task_id }
            | Self::ExpireTask { task_id } => validate_id("task_id", task_id),
            Self::SetFeePolicy {
                gas_price,
                base_gas,
                byte_gas,
            } => {
                positive("gas_price", *gas_price)?;
                if *base_gas == 0 {
                    return Err(ProtocolError::NonPositive("base_gas"));
                }
                if *byte_gas == 0 {
                    return Err(ProtocolError::NonPositive("byte_gas"));
                }
                if *gas_price > MAX_GAS_PRICE {
                    return Err(ProtocolError::OutOfRange("gas_price"));
                }
                if *base_gas > MAX_BASE_GAS {
                    return Err(ProtocolError::OutOfRange("base_gas"));
                }
                if *byte_gas > MAX_BYTE_GAS {
                    return Err(ProtocolError::OutOfRange("byte_gas"));
                }
                Ok(())
            }
            Self::DistributeFees { to, amount } => {
                validate_id("to", to)?;
                positive("amount", *amount)
            }
        }
    }

    pub fn operation_gas(&self) -> u64 {
        match self {
            Self::CreditAccount { .. } | Self::Transfer { .. } | Self::DistributeFees { .. } => 500,
            Self::CreateTask { .. } => 2_000,
            Self::AssignTask { .. } => 1_000,
            Self::CommitResult { .. } | Self::RevealResult { .. } => 1_200,
            Self::RecordConsumption { .. } => 1_800,
            Self::OpenChallenge { .. } | Self::ResolveChallenge { .. } => 2_000,
            Self::SettleTask { .. } | Self::ExpireTask { .. } => 1_500,
            Self::SetFeePolicy { .. } => 500,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountV1 {
    pub account: String,
    #[serde(with = "u128_decimal")]
    pub balance: u128,
    pub nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatusV1 {
    Open,
    Assigned,
    Committed,
    Revealed,
    Consumed,
    Challenged,
    Settled,
    ResolvedForWorker,
    ResolvedForChallenger,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskV1 {
    pub task_id: String,
    pub client: String,
    pub worker: Option<String>,
    #[serde(with = "u128_decimal")]
    pub reward: u128,
    #[serde(with = "u128_decimal")]
    pub worker_stake: u128,
    pub result_deadline_height: u64,
    pub challenge_window_blocks: u64,
    pub status: TaskStatusV1,
    pub commitment_hex: Option<String>,
    pub result_hash_hex: Option<String>,
    pub reveal_salt_hex: Option<String>,
    pub challenge_deadline_height: Option<u64>,
    pub consumer: Option<String>,
    pub consumed_units: u64,
    #[serde(with = "u128_decimal")]
    pub consumption_payment: u128,
    pub receipt_hash_hex: Option<String>,
    pub challenger: Option<String>,
    #[serde(with = "u128_decimal")]
    pub challenge_bond: u128,
    pub evidence_hash_hex: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeePolicyV1 {
    #[serde(with = "u128_decimal")]
    pub gas_price: u128,
    pub base_gas: u64,
    pub byte_gas: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct MonetaryStateV1 {
    #[serde(with = "u128_decimal")]
    pub total_issued: u128,
}

impl Default for FeePolicyV1 {
    fn default() -> Self {
        Self {
            gas_price: 1,
            base_gas: 1_000,
            byte_gas: 2,
        }
    }
}

pub fn account_key(account: &str) -> String {
    object_key("trnm.account.object-key.v1", account)
}

pub fn task_key(task_id: &str) -> String {
    object_key("trnm.poco.task.object-key.v1", task_id)
}

pub fn fee_policy_key() -> String {
    object_key("trnm.fee-policy.object-key.v1", "singleton")
}

pub fn monetary_state_key() -> String {
    object_key("trnm.monetary-state.object-key.v1", "singleton")
}

/// Stable singleton key for the immutable, genesis-derived Research trust set.
pub fn research_authority_set_key() -> String {
    object_key("trnm.research.authority-set.object-key.v1", "singleton")
}

/// Legacy snapshot key retained only for explicit migration rejection. New
/// app-version-6 genesis never writes or updates this aggregate object.
pub fn research_snapshot_key() -> String {
    object_key("trnm.research.snapshot.object-key.v1", "singleton")
}

/// Stable immutable key for a command replay/fingerprint record.
pub fn research_applied_command_key(command_id: ExternalKey) -> Result<String, ProtocolError> {
    validate_external_key("command_id", command_id)?;
    Ok(object_key_components(
        "trnm.research.applied-command.object-key.v1",
        &[command_id.as_bytes()],
    ))
}

/// Stable authenticated-state key for one immutable Paper Raid V2 finality
/// commitment. This namespace is independent from every Research V1 object.
pub fn paper_raid_finality_commitment_key(
    commitment_id: ExternalKey,
) -> Result<String, ProtocolError> {
    validate_external_key("paper_raid_finality_commitment_id", commitment_id)?;
    Ok(hex::encode(canonical_hash(
        "trnm.paper-raid.finality-commitment.object-key.v2",
        commitment_id.as_bytes(),
    )))
}

/// Stable authenticated-state key for the exact-replay record of one Paper
/// Raid V2 finality command.
pub fn paper_raid_finality_applied_command_key(
    command_id: ExternalKey,
) -> Result<String, ProtocolError> {
    validate_external_key("paper_raid_finality_command_id", command_id)?;
    Ok(hex::encode(canonical_hash(
        "trnm.paper-raid.finality-applied-command.object-key.v2",
        command_id.as_bytes(),
    )))
}

/// Stable authenticated-state key for one immutable Paper Raid V3 finality
/// commitment. It cannot alias the frozen V2 commitment namespace.
pub fn paper_raid_finality_commitment_key_v3(
    commitment_id: ExternalKey,
) -> Result<String, ProtocolError> {
    validate_external_key("paper_raid_finality_v3_commitment_id", commitment_id)?;
    Ok(hex::encode(canonical_hash(
        "trnm.paper-raid.finality-commitment.object-key.v3",
        commitment_id.as_bytes(),
    )))
}

/// Stable authenticated-state key for the exact-replay record of one Paper
/// Raid V3 finality command.
pub fn paper_raid_finality_applied_command_key_v3(
    command_id: ExternalKey,
) -> Result<String, ProtocolError> {
    validate_external_key("paper_raid_finality_v3_command_id", command_id)?;
    Ok(hex::encode(canonical_hash(
        "trnm.paper-raid.finality-applied-command.object-key.v3",
        command_id.as_bytes(),
    )))
}

/// Stable key for a Research domain object. Object version is deliberately
/// excluded: successive versions of one logical object update the same
/// authenticated-state key.
pub fn research_domain_object_key(
    kind: ResearchObjectKind,
    object_key: ExternalKey,
) -> Result<String, ProtocolError> {
    validate_external_key("research_object_key", object_key)?;
    let kind_discriminant = [kind as u8];
    Ok(object_key_components(
        "trnm.research.domain-object.object-key.v1",
        &[&kind_discriminant, object_key.as_bytes()],
    ))
}

pub fn result_commitment_hex(
    task_id: &str,
    worker: &str,
    result_hash_hex: &str,
    reveal_salt_hex: &str,
) -> Result<String, ProtocolError> {
    validate_id("task_id", task_id)?;
    validate_id("worker", worker)?;
    validate_hash("result_hash_hex", result_hash_hex)?;
    validate_hash("reveal_salt_hex", reveal_salt_hex)?;
    let result_hash =
        hex::decode(result_hash_hex).map_err(|_| ProtocolError::NonCanonical("result_hash_hex"))?;
    let reveal_salt =
        hex::decode(reveal_salt_hex).map_err(|_| ProtocolError::NonCanonical("reveal_salt_hex"))?;
    let mut hasher = Sha256::new();
    for field in [
        b"trnm.result-commitment.v1".as_slice(),
        task_id.as_bytes(),
        worker.as_bytes(),
        result_hash.as_slice(),
        reveal_salt.as_slice(),
    ] {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn object_key(domain: &str, id: &str) -> String {
    object_key_components(domain, &[id.as_bytes()])
}

fn object_key_components(domain: &str, components: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain.as_bytes());
    for component in components {
        hasher.update((component.len() as u64).to_be_bytes());
        hasher.update(component);
    }
    hex::encode(hasher.finalize())
}

fn positive(label: &'static str, value: u128) -> Result<(), ProtocolError> {
    if value == 0 {
        return Err(ProtocolError::NonPositive(label));
    }
    Ok(())
}

fn validate_id(label: &'static str, value: &str) -> Result<(), ProtocolError> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || value != value.trim()
        || !value.is_ascii()
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/' | b'@')
        })
    {
        return Err(ProtocolError::NonCanonical(label));
    }
    Ok(())
}

fn validate_hash(label: &'static str, value: &str) -> Result<(), ProtocolError> {
    if value.len() != MAX_HASH_HEX_BYTES {
        return Err(ProtocolError::NonCanonical(label));
    }
    decode_lower_hex(label, value, Some(32), 32)?;
    Ok(())
}

fn validate_external_key(label: &'static str, value: ExternalKey) -> Result<(), ProtocolError> {
    if value.as_bytes() == &[0; 32] {
        return Err(ProtocolError::NonCanonical(label));
    }
    Ok(())
}

fn decode_lower_hex(
    label: &'static str,
    value: &str,
    exact_bytes: Option<usize>,
    max_bytes: usize,
) -> Result<Vec<u8>, ProtocolError> {
    if value.is_empty()
        || !value.len().is_multiple_of(2)
        || value.len() > max_bytes.saturating_mul(2)
    {
        return Err(ProtocolError::NonCanonical(label));
    }
    let bytes = hex::decode(value).map_err(|_| ProtocolError::NonCanonical(label))?;
    if exact_bytes.is_some_and(|expected| bytes.len() != expected)
        || bytes.len() > max_bytes
        || hex::encode(&bytes) != value
    {
        return Err(ProtocolError::NonCanonical(label));
    }
    Ok(bytes)
}

mod u128_decimal {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &u128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u128, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        if raw.is_empty()
            || (raw.len() > 1 && raw.starts_with('0'))
            || !raw.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(D::Error::custom("u128 decimal is not canonical"));
        }
        raw.parse().map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use trnm_research_protocol::{
        AuthorityRole, MatchEvidenceCommitmentV1, ObjectRefV1, PaperRaidAppealStatusV2,
        PaperRaidAppealStatusV3, PaperRaidFinalityCommitmentV2, PaperRaidFinalityCommitmentV3,
        ResearchCommandV1, ResearchObjectKind, SignedPaperRaidFinalityCommandV2,
        SignedPaperRaidFinalityCommandV3,
    };

    fn external_key(namespace: &str, id: &str) -> ExternalKey {
        ExternalKey::from_external_id(namespace, id).unwrap()
    }

    fn signed_research_command() -> SignedResearchCommandV1 {
        let command = ResearchCommandV1::MatchEvidenceCommitment(MatchEvidenceCommitmentV1 {
            commitment_id: external_key("nakama.commitment", "commitment-001"),
            match_id: external_key("nakama.match", "match-001"),
            challenge_id: external_key("hepta.challenge", "challenge-001"),
            event_root: [0x10; 32],
            roster_root: [0x11; 32],
            ruleset_hash: [0x12; 32],
            dataset_hash: [0x13; 32],
            archive_hash: [0x14; 32],
            event_count: 42,
            completed_at_unix_s: 1_753_449_600,
        });
        SignedResearchCommandV1::sign(
            "trnm-devnet-v1".into(),
            external_key("trnm.command", "command-001"),
            "did:trnm:nakama-authority".into(),
            AuthorityRole::NakamaAuthority,
            7,
            command,
            &SigningKey::from_bytes(&[0x11; 32]),
        )
        .unwrap()
    }

    fn canonical_research_tx() -> CanonicalResearchTxV1 {
        CanonicalResearchTxV1::from_signed_command(
            &signed_research_command(),
            250_000,
            12_345_678_901_234_567_890,
        )
        .unwrap()
    }

    fn signed_paper_raid_finality_command() -> SignedPaperRaidFinalityCommandV3 {
        SignedPaperRaidFinalityCommandV3::sign(
            "trnm-devnet-v1".into(),
            external_key("trnm.paper-raid.command", "command-001"),
            "did:trnm:hepta-authority".into(),
            9,
            PaperRaidFinalityCommitmentV3 {
                commitment_id: external_key("hepta.paper-raid.finality", "finality-001"),
                paper_project_id: external_key("hepta.paper", "paper-001"),
                submission_id: external_key("hepta.submission", "submission-001"),
                match_evidence_ref: ObjectRefV1::new(
                    ResearchObjectKind::MatchEvidence,
                    external_key("nakama.commitment", "commitment-001"),
                    1,
                ),
                release_candidate_hash: [0x21; 32],
                paper_bundle_hash: [0x22; 32],
                submission_commitment_hash: [0x23; 32],
                author_consent_set_hash: [0x24; 32],
                tolerance_policy_hash: [0x25; 32],
                evaluation_id: external_key("hepta.evaluation", "evaluation-001"),
                evaluation_hash: [0x26; 32],
                evaluation_score_bps: 8_750,
                evaluation_accepted: true,
                evaluation_completed_at_unix_s: 100,
                latest_reproduction_id: external_key("hepta.reproduction", "reproduction-001"),
                latest_reproduction_hash: [0x27; 32],
                latest_reproduction_accepted: true,
                latest_reproduction_completed_at_unix_s: 110,
                evaluation_supersedes: None,
                evaluation_superseded_by: None,
                reproduction_superseded_by: None,
                appeal_status: PaperRaidAppealStatusV3::ClosedNoAppeal,
                appeal_id: None,
                appealed_evaluation_id: None,
                appeal_resolution_hash: None,
                appeal_window_closes_at_unix_s: 120,
                settlement_policy_hash: [0x28; 32],
                scientific_finality: true,
                score_eligible: false,
                ranking_eligible: false,
                reward_eligible: false,
                economic_eligible: false,
                finalized_at_unix_s: 121,
            },
            &SigningKey::from_bytes(&[0x22; 32]),
        )
        .unwrap()
    }

    fn canonical_paper_raid_finality_tx() -> CanonicalPaperRaidFinalityTxV3 {
        CanonicalPaperRaidFinalityTxV3::from_signed_command(
            &signed_paper_raid_finality_command(),
            300_000,
            98_765_432_100,
        )
        .unwrap()
    }

    fn signed_paper_raid_finality_command_v2() -> SignedPaperRaidFinalityCommandV2 {
        let v3 = signed_paper_raid_finality_command();
        let commitment = PaperRaidFinalityCommitmentV2 {
            commitment_id: v3.commitment.commitment_id,
            paper_project_id: v3.commitment.paper_project_id,
            submission_id: v3.commitment.submission_id,
            match_evidence_ref: v3.commitment.match_evidence_ref,
            release_candidate_hash: v3.commitment.release_candidate_hash,
            paper_bundle_hash: v3.commitment.paper_bundle_hash,
            submission_commitment_hash: v3.commitment.submission_commitment_hash,
            author_consent_set_hash: v3.commitment.author_consent_set_hash,
            tolerance_policy_hash: v3.commitment.tolerance_policy_hash,
            evaluation_id: v3.commitment.evaluation_id,
            evaluation_hash: v3.commitment.evaluation_hash,
            evaluation_score_bps: v3.commitment.evaluation_score_bps,
            evaluation_accepted: v3.commitment.evaluation_accepted,
            evaluation_completed_at_unix_s: v3.commitment.evaluation_completed_at_unix_s,
            latest_reproduction_id: v3.commitment.latest_reproduction_id,
            latest_reproduction_hash: v3.commitment.latest_reproduction_hash,
            latest_reproduction_accepted: v3.commitment.latest_reproduction_accepted,
            latest_reproduction_completed_at_unix_s: v3
                .commitment
                .latest_reproduction_completed_at_unix_s,
            evaluation_superseded_by: v3.commitment.evaluation_superseded_by,
            reproduction_superseded_by: v3.commitment.reproduction_superseded_by,
            appeal_status: PaperRaidAppealStatusV2::ClosedNoAppeal,
            appeal_id: None,
            appeal_resolution_hash: None,
            appeal_window_closes_at_unix_s: v3.commitment.appeal_window_closes_at_unix_s,
            settlement_policy_hash: v3.commitment.settlement_policy_hash,
            scientific_finality: v3.commitment.scientific_finality,
            score_eligible: v3.commitment.score_eligible,
            ranking_eligible: v3.commitment.ranking_eligible,
            reward_eligible: v3.commitment.reward_eligible,
            economic_eligible: v3.commitment.economic_eligible,
            finalized_at_unix_s: v3.commitment.finalized_at_unix_s,
        };
        SignedPaperRaidFinalityCommandV2::sign(
            v3.chain_id,
            v3.command_id,
            v3.signer_did,
            v3.nonce,
            commitment,
            &SigningKey::from_bytes(&[0x22; 32]),
        )
        .unwrap()
    }

    fn canonical_paper_raid_finality_tx_v2() -> CanonicalPaperRaidFinalityTxV2 {
        CanonicalPaperRaidFinalityTxV2::from_signed_command(
            &signed_paper_raid_finality_command_v2(),
            300_000,
            98_765_432_100,
        )
        .unwrap()
    }

    #[test]
    fn object_namespaces_do_not_collide() {
        assert_ne!(account_key("same"), task_key("same"));
        assert_ne!(account_key("singleton"), fee_policy_key());
        assert_ne!(fee_policy_key(), monetary_state_key());
        assert_eq!(
            account_key("alice"),
            "77ba6285ba2db687db3cc227c2d765755d776a8b855e9838a74c87d90ce966bb"
        );
    }

    #[test]
    fn research_object_keys_have_stable_golden_values_and_namespaces() {
        let command_id = ExternalKey::from_bytes([0x44; 32]);
        let object_id = ExternalKey::from_bytes([0x55; 32]);
        let paper_commitment_id = ExternalKey::from_bytes([0x66; 32]);
        assert_eq!(
            research_authority_set_key(),
            "8cad6d2c543cfa4baa742da6cae9c50a02ca351011a7ef740fbc75f259db9451"
        );
        assert_eq!(
            research_snapshot_key(),
            "3cf001f17cda52bcd21973b83eebbb78ee4a76b86f35f6f8687ccd47ecfa854b"
        );
        assert_ne!(research_authority_set_key(), research_snapshot_key());
        assert_eq!(
            research_applied_command_key(command_id).unwrap(),
            "0fc3a6daebb13c878397ce926ba5084d9d9451202ea2b49fde828cad849d12a4"
        );
        assert_eq!(
            paper_raid_finality_applied_command_key(command_id).unwrap(),
            "ae4e3e53f82583ce55f68c713bd00ed72328ba7baf0452aebec0f17a8ca9d3e9"
        );
        assert_eq!(
            paper_raid_finality_commitment_key(paper_commitment_id).unwrap(),
            "0e550ecec51bce37aabfdbf77ab5e7355540342abbf4a3cebb8b33487e71088f"
        );
        assert_eq!(
            paper_raid_finality_applied_command_key_v3(command_id).unwrap(),
            "452459d6385495c6db30cc833162c5b771a59cc5cdc986e8b8e5014e9ea40638"
        );
        assert_eq!(
            paper_raid_finality_commitment_key_v3(paper_commitment_id).unwrap(),
            "eaa45249c7c5b24831c782e1f35830edcc71ce1e7946d5b2b5aa058c171f437b"
        );
        assert_ne!(
            paper_raid_finality_applied_command_key(command_id).unwrap(),
            paper_raid_finality_applied_command_key_v3(command_id).unwrap()
        );
        assert_ne!(
            paper_raid_finality_commitment_key(paper_commitment_id).unwrap(),
            paper_raid_finality_commitment_key_v3(paper_commitment_id).unwrap()
        );
        assert_ne!(
            paper_raid_finality_applied_command_key_v3(command_id).unwrap(),
            research_applied_command_key(command_id).unwrap()
        );
        assert_eq!(
            research_domain_object_key(ResearchObjectKind::MatchEvidence, object_id).unwrap(),
            "cc279f98ea83df3e8af781699f8839da7125c80ecdc859053b449dc09e748e97"
        );
        assert_ne!(
            research_domain_object_key(ResearchObjectKind::MatchEvidence, object_id).unwrap(),
            research_domain_object_key(ResearchObjectKind::EvaluationCommitment, object_id)
                .unwrap()
        );
        assert!(research_applied_command_key(ExternalKey::from_bytes([0; 32])).is_err());
        assert!(paper_raid_finality_applied_command_key(ExternalKey::from_bytes([0; 32])).is_err());
        assert!(paper_raid_finality_commitment_key(ExternalKey::from_bytes([0; 32])).is_err());
        assert!(
            paper_raid_finality_applied_command_key_v3(ExternalKey::from_bytes([0; 32])).is_err()
        );
        assert!(paper_raid_finality_commitment_key_v3(ExternalKey::from_bytes([0; 32])).is_err());
        assert!(research_domain_object_key(
            ResearchObjectKind::ResearchClaim,
            ExternalKey::from_bytes([0; 32])
        )
        .is_err());
    }

    #[test]
    fn legacy_canonical_transaction_bytes_remain_unchanged() {
        let legacy = CanonicalTxV1 {
            schema: CANONICAL_TX_SCHEMA_V1.into(),
            sender: "alice".into(),
            nonce: 1,
            max_gas: 1_000,
            fee_limit: 5,
            command: CanonicalCommandV1::Transfer {
                to: "bob".into(),
                amount: 2,
            },
        };
        assert_eq!(
            serde_json::to_vec(&legacy).unwrap(),
            br#"{"schema":"trnm_canonical_tx_v1","sender":"alice","nonce":1,"max_gas":1000,"fee_limit":"5","command":{"type":"transfer","to":"bob","amount":"2"}}"#
        );
    }

    #[test]
    fn canonical_research_transaction_has_stable_wire_golden() {
        let signed = signed_research_command();
        let tx = canonical_research_tx();
        let bytes = tx.canonical_bytes().unwrap();
        assert_eq!(
            CanonicalResearchTxV1::from_canonical_bytes(&bytes).unwrap(),
            tx
        );
        assert_eq!(tx.signed_research_command().unwrap(), signed);
        assert_eq!(
            hex::encode(Sha256::digest(&bytes)),
            "fc4cd763b66005da7954ba1857cb3b88a2cde70f817bd05a861d40073851cbe3"
        );
        assert_eq!(bytes.len(), 1_308);
    }

    #[test]
    fn canonical_paper_raid_finality_transaction_is_exact_and_bound() {
        let signed = signed_paper_raid_finality_command();
        let tx = canonical_paper_raid_finality_tx();
        let bytes = tx.canonical_bytes().unwrap();
        assert_eq!(
            CanonicalPaperRaidFinalityTxV3::from_canonical_bytes(&bytes).unwrap(),
            tx
        );
        assert_eq!(tx.signed_paper_raid_finality_command().unwrap(), signed);

        let mut padded = vec![b' '];
        padded.extend_from_slice(&bytes);
        assert!(CanonicalPaperRaidFinalityTxV3::from_canonical_bytes(&padded).is_err());

        let text = String::from_utf8(bytes).unwrap();
        let duplicate = text.replacen(
            '{',
            r#"{"schema":"trnm_canonical_paper_raid_finality_tx_v3","#,
            1,
        );
        assert!(
            CanonicalPaperRaidFinalityTxV3::from_canonical_bytes(duplicate.as_bytes()).is_err()
        );

        let mut wrong_sender = tx.clone();
        wrong_sender.sender = "did:trnm:other-hepta".into();
        assert_eq!(
            wrong_sender.validate(),
            Err(ProtocolError::PaperRaidFinalityBindingMismatch("sender"))
        );

        let mut wrong_payload = tx;
        wrong_payload.payload_type = CANONICAL_RESEARCH_TX_PAYLOAD_TYPE_V1.into();
        assert_eq!(
            wrong_payload.validate(),
            Err(ProtocolError::UnsupportedPayloadType)
        );
    }

    #[test]
    fn frozen_v2_canonical_transaction_remains_exact_and_distinct_from_v3() {
        let signed = signed_paper_raid_finality_command_v2();
        let tx = canonical_paper_raid_finality_tx_v2();
        let bytes = tx.canonical_bytes().unwrap();
        assert_eq!(
            CanonicalPaperRaidFinalityTxV2::from_canonical_bytes(&bytes).unwrap(),
            tx
        );
        assert_eq!(tx.signed_paper_raid_finality_command().unwrap(), signed);
        assert_eq!(tx.schema, CANONICAL_PAPER_RAID_FINALITY_TX_SCHEMA_V2);
        assert_eq!(
            tx.payload_type,
            CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V2
        );
        assert_ne!(
            bytes,
            canonical_paper_raid_finality_tx()
                .canonical_bytes()
                .unwrap()
        );

        assert_eq!(bytes.len(), 1_773);
        assert_eq!(
            hex::encode(Sha256::digest(&bytes)),
            "4870258bb4150ce2bd5bd4970227a49570ff738aac7824feba1526348a4cb244"
        );
    }

    #[test]
    fn paper_raid_finality_applied_record_is_canonical_and_fully_bound() {
        let signed = signed_paper_raid_finality_command();
        let record = PaperRaidFinalityAppliedRecordV3::from_signed(&signed).unwrap();
        assert_eq!(record.command_id, signed.command_id.to_hex());
        assert_eq!(
            record.command_fingerprint_hex,
            hex::encode(signed.command_fingerprint())
        );
        assert_eq!(
            record.commitment_id,
            signed.commitment.commitment_id.to_hex()
        );
        assert_eq!(
            record.commitment_object_key_hex,
            paper_raid_finality_commitment_key_v3(signed.commitment.commitment_id).unwrap()
        );
        assert_eq!(record.payload_hash_hex, hex::encode(signed.payload_hash()));

        let canonical = record.canonical_bytes().unwrap();
        assert_eq!(
            PaperRaidFinalityAppliedRecordV3::from_canonical_bytes(&canonical).unwrap(),
            record
        );

        let mut noncanonical = vec![b' '];
        noncanonical.extend_from_slice(&canonical);
        assert!(PaperRaidFinalityAppliedRecordV3::from_canonical_bytes(&noncanonical).is_err());

        let mut malformed = record;
        malformed.command_fingerprint_hex.make_ascii_uppercase();
        assert_eq!(
            malformed.canonical_bytes(),
            Err(ProtocolError::NonCanonical(
                "paper_raid_finality_v3_command_fingerprint"
            ))
        );
    }

    #[test]
    fn frozen_v2_applied_record_roundtrips_without_v3_reinterpretation() {
        let signed = signed_paper_raid_finality_command_v2();
        let record = PaperRaidFinalityAppliedRecordV2::from_signed(&signed).unwrap();
        assert_eq!(record.schema, PAPER_RAID_FINALITY_APPLIED_RECORD_SCHEMA_V2);
        assert_eq!(record.command_id, signed.command_id.to_hex());
        assert_eq!(
            record.commitment_object_key_hex,
            paper_raid_finality_commitment_key(signed.commitment.commitment_id).unwrap()
        );

        let canonical = record.canonical_bytes().unwrap();
        assert_eq!(
            canonical,
            br#"{"schema":"trnm_paper_raid_finality_applied_record_v2","command_id":"0a7224a9674a35181de2bb45bf395f36f7eb74521b41ad1a1f0195ca53043f0a","command_fingerprint_hex":"c7d6588f7274ad4836d8849c53372becf25a9341927e0f5117973a540084a90d","commitment_id":"65ebcc712d51047207e20917acd0aad3854ffdb0dae58e7672c7d42bc67d9d1b","commitment_object_key_hex":"2f8b7a58f647f64eca7f43e2ddbb8362ce417be616ce5c8d925199d93b39135d","payload_hash_hex":"176b3f0cd6811758d8d5b86edd591bbbc495dd904a6fe20dab10c079c0cb14ed"}"#
        );
        assert_eq!(
            PaperRaidFinalityAppliedRecordV2::from_canonical_bytes(&canonical).unwrap(),
            record
        );
        assert!(PaperRaidFinalityAppliedRecordV3::from_canonical_bytes(&canonical).is_err());
    }

    #[test]
    fn canonical_research_transaction_rejects_wire_variants() {
        let bytes = canonical_research_tx().canonical_bytes().unwrap();

        let mut whitespace = vec![b' '];
        whitespace.extend_from_slice(&bytes);
        assert!(CanonicalResearchTxV1::from_canonical_bytes(&whitespace).is_err());

        let text = String::from_utf8(bytes).unwrap();
        let duplicate = text.replacen('{', r#"{"schema":"trnm_canonical_research_tx_v1","#, 1);
        assert!(CanonicalResearchTxV1::from_canonical_bytes(duplicate.as_bytes()).is_err());

        let unknown = text.replacen('}', r#","unknown":true}"#, 1);
        assert!(CanonicalResearchTxV1::from_canonical_bytes(unknown.as_bytes()).is_err());

        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let reordered = serde_json::to_vec(&value).unwrap();
        assert_ne!(reordered, text.as_bytes());
        assert!(CanonicalResearchTxV1::from_canonical_bytes(&reordered).is_err());

        let noncanonical_decimal = text.replace(
            r#""fee_limit":"12345678901234567890""#,
            r#""fee_limit":"012345678901234567890""#,
        );
        assert!(
            CanonicalResearchTxV1::from_canonical_bytes(noncanonical_decimal.as_bytes()).is_err()
        );

        assert_eq!(
            CanonicalResearchTxV1::from_canonical_bytes(&vec![
                b'x';
                MAX_CANONICAL_RESEARCH_TX_BYTES + 1
            ]),
            Err(ProtocolError::OutOfRange("canonical_research_tx"))
        );
    }

    #[test]
    fn canonical_research_transaction_binds_every_outer_field() {
        let baseline = canonical_research_tx();

        let mut wrong_schema = baseline.clone();
        wrong_schema.schema = CANONICAL_TX_SCHEMA_V1.into();
        assert_eq!(
            wrong_schema.validate(),
            Err(ProtocolError::UnsupportedSchema)
        );

        let mut wrong_payload_type = baseline.clone();
        wrong_payload_type.payload_type = CANONICAL_TX_PAYLOAD_TYPE_V1.into();
        assert_eq!(
            wrong_payload_type.validate(),
            Err(ProtocolError::UnsupportedPayloadType)
        );

        let mut wrong_command_id = baseline.clone();
        wrong_command_id.command_id = "aa".repeat(32);
        assert_eq!(
            wrong_command_id.validate(),
            Err(ProtocolError::ResearchBindingMismatch("command_id"))
        );

        let mut uppercase_command_id = baseline.clone();
        uppercase_command_id.command_id.make_ascii_uppercase();
        assert_eq!(
            uppercase_command_id.validate(),
            Err(ProtocolError::NonCanonical("command_id"))
        );

        let mut wrong_sender = baseline.clone();
        wrong_sender.sender = "did:trnm:other-authority".into();
        assert_eq!(
            wrong_sender.validate(),
            Err(ProtocolError::ResearchBindingMismatch("sender"))
        );

        let mut wrong_nonce = baseline.clone();
        wrong_nonce.nonce += 1;
        assert_eq!(
            wrong_nonce.validate(),
            Err(ProtocolError::ResearchBindingMismatch("nonce"))
        );

        let mut zero_nonce = baseline.clone();
        zero_nonce.nonce = 0;
        assert_eq!(
            zero_nonce.validate(),
            Err(ProtocolError::NonPositive("nonce"))
        );

        let mut zero_gas = baseline.clone();
        zero_gas.max_gas = 0;
        assert_eq!(
            zero_gas.validate(),
            Err(ProtocolError::NonPositive("max_gas"))
        );

        let mut uppercase_cbor = baseline.clone();
        uppercase_cbor
            .signed_research_command_cbor_hex
            .make_ascii_uppercase();
        assert_eq!(
            uppercase_cbor.validate(),
            Err(ProtocolError::NonCanonical(
                "signed_research_command_cbor_hex"
            ))
        );

        let mut altered_signature = baseline;
        let replacement = if altered_signature
            .signed_research_command_cbor_hex
            .ends_with('0')
        {
            '1'
        } else {
            '0'
        };
        altered_signature.signed_research_command_cbor_hex.pop();
        altered_signature
            .signed_research_command_cbor_hex
            .push(replacement);
        assert_eq!(
            altered_signature.validate(),
            Err(ProtocolError::InvalidSignedResearchCommand)
        );
    }

    #[test]
    fn result_commitment_binds_every_field() {
        let hash = "11".repeat(32);
        let salt = "22".repeat(32);
        let baseline = result_commitment_hex("task-1", "worker-1", &hash, &salt).unwrap();
        assert_ne!(
            baseline,
            result_commitment_hex("task-2", "worker-1", &hash, &salt).unwrap()
        );
        assert_ne!(
            baseline,
            result_commitment_hex("task-1", "worker-2", &hash, &salt).unwrap()
        );
        assert_ne!(
            baseline,
            result_commitment_hex("task-1", "worker-1", &"33".repeat(32), &salt).unwrap()
        );
        assert_ne!(
            baseline,
            result_commitment_hex("task-1", "worker-1", &hash, &"44".repeat(32)).unwrap()
        );
    }

    #[test]
    fn identifiers_and_decimal_values_are_canonical() {
        assert!(validate_id("id", "did:worker:1").is_ok());
        for invalid in ["", " worker", "worker ", "work er", "工人", "worker#1"] {
            assert!(validate_id("id", invalid).is_err(), "{invalid:?}");
        }
        let leading_zero = br#"{
            "schema":"trnm_canonical_tx_v1",
            "sender":"alice",
            "nonce":1,
            "max_gas":1000,
            "fee_limit":"01",
            "command":{"type":"transfer","to":"bob","amount":"1"}
        }"#;
        assert!(serde_json::from_slice::<CanonicalTxV1>(leading_zero).is_err());
    }
}
