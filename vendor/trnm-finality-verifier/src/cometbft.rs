//! Fail-closed verification for canonical CometBFT/AppHash receipts.
//!
//! V2 does not alter or reinterpret the frozen synthetic V1 verifier.  A V2
//! transaction executes at height `H`; the resulting AppHash and transaction
//! results root are committed by the finalized header at `H + 1`.
//! The included transaction must be the exact outer signed-command envelope
//! committed in block `H`. Its payload must be the canonical typed Research
//! transaction, and both signature layers plus every shared security field are
//! rebound to the execution header and receipt.
//!
//! The AppHash-v4 object-key derivation and authenticated object wrapper are
//! shared wire contracts.  This verifier therefore binds the proven object to
//! the exact logical applied-command key, wrapper type/version, and canonical
//! applied-command value without relying on a caller-supplied decoder.

use std::{
    collections::BTreeSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, ensure, Context, Result};
use ics23::{commitment_proof, CommitmentProof, HashOp, InnerSpec, LeafOp, LengthOp, ProofSpec};
use prost::Message;
use tendermint::{block, validator};
use tendermint_light_client_verifier::{
    errors::VerificationErrorDetail,
    options::Options,
    types::{Time, TrustThreshold, TrustedBlockState, UntrustedBlockState},
    ProdVerifier, Verdict, Verifier,
};
use tendermint_proto::v0_38::{
    abci::ExecTxResult as RawExecTxResult,
    types::{
        Header as RawHeader, SignedHeader as RawSignedHeader, ValidatorSet as RawValidatorSet,
    },
};
use trnm_finality_types::{
    comet_tx_hash, AppHashObjectProofV1, CometBftAppHashFinalityReceiptV2, CometBftHeaderV1,
    CometBftLightFinalityProofV1, CometBftMerkleInclusionProofV1, CometBftTrustAnchorV1,
    SignedCommandEnvelopeV1, COMETBFT_APPHASH_FINALITY_RECEIPT_SCHEMA_V2,
    COMETBFT_LIGHT_FINALITY_PROOF_SCHEMA_V1, COMETBFT_TRUST_ANCHOR_SCHEMA_V1,
};
use trnm_protocol::{
    research_applied_command_key, CanonicalResearchTxV1, CANONICAL_RESEARCH_TX_PAYLOAD_TYPE_V1,
    RESEARCH_APPLIED_COMMAND_OBJECT_TYPE_V1,
};
use trnm_research_protocol::{
    AppliedCommandRecordV1, AuthorityRole, CanonicalCbor, SignedResearchCommandV1,
};

const JMT_LEAF_DOMAIN_SEPARATOR: &[u8] = b"JMT::LeafNode";
const JMT_INTERNAL_DOMAIN_SEPARATOR: &[u8] = b"JMT::IntrnalNode";
const JMT_SPARSE_PLACEHOLDER_HASH: &[u8; 32] = b"SPARSE_MERKLE_PLACEHOLDER_HASH__";

/// An externally persisted, already-verified light-client root plus policy.
///
/// The caller remains responsible for loading this state from an authenticated
/// light store.  This verifier checks that its validator set still matches the
/// stored hash before using it.
pub struct CometBftTrustContext<'a> {
    pub trusted_state: TrustedBlockState<'a>,
    pub options: &'a Options,
    pub now: Time,
}

/// One fully decoded and validated trust root suitable for repeated Receipt V2
/// verification.
///
/// All Tendermint implementation types remain owned and private. Consumers can
/// persist or vendor the stable Serde wire returned by [`Self::wire`] without
/// constructing borrowed light-client state themselves.
#[derive(Debug, Clone)]
pub struct ValidatedCometBftTrustAnchorV1 {
    wire: CometBftTrustAnchorV1,
    trusted_header: block::Header,
    trusted_next_validators: validator::Set,
    options: Options,
}

impl ValidatedCometBftTrustAnchorV1 {
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self> {
        CometBftTrustAnchorV1::from_canonical_bytes(bytes)?.try_into()
    }

    pub fn wire(&self) -> &CometBftTrustAnchorV1 {
        &self.wire
    }

    fn context(&self, now: Time) -> CometBftTrustContext<'_> {
        CometBftTrustContext {
            trusted_state: TrustedBlockState {
                chain_id: &self.trusted_header.chain_id,
                header_time: self.trusted_header.time,
                height: self.trusted_header.height,
                next_validators: &self.trusted_next_validators,
                next_validators_hash: self.trusted_header.next_validators_hash,
            },
            options: &self.options,
            now,
        }
    }
}

impl TryFrom<CometBftTrustAnchorV1> for ValidatedCometBftTrustAnchorV1 {
    type Error = anyhow::Error;

    fn try_from(wire: CometBftTrustAnchorV1) -> Result<Self> {
        wire.validate_shape()?;
        let trusted_header =
            decode_header(&wire.trusted_header).context("decode trusted CometBFT header")?;
        ensure!(
            trusted_header.time.to_rfc3339() == wire.trusted_header_time_rfc3339,
            "trusted header time does not match canonical header protobuf"
        );

        let validator_bytes = wire.trusted_next_validator_set_proto_bytes()?;
        let raw_validators = decode_canonical_message::<RawValidatorSet>(
            "trusted_next_validator_set_proto_hex",
            &validator_bytes,
        )?;
        validate_canonical_trust_validator_set(&raw_validators)?;
        let trusted_next_validators: validator::Set = raw_validators
            .clone()
            .try_into()
            .context("decode trusted next validator set")?;
        ensure!(
            trusted_next_validators.proposer().is_none(),
            "trusted next validator set must not carry a proposer"
        );
        ensure!(
            trusted_next_validators
                .validators()
                .iter()
                .all(|validator| validator.proposer_priority.value() == 0),
            "trusted next validator proposer priorities must be zero"
        );
        let normalized: RawValidatorSet = trusted_next_validators.clone().into();
        ensure!(
            normalized.encode_to_vec() == validator_bytes,
            "trusted next validator set is not in normalized canonical order"
        );
        ensure!(
            trusted_next_validators.hash() == trusted_header.next_validators_hash,
            "trusted next validator set does not match trusted header next_validators_hash"
        );

        let trust_threshold = TrustThreshold::new(
            u64::from(wire.trust_threshold_numerator),
            u64::from(wire.trust_threshold_denominator),
        )
        .context("validate CometBFT trust threshold")?;
        let trusting_period = Duration::from_secs(wire.trusting_period_seconds);
        let clock_drift = Duration::from_secs(wire.clock_drift_seconds);
        ensure!(
            trusted_header.time.checked_add(trusting_period).is_some(),
            "trusted header time plus trusting period is out of range"
        );

        Ok(Self {
            wire,
            trusted_header,
            trusted_next_validators,
            options: Options {
                trust_threshold,
                trusting_period,
                clock_drift,
            },
        })
    }
}

fn validate_canonical_trust_validator_set(raw: &RawValidatorSet) -> Result<()> {
    ensure!(
        raw.proposer.is_none(),
        "trusted next validator set must not carry a proposer"
    );
    ensure!(
        !raw.validators.is_empty(),
        "trusted next validator set must not be empty"
    );
    ensure!(
        raw.total_voting_power > 0,
        "trusted next validator total voting power must be positive"
    );

    let mut addresses = BTreeSet::new();
    let mut public_keys = BTreeSet::new();
    for raw_validator in &raw.validators {
        ensure!(
            raw_validator.voting_power > 0,
            "trusted validator voting power must be positive"
        );
        ensure!(
            raw_validator.proposer_priority == 0,
            "trusted validator proposer priority must be zero"
        );
        ensure!(
            addresses.insert(raw_validator.address.clone()),
            "trusted validator addresses must be unique"
        );
        let public_key = raw_validator
            .pub_key
            .as_ref()
            .context("trusted validator public key is absent")?;
        ensure!(
            public_keys.insert(public_key.encode_to_vec()),
            "trusted validator public keys must be unique"
        );
    }
    Ok(())
}

/// Encode an authenticated Chain checkpoint into the stable cross-repository
/// trust-anchor wire. The validator set is normalized without proposer state or
/// proposer priorities, neither of which contributes to the validator hash.
pub fn encode_cometbft_trust_anchor_v1(
    trusted_header: &block::Header,
    trusted_next_validators: &validator::Set,
    trust_threshold_numerator: u32,
    trust_threshold_denominator: u32,
    trusting_period: Duration,
    clock_drift: Duration,
) -> Result<CometBftTrustAnchorV1> {
    let mut validator_infos = trusted_next_validators.validators().clone();
    for validator in &mut validator_infos {
        validator.proposer_priority = 0i64.into();
    }
    let normalized_validators = validator::Set::without_proposer(validator_infos);
    ensure!(
        normalized_validators.hash() == trusted_header.next_validators_hash,
        "trusted next validator set does not match trusted header"
    );
    let raw_validators: RawValidatorSet = normalized_validators.into();

    let mut wire = CometBftTrustAnchorV1 {
        schema: COMETBFT_TRUST_ANCHOR_SCHEMA_V1.to_string(),
        trusted_header: encode_cometbft_header_v1(trusted_header)?,
        trusted_header_time_rfc3339: trusted_header.time.to_rfc3339(),
        trusted_next_validator_set_proto_hex: hex::encode(raw_validators.encode_to_vec()),
        trust_threshold_numerator,
        trust_threshold_denominator,
        trusting_period_seconds: trusting_period.as_secs(),
        clock_drift_seconds: clock_drift.as_secs(),
        anchor_hash_hex: String::new(),
    };
    ensure!(
        trusting_period.subsec_nanos() == 0 && clock_drift.subsec_nanos() == 0,
        "trusting period and clock drift must use whole seconds"
    );
    wire.anchor_hash_hex = hex::encode(wire.compute_anchor_hash()?);
    let _ = ValidatedCometBftTrustAnchorV1::try_from(wire.clone())?;
    Ok(wire)
}

struct ResearchTransactionBinding {
    chain_id: String,
    command_id: String,
    command_fingerprint_hex: String,
    applied_command_logical_key: String,
    applied_command_value: Vec<u8>,
    expected_gas_wanted: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCometBftReceiptV2 {
    pub receipt_hash_hex: String,
    pub chain_id: String,
    pub command_id: String,
    pub command_fingerprint_hex: String,
    pub comet_tx_hash_hex: String,
    pub transaction_index: u64,
    pub applied_command_object_key_hex: String,
    pub execution_height: u64,
    pub commitment_height: u64,
    pub commitment_header_hash_hex: String,
    pub app_hash_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "receipt verification outcomes must be checked"]
pub enum ReceiptV2VerificationOutcome {
    Final(VerifiedCometBftReceiptV2),
    StructuralInvalid { reason: String },
    Untrusted { reason: String },
    NotFinal { reason: String },
}

impl ReceiptV2VerificationOutcome {
    fn structural(error: impl core::fmt::Display) -> Self {
        Self::StructuralInvalid {
            reason: error.to_string(),
        }
    }

    fn untrusted(error: impl core::fmt::Display) -> Self {
        Self::Untrusted {
            reason: error.to_string(),
        }
    }

    fn not_final(error: impl core::fmt::Display) -> Self {
        Self::NotFinal {
            reason: error.to_string(),
        }
    }
}

struct DecodedReceiptEvidence {
    execution_timestamp_ms: u64,
    commitment_signed_header: block::signed_header::SignedHeader,
    commitment_validators: validator::Set,
}

/// Canonical, transport-neutral inputs collected from CometBFT RPC and one
/// proven ABCI object query. The caller must preserve block transaction and
/// deterministic result ordering exactly as returned for height `H`.
#[derive(Debug, Clone)]
pub struct CometBftReceiptAssemblyInputV2 {
    pub target_command_id: String,
    pub execution_header: CometBftHeaderV1,
    pub commitment_header: CometBftHeaderV1,
    pub commitment_signed_header_proto: Vec<u8>,
    pub commitment_validator_set_proto: Vec<u8>,
    pub raw_transactions: Vec<Vec<u8>>,
    pub canonical_results: Vec<Vec<u8>>,
    pub applied_command_object_proof: AppHashObjectProofV1,
}

/// Assemble a Receipt V2 without trusting caller-supplied roots, indices, or
/// Research metadata. All roots are rebuilt from ordered raw block/result
/// bytes, and the supplied JMT proof is cryptographically checked against the
/// H + 1 AppHash before the receipt hash is emitted.
pub fn assemble_cometbft_apphash_finality_receipt_v2(
    input: CometBftReceiptAssemblyInputV2,
) -> Result<CometBftAppHashFinalityReceiptV2> {
    ensure!(
        !input.raw_transactions.is_empty(),
        "execution block does not contain transactions"
    );
    ensure!(
        input.raw_transactions.len() == input.canonical_results.len(),
        "block transaction and deterministic result counts differ"
    );

    let execution_header =
        decode_header(&input.execution_header).context("decode assembler execution header H")?;
    let commitment_header = decode_header(&input.commitment_header)
        .context("decode assembler commitment header H + 1")?;
    let execution_timestamp_ms = consensus_timestamp_ms(&execution_header.time)?;
    ensure!(
        commitment_header.height.value() == execution_header.height.value().saturating_add(1),
        "assembler commitment height must equal execution height + 1"
    );
    ensure!(
        commitment_header.last_block_id.map(|id| id.hash) == Some(execution_header.hash()),
        "assembler H + 1 header does not link to H"
    );

    let mut target_index = None;
    for (index, raw_tx) in input.raw_transactions.iter().enumerate() {
        let envelope = SignedCommandEnvelopeV1::from_canonical_wire_bytes(raw_tx)
            .with_context(|| format!("decode block transaction at index {index}"))?;
        if envelope.command_id == input.target_command_id {
            ensure!(
                target_index.is_none(),
                "target command appears more than once"
            );
            target_index = Some(index);
        }
    }
    let target_index = target_index.context("target command is absent from execution block")?;
    let raw_tx = &input.raw_transactions[target_index];
    let binding = decode_research_transaction_binding(raw_tx, execution_timestamp_ms)
        .context("decode target Research transaction")?;
    ensure!(
        binding.command_id == input.target_command_id,
        "decoded target command ID drift"
    );
    ensure!(
        binding.chain_id == execution_header.chain_id.as_str()
            && binding.chain_id == commitment_header.chain_id.as_str(),
        "target Research chain ID does not match H/H + 1 headers"
    );

    for (index, result) in input.canonical_results.iter().enumerate() {
        decode_canonical_message::<RawExecTxResult>(
            &format!("canonical result at transaction index {index}"),
            result,
        )?;
    }
    verify_deterministic_exec_result_bytes(&input.canonical_results[target_index], &binding)
        .context("validate target deterministic execution result")?;

    let transaction_hashes = input
        .raw_transactions
        .iter()
        .map(|transaction| comet_tx_hash(transaction))
        .collect::<Vec<_>>();
    let transaction_inclusion_proof =
        CometBftMerkleInclusionProofV1::from_leaf_values(&transaction_hashes, target_index)?;
    let result_inclusion_proof =
        CometBftMerkleInclusionProofV1::from_leaf_values(&input.canonical_results, target_index)?;

    let mut receipt = CometBftAppHashFinalityReceiptV2 {
        schema: COMETBFT_APPHASH_FINALITY_RECEIPT_SCHEMA_V2.to_string(),
        chain_id: binding.chain_id.clone(),
        command_id: binding.command_id.clone(),
        command_fingerprint_hex: binding.command_fingerprint_hex.clone(),
        execution_height: execution_header.height.value(),
        commitment_height: commitment_header.height.value(),
        comet_tx_hash_hex: hex::encode(comet_tx_hash(raw_tx)),
        raw_tx_hex: hex::encode(raw_tx),
        execution_header: input.execution_header,
        commitment_light_proof: CometBftLightFinalityProofV1 {
            schema: COMETBFT_LIGHT_FINALITY_PROOF_SCHEMA_V1.to_string(),
            header: input.commitment_header,
            commit_height: commitment_header.height.value(),
            commit_block_id_hash_hex: hex::encode(commitment_header.hash().as_bytes()),
            signed_header_proto_hex: hex::encode(input.commitment_signed_header_proto),
            validator_set_proto_hex: hex::encode(input.commitment_validator_set_proto),
        },
        transaction_inclusion_proof,
        canonical_result_bytes_hex: hex::encode(&input.canonical_results[target_index]),
        result_inclusion_proof,
        applied_command_object_proof: input.applied_command_object_proof,
        receipt_hash_hex: String::new(),
    };
    receipt.receipt_hash_hex = hex::encode(receipt.compute_receipt_hash()?);
    receipt.validate_shape()?;

    let evidence =
        decode_receipt_evidence(&receipt).context("validate assembled header evidence")?;
    verify_deterministic_exec_result(&receipt, &binding)?;
    verify_applied_command_membership(
        &receipt.applied_command_object_proof,
        evidence.commitment_signed_header.header.app_hash.as_bytes(),
    )?;
    verify_applied_command_binding(&receipt, &binding)?;
    Ok(receipt)
}

/// Verify all evidence that can independently establish a V2 finality receipt.
///
/// The output distinguishes malformed/forged evidence, an unusable trust root,
/// and a well-formed header that has not yet met finality/trust thresholds.
pub fn verify_cometbft_apphash_finality_receipt_v2(
    receipt: &CometBftAppHashFinalityReceiptV2,
    trust: CometBftTrustContext<'_>,
) -> ReceiptV2VerificationOutcome {
    if let Err(error) = receipt.validate_shape() {
        return ReceiptV2VerificationOutcome::structural(error);
    }
    if receipt.transaction_inclusion_proof.leaf_count != receipt.result_inclusion_proof.leaf_count {
        return ReceiptV2VerificationOutcome::structural(
            "transaction and result proof leaf counts differ",
        );
    }
    let raw_tx = match receipt.raw_tx_bytes() {
        Ok(raw_tx) => raw_tx,
        Err(error) => return ReceiptV2VerificationOutcome::structural(error),
    };
    let evidence = match decode_receipt_evidence(receipt) {
        Ok(evidence) => evidence,
        Err(error) => return ReceiptV2VerificationOutcome::structural(error),
    };
    let transaction_binding = match verify_research_transaction_binding(
        &raw_tx,
        &receipt.chain_id,
        &receipt.command_id,
        &receipt.command_fingerprint_hex,
        evidence.execution_timestamp_ms,
    ) {
        Ok(binding) => binding,
        Err(error) => return ReceiptV2VerificationOutcome::structural(error),
    };

    if trust.trusted_state.next_validators.hash() != trust.trusted_state.next_validators_hash {
        return ReceiptV2VerificationOutcome::untrusted(
            "trusted next validator set does not match its authenticated hash",
        );
    }

    let untrusted = UntrustedBlockState {
        signed_header: &evidence.commitment_signed_header,
        validators: &evidence.commitment_validators,
        // Receipt V2 intentionally does not carry H + 2 validators.  They are
        // unnecessary for finalizing H/H + 1 and must be fetched before H + 1
        // can itself be promoted to a reusable trusted state.
        next_validators: None,
    };
    let verdict = ProdVerifier::default().verify_update_header(
        untrusted,
        trust.trusted_state,
        trust.options,
        trust.now,
    );
    if let Some(outcome) = classify_light_verdict(verdict) {
        return outcome;
    }

    if let Err(error) = verify_deterministic_exec_result(receipt, &transaction_binding) {
        return ReceiptV2VerificationOutcome::structural(error);
    }

    if let Err(error) = verify_applied_command_membership(
        &receipt.applied_command_object_proof,
        evidence.commitment_signed_header.header.app_hash.as_bytes(),
    ) {
        return ReceiptV2VerificationOutcome::structural(error);
    }
    if let Err(error) = verify_applied_command_binding(receipt, &transaction_binding) {
        return ReceiptV2VerificationOutcome::structural(format!(
            "applied-command binding failed: {error:#}"
        ));
    }

    ReceiptV2VerificationOutcome::Final(VerifiedCometBftReceiptV2 {
        receipt_hash_hex: receipt.receipt_hash_hex.clone(),
        chain_id: receipt.chain_id.clone(),
        command_id: receipt.command_id.clone(),
        command_fingerprint_hex: receipt.command_fingerprint_hex.clone(),
        comet_tx_hash_hex: receipt.comet_tx_hash_hex.clone(),
        transaction_index: receipt.transaction_inclusion_proof.leaf_index,
        applied_command_object_key_hex: receipt.applied_command_object_proof.object_key_hex.clone(),
        execution_height: receipt.execution_height,
        commitment_height: receipt.commitment_height,
        commitment_header_hash_hex: receipt
            .commitment_light_proof
            .header
            .header_hash_hex
            .clone(),
        app_hash_hex: receipt.commitment_light_proof.header.app_hash_hex.clone(),
    })
}

/// Verify Receipt V2 using one previously validated, fully owned trust anchor.
///
/// `verification_time` must come from the caller's trusted clock. This helper
/// never reads wall-clock time itself, which keeps replay and cross-repository
/// fixtures deterministic. Trust-anchor, clock, chain, or height failures are
/// classified as `Untrusted`; receipt evidence classifications are delegated
/// unchanged to [`verify_cometbft_apphash_finality_receipt_v2`].
pub fn verify_cometbft_apphash_finality_receipt_v2_with_trust_anchor(
    receipt: &CometBftAppHashFinalityReceiptV2,
    anchor: &ValidatedCometBftTrustAnchorV1,
    verification_time: SystemTime,
) -> ReceiptV2VerificationOutcome {
    let now = match system_time_to_tendermint_time(verification_time) {
        Ok(now) => now,
        Err(error) => return ReceiptV2VerificationOutcome::untrusted(error),
    };
    if now.checked_add(anchor.options.clock_drift).is_none() {
        return ReceiptV2VerificationOutcome::untrusted(
            "verification time plus clock drift is out of range",
        );
    }
    if receipt.chain_id != anchor.trusted_header.chain_id.as_str() {
        return ReceiptV2VerificationOutcome::untrusted(
            "receipt chain does not match the authenticated trust anchor",
        );
    }
    if anchor.trusted_header.height.value() >= receipt.commitment_height {
        return ReceiptV2VerificationOutcome::untrusted(
            "trust anchor height must precede the receipt commitment height",
        );
    }
    verify_cometbft_apphash_finality_receipt_v2(receipt, anchor.context(now))
}

fn system_time_to_tendermint_time(value: SystemTime) -> Result<Time> {
    let since_epoch = value
        .duration_since(UNIX_EPOCH)
        .map_err(|_| anyhow!("verification time predates the Unix epoch"))?;
    let seconds = i64::try_from(since_epoch.as_secs())
        .context("verification time exceeds the supported range")?;
    Time::from_unix_timestamp(seconds, since_epoch.subsec_nanos())
        .context("verification time is outside the CometBFT range")
}

fn verify_research_transaction_binding(
    raw_tx: &[u8],
    expected_chain_id: &str,
    expected_command_id: &str,
    expected_command_fingerprint_hex: &str,
    execution_timestamp_ms: u64,
) -> Result<ResearchTransactionBinding> {
    let binding = decode_research_transaction_binding(raw_tx, execution_timestamp_ms)?;
    ensure!(
        binding.chain_id == expected_chain_id,
        "outer and inner Research chain ID do not match receipt"
    );
    ensure!(
        binding.command_id == expected_command_id,
        "outer and inner Research command ID do not match receipt"
    );
    ensure!(
        binding.command_fingerprint_hex == expected_command_fingerprint_hex,
        "raw transaction command fingerprint does not match receipt"
    );
    Ok(binding)
}

fn decode_research_transaction_binding(
    raw_tx: &[u8],
    execution_timestamp_ms: u64,
) -> Result<ResearchTransactionBinding> {
    let envelope = SignedCommandEnvelopeV1::from_canonical_wire_bytes(raw_tx)
        .context("decode outer signed command envelope")?;
    let envelope_chain_id = envelope.chain_id.clone();
    envelope
        .validate_at(&envelope_chain_id, execution_timestamp_ms)
        .context("validate outer signed command envelope at execution height H")?;
    ensure!(
        envelope.payload_type == CANONICAL_RESEARCH_TX_PAYLOAD_TYPE_V1,
        "outer envelope is not a typed Research transaction"
    );

    let payload = envelope
        .payload_bytes()
        .context("decode outer Research payload")?;
    let tx = CanonicalResearchTxV1::from_canonical_bytes(&payload)
        .context("decode canonical typed Research transaction payload")?;
    ensure!(
        tx.payload_type == envelope.payload_type,
        "inner Research payload type does not match outer envelope"
    );
    ensure!(
        tx.command_id == envelope.command_id,
        "inner Research command ID does not match outer envelope"
    );
    let signed = tx
        .signed_research_command()
        .context("decode signed Research command from raw transaction")?;
    ensure!(
        signed.chain_id == envelope.chain_id,
        "inner Research chain ID does not match outer envelope"
    );
    ensure!(
        signed.command_id.to_hex() == envelope.command_id,
        "signed Research command ID does not match outer envelope"
    );
    ensure!(
        tx.sender == envelope.signer_id && signed.signer_did == envelope.signer_id,
        "Research signer DID does not match outer envelope"
    );
    let expected_role = match signed.signer_role {
        AuthorityRole::NakamaAuthority => "nakama",
        AuthorityRole::HeptaAuthority => "hepta",
    };
    ensure!(
        envelope.signer_role == expected_role,
        "Research signer role does not match outer envelope"
    );
    ensure!(
        envelope.public_key_hex == hex::encode(signed.public_key),
        "Research signer public key does not match outer envelope"
    );
    ensure!(
        tx.nonce == envelope.nonce && signed.nonce == envelope.nonce,
        "Research nonce does not match outer envelope"
    );
    let command_fingerprint_hex = hex::encode(signed.command_fingerprint());
    let applied_command_logical_key = research_applied_command_key(signed.command_id)
        .context("derive applied-command logical object key")?;
    let applied_command_record = expected_applied_command_record(&signed);
    let applied_command_value = applied_command_record.canonical_bytes();
    Ok(ResearchTransactionBinding {
        chain_id: envelope.chain_id,
        command_id: envelope.command_id,
        command_fingerprint_hex,
        applied_command_logical_key,
        applied_command_value,
        expected_gas_wanted: i64::try_from(tx.max_gas).unwrap_or(i64::MAX),
    })
}

fn expected_applied_command_record(signed: &SignedResearchCommandV1) -> AppliedCommandRecordV1 {
    AppliedCommandRecordV1 {
        command_id: signed.command_id,
        fingerprint: signed.command_fingerprint(),
        primary_object_ref: signed.command.primary_object_ref(),
    }
}

fn verify_applied_command_binding(
    receipt: &CometBftAppHashFinalityReceiptV2,
    binding: &ResearchTransactionBinding,
) -> Result<()> {
    receipt
        .verify_applied_command_object_binding(
            &binding.applied_command_logical_key,
            RESEARCH_APPLIED_COMMAND_OBJECT_TYPE_V1,
            1,
            &binding.applied_command_value,
        )
        .context("bind authenticated applied-command object")?;
    Ok(())
}

fn classify_light_verdict(verdict: Verdict) -> Option<ReceiptV2VerificationOutcome> {
    match verdict {
        Verdict::Success => None,
        // A skip-update can have a perfectly valid >2/3 commit yet lack the
        // configured overlap with the caller's trusted validator set.  That is
        // a trust-anchor/path problem, not evidence that the block itself has
        // failed to finalize.
        Verdict::NotEnoughTrust(tally) => Some(ReceiptV2VerificationOutcome::untrusted(format!(
            "trusted validator overlap is insufficient: {tally}"
        ))),
        Verdict::Invalid(detail) => {
            let reason = detail.to_string();
            match detail {
                VerificationErrorDetail::NotWithinTrustPeriod(_)
                | VerificationErrorDetail::ChainIdMismatch(_)
                | VerificationErrorDetail::NonIncreasingHeight(_)
                | VerificationErrorDetail::NonMonotonicBftTime(_)
                | VerificationErrorDetail::NotEnoughTrust(_) => {
                    Some(ReceiptV2VerificationOutcome::untrusted(reason))
                }
                VerificationErrorDetail::HeaderFromTheFuture(_)
                | VerificationErrorDetail::InsufficientSignersOverlap(_)
                | VerificationErrorDetail::NoSignatureForCommit(_) => {
                    Some(ReceiptV2VerificationOutcome::not_final(reason))
                }
                _ => Some(ReceiptV2VerificationOutcome::structural(reason)),
            }
        }
    }
}

fn decode_receipt_evidence(
    receipt: &CometBftAppHashFinalityReceiptV2,
) -> Result<DecodedReceiptEvidence> {
    let execution_header =
        decode_header(&receipt.execution_header).context("decode and bind execution header H")?;
    let execution_timestamp_ms = consensus_timestamp_ms(&execution_header.time)
        .context("convert execution header H timestamp")?;
    let commitment_header = decode_header(&receipt.commitment_light_proof.header)
        .context("decode and bind commitment header H + 1")?;

    let signed_bytes = canonical_hex_bytes(
        "signed_header_proto_hex",
        &receipt.commitment_light_proof.signed_header_proto_hex,
    )?;
    let raw_signed =
        decode_canonical_message::<RawSignedHeader>("signed_header_proto_hex", &signed_bytes)?;
    let commitment_signed_header: block::signed_header::SignedHeader = raw_signed
        .try_into()
        .context("decode CometBFT signed header")?;
    ensure!(
        commitment_signed_header.header == commitment_header,
        "signed header does not contain the exact H + 1 header"
    );

    let validator_bytes = canonical_hex_bytes(
        "validator_set_proto_hex",
        &receipt.commitment_light_proof.validator_set_proto_hex,
    )?;
    let raw_validators =
        decode_canonical_message::<RawValidatorSet>("validator_set_proto_hex", &validator_bytes)?;
    let commitment_validators: validator::Set = raw_validators
        .try_into()
        .context("decode CometBFT validator set")?;
    ensure!(
        commitment_validators.hash() == commitment_header.validators_hash,
        "validator set does not match H + 1 validators_hash"
    );

    ensure!(
        commitment_header.last_block_id.map(|id| id.hash) == Some(execution_header.hash()),
        "decoded H + 1 header does not link to decoded H header"
    );

    Ok(DecodedReceiptEvidence {
        execution_timestamp_ms,
        commitment_signed_header,
        commitment_validators,
    })
}

fn consensus_timestamp_ms(timestamp: &tendermint::Time) -> Result<u64> {
    let nanos = timestamp.unix_timestamp_nanos();
    ensure!(nanos >= 0, "consensus timestamp predates the Unix epoch");
    u64::try_from(nanos / 1_000_000).context("consensus timestamp exceeds u64 milliseconds")
}

fn decode_header(wire: &CometBftHeaderV1) -> Result<block::Header> {
    let bytes = canonical_hex_bytes("header_proto_hex", &wire.header_proto_hex)?;
    let raw = decode_canonical_message::<RawHeader>("header_proto_hex", &bytes)?;
    let header: block::Header = raw.try_into().context("decode CometBFT header")?;

    ensure!(
        header.chain_id.as_str() == wire.chain_id,
        "header chain_id drift"
    );
    ensure!(header.height.value() == wire.height, "header height drift");
    ensure!(
        hex::encode(header.hash().as_bytes()) == wire.header_hash_hex,
        "header_hash_hex does not match canonical CometBFT header"
    );
    ensure!(
        header
            .last_block_id
            .map(|id| hex::encode(id.hash.as_bytes()))
            == wire.last_block_id_hash_hex,
        "last_block_id_hash_hex drift"
    );
    ensure!(
        header.data_hash.map(|hash| hex::encode(hash.as_bytes())) == wire.data_hash_hex,
        "data_hash_hex drift"
    );
    ensure!(
        hex::encode(header.app_hash.as_bytes()) == wire.app_hash_hex,
        "app_hash_hex drift"
    );
    ensure!(
        header
            .last_results_hash
            .map(|hash| hex::encode(hash.as_bytes()))
            == wire.last_results_hash_hex,
        "last_results_hash_hex drift"
    );
    ensure!(
        hex::encode(header.validators_hash.as_bytes()) == wire.validators_hash_hex,
        "validators_hash_hex drift"
    );
    ensure!(
        hex::encode(header.next_validators_hash.as_bytes()) == wire.next_validators_hash_hex,
        "next_validators_hash_hex drift"
    );
    Ok(header)
}

/// Encode one already-decoded CometBFT header into the exact Receipt V2 wire
/// contract, including canonical v0.38 protobuf bytes and all exposed hashes.
pub fn encode_cometbft_header_v1(header: &block::Header) -> Result<CometBftHeaderV1> {
    let raw: RawHeader = header.clone().into();
    let wire = CometBftHeaderV1 {
        schema: trnm_finality_types::COMETBFT_HEADER_SCHEMA_V1.to_string(),
        chain_id: header.chain_id.to_string(),
        height: header.height.value(),
        header_hash_hex: hex::encode(header.hash().as_bytes()),
        last_block_id_hash_hex: header
            .last_block_id
            .map(|id| hex::encode(id.hash.as_bytes())),
        data_hash_hex: header.data_hash.map(|hash| hex::encode(hash.as_bytes())),
        app_hash_hex: hex::encode(header.app_hash.as_bytes()),
        last_results_hash_hex: header
            .last_results_hash
            .map(|hash| hex::encode(hash.as_bytes())),
        validators_hash_hex: hex::encode(header.validators_hash.as_bytes()),
        next_validators_hash_hex: hex::encode(header.next_validators_hash.as_bytes()),
        header_proto_hex: hex::encode(raw.encode_to_vec()),
    };
    wire.validate_shape()?;
    ensure!(
        decode_header(&wire)? == header.clone(),
        "encoded Receipt V2 header does not round-trip"
    );
    Ok(wire)
}

fn canonical_hex_bytes(label: &str, value: &str) -> Result<Vec<u8>> {
    let bytes = hex::decode(value).with_context(|| format!("decode {label}"))?;
    ensure!(
        hex::encode(&bytes) == value,
        "{label} is not lowercase canonical hex"
    );
    Ok(bytes)
}

fn decode_canonical_message<M>(label: &str, bytes: &[u8]) -> Result<M>
where
    M: Message + Default,
{
    let message = M::decode(bytes).with_context(|| format!("decode {label} protobuf"))?;
    ensure!(
        message.encode_to_vec() == bytes,
        "{label} is not canonical protobuf or contains unknown fields"
    );
    Ok(message)
}

fn verify_deterministic_exec_result(
    receipt: &CometBftAppHashFinalityReceiptV2,
    binding: &ResearchTransactionBinding,
) -> Result<()> {
    let bytes = receipt.canonical_result_bytes()?;
    verify_deterministic_exec_result_bytes(&bytes, binding)
}

fn verify_deterministic_exec_result_bytes(
    bytes: &[u8],
    binding: &ResearchTransactionBinding,
) -> Result<()> {
    let result = decode_canonical_message::<RawExecTxResult>("canonical_result_bytes_hex", bytes)?;
    ensure!(result.code == 0, "committed ExecTxResult is not successful");
    ensure!(
        result.data.is_empty(),
        "committed Research ExecTxResult contains unexpected data"
    );
    ensure!(
        binding.expected_gas_wanted > 0 && result.gas_wanted == binding.expected_gas_wanted,
        "committed ExecTxResult gas_wanted does not match Research max_gas"
    );
    ensure!(
        result.gas_used > 0 && result.gas_used <= result.gas_wanted,
        "committed ExecTxResult gas_used is outside the successful range"
    );
    ensure!(
        result.log.is_empty(),
        "committed ExecTxResult contains nondeterministic log"
    );
    ensure!(
        result.info.is_empty(),
        "committed ExecTxResult contains nondeterministic info"
    );
    ensure!(
        result.events.is_empty(),
        "committed ExecTxResult contains nondeterministic events"
    );
    ensure!(
        result.codespace.is_empty(),
        "committed ExecTxResult contains nondeterministic codespace"
    );
    Ok(())
}

fn verify_applied_command_membership(
    proof: &AppHashObjectProofV1,
    app_hash: &[u8],
) -> Result<Vec<u8>> {
    ensure!(
        app_hash.len() == 32,
        "commitment header AppHash must be exactly 32 bytes"
    );
    let key = proof.object_key_bytes()?;
    let value = proof.value_bytes()?;
    let proof_bytes = proof.commitment_proof_bytes()?;
    let commitment =
        decode_canonical_message::<CommitmentProof>("commitment_proof_hex", &proof_bytes)?;
    ensure!(
        matches!(
            commitment.proof.as_ref(),
            Some(commitment_proof::Proof::Exist(_))
        ),
        "applied-command proof must be an ICS23 membership proof"
    );
    let root = app_hash.to_vec();
    ensure!(
        ics23::verify_membership::<ics23::HostFunctionsManager>(
            &commitment,
            &jmt_ics23_spec_v1(),
            &root,
            &key,
            &value,
        ),
        "applied-command ICS23 membership proof does not match H + 1 AppHash"
    );
    Ok(value)
}

fn jmt_ics23_spec_v1() -> ProofSpec {
    ProofSpec {
        leaf_spec: Some(LeafOp {
            hash: HashOp::Sha256.into(),
            prehash_key: HashOp::Sha256.into(),
            prehash_value: HashOp::Sha256.into(),
            length: LengthOp::NoPrefix.into(),
            prefix: JMT_LEAF_DOMAIN_SEPARATOR.to_vec(),
        }),
        inner_spec: Some(InnerSpec {
            hash: HashOp::Sha256.into(),
            child_order: vec![0, 1],
            min_prefix_length: JMT_INTERNAL_DOMAIN_SEPARATOR.len() as i32,
            max_prefix_length: JMT_INTERNAL_DOMAIN_SEPARATOR.len() as i32,
            child_size: 32,
            empty_child: JMT_SPARSE_PLACEHOLDER_HASH.to_vec(),
        }),
        min_depth: 0,
        max_depth: 64,
        prehash_key_before_comparison: true,
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use ed25519_dalek::{Signer, SigningKey};
    use ics23::{commitment_proof, CommitmentProof, ExistenceProof, InnerOp};
    use sha2::{Digest, Sha256};
    use tendermint_light_client_verifier::{
        operations::voting_power::VotingPowerTally,
        options::Options,
        types::{LightBlock, TrustThreshold},
    };
    use tendermint_testgen::{
        light_block::{LightBlock as TestgenLightBlock, TmLightBlock},
        Generator, Validator as TestgenValidator,
    };
    use trnm_finality_types::{
        authenticated_object_proof_key_v4, AppHashProofOpV1, AuthenticatedObjectRecordV1,
        CometBftLightFinalityProofV1, CometBftMerkleInclusionProofV1,
        APPHASH_OBJECT_PROOF_SCHEMA_V1, COMETBFT_APPHASH_FINALITY_RECEIPT_SCHEMA_V2,
        COMETBFT_HEADER_SCHEMA_V1, COMETBFT_JMT_PROOF_OP_TYPE_V1,
        COMETBFT_LIGHT_FINALITY_PROOF_SCHEMA_V1, COMETBFT_MERKLE_INCLUSION_PROOF_SCHEMA_V1,
    };
    use trnm_research_protocol::{
        ExternalKey, MatchEvidenceCommitmentV1, ResearchCommandV1, SignedResearchCommandV1,
    };

    use super::*;

    const EXECUTION_TIMESTAMP_MS: u64 = 1_753_449_600_000;
    const CROSS_REPO_RECEIPT_V2: &[u8] =
        include_bytes!("../fixtures/cometbft-apphash-finality-receipt-v2.json");
    const CROSS_REPO_TRUST_ANCHOR_V1: &[u8] =
        include_bytes!("../fixtures/cometbft-trust-anchor-v1.json");
    const CROSS_REPO_EXPECTED_V1: &[u8] =
        include_bytes!("../fixtures/cometbft-receipt-v2-verified-outcome-v1.json");
    const CROSS_REPO_TAMPERS_V1: &[u8] =
        include_bytes!("../fixtures/cometbft-receipt-v2-tamper-vectors-v1.json");

    fn canonical_fixture_payload(file: &'static [u8]) -> &'static [u8] {
        let payload = file
            .strip_suffix(b"\n")
            .expect("repository JSON fixture must end in one transport newline");
        assert!(!payload.ends_with(b"\n"));
        payload
    }

    fn options() -> Options {
        Options {
            trust_threshold: TrustThreshold::TWO_THIRDS,
            trusting_period: Duration::from_secs(60),
            clock_drift: Duration::from_secs(1),
        }
    }

    fn refresh_trust_anchor_hash(anchor: &mut CometBftTrustAnchorV1) {
        anchor.anchor_hash_hex.clear();
        anchor.anchor_hash_hex = hex::encode(anchor.compute_anchor_hash().unwrap());
    }

    fn light_block(raw: TmLightBlock) -> LightBlock {
        LightBlock::new(
            raw.signed_header,
            raw.validators,
            raw.next_validators,
            raw.provider,
        )
    }

    fn external_key(namespace: &str, id: &str) -> ExternalKey {
        ExternalKey::from_external_id(namespace, id).unwrap()
    }

    fn research_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[0x11; 32])
    }

    fn signed_research_command() -> SignedResearchCommandV1 {
        SignedResearchCommandV1::sign(
            "trnm-test-1".to_string(),
            external_key("trnm.command", "command-001"),
            "did:trnm:nakama-authority".to_string(),
            AuthorityRole::NakamaAuthority,
            7,
            ResearchCommandV1::MatchEvidenceCommitment(MatchEvidenceCommitmentV1 {
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
            }),
            &research_signing_key(),
        )
        .unwrap()
    }

    fn research_envelope(
        signed: &SignedResearchCommandV1,
        issued_at_unix_ms: u64,
        expires_at_unix_ms: u64,
    ) -> (CanonicalResearchTxV1, SignedCommandEnvelopeV1) {
        let tx = CanonicalResearchTxV1::from_signed_command(signed, 250_000, 1_000_000).unwrap();
        let payload = tx.canonical_bytes().unwrap();
        let envelope = SignedCommandEnvelopeV1::sign(
            signed.chain_id.clone(),
            signed.command_id.to_hex(),
            signed.signer_did.clone(),
            "nakama",
            signed.nonce,
            issued_at_unix_ms,
            expires_at_unix_ms,
            CANONICAL_RESEARCH_TX_PAYLOAD_TYPE_V1,
            &payload,
            &research_signing_key(),
        )
        .unwrap();
        (tx, envelope)
    }

    fn envelope_wire(envelope: &SignedCommandEnvelopeV1) -> Vec<u8> {
        envelope.to_wire_bytes().unwrap()
    }

    fn resign_envelope(
        envelope: &SignedCommandEnvelopeV1,
        signing_key: &SigningKey,
    ) -> SignedCommandEnvelopeV1 {
        SignedCommandEnvelopeV1::sign(
            envelope.chain_id.clone(),
            envelope.command_id.clone(),
            envelope.signer_id.clone(),
            envelope.signer_role.clone(),
            envelope.nonce,
            envelope.issued_at_unix_ms,
            envelope.expires_at_unix_ms,
            envelope.payload_type.clone(),
            &envelope.payload_bytes().unwrap(),
            signing_key,
        )
        .unwrap()
    }

    fn successful_research_result(
        binding: &ResearchTransactionBinding,
        gas_used: i64,
    ) -> RawExecTxResult {
        RawExecTxResult {
            gas_wanted: binding.expected_gas_wanted,
            gas_used,
            ..Default::default()
        }
    }

    fn comet_leaf_hash(value: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update([0]);
        hasher.update(value);
        hasher.finalize().into()
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

    fn jmt_membership_fixture(
        query_height: u64,
        key: &[u8],
        value: &[u8],
        siblings: &[[u8; 32]],
    ) -> (AppHashObjectProofV1, [u8; 32]) {
        let key_hash: [u8; 32] = Sha256::digest(key).into();
        let mut leaf_hasher = Sha256::new();
        leaf_hasher.update(JMT_LEAF_DOMAIN_SEPARATOR);
        leaf_hasher.update(key_hash);
        leaf_hasher.update(Sha256::digest(value));
        let mut current: [u8; 32] = leaf_hasher.finalize().into();

        let mut path = Vec::with_capacity(siblings.len());
        let mut skip = 256usize - siblings.len();
        let mut sibling_index = 0usize;
        for byte_index in (0..32).rev() {
            for bit_index in 0..8 {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                let sibling = siblings[sibling_index];
                let bit = (key_hash[byte_index] >> bit_index) & 1;
                let (prefix, suffix) = if bit == 1 {
                    let mut prefix = JMT_INTERNAL_DOMAIN_SEPARATOR.to_vec();
                    prefix.extend_from_slice(&sibling);
                    (prefix, Vec::new())
                } else {
                    (JMT_INTERNAL_DOMAIN_SEPARATOR.to_vec(), sibling.to_vec())
                };
                let mut inner_hasher = Sha256::new();
                inner_hasher.update(&prefix);
                inner_hasher.update(current);
                inner_hasher.update(&suffix);
                current = inner_hasher.finalize().into();
                path.push(InnerOp {
                    hash: HashOp::Sha256.into(),
                    prefix,
                    suffix,
                });
                sibling_index += 1;
            }
        }
        assert_eq!(sibling_index, siblings.len());

        let commitment = CommitmentProof {
            proof: Some(commitment_proof::Proof::Exist(ExistenceProof {
                key: key.to_vec(),
                value: value.to_vec(),
                leaf: Some(LeafOp {
                    hash: HashOp::Sha256.into(),
                    prehash_key: HashOp::Sha256.into(),
                    prehash_value: HashOp::Sha256.into(),
                    length: LengthOp::NoPrefix.into(),
                    prefix: JMT_LEAF_DOMAIN_SEPARATOR.to_vec(),
                }),
                path,
            })),
        };
        let proof_bytes = commitment.encode_to_vec();
        let proof_hex = hex::encode(proof_bytes);
        let key_hex = hex::encode(key);
        (
            AppHashObjectProofV1 {
                schema: APPHASH_OBJECT_PROOF_SCHEMA_V1.to_string(),
                query_height,
                object_key_hex: key_hex.clone(),
                value_hex: hex::encode(value),
                proof_op: AppHashProofOpV1 {
                    proof_type: COMETBFT_JMT_PROOF_OP_TYPE_V1.to_string(),
                    key_hex,
                    data_hex: proof_hex.clone(),
                },
                commitment_proof_hex: proof_hex,
            },
            current,
        )
    }

    fn binding_header(
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

    fn refresh_receipt_hash(receipt: &mut CometBftAppHashFinalityReceiptV2) {
        receipt.receipt_hash_hex.clear();
        receipt.receipt_hash_hex = hex::encode(receipt.compute_receipt_hash().unwrap());
        receipt.validate_shape().unwrap();
    }

    fn replace_authenticated_applied_command(
        receipt: &mut CometBftAppHashFinalityReceiptV2,
        logical_key: &str,
        object_type: &str,
        object_version: u64,
        value: Vec<u8>,
    ) {
        let proof_key_hex = hex::encode(authenticated_object_proof_key_v4(logical_key).unwrap());
        let record = AuthenticatedObjectRecordV1::new(object_type, object_version, value)
            .unwrap()
            .encode()
            .unwrap();
        receipt.applied_command_object_proof.object_key_hex = proof_key_hex.clone();
        receipt.applied_command_object_proof.proof_op.key_hex = proof_key_hex;
        receipt.applied_command_object_proof.value_hex = hex::encode(record);
        refresh_receipt_hash(receipt);
    }

    fn applied_command_binding_fixture() -> (
        CometBftAppHashFinalityReceiptV2,
        ResearchTransactionBinding,
        SignedResearchCommandV1,
    ) {
        let signed = signed_research_command();
        let (_, envelope) = research_envelope(
            &signed,
            EXECUTION_TIMESTAMP_MS - 1_000,
            EXECUTION_TIMESTAMP_MS + 1_000,
        );
        let raw_tx = envelope_wire(&envelope);
        let binding = verify_research_transaction_binding(
            &raw_tx,
            &signed.chain_id,
            &signed.command_id.to_hex(),
            &hex::encode(signed.command_fingerprint()),
            EXECUTION_TIMESTAMP_MS,
        )
        .unwrap();
        let transaction_proof = single_leaf_proof(&comet_tx_hash(&raw_tx));
        let result = successful_research_result(&binding, 10_000).encode_to_vec();
        let result_proof = single_leaf_proof(&result);
        let execution_header = binding_header(
            41,
            1,
            Some(hex::encode([0; 32])),
            Some(hex::encode(transaction_proof.root_hash().unwrap())),
            3,
            Some(hex::encode([2; 32])),
        );
        let commitment_header = binding_header(
            42,
            2,
            Some(execution_header.header_hash_hex.clone()),
            None,
            4,
            Some(hex::encode(result_proof.root_hash().unwrap())),
        );
        let authenticated_record = AuthenticatedObjectRecordV1::new(
            RESEARCH_APPLIED_COMMAND_OBJECT_TYPE_V1,
            1,
            binding.applied_command_value.clone(),
        )
        .unwrap()
        .encode()
        .unwrap();
        let proof_key_hex = hex::encode(
            authenticated_object_proof_key_v4(&binding.applied_command_logical_key).unwrap(),
        );
        let commitment_proof_hex = "0a0101".to_string();
        let mut receipt = CometBftAppHashFinalityReceiptV2 {
            schema: COMETBFT_APPHASH_FINALITY_RECEIPT_SCHEMA_V2.to_string(),
            chain_id: signed.chain_id.clone(),
            command_id: signed.command_id.to_hex(),
            command_fingerprint_hex: hex::encode(signed.command_fingerprint()),
            execution_height: 41,
            commitment_height: 42,
            comet_tx_hash_hex: hex::encode(Sha256::digest(&raw_tx)),
            raw_tx_hex: hex::encode(raw_tx),
            execution_header,
            commitment_light_proof: CometBftLightFinalityProofV1 {
                schema: COMETBFT_LIGHT_FINALITY_PROOF_SCHEMA_V1.to_string(),
                header: commitment_header.clone(),
                commit_height: 42,
                commit_block_id_hash_hex: commitment_header.header_hash_hex.clone(),
                signed_header_proto_hex: "0a0102".to_string(),
                validator_set_proto_hex: "0a0103".to_string(),
            },
            transaction_inclusion_proof: transaction_proof,
            canonical_result_bytes_hex: hex::encode(result),
            result_inclusion_proof: result_proof,
            applied_command_object_proof: AppHashObjectProofV1 {
                schema: APPHASH_OBJECT_PROOF_SCHEMA_V1.to_string(),
                query_height: 41,
                object_key_hex: proof_key_hex.clone(),
                value_hex: hex::encode(authenticated_record),
                proof_op: AppHashProofOpV1 {
                    proof_type: COMETBFT_JMT_PROOF_OP_TYPE_V1.to_string(),
                    key_hex: proof_key_hex,
                    data_hex: commitment_proof_hex.clone(),
                },
                commitment_proof_hex,
            },
            receipt_hash_hex: String::new(),
        };
        refresh_receipt_hash(&mut receipt);
        (receipt, binding, signed)
    }

    fn validator_fixture() -> (TestgenValidator, validator::Set) {
        let generator = TestgenValidator::new("receipt-validator").voting_power(100);
        let info = generator.generate().unwrap();
        let validators = validator::Set::without_proposer(vec![info]);
        (generator, validators)
    }

    fn signed_header_proto(
        header: block::Header,
        generator: &TestgenValidator,
    ) -> (block::signed_header::SignedHeader, Vec<u8>) {
        let validator_info = generator.generate().unwrap();
        let block_id = block::Id {
            hash: header.hash(),
            part_set_header: block::parts::Header::new(1, header.hash()).unwrap(),
        };
        let mut vote = tendermint::vote::Vote {
            vote_type: tendermint::vote::Type::Precommit,
            height: header.height,
            round: block::Round::try_from(0u32).unwrap(),
            block_id: Some(block_id),
            timestamp: Some(header.time),
            validator_address: validator_info.address,
            validator_index: tendermint::vote::ValidatorIndex::try_from(0u32).unwrap(),
            signature: None,
            extension: Vec::new(),
            extension_signature: None,
        };
        // `tendermint-testgen::get_vote_sign_bytes` constructs a SignedVote
        // and therefore requires the otherwise-irrelevant signature slot to
        // be populated before it can canonicalize the vote.  The signature
        // bytes are not part of the canonical vote sign bytes; replace this
        // placeholder immediately with the real Ed25519 signature below.
        vote.signature = tendermint::signature::Signature::new([0_u8; 64]).unwrap();
        let sign_bytes =
            tendermint_testgen::helpers::get_vote_sign_bytes(header.chain_id.clone(), &vote);
        let validator_id = generator.id.as_ref().unwrap().as_bytes();
        let mut seed = [0u8; 32];
        seed[..validator_id.len()].copy_from_slice(validator_id);
        vote.signature = tendermint::signature::Signature::new(
            SigningKey::from_bytes(&seed).sign(&sign_bytes).to_bytes(),
        )
        .unwrap();
        let commit = block::Commit {
            height: header.height,
            round: block::Round::try_from(0u32).unwrap(),
            block_id,
            signatures: vec![block::CommitSig::BlockIdFlagCommit {
                validator_address: validator_info.address,
                timestamp: header.time,
                signature: vote.signature,
            }],
        };
        let signed_header = block::signed_header::SignedHeader::new(header, commit).unwrap();
        let raw: RawSignedHeader = signed_header.clone().into();
        (signed_header, raw.encode_to_vec())
    }

    fn receipt_v2_e2e_fixture() -> (
        CometBftAppHashFinalityReceiptV2,
        block::Header,
        validator::Set,
    ) {
        let signed = signed_research_command();
        let (_, target_envelope) = research_envelope(
            &signed,
            EXECUTION_TIMESTAMP_MS - 1_000,
            EXECUTION_TIMESTAMP_MS + 10_000,
        );
        let target_raw_tx = envelope_wire(&target_envelope);
        let binding =
            decode_research_transaction_binding(&target_raw_tx, EXECUTION_TIMESTAMP_MS).unwrap();

        let mut raw_transactions = Vec::new();
        for index in 0..5u8 {
            if index == 2 {
                raw_transactions.push(target_raw_tx.clone());
            } else {
                let mut other = target_envelope.clone();
                other.command_id = hex::encode([0x80 + index; 32]);
                raw_transactions.push(envelope_wire(&resign_envelope(
                    &other,
                    &research_signing_key(),
                )));
            }
        }
        let canonical_results = (0..5)
            .map(|index| {
                if index == 2 {
                    successful_research_result(&binding, 10_000).encode_to_vec()
                } else {
                    RawExecTxResult::default().encode_to_vec()
                }
            })
            .collect::<Vec<_>>();
        let transaction_hashes = raw_transactions
            .iter()
            .map(|transaction| comet_tx_hash(transaction))
            .collect::<Vec<_>>();
        let tx_root = CometBftMerkleInclusionProofV1::from_leaf_values(&transaction_hashes, 2)
            .unwrap()
            .root_hash()
            .unwrap();
        let results_root = CometBftMerkleInclusionProofV1::from_leaf_values(&canonical_results, 2)
            .unwrap()
            .root_hash()
            .unwrap();

        let authenticated_record = AuthenticatedObjectRecordV1::new(
            RESEARCH_APPLIED_COMMAND_OBJECT_TYPE_V1,
            1,
            binding.applied_command_value.clone(),
        )
        .unwrap()
        .encode()
        .unwrap();
        let proof_key =
            authenticated_object_proof_key_v4(&binding.applied_command_logical_key).unwrap();
        let (applied_command_object_proof, app_hash) = jmt_membership_fixture(
            41,
            &proof_key,
            &authenticated_record,
            &[[0x21; 32], [0x32; 32], [0x43; 32], [0x54; 32]],
        );

        let (validator_generator, validators) = validator_fixture();
        let validator_info = validator_generator.generate().unwrap();
        let execution_time = tendermint::Time::from_unix_timestamp(
            i64::try_from(EXECUTION_TIMESTAMP_MS / 1_000).unwrap(),
            u32::try_from((EXECUTION_TIMESTAMP_MS % 1_000) * 1_000_000).unwrap(),
        )
        .unwrap();
        let execution_header = block::Header {
            version: block::header::Version { block: 11, app: 5 },
            chain_id: signed.chain_id.parse().unwrap(),
            height: block::Height::try_from(41u64).unwrap(),
            time: execution_time,
            last_block_id: None,
            last_commit_hash: None,
            data_hash: Some(tendermint::Hash::Sha256(tx_root)),
            validators_hash: validators.hash(),
            next_validators_hash: validators.hash(),
            consensus_hash: tendermint::Hash::Sha256([0x65; 32]),
            app_hash: tendermint::AppHash::try_from(vec![0x76; 32]).unwrap(),
            last_results_hash: Some(tendermint::Hash::Sha256([0x87; 32])),
            evidence_hash: None,
            proposer_address: validator_info.address,
        };
        let commitment_header = block::Header {
            version: block::header::Version { block: 11, app: 5 },
            chain_id: execution_header.chain_id.clone(),
            height: block::Height::try_from(42u64).unwrap(),
            time: (execution_time + Duration::from_secs(1)).unwrap(),
            last_block_id: Some(block::Id {
                hash: execution_header.hash(),
                part_set_header: block::parts::Header::new(1, execution_header.hash()).unwrap(),
            }),
            last_commit_hash: None,
            data_hash: None,
            validators_hash: validators.hash(),
            next_validators_hash: validators.hash(),
            consensus_hash: tendermint::Hash::Sha256([0x65; 32]),
            app_hash: tendermint::AppHash::try_from(app_hash.to_vec()).unwrap(),
            last_results_hash: Some(tendermint::Hash::Sha256(results_root)),
            evidence_hash: None,
            proposer_address: validator_info.address,
        };
        let (_, commitment_signed_header_proto) =
            signed_header_proto(commitment_header.clone(), &validator_generator);
        let raw_validators: RawValidatorSet = validators.clone().into();
        let receipt =
            assemble_cometbft_apphash_finality_receipt_v2(CometBftReceiptAssemblyInputV2 {
                target_command_id: signed.command_id.to_hex(),
                execution_header: encode_cometbft_header_v1(&execution_header).unwrap(),
                commitment_header: encode_cometbft_header_v1(&commitment_header).unwrap(),
                commitment_signed_header_proto,
                commitment_validator_set_proto: raw_validators.encode_to_vec(),
                raw_transactions,
                canonical_results,
                applied_command_object_proof,
            })
            .unwrap();
        (receipt, execution_header, validators)
    }

    #[test]
    fn production_light_verifier_accepts_adjacent_signed_header() {
        let trusted = light_block(TestgenLightBlock::new_default(10).generate().unwrap());
        let untrusted = light_block(
            TestgenLightBlock::new_default(10)
                .next()
                .generate()
                .unwrap(),
        );
        let now = (untrusted.time() + Duration::from_secs(1)).unwrap();
        let verdict = ProdVerifier::default().verify_update_header(
            untrusted.as_untrusted_state(),
            trusted.as_trusted_state(),
            &options(),
            now,
        );
        assert_eq!(verdict, Verdict::Success);
    }

    #[test]
    fn public_receipt_v2_verifier_accepts_assembled_five_leaf_evidence_and_rejects_tampering() {
        let (receipt, trusted_header, trusted_validators) = receipt_v2_e2e_fixture();
        assert_eq!(receipt.transaction_inclusion_proof.leaf_count, 5);
        assert_eq!(receipt.result_inclusion_proof.leaf_count, 5);
        let trust_options = options();
        let now = (trusted_header.time + Duration::from_secs(2)).unwrap();
        let verify = |candidate: &CometBftAppHashFinalityReceiptV2| {
            verify_cometbft_apphash_finality_receipt_v2(
                candidate,
                CometBftTrustContext {
                    trusted_state: TrustedBlockState {
                        chain_id: &trusted_header.chain_id,
                        header_time: trusted_header.time,
                        height: trusted_header.height,
                        next_validators: &trusted_validators,
                        next_validators_hash: trusted_validators.hash(),
                    },
                    options: &trust_options,
                    now,
                },
            )
        };
        let ReceiptV2VerificationOutcome::Final(verified) = verify(&receipt) else {
            panic!("assembled Receipt V2 did not verify");
        };
        assert_eq!(verified.command_id, receipt.command_id);
        assert_eq!(verified.transaction_index, 2);

        let mut tampered_object = receipt.clone();
        let mut value = tampered_object
            .applied_command_object_proof
            .value_bytes()
            .unwrap();
        *value.last_mut().unwrap() ^= 1;
        tampered_object.applied_command_object_proof.value_hex = hex::encode(value);
        tampered_object.receipt_hash_hex.clear();
        tampered_object.receipt_hash_hex =
            hex::encode(tampered_object.compute_receipt_hash().unwrap());
        assert!(matches!(
            verify(&tampered_object),
            ReceiptV2VerificationOutcome::StructuralInvalid { .. }
        ));

        let mut tampered_signed_header = receipt;
        tampered_signed_header
            .commitment_light_proof
            .signed_header_proto_hex
            .push_str("f80701");
        tampered_signed_header.receipt_hash_hex.clear();
        tampered_signed_header.receipt_hash_hex =
            hex::encode(tampered_signed_header.compute_receipt_hash().unwrap());
        assert!(matches!(
            verify(&tampered_signed_header),
            ReceiptV2VerificationOutcome::StructuralInvalid { .. }
        ));
    }

    #[test]
    fn owned_trust_anchor_matches_low_level_verifier_without_borrowed_public_inputs() {
        let (receipt, trusted_header, trusted_validators) = receipt_v2_e2e_fixture();
        let anchor_wire = encode_cometbft_trust_anchor_v1(
            &trusted_header,
            &trusted_validators,
            2,
            3,
            Duration::from_secs(60),
            Duration::from_secs(1),
        )
        .unwrap();
        let anchor_bytes = anchor_wire.canonical_bytes().unwrap();
        let anchor = ValidatedCometBftTrustAnchorV1::from_canonical_bytes(&anchor_bytes).unwrap();

        let now = (trusted_header.time + Duration::from_secs(2)).unwrap();
        let now_nanos = u64::try_from(now.unix_timestamp_nanos()).unwrap();
        let high_level = verify_cometbft_apphash_finality_receipt_v2_with_trust_anchor(
            &receipt,
            &anchor,
            UNIX_EPOCH + Duration::from_nanos(now_nanos),
        );
        let manual = verify_cometbft_apphash_finality_receipt_v2(
            &receipt,
            CometBftTrustContext {
                trusted_state: TrustedBlockState {
                    chain_id: &trusted_header.chain_id,
                    header_time: trusted_header.time,
                    height: trusted_header.height,
                    next_validators: &trusted_validators,
                    next_validators_hash: trusted_validators.hash(),
                },
                options: &options(),
                now,
            },
        );
        assert_eq!(high_level, manual);

        let before_epoch = UNIX_EPOCH.checked_sub(Duration::from_secs(1)).unwrap();
        assert!(matches!(
            verify_cometbft_apphash_finality_receipt_v2_with_trust_anchor(
                &receipt,
                &anchor,
                before_epoch,
            ),
            ReceiptV2VerificationOutcome::Untrusted { .. }
        ));
        let expired = UNIX_EPOCH
            + Duration::from_secs(
                u64::try_from(trusted_header.time.unix_timestamp()).unwrap() + 61,
            );
        assert!(matches!(
            verify_cometbft_apphash_finality_receipt_v2_with_trust_anchor(
                &receipt, &anchor, expired,
            ),
            ReceiptV2VerificationOutcome::Untrusted { .. }
        ));
    }

    #[test]
    fn trust_anchor_rejects_semantic_header_and_validator_drift() {
        let (_, trusted_header, trusted_validators) = receipt_v2_e2e_fixture();
        let wire = encode_cometbft_trust_anchor_v1(
            &trusted_header,
            &trusted_validators,
            2,
            3,
            Duration::from_secs(60),
            Duration::from_secs(1),
        )
        .unwrap();

        let mut wrong_time = wire.clone();
        wrong_time.trusted_header_time_rfc3339 = (trusted_header.time + Duration::from_secs(1))
            .unwrap()
            .to_rfc3339();
        refresh_trust_anchor_hash(&mut wrong_time);
        assert!(ValidatedCometBftTrustAnchorV1::try_from(wrong_time).is_err());

        let mut unknown_header_field = wire.clone();
        unknown_header_field
            .trusted_header
            .header_proto_hex
            .push_str("f80701");
        refresh_trust_anchor_hash(&mut unknown_header_field);
        assert!(ValidatedCometBftTrustAnchorV1::try_from(unknown_header_field).is_err());

        let mut raw_validators = decode_canonical_message::<RawValidatorSet>(
            "trusted validator fixture",
            &wire.trusted_next_validator_set_proto_bytes().unwrap(),
        )
        .unwrap();
        raw_validators.validators[0].proposer_priority = 1;
        let mut nonzero_priority = wire.clone();
        nonzero_priority.trusted_next_validator_set_proto_hex =
            hex::encode(raw_validators.encode_to_vec());
        refresh_trust_anchor_hash(&mut nonzero_priority);
        assert!(ValidatedCometBftTrustAnchorV1::try_from(nonzero_priority).is_err());

        let mut duplicate_validator = decode_canonical_message::<RawValidatorSet>(
            "trusted validator fixture",
            &wire.trusted_next_validator_set_proto_bytes().unwrap(),
        )
        .unwrap();
        duplicate_validator
            .validators
            .push(duplicate_validator.validators[0].clone());
        duplicate_validator.total_voting_power *= 2;
        let mut duplicate = wire;
        duplicate.trusted_next_validator_set_proto_hex =
            hex::encode(duplicate_validator.encode_to_vec());
        refresh_trust_anchor_hash(&mut duplicate);
        assert!(ValidatedCometBftTrustAnchorV1::try_from(duplicate).is_err());
    }

    #[test]
    fn cross_repo_receipt_v2_fixture_exports_and_verifies_deterministically() {
        let receipt_bytes = canonical_fixture_payload(CROSS_REPO_RECEIPT_V2);
        let anchor_bytes = canonical_fixture_payload(CROSS_REPO_TRUST_ANCHOR_V1);
        let receipt =
            CometBftAppHashFinalityReceiptV2::from_canonical_bytes(receipt_bytes).unwrap();
        let anchor = ValidatedCometBftTrustAnchorV1::from_canonical_bytes(anchor_bytes).unwrap();
        let expected: serde_json::Value = serde_json::from_slice(CROSS_REPO_EXPECTED_V1).unwrap();

        assert_eq!(
            hex::encode(Sha256::digest(receipt_bytes)),
            expected["canonical_receipt_sha256_hex"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(Sha256::digest(anchor_bytes)),
            expected["canonical_trust_anchor_sha256_hex"]
                .as_str()
                .unwrap()
        );
        assert_eq!(
            anchor.wire().anchor_hash_hex,
            expected["trust_anchor_hash_hex"].as_str().unwrap()
        );

        let execution_header = decode_header(&receipt.execution_header).unwrap();
        let raw_validators = decode_canonical_message::<RawValidatorSet>(
            "fixture trusted next validator set",
            &canonical_hex_bytes(
                "fixture validator_set_proto_hex",
                &receipt.commitment_light_proof.validator_set_proto_hex,
            )
            .unwrap(),
        )
        .unwrap();
        let validators: validator::Set = raw_validators.try_into().unwrap();
        let exported = encode_cometbft_trust_anchor_v1(
            &execution_header,
            &validators,
            2,
            3,
            Duration::from_secs(86_400),
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(exported.canonical_bytes().unwrap(), anchor_bytes);

        let verification_time = UNIX_EPOCH
            + Duration::new(
                expected["verification_time_unix_seconds"].as_u64().unwrap(),
                u32::try_from(expected["verification_time_subsec_nanos"].as_u64().unwrap())
                    .unwrap(),
            );
        let ReceiptV2VerificationOutcome::Final(verified) =
            verify_cometbft_apphash_finality_receipt_v2_with_trust_anchor(
                &receipt,
                &anchor,
                verification_time,
            )
        else {
            panic!("cross-repository Receipt V2 fixture did not verify")
        };
        let expected_verified = &expected["verified"];
        assert_eq!(
            verified.receipt_hash_hex,
            expected_verified["receipt_hash_hex"].as_str().unwrap()
        );
        assert_eq!(
            verified.chain_id,
            expected_verified["chain_id"].as_str().unwrap()
        );
        assert_eq!(
            verified.command_id,
            expected_verified["command_id"].as_str().unwrap()
        );
        assert_eq!(
            verified.command_fingerprint_hex,
            expected_verified["command_fingerprint_hex"]
                .as_str()
                .unwrap()
        );
        assert_eq!(
            verified.comet_tx_hash_hex,
            expected_verified["comet_tx_hash_hex"].as_str().unwrap()
        );
        assert_eq!(
            verified.transaction_index,
            expected_verified["transaction_index"].as_u64().unwrap()
        );
        assert_eq!(
            verified.applied_command_object_key_hex,
            expected_verified["applied_command_object_key_hex"]
                .as_str()
                .unwrap()
        );
        assert_eq!(
            verified.execution_height,
            expected_verified["execution_height"].as_u64().unwrap()
        );
        assert_eq!(
            verified.commitment_height,
            expected_verified["commitment_height"].as_u64().unwrap()
        );
        assert_eq!(
            verified.commitment_header_hash_hex,
            expected_verified["commitment_header_hash_hex"]
                .as_str()
                .unwrap()
        );
        assert_eq!(
            verified.app_hash_hex,
            expected_verified["app_hash_hex"].as_str().unwrap()
        );
    }

    #[test]
    fn cross_repo_tamper_vectors_fail_closed() {
        let receipt = CometBftAppHashFinalityReceiptV2::from_canonical_bytes(
            canonical_fixture_payload(CROSS_REPO_RECEIPT_V2),
        )
        .unwrap();
        let anchor_wire = CometBftTrustAnchorV1::from_canonical_bytes(canonical_fixture_payload(
            CROSS_REPO_TRUST_ANCHOR_V1,
        ))
        .unwrap();
        let anchor = ValidatedCometBftTrustAnchorV1::try_from(anchor_wire.clone()).unwrap();
        let fixture: serde_json::Value = serde_json::from_slice(CROSS_REPO_TAMPERS_V1).unwrap();
        assert_eq!(
            fixture["schema"],
            "trnm_cometbft_receipt_v2_tamper_vectors_v1"
        );

        for vector in fixture["vectors"].as_array().unwrap() {
            let id = vector["id"].as_str().unwrap();
            let scope = vector["scope"].as_str().unwrap();
            let expected = vector["expected"].as_str().unwrap();
            match scope {
                "receipt" => {
                    let mut json = serde_json::to_value(&receipt).unwrap();
                    apply_fixture_mutation(&mut json, vector);
                    let mut candidate: CometBftAppHashFinalityReceiptV2 =
                        serde_json::from_value(json).unwrap();
                    candidate.receipt_hash_hex.clear();
                    candidate.receipt_hash_hex =
                        hex::encode(candidate.compute_receipt_hash().unwrap());
                    let outcome = verify_cometbft_apphash_finality_receipt_v2_with_trust_anchor(
                        &candidate,
                        &anchor,
                        UNIX_EPOCH + Duration::from_secs(1_786_034_510),
                    );
                    assert_eq!(expected, "structural_invalid", "unexpected fixture {id}");
                    assert!(
                        matches!(
                            outcome,
                            ReceiptV2VerificationOutcome::StructuralInvalid { .. }
                        ),
                        "receipt tamper vector {id} did not fail structurally: {outcome:?}"
                    );
                }
                "anchor" => {
                    let mut json = serde_json::to_value(&anchor_wire).unwrap();
                    apply_fixture_mutation(&mut json, vector);
                    let mut candidate: CometBftTrustAnchorV1 =
                        serde_json::from_value(json).unwrap();
                    refresh_trust_anchor_hash(&mut candidate);
                    assert_eq!(expected, "anchor_rejected", "unexpected fixture {id}");
                    assert!(
                        ValidatedCometBftTrustAnchorV1::try_from(candidate).is_err(),
                        "anchor tamper vector {id} was accepted"
                    );
                }
                "verification_time" => {
                    let seconds = vector["value_unix_seconds"].as_u64().unwrap();
                    let nanos =
                        u32::try_from(vector["value_subsec_nanos"].as_u64().unwrap()).unwrap();
                    let outcome = verify_cometbft_apphash_finality_receipt_v2_with_trust_anchor(
                        &receipt,
                        &anchor,
                        UNIX_EPOCH + Duration::new(seconds, nanos),
                    );
                    assert_eq!(expected, "untrusted", "unexpected fixture {id}");
                    assert!(
                        matches!(outcome, ReceiptV2VerificationOutcome::Untrusted { .. }),
                        "verification-time vector {id} had unexpected outcome: {outcome:?}"
                    );
                }
                _ => panic!("unsupported tamper fixture scope {scope}"),
            }
        }
    }

    fn apply_fixture_mutation(document: &mut serde_json::Value, vector: &serde_json::Value) {
        let pointer = vector["json_pointer"].as_str().unwrap();
        let target = document
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("tamper fixture pointer is absent: {pointer}"));
        match vector["operation"].as_str().unwrap() {
            "replace" => *target = vector["value"].clone(),
            "append_hex" => {
                let value = format!(
                    "{}{}",
                    target.as_str().unwrap(),
                    vector["value"].as_str().unwrap()
                );
                *target = serde_json::Value::String(value);
            }
            "flip_last_hex_nibble" => {
                let mut value = target.as_str().unwrap().to_string();
                let last = value.pop().unwrap();
                value.push(if last == '0' { '1' } else { '0' });
                *target = serde_json::Value::String(value);
            }
            operation => panic!("unsupported tamper fixture operation {operation}"),
        }
    }

    #[test]
    fn expired_trust_root_is_classified_untrusted() {
        let trusted = light_block(TestgenLightBlock::new_default(10).generate().unwrap());
        let untrusted = light_block(
            TestgenLightBlock::new_default(10)
                .next()
                .generate()
                .unwrap(),
        );
        let now = (trusted.time() + Duration::from_secs(61)).unwrap();
        let verdict = ProdVerifier::default().verify_update_header(
            untrusted.as_untrusted_state(),
            trusted.as_trusted_state(),
            &options(),
            now,
        );
        assert!(matches!(
            classify_light_verdict(verdict),
            Some(ReceiptV2VerificationOutcome::Untrusted { .. })
        ));
    }

    #[test]
    fn insufficient_trusted_overlap_is_classified_untrusted_not_not_final() {
        let verdict = Verdict::NotEnoughTrust(VotingPowerTally {
            total: 100,
            tallied: 20,
            trust_threshold: TrustThreshold::TWO_THIRDS,
        });
        assert!(matches!(
            classify_light_verdict(verdict),
            Some(ReceiptV2VerificationOutcome::Untrusted { .. })
        ));
    }

    #[test]
    fn outer_envelope_raw_transaction_binds_both_signature_layers() {
        let signed = signed_research_command();
        let (_, envelope) = research_envelope(
            &signed,
            EXECUTION_TIMESTAMP_MS - 1_000,
            EXECUTION_TIMESTAMP_MS + 1_000,
        );
        let raw_tx = envelope_wire(&envelope);
        let command_id = signed.command_id.to_hex();
        let fingerprint = hex::encode(signed.command_fingerprint());

        let binding = verify_research_transaction_binding(
            &raw_tx,
            &signed.chain_id,
            &command_id,
            &fingerprint,
            EXECUTION_TIMESTAMP_MS,
        )
        .unwrap();
        assert_eq!(binding.expected_gas_wanted, 250_000);
        assert!(verify_research_transaction_binding(
            &raw_tx,
            "trnm-other-chain",
            &command_id,
            &fingerprint,
            EXECUTION_TIMESTAMP_MS,
        )
        .is_err());
        assert!(verify_research_transaction_binding(
            &raw_tx,
            &signed.chain_id,
            &hex::encode([3; 32]),
            &fingerprint,
            EXECUTION_TIMESTAMP_MS,
        )
        .is_err());
        assert!(verify_research_transaction_binding(
            &raw_tx,
            &signed.chain_id,
            &command_id,
            &hex::encode([4; 32]),
            EXECUTION_TIMESTAMP_MS,
        )
        .is_err());
    }

    #[test]
    fn inner_only_or_noncanonical_inner_payload_is_rejected() {
        let signed = signed_research_command();
        let (tx, envelope) = research_envelope(
            &signed,
            EXECUTION_TIMESTAMP_MS - 1_000,
            EXECUTION_TIMESTAMP_MS + 1_000,
        );
        let command_id = signed.command_id.to_hex();
        let fingerprint = hex::encode(signed.command_fingerprint());
        let inner = tx.canonical_bytes().unwrap();

        assert!(verify_research_transaction_binding(
            &inner,
            &signed.chain_id,
            &command_id,
            &fingerprint,
            EXECUTION_TIMESTAMP_MS,
        )
        .is_err());

        let mut noncanonical_inner = vec![b' '];
        noncanonical_inner.extend_from_slice(&inner);
        let noncanonical_envelope = SignedCommandEnvelopeV1::sign(
            envelope.chain_id,
            envelope.command_id,
            envelope.signer_id,
            envelope.signer_role,
            envelope.nonce,
            envelope.issued_at_unix_ms,
            envelope.expires_at_unix_ms,
            envelope.payload_type,
            &noncanonical_inner,
            &research_signing_key(),
        )
        .unwrap();
        assert!(verify_research_transaction_binding(
            &envelope_wire(&noncanonical_envelope),
            &signed.chain_id,
            &command_id,
            &fingerprint,
            EXECUTION_TIMESTAMP_MS,
        )
        .is_err());
    }

    #[test]
    fn receipt_v2_rejects_noncanonical_outer_envelope_encodings() {
        let signed = signed_research_command();
        let (_, envelope) = research_envelope(
            &signed,
            EXECUTION_TIMESTAMP_MS - 1_000,
            EXECUTION_TIMESTAMP_MS + 1_000,
        );
        let canonical = envelope_wire(&envelope);
        let canonical_json = String::from_utf8(canonical.clone()).unwrap();
        let schema_field = format!(
            "\"schema\":{},",
            serde_json::to_string(&envelope.schema).unwrap()
        );
        let chain_field = format!(
            "\"chain_id\":{},",
            serde_json::to_string(&envelope.chain_id).unwrap()
        );
        let canonical_prefix = format!("{{{schema_field}{chain_field}");
        assert!(canonical_json.starts_with(&canonical_prefix));

        let mut whitespace = vec![b' '];
        whitespace.extend_from_slice(&canonical);
        let reordered = canonical_json
            .replacen(
                &canonical_prefix,
                &format!("{{{chain_field}{schema_field}"),
                1,
            )
            .into_bytes();
        let unknown = canonical_json
            .replacen('{', "{\"unexpected\":true,", 1)
            .into_bytes();
        let duplicate = canonical_json
            .replacen('{', &format!("{{{schema_field}"), 1)
            .into_bytes();

        for raw_tx in [whitespace, reordered, unknown, duplicate] {
            assert!(decode_research_transaction_binding(&raw_tx, EXECUTION_TIMESTAMP_MS).is_err());
        }
        decode_research_transaction_binding(&canonical, EXECUTION_TIMESTAMP_MS).unwrap();
    }

    #[test]
    fn outer_envelope_security_field_drift_is_rejected_even_when_resigned() {
        let signed = signed_research_command();
        let (_, envelope) = research_envelope(
            &signed,
            EXECUTION_TIMESTAMP_MS - 1_000,
            EXECUTION_TIMESTAMP_MS + 1_000,
        );
        let command_id = signed.command_id.to_hex();
        let fingerprint = hex::encode(signed.command_fingerprint());

        let mut wrong_chain = envelope.clone();
        wrong_chain.chain_id = "trnm-other-chain".to_string();
        let mut wrong_command = envelope.clone();
        wrong_command.command_id = hex::encode([0x22; 32]);
        let mut wrong_signer = envelope.clone();
        wrong_signer.signer_id = "did:trnm:other-authority".to_string();
        let mut wrong_role = envelope.clone();
        wrong_role.signer_role = "hepta".to_string();
        let mut wrong_nonce = envelope.clone();
        wrong_nonce.nonce += 1;
        let mut wrong_payload_type = envelope.clone();
        wrong_payload_type.payload_type = "trnm.canonical.tx.v1".to_string();

        let cases = [
            resign_envelope(&wrong_chain, &research_signing_key()),
            resign_envelope(&wrong_command, &research_signing_key()),
            resign_envelope(&wrong_signer, &research_signing_key()),
            resign_envelope(&wrong_role, &research_signing_key()),
            resign_envelope(&wrong_nonce, &research_signing_key()),
            resign_envelope(&wrong_payload_type, &research_signing_key()),
            resign_envelope(&envelope, &SigningKey::from_bytes(&[0x44; 32])),
        ];
        for candidate in cases {
            assert!(verify_research_transaction_binding(
                &envelope_wire(&candidate),
                &signed.chain_id,
                &command_id,
                &fingerprint,
                EXECUTION_TIMESTAMP_MS,
            )
            .is_err());
        }
    }

    #[test]
    fn outer_signature_payload_hash_and_execution_time_are_fail_closed() {
        let signed = signed_research_command();
        let (_, envelope) = research_envelope(
            &signed,
            EXECUTION_TIMESTAMP_MS - 1_000,
            EXECUTION_TIMESTAMP_MS + 1_000,
        );
        let command_id = signed.command_id.to_hex();
        let fingerprint = hex::encode(signed.command_fingerprint());

        let mut bad_signature = envelope.clone();
        let mut signature = hex::decode(&bad_signature.signature_hex).unwrap();
        signature[0] ^= 1;
        bad_signature.signature_hex = hex::encode(signature);
        assert!(verify_research_transaction_binding(
            &envelope_wire(&bad_signature),
            &signed.chain_id,
            &command_id,
            &fingerprint,
            EXECUTION_TIMESTAMP_MS,
        )
        .is_err());

        let payload_hash_field = format!("\"payload_hash_hex\":\"{}\"", envelope.payload_hash_hex);
        let bad_payload_hash = String::from_utf8(envelope_wire(&envelope))
            .unwrap()
            .replacen(
                &payload_hash_field,
                &format!("\"payload_hash_hex\":\"{}\"", hex::encode([0x55; 32])),
                1,
            )
            .into_bytes();
        assert!(verify_research_transaction_binding(
            &bad_payload_hash,
            &signed.chain_id,
            &command_id,
            &fingerprint,
            EXECUTION_TIMESTAMP_MS,
        )
        .is_err());

        assert!(verify_research_transaction_binding(
            &envelope_wire(&envelope),
            &signed.chain_id,
            &command_id,
            &fingerprint,
            envelope.expires_at_unix_ms + 1,
        )
        .is_err());

        let (_, future) = research_envelope(
            &signed,
            EXECUTION_TIMESTAMP_MS + 60_001,
            EXECUTION_TIMESTAMP_MS + 70_000,
        );
        assert!(verify_research_transaction_binding(
            &envelope_wire(&future),
            &signed.chain_id,
            &command_id,
            &fingerprint,
            EXECUTION_TIMESTAMP_MS,
        )
        .is_err());

        verify_research_transaction_binding(
            &envelope_wire(&envelope),
            &signed.chain_id,
            &command_id,
            &fingerprint,
            EXECUTION_TIMESTAMP_MS,
        )
        .unwrap();
    }

    #[test]
    fn execution_header_timestamp_matches_consensus_millisecond_flooring() {
        let timestamp = tendermint::Time::from_unix_timestamp(1_753_449_600, 999_999_999).unwrap();
        assert_eq!(
            consensus_timestamp_ms(&timestamp).unwrap(),
            1_753_449_600_999
        );
        assert!(
            consensus_timestamp_ms(&tendermint::Time::from_unix_timestamp(-1, 0).unwrap()).is_err()
        );
    }

    #[test]
    fn canonical_header_bytes_and_exposed_commitments_are_bound_exactly() {
        let block = TestgenLightBlock::new_default(10).generate().unwrap();
        let mut header = block.signed_header.header;
        header.app_hash = tendermint::AppHash::try_from(vec![5; 32]).unwrap();
        let wire = encode_cometbft_header_v1(&header).unwrap();

        assert_eq!(decode_header(&wire).unwrap(), header);

        let mut forged_hash = wire.clone();
        forged_hash.header_hash_hex = hex::encode([9; 32]);
        assert!(decode_header(&forged_hash).is_err());

        let mut unknown_field = wire;
        unknown_field.header_proto_hex.push_str("f80701");
        assert!(decode_header(&unknown_field).is_err());
    }

    #[test]
    fn result_commitment_rejects_nondeterministic_abci_metadata() {
        let signed = signed_research_command();
        let (_, envelope) = research_envelope(
            &signed,
            EXECUTION_TIMESTAMP_MS - 1_000,
            EXECUTION_TIMESTAMP_MS + 1_000,
        );
        let binding =
            decode_research_transaction_binding(&envelope_wire(&envelope), EXECUTION_TIMESTAMP_MS)
                .unwrap();
        let canonical_result = successful_research_result(&binding, 10_000);
        let canonical = canonical_result.encode_to_vec();
        verify_deterministic_exec_result_bytes(&canonical, &binding).unwrap();

        let with_log = RawExecTxResult {
            log: "node-local diagnostic".to_string(),
            ..canonical_result.clone()
        };
        assert!(
            verify_deterministic_exec_result_bytes(&with_log.encode_to_vec(), &binding).is_err()
        );

        let with_data = RawExecTxResult {
            data: vec![1].into(),
            ..canonical_result.clone()
        };
        assert!(
            verify_deterministic_exec_result_bytes(&with_data.encode_to_vec(), &binding).is_err()
        );

        let with_info = RawExecTxResult {
            info: "node-local info".to_string(),
            ..canonical_result.clone()
        };
        assert!(
            verify_deterministic_exec_result_bytes(&with_info.encode_to_vec(), &binding).is_err()
        );

        let with_codespace = RawExecTxResult {
            codespace: "trnm".to_string(),
            ..canonical_result.clone()
        };
        assert!(
            verify_deterministic_exec_result_bytes(&with_codespace.encode_to_vec(), &binding)
                .is_err()
        );

        let mut with_event = canonical_result.clone();
        with_event.events.push(Default::default());
        assert!(
            verify_deterministic_exec_result_bytes(&with_event.encode_to_vec(), &binding).is_err()
        );

        let failed = RawExecTxResult {
            code: 1,
            ..canonical_result.clone()
        };
        assert!(verify_deterministic_exec_result_bytes(&failed.encode_to_vec(), &binding).is_err());

        let wrong_gas_wanted = RawExecTxResult {
            gas_wanted: 249_999,
            ..canonical_result.clone()
        };
        assert!(verify_deterministic_exec_result_bytes(
            &wrong_gas_wanted.encode_to_vec(),
            &binding
        )
        .is_err());

        let zero_gas_used = RawExecTxResult {
            gas_used: 0,
            ..canonical_result.clone()
        };
        assert!(
            verify_deterministic_exec_result_bytes(&zero_gas_used.encode_to_vec(), &binding)
                .is_err()
        );

        let excessive_gas_used = RawExecTxResult {
            gas_used: 250_001,
            ..canonical_result.clone()
        };
        assert!(verify_deterministic_exec_result_bytes(
            &excessive_gas_used.encode_to_vec(),
            &binding
        )
        .is_err());
    }

    #[test]
    fn jmt_ics23_membership_is_bound_to_exact_apphash_key_and_value() {
        let key = b"namespaced-command-key".to_vec();
        let value = b"authenticated-object-record".to_vec();
        let leaf = LeafOp {
            hash: HashOp::Sha256.into(),
            prehash_key: HashOp::Sha256.into(),
            prehash_value: HashOp::Sha256.into(),
            length: LengthOp::NoPrefix.into(),
            prefix: JMT_LEAF_DOMAIN_SEPARATOR.to_vec(),
        };
        let existence = ExistenceProof {
            key: key.clone(),
            value: value.clone(),
            leaf: Some(leaf),
            path: Vec::new(),
        };
        let commitment = CommitmentProof {
            proof: Some(commitment_proof::Proof::Exist(existence)),
        };
        let mut hasher = Sha256::new();
        hasher.update(JMT_LEAF_DOMAIN_SEPARATOR);
        hasher.update(Sha256::digest(&key));
        hasher.update(Sha256::digest(&value));
        let root: [u8; 32] = hasher.finalize().into();
        let proof_hex = hex::encode(commitment.encode_to_vec());
        let wire = AppHashObjectProofV1 {
            schema: trnm_finality_types::APPHASH_OBJECT_PROOF_SCHEMA_V1.to_string(),
            query_height: 41,
            object_key_hex: hex::encode(&key),
            value_hex: hex::encode(&value),
            proof_op: trnm_finality_types::AppHashProofOpV1 {
                proof_type: trnm_finality_types::COMETBFT_JMT_PROOF_OP_TYPE_V1.to_string(),
                key_hex: hex::encode(&key),
                data_hex: proof_hex.clone(),
            },
            commitment_proof_hex: proof_hex,
        };

        assert_eq!(
            verify_applied_command_membership(&wire, &root).unwrap(),
            value
        );
        assert!(verify_applied_command_membership(&wire, &[9; 32]).is_err());

        let mut wrong_key = wire;
        wrong_key.object_key_hex = hex::encode(b"other-key");
        wrong_key.proof_op.key_hex = wrong_key.object_key_hex.clone();
        assert!(verify_applied_command_membership(&wrong_key, &root).is_err());
    }

    #[test]
    fn multi_path_jmt_ics23_membership_binds_every_inner_node() {
        let key = b"namespaced-multi-path-command-key";
        let value = b"authenticated-multi-path-object-record";
        let siblings = [[0x31; 32], [0x42; 32], [0x53; 32], [0x64; 32]];
        let (proof, root) = jmt_membership_fixture(41, key, value, &siblings);

        let decoded = decode_canonical_message::<CommitmentProof>(
            "commitment_proof_hex",
            &proof.commitment_proof_bytes().unwrap(),
        )
        .unwrap();
        let Some(commitment_proof::Proof::Exist(existence)) = decoded.proof else {
            panic!("fixture must contain a membership proof");
        };
        assert_eq!(existence.path.len(), siblings.len());
        assert_eq!(
            verify_applied_command_membership(&proof, &root).unwrap(),
            value
        );

        let mut wrong_root = root;
        wrong_root[0] ^= 1;
        assert!(verify_applied_command_membership(&proof, &wrong_root).is_err());

        let mut wrong_key = proof.clone();
        wrong_key.object_key_hex = hex::encode(b"other-key");
        wrong_key.proof_op.key_hex = wrong_key.object_key_hex.clone();
        assert!(verify_applied_command_membership(&wrong_key, &root).is_err());

        let mut wrong_value = proof;
        wrong_value.value_hex = hex::encode(b"other-value");
        assert!(verify_applied_command_membership(&wrong_value, &root).is_err());
    }

    #[test]
    fn built_in_applied_command_binding_accepts_exact_record() {
        let (receipt, binding, _) = applied_command_binding_fixture();
        verify_applied_command_binding(&receipt, &binding).unwrap();
    }

    #[test]
    fn applied_command_binding_rejects_wrong_logical_key() {
        let (mut receipt, binding, _) = applied_command_binding_fixture();
        replace_authenticated_applied_command(
            &mut receipt,
            &"11".repeat(32),
            RESEARCH_APPLIED_COMMAND_OBJECT_TYPE_V1,
            1,
            binding.applied_command_value.clone(),
        );

        let error = verify_applied_command_binding(&receipt, &binding).unwrap_err();
        assert!(format!("{error:#}").contains("proof key"));
    }

    #[test]
    fn applied_command_binding_rejects_wrong_object_type() {
        let (mut receipt, binding, _) = applied_command_binding_fixture();
        replace_authenticated_applied_command(
            &mut receipt,
            &binding.applied_command_logical_key,
            "trnm.research.domain-object.v1",
            1,
            binding.applied_command_value.clone(),
        );

        let error = verify_applied_command_binding(&receipt, &binding).unwrap_err();
        assert!(format!("{error:#}").contains("object type mismatch"));
    }

    #[test]
    fn applied_command_binding_rejects_wrong_object_version() {
        let (mut receipt, binding, _) = applied_command_binding_fixture();
        replace_authenticated_applied_command(
            &mut receipt,
            &binding.applied_command_logical_key,
            RESEARCH_APPLIED_COMMAND_OBJECT_TYPE_V1,
            2,
            binding.applied_command_value.clone(),
        );

        let error = verify_applied_command_binding(&receipt, &binding).unwrap_err();
        assert!(format!("{error:#}").contains("object version mismatch"));
    }

    #[test]
    fn applied_command_binding_rejects_wrong_exact_value() {
        let (mut receipt, binding, _) = applied_command_binding_fixture();
        replace_authenticated_applied_command(
            &mut receipt,
            &binding.applied_command_logical_key,
            RESEARCH_APPLIED_COMMAND_OBJECT_TYPE_V1,
            1,
            b"not-the-canonical-applied-command".to_vec(),
        );

        let error = verify_applied_command_binding(&receipt, &binding).unwrap_err();
        assert!(format!("{error:#}").contains("object value mismatch"));
    }

    #[test]
    fn applied_command_binding_rejects_wrong_record_command_id() {
        let (mut receipt, binding, signed) = applied_command_binding_fixture();
        let mut forged = expected_applied_command_record(&signed);
        forged.command_id = external_key("trnm.command", "other-command");
        replace_authenticated_applied_command(
            &mut receipt,
            &binding.applied_command_logical_key,
            RESEARCH_APPLIED_COMMAND_OBJECT_TYPE_V1,
            1,
            forged.canonical_bytes(),
        );

        let error = verify_applied_command_binding(&receipt, &binding).unwrap_err();
        assert!(format!("{error:#}").contains("object value mismatch"));
    }

    #[test]
    fn applied_command_binding_rejects_wrong_fingerprint() {
        let (mut receipt, binding, signed) = applied_command_binding_fixture();
        let mut forged = expected_applied_command_record(&signed);
        forged.fingerprint[0] ^= 1;
        replace_authenticated_applied_command(
            &mut receipt,
            &binding.applied_command_logical_key,
            RESEARCH_APPLIED_COMMAND_OBJECT_TYPE_V1,
            1,
            forged.canonical_bytes(),
        );

        let error = verify_applied_command_binding(&receipt, &binding).unwrap_err();
        assert!(format!("{error:#}").contains("object value mismatch"));
    }

    #[test]
    fn applied_command_binding_rejects_wrong_primary_object_ref() {
        let (mut receipt, binding, signed) = applied_command_binding_fixture();
        let mut forged = expected_applied_command_record(&signed);
        forged.primary_object_ref.key = external_key("nakama.commitment", "other-commitment");
        replace_authenticated_applied_command(
            &mut receipt,
            &binding.applied_command_logical_key,
            RESEARCH_APPLIED_COMMAND_OBJECT_TYPE_V1,
            1,
            forged.canonical_bytes(),
        );

        let error = verify_applied_command_binding(&receipt, &binding).unwrap_err();
        assert!(format!("{error:#}").contains("object value mismatch"));
    }
}
