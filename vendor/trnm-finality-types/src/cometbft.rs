//! Versioned, transport-neutral CometBFT/AppHash receipt wire types.
//!
//! These types deliberately do not reinterpret [`crate::FinalityReceiptV1`].
//! V1 describes the frozen synthetic quorum protocol.  V2 carries the
//! cross-height evidence needed for canonical CometBFT finality: a transaction
//! executes in block `H`, while its post-state AppHash and execution-result root
//! are committed by block `H + 1`.

use anyhow::{anyhow, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    authenticated_object_proof_key_v4, decode_hash32, hash_domain, AuthenticatedObjectRecordV1,
    Hash32, MAX_SIGNED_COMMAND_ENVELOPE_WIRE_BYTES,
};

pub const APPHASH_OBJECT_PROOF_SCHEMA_V1: &str = "trnm_apphash_object_proof_v1";
pub const COMETBFT_HEADER_SCHEMA_V1: &str = "trnm_cometbft_header_v1";
pub const COMETBFT_LIGHT_FINALITY_PROOF_SCHEMA_V1: &str = "trnm_cometbft_light_finality_proof_v1";
pub const COMETBFT_MERKLE_INCLUSION_PROOF_SCHEMA_V1: &str =
    "trnm_cometbft_merkle_inclusion_proof_v1";
pub const COMETBFT_APPHASH_FINALITY_RECEIPT_SCHEMA_V2: &str =
    "trnm_cometbft_apphash_finality_receipt_v2";
pub const COMETBFT_TRUST_ANCHOR_SCHEMA_V1: &str = "trnm_cometbft_trust_anchor_v1";
pub const COMETBFT_JMT_PROOF_OP_TYPE_V1: &str = "ics23:jmt:v1";
pub const MAX_COMETBFT_RECEIPT_V2_WIRE_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_COMETBFT_TRUST_ANCHOR_V1_WIRE_BYTES: usize = 16 * 1024 * 1024;

const MAX_WIRE_KEY_BYTES: usize = 4 * 1024;
const MAX_RESULT_BYTES: usize = 4 * 1024 * 1024;
const MAX_OBJECT_VALUE_BYTES: usize = 16 * 1024 * 1024;
const MAX_COMMITMENT_PROOF_BYTES: usize = 1024 * 1024;
const MAX_PROTO_BYTES: usize = 4 * 1024 * 1024;
const MAX_MERKLE_LEAVES: u64 = u32::MAX as u64;
const MAX_MERKLE_AUNTS: usize = 64;

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
    Ok(())
}

fn ensure_receipt_v2_wire_len(len: usize) -> Result<()> {
    ensure!(len > 0, "Receipt V2 wire must not be empty");
    ensure!(
        len <= MAX_COMETBFT_RECEIPT_V2_WIRE_BYTES,
        "Receipt V2 wire exceeds the {}-byte limit",
        MAX_COMETBFT_RECEIPT_V2_WIRE_BYTES
    );
    Ok(())
}

fn ensure_trust_anchor_v1_wire_len(len: usize) -> Result<()> {
    ensure!(len > 0, "CometBFT trust anchor wire must not be empty");
    ensure!(
        len <= MAX_COMETBFT_TRUST_ANCHOR_V1_WIRE_BYTES,
        "CometBFT trust anchor wire exceeds the {}-byte limit",
        MAX_COMETBFT_TRUST_ANCHOR_V1_WIRE_BYTES
    );
    Ok(())
}

fn decode_canonical_hex(
    label: &str,
    value: &str,
    min_bytes: usize,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    let bytes = hex::decode(value).map_err(|_| anyhow!("{label} must be lowercase hex"))?;
    ensure!(
        bytes.len() >= min_bytes,
        "{label} must encode at least {min_bytes} bytes"
    );
    ensure!(
        bytes.len() <= max_bytes,
        "{label} exceeds the {max_bytes}-byte limit"
    );
    ensure!(
        hex::encode(&bytes) == value,
        "{label} must use canonical lowercase hex"
    );
    Ok(bytes)
}

fn comet_leaf_hash(value: &[u8]) -> Hash32 {
    let mut hasher = Sha256::new();
    hasher.update([0]);
    hasher.update(value);
    hasher.finalize().into()
}

/// CometBFT transaction identity is the plain SHA-256 of the exact raw bytes.
pub fn comet_tx_hash(raw_tx: &[u8]) -> Hash32 {
    Sha256::digest(raw_tx).into()
}

fn comet_inner_hash(left: &Hash32, right: &Hash32) -> Hash32 {
    let mut hasher = Sha256::new();
    hasher.update([1]);
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

fn split_point(total: u64) -> u64 {
    debug_assert!(total > 1);
    let mut split = 1u64;
    while split < total {
        split <<= 1;
    }
    split >> 1
}

fn compute_hash_from_aunts(
    index: u64,
    total: u64,
    leaf_hash: Hash32,
    aunts: &[Hash32],
) -> Result<Hash32> {
    ensure!(total > 0, "Merkle proof total must be positive");
    ensure!(index < total, "Merkle proof index is out of range");
    if total == 1 {
        ensure!(aunts.is_empty(), "single-leaf proof must not carry aunts");
        return Ok(leaf_hash);
    }
    let (aunt, remaining) = aunts
        .split_last()
        .ok_or_else(|| anyhow!("Merkle proof does not contain enough aunts"))?;
    let split = split_point(total);
    if index < split {
        let left = compute_hash_from_aunts(index, split, leaf_hash, remaining)?;
        Ok(comet_inner_hash(&left, aunt))
    } else {
        let right = compute_hash_from_aunts(index - split, total - split, leaf_hash, remaining)?;
        Ok(comet_inner_hash(aunt, &right))
    }
}

fn comet_root_from_hashes(hashes: &[Hash32]) -> Result<Hash32> {
    ensure!(!hashes.is_empty(), "CometBFT Merkle tree must not be empty");
    if hashes.len() == 1 {
        return Ok(hashes[0]);
    }
    let total = u64::try_from(hashes.len()).context("Merkle leaf count exceeds u64")?;
    let split = usize::try_from(split_point(total)).context("Merkle split exceeds usize")?;
    Ok(comet_inner_hash(
        &comet_root_from_hashes(&hashes[..split])?,
        &comet_root_from_hashes(&hashes[split..])?,
    ))
}

fn comet_aunts_from_hashes(hashes: &[Hash32], index: usize) -> Result<Vec<Hash32>> {
    ensure!(!hashes.is_empty(), "CometBFT Merkle tree must not be empty");
    ensure!(index < hashes.len(), "Merkle proof index is out of range");
    if hashes.len() == 1 {
        return Ok(Vec::new());
    }
    let total = u64::try_from(hashes.len()).context("Merkle leaf count exceeds u64")?;
    let split = usize::try_from(split_point(total)).context("Merkle split exceeds usize")?;
    if index < split {
        let mut aunts = comet_aunts_from_hashes(&hashes[..split], index)?;
        aunts.push(comet_root_from_hashes(&hashes[split..])?);
        Ok(aunts)
    } else {
        let mut aunts = comet_aunts_from_hashes(&hashes[split..], index - split)?;
        aunts.push(comet_root_from_hashes(&hashes[..split])?);
        Ok(aunts)
    }
}

/// Exact ABCI `ProofOp` bytes carried by an authenticated object query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppHashProofOpV1 {
    #[serde(rename = "type")]
    pub proof_type: String,
    pub key_hex: String,
    pub data_hex: String,
}

impl AppHashProofOpV1 {
    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.proof_type == COMETBFT_JMT_PROOF_OP_TYPE_V1,
            "unsupported AppHash proof op type"
        );
        let _ = decode_canonical_hex("proof_op.key_hex", &self.key_hex, 1, MAX_WIRE_KEY_BYTES)?;
        let _ = decode_canonical_hex(
            "proof_op.data_hex",
            &self.data_hex,
            1,
            MAX_COMMITMENT_PROOF_BYTES,
        )?;
        Ok(())
    }
}

/// Stable wire representation of a JMT/ICS23 object-membership response.
///
/// `object_key_hex` is the exact namespaced key returned by ABCI, not the
/// human-facing Research object identifier.
///
/// `commitment_proof_hex` intentionally duplicates `proof_op.data_hex`.  The
/// explicit field lets proof verifiers decode the ICS23 protobuf without
/// depending on an ABCI protobuf type, while validation guarantees that both
/// views are byte-identical.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppHashObjectProofV1 {
    pub schema: String,
    pub query_height: u64,
    pub object_key_hex: String,
    pub value_hex: String,
    pub proof_op: AppHashProofOpV1,
    pub commitment_proof_hex: String,
}

impl AppHashObjectProofV1 {
    pub fn object_key_bytes(&self) -> Result<Vec<u8>> {
        decode_canonical_hex(
            "object_key_hex",
            &self.object_key_hex,
            1,
            MAX_WIRE_KEY_BYTES,
        )
    }

    pub fn value_bytes(&self) -> Result<Vec<u8>> {
        decode_canonical_hex("value_hex", &self.value_hex, 1, MAX_OBJECT_VALUE_BYTES)
    }

    pub fn commitment_proof_bytes(&self) -> Result<Vec<u8>> {
        decode_canonical_hex(
            "commitment_proof_hex",
            &self.commitment_proof_hex,
            1,
            MAX_COMMITMENT_PROOF_BYTES,
        )
    }

    /// Bind the exact ABCI/JMT proof key to a logical object key and strictly
    /// decode the authenticated Borsh wrapper carried as the proof value.
    pub fn decode_authenticated_object_record(
        &self,
        logical_object_key: &str,
    ) -> Result<AuthenticatedObjectRecordV1> {
        self.validate_shape()?;
        let expected_proof_key = authenticated_object_proof_key_v4(logical_object_key)?;
        ensure!(
            self.object_key_bytes()? == expected_proof_key,
            "AppHash proof key does not match the logical object key"
        );
        AuthenticatedObjectRecordV1::decode(&self.value_bytes()?)
    }

    /// Bind proof key, object wrapper type/version, and the exact inner value.
    pub fn verify_authenticated_object_binding(
        &self,
        logical_object_key: &str,
        expected_object_type: &str,
        expected_object_version: u64,
        expected_value: &[u8],
    ) -> Result<AuthenticatedObjectRecordV1> {
        let record = self.decode_authenticated_object_record(logical_object_key)?;
        record.verify_binding(
            expected_object_type,
            expected_object_version,
            expected_value,
        )?;
        Ok(record)
    }

    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.schema == APPHASH_OBJECT_PROOF_SCHEMA_V1,
            "unsupported AppHash object proof schema"
        );
        ensure!(self.query_height > 0, "query_height must be at least 1");
        let _ = self.object_key_bytes()?;
        let _ = self.value_bytes()?;
        let _ = self.commitment_proof_bytes()?;
        self.proof_op.validate_shape()?;
        ensure!(
            self.proof_op.key_hex == self.object_key_hex,
            "proof op key does not match object_key_hex"
        );
        ensure!(
            self.proof_op.data_hex == self.commitment_proof_hex,
            "proof op data does not match commitment_proof_hex"
        );
        Ok(())
    }
}

/// Relevant CometBFT header commitments plus the exact protobuf header bytes.
///
/// A verifier must recompute `header_hash_hex` from `header_proto_hex`; the
/// shape validator only enforces canonical encoding and cross-field linkage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CometBftHeaderV1 {
    pub schema: String,
    pub chain_id: String,
    pub height: u64,
    pub header_hash_hex: String,
    pub last_block_id_hash_hex: Option<String>,
    pub data_hash_hex: Option<String>,
    pub app_hash_hex: String,
    pub last_results_hash_hex: Option<String>,
    pub validators_hash_hex: String,
    pub next_validators_hash_hex: String,
    pub header_proto_hex: String,
}

impl CometBftHeaderV1 {
    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.schema == COMETBFT_HEADER_SCHEMA_V1,
            "unsupported CometBFT header schema"
        );
        ensure_token("header.chain_id", &self.chain_id, 128)?;
        ensure!(self.height > 0, "header height must be at least 1");
        let _ = decode_hash32("header_hash_hex", &self.header_hash_hex)?;
        if let Some(hash) = &self.last_block_id_hash_hex {
            let _ = decode_hash32("last_block_id_hash_hex", hash)?;
        }
        if let Some(hash) = &self.data_hash_hex {
            let _ = decode_hash32("data_hash_hex", hash)?;
        }
        let _ = decode_hash32("app_hash_hex", &self.app_hash_hex)?;
        if let Some(hash) = &self.last_results_hash_hex {
            let _ = decode_hash32("last_results_hash_hex", hash)?;
        }
        let _ = decode_hash32("validators_hash_hex", &self.validators_hash_hex)?;
        let _ = decode_hash32("next_validators_hash_hex", &self.next_validators_hash_hex)?;
        let _ = decode_canonical_hex(
            "header_proto_hex",
            &self.header_proto_hex,
            1,
            MAX_PROTO_BYTES,
        )?;
        Ok(())
    }
}

/// A signed `H + 1` header and the validator set required to verify its commit.
///
/// This is evidence, not a self-authenticating trust root.  Callers must supply
/// an independently trusted light-client state when cryptographically verifying
/// the signed header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CometBftLightFinalityProofV1 {
    pub schema: String,
    pub header: CometBftHeaderV1,
    pub commit_height: u64,
    pub commit_block_id_hash_hex: String,
    pub signed_header_proto_hex: String,
    pub validator_set_proto_hex: String,
}

impl CometBftLightFinalityProofV1 {
    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.schema == COMETBFT_LIGHT_FINALITY_PROOF_SCHEMA_V1,
            "unsupported CometBFT light finality proof schema"
        );
        self.header.validate_shape()?;
        ensure!(
            self.commit_height == self.header.height,
            "commit height does not match signed header"
        );
        let _ = decode_hash32("commit_block_id_hash_hex", &self.commit_block_id_hash_hex)?;
        ensure!(
            self.commit_block_id_hash_hex == self.header.header_hash_hex,
            "commit block ID does not match signed header hash"
        );
        let _ = decode_canonical_hex(
            "signed_header_proto_hex",
            &self.signed_header_proto_hex,
            1,
            MAX_PROTO_BYTES,
        )?;
        let _ = decode_canonical_hex(
            "validator_set_proto_hex",
            &self.validator_set_proto_hex,
            1,
            MAX_PROTO_BYTES,
        )?;
        Ok(())
    }
}

/// CometBFT simple-Merkle inclusion proof with exact leaf bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CometBftMerkleInclusionProofV1 {
    pub schema: String,
    pub leaf_index: u64,
    pub leaf_count: u64,
    pub leaf_value_hex: String,
    pub leaf_hash_hex: String,
    pub aunts_hex: Vec<String>,
}

impl CometBftMerkleInclusionProofV1 {
    /// Build the exact CometBFT simple-Merkle proof for one value in an
    /// ordered block/result list. Aunts are emitted leaf-to-root, matching the
    /// canonical Comet proof wire convention.
    pub fn from_leaf_values<T: AsRef<[u8]>>(values: &[T], leaf_index: usize) -> Result<Self> {
        ensure!(!values.is_empty(), "Merkle leaf list must not be empty");
        ensure!(
            values.len() <= MAX_MERKLE_LEAVES as usize,
            "Merkle leaf list exceeds the wire limit"
        );
        ensure!(
            leaf_index < values.len(),
            "Merkle proof index is out of range"
        );
        let mut hashes = Vec::with_capacity(values.len());
        for value in values {
            let value = value.as_ref();
            ensure!(
                value.len() <= MAX_RESULT_BYTES,
                "Merkle leaf value exceeds the wire limit"
            );
            hashes.push(comet_leaf_hash(value));
        }
        let proof = Self {
            schema: COMETBFT_MERKLE_INCLUSION_PROOF_SCHEMA_V1.to_string(),
            leaf_index: u64::try_from(leaf_index).context("Merkle leaf index exceeds u64")?,
            leaf_count: u64::try_from(values.len()).context("Merkle leaf count exceeds u64")?,
            leaf_value_hex: hex::encode(values[leaf_index].as_ref()),
            leaf_hash_hex: hex::encode(hashes[leaf_index]),
            aunts_hex: comet_aunts_from_hashes(&hashes, leaf_index)?
                .into_iter()
                .map(hex::encode)
                .collect(),
        };
        proof.validate_shape()?;
        Ok(proof)
    }

    pub fn leaf_value_bytes(&self) -> Result<Vec<u8>> {
        // A successful, otherwise-default CometBFT `ExecTxResult` has an empty
        // protobuf encoding, so the Merkle leaf must permit zero bytes.
        decode_canonical_hex("leaf_value_hex", &self.leaf_value_hex, 0, MAX_RESULT_BYTES)
    }

    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.schema == COMETBFT_MERKLE_INCLUSION_PROOF_SCHEMA_V1,
            "unsupported CometBFT Merkle proof schema"
        );
        ensure!(self.leaf_count > 0, "leaf_count must be positive");
        ensure!(
            self.leaf_count <= MAX_MERKLE_LEAVES,
            "leaf_count exceeds the wire limit"
        );
        ensure!(
            self.leaf_index < self.leaf_count,
            "leaf_index is out of range"
        );
        ensure!(
            self.aunts_hex.len() <= MAX_MERKLE_AUNTS,
            "Merkle proof carries too many aunts"
        );
        let value = self.leaf_value_bytes()?;
        let leaf_hash = decode_hash32("leaf_hash_hex", &self.leaf_hash_hex)?;
        ensure!(
            leaf_hash == comet_leaf_hash(&value),
            "leaf_hash_hex does not bind leaf_value_hex"
        );
        let aunts = self
            .aunts_hex
            .iter()
            .map(|hash| decode_hash32("Merkle aunt", hash))
            .collect::<Result<Vec<_>>>()?;
        let _ = compute_hash_from_aunts(self.leaf_index, self.leaf_count, leaf_hash, &aunts)?;
        Ok(())
    }

    pub fn root_hash(&self) -> Result<Hash32> {
        self.validate_shape()?;
        let leaf_hash = decode_hash32("leaf_hash_hex", &self.leaf_hash_hex)?;
        let aunts = self
            .aunts_hex
            .iter()
            .map(|hash| decode_hash32("Merkle aunt", hash))
            .collect::<Result<Vec<_>>>()?;
        compute_hash_from_aunts(self.leaf_index, self.leaf_count, leaf_hash, &aunts)
    }
}

/// Canonical, externally authenticated CometBFT light-client trust root.
///
/// This document is not self-authenticating. Operators must obtain it from a
/// pinned genesis/checkpoint ceremony or an already verified light store. The
/// explicit header and validator-set protobuf bytes let the verifier rebuild
/// the exact Tendermint objects without exposing those implementation types to
/// receipt consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CometBftTrustAnchorV1 {
    pub schema: String,
    pub trusted_header: CometBftHeaderV1,
    pub trusted_header_time_rfc3339: String,
    pub trusted_next_validator_set_proto_hex: String,
    pub trust_threshold_numerator: u32,
    pub trust_threshold_denominator: u32,
    pub trusting_period_seconds: u64,
    pub clock_drift_seconds: u64,
    pub anchor_hash_hex: String,
}

#[derive(Serialize)]
struct UnsignedCometBftTrustAnchorV1<'a> {
    schema: &'a str,
    trusted_header: &'a CometBftHeaderV1,
    trusted_header_time_rfc3339: &'a str,
    trusted_next_validator_set_proto_hex: &'a str,
    trust_threshold_numerator: u32,
    trust_threshold_denominator: u32,
    trusting_period_seconds: u64,
    clock_drift_seconds: u64,
}

impl CometBftTrustAnchorV1 {
    /// Decode only the exact compact JSON representation emitted by
    /// [`Self::canonical_bytes`]. Duplicate/unknown members, whitespace,
    /// reordered fields, and alternate number spellings fail closed.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self> {
        ensure_trust_anchor_v1_wire_len(bytes.len())?;
        let anchor: Self =
            serde_json::from_slice(bytes).context("decode CometBFT trust anchor JSON")?;
        anchor.validate_shape()?;
        let canonical = serde_json::to_vec(&anchor)?;
        ensure!(
            canonical == bytes,
            "CometBFT trust anchor JSON is not canonical"
        );
        Ok(anchor)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate_shape()?;
        let bytes = serde_json::to_vec(self)?;
        ensure_trust_anchor_v1_wire_len(bytes.len())?;
        Ok(bytes)
    }

    pub fn unsigned_bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.schema == COMETBFT_TRUST_ANCHOR_SCHEMA_V1,
            "unsupported CometBFT trust anchor schema"
        );
        self.trusted_header.validate_shape()?;
        let bytes = serde_json::to_vec(&UnsignedCometBftTrustAnchorV1 {
            schema: &self.schema,
            trusted_header: &self.trusted_header,
            trusted_header_time_rfc3339: &self.trusted_header_time_rfc3339,
            trusted_next_validator_set_proto_hex: &self.trusted_next_validator_set_proto_hex,
            trust_threshold_numerator: self.trust_threshold_numerator,
            trust_threshold_denominator: self.trust_threshold_denominator,
            trusting_period_seconds: self.trusting_period_seconds,
            clock_drift_seconds: self.clock_drift_seconds,
        })?;
        ensure_trust_anchor_v1_wire_len(bytes.len())?;
        Ok(bytes)
    }

    pub fn compute_anchor_hash(&self) -> Result<Hash32> {
        Ok(hash_domain(
            "trnm.cometbft.trust.anchor.v1",
            &[&self.unsigned_bytes()?],
        ))
    }

    pub fn trusted_next_validator_set_proto_bytes(&self) -> Result<Vec<u8>> {
        decode_canonical_hex(
            "trusted_next_validator_set_proto_hex",
            &self.trusted_next_validator_set_proto_hex,
            1,
            MAX_PROTO_BYTES,
        )
    }

    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.schema == COMETBFT_TRUST_ANCHOR_SCHEMA_V1,
            "unsupported CometBFT trust anchor schema"
        );
        self.trusted_header.validate_shape()?;
        ensure!(
            !self.trusted_header_time_rfc3339.is_empty()
                && self.trusted_header_time_rfc3339.len() <= 64
                && self.trusted_header_time_rfc3339 == self.trusted_header_time_rfc3339.trim()
                && self.trusted_header_time_rfc3339.ends_with('Z')
                && self
                    .trusted_header_time_rfc3339
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic()),
            "trusted_header_time_rfc3339 is not canonical UTC text"
        );
        let _ = self.trusted_next_validator_set_proto_bytes()?;
        ensure!(
            self.trust_threshold_numerator > 0
                && self.trust_threshold_denominator > 0
                && self.trust_threshold_numerator <= self.trust_threshold_denominator,
            "invalid CometBFT trust threshold fraction"
        );
        ensure!(
            u64::from(self.trust_threshold_numerator) * 3
                >= u64::from(self.trust_threshold_denominator),
            "CometBFT trust threshold must be at least one third"
        );
        ensure!(
            gcd_u32(
                self.trust_threshold_numerator,
                self.trust_threshold_denominator
            ) == 1,
            "CometBFT trust threshold fraction must be reduced"
        );
        ensure!(
            self.trusting_period_seconds > 0,
            "CometBFT trusting period must be positive"
        );
        ensure!(
            self.clock_drift_seconds < self.trusting_period_seconds,
            "CometBFT clock drift must be shorter than the trusting period"
        );
        let expected_hash = self.compute_anchor_hash()?;
        ensure!(
            decode_hash32("anchor_hash_hex", &self.anchor_hash_hex)? == expected_hash,
            "CometBFT trust anchor hash mismatch"
        );
        Ok(())
    }
}

fn gcd_u32(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

/// Canonical CometBFT/AppHash finality receipt.
///
/// The transaction is included and executed at `execution_height = H`.
/// CometBFT commits the resulting AppHash and `LastResultsHash` in the header at
/// `commitment_height = H + 1`; the light proof therefore finalizes `H + 1` and
/// links it back to the execution block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CometBftAppHashFinalityReceiptV2 {
    pub schema: String,
    pub chain_id: String,
    pub command_id: String,
    pub command_fingerprint_hex: String,
    pub execution_height: u64,
    pub commitment_height: u64,
    pub comet_tx_hash_hex: String,
    /// Exact app-v5 canonical outer `SignedCommandEnvelopeV1` bytes committed
    /// in block `H`.
    pub raw_tx_hex: String,
    pub execution_header: CometBftHeaderV1,
    pub commitment_light_proof: CometBftLightFinalityProofV1,
    pub transaction_inclusion_proof: CometBftMerkleInclusionProofV1,
    /// Exact deterministic `ExecTxResult` bytes committed by CometBFT.
    /// CometBFT strips log, info, events, and codespace before computing
    /// `LastResultsHash`; Research identity is instead bound by the
    /// authenticated applied-command object proof below.
    pub canonical_result_bytes_hex: String,
    pub result_inclusion_proof: CometBftMerkleInclusionProofV1,
    pub applied_command_object_proof: AppHashObjectProofV1,
    pub receipt_hash_hex: String,
}

impl CometBftAppHashFinalityReceiptV2 {
    /// Decode only the exact compact JSON representation emitted by
    /// [`Self::canonical_bytes`], with an overall limit applied before Serde
    /// allocates any attacker-controlled strings or arrays.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self> {
        ensure_receipt_v2_wire_len(bytes.len())?;
        let receipt: Self = serde_json::from_slice(bytes).context("decode Receipt V2 JSON")?;
        receipt.validate_shape()?;
        let canonical = serde_json::to_vec(&receipt)?;
        ensure!(canonical == bytes, "Receipt V2 JSON is not canonical");
        Ok(receipt)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate_shape()?;
        let bytes = serde_json::to_vec(self)?;
        ensure_receipt_v2_wire_len(bytes.len())?;
        Ok(bytes)
    }

    pub fn raw_tx_bytes(&self) -> Result<Vec<u8>> {
        decode_canonical_hex(
            "raw_tx_hex",
            &self.raw_tx_hex,
            1,
            MAX_SIGNED_COMMAND_ENVELOPE_WIRE_BYTES,
        )
    }

    pub fn canonical_result_bytes(&self) -> Result<Vec<u8>> {
        decode_canonical_hex(
            "canonical_result_bytes_hex",
            &self.canonical_result_bytes_hex,
            0,
            MAX_RESULT_BYTES,
        )
    }

    /// Strictly decode the authenticated applied-command wrapper after
    /// rebinding its proof key to the supplied logical Research object key.
    pub fn decode_applied_command_object_record(
        &self,
        logical_object_key: &str,
    ) -> Result<AuthenticatedObjectRecordV1> {
        self.validate_shape()?;
        self.applied_command_object_proof
            .decode_authenticated_object_record(logical_object_key)
    }

    /// Bind the applied-command proof to its logical key and every committed
    /// wrapper field.  Cryptographic ICS23/AppHash verification remains the
    /// verifier crate's responsibility.
    pub fn verify_applied_command_object_binding(
        &self,
        logical_object_key: &str,
        expected_object_type: &str,
        expected_object_version: u64,
        expected_value: &[u8],
    ) -> Result<AuthenticatedObjectRecordV1> {
        self.validate_shape()?;
        self.applied_command_object_proof
            .verify_authenticated_object_binding(
                logical_object_key,
                expected_object_type,
                expected_object_version,
                expected_value,
            )
    }

    fn validate_common(&self, require_receipt_hash: bool) -> Result<()> {
        ensure!(
            self.schema == COMETBFT_APPHASH_FINALITY_RECEIPT_SCHEMA_V2,
            "unsupported CometBFT AppHash finality receipt schema"
        );
        ensure_token("chain_id", &self.chain_id, 128)?;
        ensure_token("command_id", &self.command_id, 160)?;
        let _ = decode_hash32("command_fingerprint_hex", &self.command_fingerprint_hex)?;
        ensure!(
            self.execution_height > 0,
            "execution_height must be at least 1"
        );
        let expected_commitment_height = self
            .execution_height
            .checked_add(1)
            .ok_or_else(|| anyhow!("execution_height cannot be incremented"))?;
        ensure!(
            self.commitment_height == expected_commitment_height,
            "commitment_height must equal execution_height + 1"
        );

        let raw_tx = self.raw_tx_bytes()?;
        let expected_tx_hash = comet_tx_hash(&raw_tx);
        ensure!(
            decode_hash32("comet_tx_hash_hex", &self.comet_tx_hash_hex)? == expected_tx_hash,
            "comet_tx_hash_hex does not equal SHA-256(raw_tx)"
        );

        self.execution_header.validate_shape()?;
        self.commitment_light_proof.validate_shape()?;
        ensure!(
            self.execution_header.chain_id == self.chain_id
                && self.commitment_light_proof.header.chain_id == self.chain_id,
            "receipt chain_id does not match both headers"
        );
        ensure!(
            self.execution_header.height == self.execution_height,
            "execution header height mismatch"
        );
        ensure!(
            self.commitment_light_proof.header.height == self.commitment_height,
            "commitment header height mismatch"
        );
        ensure!(
            self.commitment_light_proof
                .header
                .last_block_id_hash_hex
                .as_deref()
                == Some(self.execution_header.header_hash_hex.as_str()),
            "H + 1 last_block_id does not link to the H header"
        );

        self.transaction_inclusion_proof.validate_shape()?;
        ensure!(
            self.transaction_inclusion_proof.leaf_value_hex == self.comet_tx_hash_hex,
            "transaction proof leaf does not bind comet_tx_hash_hex"
        );
        let data_hash = self
            .execution_header
            .data_hash_hex
            .as_ref()
            .ok_or_else(|| anyhow!("execution header must carry data_hash_hex"))?;
        ensure!(
            self.transaction_inclusion_proof.root_hash()?
                == decode_hash32("data_hash_hex", data_hash)?,
            "transaction proof root does not match header H data_hash"
        );

        let _ = self.canonical_result_bytes()?;
        self.result_inclusion_proof.validate_shape()?;
        ensure!(
            self.result_inclusion_proof.leaf_value_hex == self.canonical_result_bytes_hex,
            "result proof leaf does not bind canonical_result_bytes_hex"
        );
        ensure!(
            self.result_inclusion_proof.leaf_index == self.transaction_inclusion_proof.leaf_index,
            "transaction and result proof indices differ"
        );
        let results_hash = self
            .commitment_light_proof
            .header
            .last_results_hash_hex
            .as_ref()
            .ok_or_else(|| anyhow!("commitment header must carry last_results_hash_hex"))?;
        ensure!(
            self.result_inclusion_proof.root_hash()?
                == decode_hash32("last_results_hash_hex", results_hash)?,
            "result proof root does not match header H + 1 last_results_hash"
        );

        self.applied_command_object_proof.validate_shape()?;
        ensure!(
            self.applied_command_object_proof.query_height == self.execution_height,
            "applied-command proof query_height must equal execution_height"
        );
        // Decoding the versioned applied-command record, verifying its ICS23
        // proof against the H + 1 AppHash, and comparing the embedded command
        // fingerprint are cryptographic verifier responsibilities.  This wire
        // layer keeps the evidence exact and rejects ambiguous encodings.

        if require_receipt_hash {
            let receipt_hash = decode_hash32("receipt_hash_hex", &self.receipt_hash_hex)?;
            ensure!(
                receipt_hash == self.compute_receipt_hash()?,
                "receipt_hash_hex mismatch"
            );
        } else {
            ensure!(
                self.receipt_hash_hex.is_empty()
                    || decode_hash32("receipt_hash_hex", &self.receipt_hash_hex).is_ok(),
                "receipt_hash_hex is malformed"
            );
        }
        Ok(())
    }

    pub fn validate_shape(&self) -> Result<()> {
        self.validate_common(true)
    }

    pub fn unsigned_bytes(&self) -> Result<Vec<u8>> {
        self.validate_common(false)?;
        let mut copy = self.clone();
        copy.receipt_hash_hex.clear();
        serde_json::to_vec(&copy).map_err(Into::into)
    }

    pub fn compute_receipt_hash(&self) -> Result<Hash32> {
        Ok(hash_domain(
            "trnm.cometbft.apphash.finality.receipt.v2",
            &[&self.unsigned_bytes()?],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trust_anchor_fixture() -> CometBftTrustAnchorV1 {
        let mut anchor = CometBftTrustAnchorV1 {
            schema: COMETBFT_TRUST_ANCHOR_SCHEMA_V1.to_string(),
            trusted_header: header(
                41,
                1,
                Some(hex::encode([0; 32])),
                Some(hex::encode([2; 32])),
                3,
                Some(hex::encode([4; 32])),
            ),
            trusted_header_time_rfc3339: "2026-08-06T16:41:48.582677787Z".to_string(),
            trusted_next_validator_set_proto_hex: "0a0101".to_string(),
            trust_threshold_numerator: 2,
            trust_threshold_denominator: 3,
            trusting_period_seconds: 86_400,
            clock_drift_seconds: 10,
            anchor_hash_hex: String::new(),
        };
        anchor.anchor_hash_hex = hex::encode(anchor.compute_anchor_hash().unwrap());
        anchor
    }

    fn single_leaf_proof(value: &[u8]) -> CometBftMerkleInclusionProofV1 {
        CometBftMerkleInclusionProofV1 {
            schema: COMETBFT_MERKLE_INCLUSION_PROOF_SCHEMA_V1.to_string(),
            leaf_index: 0,
            leaf_count: 1,
            leaf_value_hex: hex::encode(value),
            leaf_hash_hex: hex::encode(comet_leaf_hash(value)),
            aunts_hex: Vec::new(),
        }
    }

    fn header(
        height: u64,
        header_byte: u8,
        last_block_id_hash_hex: Option<String>,
        data_hash_hex: Option<String>,
        app_hash_byte: u8,
        last_results_hash_hex: Option<String>,
    ) -> CometBftHeaderV1 {
        CometBftHeaderV1 {
            schema: COMETBFT_HEADER_SCHEMA_V1.to_string(),
            chain_id: "trnm-test-1".to_string(),
            height,
            header_hash_hex: hex::encode([header_byte; 32]),
            last_block_id_hash_hex,
            data_hash_hex,
            app_hash_hex: hex::encode([app_hash_byte; 32]),
            last_results_hash_hex,
            validators_hash_hex: hex::encode([7; 32]),
            next_validators_hash_hex: hex::encode([8; 32]),
            header_proto_hex: hex::encode([header_byte, height as u8]),
        }
    }

    fn fixture() -> CometBftAppHashFinalityReceiptV2 {
        let raw_tx = b"canonical-research-tx";
        let tx_hash = comet_tx_hash(raw_tx);
        let result = b"deterministic-exec-result-with-canonical-event";
        // CometBFT's block DataHash tree commits the SHA-256 transaction
        // hashes as its leaf values, while the receipt separately carries the
        // exact raw transaction bytes bound by `comet_tx_hash_hex`.
        let tx_proof = single_leaf_proof(&tx_hash);
        let result_proof = single_leaf_proof(result);
        let execution_header = header(
            41,
            1,
            Some(hex::encode([0; 32])),
            Some(hex::encode(tx_proof.root_hash().unwrap())),
            3,
            Some(hex::encode([2; 32])),
        );
        let commitment_header = header(
            42,
            2,
            Some(execution_header.header_hash_hex.clone()),
            None,
            4,
            Some(hex::encode(result_proof.root_hash().unwrap())),
        );
        let commitment_proof_hex = "0a0101".to_string();
        let object_key_hex = hex::encode(b"trnm-state-object-key");
        let mut receipt = CometBftAppHashFinalityReceiptV2 {
            schema: COMETBFT_APPHASH_FINALITY_RECEIPT_SCHEMA_V2.to_string(),
            chain_id: "trnm-test-1".to_string(),
            command_id: "research-command-41".to_string(),
            command_fingerprint_hex: hex::encode([9; 32]),
            execution_height: 41,
            commitment_height: 42,
            comet_tx_hash_hex: hex::encode(tx_hash),
            raw_tx_hex: hex::encode(raw_tx),
            execution_header,
            commitment_light_proof: CometBftLightFinalityProofV1 {
                schema: COMETBFT_LIGHT_FINALITY_PROOF_SCHEMA_V1.to_string(),
                commit_height: 42,
                commit_block_id_hash_hex: commitment_header.header_hash_hex.clone(),
                signed_header_proto_hex: "0a0102".to_string(),
                validator_set_proto_hex: "0a0103".to_string(),
                header: commitment_header,
            },
            transaction_inclusion_proof: tx_proof,
            canonical_result_bytes_hex: hex::encode(result),
            result_inclusion_proof: result_proof,
            applied_command_object_proof: AppHashObjectProofV1 {
                schema: APPHASH_OBJECT_PROOF_SCHEMA_V1.to_string(),
                query_height: 41,
                object_key_hex: object_key_hex.clone(),
                value_hex: hex::encode(b"applied-command-record"),
                proof_op: AppHashProofOpV1 {
                    proof_type: COMETBFT_JMT_PROOF_OP_TYPE_V1.to_string(),
                    key_hex: object_key_hex,
                    data_hex: commitment_proof_hex.clone(),
                },
                commitment_proof_hex,
            },
            receipt_hash_hex: String::new(),
        };
        receipt.receipt_hash_hex = hex::encode(receipt.compute_receipt_hash().unwrap());
        receipt
    }

    #[test]
    fn receipt_v2_golden_hash_binds_cross_height_evidence() {
        let receipt = fixture();
        receipt.validate_shape().unwrap();
        assert_eq!(
            receipt.receipt_hash_hex,
            "c4e9c3b32576dcc9dfc65edd1e517fa9569cb58993f6010b7a94a71008185559"
        );
    }

    #[test]
    fn trust_anchor_v1_is_strict_canonical_and_domain_hashed() {
        let anchor = trust_anchor_fixture();
        let bytes = anchor.canonical_bytes().unwrap();
        assert_eq!(
            anchor.anchor_hash_hex,
            "56fa393718d93fcd19590ccf7d69c80869f120e9b392b6f6c46d330e8d5e3770"
        );
        assert_eq!(bytes.len(), 1_090);
        assert_eq!(
            hex::encode(Sha256::digest(&bytes)),
            "56d597038dc390ee789094b39233f21b7e9324a86540a5680ef207e95f659408"
        );
        assert_eq!(
            CometBftTrustAnchorV1::from_canonical_bytes(&bytes).unwrap(),
            anchor
        );

        let mut whitespace = vec![b' '];
        whitespace.extend_from_slice(&bytes);
        assert!(CometBftTrustAnchorV1::from_canonical_bytes(&whitespace).is_err());

        let json = String::from_utf8(bytes).unwrap();
        let schema_field = format!(
            "\"schema\":{},",
            serde_json::to_string(&anchor.schema).unwrap()
        );
        let header_field = format!(
            "\"trusted_header\":{},",
            serde_json::to_string(&anchor.trusted_header).unwrap()
        );
        let canonical_prefix = format!("{{{schema_field}{header_field}");
        assert!(json.starts_with(&canonical_prefix));
        let reordered = json.replacen(
            &canonical_prefix,
            &format!("{{{header_field}{schema_field}"),
            1,
        );
        assert!(CometBftTrustAnchorV1::from_canonical_bytes(reordered.as_bytes()).is_err());
        let duplicate = json.replacen('{', &format!("{{{schema_field}"), 1);
        assert!(CometBftTrustAnchorV1::from_canonical_bytes(duplicate.as_bytes()).is_err());
        let unknown = json.replacen('{', "{\"unknown\":true,", 1);
        assert!(CometBftTrustAnchorV1::from_canonical_bytes(unknown.as_bytes()).is_err());
    }

    #[test]
    fn trust_anchor_v1_rejects_ambiguous_policy_and_proto_encodings() {
        let mut anchor = trust_anchor_fixture();
        anchor.trust_threshold_numerator = 2;
        anchor.trust_threshold_denominator = 6;
        assert!(anchor.validate_shape().is_err());

        let mut anchor = trust_anchor_fixture();
        anchor.trust_threshold_numerator = 1;
        anchor.trust_threshold_denominator = 4;
        assert!(anchor.validate_shape().is_err());

        let mut anchor = trust_anchor_fixture();
        anchor.trusting_period_seconds = 0;
        assert!(anchor.validate_shape().is_err());

        let mut anchor = trust_anchor_fixture();
        anchor.clock_drift_seconds = anchor.trusting_period_seconds;
        assert!(anchor.validate_shape().is_err());

        let mut anchor = trust_anchor_fixture();
        anchor.trusted_next_validator_set_proto_hex = "0A0101".to_string();
        assert!(anchor.validate_shape().is_err());

        let mut anchor = trust_anchor_fixture();
        anchor.anchor_hash_hex = hex::encode([0; 32]);
        assert!(anchor.validate_shape().is_err());
    }

    #[test]
    fn receipt_v2_rejects_wrong_commitment_height_and_link() {
        let mut receipt = fixture();
        receipt.commitment_height = 43;
        assert!(receipt.validate_shape().is_err());

        let mut receipt = fixture();
        receipt.commitment_light_proof.header.last_block_id_hash_hex = Some(hex::encode([5; 32]));
        assert!(receipt.validate_shape().is_err());
    }

    #[test]
    fn receipt_v2_rejects_raw_tx_and_result_root_drift() {
        let mut receipt = fixture();
        receipt.raw_tx_hex = hex::encode(b"different-tx");
        assert!(receipt.validate_shape().is_err());

        let mut receipt = fixture();
        receipt.commitment_light_proof.header.last_results_hash_hex = Some(hex::encode([6; 32]));
        assert!(receipt.validate_shape().is_err());
    }

    #[test]
    fn object_proof_rejects_abci_and_ics23_byte_drift() {
        let mut receipt = fixture();
        receipt.applied_command_object_proof.proof_op.data_hex = "0a0102".to_string();
        assert!(receipt.validate_shape().is_err());

        let mut receipt = fixture();
        receipt.applied_command_object_proof.proof_op.key_hex = hex::encode(b"other-key");
        assert!(receipt.validate_shape().is_err());
    }

    #[test]
    fn receipt_helper_binds_logical_key_and_authenticated_record_fields() {
        let logical_key = "0fc3a6daebb13c878397ce926ba5084d9d9451202ea2b49fde828cad849d12a4";
        let proof_key = authenticated_object_proof_key_v4(logical_key).unwrap();
        let inner_value = b"canonical-applied-command";
        let record = AuthenticatedObjectRecordV1::new(
            "trnm.research.applied-command.v1",
            1,
            inner_value.to_vec(),
        )
        .unwrap();
        let mut receipt = fixture();
        let proof_key_hex = hex::encode(&proof_key);
        receipt.applied_command_object_proof.object_key_hex = proof_key_hex.clone();
        receipt.applied_command_object_proof.proof_op.key_hex = proof_key_hex;
        receipt.applied_command_object_proof.value_hex = hex::encode(record.encode().unwrap());
        receipt.receipt_hash_hex = hex::encode(receipt.compute_receipt_hash().unwrap());

        assert_eq!(
            receipt
                .verify_applied_command_object_binding(
                    logical_key,
                    "trnm.research.applied-command.v1",
                    1,
                    inner_value,
                )
                .unwrap(),
            record
        );
        assert!(receipt
            .decode_applied_command_object_record(&"11".repeat(32))
            .is_err());
        assert!(receipt
            .verify_applied_command_object_binding(logical_key, "wrong", 1, inner_value)
            .is_err());
        assert!(receipt
            .verify_applied_command_object_binding(
                logical_key,
                "trnm.research.applied-command.v1",
                2,
                inner_value,
            )
            .is_err());
        assert!(receipt
            .verify_applied_command_object_binding(
                logical_key,
                "trnm.research.applied-command.v1",
                1,
                b"other",
            )
            .is_err());
    }

    #[test]
    fn comet_merkle_proof_rejects_wrong_aunt_count_and_noncanonical_hex() {
        let left = b"left";
        let right = b"right";
        let proof = CometBftMerkleInclusionProofV1 {
            schema: COMETBFT_MERKLE_INCLUSION_PROOF_SCHEMA_V1.to_string(),
            leaf_index: 0,
            leaf_count: 2,
            leaf_value_hex: hex::encode(left),
            leaf_hash_hex: hex::encode(comet_leaf_hash(left)),
            aunts_hex: vec![hex::encode(comet_leaf_hash(right))],
        };
        assert_eq!(
            proof.root_hash().unwrap(),
            comet_inner_hash(&comet_leaf_hash(left), &comet_leaf_hash(right))
        );

        let mut missing_aunt = proof.clone();
        missing_aunt.aunts_hex.clear();
        assert!(missing_aunt.validate_shape().is_err());

        let mut uppercase = proof;
        uppercase.leaf_value_hex = "4C656674".to_string();
        assert!(uppercase.validate_shape().is_err());
    }

    #[test]
    fn comet_merkle_builder_covers_three_and_five_leaf_trees() {
        for leaf_count in [3usize, 5] {
            let leaves = (0..leaf_count)
                .map(|index| format!("leaf-{leaf_count}-{index}").into_bytes())
                .collect::<Vec<_>>();
            let proofs = (0..leaf_count)
                .map(|index| {
                    CometBftMerkleInclusionProofV1::from_leaf_values(&leaves, index).unwrap()
                })
                .collect::<Vec<_>>();
            let expected_root = proofs[0].root_hash().unwrap();
            for (index, proof) in proofs.iter().enumerate() {
                assert_eq!(proof.leaf_index, index as u64);
                assert_eq!(proof.leaf_count, leaf_count as u64);
                assert_eq!(proof.root_hash().unwrap(), expected_root);
            }

            let mut forged = proofs[leaf_count / 2].clone();
            forged.leaf_value_hex = hex::encode(b"forged-leaf");
            assert!(forged.validate_shape().is_err());
        }
    }

    #[test]
    fn receipt_v2_rejects_unknown_fields_and_receipt_hash_drift() {
        let receipt = fixture();
        let mut json = serde_json::to_value(&receipt).unwrap();
        json.as_object_mut().unwrap().insert(
            "legacy_quorum_certificate".to_string(),
            serde_json::json!({}),
        );
        assert!(serde_json::from_value::<CometBftAppHashFinalityReceiptV2>(json).is_err());

        let mut receipt = receipt;
        receipt.receipt_hash_hex = hex::encode([0; 32]);
        assert!(receipt.validate_shape().is_err());

        let mut receipt = fixture();
        receipt.command_fingerprint_hex = hex::encode([8; 32]);
        assert!(receipt.validate_shape().is_err());
    }

    #[test]
    fn receipt_v2_canonical_decoder_is_exact_and_prebounded() {
        let receipt = fixture();
        let bytes = receipt.canonical_bytes().unwrap();
        assert_eq!(
            CometBftAppHashFinalityReceiptV2::from_canonical_bytes(&bytes).unwrap(),
            receipt
        );

        let mut padded = vec![b' '];
        padded.extend_from_slice(&bytes);
        assert!(CometBftAppHashFinalityReceiptV2::from_canonical_bytes(&padded).is_err());

        let json = String::from_utf8(bytes).unwrap();
        let schema_field = format!(
            "\"schema\":{},",
            serde_json::to_string(&receipt.schema).unwrap()
        );
        let chain_field = format!(
            "\"chain_id\":{},",
            serde_json::to_string(&receipt.chain_id).unwrap()
        );
        let canonical_prefix = format!("{{{schema_field}{chain_field}");
        assert!(json.starts_with(&canonical_prefix));
        let reordered = json.replacen(
            &canonical_prefix,
            &format!("{{{chain_field}{schema_field}"),
            1,
        );
        assert!(
            CometBftAppHashFinalityReceiptV2::from_canonical_bytes(reordered.as_bytes()).is_err()
        );
        let unknown = json.replacen('{', "{\"unexpected\":true,", 1);
        assert!(
            CometBftAppHashFinalityReceiptV2::from_canonical_bytes(unknown.as_bytes()).is_err()
        );
        let duplicate = json.replacen('{', &format!("{{{schema_field}"), 1);
        assert!(
            CometBftAppHashFinalityReceiptV2::from_canonical_bytes(duplicate.as_bytes()).is_err()
        );
        assert!(ensure_receipt_v2_wire_len(MAX_COMETBFT_RECEIPT_V2_WIRE_BYTES + 1).is_err());
    }

    #[test]
    fn default_exec_result_may_have_an_empty_canonical_encoding() {
        let proof = single_leaf_proof(b"");
        proof.validate_shape().unwrap();
        assert_eq!(proof.root_hash().unwrap(), comet_leaf_hash(b""));
    }
}
