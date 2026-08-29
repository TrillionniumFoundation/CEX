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
use chrono::{DateTime, SecondsFormat, Utc};
use ed25519_dalek::SigningKey;
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
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
    paper_raid_finality_applied_command_key_v4, research_applied_command_key,
    CanonicalPaperRaidFinalityTxV2, CanonicalPaperRaidFinalityTxV3, CanonicalPaperRaidFinalityTxV4,
    CanonicalResearchTxV1, CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V2,
    CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V3,
    CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V4, CANONICAL_RESEARCH_TX_PAYLOAD_TYPE_V1,
};
use trnm_research_protocol::{
    AuthorityRole, CanonicalCbor, ExternalKey, MatchEvidenceCommitmentV1, ObjectRefV1,
    PaperRaidAppealStatusV3, PaperRaidFinalityCommitmentV2, PaperRaidFinalityCommitmentV3,
    PaperRaidFinalityCommitmentV4, PaperRaidReworkLineageV1, ResearchCommandV1, ResearchObjectKind,
    SignedPaperRaidFinalityCommandV2, SignedPaperRaidFinalityCommandV3,
    SignedPaperRaidFinalityCommandV4, SignedResearchCommandV1,
    HEPTA_APPEAL_EXTERNAL_KEY_NAMESPACE_V1, HEPTA_EVALUATION_EXTERNAL_KEY_NAMESPACE_V1,
    HEPTA_PAPER_EXTERNAL_KEY_NAMESPACE_V1,
    HEPTA_PAPER_RAID_FINALITY_PREPARATION_EXTERNAL_KEY_NAMESPACE_V1,
    HEPTA_REPRODUCTION_EXTERNAL_KEY_NAMESPACE_V1, HEPTA_REVISION_EXTERNAL_KEY_NAMESPACE_V1,
    HEPTA_REWORK_EXTERNAL_KEY_NAMESPACE_V1, HEPTA_SUBMISSION_EXTERNAL_KEY_NAMESPACE_V1,
};

const MAX_RPC_JSON_BYTES: u64 = 256 * 1024 * 1024;
const MAX_BINARY_EVIDENCE_BYTES: u64 = 32 * 1024 * 1024;
const TRUSTING_PERIOD_SECONDS: u64 = 24 * 60 * 60;
const CLOCK_DRIFT_SECONDS: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StableFileIdentity {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: i64,
    mtime_nsec: i64,
    ctime: i64,
    ctime_nsec: i64,
    nlink: u64,
}

fn stable_file_identity(metadata: &fs::Metadata) -> StableFileIdentity {
    StableFileIdentity {
        dev: metadata.dev(),
        ino: metadata.ino(),
        len: metadata.len(),
        mtime: metadata.mtime(),
        mtime_nsec: metadata.mtime_nsec(),
        ctime: metadata.ctime(),
        ctime_nsec: metadata.ctime_nsec(),
        nlink: metadata.nlink(),
    }
}

fn usage(program: &str) -> anyhow::Error {
    anyhow!(
        "usage:\n  {program} public-key PRIVATE_KEY\n  {program} fixture-tx PRIVATE_KEY OUTPUT_TX\n  {program} sign-and-wrap SIGNING_INPUT PRIVATE_KEY SIGNED_COMMAND_OUTPUT OUTPUT_TX\n  {program} paper-raid-v2-sign-and-wrap SIGNING_INPUT PRIVATE_KEY SIGNED_COMMAND_OUTPUT OUTPUT_TX\n  {program} paper-raid-v3-pre-v7-artifact SIGNING_INPUT PRIVATE_KEY SIGNED_COMMAND_OUTPUT OUTPUT_TX\n  {program} paper-raid-v4-sign-and-wrap SIGNING_INPUT PRIVATE_KEY SIGNED_COMMAND_OUTPUT OUTPUT_TX\n  {program} paper-raid-v4-hepta-sign-and-wrap HEPTA_SIGNING_INPUT PRIVATE_KEY SIGNED_COMMAND_OUTPUT OUTPUT_TX\n  {program} assemble-and-verify EVIDENCE_DIR RECEIPT_OUTPUT TRUSTED_EXECUTION_HEADER_HASH_HEX [TRUST_ANCHOR_OUTPUT]"
    )
}

fn read_bounded_with_hooks(
    path: &Path,
    max_bytes: u64,
    validate_opened_metadata: impl FnOnce(&fs::Metadata) -> Result<()>,
    after_open: impl FnOnce() -> Result<()>,
) -> Result<Vec<u8>> {
    let path_metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect evidence file {}", path.display()))?;
    ensure!(
        path_metadata.file_type().is_file()
            && !path_metadata.file_type().is_symlink()
            && path_metadata.nlink() == 1,
        "evidence path is not a regular single-link non-symlink file: {}",
        path.display()
    );
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .with_context(|| format!("open evidence file {}", path.display()))?;
    let opened_metadata = file
        .metadata()
        .with_context(|| format!("inspect opened evidence file {}", path.display()))?;
    ensure!(
        opened_metadata.file_type().is_file()
            && opened_metadata.nlink() == 1
            && stable_file_identity(&opened_metadata) == stable_file_identity(&path_metadata),
        "evidence path changed while it was opened: {}",
        path.display()
    );
    ensure!(
        opened_metadata.len() <= max_bytes,
        "evidence file exceeds the {max_bytes}-byte limit: {}",
        path.display()
    );
    validate_opened_metadata(&opened_metadata)?;
    let before_identity = stable_file_identity(&opened_metadata);
    after_open()?;
    let mut bytes = Vec::with_capacity(
        usize::try_from(opened_metadata.len()).context("evidence length exceeds usize")?,
    );
    Read::by_ref(&mut file)
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("read evidence file {}", path.display()))?;
    ensure!(
        u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= max_bytes,
        "evidence file grew beyond the {max_bytes}-byte limit while reading: {}",
        path.display()
    );
    let after_metadata = file
        .metadata()
        .with_context(|| format!("reinspect opened evidence file {}", path.display()))?;
    let final_path_metadata = fs::symlink_metadata(path)
        .with_context(|| format!("reinspect evidence path {}", path.display()))?;
    ensure!(
        after_metadata.file_type().is_file()
            && final_path_metadata.file_type().is_file()
            && !final_path_metadata.file_type().is_symlink()
            && before_identity == stable_file_identity(&after_metadata)
            && before_identity == stable_file_identity(&final_path_metadata)
            && u64::try_from(bytes.len()).ok() == Some(before_identity.len),
        "evidence path or file metadata changed while it was read: {}",
        path.display()
    );
    Ok(bytes)
}

fn read_bounded_after_open(
    path: &Path,
    max_bytes: u64,
    after_open: impl FnOnce() -> Result<()>,
) -> Result<Vec<u8>> {
    read_bounded_with_hooks(path, max_bytes, |_| Ok(()), after_open)
}

fn read_bounded(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    read_bounded_after_open(path, max_bytes, || Ok(()))
}

fn read_bounded_signing_key(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    read_bounded_with_hooks(
        path,
        max_bytes,
        |metadata| {
            // SAFETY: geteuid has no preconditions and does not dereference
            // pointers. The value is compared only with the held file's uid.
            let effective_uid = unsafe { libc::geteuid() };
            ensure!(
                metadata.uid() == effective_uid,
                "private key must be owned by the effective local user"
            );
            ensure!(
                matches!(metadata.mode() & 0o7777, 0o400 | 0o600),
                "private key mode must be 0400 or 0600"
            );
            Ok(())
        },
        || Ok(()),
    )
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

fn write_new_with_finalize(
    path: &Path,
    bytes: &[u8],
    finalize: impl FnOnce(&mut fs::File) -> Result<()>,
) -> Result<()> {
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
    let write_result = (|| -> Result<()> {
        file.write_all(bytes)
            .with_context(|| format!("write new output {}", path.display()))?;
        finalize(&mut file)?;
        Ok(())
    })();
    if let Err(error) = write_result {
        drop(file);
        return match fs::remove_file(path) {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(error.context(format!(
                "also failed to remove partial output {}: {cleanup_error}",
                path.display()
            ))),
        };
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    write_new_with_finalize(path, bytes, |file| {
        file.flush()
            .with_context(|| format!("flush new output {}", path.display()))?;
        file.sync_all()
            .with_context(|| format!("fsync new output {}", path.display()))?;
        Ok(())
    })
}

fn write_new_pair(
    first_path: &Path,
    first_bytes: &[u8],
    second_path: &Path,
    second_bytes: &[u8],
) -> Result<()> {
    write_new(first_path, first_bytes)?;
    if let Err(error) = write_new(second_path, second_bytes) {
        return match fs::remove_file(first_path) {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(error.context(format!(
                "also failed to remove first partial output {}: {cleanup_error}",
                first_path.display()
            ))),
        };
    }
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
    let encoded_key = read_bounded_signing_key(private_key, 256)?;
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

fn print_signing_public_key(private_key: &Path) -> Result<()> {
    let signing_key = read_signing_key(private_key)?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema":"trnm_ed25519_signing_key_public_v1",
            "public_key_hex":hex::encode(signing_key.verifying_key().to_bytes()),
        }))?
    );
    Ok(())
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
    broadcastable_on_consensus: bool,
    superseded_by_consensus_app_version: u64,
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
struct PaperRaidSignAndWrapInputV4 {
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum HeptaPaperRaidAppealStatusV2 {
    ClosedNoAppeal,
    ResolvedDenied,
    ResolvedUpheld,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum HeptaPaperRaidPreparationStatusV2 {
    AwaitingChainVerifierUpgrade,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct HeptaPaperRaidReworkLineageV1 {
    schema: String,
    rework_id: String,
    rework_cycle: u64,
    rejected_submission_id: String,
    replacement_submission_id: String,
    rejected_revision_id: String,
    replacement_revision_id: String,
    rejected_release_candidate_hash: String,
    replacement_release_candidate_hash: String,
    rejected_paper_bundle_hash: String,
    replacement_paper_bundle_hash: String,
    rejected_rework_content_commitment_sha256: String,
    replacement_rework_content_commitment_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct HeptaPaperRaidCommandBindingV2 {
    schema: String,
    commitment_id: String,
    source_fingerprint: String,
    window_arm_id: String,
    paper_project_id: String,
    submission_id: String,
    research_session_id: String,
    research_session_roster_version: u64,
    match_evidence_commitment_id: String,
    match_evidence_object_version: u64,
    release_candidate_hash: String,
    paper_bundle_hash: String,
    submission_commitment_hash: String,
    author_consent_set_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rework_lineage: Option<HeptaPaperRaidReworkLineageV1>,
    tolerance_policy_hash: String,
    evaluation_id: String,
    evaluation_signing_hash: String,
    evaluation_score_bps: u16,
    evaluation_accepted: bool,
    evaluation_completed_at_unix_s: u64,
    evaluation_supersedes_evaluation_id: Option<String>,
    evaluation_superseded_by_evaluation_id: Option<String>,
    latest_reproduction_id: String,
    latest_reproduction_report_hash: String,
    latest_reproduction_accepted: bool,
    latest_reproduction_completed_at_unix_s: u64,
    reproduction_supersedes_reproduction_id: Option<String>,
    reproduction_superseded_by_reproduction_id: Option<String>,
    appeal_status: HeptaPaperRaidAppealStatusV2,
    appeal_id: Option<String>,
    appealed_evaluation_id: Option<String>,
    appeal_resolution_id: Option<String>,
    appeal_resolution_hash: Option<String>,
    start_checkpoint_hash: String,
    start_checkpoint_anchor_hash: String,
    start_checkpoint_chain_id: String,
    start_checkpoint_height: u64,
    start_checkpoint_header_hash: String,
    start_checkpoint_consensus_time_unix_ms: u64,
    final_checkpoint_hash: String,
    final_checkpoint_anchor_hash: String,
    final_checkpoint_chain_id: String,
    final_checkpoint_height: u64,
    final_checkpoint_header_hash: String,
    final_checkpoint_consensus_time_unix_ms: u64,
    max_chain_time_lag_ms: u64,
    appeal_window_closes_at_unix_ms: u64,
    appeal_window_closes_at_unix_s: u64,
    settlement_policy_hash: String,
    scientific_finality: bool,
    score_eligible: bool,
    ranking_eligible: bool,
    reward_eligible: bool,
    economic_eligible: bool,
    finalized_at_unix_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct HeptaPaperRaidFinalityPreparationV2 {
    schema: String,
    preparation_id: String,
    idempotency_key: String,
    request_hash: String,
    binding: HeptaPaperRaidCommandBindingV2,
    binding_fingerprint: String,
    status: HeptaPaperRaidPreparationStatusV2,
    created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HeptaPaperRaidV4SignAndWrapInputV1 {
    schema: String,
    chain_id: String,
    signer_did: String,
    nonce: u64,
    max_gas: u64,
    fee_limit: u128,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    preparation: HeptaPaperRaidFinalityPreparationV2,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaperRaidSignedCommandOutputV4 {
    schema: String,
    required_consensus_app_version: u64,
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
    rework_id: Option<String>,
    rework_cycle: Option<u64>,
    applied_command_logical_key: String,
    outer_envelope_payload_hash_hex: String,
    comet_tx_hash_hex: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HeptaPaperRaidSignedCommandOutputV4 {
    schema: String,
    required_consensus_app_version: u64,
    chain_id: String,
    command_id: String,
    signer_did: String,
    nonce: u64,
    public_key_hex: String,
    signed_command_cbor_hex: String,
    signed_command_cbor_sha256: String,
    canonical_transaction_hex: String,
    command_fingerprint_hex: String,
    commitment_hash_hex: String,
    domain_payload_hash_hex: String,
    preparation_id: String,
    preparation_idempotency_key: String,
    binding_fingerprint: String,
    source_commitment_id_sha256: String,
    v4_commitment_id_hex: String,
    rework_id: Option<String>,
    rework_cycle: Option<u64>,
    applied_command_logical_key: String,
    outer_envelope_payload_hash_hex: String,
    comet_tx_hash_hex: String,
}

#[derive(Debug, Clone)]
struct HeptaPaperRaidProjectionSourceV2 {
    preparation_id: String,
    preparation_idempotency_key: String,
    binding_fingerprint: String,
    source_commitment_id_sha256: String,
}

const HEPTA_PREPARATION_SCHEMA_V2: &str = "hepta.paper_raid.trnm_finality_preparation.v2";
const HEPTA_BINDING_SCHEMA_V2: &str = "hepta.paper_raid.trnm_command_binding.v2";
const HEPTA_REWORK_LINEAGE_SCHEMA_V1: &str = "hepta.paper_raid.trnm_finality_rework_lineage.v1";
const HEPTA_BINDING_COMMITMENT_ID_DOMAIN_V2: &str =
    "hepta.paper_raid.trnm_finality_commitment_id.v2";
const HEPTA_ZERO_SHA256: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";
const HEPTA_MAX_CHAIN_TIME_LAG_MS_V1: u64 = 15 * 60 * 1_000;
const HEPTA_NO_APPEAL_WINDOW_MS_V1: u64 = 24 * 60 * 60 * 1_000;
const JSON_SAFE_U64_MAX: u64 = 9_007_199_254_740_991;

fn canonical_json_sha256(value: &impl Serialize) -> Result<String> {
    fn sort_value(value: Value) -> Value {
        match value {
            Value::Array(values) => Value::Array(values.into_iter().map(sort_value).collect()),
            Value::Object(values) => {
                let mut entries = values.into_iter().collect::<Vec<_>>();
                entries.sort_by(|left, right| left.0.cmp(&right.0));
                Value::Object(
                    entries
                        .into_iter()
                        .map(|(key, value)| (key, sort_value(value)))
                        .collect(),
                )
            }
            primitive => primitive,
        }
    }
    let value = serde_json::to_value(value).context("project canonical Hepta JSON")?;
    let bytes = serde_json::to_vec(&sort_value(value)).context("encode canonical Hepta JSON")?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

fn strict_sha256_digest(label: &str, value: &str) -> Result<[u8; 32]> {
    let digest = value
        .strip_prefix("sha256:")
        .with_context(|| format!("{label} must use the exact sha256: prefix"))?;
    let decoded = decode_canonical_hex(label, digest, Some(32), 32)?;
    let digest: [u8; 32] = decoded
        .try_into()
        .map_err(|_| anyhow!("{label} must encode 32 bytes"))?;
    ensure!(digest != [0; 32], "{label} must be non-zero");
    Ok(digest)
}

fn strict_raw_sha256(label: &str, value: &str) -> Result<[u8; 32]> {
    let decoded = decode_canonical_hex(label, value, Some(32), 32)?;
    let digest: [u8; 32] = decoded
        .try_into()
        .map_err(|_| anyhow!("{label} must encode 32 bytes"))?;
    ensure!(digest != [0; 32], "{label} must be non-zero");
    Ok(digest)
}

fn hepta_uuid_key(label: &str, namespace: &str, value: &str) -> Result<ExternalKey> {
    ensure!(
        value != "00000000-0000-0000-0000-000000000000",
        "{label} must be a non-zero canonical lowercase UUID"
    );
    ExternalKey::from_uuid(namespace, value)
        .map_err(|error| anyhow!("{label} is not a canonical lowercase UUID: {error}"))
}

fn validate_hepta_uuid(label: &str, value: &str) -> Result<()> {
    hepta_uuid_key(
        label,
        HEPTA_PAPER_RAID_FINALITY_PREPARATION_EXTERNAL_KEY_NAMESPACE_V1,
        value,
    )
    .map(|_| ())
}

fn optional_hepta_uuid_key(
    label: &str,
    namespace: &str,
    value: Option<&String>,
) -> Result<Option<ExternalKey>> {
    value
        .map(|value| hepta_uuid_key(label, namespace, value))
        .transpose()
}

fn ceil_millis_to_seconds(value: u64) -> Result<u64> {
    value
        .checked_add(999)
        .map(|value| value / 1_000)
        .context("Hepta millisecond timestamp overflow")
}

fn project_hepta_preparation_v2_to_v4(
    chain_id: &str,
    preparation: &HeptaPaperRaidFinalityPreparationV2,
) -> Result<(ExternalKey, PaperRaidFinalityCommitmentV4)> {
    ensure!(
        preparation.schema == HEPTA_PREPARATION_SCHEMA_V2,
        "unsupported Hepta Paper Raid preparation schema"
    );
    let command_id = hepta_uuid_key(
        "preparation_id",
        HEPTA_PAPER_RAID_FINALITY_PREPARATION_EXTERNAL_KEY_NAMESPACE_V1,
        &preparation.preparation_id,
    )?;
    ensure!(
        !preparation.idempotency_key.is_empty()
            && preparation.idempotency_key.len() <= 128
            && preparation.idempotency_key.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
            }),
        "preparation.idempotency_key must contain 1-128 canonical ASCII token bytes"
    );
    ensure!(
        preparation.status == HeptaPaperRaidPreparationStatusV2::AwaitingChainVerifierUpgrade,
        "unsupported Hepta Paper Raid preparation status"
    );
    strict_sha256_digest("request_hash", &preparation.request_hash)?;
    strict_sha256_digest("binding_fingerprint", &preparation.binding_fingerprint)?;

    let binding = &preparation.binding;
    ensure!(
        binding.schema == HEPTA_BINDING_SCHEMA_V2,
        "unsupported Hepta Paper Raid binding schema"
    );
    let mut commitment_preimage = binding.clone();
    commitment_preimage.commitment_id = HEPTA_ZERO_SHA256.to_string();
    let expected_commitment_id = canonical_json_sha256(&json!({
        "domain": HEPTA_BINDING_COMMITMENT_ID_DOMAIN_V2,
        "binding": commitment_preimage,
    }))?;
    ensure!(
        binding.commitment_id == expected_commitment_id,
        "Hepta binding commitment_id mismatch"
    );
    ensure!(
        preparation.binding_fingerprint == canonical_json_sha256(binding)?,
        "Hepta preparation binding_fingerprint mismatch"
    );
    let final_checkpoint_millis = i64::try_from(binding.final_checkpoint_consensus_time_unix_ms)
        .context("final_checkpoint_consensus_time_unix_ms exceeds chrono i64 range")?;
    let expected_created_at = DateTime::<Utc>::from_timestamp_millis(final_checkpoint_millis)
        .context("final_checkpoint_consensus_time_unix_ms is outside chrono range")?
        .to_rfc3339_opts(SecondsFormat::AutoSi, true);
    ensure!(
        preparation.created_at == expected_created_at,
        "created_at must be the canonical UTC serialization of final_checkpoint_consensus_time_unix_ms"
    );

    for (label, value) in [
        ("source_fingerprint", &binding.source_fingerprint),
        ("start_checkpoint_hash", &binding.start_checkpoint_hash),
        ("final_checkpoint_hash", &binding.final_checkpoint_hash),
    ] {
        strict_sha256_digest(label, value)?;
    }
    ensure!(
        binding.start_checkpoint_hash != binding.final_checkpoint_hash,
        "Hepta start and final checkpoint hashes must be distinct"
    );
    for (label, value) in [
        (
            "start_checkpoint_anchor_hash",
            &binding.start_checkpoint_anchor_hash,
        ),
        (
            "start_checkpoint_header_hash",
            &binding.start_checkpoint_header_hash,
        ),
        (
            "final_checkpoint_anchor_hash",
            &binding.final_checkpoint_anchor_hash,
        ),
        (
            "final_checkpoint_header_hash",
            &binding.final_checkpoint_header_hash,
        ),
    ] {
        strict_raw_sha256(label, value)?;
    }
    validate_hepta_uuid("window_arm_id", &binding.window_arm_id)?;
    ensure!(
        !binding.research_session_id.is_empty()
            && binding.research_session_id.len() <= 128
            && binding
                .research_session_id
                .bytes()
                .enumerate()
                .all(|(index, byte)| {
                    byte.is_ascii_alphanumeric()
                        || (index > 0 && matches!(byte, b'.' | b'_' | b':' | b'-'))
                }),
        "research_session_id is not canonical"
    );
    ensure!(
        binding.research_session_roster_version > 0,
        "research_session_roster_version must be positive"
    );
    ensure!(
        binding.start_checkpoint_chain_id == chain_id
            && binding.final_checkpoint_chain_id == chain_id,
        "Hepta checkpoint chain_id does not match signing chain_id"
    );
    ensure!(
        binding.start_checkpoint_height > 0
            && binding.final_checkpoint_height > binding.start_checkpoint_height,
        "Hepta checkpoint heights regress"
    );
    ensure!(
        binding.start_checkpoint_consensus_time_unix_ms
            < binding.final_checkpoint_consensus_time_unix_ms,
        "Hepta checkpoint time regresses"
    );
    ensure!(
        binding.max_chain_time_lag_ms == HEPTA_MAX_CHAIN_TIME_LAG_MS_V1,
        "unsupported Hepta Chain-time lag policy"
    );
    let policy_delay_ms = match binding.appeal_status {
        HeptaPaperRaidAppealStatusV2::ClosedNoAppeal => HEPTA_NO_APPEAL_WINDOW_MS_V1,
        HeptaPaperRaidAppealStatusV2::ResolvedDenied
        | HeptaPaperRaidAppealStatusV2::ResolvedUpheld => 0,
    };
    let expected_window_close = binding
        .start_checkpoint_consensus_time_unix_ms
        .checked_add(binding.max_chain_time_lag_ms)
        .and_then(|value| value.checked_add(policy_delay_ms))
        .context("Hepta Appeal deadline overflow")?;
    ensure!(
        binding.appeal_window_closes_at_unix_ms == expected_window_close
            && binding.appeal_window_closes_at_unix_s
                == ceil_millis_to_seconds(binding.appeal_window_closes_at_unix_ms)?
            && binding.final_checkpoint_consensus_time_unix_ms
                >= binding.appeal_window_closes_at_unix_ms
            && binding.finalized_at_unix_s
                == ceil_millis_to_seconds(binding.final_checkpoint_consensus_time_unix_ms)?,
        "Hepta consensus-time projection mismatch"
    );
    ensure!(
        binding.evaluation_completed_at_unix_s > 0
            && binding.latest_reproduction_completed_at_unix_s
                >= binding.evaluation_completed_at_unix_s
            && binding.appeal_window_closes_at_unix_s
                >= binding.latest_reproduction_completed_at_unix_s
            && binding.finalized_at_unix_s >= binding.appeal_window_closes_at_unix_s,
        "Hepta scientific-finality timestamps regress"
    );
    ensure!(
        binding.match_evidence_object_version == 1,
        "Hepta MatchEvidence object version must be 1"
    );
    ensure!(
        binding.evaluation_score_bps <= 10_000
            && (!binding.evaluation_accepted || binding.evaluation_score_bps > 0),
        "Hepta evaluation score is inconsistent"
    );
    ensure!(
        binding.scientific_finality
            && !binding.score_eligible
            && !binding.ranking_eligible
            && !binding.reward_eligible
            && !binding.economic_eligible,
        "Hepta preparation must keep every settlement eligibility flag locked"
    );
    ensure!(
        binding.evaluation_superseded_by_evaluation_id.is_none()
            && binding.reproduction_superseded_by_reproduction_id.is_none(),
        "Hepta preparation cannot finalize superseded facts"
    );

    for (label, value) in [
        (
            "evaluation_supersedes_evaluation_id",
            binding.evaluation_supersedes_evaluation_id.as_ref(),
        ),
        (
            "evaluation_superseded_by_evaluation_id",
            binding.evaluation_superseded_by_evaluation_id.as_ref(),
        ),
        (
            "reproduction_supersedes_reproduction_id",
            binding.reproduction_supersedes_reproduction_id.as_ref(),
        ),
        (
            "reproduction_superseded_by_reproduction_id",
            binding.reproduction_superseded_by_reproduction_id.as_ref(),
        ),
        ("appeal_id", binding.appeal_id.as_ref()),
        (
            "appealed_evaluation_id",
            binding.appealed_evaluation_id.as_ref(),
        ),
        (
            "appeal_resolution_id",
            binding.appeal_resolution_id.as_ref(),
        ),
    ] {
        if let Some(value) = value {
            validate_hepta_uuid(label, value)?;
        }
    }
    match binding.appeal_status {
        HeptaPaperRaidAppealStatusV2::ClosedNoAppeal => ensure!(
            binding.appeal_id.is_none()
                && binding.appealed_evaluation_id.is_none()
                && binding.appeal_resolution_id.is_none()
                && binding.appeal_resolution_hash.is_none()
                && binding.evaluation_supersedes_evaluation_id.is_none(),
            "closed_no_appeal carries Appeal or supersession fields"
        ),
        HeptaPaperRaidAppealStatusV2::ResolvedDenied => ensure!(
            binding.appeal_id.is_some()
                && binding.appealed_evaluation_id.as_ref() == Some(&binding.evaluation_id)
                && binding.appeal_resolution_id.is_some()
                && binding.appeal_resolution_hash.is_some(),
            "resolved_denied lacks its exact Appeal resolution"
        ),
        HeptaPaperRaidAppealStatusV2::ResolvedUpheld => ensure!(
            binding.appeal_id.is_some()
                && binding.appeal_resolution_id.is_some()
                && binding.appeal_resolution_hash.is_some()
                && binding.appealed_evaluation_id.is_some()
                && binding.appealed_evaluation_id.as_ref() != Some(&binding.evaluation_id)
                && binding.evaluation_supersedes_evaluation_id == binding.appealed_evaluation_id,
            "resolved_upheld lacks its exact superseding evaluation"
        ),
    }

    let rework_lineage = binding
        .rework_lineage
        .as_ref()
        .map(|lineage| -> Result<PaperRaidReworkLineageV1> {
            ensure!(
                lineage.schema == HEPTA_REWORK_LINEAGE_SCHEMA_V1,
                "unsupported Hepta rework lineage schema"
            );
            ensure!(
                (2..=JSON_SAFE_U64_MAX).contains(&lineage.rework_cycle),
                "Hepta rework_cycle is outside the canonical JSON-safe range"
            );
            Ok(PaperRaidReworkLineageV1 {
                rework_id: hepta_uuid_key(
                    "rework_id",
                    HEPTA_REWORK_EXTERNAL_KEY_NAMESPACE_V1,
                    &lineage.rework_id,
                )?,
                rework_cycle: lineage.rework_cycle,
                rejected_submission_id: hepta_uuid_key(
                    "rejected_submission_id",
                    HEPTA_SUBMISSION_EXTERNAL_KEY_NAMESPACE_V1,
                    &lineage.rejected_submission_id,
                )?,
                replacement_submission_id: hepta_uuid_key(
                    "replacement_submission_id",
                    HEPTA_SUBMISSION_EXTERNAL_KEY_NAMESPACE_V1,
                    &lineage.replacement_submission_id,
                )?,
                rejected_revision_id: hepta_uuid_key(
                    "rejected_revision_id",
                    HEPTA_REVISION_EXTERNAL_KEY_NAMESPACE_V1,
                    &lineage.rejected_revision_id,
                )?,
                replacement_revision_id: hepta_uuid_key(
                    "replacement_revision_id",
                    HEPTA_REVISION_EXTERNAL_KEY_NAMESPACE_V1,
                    &lineage.replacement_revision_id,
                )?,
                rejected_release_candidate_hash: strict_sha256_digest(
                    "rejected_release_candidate_hash",
                    &lineage.rejected_release_candidate_hash,
                )?,
                replacement_release_candidate_hash: strict_sha256_digest(
                    "replacement_release_candidate_hash",
                    &lineage.replacement_release_candidate_hash,
                )?,
                rejected_paper_bundle_hash: strict_sha256_digest(
                    "rejected_paper_bundle_hash",
                    &lineage.rejected_paper_bundle_hash,
                )?,
                replacement_paper_bundle_hash: strict_sha256_digest(
                    "replacement_paper_bundle_hash",
                    &lineage.replacement_paper_bundle_hash,
                )?,
                rejected_rework_content_commitment_sha256: strict_sha256_digest(
                    "rejected_rework_content_commitment_sha256",
                    &lineage.rejected_rework_content_commitment_sha256,
                )?,
                replacement_rework_content_commitment_sha256: strict_sha256_digest(
                    "replacement_rework_content_commitment_sha256",
                    &lineage.replacement_rework_content_commitment_sha256,
                )?,
            })
        })
        .transpose()?;

    let paper_project_id = hepta_uuid_key(
        "paper_project_id",
        HEPTA_PAPER_EXTERNAL_KEY_NAMESPACE_V1,
        &binding.paper_project_id,
    )?;
    let submission_id = hepta_uuid_key(
        "submission_id",
        HEPTA_SUBMISSION_EXTERNAL_KEY_NAMESPACE_V1,
        &binding.submission_id,
    )?;
    let evaluation_id = hepta_uuid_key(
        "evaluation_id",
        HEPTA_EVALUATION_EXTERNAL_KEY_NAMESPACE_V1,
        &binding.evaluation_id,
    )?;
    let latest_reproduction_id = hepta_uuid_key(
        "latest_reproduction_id",
        HEPTA_REPRODUCTION_EXTERNAL_KEY_NAMESPACE_V1,
        &binding.latest_reproduction_id,
    )?;
    let appeal_status = match binding.appeal_status {
        HeptaPaperRaidAppealStatusV2::ClosedNoAppeal => PaperRaidAppealStatusV3::ClosedNoAppeal,
        HeptaPaperRaidAppealStatusV2::ResolvedDenied => PaperRaidAppealStatusV3::ResolvedDenied,
        HeptaPaperRaidAppealStatusV2::ResolvedUpheld => PaperRaidAppealStatusV3::ResolvedUpheld,
    };
    let commitment = PaperRaidFinalityCommitmentV4 {
        commitment_id: ExternalKey::from_bytes(strict_sha256_digest(
            "commitment_id",
            &binding.commitment_id,
        )?),
        paper_project_id,
        submission_id,
        match_evidence_ref: ObjectRefV1::new(
            ResearchObjectKind::MatchEvidence,
            ExternalKey::from_bytes(strict_sha256_digest(
                "match_evidence_commitment_id",
                &binding.match_evidence_commitment_id,
            )?),
            binding.match_evidence_object_version,
        ),
        release_candidate_hash: strict_sha256_digest(
            "release_candidate_hash",
            &binding.release_candidate_hash,
        )?,
        paper_bundle_hash: strict_sha256_digest("paper_bundle_hash", &binding.paper_bundle_hash)?,
        submission_commitment_hash: strict_sha256_digest(
            "submission_commitment_hash",
            &binding.submission_commitment_hash,
        )?,
        author_consent_set_hash: strict_sha256_digest(
            "author_consent_set_hash",
            &binding.author_consent_set_hash,
        )?,
        tolerance_policy_hash: strict_sha256_digest(
            "tolerance_policy_hash",
            &binding.tolerance_policy_hash,
        )?,
        evaluation_id,
        evaluation_hash: strict_sha256_digest(
            "evaluation_signing_hash",
            &binding.evaluation_signing_hash,
        )?,
        evaluation_score_bps: binding.evaluation_score_bps,
        evaluation_accepted: binding.evaluation_accepted,
        evaluation_completed_at_unix_s: binding.evaluation_completed_at_unix_s,
        latest_reproduction_id,
        latest_reproduction_hash: strict_sha256_digest(
            "latest_reproduction_report_hash",
            &binding.latest_reproduction_report_hash,
        )?,
        latest_reproduction_accepted: binding.latest_reproduction_accepted,
        latest_reproduction_completed_at_unix_s: binding.latest_reproduction_completed_at_unix_s,
        evaluation_supersedes: optional_hepta_uuid_key(
            "evaluation_supersedes_evaluation_id",
            HEPTA_EVALUATION_EXTERNAL_KEY_NAMESPACE_V1,
            binding.evaluation_supersedes_evaluation_id.as_ref(),
        )?,
        evaluation_superseded_by: optional_hepta_uuid_key(
            "evaluation_superseded_by_evaluation_id",
            HEPTA_EVALUATION_EXTERNAL_KEY_NAMESPACE_V1,
            binding.evaluation_superseded_by_evaluation_id.as_ref(),
        )?,
        reproduction_superseded_by: optional_hepta_uuid_key(
            "reproduction_superseded_by_reproduction_id",
            HEPTA_REPRODUCTION_EXTERNAL_KEY_NAMESPACE_V1,
            binding.reproduction_superseded_by_reproduction_id.as_ref(),
        )?,
        appeal_status,
        appeal_id: optional_hepta_uuid_key(
            "appeal_id",
            HEPTA_APPEAL_EXTERNAL_KEY_NAMESPACE_V1,
            binding.appeal_id.as_ref(),
        )?,
        appealed_evaluation_id: optional_hepta_uuid_key(
            "appealed_evaluation_id",
            HEPTA_EVALUATION_EXTERNAL_KEY_NAMESPACE_V1,
            binding.appealed_evaluation_id.as_ref(),
        )?,
        appeal_resolution_hash: binding
            .appeal_resolution_hash
            .as_ref()
            .map(|value| strict_sha256_digest("appeal_resolution_hash", value))
            .transpose()?,
        appeal_window_closes_at_unix_s: binding.appeal_window_closes_at_unix_s,
        settlement_policy_hash: strict_sha256_digest(
            "settlement_policy_hash",
            &binding.settlement_policy_hash,
        )?,
        scientific_finality: binding.scientific_finality,
        score_eligible: binding.score_eligible,
        ranking_eligible: binding.ranking_eligible,
        reward_eligible: binding.reward_eligible,
        economic_eligible: binding.economic_eligible,
        finalized_at_unix_s: binding.finalized_at_unix_s,
        rework_lineage,
    };
    // Hepta's reproduction_supersedes_reproduction_id and appeal_resolution_id
    // have no V4 commitment slots. They were schema/UUID/Appeal-relationship
    // validated above and are intentionally not folded into another field.
    commitment
        .validate()
        .context("projected Hepta Paper Raid V4 commitment is invalid")?;
    Ok((command_id, commitment))
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
    let envelope_bytes = envelope.to_wire_bytes()?;
    write_new_pair(
        signed_command_output,
        &signed_output,
        transaction_output,
        &envelope_bytes,
    )?;
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
    write_new_pair(
        signed_command_output,
        &signed_output,
        transaction_output,
        &transaction_bytes,
    )?;
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
        broadcastable_on_consensus: false,
        superseded_by_consensus_app_version: 7,
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
    write_new_pair(
        signed_command_output,
        &signed_output,
        transaction_output,
        &transaction_bytes,
    )?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema":"trnm_paper_raid_finality_pre_v7_artifact_result_v3",
            "broadcastable_on_consensus":false,
            "superseded_by_consensus_app_version":7,
            "status":"historical_offline_artifact_only",
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

fn paper_raid_v4_sign_and_wrap(
    signing_input: &Path,
    private_key: &Path,
    signed_command_output: &Path,
    transaction_output: &Path,
) -> Result<()> {
    let input: PaperRaidSignAndWrapInputV4 =
        serde_json::from_slice(&read_bounded(signing_input, MAX_BINARY_EVIDENCE_BYTES)?)
            .with_context(|| format!("decode signing input {}", signing_input.display()))?;
    ensure!(
        input.schema == "trnm_paper_raid_finality_sign_and_wrap_input_v4",
        "unsupported Paper Raid V4 signing input schema"
    );
    ensure!(
        input.expires_at_unix_ms > input.issued_at_unix_ms
            && input
                .expires_at_unix_ms
                .saturating_sub(input.issued_at_unix_ms)
                <= 300_000,
        "Paper Raid V4 outer-envelope lifetime must be 1..=300000 milliseconds"
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
    let commitment = PaperRaidFinalityCommitmentV4::from_canonical_bytes(&commitment_bytes)
        .context("decode canonical Paper Raid V4 finality commitment")?;
    write_paper_raid_v4_signed_outputs(
        input,
        ExternalKey::from_bytes(command_id),
        commitment,
        private_key,
        signed_command_output,
        transaction_output,
        None,
        "trnm_paper_raid_signed_command_output_v4",
        "trnm_paper_raid_finality_sign_and_wrap_result_v4",
    )
}

fn paper_raid_v4_hepta_sign_and_wrap(
    signing_input: &Path,
    private_key: &Path,
    signed_command_output: &Path,
    transaction_output: &Path,
) -> Result<()> {
    let input: HeptaPaperRaidV4SignAndWrapInputV1 =
        serde_json::from_slice(&read_bounded(signing_input, MAX_BINARY_EVIDENCE_BYTES)?)
            .with_context(|| format!("decode Hepta signing input {}", signing_input.display()))?;
    ensure!(
        input.schema == "trnm_hepta_paper_raid_v4_sign_and_wrap_input_v1",
        "unsupported Hepta Paper Raid V4 signing input schema"
    );
    let (command_id, commitment) =
        project_hepta_preparation_v2_to_v4(&input.chain_id, &input.preparation)?;
    let source = HeptaPaperRaidProjectionSourceV2 {
        preparation_id: input.preparation.preparation_id.clone(),
        preparation_idempotency_key: input.preparation.idempotency_key.clone(),
        binding_fingerprint: input.preparation.binding_fingerprint.clone(),
        source_commitment_id_sha256: input.preparation.binding.commitment_id.clone(),
    };
    let projected = PaperRaidSignAndWrapInputV4 {
        schema: "trnm_paper_raid_finality_sign_and_wrap_input_v4".to_string(),
        chain_id: input.chain_id,
        command_id_hex: command_id.to_hex(),
        signer_did: input.signer_did,
        nonce: input.nonce,
        max_gas: input.max_gas,
        fee_limit: input.fee_limit,
        issued_at_unix_ms: input.issued_at_unix_ms,
        expires_at_unix_ms: input.expires_at_unix_ms,
        commitment_cbor_hex: hex::encode(commitment.canonical_bytes()),
    };
    write_paper_raid_v4_signed_outputs(
        projected,
        command_id,
        commitment,
        private_key,
        signed_command_output,
        transaction_output,
        Some(source),
        "trnm_hepta_paper_raid_signed_command_output_v4",
        "trnm_hepta_paper_raid_finality_sign_and_wrap_result_v4",
    )
}

#[allow(clippy::too_many_arguments)]
fn write_paper_raid_v4_signed_outputs(
    input: PaperRaidSignAndWrapInputV4,
    command_id: ExternalKey,
    commitment: PaperRaidFinalityCommitmentV4,
    private_key: &Path,
    signed_command_output: &Path,
    transaction_output: &Path,
    hepta_source: Option<HeptaPaperRaidProjectionSourceV2>,
    output_schema: &str,
    result_schema: &str,
) -> Result<()> {
    ensure!(
        signed_command_output != transaction_output,
        "signed command and transaction outputs must be distinct"
    );
    ensure!(
        input.expires_at_unix_ms > input.issued_at_unix_ms
            && input
                .expires_at_unix_ms
                .saturating_sub(input.issued_at_unix_ms)
                <= 300_000,
        "Paper Raid V4 outer-envelope lifetime must be 1..=300000 milliseconds"
    );
    ensure!(
        input.command_id_hex == command_id.to_hex(),
        "Paper Raid V4 projected command ID mismatch"
    );
    ensure!(
        !commitment.score_eligible
            && !commitment.ranking_eligible
            && !commitment.reward_eligible
            && !commitment.economic_eligible,
        "Paper Raid V4 candidate signing keeps all settlement eligibility locked"
    );
    let signing_key = read_signing_key(private_key)?;
    let signed = SignedPaperRaidFinalityCommandV4::sign(
        input.chain_id,
        command_id,
        input.signer_did,
        input.nonce,
        commitment,
        &signing_key,
    )
    .context("sign Paper Raid V4 finality command")?;
    let canonical_tx = CanonicalPaperRaidFinalityTxV4::from_signed_command(
        &signed,
        input.max_gas,
        input.fee_limit,
    )
    .context("build canonical Paper Raid V4 finality transaction")?;
    let canonical_tx_bytes = canonical_tx.canonical_bytes()?;
    let envelope = SignedCommandEnvelopeV1::sign(
        signed.chain_id.clone(),
        signed.command_id.to_hex(),
        signed.signer_did.clone(),
        "hepta",
        signed.nonce,
        input.issued_at_unix_ms,
        input.expires_at_unix_ms,
        CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V4,
        &canonical_tx_bytes,
        &signing_key,
    )?;
    envelope
        .validate_at(&signed.chain_id, now_unix_ms()?)
        .context("validate Paper Raid V4 outer envelope against the current clock")?;
    let transaction_bytes = envelope.to_wire_bytes()?;
    let signed_command_cbor = signed.canonical_bytes();
    let signed_command_cbor_hex = hex::encode(&signed_command_cbor);
    let signed_command_cbor_sha256 = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(&signed_command_cbor))
    );
    let command_fingerprint_hex = hex::encode(signed.command_fingerprint());
    let domain_payload_hash_hex = hex::encode(signed.payload_hash());
    let v4_commitment_id_hex = signed.commitment.commitment_id.to_hex();
    let (rework_id, rework_cycle) = signed
        .commitment
        .rework_lineage
        .as_ref()
        .map(|lineage| (Some(lineage.rework_id.to_hex()), Some(lineage.rework_cycle)))
        .unwrap_or((None, None));
    let applied_command_logical_key =
        paper_raid_finality_applied_command_key_v4(signed.command_id)?;
    let comet_tx_hash_hex = hex::encode(comet_tx_hash(&transaction_bytes));
    let signed_output = match hepta_source.as_ref() {
        Some(source) => {
            ensure!(
                strict_sha256_digest(
                    "source_commitment_id_sha256",
                    &source.source_commitment_id_sha256,
                )? == *signed.commitment.commitment_id.as_bytes(),
                "Hepta source commitment ID diverges from projected V4 commitment ID"
            );
            serde_json::to_vec(&HeptaPaperRaidSignedCommandOutputV4 {
                schema: output_schema.to_string(),
                required_consensus_app_version: 7,
                chain_id: signed.chain_id.clone(),
                command_id: signed.command_id.to_hex(),
                signer_did: signed.signer_did.clone(),
                nonce: signed.nonce,
                public_key_hex: hex::encode(signed.public_key),
                signed_command_cbor_hex: signed_command_cbor_hex.clone(),
                signed_command_cbor_sha256: signed_command_cbor_sha256.clone(),
                canonical_transaction_hex: hex::encode(&canonical_tx_bytes),
                command_fingerprint_hex: command_fingerprint_hex.clone(),
                commitment_hash_hex: domain_payload_hash_hex.clone(),
                domain_payload_hash_hex: domain_payload_hash_hex.clone(),
                preparation_id: source.preparation_id.clone(),
                preparation_idempotency_key: source.preparation_idempotency_key.clone(),
                binding_fingerprint: source.binding_fingerprint.clone(),
                source_commitment_id_sha256: source.source_commitment_id_sha256.clone(),
                v4_commitment_id_hex: v4_commitment_id_hex.clone(),
                rework_id: rework_id.clone(),
                rework_cycle,
                applied_command_logical_key: applied_command_logical_key.clone(),
                outer_envelope_payload_hash_hex: envelope.payload_hash_hex.clone(),
                comet_tx_hash_hex: comet_tx_hash_hex.clone(),
            })?
        }
        None => serde_json::to_vec(&PaperRaidSignedCommandOutputV4 {
            schema: output_schema.to_string(),
            required_consensus_app_version: 7,
            chain_id: signed.chain_id.clone(),
            command_id: signed.command_id.to_hex(),
            signer_did: signed.signer_did.clone(),
            nonce: signed.nonce,
            public_key_hex: hex::encode(signed.public_key),
            signed_command_cbor_hex: signed_command_cbor_hex.clone(),
            canonical_transaction_hex: hex::encode(&canonical_tx_bytes),
            command_fingerprint_hex: command_fingerprint_hex.clone(),
            commitment_id: v4_commitment_id_hex.clone(),
            commitment_hash_hex: domain_payload_hash_hex.clone(),
            rework_id: rework_id.clone(),
            rework_cycle,
            applied_command_logical_key: applied_command_logical_key.clone(),
            outer_envelope_payload_hash_hex: envelope.payload_hash_hex.clone(),
            comet_tx_hash_hex: comet_tx_hash_hex.clone(),
        })?,
    };
    write_new_pair(
        signed_command_output,
        &signed_output,
        transaction_output,
        &transaction_bytes,
    )?;
    let mut result = json!({
        "schema":result_schema,
        "required_consensus_app_version":7,
        "signed_command_path":signed_command_output,
        "transaction_path":transaction_output,
        "command_id":signed.command_id.to_hex(),
        "command_fingerprint_hex":command_fingerprint_hex,
        "commitment_id":v4_commitment_id_hex,
        "commitment_hash_hex":domain_payload_hash_hex.clone(),
        "domain_payload_hash_hex":domain_payload_hash_hex,
        "signed_command_cbor_hex":signed_command_cbor_hex,
        "signed_command_cbor_sha256":signed_command_cbor_sha256,
        "rework_id":rework_id,
        "rework_cycle":rework_cycle,
        "applied_command_logical_key":applied_command_logical_key,
        "public_key_hex":hex::encode(signing_key.verifying_key().to_bytes()),
        "comet_tx_hash_hex":comet_tx_hash_hex
    });
    if let Some(source) = hepta_source {
        let object = result
            .as_object_mut()
            .context("Paper Raid V4 result must be a JSON object")?;
        object.insert("preparation_id".to_string(), json!(source.preparation_id));
        object.insert(
            "preparation_idempotency_key".to_string(),
            json!(source.preparation_idempotency_key),
        );
        object.insert(
            "binding_fingerprint".to_string(),
            json!(source.binding_fingerprint),
        );
        object.insert(
            "source_commitment_id_sha256".to_string(),
            json!(source.source_commitment_id_sha256),
        );
        object.insert(
            "v4_commitment_id_hex".to_string(),
            json!(signed.commitment.commitment_id.to_hex()),
        );
    }
    println!("{}", serde_json::to_string(&result)?);
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
        VerifiedCometBftDomainCommandV2::PaperRaidFinalityV4(_) => "paper_raid_finality_v4",
    };
    let receipt_bytes = receipt.canonical_bytes()?;
    if let Some(trust_anchor_output) = trust_anchor_output {
        write_new_pair(
            receipt_output,
            &receipt_bytes,
            trust_anchor_output,
            &trust_anchor_bytes,
        )?;
    } else {
        write_new(receipt_output, &receipt_bytes)?;
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
        Some("public-key") if arguments.len() == 3 => {
            print_signing_public_key(Path::new(&arguments[2]))
        }
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
        Some("paper-raid-v4-sign-and-wrap") if arguments.len() == 6 => paper_raid_v4_sign_and_wrap(
            Path::new(&arguments[2]),
            Path::new(&arguments[3]),
            Path::new(&arguments[4]),
            Path::new(&arguments[5]),
        ),
        Some("paper-raid-v4-hepta-sign-and-wrap") if arguments.len() == 6 => {
            paper_raid_v4_hepta_sign_and_wrap(
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
        ObjectRefV1, PaperRaidAppealStatusV2, PaperRaidAppealStatusV3, PaperRaidReworkLineageV1,
        ResearchObjectKind,
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

    fn write_test_signing_key(path: &Path, byte: u8) {
        fs::write(path, hex::encode([byte; 32])).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
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

    fn commitment_v4() -> PaperRaidFinalityCommitmentV4 {
        let v3 = commitment_v3();
        PaperRaidFinalityCommitmentV4 {
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
            evaluation_supersedes: v3.evaluation_supersedes,
            evaluation_superseded_by: v3.evaluation_superseded_by,
            reproduction_superseded_by: v3.reproduction_superseded_by,
            appeal_status: v3.appeal_status,
            appeal_id: v3.appeal_id,
            appealed_evaluation_id: v3.appealed_evaluation_id,
            appeal_resolution_hash: v3.appeal_resolution_hash,
            appeal_window_closes_at_unix_s: v3.appeal_window_closes_at_unix_s,
            settlement_policy_hash: v3.settlement_policy_hash,
            scientific_finality: v3.scientific_finality,
            score_eligible: v3.score_eligible,
            ranking_eligible: v3.ranking_eligible,
            reward_eligible: v3.reward_eligible,
            economic_eligible: v3.economic_eligible,
            finalized_at_unix_s: v3.finalized_at_unix_s,
            rework_lineage: Some(PaperRaidReworkLineageV1 {
                rework_id: key(15),
                rework_cycle: 2,
                rejected_submission_id: key(16),
                replacement_submission_id: v3.submission_id,
                rejected_revision_id: key(17),
                replacement_revision_id: key(18),
                rejected_release_candidate_hash: [19; 32],
                replacement_release_candidate_hash: v3.release_candidate_hash,
                rejected_paper_bundle_hash: [20; 32],
                replacement_paper_bundle_hash: v3.paper_bundle_hash,
                rejected_rework_content_commitment_sha256: [21; 32],
                replacement_rework_content_commitment_sha256: [22; 32],
            }),
        }
    }

    fn hepta_digest(byte: char) -> String {
        format!("sha256:{}", byte.to_string().repeat(64))
    }

    fn hepta_raw_digest(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn refresh_hepta_preparation_hashes(preparation: &mut HeptaPaperRaidFinalityPreparationV2) {
        preparation.binding.commitment_id = HEPTA_ZERO_SHA256.to_string();
        preparation.binding.commitment_id = canonical_json_sha256(&json!({
            "domain": HEPTA_BINDING_COMMITMENT_ID_DOMAIN_V2,
            "binding": preparation.binding,
        }))
        .unwrap();
        preparation.binding_fingerprint = canonical_json_sha256(&preparation.binding).unwrap();
        preparation.created_at = DateTime::<Utc>::from_timestamp_millis(
            i64::try_from(preparation.binding.final_checkpoint_consensus_time_unix_ms).unwrap(),
        )
        .unwrap()
        .to_rfc3339_opts(SecondsFormat::AutoSi, true);
    }

    fn hepta_preparation_v2() -> HeptaPaperRaidFinalityPreparationV2 {
        let start_checkpoint_consensus_time_unix_ms = 1_753_449_000_123;
        let appeal_window_closes_at_unix_ms = start_checkpoint_consensus_time_unix_ms
            + HEPTA_MAX_CHAIN_TIME_LAG_MS_V1
            + HEPTA_NO_APPEAL_WINDOW_MS_V1;
        let final_checkpoint_consensus_time_unix_ms = appeal_window_closes_at_unix_ms + 1_000;
        let mut preparation = HeptaPaperRaidFinalityPreparationV2 {
            schema: HEPTA_PREPARATION_SCHEMA_V2.to_string(),
            preparation_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_string(),
            idempotency_key: "client.retry-token:v2".to_string(),
            request_hash: hepta_digest('1'),
            binding: HeptaPaperRaidCommandBindingV2 {
                schema: HEPTA_BINDING_SCHEMA_V2.to_string(),
                commitment_id: HEPTA_ZERO_SHA256.to_string(),
                source_fingerprint: hepta_digest('a'),
                window_arm_id: "11111111-1111-4111-8111-111111111111".to_string(),
                paper_project_id: "22222222-2222-4222-8222-222222222222".to_string(),
                submission_id: "33333333-3333-4333-8333-333333333333".to_string(),
                research_session_id: "paper-raid:session_v2".to_string(),
                research_session_roster_version: 7,
                match_evidence_commitment_id: hepta_digest('9'),
                match_evidence_object_version: 1,
                release_candidate_hash: hepta_digest('b'),
                paper_bundle_hash: hepta_digest('c'),
                submission_commitment_hash: hepta_digest('f'),
                author_consent_set_hash: hepta_digest('d'),
                rework_lineage: Some(HeptaPaperRaidReworkLineageV1 {
                    schema: HEPTA_REWORK_LINEAGE_SCHEMA_V1.to_string(),
                    rework_id: "66666666-6666-4666-8666-666666666666".to_string(),
                    rework_cycle: 2,
                    rejected_submission_id: "77777777-7777-4777-8777-777777777777".to_string(),
                    replacement_submission_id: "33333333-3333-4333-8333-333333333333".to_string(),
                    rejected_revision_id: "88888888-8888-4888-8888-888888888888".to_string(),
                    replacement_revision_id: "99999999-9999-4999-8999-999999999999".to_string(),
                    rejected_release_candidate_hash: hepta_digest('4'),
                    replacement_release_candidate_hash: hepta_digest('b'),
                    rejected_paper_bundle_hash: hepta_digest('5'),
                    replacement_paper_bundle_hash: hepta_digest('c'),
                    rejected_rework_content_commitment_sha256: hepta_digest('6'),
                    replacement_rework_content_commitment_sha256: hepta_digest('7'),
                }),
                tolerance_policy_hash: hepta_digest('e'),
                evaluation_id: "44444444-4444-4444-8444-444444444444".to_string(),
                evaluation_signing_hash: hepta_digest('1'),
                evaluation_score_bps: 8_500,
                evaluation_accepted: true,
                evaluation_completed_at_unix_s: 1_753_449_100,
                evaluation_supersedes_evaluation_id: None,
                evaluation_superseded_by_evaluation_id: None,
                latest_reproduction_id: "55555555-5555-4555-8555-555555555555".to_string(),
                latest_reproduction_report_hash: hepta_digest('2'),
                latest_reproduction_accepted: true,
                latest_reproduction_completed_at_unix_s: 1_753_449_200,
                reproduction_supersedes_reproduction_id: None,
                reproduction_superseded_by_reproduction_id: None,
                appeal_status: HeptaPaperRaidAppealStatusV2::ClosedNoAppeal,
                appeal_id: None,
                appealed_evaluation_id: None,
                appeal_resolution_id: None,
                appeal_resolution_hash: None,
                start_checkpoint_hash: hepta_digest('3'),
                start_checkpoint_anchor_hash: hepta_raw_digest('5'),
                start_checkpoint_chain_id: "trnm-test-1".to_string(),
                start_checkpoint_height: 100,
                start_checkpoint_header_hash: hepta_raw_digest('6'),
                start_checkpoint_consensus_time_unix_ms,
                final_checkpoint_hash: hepta_digest('4'),
                final_checkpoint_anchor_hash: hepta_raw_digest('7'),
                final_checkpoint_chain_id: "trnm-test-1".to_string(),
                final_checkpoint_height: 200,
                final_checkpoint_header_hash: hepta_raw_digest('8'),
                final_checkpoint_consensus_time_unix_ms,
                max_chain_time_lag_ms: HEPTA_MAX_CHAIN_TIME_LAG_MS_V1,
                appeal_window_closes_at_unix_ms,
                appeal_window_closes_at_unix_s: ceil_millis_to_seconds(
                    appeal_window_closes_at_unix_ms,
                )
                .unwrap(),
                settlement_policy_hash: hepta_digest('3'),
                scientific_finality: true,
                score_eligible: false,
                ranking_eligible: false,
                reward_eligible: false,
                economic_eligible: false,
                finalized_at_unix_s: ceil_millis_to_seconds(
                    final_checkpoint_consensus_time_unix_ms,
                )
                .unwrap(),
            },
            binding_fingerprint: HEPTA_ZERO_SHA256.to_string(),
            status: HeptaPaperRaidPreparationStatusV2::AwaitingChainVerifierUpgrade,
            created_at: String::new(),
        };
        refresh_hepta_preparation_hashes(&mut preparation);
        preparation
    }

    fn optional_key_hex(value: Option<ExternalKey>) -> Value {
        value
            .map(|key| Value::String(key.to_hex()))
            .unwrap_or(Value::Null)
    }

    fn optional_digest_hex(value: Option<[u8; 32]>) -> Value {
        value
            .map(|digest| Value::String(hex::encode(digest)))
            .unwrap_or(Value::Null)
    }

    fn paper_raid_v4_fixture_json(commitment: &PaperRaidFinalityCommitmentV4) -> Value {
        let appeal_status = match commitment.appeal_status {
            PaperRaidAppealStatusV3::Open => "open",
            PaperRaidAppealStatusV3::ClosedNoAppeal => "closed_no_appeal",
            PaperRaidAppealStatusV3::ResolvedDenied => "resolved_denied",
            PaperRaidAppealStatusV3::ResolvedUpheld => "resolved_upheld",
        };
        let match_evidence_kind = match commitment.match_evidence_ref.kind {
            ResearchObjectKind::MatchEvidence => "match_evidence",
            other => panic!("unexpected Paper Raid MatchEvidence kind: {other:?}"),
        };
        let rework_lineage = commitment
            .rework_lineage
            .as_ref()
            .map(|lineage| {
                json!({
                    "rework_id_hex":lineage.rework_id.to_hex(),
                    "rework_cycle":lineage.rework_cycle,
                    "rejected_submission_id_hex":lineage.rejected_submission_id.to_hex(),
                    "replacement_submission_id_hex":lineage.replacement_submission_id.to_hex(),
                    "rejected_revision_id_hex":lineage.rejected_revision_id.to_hex(),
                    "replacement_revision_id_hex":lineage.replacement_revision_id.to_hex(),
                    "rejected_release_candidate_hash_hex":hex::encode(lineage.rejected_release_candidate_hash),
                    "replacement_release_candidate_hash_hex":hex::encode(lineage.replacement_release_candidate_hash),
                    "rejected_paper_bundle_hash_hex":hex::encode(lineage.rejected_paper_bundle_hash),
                    "replacement_paper_bundle_hash_hex":hex::encode(lineage.replacement_paper_bundle_hash),
                    "rejected_rework_content_commitment_sha256_hex":hex::encode(lineage.rejected_rework_content_commitment_sha256),
                    "replacement_rework_content_commitment_sha256_hex":hex::encode(lineage.replacement_rework_content_commitment_sha256),
                })
            })
            .unwrap_or(Value::Null);
        json!({
            "commitment_id_hex":commitment.commitment_id.to_hex(),
            "paper_project_id_hex":commitment.paper_project_id.to_hex(),
            "submission_id_hex":commitment.submission_id.to_hex(),
            "match_evidence_ref":{
                "kind":match_evidence_kind,
                "key_hex":commitment.match_evidence_ref.key.to_hex(),
                "object_version":commitment.match_evidence_ref.object_version,
            },
            "release_candidate_hash_hex":hex::encode(commitment.release_candidate_hash),
            "paper_bundle_hash_hex":hex::encode(commitment.paper_bundle_hash),
            "submission_commitment_hash_hex":hex::encode(commitment.submission_commitment_hash),
            "author_consent_set_hash_hex":hex::encode(commitment.author_consent_set_hash),
            "tolerance_policy_hash_hex":hex::encode(commitment.tolerance_policy_hash),
            "evaluation_id_hex":commitment.evaluation_id.to_hex(),
            "evaluation_hash_hex":hex::encode(commitment.evaluation_hash),
            "evaluation_score_bps":commitment.evaluation_score_bps,
            "evaluation_accepted":commitment.evaluation_accepted,
            "evaluation_completed_at_unix_s":commitment.evaluation_completed_at_unix_s,
            "latest_reproduction_id_hex":commitment.latest_reproduction_id.to_hex(),
            "latest_reproduction_hash_hex":hex::encode(commitment.latest_reproduction_hash),
            "latest_reproduction_accepted":commitment.latest_reproduction_accepted,
            "latest_reproduction_completed_at_unix_s":commitment.latest_reproduction_completed_at_unix_s,
            "evaluation_supersedes_hex":optional_key_hex(commitment.evaluation_supersedes),
            "evaluation_superseded_by_hex":optional_key_hex(commitment.evaluation_superseded_by),
            "reproduction_superseded_by_hex":optional_key_hex(commitment.reproduction_superseded_by),
            "appeal_status":appeal_status,
            "appeal_id_hex":optional_key_hex(commitment.appeal_id),
            "appealed_evaluation_id_hex":optional_key_hex(commitment.appealed_evaluation_id),
            "appeal_resolution_hash_hex":optional_digest_hex(commitment.appeal_resolution_hash),
            "appeal_window_closes_at_unix_s":commitment.appeal_window_closes_at_unix_s,
            "settlement_policy_hash_hex":hex::encode(commitment.settlement_policy_hash),
            "scientific_finality":commitment.scientific_finality,
            "score_eligible":commitment.score_eligible,
            "ranking_eligible":commitment.ranking_eligible,
            "reward_eligible":commitment.reward_eligible,
            "economic_eligible":commitment.economic_eligible,
            "finalized_at_unix_s":commitment.finalized_at_unix_s,
            "rework_lineage":rework_lineage,
        })
    }

    #[test]
    fn checked_in_hepta_v4_projection_golden_is_literal_and_complete() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/hepta-paper-raid-v4-projection-golden-v1.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["schema"],
            "trnm_hepta_paper_raid_v4_projection_golden_v1"
        );
        assert_eq!(fixture["chain_id"], "trnm-test-1");
        assert_eq!(
            fixture["external_key_namespaces"],
            json!({
                "preparation_id":HEPTA_PAPER_RAID_FINALITY_PREPARATION_EXTERNAL_KEY_NAMESPACE_V1,
                "paper_project_id":HEPTA_PAPER_EXTERNAL_KEY_NAMESPACE_V1,
                "submission_id":HEPTA_SUBMISSION_EXTERNAL_KEY_NAMESPACE_V1,
                "evaluation_id":HEPTA_EVALUATION_EXTERNAL_KEY_NAMESPACE_V1,
                "reproduction_id":HEPTA_REPRODUCTION_EXTERNAL_KEY_NAMESPACE_V1,
                "appeal_id":HEPTA_APPEAL_EXTERNAL_KEY_NAMESPACE_V1,
                "rework_id":HEPTA_REWORK_EXTERNAL_KEY_NAMESPACE_V1,
                "revision_id":HEPTA_REVISION_EXTERNAL_KEY_NAMESPACE_V1,
            })
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 4);
        assert_eq!(
            cases
                .iter()
                .map(|case| case["name"].as_str().unwrap())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["original", "rework", "denied", "upheld"])
        );
        for case in cases {
            let preparation: HeptaPaperRaidFinalityPreparationV2 =
                serde_json::from_value(case["preparation"].clone()).unwrap();
            let (command_id, commitment) =
                project_hepta_preparation_v2_to_v4("trnm-test-1", &preparation).unwrap();
            let expected = &case["expected"];
            assert_eq!(
                expected["binding_commitment_id"],
                preparation.binding.commitment_id
            );
            assert_eq!(
                expected["binding_fingerprint"],
                preparation.binding_fingerprint
            );
            assert_eq!(expected["command_id_hex"], command_id.to_hex());
            assert_eq!(
                expected["v4_commitment"],
                paper_raid_v4_fixture_json(&commitment)
            );
            let commitment_cbor = commitment.canonical_bytes();
            assert_eq!(
                expected["v4_commitment_cbor_hex"],
                hex::encode(&commitment_cbor)
            );
            assert_eq!(
                expected["v4_commitment_cbor_sha256_hex"],
                hex::encode(Sha256::digest(&commitment_cbor))
            );
            assert_eq!(
                expected["domain_payload_hash_hex"],
                hex::encode(commitment.canonical_hash("trnm-paper-raid-finality-commitment-v4"))
            );
        }
    }

    #[test]
    fn bounded_local_reads_reject_symlink_hardlink_and_path_swap_inputs() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let original = directory.path("original.json");
        let hardlink = directory.path("hardlink.json");
        let symlink_path = directory.path("symlink.json");
        fs::write(&original, b"original").unwrap();
        fs::hard_link(&original, &hardlink).unwrap();
        let hardlink_error = read_bounded(&original, 64).unwrap_err();
        assert!(hardlink_error.to_string().contains("single-link"));
        fs::remove_file(&hardlink).unwrap();
        symlink(&original, &symlink_path).unwrap();
        let symlink_error = read_bounded(&symlink_path, 64).unwrap_err();
        assert!(symlink_error.to_string().contains("non-symlink"));

        let replacement = directory.path("replacement.json");
        let moved = directory.path("moved.json");
        fs::write(&replacement, b"replacement").unwrap();
        let swap_error = read_bounded_after_open(&original, 64, || {
            fs::rename(&original, &moved)?;
            fs::rename(&replacement, &original)?;
            Ok(())
        })
        .unwrap_err();
        assert!(swap_error.to_string().contains("changed while it was read"));
    }

    #[test]
    fn signing_key_permissions_and_output_partial_cleanup_fail_closed() {
        let directory = TestDirectory::new();
        let private_key = directory.path("private-key.hex");
        fs::write(&private_key, hex::encode([0x5a; 32])).unwrap();
        fs::set_permissions(&private_key, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(read_signing_key(&private_key)
            .unwrap_err()
            .to_string()
            .contains("0400 or 0600"));
        fs::set_permissions(&private_key, fs::Permissions::from_mode(0o4600)).unwrap();
        assert!(read_signing_key(&private_key)
            .unwrap_err()
            .to_string()
            .contains("0400 or 0600"));
        fs::set_permissions(&private_key, fs::Permissions::from_mode(0o400)).unwrap();
        let signing_key = read_signing_key(&private_key).unwrap();
        assert_eq!(
            signing_key.verifying_key().to_bytes(),
            SigningKey::from_bytes(&[0x5a; 32])
                .verifying_key()
                .to_bytes()
        );

        let complete = directory.path("complete-output.bin");
        write_new(&complete, b"complete").unwrap();
        assert_eq!(fs::read(&complete).unwrap(), b"complete");
        assert_eq!(
            fs::metadata(&complete).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let partial = directory.path("partial-output.bin");
        let injected = write_new_with_finalize(&partial, b"partially written", |_| {
            Err(anyhow!("injected flush/fsync failure"))
        })
        .unwrap_err();
        assert!(injected
            .to_string()
            .contains("injected flush/fsync failure"));
        assert!(!partial.exists());

        let first = directory.path("first-output.bin");
        let occupied_second = directory.path("occupied-second-output.bin");
        fs::write(&occupied_second, b"preexisting").unwrap();
        assert!(write_new_pair(&first, b"first", &occupied_second, b"second").is_err());
        assert!(!first.exists());
        assert_eq!(fs::read(&occupied_second).unwrap(), b"preexisting");
    }

    #[test]
    fn hepta_projection_freezes_some_none_command_namespaces_and_cbor() {
        let rework = hepta_preparation_v2();
        let (command_id, commitment) =
            project_hepta_preparation_v2_to_v4("trnm-test-1", &rework).unwrap();
        assert_eq!(
            rework.binding.commitment_id,
            "sha256:f1a030d0e3c7d4e4226bfd9502156f581cd79ec9ad0e2912567337beb977e1a4"
        );
        assert_eq!(
            rework.binding_fingerprint,
            "sha256:e5c777d6c636fd3f2d0b7ffdeb27f83b5c1807a5f8ed5a38e7bfe83622ba866f"
        );
        assert_eq!(
            command_id.to_hex(),
            "49b81370713efa18ec9b30e2d959e40452d9eb4682526d71136572e1932313d2"
        );
        assert_eq!(
            commitment.paper_project_id.to_hex(),
            "98118e8b257c2d2c0042302d4ee2f009ddf9bf090635d71455253ceedbb1066c"
        );
        assert_eq!(
            commitment.submission_id.to_hex(),
            "9e076a4fc993b95e2ae16cb2250c3522839c81b0e96520a607ee8b45091630bc"
        );
        assert_eq!(
            commitment.evaluation_id.to_hex(),
            "c6ac79b4114c81c94fa23236553290eaa32cc1a74f34b735f6d802b86b7c21c1"
        );
        assert_eq!(
            commitment.latest_reproduction_id.to_hex(),
            "a09e6ec329c121b0fa101481ff743bf532dcbb16c24ad306d8a181f6022aa67e"
        );
        assert_eq!(
            commitment.commitment_id.to_hex(),
            rework
                .binding
                .commitment_id
                .strip_prefix("sha256:")
                .unwrap()
        );
        assert_eq!(commitment.match_evidence_ref.key.to_hex(), "9".repeat(64));
        let lineage = commitment.rework_lineage.as_ref().unwrap();
        assert_eq!(
            lineage.rework_id.to_hex(),
            "794d97d35c723f803c9db80e6aeae293139042f36d7a353ac27ea3b5ccbc864e"
        );
        assert_eq!(
            lineage.rejected_submission_id.to_hex(),
            "f6e8f52e552157801cf33b6fbf77ddc2ed9708159c42313426d0c97b193eb670"
        );
        assert_eq!(
            lineage.rejected_revision_id.to_hex(),
            "a1ab090e4bb66ba6b905cde5095b4417f9002f04e3f3f6eee410df6f7cc8b8ca"
        );
        assert_eq!(
            lineage.replacement_revision_id.to_hex(),
            "6595d5d5effdd645ea48a5a31ac73e53bb612b29ec5e24e98899cf0492ae50fc"
        );
        assert_eq!(lineage.replacement_submission_id, commitment.submission_id);
        assert_eq!(
            lineage.replacement_release_candidate_hash,
            commitment.release_candidate_hash
        );
        assert_eq!(
            lineage.replacement_paper_bundle_hash,
            commitment.paper_bundle_hash
        );
        assert_ne!(
            lineage.replacement_rework_content_commitment_sha256,
            commitment.submission_commitment_hash
        );
        assert_eq!(
            hex::encode(Sha256::digest(commitment.canonical_bytes())),
            "ff4ceff3a645f60ef623d6cc64984a9e49885271eb6ee2186559cf5fb424fa62"
        );

        let mut original = rework.clone();
        original.preparation_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_string();
        original.binding.rework_lineage = None;
        refresh_hepta_preparation_hashes(&mut original);
        let (original_command_id, original_commitment) =
            project_hepta_preparation_v2_to_v4("trnm-test-1", &original).unwrap();
        assert_eq!(
            original.binding.commitment_id,
            "sha256:636042610c6fb885bf807d6684c9bf59fc3bd8b9f751cdf8635839df25c9d637"
        );
        assert_eq!(
            original.binding_fingerprint,
            "sha256:848ed8f103960d8521ab2262f0f24f7bcd406bf1b5d5c8ba0fb03656aa05c2b4"
        );
        assert_eq!(
            original_command_id.to_hex(),
            "c43d3169df7e9e92e0a5a54b2ad0732380d5550970fd195b5968a859545d7c6e"
        );
        assert!(original_commitment.rework_lineage.is_none());
        assert_eq!(original_commitment.canonical_bytes().last(), Some(&0xf6));
        assert_ne!(original_command_id, command_id);
        assert_eq!(
            hex::encode(Sha256::digest(original_commitment.canonical_bytes())),
            "c5070822efa8068946818ad82b5124e11dc6b06ad1122eccdabcc2b66c71235d"
        );
    }

    #[test]
    fn hepta_projection_rejects_noncanonical_identity_time_hash_and_shape() {
        let baseline = hepta_preparation_v2();

        let mut uppercase = baseline.clone();
        uppercase.preparation_id = uppercase.preparation_id.to_ascii_uppercase();
        assert!(
            project_hepta_preparation_v2_to_v4("trnm-test-1", &uppercase)
                .unwrap_err()
                .to_string()
                .contains("canonical lowercase UUID")
        );

        let mut bad_token = baseline.clone();
        bad_token.idempotency_key = "not/allowed".to_string();
        assert!(
            project_hepta_preparation_v2_to_v4("trnm-test-1", &bad_token)
                .unwrap_err()
                .to_string()
                .contains("canonical ASCII token")
        );

        let mut equivalent_noncanonical_time = baseline.clone();
        equivalent_noncanonical_time.created_at = equivalent_noncanonical_time
            .created_at
            .replace('Z', "+00:00");
        assert!(
            project_hepta_preparation_v2_to_v4("trnm-test-1", &equivalent_noncanonical_time,)
                .unwrap_err()
                .to_string()
                .contains("canonical UTC serialization")
        );

        let mut stale_commitment = baseline.clone();
        stale_commitment
            .binding
            .reproduction_supersedes_reproduction_id =
            Some("abababab-abab-4bab-8bab-abababababab".to_string());
        assert!(
            project_hepta_preparation_v2_to_v4("trnm-test-1", &stale_commitment)
                .unwrap_err()
                .to_string()
                .contains("commitment_id mismatch")
        );

        let mut stale_fingerprint = baseline.clone();
        stale_fingerprint.binding_fingerprint = hepta_digest('8');
        assert!(
            project_hepta_preparation_v2_to_v4("trnm-test-1", &stale_fingerprint)
                .unwrap_err()
                .to_string()
                .contains("binding_fingerprint mismatch")
        );

        let mut identical_checkpoint_hashes = baseline.clone();
        identical_checkpoint_hashes.binding.final_checkpoint_hash = identical_checkpoint_hashes
            .binding
            .start_checkpoint_hash
            .clone();
        refresh_hepta_preparation_hashes(&mut identical_checkpoint_hashes);
        assert!(
            project_hepta_preparation_v2_to_v4("trnm-test-1", &identical_checkpoint_hashes,)
                .unwrap_err()
                .to_string()
                .contains("checkpoint hashes must be distinct")
        );

        let mut same_second_but_earlier_final = baseline.clone();
        same_second_but_earlier_final
            .binding
            .final_checkpoint_consensus_time_unix_ms = same_second_but_earlier_final
            .binding
            .appeal_window_closes_at_unix_ms
            - 1;
        same_second_but_earlier_final.binding.finalized_at_unix_s = ceil_millis_to_seconds(
            same_second_but_earlier_final
                .binding
                .final_checkpoint_consensus_time_unix_ms,
        )
        .unwrap();
        refresh_hepta_preparation_hashes(&mut same_second_but_earlier_final);
        assert!(
            project_hepta_preparation_v2_to_v4("trnm-test-1", &same_second_but_earlier_final,)
                .unwrap_err()
                .to_string()
                .contains("consensus-time projection mismatch")
        );

        let mut replacement_drift = baseline.clone();
        replacement_drift
            .binding
            .rework_lineage
            .as_mut()
            .unwrap()
            .replacement_submission_id = "77777777-7777-4777-8777-777777777777".to_string();
        refresh_hepta_preparation_hashes(&mut replacement_drift);
        assert!(
            project_hepta_preparation_v2_to_v4("trnm-test-1", &replacement_drift)
                .unwrap_err()
                .to_string()
                .contains("projected Hepta Paper Raid V4 commitment is invalid")
        );

        let mut unknown = serde_json::to_value(&baseline).unwrap();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_string(), json!(true));
        assert!(serde_json::from_value::<HeptaPaperRaidFinalityPreparationV2>(unknown).is_err());
        let mut wrong_status = serde_json::to_value(&baseline).unwrap();
        wrong_status["status"] = json!("ready_to_broadcast");
        assert!(
            serde_json::from_value::<HeptaPaperRaidFinalityPreparationV2>(wrong_status).is_err()
        );

        let (baseline_id, _) =
            project_hepta_preparation_v2_to_v4("trnm-test-1", &baseline).unwrap();
        let mut retry_token = baseline.clone();
        retry_token.idempotency_key = "a-completely-different.retry:token".to_string();
        let (retry_id, _) =
            project_hepta_preparation_v2_to_v4("trnm-test-1", &retry_token).unwrap();
        assert_eq!(retry_id, baseline_id, "idempotency is not command identity");
        let mut distinct_preparation = baseline;
        distinct_preparation.preparation_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_string();
        let (distinct_id, _) =
            project_hepta_preparation_v2_to_v4("trnm-test-1", &distinct_preparation).unwrap();
        assert_ne!(
            distinct_id, baseline_id,
            "distinct preparation UUIDs cannot alias"
        );
    }

    #[test]
    fn hepta_projection_maps_denied_upheld_and_binds_fields_without_v4_slots() {
        let original = hepta_preparation_v2();
        let (_, original_commitment) =
            project_hepta_preparation_v2_to_v4("trnm-test-1", &original).unwrap();

        let mut reproduction_parent = original.clone();
        reproduction_parent
            .binding
            .reproduction_supersedes_reproduction_id =
            Some("abababab-abab-4bab-8bab-abababababab".to_string());
        refresh_hepta_preparation_hashes(&mut reproduction_parent);
        let (_, reproduction_parent_commitment) =
            project_hepta_preparation_v2_to_v4("trnm-test-1", &reproduction_parent).unwrap();
        assert_ne!(
            reproduction_parent_commitment.commitment_id,
            original_commitment.commitment_id
        );

        let mut denied = original.clone();
        denied.preparation_id = "12121212-1212-4121-8121-121212121212".to_string();
        denied.binding.appeal_status = HeptaPaperRaidAppealStatusV2::ResolvedDenied;
        denied.binding.appeal_id = Some("cccccccc-cccc-4ccc-8ccc-cccccccccccc".to_string());
        denied.binding.appealed_evaluation_id = Some(denied.binding.evaluation_id.clone());
        denied.binding.appeal_resolution_id =
            Some("dddddddd-dddd-4ddd-8ddd-dddddddddddd".to_string());
        denied.binding.appeal_resolution_hash = Some(hepta_digest('8'));
        denied.binding.appeal_window_closes_at_unix_ms =
            denied.binding.start_checkpoint_consensus_time_unix_ms + HEPTA_MAX_CHAIN_TIME_LAG_MS_V1;
        denied.binding.appeal_window_closes_at_unix_s =
            ceil_millis_to_seconds(denied.binding.appeal_window_closes_at_unix_ms).unwrap();
        refresh_hepta_preparation_hashes(&mut denied);
        let (denied_command_id, denied_commitment) =
            project_hepta_preparation_v2_to_v4("trnm-test-1", &denied).unwrap();
        assert_eq!(
            denied.binding.commitment_id,
            "sha256:b04857567f25bbdd59cf51b0fbf8f4ae854ae65a9c74e3c142666d8858f8ef46"
        );
        assert_eq!(
            denied.binding_fingerprint,
            "sha256:32b19b8c535bd592f084c04f8f38dcb296434123190d9102f408d14ab6743128"
        );
        assert_eq!(
            denied_command_id.to_hex(),
            "593c65760f470e1a68821ef1557c86efdba17019709b394e88d469547535e921"
        );
        assert_eq!(
            hex::encode(Sha256::digest(denied_commitment.canonical_bytes())),
            "bec1a9fe3af9a103d751df5fc26e98e8a01aa470d36568fa6bcc29b3bf5e6dd3"
        );
        assert_eq!(
            denied_commitment.appeal_status,
            PaperRaidAppealStatusV3::ResolvedDenied
        );
        assert_eq!(
            denied_commitment.appealed_evaluation_id,
            Some(denied_commitment.evaluation_id)
        );
        let denied_resolution_bound = denied_commitment.commitment_id;
        denied.binding.appeal_resolution_id =
            Some("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee".to_string());
        assert!(project_hepta_preparation_v2_to_v4("trnm-test-1", &denied).is_err());
        refresh_hepta_preparation_hashes(&mut denied);
        let (_, changed_resolution) =
            project_hepta_preparation_v2_to_v4("trnm-test-1", &denied).unwrap();
        assert_ne!(changed_resolution.commitment_id, denied_resolution_bound);

        let mut upheld = original;
        upheld.preparation_id = "13131313-1313-4131-8131-131313131313".to_string();
        upheld.binding.appeal_status = HeptaPaperRaidAppealStatusV2::ResolvedUpheld;
        upheld.binding.appeal_id = Some("cccccccc-cccc-4ccc-8ccc-cccccccccccc".to_string());
        let appealed = "ffffffff-ffff-4fff-8fff-ffffffffffff".to_string();
        upheld.binding.appealed_evaluation_id = Some(appealed.clone());
        upheld.binding.evaluation_supersedes_evaluation_id = Some(appealed);
        upheld.binding.appeal_resolution_id =
            Some("dddddddd-dddd-4ddd-8ddd-dddddddddddd".to_string());
        upheld.binding.appeal_resolution_hash = Some(hepta_digest('8'));
        upheld.binding.appeal_window_closes_at_unix_ms =
            upheld.binding.start_checkpoint_consensus_time_unix_ms + HEPTA_MAX_CHAIN_TIME_LAG_MS_V1;
        upheld.binding.appeal_window_closes_at_unix_s =
            ceil_millis_to_seconds(upheld.binding.appeal_window_closes_at_unix_ms).unwrap();
        refresh_hepta_preparation_hashes(&mut upheld);
        let (upheld_command_id, upheld_commitment) =
            project_hepta_preparation_v2_to_v4("trnm-test-1", &upheld).unwrap();
        assert_eq!(
            upheld.binding.commitment_id,
            "sha256:113048e1dc82d839fd2f2c271848351709d7f2f90a5fc406f17039a82fb30646"
        );
        assert_eq!(
            upheld.binding_fingerprint,
            "sha256:b34ea1f5294534acfae9ffbf5a955c192ca3e19b63de47b9a80ae09eb70d8cfe"
        );
        assert_eq!(
            upheld_command_id.to_hex(),
            "93f52eab32648869868bd2491db74d6d30c61c416f146b2d555bce4450b533f3"
        );
        assert_eq!(
            hex::encode(Sha256::digest(upheld_commitment.canonical_bytes())),
            "bf55f8db93955082f6ffda678674882765b7e9a79922e83cdce0543ea00d6929"
        );
        assert_eq!(
            upheld_commitment.appeal_status,
            PaperRaidAppealStatusV3::ResolvedUpheld
        );
        assert_eq!(
            upheld_commitment.evaluation_supersedes,
            upheld_commitment.appealed_evaluation_id
        );
    }

    #[test]
    fn hepta_signing_output_echoes_source_and_uses_null_for_original_lineage() {
        let directory = TestDirectory::new();
        let private_key_path = directory.path("hepta-private-key.hex");
        write_test_signing_key(&private_key_path, 0x58);
        let now = now_unix_ms().unwrap();
        let rework_preparation = hepta_preparation_v2();
        let rework_input = HeptaPaperRaidV4SignAndWrapInputV1 {
            schema: "trnm_hepta_paper_raid_v4_sign_and_wrap_input_v1".to_string(),
            chain_id: "trnm-test-1".to_string(),
            signer_did: "did:trnm:hepta-authority".to_string(),
            nonce: 12,
            max_gas: 300_000,
            fee_limit: 1_000_000,
            issued_at_unix_ms: now.saturating_sub(1_000),
            expires_at_unix_ms: now.saturating_add(60_000),
            preparation: rework_preparation.clone(),
        };
        let rework_input_path = directory.path("hepta-rework-input.json");
        let rework_output_path = directory.path("hepta-rework-output.json");
        let rework_tx_path = directory.path("hepta-rework-transaction.bin");
        fs::write(
            &rework_input_path,
            serde_json::to_vec(&rework_input).unwrap(),
        )
        .unwrap();
        paper_raid_v4_hepta_sign_and_wrap(
            &rework_input_path,
            &private_key_path,
            &rework_output_path,
            &rework_tx_path,
        )
        .unwrap();
        let output: HeptaPaperRaidSignedCommandOutputV4 =
            serde_json::from_slice(&fs::read(&rework_output_path).unwrap()).unwrap();
        assert_eq!(
            output.schema,
            "trnm_hepta_paper_raid_signed_command_output_v4"
        );
        assert_eq!(output.preparation_id, rework_preparation.preparation_id);
        assert_eq!(output.preparation_idempotency_key, "client.retry-token:v2");
        assert_eq!(
            output.binding_fingerprint,
            rework_preparation.binding_fingerprint
        );
        assert_eq!(
            output.source_commitment_id_sha256,
            rework_preparation.binding.commitment_id
        );
        assert_eq!(
            output.v4_commitment_id_hex,
            output
                .source_commitment_id_sha256
                .strip_prefix("sha256:")
                .unwrap()
        );
        assert_eq!(
            output.signed_command_cbor_sha256,
            format!(
                "sha256:{}",
                hex::encode(Sha256::digest(
                    hex::decode(&output.signed_command_cbor_hex).unwrap()
                ))
            )
        );
        let signed = SignedPaperRaidFinalityCommandV4::from_canonical_bytes(
            &hex::decode(&output.signed_command_cbor_hex).unwrap(),
        )
        .unwrap();
        assert_eq!(
            output.command_fingerprint_hex,
            hex::encode(signed.command_fingerprint())
        );
        assert_eq!(
            output.domain_payload_hash_hex,
            hex::encode(signed.payload_hash())
        );
        assert_eq!(output.rework_cycle, Some(2));

        let mut original_input = rework_input;
        original_input.nonce = 13;
        original_input.preparation.preparation_id =
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_string();
        original_input.preparation.binding.rework_lineage = None;
        refresh_hepta_preparation_hashes(&mut original_input.preparation);
        let original_input_path = directory.path("hepta-original-input.json");
        let original_output_path = directory.path("hepta-original-output.json");
        let original_tx_path = directory.path("hepta-original-transaction.bin");
        fs::write(
            &original_input_path,
            serde_json::to_vec(&original_input).unwrap(),
        )
        .unwrap();
        paper_raid_v4_hepta_sign_and_wrap(
            &original_input_path,
            &private_key_path,
            &original_output_path,
            &original_tx_path,
        )
        .unwrap();
        let original_json: Value =
            serde_json::from_slice(&fs::read(&original_output_path).unwrap()).unwrap();
        assert!(original_json["rework_id"].is_null());
        assert!(original_json["rework_cycle"].is_null());
        let original_output: HeptaPaperRaidSignedCommandOutputV4 =
            serde_json::from_value(original_json).unwrap();
        assert_eq!(original_output.rework_id, None);
        assert_eq!(original_output.rework_cycle, None);
    }

    #[test]
    fn pre_v7_paper_raid_v3_lane_marks_artifacts_historical_only() {
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
        write_test_signing_key(&private_key_path, 0x55);

        paper_raid_v3_pre_v7_artifact(
            &input_path,
            &private_key_path,
            &signed_output_path,
            &transaction_path,
        )
        .unwrap();

        let output: PaperRaidSignedCommandOutputV3 =
            serde_json::from_slice(&fs::read(&signed_output_path).unwrap()).unwrap();
        assert!(!output.broadcastable_on_consensus);
        assert_eq!(output.superseded_by_consensus_app_version, 7);
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
    fn app_v7_v4_signing_lane_preserves_exact_rework_native_command() {
        let directory = TestDirectory::new();
        let input_path = directory.path("v4-input.json");
        let private_key_path = directory.path("v4-private-key.hex");
        let signed_output_path = directory.path("v4-signed-command.json");
        let transaction_path = directory.path("v4-transaction.json");
        let now = now_unix_ms().unwrap();
        let mut input = PaperRaidSignAndWrapInputV4 {
            schema: "trnm_paper_raid_finality_sign_and_wrap_input_v4".to_string(),
            chain_id: "trnm-test-1".to_string(),
            command_id_hex: hex::encode([0x33; 32]),
            signer_did: "did:trnm:hepta-authority".to_string(),
            nonce: 11,
            max_gas: 300_000,
            fee_limit: 1_000_000,
            issued_at_unix_ms: now.saturating_sub(1_000),
            expires_at_unix_ms: now.saturating_add(60_000),
            commitment_cbor_hex: hex::encode(commitment_v4().canonical_bytes()),
        };
        fs::write(&input_path, serde_json::to_vec(&input).unwrap()).unwrap();
        write_test_signing_key(&private_key_path, 0x57);

        paper_raid_v4_sign_and_wrap(
            &input_path,
            &private_key_path,
            &signed_output_path,
            &transaction_path,
        )
        .unwrap();

        let output: PaperRaidSignedCommandOutputV4 =
            serde_json::from_slice(&fs::read(&signed_output_path).unwrap()).unwrap();
        assert_eq!(output.schema, "trnm_paper_raid_signed_command_output_v4");
        assert_eq!(output.required_consensus_app_version, 7);
        assert_eq!(
            output.rework_id,
            Some(
                commitment_v4()
                    .rework_lineage
                    .as_ref()
                    .unwrap()
                    .rework_id
                    .to_hex()
            )
        );
        assert_eq!(output.rework_cycle, Some(2));
        let signed_bytes = decode_canonical_hex(
            "signed_command_cbor_hex",
            &output.signed_command_cbor_hex,
            None,
            256 * 1024,
        )
        .unwrap();
        let signed = SignedPaperRaidFinalityCommandV4::from_canonical_bytes(&signed_bytes).unwrap();
        assert_eq!(signed.command_id.to_hex(), input.command_id_hex);
        assert_eq!(signed.commitment, commitment_v4());
        assert!(SignedPaperRaidFinalityCommandV3::from_canonical_bytes(&signed_bytes).is_err());
        assert_eq!(
            output.commitment_hash_hex,
            hex::encode(signed.payload_hash())
        );
        assert_eq!(
            output.applied_command_logical_key,
            paper_raid_finality_applied_command_key_v4(signed.command_id).unwrap()
        );

        let transaction_bytes = fs::read(&transaction_path).unwrap();
        let envelope =
            SignedCommandEnvelopeV1::from_canonical_wire_bytes(&transaction_bytes).unwrap();
        envelope.validate_at(&input.chain_id, now).unwrap();
        assert_eq!(
            envelope.payload_type,
            CANONICAL_PAPER_RAID_FINALITY_TX_PAYLOAD_TYPE_V4
        );
        let canonical_tx_bytes = envelope.payload_bytes().unwrap();
        assert_eq!(
            hex::encode(&canonical_tx_bytes),
            output.canonical_transaction_hex
        );
        let canonical_tx =
            CanonicalPaperRaidFinalityTxV4::from_canonical_bytes(&canonical_tx_bytes).unwrap();
        assert_eq!(
            canonical_tx.signed_paper_raid_finality_command().unwrap(),
            signed
        );
        assert_eq!(
            output.comet_tx_hash_hex,
            hex::encode(comet_tx_hash(&transaction_bytes))
        );

        let mut unlocked = commitment_v4();
        unlocked.score_eligible = true;
        unlocked.ranking_eligible = true;
        unlocked.reward_eligible = true;
        unlocked.economic_eligible = true;
        input.commitment_cbor_hex = hex::encode(unlocked.canonical_bytes());
        let unlocked_input_path = directory.path("v4-unlocked-input.json");
        fs::write(&unlocked_input_path, serde_json::to_vec(&input).unwrap()).unwrap();
        let error = paper_raid_v4_sign_and_wrap(
            &unlocked_input_path,
            &private_key_path,
            &directory.path("v4-unlocked-signed.json"),
            &directory.path("v4-unlocked-transaction.json"),
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
        write_test_signing_key(&private_key_path, 0x56);

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
