use std::{
    collections::BTreeSet,
    env, fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tendermint::{block, validator};
use tendermint_proto::v0_38::{
    abci::{Event, EventAttribute, ExecTxResult},
    types::{SignedHeader as RawSignedHeader, ValidatorSet as RawValidatorSet},
};
use trnm_finality_types::{
    comet_tx_hash, AppHashObjectProofV1, AppHashProofOpV1, SignedCommandEnvelopeV1,
    APPHASH_OBJECT_PROOF_SCHEMA_V1, COMETBFT_JMT_PROOF_OP_TYPE_V1,
};
use trnm_finality_verifier::{
    assemble_cometbft_apphash_finality_receipt_v2, encode_cometbft_header_v1,
    encode_cometbft_trust_anchor_v1, verify_cometbft_apphash_finality_receipt_v2_with_trust_anchor,
    CometBftReceiptAssemblyInputV2, ReceiptV2VerificationOutcome, ValidatedCometBftTrustAnchorV1,
    VerifiedCometBftDomainCommandV2,
};
use trnm_protocol::{
    paper_raid_finality_applied_command_key, paper_raid_finality_applied_command_key_v3,
    research_applied_command_key, CanonicalPaperRaidFinalityTxV2, CanonicalPaperRaidFinalityTxV3,
    CanonicalResearchTxV1, CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V2,
    CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V3, CANONICAL_RESEARCH_TX_PAYLOAD_TYPE_V1,
};
use trnm_research_protocol::{
    AuthorityRole, CanonicalCbor, ExternalKey, MatchEvidenceCommitmentV1,
    PaperRaidFinalityCommitmentV2, PaperRaidFinalityCommitmentV3, ResearchCommandV1,
    SignedPaperRaidFinalityCommandV2, SignedPaperRaidFinalityCommandV3, SignedResearchCommandV1,
};

const MAX_RPC_JSON_BYTES: u64 = 256 * 1024 * 1024;
const MAX_BINARY_EVIDENCE_BYTES: u64 = 32 * 1024 * 1024;
const TRUSTING_PERIOD_SECONDS: u64 = 24 * 60 * 60;
const CLOCK_DRIFT_SECONDS: u64 = 10;

fn usage(program: &str) -> anyhow::Error {
    anyhow!(
        "usage:\n  {program} fixture-tx PRIVATE_KEY OUTPUT_TX\n  {program} sign-and-wrap SIGNING_INPUT PRIVATE_KEY SIGNED_COMMAND_OUTPUT OUTPUT_TX\n  {program} paper-raid-v2-sign-and-wrap SIGNING_INPUT PRIVATE_KEY SIGNED_COMMAND_OUTPUT OUTPUT_TX\n  {program} paper-raid-v3-pre-v7-artifact SIGNING_INPUT PRIVATE_KEY SIGNED_COMMAND_OUTPUT OUTPUT_TX\n  {program} assemble-and-verify EVIDENCE_DIR RECEIPT_OUTPUT TRUSTED_EXECUTION_HEADER_HASH_HEX [TRUST_ANCHOR_OUTPUT]"
    )
}

fn read_bounded(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    let path_metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect evidence file {}", path.display()))?;
    ensure!(
        path_metadata.file_type().is_file() && !path_metadata.file_type().is_symlink(),
        "evidence path is not a regular non-symlink file: {}",
        path.display()
    );
    let file =
        fs::File::open(path).with_context(|| format!("open evidence file {}", path.display()))?;
    let opened_metadata = file
        .metadata()
        .with_context(|| format!("inspect opened evidence file {}", path.display()))?;
    ensure!(
        opened_metadata.file_type().is_file()
            && opened_metadata.dev() == path_metadata.dev()
            && opened_metadata.ino() == path_metadata.ino(),
        "evidence path changed while it was opened: {}",
        path.display()
    );
    ensure!(
        opened_metadata.len() <= max_bytes,
        "evidence file exceeds the {max_bytes}-byte limit: {}",
        path.display()
    );
    let mut bytes = Vec::with_capacity(
        usize::try_from(opened_metadata.len()).context("evidence length exceeds usize")?,
    );
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("read evidence file {}", path.display()))?;
    ensure!(
        u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= max_bytes,
        "evidence file grew beyond the {max_bytes}-byte limit while reading: {}",
        path.display()
    );
    Ok(bytes)
}

fn read_json(path: &Path) -> Result<Value> {
    let bytes = read_bounded(path, MAX_RPC_JSON_BYTES)?;
    serde_json::from_slice(&bytes).with_context(|| format!("decode JSON {}", path.display()))
}

fn required<'a>(value: &'a Value, pointer: &str) -> Result<&'a Value> {
    value
        .pointer(pointer)
        .with_context(|| format!("missing JSON pointer {pointer}"))
}

fn required_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    required(value, pointer)?
        .as_str()
        .with_context(|| format!("JSON pointer {pointer} is not a string"))
}

fn parse_u64(value: &Value, label: &str) -> Result<u64> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .with_context(|| format!("{label} is not an unsigned integer")),
        Value::String(text) => {
            let parsed = text
                .parse::<u64>()
                .with_context(|| format!("parse {label}"))?;
            ensure!(
                parsed.to_string() == text.as_str(),
                "{label} is not canonical decimal"
            );
            Ok(parsed)
        }
        _ => Err(anyhow!("{label} is not an unsigned integer")),
    }
}

fn parse_i64(value: Option<&Value>, label: &str) -> Result<i64> {
    let Some(value) = value else {
        return Ok(0);
    };
    match value {
        Value::Number(number) => number
            .as_i64()
            .with_context(|| format!("{label} is not a signed integer")),
        Value::String(text) => {
            let parsed = text
                .parse::<i64>()
                .with_context(|| format!("parse {label}"))?;
            ensure!(
                parsed.to_string() == text.as_str(),
                "{label} is not canonical decimal"
            );
            Ok(parsed)
        }
        _ => Err(anyhow!("{label} is not a signed integer")),
    }
}

fn parse_u32(value: Option<&Value>, label: &str) -> Result<u32> {
    let parsed = parse_i64(value, label)?;
    u32::try_from(parsed).with_context(|| format!("{label} is outside u32"))
}

fn parse_bool(value: Option<&Value>, label: &str) -> Result<bool> {
    match value {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(Value::String(value)) if value == "true" => Ok(true),
        Some(Value::String(value)) if value == "false" => Ok(false),
        _ => Err(anyhow!("{label} is not a boolean")),
    }
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .with_context(|| format!("{label} is not a JSON object"))
}

fn ensure_exact_fields(object: &Map<String, Value>, allowed: &[&str], label: &str) -> Result<()> {
    let allowed = allowed.iter().copied().collect::<BTreeSet<_>>();
    for key in object.keys() {
        ensure!(
            allowed.contains(key.as_str()),
            "{label} contains unsupported field {key}"
        );
    }
    Ok(())
}

fn optional_string(object: &Map<String, Value>, field: &str, label: &str) -> Result<String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(value)) => Ok(value.clone()),
        _ => Err(anyhow!("{label}.{field} is not a string")),
    }
}

fn optional_base64(object: &Map<String, Value>, field: &str, label: &str) -> Result<Vec<u8>> {
    let encoded = optional_string(object, field, label)?;
    if encoded.is_empty() {
        return Ok(Vec::new());
    }
    BASE64
        .decode(encoded.as_bytes())
        .with_context(|| format!("decode {label}.{field} base64"))
}

fn rpc_exec_result(value: &Value, index: usize) -> Result<ExecTxResult> {
    let label = format!("tx result {index}");
    let result_object = object(value, &label)?;
    ensure_exact_fields(
        result_object,
        &[
            "code",
            "data",
            "log",
            "info",
            "gas_wanted",
            "gas_used",
            "events",
            "codespace",
        ],
        &label,
    )?;
    let events = match result_object.get("events") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(events)) => events
            .iter()
            .enumerate()
            .map(|(event_index, event)| {
                let event_label = format!("{label} event {event_index}");
                let event = object(event, &event_label)?;
                ensure_exact_fields(event, &["type", "attributes"], &event_label)?;
                let event_type = event
                    .get("type")
                    .and_then(Value::as_str)
                    .with_context(|| format!("{event_label}.type is not a string"))?;
                let attributes = match event.get("attributes") {
                    None | Some(Value::Null) => Vec::new(),
                    Some(Value::Array(attributes)) => attributes
                        .iter()
                        .enumerate()
                        .map(|(attribute_index, attribute)| {
                            let attribute_label =
                                format!("{event_label} attribute {attribute_index}");
                            let attribute = object(attribute, &attribute_label)?;
                            ensure_exact_fields(
                                attribute,
                                &["key", "value", "index"],
                                &attribute_label,
                            )?;
                            Ok(EventAttribute {
                                key: attribute
                                    .get("key")
                                    .and_then(Value::as_str)
                                    .with_context(|| {
                                        format!("{attribute_label}.key is not a string")
                                    })?
                                    .to_string(),
                                value: attribute
                                    .get("value")
                                    .and_then(Value::as_str)
                                    .with_context(|| {
                                        format!("{attribute_label}.value is not a string")
                                    })?
                                    .to_string(),
                                index: parse_bool(
                                    attribute.get("index"),
                                    &format!("{attribute_label}.index"),
                                )?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                    _ => return Err(anyhow!("{event_label}.attributes is not an array")),
                };
                Ok(Event {
                    r#type: event_type.to_string(),
                    attributes,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err(anyhow!("{label}.events is not an array")),
    };
    Ok(ExecTxResult {
        code: parse_u32(result_object.get("code"), &format!("{label}.code"))?,
        data: optional_base64(result_object, "data", &label)?.into(),
        log: optional_string(result_object, "log", &label)?,
        info: optional_string(result_object, "info", &label)?,
        gas_wanted: parse_i64(
            result_object.get("gas_wanted"),
            &format!("{label}.gas_wanted"),
        )?,
        gas_used: parse_i64(result_object.get("gas_used"), &format!("{label}.gas_used"))?,
        events,
        codespace: optional_string(result_object, "codespace", &label)?,
    })
}

fn decode_block_transactions(block: &Value) -> Result<Vec<Vec<u8>>> {
    let transactions = required(block, "/result/block/data/txs")?
        .as_array()
        .context("block transactions are not an array")?;
    ensure!(
        !transactions.is_empty(),
        "execution block has no transactions"
    );
    transactions
        .iter()
        .enumerate()
        .map(|(index, encoded)| {
            let encoded = encoded
                .as_str()
                .with_context(|| format!("block transaction {index} is not base64 text"))?;
            BASE64
                .decode(encoded.as_bytes())
                .with_context(|| format!("decode block transaction {index}"))
        })
        .collect()
}

fn decode_results(results: &Value) -> Result<Vec<Vec<u8>>> {
    let values = results
        .pointer("/result/txs_results")
        .or_else(|| results.pointer("/result/tx_results"))
        .context("block_results has no transaction results")?
        .as_array()
        .context("block transaction results are not an array")?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let result = rpc_exec_result(value, index)?;
            // Match CometBFT v0.38 `types.deterministicExecTxResult` exactly.
            // Log, info, events, and codespace are RPC-visible but are not
            // committed by LastResultsHash.
            Ok(ExecTxResult {
                code: result.code,
                data: result.data,
                gas_wanted: result.gas_wanted,
                gas_used: result.gas_used,
                ..Default::default()
            }
            .encode_to_vec())
        })
        .collect()
}

fn decode_proof(proof: &Value) -> Result<AppHashObjectProofV1> {
    let response = required(proof, "/result/response")?;
    let query_height = parse_u64(
        response
            .get("height")
            .context("ABCI response height is absent")?,
        "ABCI response height",
    )?;
    let key = BASE64
        .decode(required_string(proof, "/result/response/key")?)
        .context("decode ABCI response key")?;
    let value = BASE64
        .decode(required_string(proof, "/result/response/value")?)
        .context("decode ABCI response value")?;
    let proof_ops = response
        .pointer("/proofOps/ops")
        .or_else(|| response.pointer("/proof_ops/ops"))
        .and_then(Value::as_array)
        .context("ABCI response proof ops are absent")?;
    ensure!(
        proof_ops.len() == 1,
        "ABCI response must contain one proof op"
    );
    let proof_type = required_string(&proof_ops[0], "/type")?;
    ensure!(
        proof_type == COMETBFT_JMT_PROOF_OP_TYPE_V1,
        "ABCI response proof type mismatch"
    );
    let proof_key = BASE64
        .decode(required_string(&proof_ops[0], "/key")?)
        .context("decode proof op key")?;
    ensure!(proof_key == key, "ABCI response/proof-op key mismatch");
    let proof_data = BASE64
        .decode(required_string(&proof_ops[0], "/data")?)
        .context("decode proof op data")?;
    ensure!(
        !key.is_empty() && !value.is_empty() && !proof_data.is_empty(),
        "ABCI membership evidence must be non-empty"
    );
    let proof_hex = hex::encode(proof_data);
    let key_hex = hex::encode(key);
    Ok(AppHashObjectProofV1 {
        schema: APPHASH_OBJECT_PROOF_SCHEMA_V1.to_string(),
        query_height,
        object_key_hex: key_hex.clone(),
        value_hex: hex::encode(value),
        proof_op: AppHashProofOpV1 {
            proof_type: proof_type.to_string(),
            key_hex,
            data_hex: proof_hex.clone(),
        },
        commitment_proof_hex: proof_hex,
    })
}

fn validate_sha256sums(evidence_dir: &Path) -> Result<()> {
    const REQUIRED_EVIDENCE_FILES: [&str; 13] = [
        "applied-command-proof.json",
        "block-h-plus-1.json",
        "block-h.json",
        "block-results-h.json",
        "commit-h-plus-1.json",
        "expected-proof-key.bin",
        "ics23-proof.bin",
        "manifest.json",
        "proof-key.bin",
        "proof-value.bin",
        "target-raw-tx.bin",
        "target-result.rpc.json",
        "validators-h-plus-1.json",
    ];
    let bytes = read_bounded(&evidence_dir.join("SHA256SUMS"), 64 * 1024)?;
    let text = std::str::from_utf8(&bytes).context("SHA256SUMS is not UTF-8")?;
    ensure!(!text.is_empty(), "SHA256SUMS is empty");
    let mut observed = BTreeSet::new();
    for (line_index, line) in text.lines().enumerate() {
        let (expected, name) = line
            .split_once("  ")
            .with_context(|| format!("malformed SHA256SUMS line {}", line_index + 1))?;
        ensure!(
            expected.len() == 64
                && expected.bytes().all(|byte| byte.is_ascii_hexdigit())
                && expected == expected.to_ascii_lowercase(),
            "malformed SHA-256 on line {}",
            line_index + 1
        );
        ensure!(
            !name.is_empty()
                && !name.contains('/')
                && name != "."
                && name != ".."
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)),
            "unsafe SHA256SUMS filename on line {}",
            line_index + 1
        );
        ensure!(
            observed.insert(name),
            "duplicate SHA256SUMS filename on line {}",
            line_index + 1
        );
        let file = read_bounded(&evidence_dir.join(name), MAX_RPC_JSON_BYTES)?;
        ensure!(
            hex::encode(comet_tx_hash(&file)) == expected,
            "SHA-256 mismatch for {name}"
        );
    }
    let required = REQUIRED_EVIDENCE_FILES.into_iter().collect::<BTreeSet<_>>();
    ensure!(
        observed == required,
        "SHA256SUMS must cover the exact canonical Receipt V2 evidence set"
    );
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure!(
        path.is_absolute() && path.parent().is_some(),
        "receipt output must be an absolute non-root path"
    );
    let parent = path.parent().unwrap();
    ensure!(parent.is_dir(), "receipt output parent does not exist");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("create receipt output {}", path.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn now_unix_ms() -> Result<u64> {
    u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())
        .context("current Unix time exceeds u64 milliseconds")
}

fn external_key(namespace: &str, value: &str) -> Result<ExternalKey> {
    ExternalKey::from_external_id(namespace, value)
        .map_err(|error| anyhow!("create fixture {namespace} key: {error}"))
}

fn read_signing_key(private_key: &Path) -> Result<SigningKey> {
    let encoded_key = read_bounded(private_key, 256)?;
    let encoded_key = std::str::from_utf8(&encoded_key).context("private key is not UTF-8")?;
    let encoded_key = match encoded_key.as_bytes() {
        bytes if bytes.len() == 64 => encoded_key,
        bytes if bytes.len() == 65 && bytes[64] == b'\n' => &encoded_key[..64],
        _ => "",
    };
    ensure!(
        encoded_key.len() == 64
            && encoded_key.bytes().all(|byte| byte.is_ascii_hexdigit())
            && encoded_key == encoded_key.to_ascii_lowercase(),
        "private key must be one lowercase-hex Ed25519 seed plus an optional newline"
    );
    let seed: [u8; 32] = hex::decode(encoded_key)?
        .try_into()
        .map_err(|_| anyhow!("private key must encode 32 bytes"))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn decode_canonical_hex(
    label: &str,
    value: &str,
    exact_bytes: Option<usize>,
    max: usize,
) -> Result<Vec<u8>> {
    ensure!(
        !value.is_empty() && value.len().is_multiple_of(2) && value.len() <= max.saturating_mul(2),
        "{label} is outside the canonical hex byte limit"
    );
    let bytes = hex::decode(value).with_context(|| format!("decode {label}"))?;
    ensure!(
        exact_bytes.is_none_or(|expected| bytes.len() == expected)
            && bytes.len() <= max
            && hex::encode(&bytes) == value,
        "{label} is not canonical lowercase hex"
    );
    Ok(bytes)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignAndWrapInputV1 {
    schema: String,
    chain_id: String,
    command_namespace: String,
    command_external_id: String,
    signer_did: String,
    signer_role: AuthorityRole,
    nonce: u64,
    max_gas: u64,
    fee_limit: u128,
    command: ResearchCommandV1,
}

#[derive(Serialize)]
struct SignedCommandOutputV1 {
    protocol: &'static str,
    signed_command: SignedResearchCommandV1,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaperRaidSignAndWrapInputV2 {
    schema: String,
    chain_id: String,
    command_id_hex: String,
    signer_did: String,
    nonce: u64,
    max_gas: u64,
    fee_limit: u128,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    commitment_cbor_hex: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PaperRaidSignedCommandOutputV2 {
    schema: String,
    chain_id: String,
    command_id: String,
    signer_did: String,
    nonce: u64,
    public_key_hex: String,
    signed_command_cbor_hex: String,
    canonical_transaction_hex: String,
    command_fingerprint_hex: String,
    commitment_id: String,
    commitment_hash_hex: String,
    applied_command_logical_key: String,
    outer_envelope_payload_hash_hex: String,
    comet_tx_hash_hex: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaperRaidSignAndWrapInputV3 {
    schema: String,
    chain_id: String,
    command_id_hex: String,
    signer_did: String,
    nonce: u64,
    max_gas: u64,
    fee_limit: u128,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    commitment_cbor_hex: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PaperRaidSignedCommandOutputV3 {
    schema: String,
    required_consensus_app_version: u64,
    broadcastable_on_app_v6: bool,
    chain_id: String,
    command_id: String,
    signer_did: String,
    nonce: u64,
    public_key_hex: String,
    signed_command_cbor_hex: String,
    canonical_transaction_hex: String,
    command_fingerprint_hex: String,
    commitment_id: String,
    commitment_hash_hex: String,
    applied_command_logical_key: String,
    outer_envelope_payload_hash_hex: String,
    comet_tx_hash_hex: String,
}

fn signer_role_name(role: AuthorityRole) -> &'static str {
    match role {
        AuthorityRole::NakamaAuthority => "nakama",
        AuthorityRole::HeptaAuthority => "hepta",
    }
}

fn sign_and_wrap(
    signing_input: &Path,
    private_key: &Path,
    signed_command_output: &Path,
    transaction_output: &Path,
) -> Result<()> {
    ensure!(
        signed_command_output != transaction_output,
        "signed command and transaction outputs must be distinct"
    );
    let input: SignAndWrapInputV1 =
        serde_json::from_slice(&read_bounded(signing_input, MAX_BINARY_EVIDENCE_BYTES)?)
            .with_context(|| format!("decode signing input {}", signing_input.display()))?;
    ensure!(
        input.schema == "trnm_research_sign_and_wrap_input_v1",
        "unsupported signing input schema"
    );
    let signing_key = read_signing_key(private_key)?;
    let command_id =
        ExternalKey::from_external_id(&input.command_namespace, &input.command_external_id)
            .map_err(|error| anyhow!("create command external key: {error}"))?;
    let signed = SignedResearchCommandV1::sign(
        input.chain_id,
        command_id,
        input.signer_did,
        input.signer_role,
        input.nonce,
        input.command,
        &signing_key,
    )
    .context("sign research command")?;
    let research_tx =
        CanonicalResearchTxV1::from_signed_command(&signed, input.max_gas, input.fee_limit)
            .context("build canonical research transaction")?;
    let issued_at_unix_ms = now_unix_ms()?;
    let envelope = SignedCommandEnvelopeV1::sign(
        signed.chain_id.clone(),
        signed.command_id.to_hex(),
        signed.signer_did.clone(),
        signer_role_name(signed.signer_role),
        signed.nonce,
        issued_at_unix_ms,
        issued_at_unix_ms.saturating_add(300_000),
        CANONICAL_RESEARCH_TX_PAYLOAD_TYPE_V1,
        &research_tx.canonical_bytes()?,
        &signing_key,
    )?;
    let signed_output = serde_json::to_vec(&SignedCommandOutputV1 {
        protocol: "hepta_signed_trnm_command_v1",
        signed_command: signed.clone(),
    })?;
    write_new(signed_command_output, &signed_output)?;
    if let Err(error) = write_new(transaction_output, &envelope.to_wire_bytes()?) {
        fs::remove_file(signed_command_output).with_context(|| {
            format!(
                "remove incomplete signed command output {}",
                signed_command_output.display()
            )
        })?;
        return Err(error);
    }
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema":"trnm_research_sign_and_wrap_result_v1",
            "signed_command_path":signed_command_output,
            "transaction_path":transaction_output,
            "command_id":signed.command_id.to_hex(),
            "command_fingerprint_hex":hex::encode(signed.command_fingerprint()),
            "applied_command_logical_key":research_applied_command_key(signed.command_id)?,
            "public_key_hex":hex::encode(signing_key.verifying_key().to_bytes())
        }))?
    );
    Ok(())
}

fn paper_raid_v2_sign_and_wrap(
    signing_input: &Path,
    private_key: &Path,
    signed_command_output: &Path,
    transaction_output: &Path,
) -> Result<()> {
    ensure!(
        signed_command_output != transaction_output,
        "signed command and transaction outputs must be distinct"
    );
    let input: PaperRaidSignAndWrapInputV2 =
        serde_json::from_slice(&read_bounded(signing_input, MAX_BINARY_EVIDENCE_BYTES)?)
            .with_context(|| format!("decode signing input {}", signing_input.display()))?;
    ensure!(
        input.schema == "trnm_paper_raid_finality_sign_and_wrap_input_v2",
        "unsupported Paper Raid signing input schema"
    );
    ensure!(
        input.expires_at_unix_ms > input.issued_at_unix_ms
            && input
                .expires_at_unix_ms
                .saturating_sub(input.issued_at_unix_ms)
                <= 300_000,
        "Paper Raid outer-envelope lifetime must be 1..=300000 milliseconds"
    );
    let command_id: [u8; 32] =
        decode_canonical_hex("command_id_hex", &input.command_id_hex, Some(32), 32)?
            .try_into()
            .map_err(|_| anyhow!("command_id_hex must encode 32 bytes"))?;
    let commitment_bytes = decode_canonical_hex(
        "commitment_cbor_hex",
        &input.commitment_cbor_hex,
        None,
        256 * 1024,
    )?;
    let commitment = PaperRaidFinalityCommitmentV2::from_canonical_bytes(&commitment_bytes)
        .context("decode canonical Paper Raid finality commitment")?;
    ensure!(
        !commitment.score_eligible
            && !commitment.ranking_eligible
            && !commitment.reward_eligible
            && !commitment.economic_eligible,
        "Paper Raid candidate signing keeps all settlement eligibility locked"
    );
    let signing_key = read_signing_key(private_key)?;
    let signed = SignedPaperRaidFinalityCommandV2::sign(
        input.chain_id,
        ExternalKey::from_bytes(command_id),
        input.signer_did,
        input.nonce,
        commitment,
        &signing_key,
    )
    .context("sign Paper Raid finality command")?;
    let canonical_tx = CanonicalPaperRaidFinalityTxV2::from_signed_command(
        &signed,
        input.max_gas,
        input.fee_limit,
    )
    .context("build canonical Paper Raid finality transaction")?;
    let canonical_tx_bytes = canonical_tx.canonical_bytes()?;
    let envelope = SignedCommandEnvelopeV1::sign(
        signed.chain_id.clone(),
        signed.command_id.to_hex(),
        signed.signer_did.clone(),
        "hepta",
        signed.nonce,
        input.issued_at_unix_ms,
        input.expires_at_unix_ms,
        CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V2,
        &canonical_tx_bytes,
        &signing_key,
    )?;
    envelope
        .validate_at(&signed.chain_id, now_unix_ms()?)
        .context("validate Paper Raid outer envelope against the current clock")?;
    let transaction_bytes = envelope.to_wire_bytes()?;
    let signed_output = serde_json::to_vec(&PaperRaidSignedCommandOutputV2 {
        schema: "trnm_paper_raid_signed_command_output_v2".to_string(),
        chain_id: signed.chain_id.clone(),
        command_id: signed.command_id.to_hex(),
        signer_did: signed.signer_did.clone(),
        nonce: signed.nonce,
        public_key_hex: hex::encode(signed.public_key),
        signed_command_cbor_hex: hex::encode(signed.canonical_bytes()),
        canonical_transaction_hex: hex::encode(&canonical_tx_bytes),
        command_fingerprint_hex: hex::encode(signed.command_fingerprint()),
        commitment_id: signed.commitment.commitment_id.to_hex(),
        commitment_hash_hex: hex::encode(signed.payload_hash()),
        applied_command_logical_key: paper_raid_finality_applied_command_key(signed.command_id)?,
        outer_envelope_payload_hash_hex: envelope.payload_hash_hex.clone(),
        comet_tx_hash_hex: hex::encode(comet_tx_hash(&transaction_bytes)),
    })?;
    write_new(signed_command_output, &signed_output)?;
    if let Err(error) = write_new(transaction_output, &transaction_bytes) {
        fs::remove_file(signed_command_output).with_context(|| {
            format!(
                "remove incomplete Paper Raid signed command output {}",
                signed_command_output.display()
            )
        })?;
        return Err(error);
    }
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema":"trnm_paper_raid_finality_sign_and_wrap_result_v2",
            "signed_command_path":signed_command_output,
            "transaction_path":transaction_output,
            "command_id":signed.command_id.to_hex(),
            "command_fingerprint_hex":hex::encode(signed.command_fingerprint()),
            "commitment_id":signed.commitment.commitment_id.to_hex(),
            "commitment_hash_hex":hex::encode(signed.payload_hash()),
            "applied_command_logical_key":paper_raid_finality_applied_command_key(signed.command_id)?,
            "public_key_hex":hex::encode(signing_key.verifying_key().to_bytes()),
            "comet_tx_hash_hex":hex::encode(comet_tx_hash(&transaction_bytes))
        }))?
    );
    Ok(())
}

fn paper_raid_v3_pre_v7_artifact(
    signing_input: &Path,
    private_key: &Path,
    signed_command_output: &Path,
    transaction_output: &Path,
) -> Result<()> {
    ensure!(
        signed_command_output != transaction_output,
        "signed command and transaction outputs must be distinct"
    );
    let input: PaperRaidSignAndWrapInputV3 =
        serde_json::from_slice(&read_bounded(signing_input, MAX_BINARY_EVIDENCE_BYTES)?)
            .with_context(|| format!("decode signing input {}", signing_input.display()))?;
    ensure!(
        input.schema == "trnm_paper_raid_finality_pre_v7_artifact_input_v3",
        "unsupported Paper Raid V3 pre-v7 artifact input schema"
    );
    ensure!(
        input.expires_at_unix_ms > input.issued_at_unix_ms
            && input
                .expires_at_unix_ms
                .saturating_sub(input.issued_at_unix_ms)
                <= 300_000,
        "Paper Raid V3 outer-envelope lifetime must be 1..=300000 milliseconds"
    );
    let command_id: [u8; 32] =
        decode_canonical_hex("command_id_hex", &input.command_id_hex, Some(32), 32)?
            .try_into()
            .map_err(|_| anyhow!("command_id_hex must encode 32 bytes"))?;
    let commitment_bytes = decode_canonical_hex(
        "commitment_cbor_hex",
        &input.commitment_cbor_hex,
        None,
        256 * 1024,
    )?;
    let commitment = PaperRaidFinalityCommitmentV3::from_canonical_bytes(&commitment_bytes)
        .context("decode canonical Paper Raid V3 finality commitment")?;
    ensure!(
        !commitment.score_eligible
            && !commitment.ranking_eligible
            && !commitment.reward_eligible
            && !commitment.economic_eligible,
        "Paper Raid V3 candidate signing keeps all settlement eligibility locked"
    );
    let signing_key = read_signing_key(private_key)?;
    let signed = SignedPaperRaidFinalityCommandV3::sign(
        input.chain_id,
        ExternalKey::from_bytes(command_id),
        input.signer_did,
        input.nonce,
        commitment,
        &signing_key,
    )
    .context("sign Paper Raid V3 finality command")?;
    let canonical_tx = CanonicalPaperRaidFinalityTxV3::from_signed_command(
        &signed,
        input.max_gas,
        input.fee_limit,
    )
    .context("build canonical Paper Raid V3 finality transaction")?;
    let canonical_tx_bytes = canonical_tx.canonical_bytes()?;
    let envelope = SignedCommandEnvelopeV1::sign(
        signed.chain_id.clone(),
        signed.command_id.to_hex(),
        signed.signer_did.clone(),
        "hepta",
        signed.nonce,
        input.issued_at_unix_ms,
        input.expires_at_unix_ms,
        CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V3,
        &canonical_tx_bytes,
        &signing_key,
    )?;
    envelope
        .validate_at(&signed.chain_id, now_unix_ms()?)
        .context("validate Paper Raid V3 outer envelope against the current clock")?;
    let transaction_bytes = envelope.to_wire_bytes()?;
    let signed_output = serde_json::to_vec(&PaperRaidSignedCommandOutputV3 {
        schema: "trnm_paper_raid_pre_v7_signed_command_artifact_v3".to_string(),
        required_consensus_app_version: 7,
        broadcastable_on_app_v6: false,
        chain_id: signed.chain_id.clone(),
        command_id: signed.command_id.to_hex(),
        signer_did: signed.signer_did.clone(),
        nonce: signed.nonce,
        public_key_hex: hex::encode(signed.public_key),
        signed_command_cbor_hex: hex::encode(signed.canonical_bytes()),
        canonical_transaction_hex: hex::encode(&canonical_tx_bytes),
        command_fingerprint_hex: hex::encode(signed.command_fingerprint()),
        commitment_id: signed.commitment.commitment_id.to_hex(),
        commitment_hash_hex: hex::encode(signed.payload_hash()),
        applied_command_logical_key: paper_raid_finality_applied_command_key_v3(signed.command_id)?,
        outer_envelope_payload_hash_hex: envelope.payload_hash_hex.clone(),
        comet_tx_hash_hex: hex::encode(comet_tx_hash(&transaction_bytes)),
    })?;
    write_new(signed_command_output, &signed_output)?;
    if let Err(error) = write_new(transaction_output, &transaction_bytes) {
        fs::remove_file(signed_command_output).with_context(|| {
            format!(
                "remove incomplete Paper Raid V3 signed command output {}",
                signed_command_output.display()
            )
        })?;
        return Err(error);
    }
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema":"trnm_paper_raid_finality_pre_v7_artifact_result_v3",
            "required_consensus_app_version":7,
            "broadcastable_on_app_v6":false,
            "status":"pre_v7_offline_artifact_only",
            "signed_command_path":signed_command_output,
            "transaction_path":transaction_output,
            "command_id":signed.command_id.to_hex(),
            "command_fingerprint_hex":hex::encode(signed.command_fingerprint()),
            "commitment_id":signed.commitment.commitment_id.to_hex(),
            "commitment_hash_hex":hex::encode(signed.payload_hash()),
            "applied_command_logical_key":paper_raid_finality_applied_command_key_v3(signed.command_id)?,
            "public_key_hex":hex::encode(signing_key.verifying_key().to_bytes()),
            "comet_tx_hash_hex":hex::encode(comet_tx_hash(&transaction_bytes))
        }))?
    );
    Ok(())
}

fn fixture_tx(private_key: &Path, output: &Path) -> Result<()> {
    let signing_key = read_signing_key(private_key)?;
    let signed = SignedResearchCommandV1::sign(
        "trnm-comet-spike".to_string(),
        external_key("trnm.command", "single-node-receipt-v2")?,
        "did:trnm:nakama-authority".to_string(),
        AuthorityRole::NakamaAuthority,
        1,
        ResearchCommandV1::MatchEvidenceCommitment(MatchEvidenceCommitmentV1 {
            commitment_id: external_key("nakama.commitment", "single-node-receipt-v2-commitment")?,
            match_id: external_key("nakama.match", "single-node-receipt-v2-match")?,
            challenge_id: external_key("hepta.challenge", "single-node-receipt-v2-challenge")?,
            event_root: [0x11; 32],
            roster_root: [0x22; 32],
            ruleset_hash: [0x33; 32],
            dataset_hash: [0x44; 32],
            archive_hash: [0x55; 32],
            event_count: 3,
            completed_at_unix_s: 1_753_449_600,
        }),
        &signing_key,
    )?;
    let research_tx = CanonicalResearchTxV1::from_signed_command(&signed, 100_000, 100_000)?;
    let issued_at_unix_ms = now_unix_ms()?;
    let envelope = SignedCommandEnvelopeV1::sign(
        signed.chain_id.clone(),
        signed.command_id.to_hex(),
        signed.signer_did.clone(),
        "nakama",
        signed.nonce,
        issued_at_unix_ms,
        issued_at_unix_ms.saturating_add(300_000),
        CANONICAL_RESEARCH_TX_PAYLOAD_TYPE_V1,
        &research_tx.canonical_bytes()?,
        &signing_key,
    )?;
    write_new(output, &envelope.to_wire_bytes()?)?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema":"trnm_research_receipt_v2_fixture_tx_v1",
            "transaction_path":output,
            "command_id":signed.command_id.to_hex(),
            "applied_command_logical_key":research_applied_command_key(signed.command_id)?,
            "public_key_hex":hex::encode(signing_key.verifying_key().to_bytes())
        }))?
    );
    Ok(())
}

fn assemble_and_verify(
    evidence_dir: &Path,
    receipt_output: &Path,
    trusted_execution_header_hash_hex: &str,
    trust_anchor_output: Option<&Path>,
) -> Result<()> {
    if let Some(trust_anchor_output) = trust_anchor_output {
        ensure!(
            receipt_output != trust_anchor_output,
            "receipt and trust-anchor outputs must be distinct"
        );
    }
    ensure!(
        evidence_dir.is_absolute()
            && evidence_dir.is_dir()
            && !fs::symlink_metadata(evidence_dir)?.file_type().is_symlink(),
        "evidence directory must be an absolute real directory"
    );
    ensure!(
        trusted_execution_header_hash_hex.len() == 64
            && trusted_execution_header_hash_hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            && trusted_execution_header_hash_hex
                == trusted_execution_header_hash_hex.to_ascii_lowercase(),
        "trusted execution header hash must be 32-byte lowercase hex"
    );

    validate_sha256sums(evidence_dir)?;
    let manifest = read_json(&evidence_dir.join("manifest.json"))?;
    ensure!(
        required_string(&manifest, "/schema")?
            == "trnm_research_receipt_v2_rpc_evidence_manifest_v1",
        "unsupported RPC evidence manifest schema"
    );
    let command_id = required_string(&manifest, "/command_id")?.to_string();
    let execution_height = parse_u64(
        required(&manifest, "/execution_height")?,
        "manifest execution_height",
    )?;
    let commitment_height = parse_u64(
        required(&manifest, "/commitment_height")?,
        "manifest commitment_height",
    )?;
    ensure!(
        commitment_height == execution_height.saturating_add(1),
        "manifest commitment height mismatch"
    );
    let target_index = usize::try_from(parse_u64(
        required(&manifest, "/transaction_index")?,
        "manifest transaction_index",
    )?)
    .context("transaction index exceeds usize")?;

    let block_h = read_json(&evidence_dir.join("block-h.json"))?;
    let block_h_plus_1 = read_json(&evidence_dir.join("block-h-plus-1.json"))?;
    let commit_h_plus_1 = read_json(&evidence_dir.join("commit-h-plus-1.json"))?;
    let validators_h_plus_1 = read_json(&evidence_dir.join("validators-h-plus-1.json"))?;
    let block_results_h = read_json(&evidence_dir.join("block-results-h.json"))?;
    let proof = read_json(&evidence_dir.join("applied-command-proof.json"))?;

    let execution_header: block::Header =
        serde_json::from_value(required(&block_h, "/result/block/header")?.clone())
            .context("decode execution header H from RPC JSON")?;
    let commitment_header: block::Header =
        serde_json::from_value(required(&block_h_plus_1, "/result/block/header")?.clone())
            .context("decode commitment header H + 1 from RPC JSON")?;
    let signed_header: block::signed_header::SignedHeader =
        serde_json::from_value(required(&commit_h_plus_1, "/result/signed_header")?.clone())
            .context("decode signed commitment header from RPC JSON")?;
    ensure!(
        signed_header.header == commitment_header,
        "commit and block endpoints disagree on H + 1 header"
    );
    let validator_infos: Vec<validator::Info> =
        serde_json::from_value(required(&validators_h_plus_1, "/result/validators")?.clone())
            .context("decode H + 1 validator set from RPC JSON")?;
    ensure!(!validator_infos.is_empty(), "validator set is empty");
    let validators = validator::Set::without_proposer(validator_infos);
    ensure!(
        validators.hash() == commitment_header.validators_hash,
        "collected validators do not match H + 1 validators_hash"
    );

    ensure!(
        execution_header.height.value() == execution_height
            && commitment_header.height.value() == commitment_height,
        "RPC header heights do not match manifest"
    );
    let execution_header_hash_hex = hex::encode(execution_header.hash().as_bytes());
    ensure!(
        execution_header_hash_hex == *trusted_execution_header_hash_hex,
        "execution header does not match the externally pinned trust hash"
    );
    ensure!(
        execution_header.next_validators_hash == validators.hash(),
        "H next_validators_hash does not match collected H + 1 validators"
    );

    let raw_transactions = decode_block_transactions(&block_h)?;
    let canonical_results = decode_results(&block_results_h)?;
    ensure!(
        target_index < raw_transactions.len(),
        "manifest target transaction index is out of range"
    );
    ensure!(
        raw_transactions.len() == canonical_results.len(),
        "RPC transaction/result count mismatch"
    );
    let target_raw_tx = read_bounded(
        &evidence_dir.join("target-raw-tx.bin"),
        MAX_BINARY_EVIDENCE_BYTES,
    )?;
    ensure!(
        raw_transactions[target_index] == target_raw_tx,
        "manifest target transaction bytes mismatch"
    );

    let raw_signed_header: RawSignedHeader = signed_header.into();
    let raw_validators: RawValidatorSet = validators.clone().into();
    let receipt = assemble_cometbft_apphash_finality_receipt_v2(CometBftReceiptAssemblyInputV2 {
        target_command_id: command_id,
        execution_header: encode_cometbft_header_v1(&execution_header)?,
        commitment_header: encode_cometbft_header_v1(&commitment_header)?,
        commitment_signed_header_proto: raw_signed_header.encode_to_vec(),
        commitment_validator_set_proto: raw_validators.encode_to_vec(),
        raw_transactions,
        canonical_results,
        applied_command_object_proof: decode_proof(&proof)?,
    })
    .context("assemble Receipt V2 from RPC evidence")?;

    let trust_anchor_wire = encode_cometbft_trust_anchor_v1(
        &execution_header,
        &validators,
        2,
        3,
        Duration::from_secs(TRUSTING_PERIOD_SECONDS),
        Duration::from_secs(CLOCK_DRIFT_SECONDS),
    )?;
    let trust_anchor_hash_hex = trust_anchor_wire.anchor_hash_hex.clone();
    let trust_anchor_bytes = trust_anchor_wire.canonical_bytes()?;
    let trust_anchor = ValidatedCometBftTrustAnchorV1::try_from(trust_anchor_wire)?;
    let outcome = verify_cometbft_apphash_finality_receipt_v2_with_trust_anchor(
        &receipt,
        &trust_anchor,
        SystemTime::now(),
    );
    let ReceiptV2VerificationOutcome::Final(verified) = outcome else {
        return Err(anyhow!(
            "public Receipt V2 verifier did not return Final: {outcome:?}"
        ));
    };
    let domain_command_version = match &verified.domain_command {
        VerifiedCometBftDomainCommandV2::ResearchV1(_) => "research_v1",
        VerifiedCometBftDomainCommandV2::PaperRaidFinalityV2(_) => "paper_raid_finality_v2",
        VerifiedCometBftDomainCommandV2::PaperRaidFinalityV3(_) => "paper_raid_finality_v3",
    };
    let receipt_bytes = receipt.canonical_bytes()?;
    write_new(receipt_output, &receipt_bytes)?;
    if let Some(trust_anchor_output) = trust_anchor_output {
        if let Err(error) = write_new(trust_anchor_output, &trust_anchor_bytes) {
            fs::remove_file(receipt_output).with_context(|| {
                format!(
                    "remove incomplete receipt output {}",
                    receipt_output.display()
                )
            })?;
            return Err(error);
        }
    }
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema":"trnm_research_receipt_v2_assembly_result_v1",
            "status":"final",
            "receipt_path":receipt_output,
            "trust_anchor_path":trust_anchor_output,
            "receipt_hash_hex":verified.receipt_hash_hex,
            "trust_anchor_hash_hex":trust_anchor_hash_hex,
            "domain_command_version":domain_command_version,
            "command_id":verified.command_id,
            "execution_height":verified.execution_height,
            "commitment_height":verified.commitment_height,
            "commitment_header_hash_hex":verified.commitment_header_hash_hex,
            "app_hash_hex":verified.app_hash_hex
        }))?
    );
    Ok(())
}

fn run() -> Result<()> {
    let arguments = env::args().collect::<Vec<_>>();
    match arguments.get(1).map(String::as_str) {
        Some("fixture-tx") if arguments.len() == 4 => {
            fixture_tx(Path::new(&arguments[2]), Path::new(&arguments[3]))
        }
        Some("sign-and-wrap") if arguments.len() == 6 => sign_and_wrap(
            Path::new(&arguments[2]),
            Path::new(&arguments[3]),
            Path::new(&arguments[4]),
            Path::new(&arguments[5]),
        ),
        Some("paper-raid-v2-sign-and-wrap") if arguments.len() == 6 => paper_raid_v2_sign_and_wrap(
            Path::new(&arguments[2]),
            Path::new(&arguments[3]),
            Path::new(&arguments[4]),
            Path::new(&arguments[5]),
        ),
        Some("paper-raid-v3-pre-v7-artifact") if arguments.len() == 6 => {
            paper_raid_v3_pre_v7_artifact(
                Path::new(&arguments[2]),
                Path::new(&arguments[3]),
                Path::new(&arguments[4]),
                Path::new(&arguments[5]),
            )
        }
        Some("assemble-and-verify") if arguments.len() == 5 || arguments.len() == 6 => {
            assemble_and_verify(
                Path::new(&arguments[2]),
                Path::new(&arguments[3]),
                &arguments[4],
                arguments.get(5).map(Path::new),
            )
        }
        _ => Err(usage(&arguments[0])),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("TRNM_RESEARCH_RECEIPT_V2_FAILED error={error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use trnm_research_protocol::{
        ObjectRefV1, PaperRaidAppealStatusV2, PaperRaidAppealStatusV3, ResearchObjectKind,
    };

    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let unique = format!(
                "trnm-paper-raid-signing-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn key(byte: u8) -> ExternalKey {
        ExternalKey::from_bytes([byte; 32])
    }

    fn commitment_v3() -> PaperRaidFinalityCommitmentV3 {
        PaperRaidFinalityCommitmentV3 {
            commitment_id: key(1),
            paper_project_id: key(2),
            submission_id: key(3),
            match_evidence_ref: ObjectRefV1::new(ResearchObjectKind::MatchEvidence, key(4), 1),
            release_candidate_hash: [5; 32],
            paper_bundle_hash: [6; 32],
            submission_commitment_hash: [7; 32],
            author_consent_set_hash: [8; 32],
            tolerance_policy_hash: [9; 32],
            evaluation_id: key(10),
            evaluation_hash: [11; 32],
            evaluation_score_bps: 8_500,
            evaluation_accepted: true,
            evaluation_completed_at_unix_s: 1_753_449_400,
            latest_reproduction_id: key(12),
            latest_reproduction_hash: [13; 32],
            latest_reproduction_accepted: true,
            latest_reproduction_completed_at_unix_s: 1_753_449_450,
            evaluation_supersedes: None,
            evaluation_superseded_by: None,
            reproduction_superseded_by: None,
            appeal_status: PaperRaidAppealStatusV3::ClosedNoAppeal,
            appeal_id: None,
            appealed_evaluation_id: None,
            appeal_resolution_hash: None,
            appeal_window_closes_at_unix_s: 1_753_449_500,
            settlement_policy_hash: [14; 32],
            scientific_finality: true,
            score_eligible: false,
            ranking_eligible: false,
            reward_eligible: false,
            economic_eligible: false,
            finalized_at_unix_s: 1_753_449_501,
        }
    }

    fn commitment_v2() -> PaperRaidFinalityCommitmentV2 {
        let v3 = commitment_v3();
        PaperRaidFinalityCommitmentV2 {
            commitment_id: v3.commitment_id,
            paper_project_id: v3.paper_project_id,
            submission_id: v3.submission_id,
            match_evidence_ref: v3.match_evidence_ref,
            release_candidate_hash: v3.release_candidate_hash,
            paper_bundle_hash: v3.paper_bundle_hash,
            submission_commitment_hash: v3.submission_commitment_hash,
            author_consent_set_hash: v3.author_consent_set_hash,
            tolerance_policy_hash: v3.tolerance_policy_hash,
            evaluation_id: v3.evaluation_id,
            evaluation_hash: v3.evaluation_hash,
            evaluation_score_bps: v3.evaluation_score_bps,
            evaluation_accepted: v3.evaluation_accepted,
            evaluation_completed_at_unix_s: v3.evaluation_completed_at_unix_s,
            latest_reproduction_id: v3.latest_reproduction_id,
            latest_reproduction_hash: v3.latest_reproduction_hash,
            latest_reproduction_accepted: v3.latest_reproduction_accepted,
            latest_reproduction_completed_at_unix_s: v3.latest_reproduction_completed_at_unix_s,
            evaluation_superseded_by: v3.evaluation_superseded_by,
            reproduction_superseded_by: v3.reproduction_superseded_by,
            appeal_status: PaperRaidAppealStatusV2::ClosedNoAppeal,
            appeal_id: None,
            appeal_resolution_hash: None,
            appeal_window_closes_at_unix_s: v3.appeal_window_closes_at_unix_s,
            settlement_policy_hash: v3.settlement_policy_hash,
            scientific_finality: v3.scientific_finality,
            score_eligible: v3.score_eligible,
            ranking_eligible: v3.ranking_eligible,
            reward_eligible: v3.reward_eligible,
            economic_eligible: v3.economic_eligible,
            finalized_at_unix_s: v3.finalized_at_unix_s,
        }
    }

    #[test]
    fn pre_v7_paper_raid_v3_lane_marks_artifacts_unbroadcastable_on_app_v6() {
        let directory = TestDirectory::new();
        let input_path = directory.path("input.json");
        let private_key_path = directory.path("private-key.hex");
        let signed_output_path = directory.path("signed-command.json");
        let transaction_path = directory.path("transaction.json");
        let now = now_unix_ms().unwrap();
        let mut input = PaperRaidSignAndWrapInputV3 {
            schema: "trnm_paper_raid_finality_pre_v7_artifact_input_v3".to_string(),
            chain_id: "trnm-test-1".to_string(),
            command_id_hex: hex::encode([0x31; 32]),
            signer_did: "did:trnm:hepta-authority".to_string(),
            nonce: 9,
            max_gas: 300_000,
            fee_limit: 1_000_000,
            issued_at_unix_ms: now.saturating_sub(1_000),
            expires_at_unix_ms: now.saturating_add(60_000),
            commitment_cbor_hex: hex::encode(commitment_v3().canonical_bytes()),
        };
        fs::write(&input_path, serde_json::to_vec(&input).unwrap()).unwrap();
        fs::write(&private_key_path, hex::encode([0x55; 32])).unwrap();

        paper_raid_v3_pre_v7_artifact(
            &input_path,
            &private_key_path,
            &signed_output_path,
            &transaction_path,
        )
        .unwrap();

        let output: PaperRaidSignedCommandOutputV3 =
            serde_json::from_slice(&fs::read(&signed_output_path).unwrap()).unwrap();
        assert_eq!(output.required_consensus_app_version, 7);
        assert!(!output.broadcastable_on_app_v6);
        let signed_bytes = decode_canonical_hex(
            "signed_command_cbor_hex",
            &output.signed_command_cbor_hex,
            None,
            256 * 1024,
        )
        .unwrap();
        let signed = SignedPaperRaidFinalityCommandV3::from_canonical_bytes(&signed_bytes).unwrap();
        assert_eq!(signed.command_id.to_hex(), input.command_id_hex);
        assert_eq!(signed.commitment, commitment_v3());
        assert_eq!(
            output.commitment_hash_hex,
            hex::encode(signed.payload_hash())
        );
        assert_eq!(
            output.applied_command_logical_key,
            paper_raid_finality_applied_command_key_v3(signed.command_id).unwrap()
        );

        let transaction_bytes = fs::read(&transaction_path).unwrap();
        let envelope =
            SignedCommandEnvelopeV1::from_canonical_wire_bytes(&transaction_bytes).unwrap();
        envelope.validate_at(&input.chain_id, now).unwrap();
        assert_eq!(
            envelope.payload_type,
            CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V3
        );
        let canonical_tx_bytes = envelope.payload_bytes().unwrap();
        assert_eq!(
            hex::encode(&canonical_tx_bytes),
            output.canonical_transaction_hex
        );
        let canonical_tx =
            CanonicalPaperRaidFinalityTxV3::from_canonical_bytes(&canonical_tx_bytes).unwrap();
        assert_eq!(
            canonical_tx.signed_paper_raid_finality_command().unwrap(),
            signed
        );
        assert_eq!(
            output.comet_tx_hash_hex,
            hex::encode(comet_tx_hash(&transaction_bytes))
        );
        assert_eq!(
            fs::metadata(&signed_output_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        let mut unlocked = commitment_v3();
        unlocked.score_eligible = true;
        input.commitment_cbor_hex = hex::encode(unlocked.canonical_bytes());
        let unlocked_input_path = directory.path("unlocked-input.json");
        fs::write(&unlocked_input_path, serde_json::to_vec(&input).unwrap()).unwrap();
        let error = paper_raid_v3_pre_v7_artifact(
            &unlocked_input_path,
            &private_key_path,
            &directory.path("unlocked-signed.json"),
            &directory.path("unlocked-transaction.json"),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("keeps all settlement eligibility locked"));
    }

    #[test]
    fn app_v6_v2_signing_lane_remains_explicit_and_never_reinterprets_as_v3() {
        let directory = TestDirectory::new();
        let input_path = directory.path("v2-input.json");
        let private_key_path = directory.path("v2-private-key.hex");
        let signed_output_path = directory.path("v2-signed-command.json");
        let transaction_path = directory.path("v2-transaction.json");
        let now = now_unix_ms().unwrap();
        let input = PaperRaidSignAndWrapInputV2 {
            schema: "trnm_paper_raid_finality_sign_and_wrap_input_v2".to_string(),
            chain_id: "trnm-test-1".to_string(),
            command_id_hex: hex::encode([0x32; 32]),
            signer_did: "did:trnm:hepta-authority".to_string(),
            nonce: 10,
            max_gas: 300_000,
            fee_limit: 1_000_000,
            issued_at_unix_ms: now.saturating_sub(1_000),
            expires_at_unix_ms: now.saturating_add(60_000),
            commitment_cbor_hex: hex::encode(commitment_v2().canonical_bytes()),
        };
        fs::write(&input_path, serde_json::to_vec(&input).unwrap()).unwrap();
        fs::write(&private_key_path, hex::encode([0x56; 32])).unwrap();

        paper_raid_v2_sign_and_wrap(
            &input_path,
            &private_key_path,
            &signed_output_path,
            &transaction_path,
        )
        .unwrap();
        let output: PaperRaidSignedCommandOutputV2 =
            serde_json::from_slice(&fs::read(&signed_output_path).unwrap()).unwrap();
        assert_eq!(output.schema, "trnm_paper_raid_signed_command_output_v2");
        let signed_bytes = decode_canonical_hex(
            "signed_command_cbor_hex",
            &output.signed_command_cbor_hex,
            None,
            256 * 1024,
        )
        .unwrap();
        let signed = SignedPaperRaidFinalityCommandV2::from_canonical_bytes(&signed_bytes).unwrap();
        assert_eq!(signed.commitment, commitment_v2());
        assert!(SignedPaperRaidFinalityCommandV3::from_canonical_bytes(&signed_bytes).is_err());
        assert_eq!(
            output.applied_command_logical_key,
            paper_raid_finality_applied_command_key(signed.command_id).unwrap()
        );

        let transaction_bytes = fs::read(&transaction_path).unwrap();
        let envelope =
            SignedCommandEnvelopeV1::from_canonical_wire_bytes(&transaction_bytes).unwrap();
        assert_eq!(
            envelope.payload_type,
            CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V2
        );
        let canonical_tx_bytes = envelope.payload_bytes().unwrap();
        let canonical_tx =
            CanonicalPaperRaidFinalityTxV2::from_canonical_bytes(&canonical_tx_bytes).unwrap();
        assert_eq!(
            canonical_tx.signed_paper_raid_finality_command().unwrap(),
            signed
        );
        assert!(CanonicalPaperRaidFinalityTxV3::from_canonical_bytes(&canonical_tx_bytes).is_err());
    }
}
