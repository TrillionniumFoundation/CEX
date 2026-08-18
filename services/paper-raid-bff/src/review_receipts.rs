use std::collections::BTreeSet;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use hepta_paper_raid_contracts::{
    canonical_json_bytes, canonical_json_sha256, paper_bundle_hash, review_execution_metrics_hash,
    review_execution_receipt_hash, verify_frozen_review_bundle, FrozenReviewBundleV1,
    PaperBundleV2, ReviewEvaluationExecutionResultV1, ReviewExecutionReceiptV1,
    ReviewExecutionScoreV1, ReviewExecutionToleranceRuleV1, ReviewReproductionExecutionResultV1,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    agent_bridge,
    app::{private_no_store, with_rotated_csrf, AppState},
    auth::AuthenticatedSession,
    config::{AlphaIdentity, AlphaIdentityScope},
    error::AppError,
    hepta::{BrowserCommand, CommandName, HumanSigningFrameRequest},
};

const RECEIPT_REQUEST_SCHEMA: &str = "hepta.paper_raid.agent_bridge.review_receipt_request.v1";
const NONE_STATISTICAL_EVIDENCE_SCHEMA: &str = "hepta.paper_raid.statistical_evidence.none.v1";
const NONE_STATISTICAL_EVIDENCE_REASON: &str = "frozen_evaluator_did_not_emit_statistical_evidence";
const CONFIRMATION_ID_DOMAIN: &str = "hepta.paper_raid.review_receipt_confirmation_idempotency.v1";
const CONFIRMATION_FRAME_SCHEMA: &str = "hepta.paper_raid.review_receipt_confirmation_frame.v1";
const CONFIRMATION_CONTEXT_SCHEMA: &str = "hepta.paper_raid.review_receipt_confirmation_context.v1";
const CONFIRMATION_CONTEXT_SIGNING_SCHEMA: &str =
    "hepta.paper_raid.review_receipt_confirmation_context_signing.v1";
const JSON_SAFE_I64_MAX: i64 = 9_007_199_254_740_991;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewReceiptSigningFrameRequest {
    coi_attestation_hash: String,
    #[serde(default)]
    score_components: Option<ReviewExecutionScoreV1>,
    #[serde(default)]
    observable_hard_gates: Option<ObservableHardGates>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservableHardGates {
    citations_and_data_authentic: bool,
    failed_runs_disclosed: bool,
    core_claims_have_evidence: bool,
    license_ethics_coi_complete: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewReceiptConfirmationRequest {
    signature: String,
    receipt_context_signature: String,
}

#[derive(Debug)]
struct StoredReviewReceipt {
    receipt_id: Uuid,
    task_id: Uuid,
    binding_id: Uuid,
    assignment_id: Uuid,
    paper_id: Uuid,
    submission_id: Uuid,
    evaluation_id: Uuid,
    kind: String,
    attempt: i64,
    fencing_token: i64,
    bundle_hash: String,
    receipt_hash: String,
    envelope: Value,
    state: String,
    confirmation_frame: Option<Value>,
    confirmation_frame_hash: Option<String>,
    confirmation_idempotency_key: Option<Uuid>,
    confirmation_hash: Option<String>,
    confirmation_context_signature: Option<String>,
    response_status: Option<i32>,
    response_body: Option<Vec<u8>>,
    subject_id: String,
    player_id: Uuid,
    agent_id: String,
    agent_key_id: String,
}

#[derive(Debug)]
enum TypedReviewOutput {
    Evaluation(ReviewEvaluationExecutionResultV1),
    Reproduction(ReviewReproductionExecutionResultV1),
}

impl TypedReviewOutput {
    fn as_value(&self) -> Result<Value, AppError> {
        match self {
            Self::Evaluation(output) => {
                serde_json::to_value(output).map_err(|_| AppError::Internal)
            }
            Self::Reproduction(output) => {
                serde_json::to_value(output).map_err(|_| AppError::Internal)
            }
        }
    }
}

struct ValidatedReviewReceipt {
    receipt: ReviewExecutionReceiptV1,
    output: TypedReviewOutput,
    run_manifest: Value,
}

impl StoredReviewReceipt {
    fn from_row(row: &PgRow) -> Result<Self, AppError> {
        Ok(Self {
            receipt_id: row.try_get("receipt_id")?,
            task_id: row.try_get("task_id")?,
            binding_id: row.try_get("binding_id")?,
            assignment_id: row.try_get("assignment_id")?,
            paper_id: row.try_get("paper_id")?,
            submission_id: row.try_get("submission_id")?,
            evaluation_id: row.try_get("evaluation_id")?,
            kind: row.try_get("kind")?,
            attempt: row.try_get("attempt")?,
            fencing_token: row.try_get("fencing_token")?,
            bundle_hash: row.try_get("bundle_hash")?,
            receipt_hash: row.try_get("receipt_hash")?,
            envelope: row.try_get("receipt")?,
            state: row.try_get("state")?,
            confirmation_frame: row.try_get("confirmation_frame")?,
            confirmation_frame_hash: row.try_get("confirmation_frame_hash")?,
            confirmation_idempotency_key: row.try_get("confirmation_idempotency_key")?,
            confirmation_hash: row.try_get("confirmation_hash")?,
            confirmation_context_signature: row.try_get("confirmation_context_signature")?,
            response_status: row.try_get("response_status")?,
            response_body: row.try_get("response_body")?,
            subject_id: row.try_get("subject_id")?,
            player_id: row.try_get("player_id")?,
            agent_id: row.try_get("agent_id")?,
            agent_key_id: row.try_get("agent_key_id")?,
        })
    }
}

const RECEIPT_SELECT: &str =
    "SELECT r.receipt_id,r.task_id,r.binding_id,r.assignment_id,r.paper_id,\
            r.submission_id,r.evaluation_id,r.kind,r.attempt,r.fencing_token,\
            r.bundle_hash,r.receipt_hash,r.receipt,r.state,r.confirmation_frame,\
            r.confirmation_frame_hash,r.confirmation_idempotency_key,r.confirmation_hash,\
            r.confirmation_context_signature,\
            r.response_status,r.response_body,b.subject_id,b.player_id,b.agent_id,b.agent_key_id \
     FROM paper_raid_bff_review_execution_receipts r \
     JOIN paper_raid_bff_agent_bridge_bindings b ON b.binding_id=r.binding_id";

async fn lock_receipt(
    tx: &mut Transaction<'_, Postgres>,
    identity: &AlphaIdentity,
    paper_id: Uuid,
    receipt_id: Uuid,
) -> Result<StoredReviewReceipt, AppError> {
    let row = sqlx::query(&format!(
        "{RECEIPT_SELECT} WHERE r.receipt_id=$1 AND r.paper_id=$2 \
         AND b.subject_id=$3 AND b.player_id=$4 FOR UPDATE OF r"
    ))
    .bind(receipt_id)
    .bind(paper_id)
    .bind(&identity.subject_id)
    .bind(identity.player_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(AppError::Forbidden)?;
    StoredReviewReceipt::from_row(&row)
}

fn exact_object_keys(value: &Value, expected: &[&str]) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    actual == expected
}

fn is_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|raw| {
        raw.len() == 64
            && raw
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn safe_metric_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
        && value.as_bytes()[0].is_ascii_alphanumeric()
}

fn json_safe_i64(value: i64) -> bool {
    (-JSON_SAFE_I64_MAX..=JSON_SAFE_I64_MAX).contains(&value)
}

fn validate_evaluation_output(
    value: Value,
    receipt: &ReviewExecutionReceiptV1,
) -> Result<TypedReviewOutput, AppError> {
    let output: ReviewEvaluationExecutionResultV1 = serde_json::from_value(value)
        .map_err(|_| AppError::Invalid("invalid typed evaluation receipt output".into()))?;
    if output.reference_metrics_micros.is_empty()
        || output.reference_metrics_micros.len() > 256
        || output
            .reference_metrics_micros
            .keys()
            .any(|metric| !safe_metric_key(metric))
        || output.tolerance_policy_version != "1"
        || output.tolerance_rules.is_empty()
        || output
            .reference_metrics_micros
            .values()
            .any(|value| !json_safe_i64(*value))
        || output.tolerance_rules.len() > 128
    {
        return Err(AppError::Invalid(
            "evaluation receipt output violates the bounded v1 schema".into(),
        ));
    }
    if receipt.candidate_passed != Some(output.candidate_passed) {
        return Err(AppError::Conflict(
            "evaluation receipt outcome differs from its typed output".into(),
        ));
    }
    let mut rules = BTreeSet::new();
    for rule in &output.tolerance_rules {
        let (key, valid) = match rule {
            ReviewExecutionToleranceRuleV1::Absolute {
                metric,
                max_delta_micros,
            } => (
                format!("absolute:{metric}"),
                output.reference_metrics_micros.contains_key(metric)
                    && *max_delta_micros >= 0
                    && json_safe_i64(*max_delta_micros),
            ),
            ReviewExecutionToleranceRuleV1::Relative {
                metric,
                max_delta_bps,
            } => (
                format!("relative:{metric}"),
                output.reference_metrics_micros.contains_key(metric) && *max_delta_bps <= 10_000,
            ),
            ReviewExecutionToleranceRuleV1::Statistical {
                metric,
                minimum_interval_overlap_bps,
                maximum_effect_delta_micros,
                minimum_p_value_micros,
            } => (
                format!("statistical:{metric}"),
                output.reference_metrics_micros.contains_key(metric)
                    && *minimum_interval_overlap_bps <= 10_000
                    && *maximum_effect_delta_micros >= 0
                    && json_safe_i64(*maximum_effect_delta_micros)
                    && *minimum_p_value_micros <= 1_000_000,
            ),
            ReviewExecutionToleranceRuleV1::Seed {
                expected_seed_set_hash,
            } => (
                format!("seed:{expected_seed_set_hash}"),
                is_digest(expected_seed_set_hash),
            ),
        };
        if !valid || !rules.insert(key) {
            return Err(AppError::Invalid(
                "evaluation tolerance rule is invalid or duplicated".into(),
            ));
        }
    }
    let none = json!({
        "schema": NONE_STATISTICAL_EVIDENCE_SCHEMA,
        "reason": NONE_STATISTICAL_EVIDENCE_REASON,
    });
    if receipt.observed_metrics_micros != output.reference_metrics_micros
        || receipt.statistical_evidence != none
    {
        return Err(AppError::Conflict(
            "evaluation receipt metrics differ from its typed output".into(),
        ));
    }
    Ok(TypedReviewOutput::Evaluation(output))
}

fn validate_human_score(score: &ReviewExecutionScoreV1) -> Result<(), AppError> {
    let components = [
        (score.method_rigor_bps, 2_500_u16),
        (score.experiment_statistics_bps, 1_500),
        (score.reproducibility_bps, 1_500),
        (score.evidence_citations_bps, 1_500),
        (score.value_originality_bps, 1_500),
        (score.argument_expression_bps, 1_000),
        (score.ethics_transparency_bps, 500),
    ];
    if components.iter().any(|(value, maximum)| value > maximum)
        || components
            .iter()
            .map(|(value, _)| u32::from(*value))
            .sum::<u32>()
            > 10_000
    {
        return Err(AppError::Invalid(
            "human evaluation score exceeds its authoritative bounds".into(),
        ));
    }
    Ok(())
}

fn validate_reproduction_output(
    value: Value,
    receipt: &ReviewExecutionReceiptV1,
) -> Result<TypedReviewOutput, AppError> {
    let output: ReviewReproductionExecutionResultV1 = serde_json::from_value(value)
        .map_err(|_| AppError::Invalid("invalid typed reproduction receipt output".into()))?;
    if output.observed_metrics_micros.is_empty()
        || output.observed_metrics_micros.len() > 256
        || output
            .observed_metrics_micros
            .keys()
            .any(|metric| !safe_metric_key(metric))
        || output
            .observed_metrics_micros
            .values()
            .any(|value| !json_safe_i64(*value))
        || output
            .statistical_evidence
            .iter()
            .any(|(metric, evidence)| {
                !output.observed_metrics_micros.contains_key(metric)
                    || evidence.interval_overlap_bps > 10_000
                    || !json_safe_i64(evidence.effect_delta_micros)
                    || evidence.p_value_micros > 1_000_000
            })
        || receipt.candidate_passed.is_some()
    {
        return Err(AppError::Invalid(
            "reproduction receipt output violates the bounded v1 schema".into(),
        ));
    }
    let evidence =
        serde_json::to_value(&output.statistical_evidence).map_err(|_| AppError::Internal)?;
    if receipt.observed_metrics_micros != output.observed_metrics_micros
        || receipt.statistical_evidence != evidence
    {
        return Err(AppError::Conflict(
            "reproduction receipt metrics differ from its typed output".into(),
        ));
    }
    Ok(TypedReviewOutput::Reproduction(output))
}

fn validate_run_manifest(
    manifest: &Value,
    environment: &Value,
    logs: &Value,
    stored: &StoredReviewReceipt,
    receipt: &ReviewExecutionReceiptV1,
    descriptor: &FrozenReviewBundleV1,
) -> Result<(), AppError> {
    const REQUIRED: &[&str] = &[
        "schema",
        "task_id",
        "assignment_id",
        "paper_project_id",
        "submission_id",
        "evaluation_id",
        "kind",
        "attempt",
        "fencing_token",
        "bundle_hash",
        "evaluator_version",
        "adapter",
        "entrypoint",
        "timeout_ms",
        "candidate_passed",
        "input_objects",
        "input_root",
        "output_root",
        "metrics_hash",
        "seed",
        "seed_set_hash",
        "environment_hash",
        "logs_hash",
        "exit_code",
        "elapsed_ms",
        "started_at_unix",
        "completed_at_unix",
    ];
    let Some(object) = manifest.as_object() else {
        return Err(AppError::Invalid(
            "review run manifest is not an object".into(),
        ));
    };
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let required = REQUIRED.iter().copied().collect::<BTreeSet<_>>();
    let mut allowed_with_timeout = required.clone();
    allowed_with_timeout.insert("timed_out");
    if actual != required && actual != allowed_with_timeout {
        return Err(AppError::Invalid(
            "review run manifest contains unsupported or missing fields".into(),
        ));
    }
    if object
        .get("timed_out")
        .is_some_and(|value| value != &Value::Bool(false))
    {
        return Err(AppError::Invalid(
            "timed-out execution cannot produce a consumable receipt".into(),
        ));
    }
    let expected = [
        ("task_id", json!(stored.task_id)),
        ("assignment_id", json!(stored.assignment_id)),
        ("paper_project_id", json!(stored.paper_id)),
        ("submission_id", json!(stored.submission_id)),
        ("evaluation_id", json!(stored.evaluation_id)),
        ("kind", json!(&stored.kind)),
        ("attempt", json!(stored.attempt)),
        ("fencing_token", json!(stored.fencing_token)),
        ("bundle_hash", json!(&stored.bundle_hash)),
        (
            "evaluator_version",
            json!(&descriptor.execution.evaluator_version),
        ),
        ("adapter", json!(&descriptor.execution.adapter)),
        ("entrypoint", json!(&descriptor.execution.entrypoint)),
        ("timeout_ms", json!(descriptor.execution.timeout_ms)),
        ("candidate_passed", json!(receipt.candidate_passed)),
        (
            "input_objects",
            json!(descriptor
                .objects
                .iter()
                .map(|object| json!({
                    "object_key": object.object_key,
                    "logical_path": object.logical_path,
                    "role": object.role,
                    "digest": object.digest,
                    "size_bytes": object.size_bytes,
                }))
                .collect::<Vec<_>>()),
        ),
        ("input_root", json!(&receipt.input_root)),
        ("output_root", json!(&receipt.output_root)),
        ("metrics_hash", json!(&receipt.metrics_hash)),
        ("seed", json!(descriptor.execution.seed)),
        ("seed_set_hash", json!(&receipt.seed_set_hash)),
        ("environment_hash", json!(&receipt.environment_hash)),
        ("logs_hash", json!(&receipt.logs_hash)),
        ("started_at_unix", json!(receipt.started_at_unix)),
        ("completed_at_unix", json!(receipt.completed_at_unix)),
    ];
    if manifest.get("schema").and_then(Value::as_str)
        != Some("hepta.paper_raid.review_run_manifest.v1")
        || expected
            .iter()
            .any(|(field, expected)| manifest.get(field) != Some(expected))
        || !matches!(
            manifest.get("exit_code").and_then(Value::as_i64),
            Some(0 | 1)
        )
    {
        return Err(AppError::Conflict(
            "review run manifest differs from receipt or frozen execution authority".into(),
        ));
    }
    let exit_code = manifest
        .get("exit_code")
        .and_then(Value::as_i64)
        .ok_or(AppError::Internal)?;
    if stored.kind == "evaluate" && receipt.candidate_passed != Some(exit_code == 0) {
        return Err(AppError::Conflict(
            "evaluation outcome differs from its frozen evaluator exit code".into(),
        ));
    }
    let manifest_elapsed_ms = manifest
        .get("elapsed_ms")
        .and_then(Value::as_u64)
        .filter(|elapsed| *elapsed <= descriptor.execution.timeout_ms)
        .ok_or_else(|| AppError::Invalid("review execution elapsed_ms is invalid".into()))?;
    let elapsed = receipt
        .completed_at_unix
        .checked_sub(receipt.started_at_unix)
        .ok_or_else(|| AppError::Invalid("review receipt time is invalid".into()))?;
    let wall_elapsed_ms = u128::try_from(elapsed)
        .map_err(|_| AppError::Invalid("review receipt time is invalid".into()))?
        * 1_000;
    if wall_elapsed_ms > u128::from(descriptor.execution.timeout_ms) + 999
        || manifest_elapsed_ms > descriptor.execution.timeout_ms
    {
        return Err(AppError::Conflict(
            "review execution exceeded its frozen timeout".into(),
        ));
    }
    let input_objects = manifest.get("input_objects").ok_or(AppError::Internal)?;
    if canonical_json_sha256(input_objects).ok().as_deref() != Some(receipt.input_root.as_str())
        || canonical_json_sha256(&json!([descriptor.execution.seed]))
            .ok()
            .as_deref()
            != Some(receipt.seed_set_hash.as_str())
    {
        return Err(AppError::Conflict(
            "review input or seed seal differs from the frozen execution authority".into(),
        ));
    }

    if !exact_object_keys(
        environment,
        &[
            "schema",
            "adapter",
            "bridge_version",
            "runtime",
            "runtime_path",
            "runtime_flags",
            "loader_digest",
            "evaluator_digest",
            "support_digests",
            "review_kind",
            "platform",
            "architecture",
        ],
    ) || environment.get("schema").and_then(Value::as_str)
        != Some("hepta.paper_raid.review_execution_environment.v1")
        || environment.get("adapter").and_then(Value::as_str)
            != Some(descriptor.execution.adapter.as_str())
        || environment.get("runtime").and_then(Value::as_str) != Some("python3-stdlib")
        || environment.get("runtime_path").and_then(Value::as_str) != Some("/usr/bin/python3")
        || environment.get("runtime_flags") != Some(&json!(["-I", "-S", "-B"]))
        || environment
            .get("loader_digest")
            .and_then(Value::as_str)
            .is_none_or(|digest| !is_digest(digest))
        || environment.get("evaluator_digest").and_then(Value::as_str)
            != Some(descriptor.execution.evaluator_version.as_str())
        || environment.get("review_kind").and_then(Value::as_str) != Some(stored.kind.as_str())
        || ["bridge_version", "platform", "architecture"]
            .iter()
            .any(|field| {
                environment
                    .get(*field)
                    .and_then(Value::as_str)
                    .is_none_or(|value| value.is_empty() || value.len() > 128)
            })
    {
        return Err(AppError::Invalid(
            "review execution environment is not the sealed Bridge runtime".into(),
        ));
    }
    let expected_support = descriptor
        .objects
        .iter()
        .filter(|object| object.role == "evaluator_support")
        .map(|object| Value::String(object.digest.clone()))
        .collect::<Vec<_>>();
    if environment.get("support_digests") != Some(&Value::Array(expected_support)) {
        return Err(AppError::Conflict(
            "review environment support digests differ from the frozen bundle".into(),
        ));
    }
    if !exact_object_keys(
        logs,
        &[
            "schema",
            "stdout_hash",
            "stderr_hash",
            "stdout_bytes",
            "stderr_bytes",
            "truncated",
        ],
    ) || logs.get("schema").and_then(Value::as_str)
        != Some("hepta.paper_raid.review_execution_logs.v1")
        || logs.get("truncated") != Some(&Value::Bool(false))
        || ["stdout_hash", "stderr_hash"].iter().any(|field| {
            logs.get(*field)
                .and_then(Value::as_str)
                .is_none_or(|digest| !is_digest(digest))
        })
        || ["stdout_bytes", "stderr_bytes"].iter().any(|field| {
            logs.get(*field)
                .and_then(Value::as_u64)
                .is_none_or(|bytes| bytes > 256 * 1024)
        })
    {
        return Err(AppError::Invalid(
            "review execution log seal is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_stored_receipt(
    stored: &StoredReviewReceipt,
    identity: &AlphaIdentity,
    descriptor: &FrozenReviewBundleV1,
) -> Result<ValidatedReviewReceipt, AppError> {
    if stored.subject_id != identity.subject_id
        || stored.player_id != identity.player_id
        || stored.paper_id != descriptor.paper_project_id
        || stored.submission_id != descriptor.submission_id
        || stored.assignment_id != descriptor.assignment_id
        || stored.bundle_hash != descriptor.bundle_hash
        || stored.fencing_token != i64::try_from(descriptor.assignment_version).unwrap_or(-1)
        || stored.kind != descriptor.execution.kind
        || (stored.kind == "evaluate" && descriptor.slot != "evaluator")
        || (stored.kind == "reproduce" && descriptor.slot != "reproducer")
    {
        return Err(AppError::Conflict(
            "review receipt authority tuple is no longer active".into(),
        ));
    }
    if !exact_object_keys(
        &stored.envelope,
        &[
            "schema",
            "idempotency_key",
            "receipt",
            "output",
            "environment",
            "run_manifest",
            "logs",
        ],
    ) || stored.envelope.get("schema").and_then(Value::as_str) != Some(RECEIPT_REQUEST_SCHEMA)
        || stored
            .envelope
            .get("idempotency_key")
            .and_then(Value::as_str)
            != Some(stored.receipt_id.to_string().as_str())
    {
        return Err(AppError::Internal);
    }
    let receipt: ReviewExecutionReceiptV1 = serde_json::from_value(
        stored
            .envelope
            .get("receipt")
            .cloned()
            .ok_or(AppError::Internal)?,
    )
    .map_err(|_| AppError::Internal)?;
    if receipt.receipt_id != stored.receipt_id
        || receipt.task_id != stored.task_id
        || receipt.binding_id != stored.binding_id
        || receipt.assignment_id != stored.assignment_id
        || receipt.paper_project_id != stored.paper_id
        || receipt.submission_id != stored.submission_id
        || receipt.evaluation_id != stored.evaluation_id
        || receipt.kind != stored.kind
        || i64::try_from(receipt.attempt).ok() != Some(stored.attempt)
        || i64::try_from(receipt.fencing_token).ok() != Some(stored.fencing_token)
        || receipt.bundle_hash != stored.bundle_hash
        || receipt.evaluator_version != descriptor.execution.evaluator_version
        || receipt.agent_id != stored.agent_id
        || receipt.agent_key_id != stored.agent_key_id
        || review_execution_receipt_hash(&receipt).ok().as_deref()
            != Some(stored.receipt_hash.as_str())
    {
        return Err(AppError::Conflict(
            "stored review receipt envelope differs from its indexed authority".into(),
        ));
    }
    let output_value = stored
        .envelope
        .get("output")
        .cloned()
        .ok_or(AppError::Internal)?;
    if canonical_json_sha256(&output_value).ok().as_deref() != Some(receipt.output_root.as_str())
        || canonical_json_sha256(
            stored
                .envelope
                .get("environment")
                .ok_or(AppError::Internal)?,
        )
        .ok()
        .as_deref()
            != Some(receipt.environment_hash.as_str())
        || canonical_json_sha256(stored.envelope.get("logs").ok_or(AppError::Internal)?)
            .ok()
            .as_deref()
            != Some(receipt.logs_hash.as_str())
        || review_execution_metrics_hash(&receipt).ok().as_deref()
            != Some(receipt.metrics_hash.as_str())
    {
        return Err(AppError::Conflict(
            "stored review receipt seals do not match its immutable result".into(),
        ));
    }
    let output = match stored.kind.as_str() {
        "evaluate" => validate_evaluation_output(output_value, &receipt)?,
        "reproduce" => validate_reproduction_output(output_value, &receipt)?,
        _ => return Err(AppError::Internal),
    };
    let environment = stored
        .envelope
        .get("environment")
        .cloned()
        .ok_or(AppError::Internal)?;
    let logs = stored
        .envelope
        .get("logs")
        .cloned()
        .ok_or(AppError::Internal)?;
    let run_manifest = stored
        .envelope
        .get("run_manifest")
        .cloned()
        .ok_or(AppError::Internal)?;
    if canonical_json_sha256(&run_manifest).ok().as_deref()
        != Some(receipt.run_manifest_hash.as_str())
    {
        return Err(AppError::Conflict(
            "stored review run manifest hash is invalid".into(),
        ));
    }
    validate_run_manifest(
        &run_manifest,
        &environment,
        &logs,
        stored,
        &receipt,
        descriptor,
    )?;
    Ok(ValidatedReviewReceipt {
        receipt,
        output,
        run_manifest,
    })
}

fn current_assignment_status<'a>(
    bundle: &'a Value,
    identity: &AlphaIdentity,
    stored: &StoredReviewReceipt,
) -> Option<&'a str> {
    let expected_assignment = stored.assignment_id.to_string();
    let expected_paper = stored.paper_id.to_string();
    let expected_submission = stored.submission_id.to_string();
    let expected_player = identity.player_id.to_string();
    let assignments = bundle.get("my_assignments").and_then(Value::as_array)?;
    if assignments.len() != 1
        || assignments[0].get("assignment_id").and_then(Value::as_str)
            != Some(expected_assignment.as_str())
        || assignments[0]
            .get("paper_project_id")
            .and_then(Value::as_str)
            != Some(expected_paper.as_str())
        || assignments[0].get("submission_id").and_then(Value::as_str)
            != Some(expected_submission.as_str())
        || assignments[0].get("player_id").and_then(Value::as_str) != Some(expected_player.as_str())
        || assignments[0].get("version").and_then(Value::as_i64) != Some(stored.fencing_token)
    {
        return None;
    }
    assignments[0]
        .get("status")
        .and_then(Value::as_str)
        .filter(|status| matches!(*status, "claimed" | "pinned"))
}

async fn current_authority(
    state: &AppState,
    identity: &AlphaIdentity,
    stored: &StoredReviewReceipt,
) -> Result<(Value, FrozenReviewBundleV1, ValidatedReviewReceipt), AppError> {
    let expected_scope = match stored.kind.as_str() {
        "evaluate" => AlphaIdentityScope::Evaluator,
        "reproduce" => AlphaIdentityScope::Reproducer,
        _ => return Err(AppError::Internal),
    };
    if !identity.has_scope(expected_scope) {
        return Err(AppError::Forbidden);
    }
    let hepta_bundle = state
        .hepta
        .get_paper_review_bundle(identity, stored.paper_id)
        .await?;
    let assignment_status = current_assignment_status(&hepta_bundle, identity, stored)
        .ok_or_else(|| AppError::Conflict("review assignment expired or changed version".into()))?;
    let resolved =
        agent_bridge::resolve_frozen_review_bundle(state, &hepta_bundle, identity.player_id)
            .await?;
    let descriptor: FrozenReviewBundleV1 = serde_json::from_value(
        resolved
            .get("resolved_frozen_review_bundle")
            .cloned()
            .ok_or(AppError::Upstream)?,
    )
    .map_err(|_| AppError::Upstream)?;
    verify_frozen_review_bundle(&descriptor).map_err(|_| AppError::Upstream)?;
    let expires_at = DateTime::parse_from_rfc3339(&descriptor.expires_at)
        .map_err(|_| AppError::Upstream)?
        .with_timezone(&Utc);
    if assignment_status == "claimed" && expires_at <= Utc::now() {
        return Err(AppError::Conflict("review assignment expired".into()));
    }
    if stored.kind == "reproduce" {
        let current_evaluation = resolved
            .get("evaluation")
            .and_then(|value| value.get("evaluation_id"))
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok());
        if current_evaluation != Some(stored.evaluation_id) {
            return Err(AppError::Conflict(
                "reproduction receipt names a non-current evaluation".into(),
            ));
        }
    }
    let validated = validate_stored_receipt(stored, identity, &descriptor)?;
    if validated.receipt.completed_at_unix > expires_at.timestamp() {
        return Err(AppError::Conflict(
            "review execution completed after the assignment claim expired".into(),
        ));
    }
    Ok((resolved, descriptor, validated))
}

fn authoritative_hard_gates(
    bundle: &Value,
    descriptor: &FrozenReviewBundleV1,
    human: &ObservableHardGates,
) -> Result<Value, AppError> {
    let paper_bundle: PaperBundleV2 = serde_json::from_value(
        bundle
            .get("paper_bundle")
            .cloned()
            .ok_or(AppError::Upstream)?,
    )
    .map_err(|_| AppError::Upstream)?;
    if paper_bundle_hash(&paper_bundle).ok().as_deref()
        != Some(paper_bundle.paper_bundle_hash.as_str())
        || paper_bundle.paper_bundle_hash != descriptor.paper_bundle_hash
        || paper_bundle.release_candidate.artifact_manifest_hash
            != descriptor.artifact_manifest_hash
    {
        return Err(AppError::Conflict(
            "PaperBundle authority differs from the frozen review descriptor".into(),
        ));
    }
    let authors = paper_bundle
        .release_candidate
        .authors
        .iter()
        .map(|author| author.player_id)
        .collect::<BTreeSet<_>>();
    let consents = paper_bundle
        .author_consents
        .iter()
        .map(|consent| consent.player_id)
        .collect::<BTreeSet<_>>();
    let all_authors_consented = authors.len() == paper_bundle.release_candidate.authors.len()
        && consents.len() == paper_bundle.author_consents.len()
        && authors == consents;
    // This gate is derived only from the verified PaperBundle and Hepta's exact frozen
    // ArtifactManifest authority.  The authority deliberately also contains human-review
    // material which must never enter the Agent executable descriptor, so exactness is a
    // bijection between the executable subset on each side rather than whole-vector equality.
    // Challenge-pack defaults or browser booleans cannot satisfy it.
    let artifact_lineage_complete = executable_artifact_lineage_complete(descriptor);
    Ok(json!({
        "citations_and_data_authentic": human.citations_and_data_authentic,
        "failed_runs_disclosed": human.failed_runs_disclosed,
        "all_authors_consented": all_authors_consented,
        "core_claims_have_evidence": human.core_claims_have_evidence,
        "artifact_lineage_complete": artifact_lineage_complete,
        "license_ethics_coi_complete": human.license_ethics_coi_complete,
    }))
}

fn executable_artifact_lineage_complete(descriptor: &FrozenReviewBundleV1) -> bool {
    let executable_role = |role: &str| {
        matches!(
            role,
            "candidate" | "dataset" | "evaluator_support" | "frozen_evaluator"
        )
    };
    let exact_transport_binding =
        |resolved: &hepta_paper_raid_contracts::FrozenReviewObjectV1,
         authority: &hepta_paper_raid_contracts::FrozenReviewObjectV1| {
            resolved.object_key == authority.object_key
                && resolved.role == authority.role
                && resolved.digest == authority.digest
                && resolved.size_bytes == authority.size_bytes
                && resolved.media_type == authority.media_type
                && resolved.download_path == authority.download_path
        };
    !descriptor.objects.is_empty()
        && descriptor.objects.iter().all(|resolved| {
            executable_role(&resolved.role)
                && is_digest(&resolved.digest)
                && resolved.size_bytes > 0
                && descriptor
                    .authority
                    .artifact_objects
                    .iter()
                    .filter(|authority| exact_transport_binding(resolved, authority))
                    .count()
                    == 1
        })
        && descriptor
            .authority
            .artifact_objects
            .iter()
            .filter(|authority| executable_role(&authority.role))
            .all(|authority| {
                is_digest(&authority.digest)
                    && authority.size_bytes > 0
                    && descriptor
                        .objects
                        .iter()
                        .filter(|resolved| exact_transport_binding(resolved, authority))
                        .count()
                        == 1
            })
}

fn stable_uuid(domain: &str, fields: &[&str]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(domain.as_bytes());
    digest.update([0]);
    for field in fields {
        digest.update(field.as_bytes());
        digest.update([0]);
    }
    let mut bytes: [u8; 16] = digest.finalize()[..16]
        .try_into()
        .expect("sha256 prefix has fixed length");
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn confirmation_idempotency_key(stored: &StoredReviewReceipt, frame_hash: &str) -> Uuid {
    stable_uuid(
        CONFIRMATION_ID_DOMAIN,
        &[
            &stored.receipt_id.to_string(),
            &stored.evaluation_id.to_string(),
            frame_hash,
        ],
    )
}

fn confirmation_context(
    stored: &StoredReviewReceipt,
    validated: &ValidatedReviewReceipt,
    descriptor: &FrozenReviewBundleV1,
) -> Value {
    json!({
        "schema": CONFIRMATION_CONTEXT_SCHEMA,
        "receipt_id": stored.receipt_id,
        "receipt_hash": stored.receipt_hash,
        "task_id": stored.task_id,
        "assignment_id": stored.assignment_id,
        "assignment_version": stored.fencing_token,
        "paper_project_id": stored.paper_id,
        "submission_id": stored.submission_id,
        "evaluation_id": stored.evaluation_id,
        "kind": stored.kind,
        "bundle_hash": stored.bundle_hash,
        "release_candidate_hash": descriptor.release_candidate_hash,
        "paper_bundle_hash": descriptor.paper_bundle_hash,
        "artifact_manifest_hash": descriptor.artifact_manifest_hash,
        "evaluator_version": validated.receipt.evaluator_version,
        "agent_id": stored.agent_id,
        "agent_key_id": stored.agent_key_id,
        "input_root": validated.receipt.input_root,
        "output_root": validated.receipt.output_root,
        "metrics_hash": validated.receipt.metrics_hash,
        "candidate_passed": validated.receipt.candidate_passed,
        "seed_set_hash": validated.receipt.seed_set_hash,
        "environment_hash": validated.receipt.environment_hash,
        "run_manifest_hash": validated.receipt.run_manifest_hash,
        "logs_hash": validated.receipt.logs_hash,
        "completed_at_unix": validated.receipt.completed_at_unix,
    })
}

fn confirmation_context_signing_claim(frame: &Value, context: &Value) -> Result<Value, AppError> {
    let command = frame.get("command").cloned().ok_or(AppError::Upstream)?;
    let resource_id = frame
        .get("resource_id")
        .cloned()
        .ok_or(AppError::Upstream)?;
    let child_id = frame.get("child_id").cloned().ok_or(AppError::Upstream)?;
    let upstream_signing_bytes = frame
        .get("signing_bytes")
        .and_then(Value::as_str)
        .filter(|value| BASE64.decode(value).is_ok())
        .ok_or(AppError::Upstream)?;
    let signing_key_id = frame
        .get("signing_key_id")
        .cloned()
        .ok_or(AppError::Upstream)?;
    let signing_public_key_hash = frame
        .get("signing_public_key_hash")
        .cloned()
        .ok_or(AppError::Upstream)?;
    Ok(json!({
        "schema": CONFIRMATION_CONTEXT_SIGNING_SCHEMA,
        "receipt_context": context,
        "command": command,
        "resource_id": resource_id,
        "child_id": child_id,
        "upstream_signing_bytes": upstream_signing_bytes,
        "signing_key_id": signing_key_id,
        "signing_public_key_hash": signing_public_key_hash,
    }))
}

fn attach_confirmation_context(frame: Value, context: Value) -> Result<Value, AppError> {
    if !exact_object_keys(
        &frame,
        &[
            "command",
            "resource_id",
            "child_id",
            "payload",
            "signing_bytes",
            "signing_key_id",
            "signing_public_key",
            "signing_public_key_hash",
        ],
    ) {
        return Err(AppError::Upstream);
    }
    let context_signing_claim = confirmation_context_signing_claim(&frame, &context)?;
    let context_signing_bytes =
        canonical_json_bytes(&context_signing_claim).map_err(|_| AppError::Internal)?;
    let mut frame = frame.as_object().cloned().ok_or(AppError::Upstream)?;
    frame.insert("schema".into(), json!(CONFIRMATION_FRAME_SCHEMA));
    frame.insert("receipt_context".into(), context);
    frame.insert(
        "receipt_context_signing_bytes".into(),
        json!(BASE64.encode(context_signing_bytes)),
    );
    Ok(Value::Object(frame))
}

fn context_signing_bytes_match_frame(frame: &Value) -> bool {
    let Some(context) = frame.get("receipt_context") else {
        return false;
    };
    let Some(actual) = frame
        .get("receipt_context_signing_bytes")
        .and_then(Value::as_str)
        .and_then(|value| BASE64.decode(value).ok())
    else {
        return false;
    };
    confirmation_context_signing_claim(frame, context)
        .and_then(|claim| canonical_json_bytes(&claim).map_err(|_| AppError::Internal))
        .is_ok_and(|expected| expected == actual)
}

fn context_matches_stored(frame: &Value, stored: &StoredReviewReceipt) -> bool {
    let Some(context) = frame.get("receipt_context") else {
        return false;
    };
    if !exact_object_keys(
        context,
        &[
            "schema",
            "receipt_id",
            "receipt_hash",
            "task_id",
            "assignment_id",
            "assignment_version",
            "paper_project_id",
            "submission_id",
            "evaluation_id",
            "kind",
            "bundle_hash",
            "release_candidate_hash",
            "paper_bundle_hash",
            "artifact_manifest_hash",
            "evaluator_version",
            "agent_id",
            "agent_key_id",
            "input_root",
            "output_root",
            "metrics_hash",
            "candidate_passed",
            "seed_set_hash",
            "environment_hash",
            "run_manifest_hash",
            "logs_hash",
            "completed_at_unix",
        ],
    ) || context.get("schema").and_then(Value::as_str) != Some(CONFIRMATION_CONTEXT_SCHEMA)
    {
        return false;
    }
    let receipt = stored
        .envelope
        .get("receipt")
        .cloned()
        .and_then(|value| serde_json::from_value::<ReviewExecutionReceiptV1>(value).ok());
    let Some(receipt) = receipt else {
        return false;
    };
    if review_execution_receipt_hash(&receipt).ok().as_deref() != Some(stored.receipt_hash.as_str())
    {
        return false;
    }
    let expected = [
        ("receipt_id", json!(stored.receipt_id)),
        ("receipt_hash", json!(&stored.receipt_hash)),
        ("task_id", json!(stored.task_id)),
        ("assignment_id", json!(stored.assignment_id)),
        ("assignment_version", json!(stored.fencing_token)),
        ("paper_project_id", json!(stored.paper_id)),
        ("submission_id", json!(stored.submission_id)),
        ("evaluation_id", json!(stored.evaluation_id)),
        ("kind", json!(&stored.kind)),
        ("bundle_hash", json!(&stored.bundle_hash)),
        ("evaluator_version", json!(&receipt.evaluator_version)),
        ("agent_id", json!(&stored.agent_id)),
        ("agent_key_id", json!(&stored.agent_key_id)),
        ("input_root", json!(&receipt.input_root)),
        ("output_root", json!(&receipt.output_root)),
        ("metrics_hash", json!(&receipt.metrics_hash)),
        ("candidate_passed", json!(receipt.candidate_passed)),
        ("seed_set_hash", json!(&receipt.seed_set_hash)),
        ("environment_hash", json!(&receipt.environment_hash)),
        ("run_manifest_hash", json!(&receipt.run_manifest_hash)),
        ("logs_hash", json!(&receipt.logs_hash)),
        ("completed_at_unix", json!(receipt.completed_at_unix)),
    ];
    expected
        .iter()
        .all(|(field, expected)| context.get(field) == Some(expected))
        && [
            "release_candidate_hash",
            "paper_bundle_hash",
            "artifact_manifest_hash",
        ]
        .iter()
        .all(|field| {
            context
                .get(*field)
                .and_then(Value::as_str)
                .is_some_and(is_digest)
        })
}

fn confirmation_frame_request(
    stored: &StoredReviewReceipt,
    validated: &ValidatedReviewReceipt,
    bundle: &Value,
    descriptor: &FrozenReviewBundleV1,
    request: &ReviewReceiptSigningFrameRequest,
) -> Result<HumanSigningFrameRequest, AppError> {
    if !is_digest(&request.coi_attestation_hash) {
        return Err(AppError::Invalid(
            "conflict-of-interest attestation hash is invalid".into(),
        ));
    }
    match &validated.output {
        TypedReviewOutput::Evaluation(output) => {
            let score = request
                .score_components
                .as_ref()
                .ok_or_else(|| AppError::Invalid("human evaluation score is required".into()))?;
            let observable = request.observable_hard_gates.as_ref().ok_or_else(|| {
                AppError::Invalid("human hard-gate confirmation is required".into())
            })?;
            validate_human_score(score)?;
            let hard_gates = authoritative_hard_gates(bundle, descriptor, observable)?;
            Ok(HumanSigningFrameRequest {
                command: CommandName::CreatePaperEvaluationDraft,
                resource_id: Some(stored.paper_id),
                child_id: None,
                payload: json!({
                    "evaluation_id": stored.evaluation_id,
                    "supersedes_evaluation_id": Value::Null,
                    "tolerance_policy": {
                        "schema": "hepta.paper_raid.tolerance_policy.v1",
                        "version": output.tolerance_policy_version,
                        "rules": output.tolerance_rules,
                    },
                    "reference_metrics_micros": output.reference_metrics_micros,
                    "score_components": score,
                    "hard_gates": hard_gates,
                    "evaluator_coi_attestation_hash": request.coi_attestation_hash,
                }),
            })
        }
        TypedReviewOutput::Reproduction(output) => {
            if request.score_components.is_some() || request.observable_hard_gates.is_some() {
                return Err(AppError::Invalid(
                    "reproduction confirmation cannot carry evaluation judgments".into(),
                ));
            }
            Ok(HumanSigningFrameRequest {
                command: CommandName::SubmitReproduction,
                resource_id: Some(stored.paper_id),
                child_id: Some(stored.evaluation_id),
                payload: json!({
                    "reproduction_id": stable_uuid(
                        "hepta.paper_raid.review_receipt_reproduction_id.v1",
                        &[
                            &stored.receipt_id.to_string(),
                            &stored.evaluation_id.to_string(),
                            &stored.bundle_hash,
                        ],
                    ),
                    "supersedes_reproduction_id": Value::Null,
                    "observed_metrics_micros": output.observed_metrics_micros,
                    "statistical_evidence": output.statistical_evidence,
                    "seed_set_hash": validated.receipt.seed_set_hash,
                    "environment_hash": validated.receipt.environment_hash,
                    "run_manifest_hash": validated.receipt.run_manifest_hash,
                    "coi_attestation_hash": request.coi_attestation_hash,
                }),
            })
        }
    }
}

fn frame_matches_request(
    frame: &Value,
    stored: &StoredReviewReceipt,
    request: &ReviewReceiptSigningFrameRequest,
) -> bool {
    if !exact_object_keys(
        frame,
        &[
            "schema",
            "receipt_context",
            "receipt_context_signing_bytes",
            "command",
            "resource_id",
            "child_id",
            "payload",
            "signing_bytes",
            "signing_key_id",
            "signing_public_key",
            "signing_public_key_hash",
        ],
    ) || frame.get("schema").and_then(Value::as_str) != Some(CONFIRMATION_FRAME_SCHEMA)
        || !context_matches_stored(frame, stored)
        || !context_signing_bytes_match_frame(frame)
    {
        return false;
    }
    let expected_command = if stored.kind == "evaluate" {
        "create_paper_evaluation_draft"
    } else {
        "submit_reproduction"
    };
    let payload = match frame.get("payload").and_then(Value::as_object) {
        Some(payload) => payload,
        None => return false,
    };
    let (coi_field, key_field, public_key_field, public_key_hash_field, player_field) =
        if stored.kind == "evaluate" {
            (
                "evaluator_coi_attestation_hash",
                "evaluator_signing_key_id",
                "evaluator_signing_public_key",
                "evaluator_signing_public_key_hash",
                "evaluator_player_id",
            )
        } else {
            (
                "coi_attestation_hash",
                "signing_key_id",
                "signing_public_key",
                "signing_public_key_hash",
                "reproducer_player_id",
            )
        };
    let expected_payload_keys: &[&str] = if stored.kind == "evaluate" {
        &[
            "evaluation_id",
            "submission_id",
            "supersedes_evaluation_id",
            "release_candidate_hash",
            "paper_bundle_hash",
            "tolerance_policy",
            "reference_metrics_micros",
            "score_components",
            "hard_gates",
            "evaluator_player_id",
            "evaluator_signing_key_id",
            "evaluator_signing_public_key",
            "evaluator_signing_public_key_hash",
            "evaluator_coi_attestation_hash",
            "evaluator_signed_at_unix",
        ]
    } else {
        &[
            "reproduction_id",
            "supersedes_reproduction_id",
            "release_candidate_hash",
            "paper_bundle_hash",
            "observed_metrics_micros",
            "statistical_evidence",
            "seed_set_hash",
            "environment_hash",
            "run_manifest_hash",
            "reproducer_player_id",
            "signing_key_id",
            "signing_public_key",
            "signing_public_key_hash",
            "coi_attestation_hash",
            "signed_at_unix",
        ]
    };
    if payload.keys().map(String::as_str).collect::<BTreeSet<_>>()
        != expected_payload_keys
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
        || payload.get(coi_field).and_then(Value::as_str)
            != Some(request.coi_attestation_hash.as_str())
        || payload.get(player_field) != Some(&json!(stored.player_id))
        || payload.get("submission_id").is_some()
            && payload.get("submission_id") != Some(&json!(stored.submission_id))
        || payload
            .get(if stored.kind == "evaluate" {
                "evaluation_id"
            } else {
                "reproduction_id"
            })
            .is_none()
        || payload
            .get(if stored.kind == "evaluate" {
                "evaluator_signed_at_unix"
            } else {
                "signed_at_unix"
            })
            .and_then(Value::as_i64)
            .is_none_or(|timestamp| timestamp < 0)
    {
        return false;
    }
    let Some(public_key) = frame
        .get("signing_public_key")
        .and_then(Value::as_str)
        .and_then(|value| BASE64.decode(value).ok())
        .filter(|bytes| bytes.len() == 32)
    else {
        return false;
    };
    let expected_public_key_hash = format!("sha256:{:x}", Sha256::digest(&public_key));
    if frame.get("signing_key_id") != payload.get(key_field)
        || frame.get("signing_public_key") != payload.get(public_key_field)
        || frame.get("signing_public_key_hash") != payload.get(public_key_hash_field)
        || frame.get("signing_public_key_hash").and_then(Value::as_str)
            != Some(expected_public_key_hash.as_str())
        || frame
            .get("signing_bytes")
            .and_then(Value::as_str)
            .and_then(|value| BASE64.decode(value).ok())
            .is_none_or(|bytes| bytes.is_empty() || bytes.len() > 256 * 1024)
    {
        return false;
    }
    let human_fields_match = if stored.kind == "evaluate" {
        let Some(score) = request.score_components.as_ref() else {
            return false;
        };
        let Some(observable) = request.observable_hard_gates.as_ref() else {
            return false;
        };
        let score = match serde_json::to_value(score) {
            Ok(score) => score,
            Err(_) => return false,
        };
        let Some(gates) = payload.get("hard_gates").and_then(Value::as_object) else {
            return false;
        };
        payload.get("score_components") == Some(&score)
            && gates.keys().map(String::as_str).collect::<BTreeSet<_>>()
                == [
                    "citations_and_data_authentic",
                    "failed_runs_disclosed",
                    "all_authors_consented",
                    "core_claims_have_evidence",
                    "artifact_lineage_complete",
                    "license_ethics_coi_complete",
                ]
                .into_iter()
                .collect::<BTreeSet<_>>()
            && gates.get("citations_and_data_authentic")
                == Some(&json!(observable.citations_and_data_authentic))
            && gates.get("failed_runs_disclosed") == Some(&json!(observable.failed_runs_disclosed))
            && gates.get("core_claims_have_evidence")
                == Some(&json!(observable.core_claims_have_evidence))
            && gates.get("license_ethics_coi_complete")
                == Some(&json!(observable.license_ethics_coi_complete))
            && gates
                .get("all_authors_consented")
                .and_then(Value::as_bool)
                .is_some()
            && gates
                .get("artifact_lineage_complete")
                .and_then(Value::as_bool)
                .is_some()
            && payload.get("evaluation_id") == Some(&json!(stored.evaluation_id))
            && payload.get("supersedes_evaluation_id") == Some(&Value::Null)
    } else {
        request.score_components.is_none()
            && request.observable_hard_gates.is_none()
            && payload.get("reproduction_id")
                == Some(&json!(stable_uuid(
                    "hepta.paper_raid.review_receipt_reproduction_id.v1",
                    &[
                        &stored.receipt_id.to_string(),
                        &stored.evaluation_id.to_string(),
                        &stored.bundle_hash,
                    ],
                )))
            && payload.get("supersedes_reproduction_id") == Some(&Value::Null)
    };
    human_fields_match
        && frame.get("command").and_then(Value::as_str) == Some(expected_command)
        && frame.get("resource_id").and_then(Value::as_str)
            == Some(stored.paper_id.to_string().as_str())
        && if stored.kind == "reproduce" {
            frame.get("child_id").and_then(Value::as_str)
                == Some(stored.evaluation_id.to_string().as_str())
        } else {
            frame.get("child_id") == Some(&Value::Null)
        }
}

fn frame_matches_authority(
    frame: &Value,
    descriptor: &FrozenReviewBundleV1,
    frame_request: &HumanSigningFrameRequest,
) -> bool {
    let Some(context) = frame.get("receipt_context") else {
        return false;
    };
    let Some(payload) = frame.get("payload").and_then(Value::as_object) else {
        return false;
    };
    let Some(request_payload) = frame_request.payload.as_object() else {
        return false;
    };
    context.get("bundle_hash") == Some(&json!(&descriptor.bundle_hash))
        && context.get("release_candidate_hash") == Some(&json!(&descriptor.release_candidate_hash))
        && context.get("paper_bundle_hash") == Some(&json!(&descriptor.paper_bundle_hash))
        && context.get("artifact_manifest_hash") == Some(&json!(&descriptor.artifact_manifest_hash))
        && payload.get("release_candidate_hash") == Some(&json!(&descriptor.release_candidate_hash))
        && payload.get("paper_bundle_hash") == Some(&json!(&descriptor.paper_bundle_hash))
        && request_payload
            .iter()
            .all(|(field, expected)| payload.get(field) == Some(expected))
}

async fn invalidate_receipt(
    tx: &mut Transaction<'_, Postgres>,
    stored: &StoredReviewReceipt,
    reason: &str,
    upstream_status: Option<u16>,
) -> Result<(), AppError> {
    let updated = sqlx::query(
        "UPDATE paper_raid_bff_review_execution_receipts \
         SET state='invalidated',invalidated_at=now(),updated_at=now() \
         WHERE receipt_id=$1 AND state='pending'",
    )
    .bind(stored.receipt_id)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(AppError::Conflict(
            "review receipt state changed concurrently".into(),
        ));
    }
    agent_bridge::bridge_audit(
        tx,
        Some(&stored.subject_id),
        Some(stored.binding_id),
        "review_execution_receipt_invalidated",
        "succeeded",
        json!({
            "receipt_id": stored.receipt_id,
            "task_id": stored.task_id,
            "attempt": stored.attempt,
            "assignment_id": stored.assignment_id,
            "paper_id": stored.paper_id,
            "lifecycle_disposition": "invalidated",
            "reason": reason,
            "upstream_status": upstream_status,
        }),
    )
    .await?;
    Ok(())
}

fn secured(response: Response, csrf: String) -> Response {
    with_rotated_csrf(private_no_store(response), csrf)
}

async fn authenticated_mutation(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(AuthenticatedSession, String), AppError> {
    let session = state.session(headers).await?;
    let csrf = state.sessions.consume_csrf(headers, &session).await?;
    Ok((session, csrf))
}

pub(crate) async fn signing_frame(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((paper_id, receipt_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<ReviewReceiptSigningFrameRequest>,
) -> Response {
    let (session, next_csrf) = match authenticated_mutation(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let result = async {
        let mut tx = state.pool.begin().await?;
        let stored = lock_receipt(&mut tx, &session.identity, paper_id, receipt_id).await?;
        match stored.state.as_str() {
            "consumed" => {
                return Err(AppError::Conflict(
                    "review receipt is already consumed".into(),
                ))
            }
            "invalidated" => {
                return Err(AppError::Conflict("review receipt was invalidated".into()))
            }
            "pending" => {}
            _ => return Err(AppError::Internal),
        }
        if let Some(frame) = stored.confirmation_frame.as_ref() {
            let frame_hash = canonical_json_sha256(frame).map_err(|_| AppError::Internal)?;
            if stored.confirmation_frame_hash.as_deref() != Some(frame_hash.as_str())
                || stored.confirmation_idempotency_key
                    != Some(confirmation_idempotency_key(&stored, &frame_hash))
                || !frame_matches_request(frame, &stored, &request)
            {
                return Err(AppError::Conflict(
                    "review receipt signing frame differs from the frozen confirmation".into(),
                ));
            }
            tx.commit().await?;
            return Ok(Json(frame.clone()).into_response());
        }
        let (resolved, descriptor, validated) =
            match current_authority(&state, &session.identity, &stored).await {
                Ok(authority) => authority,
                Err(error @ (AppError::Conflict(_) | AppError::Forbidden | AppError::NotFound)) => {
                    invalidate_receipt(
                        &mut tx,
                        &stored,
                        "authority_drift_before_signing_frame",
                        None,
                    )
                    .await?;
                    tx.commit().await?;
                    return Err(error);
                }
                Err(error) => return Err(error),
            };
        let frame_request =
            confirmation_frame_request(&stored, &validated, &resolved, &descriptor, &request)?;
        let frame = state
            .hepta
            .human_signing_frame(&session.identity, &frame_request)
            .await?;
        let frame = attach_confirmation_context(
            serde_json::to_value(frame).map_err(|_| AppError::Internal)?,
            confirmation_context(&stored, &validated, &descriptor),
        )?;
        if !frame_matches_request(&frame, &stored, &request)
            || !frame_matches_authority(&frame, &descriptor, &frame_request)
        {
            return Err(AppError::Upstream);
        }
        let frame_hash = canonical_json_sha256(&frame).map_err(|_| AppError::Internal)?;
        let idempotency_key = confirmation_idempotency_key(&stored, &frame_hash);
        let updated = sqlx::query(
            "UPDATE paper_raid_bff_review_execution_receipts \
             SET confirmation_frame=$1,confirmation_frame_hash=$2,\
                 confirmation_idempotency_key=$3,updated_at=now() \
             WHERE receipt_id=$4 AND state='pending' AND confirmation_frame IS NULL",
        )
        .bind(&frame)
        .bind(&frame_hash)
        .bind(idempotency_key)
        .bind(stored.receipt_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::Conflict(
                "review receipt signing frame changed concurrently".into(),
            ));
        }
        tx.commit().await?;
        Ok(Json(frame).into_response())
    }
    .await;
    secured(
        result.unwrap_or_else(IntoResponse::into_response),
        next_csrf,
    )
}

fn verify_confirmation_context_signature(
    stored: &StoredReviewReceipt,
    context_signature: &str,
) -> Result<String, AppError> {
    let frame = stored
        .confirmation_frame
        .as_ref()
        .ok_or_else(|| AppError::Conflict("request a signing frame first".into()))?;
    if !context_matches_stored(frame, stored) || !context_signing_bytes_match_frame(frame) {
        return Err(AppError::Internal);
    }
    let signing_bytes_text = frame
        .get("receipt_context_signing_bytes")
        .and_then(Value::as_str)
        .ok_or(AppError::Internal)?;
    let signing_bytes = BASE64
        .decode(signing_bytes_text)
        .map_err(|_| AppError::Internal)?;
    if signing_bytes.is_empty()
        || signing_bytes.len() > 256 * 1024
        || BASE64.encode(&signing_bytes) != signing_bytes_text
    {
        return Err(AppError::Internal);
    }
    let public_key_text = frame
        .get("signing_public_key")
        .and_then(Value::as_str)
        .ok_or(AppError::Internal)?;
    let public_key = BASE64
        .decode(public_key_text)
        .map_err(|_| AppError::Internal)?;
    if public_key.len() != 32 || BASE64.encode(&public_key) != public_key_text {
        return Err(AppError::Internal);
    }
    let public_key: [u8; 32] = public_key.try_into().map_err(|_| AppError::Internal)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key).map_err(|_| AppError::Internal)?;
    let signature_bytes = BASE64
        .decode(context_signature)
        .map_err(|_| AppError::Invalid("review receipt context signature is invalid".into()))?;
    if signature_bytes.len() != 64 || BASE64.encode(&signature_bytes) != context_signature {
        return Err(AppError::Invalid(
            "review receipt context signature is invalid".into(),
        ));
    }
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| AppError::Invalid("review receipt context signature is invalid".into()))?;
    verifying_key
        .verify(&signing_bytes, &signature)
        .map_err(|_| AppError::Forbidden)?;
    Ok(context_signature.to_string())
}

fn confirmation_command(
    stored: &StoredReviewReceipt,
    signature: &str,
    context_signature: &str,
) -> Result<(BrowserCommand, String, String), AppError> {
    let signature_bytes = BASE64
        .decode(signature)
        .map_err(|_| AppError::Invalid("human signature is invalid".into()))?;
    if signature_bytes.len() != 64 || BASE64.encode(&signature_bytes) != signature {
        return Err(AppError::Invalid("human signature is invalid".into()));
    }
    let frame = stored
        .confirmation_frame
        .as_ref()
        .ok_or_else(|| AppError::Conflict("request a signing frame first".into()))?;
    if frame.get("schema").and_then(Value::as_str) != Some(CONFIRMATION_FRAME_SCHEMA)
        || !context_matches_stored(frame, stored)
    {
        return Err(AppError::Internal);
    }
    let frame_hash = canonical_json_sha256(frame).map_err(|_| AppError::Internal)?;
    if stored.confirmation_frame_hash.as_deref() != Some(frame_hash.as_str())
        || stored.confirmation_idempotency_key
            != Some(confirmation_idempotency_key(stored, &frame_hash))
    {
        return Err(AppError::Internal);
    }
    let context_signature = verify_confirmation_context_signature(stored, context_signature)?;
    let mut payload = frame.get("payload").cloned().ok_or(AppError::Internal)?;
    let object = payload.as_object_mut().ok_or(AppError::Internal)?;
    let signature_field = if stored.kind == "evaluate" {
        "evaluator_signature"
    } else {
        "signature"
    };
    if object
        .insert(signature_field.into(), json!(signature))
        .is_some()
    {
        return Err(AppError::Internal);
    }
    let idempotency_key = stored
        .confirmation_idempotency_key
        .ok_or(AppError::Internal)?;
    object.insert("idempotency_key".into(), json!(idempotency_key));
    let command_name = if stored.kind == "evaluate" {
        CommandName::CreatePaperEvaluationDraft
    } else {
        CommandName::SubmitReproduction
    };
    let command = BrowserCommand {
        command: command_name,
        resource_id: Some(stored.paper_id),
        child_id: (stored.kind == "reproduce").then_some(stored.evaluation_id),
        session_id: None,
        idempotency_key,
        payload,
    };
    let confirmation_hash = canonical_json_sha256(&json!({
        "command": command_name,
        "resource_id": stored.paper_id,
        "child_id": command.child_id,
        "session_id": Value::Null,
        "idempotency_key": idempotency_key,
        "payload": command.payload,
        "receipt_context_signature": context_signature,
    }))
    .map_err(|_| AppError::Internal)?;
    Ok((command, confirmation_hash, context_signature))
}

fn stored_success_response(
    stored: &StoredReviewReceipt,
    replayed: bool,
) -> Result<Response, AppError> {
    let status = stored.response_status.ok_or(AppError::Internal)?;
    let body = stored.response_body.clone().ok_or(AppError::Internal)?;
    let mut response = Response::builder()
        .status(u16::try_from(status).map_err(|_| AppError::Internal)?)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .map_err(|_| AppError::Internal)?;
    if replayed {
        response.headers_mut().insert(
            "x-paper-raid-idempotent-replay",
            HeaderValue::from_static("true"),
        );
    }
    Ok(response)
}

pub(crate) async fn confirm(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((paper_id, receipt_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<ReviewReceiptConfirmationRequest>,
) -> Response {
    let (session, next_csrf) = match authenticated_mutation(&state, &headers).await {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let result = async {
        let mut tx = state.pool.begin().await?;
        let stored = lock_receipt(&mut tx, &session.identity, paper_id, receipt_id).await?;
        let (command, confirmation_hash, context_signature) = confirmation_command(
            &stored,
            &request.signature,
            &request.receipt_context_signature,
        )?;
        if stored.state == "consumed" {
            if stored.confirmation_hash.as_deref() != Some(confirmation_hash.as_str())
                || stored.confirmation_context_signature.as_deref()
                    != Some(context_signature.as_str())
            {
                return Err(AppError::Conflict(
                    "consumed review receipt cannot accept a different confirmation".into(),
                ));
            }
            let response = stored_success_response(&stored, true)?;
            tx.commit().await?;
            return Ok(response);
        }
        if stored.state == "invalidated" {
            return Err(AppError::Conflict("review receipt was invalidated".into()));
        }
        if stored.state != "pending" {
            return Err(AppError::Internal);
        }

        // This read catches ordinary expiry/version/actor drift before submission.  A mismatch is
        // not immediately terminal: the same fixed command may already have succeeded upstream
        // just before a process crash.  Sending the deterministic idempotency key/body once more
        // distinguishes an authoritative replay (2xx) from a genuinely stale command (4xx).
        match current_authority(&state, &session.identity, &stored).await {
            Ok(_) | Err(AppError::Conflict(_) | AppError::Forbidden | AppError::NotFound) => {}
            Err(error) => return Err(error),
        }
        let upstream = state
            .hepta
            .forward_command(&session.identity, &command)
            .await?;
        if (200..300).contains(&upstream.status) {
            let updated = sqlx::query(
                "UPDATE paper_raid_bff_review_execution_receipts \
                 SET state='consumed',confirmation_hash=$1,confirmation_context_signature=$2,\
                     response_status=$3,response_body=$4,\
                     consumed_at=now(),updated_at=now() \
                 WHERE receipt_id=$5 AND state='pending' AND confirmation_frame_hash=$6 \
                   AND confirmation_idempotency_key=$7",
            )
            .bind(&confirmation_hash)
            .bind(&context_signature)
            .bind(i32::from(upstream.status))
            .bind(&upstream.body)
            .bind(stored.receipt_id)
            .bind(
                stored
                    .confirmation_frame_hash
                    .as_deref()
                    .ok_or(AppError::Internal)?,
            )
            .bind(
                stored
                    .confirmation_idempotency_key
                    .ok_or(AppError::Internal)?,
            )
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(AppError::Conflict(
                    "review receipt consume raced another confirmation".into(),
                ));
            }
            tx.commit().await?;
            let mut response = Response::builder()
                .status(upstream.status)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(upstream.body))
                .map_err(|_| AppError::Internal)?;
            if upstream.replayed {
                response.headers_mut().insert(
                    "x-paper-raid-idempotent-replay",
                    HeaderValue::from_static("true"),
                );
            }
            return Ok(response);
        }
        // Only an explicit semantic rejection can retire this attempt.  A stale pre-read merely
        // requires the exact idempotent upstream replay above; transport errors, 5xx, 429, 401,
        // and malformed-response statuses remain pending because they cannot prove that the
        // authoritative command did not already commit.
        if matches!(upstream.status, 403 | 404 | 409 | 410 | 422) {
            invalidate_receipt(
                &mut tx,
                &stored,
                "authoritative_semantic_rejection",
                Some(upstream.status),
            )
            .await?;
            tx.commit().await?;
        }
        Response::builder()
            .status(upstream.status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(upstream.body))
            .map_err(|_| AppError::Internal)
    }
    .await;
    secured(
        result.unwrap_or_else(IntoResponse::into_response),
        next_csrf,
    )
}

pub(crate) async fn pending_projection(
    state: &AppState,
    identity: &AlphaIdentity,
    paper_id: Uuid,
) -> Result<Value, AppError> {
    let rows = sqlx::query(&format!(
        "{RECEIPT_SELECT} WHERE r.paper_id=$1 AND b.subject_id=$2 AND b.player_id=$3 \
         AND r.state='pending' ORDER BY r.created_at DESC LIMIT 8"
    ))
    .bind(paper_id)
    .bind(&identity.subject_id)
    .bind(identity.player_id)
    .fetch_all(&state.pool)
    .await?;
    if rows.is_empty() {
        return Ok(json!({
            "schema": "hepta.paper_raid.review_receipt_projection.v1",
            "status": "unavailable",
            "reason_code": "waiting_for_agent_execution_receipt",
            "receipt": Value::Null,
        }));
    }
    for row in rows {
        let stored = StoredReviewReceipt::from_row(&row)?;
        let authority = match current_authority(state, identity, &stored).await {
            Ok(authority) => authority,
            Err(AppError::Conflict(_) | AppError::Forbidden | AppError::NotFound) => continue,
            Err(error) => return Err(error),
        };
        let (_, _, validated) = authority;
        return Ok(json!({
            "schema": "hepta.paper_raid.review_receipt_projection.v1",
            "status": "available",
            "reason_code": Value::Null,
            "receipt": {
                "receipt_id": stored.receipt_id,
                "kind": stored.kind,
                "evaluation_id": stored.evaluation_id,
                "output": validated.output.as_value()?,
                "seals": {
                    "receipt_hash": stored.receipt_hash,
                    "bundle_hash": stored.bundle_hash,
                    "evaluator_version": validated.receipt.evaluator_version,
                    "input_root": validated.receipt.input_root,
                    "output_root": validated.receipt.output_root,
                    "metrics_hash": validated.receipt.metrics_hash,
                    "seed_set_hash": validated.receipt.seed_set_hash,
                    "environment_hash": validated.receipt.environment_hash,
                    "run_manifest_hash": validated.receipt.run_manifest_hash,
                    "logs_hash": validated.receipt.logs_hash,
                    "exit_code": validated.run_manifest.get("exit_code"),
                    "started_at_unix": validated.receipt.started_at_unix,
                    "completed_at_unix": validated.receipt.completed_at_unix,
                }
            }
        }));
    }
    Ok(json!({
        "schema": "hepta.paper_raid.review_receipt_projection.v1",
        "status": "unavailable",
        "reason_code": "receipt_stale_or_invalid",
        "receipt": Value::Null,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::{HashMap, VecDeque},
        sync::{Arc, Mutex},
        time::Duration,
    };

    use axum::{
        body::{to_bytes, Body, Bytes},
        extract::OriginalUri,
        http::{Method, Request, StatusCode},
        routing::{get, post},
        Router,
    };
    use chrono::SecondsFormat;
    use ed25519_dalek::{Signer, SigningKey};
    use hepta_paper_raid_contracts::{
        agent_bridge_request_proof_signing_bytes, frozen_challenge_material_authority_hash,
        frozen_review_authority_hash, frozen_review_input_root, paper_release_candidate_hash,
        review_execution_receipt_id, review_execution_receipt_signing_bytes, sha256_digest,
        sign_authorship_consent, AgentBridgeRequestProofV1, AssignedChallengeMaterialBundleV1,
        AuthorshipConsentSigningV2, ChallengeDatasetManifestV1, ChallengeEvaluatorManifestV1,
        ChallengeManifestObjectV1, ChallengePackContentContractV1, ChallengePackDeploymentV1,
        ChallengePackManifestV1, ChallengePackObjectV1, FrozenChallengeMaterialAuthorityV1,
        FrozenReviewAuthorityV1, FrozenReviewExecutionPolicyV1, FrozenReviewInputObjectV1,
        FrozenReviewObjectV1, PaperBundleAuthorConsentV2, PaperReleaseAuthorV2,
        PaperReleaseCandidateV2, SignedConsumerUserAssertionV2, AGENT_BRIDGE_REQUEST_PROOF_V1,
        AUTHORSHIP_CONSENT_V2, FROZEN_CHALLENGE_MATERIAL_AUTHORITY_V1, FROZEN_REVIEW_AUTHORITY_V1,
        PAPER_BUNDLE_V2, PAPER_RELEASE_CANDIDATE_V2, REVIEW_EXECUTION_RECEIPT_V1,
    };
    use sqlx::PgPool;
    use tower::ServiceExt;

    use crate::{
        app::AppState,
        auth::{CSRF_HEADER, SESSION_COOKIE},
        config::{
            AgentBridgeQuotaConfig, AlphaAuthorRole, AlphaIdentityScope, CasConfig, Config,
            ConsumerAssertionConfig, EdgeScope, IdentityMode,
        },
    };

    const REVIEW_TASK_ID_DOMAIN: &str = "hepta.paper_raid.agent_bridge.review_task_id.v1";
    const REVIEW_EVALUATION_ID_DOMAIN: &str =
        "hepta.paper_raid.agent_bridge.review_evaluation_id.v1";

    #[derive(Clone)]
    struct ReviewPgMock {
        binding: Value,
        human_player: Value,
        bundles: Arc<HashMap<Uuid, Value>>,
        rooms: Arc<HashMap<Uuid, Value>>,
        cas: Arc<HashMap<String, (String, Vec<u8>)>>,
        cas_reads: Arc<Mutex<HashMap<String, usize>>>,
        review_queue_override: Arc<Mutex<Option<Value>>>,
        command_replies: Arc<Mutex<VecDeque<ReviewCommandReply>>>,
        command_calls: Arc<Mutex<Vec<ReviewCommandCall>>>,
    }

    #[derive(Clone)]
    struct ReviewCommandReply {
        status: StatusCode,
        body: Value,
        delay_before_response: Duration,
    }

    #[derive(Debug)]
    struct ReviewCommandCall {
        body: Vec<u8>,
        idempotency_key: String,
        nonce: String,
        operation: String,
        path: String,
    }

    struct ReviewPgReproducerFixture {
        bundle: FrozenReviewBundleV1,
        evaluation_id: Uuid,
    }

    struct ReviewPgHarness {
        config: Config,
        state: AppState,
        identity: AlphaIdentity,
        agent_key: SigningKey,
        binding_id: Uuid,
        agent_id: String,
        human_key: SigningKey,
        bundles: Vec<FrozenReviewBundleV1>,
        reproducer_bundles: Vec<ReviewPgReproducerFixture>,
        challenge: ChallengeRouteFixture,
        paper_source_bytes: Vec<u8>,
        candidate_bytes: Vec<u8>,
        cas_reads: Arc<Mutex<HashMap<String, usize>>>,
        review_queue_override: Arc<Mutex<Option<Value>>>,
        command_replies: Arc<Mutex<VecDeque<ReviewCommandReply>>>,
        command_calls: Arc<Mutex<Vec<ReviewCommandCall>>>,
    }

    struct ChallengeRouteFixture {
        bundle: AssignedChallengeMaterialBundleV1,
        object_bytes: Vec<u8>,
    }

    async fn mock_agent_bindings(State(mock): State<ReviewPgMock>) -> Json<Value> {
        Json(json!([mock.binding]))
    }

    async fn mock_current_human_player(State(mock): State<ReviewPgMock>) -> Json<Value> {
        Json(mock.human_player)
    }

    async fn mock_raid_state() -> Json<Value> {
        // This fixture identity deliberately has Author and Review scopes, but the review Papers
        // are not Author raids.  The mixed inbox must therefore retain the independently assigned
        // Review tasks without attempting to read an Author room.
        Json(json!({"raids": []}))
    }

    async fn mock_review_bundle(
        State(mock): State<ReviewPgMock>,
        Path(paper_id): Path<Uuid>,
    ) -> Response {
        mock.bundles
            .get(&paper_id)
            .cloned()
            .map(|bundle| Json(bundle).into_response())
            .unwrap_or_else(|| {
                (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response()
            })
    }

    async fn mock_review_queue(State(mock): State<ReviewPgMock>) -> Json<Value> {
        if let Some(queue) = mock
            .review_queue_override
            .lock()
            .expect("review queue override lock")
            .clone()
        {
            return Json(queue);
        }
        let mut papers = mock.bundles.values().collect::<Vec<_>>();
        papers.sort_by_key(|bundle| {
            bundle
                .get("paper_project_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
        Json(json!(papers
            .into_iter()
            .map(|bundle| json!({
                "paper_project_id": bundle["paper_project_id"],
                "submission_id": bundle["submission_id"],
                "release_candidate_hash": bundle["release_candidate_hash"],
                "paper_bundle_hash": bundle["paper_bundle_hash"],
                "my_assignments": bundle["my_assignments"],
            }))
            .collect::<Vec<_>>()))
    }

    async fn mock_paper_room(
        State(mock): State<ReviewPgMock>,
        Path(paper_id): Path<Uuid>,
    ) -> Response {
        mock.rooms
            .get(&paper_id)
            .cloned()
            .map(|room| Json(room).into_response())
            .unwrap_or_else(|| {
                (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response()
            })
    }

    async fn mock_cas_object(
        State(mock): State<ReviewPgMock>,
        Path((_bucket, digest)): Path<(String, String)>,
    ) -> Response {
        let digest = format!("sha256:{digest}");
        *mock
            .cas_reads
            .lock()
            .expect("CAS read counter lock")
            .entry(digest.clone())
            .or_default() += 1;
        let Some((media_type, bytes)) = mock.cas.get(&digest) else {
            return (StatusCode::NOT_FOUND, "missing").into_response();
        };
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, media_type.as_str())
            .body(Body::from(bytes.clone()))
            .expect("mock CAS response")
    }

    async fn mock_review_command(
        mock: ReviewPgMock,
        headers: HeaderMap,
        body: Bytes,
        expected_path: String,
        expected_operation: &'static str,
    ) -> Response {
        let assertion = headers
            .get("x-hepta-user-assertion")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| BASE64.decode(value).ok())
            .and_then(|bytes| serde_json::from_slice::<SignedConsumerUserAssertionV2>(&bytes).ok());
        let Some(assertion) = assertion else {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "assertion"}))).into_response();
        };
        let body_json: Value = match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(_) => {
                return (StatusCode::BAD_REQUEST, Json(json!({"error": "body"}))).into_response()
            }
        };
        if assertion.claim.http_method != "POST"
            || assertion.claim.canonical_path != expected_path
            || assertion.claim.operation != expected_operation
            || assertion.claim.body_hash != sha256_digest(&body)
            || body_json.get("idempotency_key").and_then(Value::as_str)
                != Some(assertion.claim.idempotency_key.as_str())
        {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "authority"}))).into_response();
        }
        mock.command_calls
            .lock()
            .expect("review command call lock")
            .push(ReviewCommandCall {
                body: body.to_vec(),
                idempotency_key: assertion.claim.idempotency_key,
                nonce: assertion.claim.nonce,
                operation: assertion.claim.operation,
                path: assertion.claim.canonical_path,
            });
        let reply = mock
            .command_replies
            .lock()
            .expect("review command reply lock")
            .pop_front()
            .unwrap_or(ReviewCommandReply {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                body: json!({"error": "unplanned_review_command"}),
                delay_before_response: Duration::ZERO,
            });
        if !reply.delay_before_response.is_zero() {
            tokio::time::sleep(reply.delay_before_response).await;
        }
        (reply.status, Json(reply.body)).into_response()
    }

    async fn mock_evaluation_draft(
        State(mock): State<ReviewPgMock>,
        Path(paper_id): Path<Uuid>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        mock_review_command(
            mock,
            headers,
            body,
            format!("/v2/hepta/papers/{paper_id}/evaluation-drafts"),
            "create_paper_evaluation_draft_v1",
        )
        .await
    }

    async fn mock_reproduction(
        State(mock): State<ReviewPgMock>,
        Path((paper_id, evaluation_id)): Path<(Uuid, Uuid)>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        mock_review_command(
            mock,
            headers,
            body,
            format!("/v2/hepta/papers/{paper_id}/evaluations/{evaluation_id}/reproductions"),
            "create_paper_reproduction_v1",
        )
        .await
    }

    async fn spawn_review_pg_mock(mock: ReviewPgMock) -> url::Url {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind review PG mock");
        let address = listener.local_addr().expect("review PG mock address");
        let router = Router::new()
            .route("/v2/hepta/players/me", get(mock_current_human_player))
            .route("/v2/hepta/agent-bindings", get(mock_agent_bindings))
            .route("/v2/hepta/raid-state", get(mock_raid_state))
            .route("/v2/hepta/review-queue", get(mock_review_queue))
            .route(
                "/v2/hepta/papers/:paper_id/review-bundle",
                get(mock_review_bundle),
            )
            .route("/v2/hepta/papers/:paper_id/room", get(mock_paper_room))
            .route(
                "/v2/hepta/papers/:paper_id/evaluation-drafts",
                post(mock_evaluation_draft),
            )
            .route(
                "/v2/hepta/papers/:paper_id/evaluations/:evaluation_id/reproductions",
                post(mock_reproduction),
            )
            .route("/:bucket/objects/sha256/:digest", get(mock_cas_object))
            .with_state(mock);
        tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("serve review PG mock");
        });
        url::Url::parse(&format!("http://{address}")).expect("review PG mock URL")
    }

    fn deterministic_uuid(domain: &str, fields: &[String]) -> Uuid {
        let mut hasher = Sha256::new();
        hasher.update(domain.as_bytes());
        hasher.update([0]);
        hasher.update(fields.join("\0").as_bytes());
        let digest = hasher.finalize();
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x50;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid::from_bytes(bytes)
    }

    fn review_ids(
        bundle: &FrozenReviewBundleV1,
        reproduction_evaluation_id: Option<Uuid>,
    ) -> (Uuid, Uuid) {
        let evaluation_id = match bundle.execution.kind.as_str() {
            "evaluate" => {
                assert!(reproduction_evaluation_id.is_none());
                deterministic_uuid(
                    REVIEW_EVALUATION_ID_DOMAIN,
                    &[bundle.assignment_id.to_string(), bundle.bundle_hash.clone()],
                )
            }
            "reproduce" => reproduction_evaluation_id.expect("reproduction evaluation child"),
            _ => panic!("unsupported executable review fixture kind"),
        };
        let task_id = deterministic_uuid(
            REVIEW_TASK_ID_DOMAIN,
            &[
                bundle.assignment_id.to_string(),
                bundle.bundle_hash.clone(),
                bundle.execution.kind.clone(),
                evaluation_id.to_string(),
            ],
        );
        (task_id, evaluation_id)
    }

    fn manifest_member(path: &str, bytes: &[u8], media_type: &str) -> ChallengeManifestObjectV1 {
        let digest = sha256_digest(bytes);
        ChallengeManifestObjectV1 {
            cas_uri: format!("cas://sha256/{}", &digest[7..]),
            media_type: media_type.to_string(),
            path: path.to_string(),
            sha256: digest,
            size: u64::try_from(bytes.len()).expect("fixture object size"),
        }
    }

    fn challenge_pack_object(
        path: &str,
        role: &str,
        bytes: &[u8],
        media_type: &str,
    ) -> ChallengePackObjectV1 {
        let digest = sha256_digest(bytes);
        ChallengePackObjectV1 {
            cas_uri: format!("cas://sha256/{}", &digest[7..]),
            media_type: media_type.to_string(),
            path: path.to_string(),
            role: role.to_string(),
            sha256: digest,
            size: u64::try_from(bytes.len()).expect("challenge fixture object size"),
        }
    }

    struct ChallengeFixtureInput {
        room: Value,
        paper_id: Uuid,
        work_item_id: Uuid,
        object_bytes: Vec<u8>,
        cas: HashMap<String, (String, Vec<u8>)>,
    }

    fn challenge_fixture(binding_id: Uuid, player_id: Uuid) -> ChallengeFixtureInput {
        // Keep at least one hexadecimal letter in this authority ID so the uppercase-UUID
        // route mutant below is deterministic rather than depending on random UUID text.
        let paper_id =
            Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").expect("fixture paper UUID");
        let challenge_id = Uuid::new_v4();
        let work_item_id = Uuid::new_v4();
        let pack_id = "pg-router-challenge-fixture-v1";
        let ruleset_version = "paper-raid-evidence-audit-v1";

        let brief_bytes = b"# Evidence audit\n\nResolve every frozen claim.\n".to_vec();
        let dataset_bytes = br#"{"claims":[{"id":"claim-1","value":true}]}"#.to_vec();
        let baseline_bytes = b"print('baseline fixture')\n".to_vec();
        let evaluator_bytes = b"print('evaluator fixture')\n".to_vec();
        let license_bytes = b"CC0-1.0\n".to_vec();
        let explanation_bytes = b"# Result explanation\n".to_vec();

        let evaluator_manifest = ChallengeEvaluatorManifestV1 {
            entrypoint: "evaluator.py".to_string(),
            frozen: true,
            objects: vec![
                manifest_member(
                    "baseline.py",
                    &baseline_bytes,
                    "text/x-python; charset=utf-8",
                ),
                manifest_member(
                    "evaluator.py",
                    &evaluator_bytes,
                    "text/x-python; charset=utf-8",
                ),
            ],
            pack_id: pack_id.to_string(),
            runtime: "python3-stdlib".to_string(),
            schema: "hepta.challenge_pack.evaluator_manifest.v1".to_string(),
        };
        let dataset_manifest = ChallengeDatasetManifestV1 {
            objects: vec![manifest_member(
                "dataset/claims.json",
                &dataset_bytes,
                "application/json",
            )],
            pack_id: pack_id.to_string(),
            schema: "hepta.challenge_pack.dataset_manifest.v1".to_string(),
        };
        let evaluator_manifest_bytes =
            canonical_json_bytes(&evaluator_manifest).expect("challenge evaluator manifest bytes");
        let dataset_manifest_bytes =
            canonical_json_bytes(&dataset_manifest).expect("challenge dataset manifest bytes");
        let evaluator_manifest_hash = sha256_digest(&evaluator_manifest_bytes);
        let dataset_manifest_hash = sha256_digest(&dataset_manifest_bytes);

        let pack = ChallengePackManifestV1 {
            content_contract: ChallengePackContentContractV1 {
                difficulty: "introductory".to_string(),
                duration_seconds: 900,
                modifiers: vec!["frozen-evaluator".to_string()],
                objective: "Audit the frozen claims.".to_string(),
                risks: vec!["citation-mismatch".to_string()],
                victory: "Every claim is resolved.".to_string(),
            },
            dataset_manifest_sha256: dataset_manifest_hash.clone(),
            deployment: ChallengePackDeploymentV1 {
                blocker: "fixture-ready".to_string(),
                cas_seeded: true,
                open_status_allowed: true,
            },
            evaluator_manifest_sha256: evaluator_manifest_hash.clone(),
            objects: vec![
                challenge_pack_object(
                    "LICENSE.txt",
                    "license",
                    &license_bytes,
                    "text/plain; charset=utf-8",
                ),
                challenge_pack_object(
                    "baseline.py",
                    "baseline_code",
                    &baseline_bytes,
                    "text/x-python; charset=utf-8",
                ),
                challenge_pack_object(
                    "brief.md",
                    "playable_brief",
                    &brief_bytes,
                    "text/markdown; charset=utf-8",
                ),
                challenge_pack_object(
                    "dataset.manifest.json",
                    "dataset_manifest",
                    &dataset_manifest_bytes,
                    "application/json",
                ),
                challenge_pack_object(
                    "dataset/claims.json",
                    "dataset",
                    &dataset_bytes,
                    "application/json",
                ),
                challenge_pack_object(
                    "evaluator.manifest.json",
                    "evaluator_manifest",
                    &evaluator_manifest_bytes,
                    "application/json",
                ),
                challenge_pack_object(
                    "evaluator.py",
                    "frozen_evaluator",
                    &evaluator_bytes,
                    "text/x-python; charset=utf-8",
                ),
                challenge_pack_object(
                    "result-explanation.md",
                    "result_explanation",
                    &explanation_bytes,
                    "text/markdown; charset=utf-8",
                ),
            ],
            pack_id: pack_id.to_string(),
            ruleset_version: ruleset_version.to_string(),
            schema: "hepta.challenge_pack.v1".to_string(),
            seed: 1_701,
            template: "evidence-audit".to_string(),
        };
        let pack_bytes = canonical_json_bytes(&pack).expect("challenge pack bytes");
        let pack_hash = sha256_digest(&pack_bytes);
        let challenge_snapshot_hash = sha256_digest(b"pg-router-challenge-catalog-snapshot-v1");
        let ruleset_hash = sha256_digest(b"pg-router-challenge-ruleset-v1");
        let mut authority = FrozenChallengeMaterialAuthorityV1 {
            schema: FROZEN_CHALLENGE_MATERIAL_AUTHORITY_V1.to_string(),
            authority_hash: String::new(),
            activation_id: Uuid::new_v4(),
            activation_request_sha256: sha256_digest(b"pg-router-challenge-activation-v1"),
            challenge_id,
            challenge_snapshot_hash: challenge_snapshot_hash.clone(),
            template: pack.template.clone(),
            pack_id: pack.pack_id.clone(),
            pack_manifest_hash: pack_hash.clone(),
            ruleset_version: ruleset_version.to_string(),
            ruleset_hash: ruleset_hash.clone(),
            dataset_manifest_hash: dataset_manifest_hash.clone(),
            evaluator_manifest_hash: evaluator_manifest_hash.clone(),
        };
        authority.authority_hash =
            frozen_challenge_material_authority_hash(&authority).expect("challenge authority hash");
        let snapshot = json!({
            "schema": "hepta.paper_raid.challenge_ruleset_snapshot.v1",
            "challenge_snapshot_hash": challenge_snapshot_hash,
            "ruleset_version": ruleset_version,
            "ruleset_hash": ruleset_hash,
            "enforcement": "authoritative_v1",
            "ruleset": {},
            "material_authority": authority,
        });
        let snapshot_hash =
            canonical_json_sha256(&snapshot).expect("challenge ruleset snapshot hash");
        let room = json!({
            "paper": {
                "paper_project_id": paper_id,
                "challenge_id": challenge_id,
                "phase": "researching",
                "outcome": "in_progress",
                "challenge_ruleset_snapshot_hash": snapshot_hash,
                "challenge_ruleset_snapshot": snapshot,
            },
            "team": {},
            "author_raid_progress": {},
            "team_member_acceptances": [],
            "work_items": [{
                "work_item_id": work_item_id,
                "paper_project_id": paper_id,
                "assigned_binding_id": binding_id,
                "assigned_player_id": player_id,
                "status": "in_progress",
                "version": 3,
            }],
            "paper_revisions": [],
            "authorship_consents": [],
            "joint_submission": null,
            "member_research_sessions": [],
            "artifact_manifests": [],
            "revision_artifact_bindings": [],
            "evidence_cards": [],
            "citations": [],
            "experiment_plans": [],
            "runs": [],
            "figures": [],
            "claims": [],
            "section_heads": [],
            "leases": [],
            "proposals": [],
            "decisions": [],
            "section_revisions": [],
            "section_reviews": [],
            "section_merges": [],
            "last_event_cursor": 0,
        });
        let cas = HashMap::from([
            (pack_hash, ("application/json".to_string(), pack_bytes)),
            (
                evaluator_manifest_hash,
                ("application/json".to_string(), evaluator_manifest_bytes),
            ),
            (
                dataset_manifest_hash,
                ("application/json".to_string(), dataset_manifest_bytes),
            ),
            (
                sha256_digest(&brief_bytes),
                (
                    "text/markdown; charset=utf-8".to_string(),
                    brief_bytes.clone(),
                ),
            ),
            (
                sha256_digest(&dataset_bytes),
                ("application/json".to_string(), dataset_bytes),
            ),
            (
                sha256_digest(&baseline_bytes),
                ("text/x-python; charset=utf-8".to_string(), baseline_bytes),
            ),
            (
                sha256_digest(&evaluator_bytes),
                ("text/x-python; charset=utf-8".to_string(), evaluator_bytes),
            ),
        ]);
        ChallengeFixtureInput {
            room,
            paper_id,
            work_item_id,
            object_bytes: brief_bytes,
            cas,
        }
    }

    fn authority_object(
        object_key: &str,
        path: &str,
        role: &str,
        bytes: &[u8],
        media_type: &str,
    ) -> FrozenReviewObjectV1 {
        FrozenReviewObjectV1 {
            object_key: object_key.to_string(),
            logical_path: path.to_string(),
            role: role.to_string(),
            digest: sha256_digest(bytes),
            size_bytes: u64::try_from(bytes.len()).expect("fixture object size"),
            media_type: media_type.to_string(),
            download_path: "/api/agent-bridge/review-objects".to_string(),
        }
    }

    struct FrozenAuthorityFixture<'a> {
        paper_id: Uuid,
        assignment_id: Uuid,
        submission_id: Uuid,
        slot: &'a str,
        expires_at: &'a str,
        evaluator_manifest_hash: &'a str,
        dataset_manifest_hash: &'a str,
        evaluator_bytes: &'a [u8],
        dataset_bytes: &'a [u8],
        candidate_bytes: &'a [u8],
        paper_source_bytes: &'a [u8],
        bibliography_bytes: &'a [u8],
        claim_evidence_graph_bytes: &'a [u8],
    }

    fn review_paper_bundle(
        paper_id: Uuid,
        artifact_manifest_hash: &str,
        paper_source_bytes: &[u8],
        bibliography_bytes: &[u8],
        claim_evidence_graph_bytes: &[u8],
    ) -> PaperBundleV2 {
        let authors = (0..3)
            .map(|index| PaperReleaseAuthorV2 {
                author_order: u32::try_from(index + 1).expect("fixture author order"),
                participant_slot: u32::try_from(index + 1).expect("fixture author slot"),
                player_id: deterministic_uuid(
                    "hepta.paper_raid.pg_fixture.author.v1",
                    &[paper_id.to_string(), index.to_string()],
                ),
                display_name: format!("PostgreSQL Author {}", index + 1),
                credit_roles: vec![
                    ["conceptualization", "data_curation", "software"][index].to_string()
                ],
            })
            .collect::<Vec<_>>();
        let candidate = PaperReleaseCandidateV2 {
            schema: PAPER_RELEASE_CANDIDATE_V2.to_string(),
            paper_project_id: paper_id,
            revision_id: deterministic_uuid(
                "hepta.paper_raid.pg_fixture.revision.v1",
                &[paper_id.to_string()],
            ),
            team_id: deterministic_uuid(
                "hepta.paper_raid.pg_fixture.team.v1",
                &[paper_id.to_string()],
            ),
            challenge_id: deterministic_uuid(
                "hepta.paper_raid.pg_fixture.challenge.v1",
                &[paper_id.to_string()],
            ),
            ruleset_hash: sha256_digest(b"pg-review-ruleset-v1"),
            challenge_snapshot_hash: sha256_digest(b"pg-review-challenge-snapshot-v1"),
            roster_version: 1,
            title: "Frozen PostgreSQL Review Fixture".to_string(),
            abstract_text: "A signed nested PaperBundle for the real PostgreSQL gate.".to_string(),
            target_format: "paper-raid-v2".to_string(),
            source_manifest_hash: sha256_digest(paper_source_bytes),
            artifact_manifest_hash: artifact_manifest_hash.to_string(),
            bibliography_hash: sha256_digest(bibliography_bytes),
            claim_evidence_graph_hash: sha256_digest(claim_evidence_graph_bytes),
            section_materialization_root: None,
            collaboration_compact_hash: sha256_digest(b"pg-review-collaboration-v1"),
            research_protocol_snapshot_hash: sha256_digest(b"pg-review-protocol-v1"),
            ethics_disclosure_hash: sha256_digest(b"pg-review-ethics-v1"),
            coi_disclosure_hash: sha256_digest(b"pg-review-coi-v1"),
            contribution_ledger_hash: sha256_digest(b"pg-review-contributions-v1"),
            ai_disclosure_hash: sha256_digest(b"pg-review-ai-disclosure-v1"),
            license: "CC-BY-4.0".to_string(),
            authors,
        };
        let release_candidate_hash =
            paper_release_candidate_hash(&candidate).expect("fixture release candidate hash");
        let author_consents = candidate
            .authors
            .iter()
            .enumerate()
            .map(|(index, author)| {
                let signing_key = SigningKey::from_bytes(
                    &[u8::try_from(index + 41).expect("fixture author key seed"); 32],
                );
                let signing_public_key = BASE64.encode(signing_key.verifying_key().as_bytes());
                let signing_public_key_hash = sha256_digest(signing_key.verifying_key().as_bytes());
                let consent = AuthorshipConsentSigningV2 {
                    schema: AUTHORSHIP_CONSENT_V2.to_string(),
                    consent_id: deterministic_uuid(
                        "hepta.paper_raid.pg_fixture.consent.v1",
                        &[paper_id.to_string(), index.to_string()],
                    ),
                    paper_project_id: paper_id,
                    revision_id: candidate.revision_id,
                    player_id: author.player_id,
                    signing_key_id: format!("pg-author-{}-key", index + 1),
                    signing_public_key: signing_public_key.clone(),
                    signing_public_key_hash: signing_public_key_hash.clone(),
                    release_candidate_hash: release_candidate_hash.clone(),
                    signed_at_unix: 1_700_000_000
                        + i64::try_from(index).expect("fixture consent time"),
                };
                PaperBundleAuthorConsentV2 {
                    author_order: author.author_order,
                    participant_slot: author.participant_slot,
                    player_id: author.player_id,
                    consent_id: consent.consent_id,
                    signing_key_id: consent.signing_key_id.clone(),
                    signing_public_key,
                    signing_public_key_hash,
                    signed_at_unix: consent.signed_at_unix,
                    signature: sign_authorship_consent(&consent, &signing_key)
                        .expect("fixture authorship consent signature"),
                }
            })
            .collect::<Vec<_>>();
        let mut bundle = PaperBundleV2 {
            schema: PAPER_BUNDLE_V2.to_string(),
            release_candidate: candidate,
            release_candidate_hash,
            author_consents,
            paper_bundle_hash: String::new(),
        };
        bundle.paper_bundle_hash = paper_bundle_hash(&bundle).expect("fixture PaperBundle hash");
        bundle
    }

    fn frozen_authority(
        fixture: FrozenAuthorityFixture<'_>,
    ) -> (FrozenReviewAuthorityV1, PaperBundleV2) {
        let kind = match fixture.slot {
            "evaluator" => "evaluate",
            "reproducer" => "reproduce",
            _ => panic!("unsupported executable review fixture slot"),
        };
        let artifact_manifest_hash = canonical_json_sha256(&[
            ("paper_source", sha256_digest(fixture.paper_source_bytes)),
            ("bibliography", sha256_digest(fixture.bibliography_bytes)),
            (
                "claim_evidence_graph",
                sha256_digest(fixture.claim_evidence_graph_bytes),
            ),
            ("frozen_evaluator", sha256_digest(fixture.evaluator_bytes)),
            ("dataset", sha256_digest(fixture.dataset_bytes)),
            ("candidate", sha256_digest(fixture.candidate_bytes)),
        ])
        .expect("fixture ArtifactManifest hash");
        let paper_bundle = review_paper_bundle(
            fixture.paper_id,
            &artifact_manifest_hash,
            fixture.paper_source_bytes,
            fixture.bibliography_bytes,
            fixture.claim_evidence_graph_bytes,
        );
        let mut authority = FrozenReviewAuthorityV1 {
            schema: FROZEN_REVIEW_AUTHORITY_V1.to_string(),
            authority_hash: String::new(),
            assignment_id: fixture.assignment_id,
            paper_project_id: fixture.paper_id,
            submission_id: fixture.submission_id,
            review_round: 1,
            slot: fixture.slot.to_string(),
            assignment_version: 1,
            expires_at: fixture.expires_at.to_string(),
            release_candidate_hash: paper_bundle.release_candidate_hash.clone(),
            paper_bundle_hash: paper_bundle.paper_bundle_hash.clone(),
            artifact_manifest_hash,
            evaluator_manifest_hash: fixture.evaluator_manifest_hash.to_string(),
            dataset_manifest_hash: fixture.dataset_manifest_hash.to_string(),
            artifact_objects: vec![
                authority_object(
                    "object-0000",
                    "paper/paper.md",
                    "paper_source",
                    fixture.paper_source_bytes,
                    "text/markdown; charset=utf-8",
                ),
                authority_object(
                    "object-0001",
                    "paper/references.bib",
                    "bibliography",
                    fixture.bibliography_bytes,
                    "application/x-bibtex",
                ),
                authority_object(
                    "object-0002",
                    "paper/claim-evidence.json",
                    "claim_evidence_graph",
                    fixture.claim_evidence_graph_bytes,
                    "application/json",
                ),
                authority_object(
                    "object-0003",
                    "evaluator.py",
                    "frozen_evaluator",
                    fixture.evaluator_bytes,
                    "text/x-python; charset=utf-8",
                ),
                authority_object(
                    "object-0004",
                    "dataset/claims.json",
                    "dataset",
                    fixture.dataset_bytes,
                    "application/json",
                ),
                authority_object(
                    "object-0005",
                    "candidate.json",
                    "candidate",
                    fixture.candidate_bytes,
                    "application/json",
                ),
            ],
            execution_policy: FrozenReviewExecutionPolicyV1 {
                schema: "hepta.paper_raid.review_execution_policy.v1".to_string(),
                kind: kind.to_string(),
                adapter: "python3-stdlib-v1".to_string(),
                timeout_ms: 30_000,
                seed: 7,
            },
        };
        authority.authority_hash =
            frozen_review_authority_hash(&authority).expect("frozen authority hash");
        (authority, paper_bundle)
    }

    fn hepta_review_bundle(
        authority: &FrozenReviewAuthorityV1,
        paper_bundle: &PaperBundleV2,
        identity: &AlphaIdentity,
        evaluation_id: Option<Uuid>,
    ) -> Value {
        let mut bundle = json!({
            "paper_project_id": authority.paper_project_id,
            "submission_id": authority.submission_id,
            "status": "submission_ready",
            "release_candidate_hash": authority.release_candidate_hash,
            "paper_bundle_hash": authority.paper_bundle_hash,
            "paper_bundle": paper_bundle,
            "my_assignments": [{
                "assignment_id": authority.assignment_id,
                "paper_project_id": authority.paper_project_id,
                "submission_id": authority.submission_id,
                "player_id": identity.player_id,
                "slot": authority.slot,
                "review_round": authority.review_round,
                "version": authority.assignment_version,
                "expires_at": authority.expires_at,
                "status": "claimed",
            }],
            "frozen_review_authority": authority,
        });
        if let Some(evaluation_id) = evaluation_id {
            bundle
                .as_object_mut()
                .expect("review fixture bundle object")
                .insert(
                    "evaluation".to_string(),
                    json!({
                        "evaluation_id": evaluation_id,
                        "paper_project_id": authority.paper_project_id,
                        "submission_id": authority.submission_id,
                        "release_candidate_hash": authority.release_candidate_hash,
                        "paper_bundle_hash": authority.paper_bundle_hash,
                        "tolerance_policy_hash": format!("sha256:{}", "4".repeat(64)),
                        "version": authority.review_round,
                    }),
                );
        }
        bundle
    }

    struct BridgeBindingFixture<'a> {
        binding_id: Uuid,
        agent_id: &'a str,
        agent_key_id: &'a str,
        public_key: &'a str,
        capability_hash: &'a str,
        capability: &'a Value,
    }

    async fn seed_bridge_binding(
        pool: &PgPool,
        identity: &AlphaIdentity,
        fixture: BridgeBindingFixture<'_>,
    ) {
        let grant_id = Uuid::new_v4();
        let code_hash = Sha256::digest(grant_id.as_bytes()).to_vec();
        let pinned_hash = Sha256::digest(fixture.binding_id.as_bytes()).to_vec();
        sqlx::query(
            "INSERT INTO paper_raid_bff_agent_pairing_grants (
                grant_id,subject_id,player_id,code_hash,state,pinned_request_hash,
                pinned_binding_id,created_at,expires_at,pinned_at,consumed_at,
                pair_response_status,pair_response_body,updated_at
             ) VALUES ($1,$2,$3,$4,'consumed',$5,$6,now(),now()+interval '5 minutes',
                now(),now(),200,$7,now())",
        )
        .bind(grant_id)
        .bind(&identity.subject_id)
        .bind(identity.player_id)
        .bind(code_hash)
        .bind(pinned_hash)
        .bind(fixture.binding_id)
        .bind(b"{}".as_slice())
        .execute(pool)
        .await
        .expect("seed consumed pairing grant");
        let record = json!({
            "binding_id": fixture.binding_id,
            "player_id": identity.player_id,
            "agent_id": fixture.agent_id,
            "agent_key_id": fixture.agent_key_id,
            "agent_public_key": fixture.public_key,
            "capability_disclosure_hash": fixture.capability_hash,
            "capability_disclosure": fixture.capability,
            "status": "active",
        });
        sqlx::query(
            "INSERT INTO paper_raid_bff_agent_bridge_bindings (
                binding_id,grant_id,last_pairing_grant_id,subject_id,player_id,agent_id,
                agent_key_id,capability_disclosure_hash,capability_disclosure,binding_record,
                paired_at,last_verified_at
             ) VALUES ($1,$2,$2,$3,$4,$5,$6,$7,$8::jsonb,$9::jsonb,now(),now())",
        )
        .bind(fixture.binding_id)
        .bind(grant_id)
        .bind(&identity.subject_id)
        .bind(identity.player_id)
        .bind(fixture.agent_id)
        .bind(fixture.agent_key_id)
        .bind(fixture.capability_hash)
        .bind(fixture.capability)
        .bind(record)
        .execute(pool)
        .await
        .expect("seed active Bridge binding");
    }

    async fn review_pg_harness(
        database_url: String,
        include_author_scope: bool,
    ) -> ReviewPgHarness {
        let agent_key = SigningKey::from_bytes(&[73_u8; 32]);
        let human_key = SigningKey::from_bytes(&[91_u8; 32]);
        let public_key = BASE64.encode(agent_key.verifying_key().as_bytes());
        let agent_key_id = sha256_digest(agent_key.verifying_key().as_bytes());
        let human_public_key = BASE64.encode(human_key.verifying_key().as_bytes());
        let human_public_key_hash = sha256_digest(human_key.verifying_key().as_bytes());
        let human_key_id = format!(
            "human-ed25519:{}",
            human_public_key_hash
                .strip_prefix("sha256:")
                .expect("human key digest prefix")
        );
        let binding_id = Uuid::new_v4();
        let agent_id = format!("agent.pg-review-{}", Uuid::new_v4().simple());
        let subject_id = format!("review-pg-{}", Uuid::new_v4());
        let mut scopes = vec![
            AlphaIdentityScope::Evaluator,
            AlphaIdentityScope::Reproducer,
        ];
        let author_roles = if include_author_scope {
            scopes.insert(0, AlphaIdentityScope::Author);
            vec![AlphaAuthorRole::Captain]
        } else {
            vec![]
        };
        let identity = AlphaIdentity::from_access_directory(
            subject_id,
            "PostgreSQL Review Actor".to_string(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            scopes,
            author_roles,
        )
        .expect("review PG identity");
        let capability = json!({
            "schema": "hepta.paper_raid.agent_capability_disclosure.v1",
            "assurance": "self_declared_unverified",
            "capabilities": ["experiment_execution", "reproduction"],
            "resource_classes": ["code_execution", "cpu", "sandbox"],
            "max_parallel_tasks": 2,
        });
        let capability_hash = canonical_json_sha256(&capability).expect("capability digest");

        let evaluator_bytes = b"print('review fixture')\n".to_vec();
        let dataset_bytes = br#"{"claims":[1]}"#.to_vec();
        let candidate_bytes = br#"{"candidate":true}"#.to_vec();
        let paper_source_bytes = b"# Frozen Paper\n\nHuman-readable review source.\n".to_vec();
        let bibliography_bytes = b"@misc{fixture,title={Frozen Fixture}}\n".to_vec();
        let claim_evidence_graph_bytes =
            br#"{"claims":[{"claim_id":"claim-1","evidence_ids":["evidence-1"]}]}"#.to_vec();
        let evaluator_manifest = ChallengeEvaluatorManifestV1 {
            entrypoint: "evaluator.py".to_string(),
            frozen: true,
            objects: vec![manifest_member(
                "evaluator.py",
                &evaluator_bytes,
                "text/x-python; charset=utf-8",
            )],
            pack_id: "pg-review-fixture-v1".to_string(),
            runtime: "python3-stdlib".to_string(),
            schema: "hepta.challenge_pack.evaluator_manifest.v1".to_string(),
        };
        let dataset_manifest = ChallengeDatasetManifestV1 {
            objects: vec![manifest_member(
                "dataset/claims.json",
                &dataset_bytes,
                "application/json",
            )],
            pack_id: evaluator_manifest.pack_id.clone(),
            schema: "hepta.challenge_pack.dataset_manifest.v1".to_string(),
        };
        let evaluator_manifest_bytes =
            canonical_json_bytes(&evaluator_manifest).expect("evaluator manifest bytes");
        let dataset_manifest_bytes =
            canonical_json_bytes(&dataset_manifest).expect("dataset manifest bytes");
        let evaluator_manifest_hash = sha256_digest(&evaluator_manifest_bytes);
        let dataset_manifest_hash = sha256_digest(&dataset_manifest_bytes);
        let expires_at =
            (Utc::now() + chrono::Duration::hours(1)).to_rfc3339_opts(SecondsFormat::Secs, true);

        let mut hepta_bundles = HashMap::new();
        for _ in 0..5 {
            let paper_id = Uuid::new_v4();
            let assignment_id = Uuid::new_v4();
            let submission_id = Uuid::new_v4();
            let (authority, paper_bundle) = frozen_authority(FrozenAuthorityFixture {
                paper_id,
                assignment_id,
                submission_id,
                slot: "evaluator",
                expires_at: &expires_at,
                evaluator_manifest_hash: &evaluator_manifest_hash,
                dataset_manifest_hash: &dataset_manifest_hash,
                evaluator_bytes: &evaluator_bytes,
                dataset_bytes: &dataset_bytes,
                candidate_bytes: &candidate_bytes,
                paper_source_bytes: &paper_source_bytes,
                bibliography_bytes: &bibliography_bytes,
                claim_evidence_graph_bytes: &claim_evidence_graph_bytes,
            });
            hepta_bundles.insert(
                paper_id,
                hepta_review_bundle(&authority, &paper_bundle, &identity, None),
            );
        }
        let reproducer_paper_id = Uuid::new_v4();
        let (reproducer_authority, reproducer_paper_bundle) =
            frozen_authority(FrozenAuthorityFixture {
                paper_id: reproducer_paper_id,
                assignment_id: Uuid::new_v4(),
                submission_id: Uuid::new_v4(),
                slot: "reproducer",
                expires_at: &expires_at,
                evaluator_manifest_hash: &evaluator_manifest_hash,
                dataset_manifest_hash: &dataset_manifest_hash,
                evaluator_bytes: &evaluator_bytes,
                dataset_bytes: &dataset_bytes,
                candidate_bytes: &candidate_bytes,
                paper_source_bytes: &paper_source_bytes,
                bibliography_bytes: &bibliography_bytes,
                claim_evidence_graph_bytes: &claim_evidence_graph_bytes,
            });
        let reproducer_evaluation_id = Uuid::new_v4();
        hepta_bundles.insert(
            reproducer_paper_id,
            hepta_review_bundle(
                &reproducer_authority,
                &reproducer_paper_bundle,
                &identity,
                Some(reproducer_evaluation_id),
            ),
        );
        let binding = json!({
            "binding_id": binding_id,
            "player_id": identity.player_id,
            "agent_id": agent_id,
            "agent_key_id": agent_key_id,
            "agent_public_key": public_key,
            "capability_disclosure_hash": capability_hash,
            "capability_disclosure": capability,
            "status": "active",
        });
        let challenge_input = challenge_fixture(binding_id, identity.player_id);
        let mut cas = HashMap::from([
            (
                evaluator_manifest_hash.clone(),
                ("application/json".to_string(), evaluator_manifest_bytes),
            ),
            (
                dataset_manifest_hash.clone(),
                ("application/json".to_string(), dataset_manifest_bytes),
            ),
        ]);
        for (bytes, media_type) in [
            (&evaluator_bytes, "text/x-python; charset=utf-8"),
            (&dataset_bytes, "application/json"),
            (&candidate_bytes, "application/json"),
            (&paper_source_bytes, "text/markdown; charset=utf-8"),
            (&bibliography_bytes, "application/x-bibtex"),
            (&claim_evidence_graph_bytes, "application/json"),
        ] {
            cas.insert(
                sha256_digest(bytes),
                (media_type.to_string(), bytes.to_vec()),
            );
        }
        cas.extend(challenge_input.cas.clone());
        let rooms = HashMap::from([(challenge_input.paper_id, challenge_input.room.clone())]);
        let cas_reads = Arc::new(Mutex::new(HashMap::new()));
        let review_queue_override = Arc::new(Mutex::new(None));
        let command_replies = Arc::new(Mutex::new(VecDeque::new()));
        let command_calls = Arc::new(Mutex::new(Vec::new()));
        let human_player = json!({
            "player_id": identity.player_id,
            "subject_id": identity.subject_id,
            "signing_key_id": human_key_id,
            "signing_public_key": human_public_key,
            "signing_public_key_hash": human_public_key_hash,
        });
        let mock_base = spawn_review_pg_mock(ReviewPgMock {
            binding,
            human_player,
            bundles: Arc::new(hepta_bundles.clone()),
            rooms: Arc::new(rooms),
            cas: Arc::new(cas),
            cas_reads: cas_reads.clone(),
            review_queue_override: review_queue_override.clone(),
            command_replies: command_replies.clone(),
            command_calls: command_calls.clone(),
        })
        .await;
        let config = Config {
            bind: "127.0.0.1:0".parse().expect("test bind"),
            edge_scope: EdgeScope::LoopbackProcess,
            public_origin: url::Url::parse("http://127.0.0.1:3000").expect("public origin"),
            database_url,
            session_key: [29_u8; 32],
            session_ttl: Duration::from_secs(600),
            identity_mode: IdentityMode::FixedAlpha,
            invite_alpha: None,
            agent_bridge_quota: AgentBridgeQuotaConfig {
                window: Duration::from_secs(60),
                pair_global_limit: 10_000,
                pair_bucket_limit: 10_000,
                request_global_limit: 10_000,
                request_bucket_limit: 10_000,
                request_binding_limit: 10_000,
            },
            identities: vec![identity.clone()].into(),
            hepta_base: mock_base.clone(),
            nakama_base: mock_base.clone(),
            nakama_http_key: "review-pg-test-key".to_string(),
            assertions: ConsumerAssertionConfig {
                issuer: "review-pg-consumer".to_string(),
                audience: "review-pg-hepta".to_string(),
                key_id: "review-pg-consumer-key".to_string(),
                signing_key: Arc::new(SigningKey::from_bytes(&[19_u8; 32])),
                ttl: Duration::from_secs(30),
            },
            cas: CasConfig {
                endpoint: mock_base,
                bucket: "review-pg".to_string(),
                region: "test-1".to_string(),
                access_key_id: "review-pg-access".to_string(),
                secret_access_key: "review-pg-secret".to_string(),
                max_object_bytes: 1024 * 1024,
                ready_digest: evaluator_manifest_hash,
                ready_media_type: "application/json".to_string(),
            },
        };
        let state = AppState::connect(config.clone())
            .await
            .expect("connect review PG app state");
        seed_bridge_binding(
            &state.pool,
            &identity,
            BridgeBindingFixture {
                binding_id,
                agent_id: &agent_id,
                agent_key_id: &agent_key_id,
                public_key: &public_key,
                capability_hash: &capability_hash,
                capability: &capability,
            },
        )
        .await;
        let challenge_bundle =
            crate::challenge_materials::resolve_assigned_challenge_material_bundle(
                &state,
                &challenge_input.room,
                binding_id,
                identity.player_id,
                challenge_input.work_item_id,
            )
            .await
            .expect("resolve challenge route fixture bundle");
        let mut bundles = Vec::new();
        let mut reproducer_bundles = Vec::new();
        for hepta_bundle in hepta_bundles.values() {
            let resolved = agent_bridge::resolve_frozen_review_bundle(
                &state,
                hepta_bundle,
                identity.player_id,
            )
            .await
            .expect("resolve review fixture bundle");
            let descriptor: FrozenReviewBundleV1 = serde_json::from_value(
                resolved
                    .get("resolved_frozen_review_bundle")
                    .cloned()
                    .expect("resolved descriptor"),
            )
            .expect("typed resolved descriptor");
            if descriptor.execution.kind == "reproduce" {
                let evaluation_id = resolved
                    .get("evaluation")
                    .and_then(|value| value.get("evaluation_id"))
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .expect("reproducer fixture evaluation id");
                reproducer_bundles.push(ReviewPgReproducerFixture {
                    bundle: descriptor,
                    evaluation_id,
                });
            } else {
                bundles.push(descriptor);
            }
        }
        bundles.sort_by_key(|bundle: &FrozenReviewBundleV1| bundle.paper_project_id);
        reproducer_bundles.sort_by_key(|fixture| fixture.bundle.paper_project_id);
        ReviewPgHarness {
            config,
            state,
            identity,
            agent_key,
            binding_id,
            agent_id,
            human_key,
            bundles,
            reproducer_bundles,
            challenge: ChallengeRouteFixture {
                bundle: challenge_bundle,
                object_bytes: challenge_input.object_bytes,
            },
            paper_source_bytes,
            candidate_bytes,
            cas_reads,
            review_queue_override,
            command_replies,
            command_calls,
        }
    }

    fn review_request(
        bundle: &FrozenReviewBundleV1,
        binding_id: Uuid,
        agent_id: &str,
        agent_key: &SigningKey,
        attempt: u64,
        metric: i64,
    ) -> Value {
        signed_review_request(
            bundle, binding_id, agent_id, agent_key, None, attempt, metric,
        )
    }

    fn reproduction_review_request(
        bundle: &FrozenReviewBundleV1,
        evaluation_id: Uuid,
        binding_id: Uuid,
        agent_id: &str,
        agent_key: &SigningKey,
        attempt: u64,
        metric: i64,
    ) -> Value {
        signed_review_request(
            bundle,
            binding_id,
            agent_id,
            agent_key,
            Some(evaluation_id),
            attempt,
            metric,
        )
    }

    fn signed_review_request(
        bundle: &FrozenReviewBundleV1,
        binding_id: Uuid,
        agent_id: &str,
        agent_key: &SigningKey,
        reproduction_evaluation_id: Option<Uuid>,
        attempt: u64,
        metric: i64,
    ) -> Value {
        let (task_id, evaluation_id) = review_ids(bundle, reproduction_evaluation_id);
        let receipt_id = review_execution_receipt_id(
            binding_id,
            task_id,
            bundle.assignment_id,
            &bundle.bundle_hash,
            attempt,
            bundle.assignment_version,
        )
        .expect("review receipt id");
        let input_objects = bundle
            .objects
            .iter()
            .map(|object| FrozenReviewInputObjectV1 {
                object_key: object.object_key.clone(),
                logical_path: object.logical_path.clone(),
                role: object.role.clone(),
                digest: object.digest.clone(),
                size_bytes: object.size_bytes,
            })
            .collect::<Vec<_>>();
        let (output, statistical_evidence, candidate_passed) = match bundle.execution.kind.as_str()
        {
            "evaluate" => (
                json!({
                    "reference_metrics_micros": {"accuracy": metric},
                    "tolerance_policy_version": "1",
                    "tolerance_rules": [{
                        "kind": "absolute",
                        "metric": "accuracy",
                        "max_delta_micros": 1000,
                    }],
                    "candidate_passed": true,
                }),
                json!({
                    "schema": NONE_STATISTICAL_EVIDENCE_SCHEMA,
                    "reason": NONE_STATISTICAL_EVIDENCE_REASON,
                }),
                Some(true),
            ),
            "reproduce" => {
                let evidence = json!({
                    "accuracy": {
                        "interval_overlap_bps": 9_750,
                        "effect_delta_micros": 250,
                        "p_value_micros": 50_000,
                    },
                });
                (
                    json!({
                        "observed_metrics_micros": {"accuracy": metric},
                        "statistical_evidence": evidence,
                    }),
                    evidence,
                    None,
                )
            }
            _ => panic!("unsupported executable review fixture kind"),
        };
        let support_digests = bundle
            .objects
            .iter()
            .filter(|object| object.role == "evaluator_support")
            .map(|object| object.digest.clone())
            .collect::<Vec<_>>();
        let environment = json!({
            "schema": "hepta.paper_raid.review_execution_environment.v1",
            "adapter": bundle.execution.adapter,
            "bridge_version": "pg-review-fixture-v1",
            "runtime": "python3-stdlib",
            "runtime_path": "/usr/bin/python3",
            "runtime_flags": ["-I", "-S", "-B"],
            "loader_digest": sha256_digest(b"pg-review-frozen-loader-v1"),
            "evaluator_digest": bundle.execution.evaluator_version,
            "support_digests": support_digests,
            "review_kind": bundle.execution.kind,
            "platform": "linux",
            "architecture": "x86_64",
        });
        let stdout = b"pg review execution complete\n";
        let stderr = b"";
        let logs = json!({
            "schema": "hepta.paper_raid.review_execution_logs.v1",
            "stdout_hash": sha256_digest(stdout),
            "stderr_hash": sha256_digest(stderr),
            "stdout_bytes": stdout.len(),
            "stderr_bytes": stderr.len(),
            "truncated": false,
        });
        let now = Utc::now().timestamp();
        let agent_key_id = sha256_digest(agent_key.verifying_key().as_bytes());
        let mut receipt = ReviewExecutionReceiptV1 {
            schema: REVIEW_EXECUTION_RECEIPT_V1.to_string(),
            receipt_id,
            task_id,
            binding_id,
            assignment_id: bundle.assignment_id,
            paper_project_id: bundle.paper_project_id,
            submission_id: bundle.submission_id,
            evaluation_id,
            kind: bundle.execution.kind.clone(),
            attempt,
            fencing_token: bundle.assignment_version,
            bundle_hash: bundle.bundle_hash.clone(),
            evaluator_version: bundle.execution.evaluator_version.clone(),
            input_root: frozen_review_input_root(&input_objects).expect("review input root"),
            output_root: canonical_json_sha256(&output).expect("review output root"),
            metrics_hash: format!("sha256:{}", "0".repeat(64)),
            observed_metrics_micros: [("accuracy".to_string(), metric)].into(),
            statistical_evidence,
            candidate_passed,
            seed_set_hash: canonical_json_sha256(&json!([bundle.execution.seed]))
                .expect("review seed root"),
            environment_hash: canonical_json_sha256(&environment).expect("review environment hash"),
            run_manifest_hash: format!("sha256:{}", "0".repeat(64)),
            logs_hash: canonical_json_sha256(&logs).expect("review logs hash"),
            started_at_unix: now - 1,
            completed_at_unix: now,
            agent_id: agent_id.to_string(),
            agent_key_id: agent_key_id.clone(),
            signing_public_key_hash: agent_key_id,
            signature: String::new(),
        };
        receipt.metrics_hash =
            review_execution_metrics_hash(&receipt).expect("review metrics hash");
        let run_manifest = json!({
            "schema": "hepta.paper_raid.review_run_manifest.v1",
            "task_id": receipt.task_id,
            "assignment_id": receipt.assignment_id,
            "paper_project_id": receipt.paper_project_id,
            "submission_id": receipt.submission_id,
            "evaluation_id": receipt.evaluation_id,
            "kind": receipt.kind.clone(),
            "attempt": receipt.attempt,
            "fencing_token": receipt.fencing_token,
            "bundle_hash": receipt.bundle_hash.clone(),
            "evaluator_version": receipt.evaluator_version.clone(),
            "adapter": bundle.execution.adapter.clone(),
            "entrypoint": bundle.execution.entrypoint.clone(),
            "timeout_ms": bundle.execution.timeout_ms,
            "candidate_passed": receipt.candidate_passed,
            "input_objects": input_objects,
            "input_root": receipt.input_root.clone(),
            "output_root": receipt.output_root.clone(),
            "metrics_hash": receipt.metrics_hash.clone(),
            "seed": bundle.execution.seed,
            "seed_set_hash": receipt.seed_set_hash.clone(),
            "environment_hash": receipt.environment_hash.clone(),
            "logs_hash": receipt.logs_hash.clone(),
            "exit_code": 0,
            "elapsed_ms": 1000,
            "started_at_unix": receipt.started_at_unix,
            "completed_at_unix": receipt.completed_at_unix,
        });
        receipt.run_manifest_hash =
            canonical_json_sha256(&run_manifest).expect("review run manifest hash");
        receipt.signature = BASE64.encode(
            agent_key
                .sign(
                    &review_execution_receipt_signing_bytes(&receipt)
                        .expect("review receipt signing bytes"),
                )
                .to_bytes(),
        );
        json!({
            "schema": RECEIPT_REQUEST_SCHEMA,
            "idempotency_key": receipt.receipt_id,
            "receipt": receipt,
            "output": output,
            "environment": environment,
            "run_manifest": run_manifest,
            "logs": logs,
        })
    }

    fn signed_agent_headers(
        agent_key: &SigningKey,
        binding_id: Uuid,
        agent_id: &str,
        agent_key_id: &str,
        path: &str,
        body: &[u8],
    ) -> HeaderMap {
        signed_agent_headers_for(
            agent_key,
            binding_id,
            agent_id,
            agent_key_id,
            &Method::POST,
            path,
            "",
            Uuid::new_v4(),
            body,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn signed_agent_headers_for(
        agent_key: &SigningKey,
        binding_id: Uuid,
        agent_id: &str,
        agent_key_id: &str,
        method: &Method,
        path: &str,
        canonical_query: &str,
        nonce: Uuid,
        body: &[u8],
    ) -> HeaderMap {
        let now = Utc::now().timestamp();
        let claim = AgentBridgeRequestProofV1 {
            schema: AGENT_BRIDGE_REQUEST_PROOF_V1.to_string(),
            binding_id,
            agent_id: agent_id.to_string(),
            agent_key_id: agent_key_id.to_string(),
            http_method: method.as_str().to_string(),
            canonical_path: path.to_string(),
            canonical_query: canonical_query.to_string(),
            body_hash: sha256_digest(body),
            nonce,
            issued_at_unix: now,
            expires_at_unix: now + 30,
        };
        let signature = BASE64.encode(
            agent_key
                .sign(
                    &agent_bridge_request_proof_signing_bytes(&claim)
                        .expect("Agent request signing bytes"),
                )
                .to_bytes(),
        );
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("x-paper-raid-agent-schema", claim.schema.clone()),
            (
                "x-paper-raid-agent-binding-id",
                claim.binding_id.to_string(),
            ),
            ("x-paper-raid-agent-id", claim.agent_id.clone()),
            ("x-paper-raid-agent-key-id", claim.agent_key_id.clone()),
            ("x-paper-raid-agent-nonce", claim.nonce.to_string()),
            (
                "x-paper-raid-agent-issued-at",
                claim.issued_at_unix.to_string(),
            ),
            (
                "x-paper-raid-agent-expires-at",
                claim.expires_at_unix.to_string(),
            ),
            ("x-paper-raid-agent-body-sha256", claim.body_hash.clone()),
            ("x-paper-raid-agent-signature", signature),
        ] {
            headers.insert(
                header::HeaderName::from_bytes(name.as_bytes()).expect("Agent header name"),
                HeaderValue::from_str(&value).expect("Agent header value"),
            );
        }
        headers
    }

    fn encoded_digest(value: &str) -> String {
        format!(
            "sha256%3A{}",
            value
                .strip_prefix("sha256:")
                .expect("canonical digest query value")
        )
    }

    fn challenge_object_query(
        bundle: &AssignedChallengeMaterialBundleV1,
        object_key: &str,
        digest: &str,
        paper_id: &str,
        work_item_id: &str,
    ) -> String {
        format!(
            "bundle_hash={}&digest={}&object_key={object_key}&paper_id={paper_id}&work_item_id={work_item_id}",
            encoded_digest(&bundle.bundle_hash),
            encoded_digest(digest),
        )
    }

    fn review_object_query(
        bundle: &FrozenReviewBundleV1,
        object_key: &str,
        digest: &str,
        task_id: Uuid,
    ) -> String {
        format!(
            "assignment_id={}&bundle_hash={}&digest={}&object_key={object_key}&task_id={task_id}",
            bundle.assignment_id,
            encoded_digest(&bundle.bundle_hash),
            encoded_digest(digest),
        )
    }

    fn exact_review_queue_item(bundle: &FrozenReviewBundleV1, player_id: Uuid) -> Value {
        json!({
            "paper_project_id": bundle.paper_project_id,
            "submission_id": bundle.submission_id,
            "release_candidate_hash": bundle.release_candidate_hash,
            "paper_bundle_hash": bundle.paper_bundle_hash,
            "my_assignments": [{
                "assignment_id": bundle.assignment_id,
                "paper_project_id": bundle.paper_project_id,
                "submission_id": bundle.submission_id,
                "player_id": player_id,
                "slot": bundle.slot,
                "review_round": bundle.review_round,
                "version": bundle.assignment_version,
                "expires_at": bundle.expires_at,
                "status": "claimed",
            }],
        })
    }

    async fn router_request(
        router: Router,
        method: Method,
        uri: String,
        headers: HeaderMap,
        body: Vec<u8>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let request_body = if body.is_empty() {
            Body::empty()
        } else {
            Body::from(body)
        };
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .body(request_body)
            .expect("build Router request");
        *request.headers_mut() = headers;
        let response = router
            .oneshot(request)
            .await
            .expect("Router request response");
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .expect("Router response body")
            .to_vec();
        (status, headers, body)
    }

    fn cas_read_count(harness: &ReviewPgHarness, digest: &str) -> usize {
        harness
            .cas_reads
            .lock()
            .expect("CAS read counter lock")
            .get(digest)
            .copied()
            .unwrap_or_default()
    }

    async fn response_bytes(response: Response) -> (StatusCode, Vec<u8>) {
        let status = response.status();
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .expect("Agent response body")
            .to_vec();
        (status, body)
    }

    async fn browser_headers(state: &AppState, identity: &AlphaIdentity) -> HeaderMap {
        let issue = state
            .sessions
            .issue(identity)
            .await
            .expect("issue review browser session");
        let cookie = issue
            .cookie
            .to_str()
            .expect("review browser cookie")
            .split(';')
            .next()
            .expect("review browser cookie pair");
        assert!(cookie.starts_with(&format!("{SESSION_COOKIE}=")));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(cookie).expect("review browser cookie header"),
        );
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_str(state.config.public_origin.as_str().trim_end_matches('/'))
                .expect("review browser origin"),
        );
        headers.insert(
            CSRF_HEADER,
            HeaderValue::from_str(&issue.csrf).expect("review browser csrf"),
        );
        headers
    }

    async fn recover_browser_csrf(state: &AppState, headers: &mut HeaderMap) {
        let session = state
            .session(headers)
            .await
            .expect("authenticate browser session for csrf recovery");
        let csrf = state
            .sessions
            .refresh_csrf(headers, &session)
            .await
            .expect("recover browser csrf after lost response");
        headers.insert(
            CSRF_HEADER,
            HeaderValue::from_str(&csrf).expect("recovered review browser csrf"),
        );
    }

    async fn confirm_review_receipt(
        state: AppState,
        headers: HeaderMap,
        paper_id: Uuid,
        receipt_id: Uuid,
        request: &ReviewReceiptConfirmationRequest,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let response = confirm(
            State(state),
            headers,
            Path((paper_id, receipt_id)),
            Json(ReviewReceiptConfirmationRequest {
                signature: request.signature.clone(),
                receipt_context_signature: request.receipt_context_signature.clone(),
            }),
        )
        .await;
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .expect("review confirmation response body")
            .to_vec();
        (status, headers, body)
    }

    async fn request_review_signing_frame(
        state: AppState,
        headers: HeaderMap,
        paper_id: Uuid,
        receipt_id: Uuid,
        request: ReviewReceiptSigningFrameRequest,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let response = signing_frame(
            State(state),
            headers,
            Path((paper_id, receipt_id)),
            Json(request),
        )
        .await;
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .expect("review signing-frame response body")
            .to_vec();
        (status, headers, body)
    }

    fn confirmation_request_for_frame(
        frame: &Value,
        human_key: &SigningKey,
    ) -> ReviewReceiptConfirmationRequest {
        let signing_bytes = frame
            .get("signing_bytes")
            .and_then(Value::as_str)
            .and_then(|value| BASE64.decode(value).ok())
            .expect("upstream human signing bytes");
        let context_signing_bytes = frame
            .get("receipt_context_signing_bytes")
            .and_then(Value::as_str)
            .and_then(|value| BASE64.decode(value).ok())
            .expect("confirmation context signing bytes");
        ReviewReceiptConfirmationRequest {
            signature: BASE64.encode(human_key.sign(&signing_bytes).to_bytes()),
            receipt_context_signature: BASE64
                .encode(human_key.sign(&context_signing_bytes).to_bytes()),
        }
    }

    async fn prepare_confirmation_for_test(
        state: &AppState,
        identity: &AlphaIdentity,
        descriptor: &FrozenReviewBundleV1,
        envelope: &Value,
        human_key: &SigningKey,
    ) -> (Uuid, ReviewReceiptConfirmationRequest) {
        let paper_id = envelope_uuid(envelope, "paper_project_id");
        let receipt_id = envelope_uuid(envelope, "receipt_id");
        let mut tx = state
            .pool
            .begin()
            .await
            .expect("begin confirmation fixture");
        let stored = lock_receipt(&mut tx, identity, paper_id, receipt_id)
            .await
            .expect("lock confirmation fixture receipt");
        let validated = validate_stored_receipt(&stored, identity, descriptor)
            .expect("validate confirmation fixture receipt");
        let context = confirmation_context(&stored, &validated, descriptor);
        let upstream_signing_bytes = b"review-confirmation-upstream-signing-frame-v1";
        let signing_public_key = BASE64.encode(human_key.verifying_key().as_bytes());
        let signing_public_key_hash = sha256_digest(human_key.verifying_key().as_bytes());
        let frame = attach_confirmation_context(
            json!({
                "command": "create_paper_evaluation_draft",
                "resource_id": stored.paper_id,
                "child_id": Value::Null,
                "payload": {
                    "evaluator_player_id": stored.player_id,
                },
                "signing_bytes": BASE64.encode(upstream_signing_bytes),
                "signing_key_id": "review-confirmation-human-key-v1",
                "signing_public_key": signing_public_key,
                "signing_public_key_hash": signing_public_key_hash,
            }),
            context,
        )
        .expect("attach confirmation context fixture");
        let frame_hash = canonical_json_sha256(&frame).expect("confirmation fixture frame hash");
        let idempotency_key = confirmation_idempotency_key(&stored, &frame_hash);
        let updated = sqlx::query(
            "UPDATE paper_raid_bff_review_execution_receipts \
             SET confirmation_frame=$1,confirmation_frame_hash=$2,\
                 confirmation_idempotency_key=$3,updated_at=now() \
             WHERE receipt_id=$4 AND state='pending' AND confirmation_frame IS NULL",
        )
        .bind(&frame)
        .bind(&frame_hash)
        .bind(idempotency_key)
        .bind(receipt_id)
        .execute(&mut *tx)
        .await
        .expect("store confirmation fixture frame");
        assert_eq!(updated.rows_affected(), 1);
        tx.commit().await.expect("commit confirmation fixture");

        (
            idempotency_key,
            confirmation_request_for_frame(&frame, human_key),
        )
    }

    async fn receipt_lifecycle(pool: &PgPool, receipt_id: Uuid) -> String {
        sqlx::query_scalar(
            "SELECT state FROM paper_raid_bff_review_execution_receipts WHERE receipt_id=$1",
        )
        .bind(receipt_id)
        .fetch_one(pool)
        .await
        .expect("read review receipt lifecycle")
    }

    async fn submit_review_receipt(
        state: AppState,
        agent_key: &SigningKey,
        envelope: Value,
    ) -> (StatusCode, Vec<u8>) {
        let body = canonical_json_bytes(&envelope).expect("review request body");
        let receipt = envelope.get("receipt").expect("review receipt");
        let binding_id =
            serde_json::from_value(receipt["binding_id"].clone()).expect("receipt binding id");
        let agent_id = receipt["agent_id"].as_str().expect("receipt agent id");
        let agent_key_id = receipt["agent_key_id"]
            .as_str()
            .expect("receipt agent key id");
        let path = "/api/agent-bridge/review-receipts";
        let headers =
            signed_agent_headers(agent_key, binding_id, agent_id, agent_key_id, path, &body);
        let uri: axum::http::Uri = path.parse().expect("review receipt URI");
        let response = agent_bridge::agent_review_receipt(
            State(state),
            OriginalUri(uri),
            headers,
            Bytes::from(body),
        )
        .await
        .unwrap_or_else(IntoResponse::into_response);
        response_bytes(response).await
    }

    async fn review_inbox_projection(
        state: AppState,
        agent_key: &SigningKey,
        binding_id: Uuid,
        agent_id: &str,
        paper_id: Uuid,
    ) -> Value {
        let envelope = json!({
            "schema": "hepta.paper_raid.agent_bridge.inbox_request.v1",
            "paper_ids": [paper_id],
        });
        let body = canonical_json_bytes(&envelope).expect("review inbox body");
        let agent_key_id = sha256_digest(agent_key.verifying_key().as_bytes());
        let path = "/api/agent-bridge/inbox";
        let headers =
            signed_agent_headers(agent_key, binding_id, agent_id, &agent_key_id, path, &body);
        let uri: axum::http::Uri = path.parse().expect("review inbox URI");
        let response =
            agent_bridge::agent_inbox(State(state), OriginalUri(uri), headers, Bytes::from(body))
                .await
                .unwrap_or_else(IntoResponse::into_response);
        let (status, body) = response_bytes(response).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "review inbox projection failed: {}",
            String::from_utf8_lossy(&body),
        );
        serde_json::from_slice(&body).expect("review inbox JSON")
    }

    async fn invalidate_for_test(
        state: &AppState,
        identity: &AlphaIdentity,
        paper_id: Uuid,
        receipt_id: Uuid,
    ) {
        let mut tx = state.pool.begin().await.expect("begin invalidation");
        let stored = lock_receipt(&mut tx, identity, paper_id, receipt_id)
            .await
            .expect("lock receipt for invalidation");
        invalidate_receipt(&mut tx, &stored, "real_postgres_retry_fixture", Some(409))
            .await
            .expect("invalidate receipt through production helper");
        tx.commit().await.expect("commit invalidation");
    }

    async fn consume_for_test(pool: &PgPool, envelope: &Value) {
        // Confirmation is an upstream human-signature flow tested elsewhere. This constrained
        // SQL creates only the explicit old consumed lifecycle needed to prove that the Agent
        // endpoint's immutable receipt replay is independent of mutable lifecycle state.
        let receipt = &envelope["receipt"];
        let receipt_id: Uuid =
            serde_json::from_value(receipt["receipt_id"].clone()).expect("receipt id");
        let receipt_hash = review_execution_receipt_hash(
            &serde_json::from_value(receipt.clone()).expect("typed review receipt"),
        )
        .expect("receipt hash");
        let frame = json!({
            "schema": CONFIRMATION_FRAME_SCHEMA,
            "receipt_context_signing_bytes": BASE64.encode(b"review-pg-context"),
            "receipt_context": {
                "schema": CONFIRMATION_CONTEXT_SCHEMA,
                "receipt_id": receipt_id,
                "receipt_hash": receipt_hash,
                "task_id": receipt["task_id"],
                "assignment_id": receipt["assignment_id"],
                "assignment_version": receipt["fencing_token"],
                "paper_project_id": receipt["paper_project_id"],
                "submission_id": receipt["submission_id"],
                "evaluation_id": receipt["evaluation_id"],
                "kind": receipt["kind"],
                "bundle_hash": receipt["bundle_hash"],
                "candidate_passed": receipt["candidate_passed"],
            },
            "command": "create_paper_evaluation_draft",
            "resource_id": receipt["paper_project_id"],
            "child_id": Value::Null,
        });
        let frame_hash = canonical_json_sha256(&frame).expect("confirmation frame hash");
        let confirmation_hash =
            canonical_json_sha256(&json!({"confirmed": receipt_id})).expect("confirmation hash");
        let context_signature = BASE64.encode([17_u8; 64]);
        let updated = sqlx::query(
            "UPDATE paper_raid_bff_review_execution_receipts SET
                state='consumed',confirmation_frame=$1,confirmation_frame_hash=$2,
                confirmation_idempotency_key=$3,confirmation_hash=$4,
                confirmation_context_signature=$5,response_status=200,response_body=$6,
                consumed_at=now(),updated_at=now()
             WHERE receipt_id=$7 AND state='pending'",
        )
        .bind(frame)
        .bind(frame_hash)
        .bind(Uuid::new_v4())
        .bind(confirmation_hash)
        .bind(context_signature)
        .bind(b"{\"status\":\"confirmed\"}".as_slice())
        .bind(receipt_id)
        .execute(pool)
        .await
        .expect("construct explicit consumed old state");
        assert_eq!(updated.rows_affected(), 1);
    }

    fn envelope_uuid(envelope: &Value, field: &str) -> Uuid {
        serde_json::from_value(envelope["receipt"][field].clone()).expect("receipt UUID")
    }

    #[test]
    fn confirmation_id_is_stable_and_frame_bound() {
        let receipt = Uuid::parse_str("10000000-0000-4000-8000-000000000001").unwrap();
        let evaluation = Uuid::parse_str("20000000-0000-4000-8000-000000000002").unwrap();
        let left = stable_uuid(
            CONFIRMATION_ID_DOMAIN,
            &[
                &receipt.to_string(),
                &evaluation.to_string(),
                &format!("sha256:{}", "a".repeat(64)),
            ],
        );
        let same = stable_uuid(
            CONFIRMATION_ID_DOMAIN,
            &[
                &receipt.to_string(),
                &evaluation.to_string(),
                &format!("sha256:{}", "a".repeat(64)),
            ],
        );
        let changed = stable_uuid(
            CONFIRMATION_ID_DOMAIN,
            &[
                &receipt.to_string(),
                &evaluation.to_string(),
                &format!("sha256:{}", "b".repeat(64)),
            ],
        );
        assert_eq!(left, same);
        assert_ne!(left, changed);
    }

    #[test]
    fn strict_manifest_requires_evaluation_timeout_and_exit_code() {
        assert!(RECEIPT_REQUEST_SCHEMA.ends_with(".v1"));
        let required = ["evaluation_id", "timeout_ms", "exit_code"];
        let source = include_str!("review_receipts.rs");
        assert!(required.iter().all(|marker| source.contains(marker)));
    }

    #[tokio::test]
    async fn real_postgres_router_object_surfaces_signed_get_replay_and_audience_separation() {
        let Ok(database_url) = std::env::var("PAPER_RAID_BFF_TEST_DATABASE_URL") else {
            eprintln!("PAPER_RAID_BFF_TEST_DATABASE_URL is unset; object Router PG gate skipped");
            return;
        };
        let harness = review_pg_harness(database_url, true).await;
        let agent_key_id = sha256_digest(harness.agent_key.verifying_key().as_bytes());

        // The Browser authority is intentionally a strict superset of the Agent executable
        // descriptor.  Prove both directions of the executable-subset bijection and reject
        // substitution, omission, duplicate authority and human-object injection mutants.
        let lineage = &harness.bundles[0];
        assert_eq!(lineage.authority.artifact_objects.len(), 6);
        assert_eq!(lineage.objects.len(), 3);
        assert!(executable_artifact_lineage_complete(lineage));
        let mut substituted_lineage = lineage.clone();
        substituted_lineage.objects[0].digest = format!("sha256:{}", "f".repeat(64));
        assert!(!executable_artifact_lineage_complete(&substituted_lineage));
        let mut omitted_lineage = lineage.clone();
        omitted_lineage.objects.pop();
        assert!(!executable_artifact_lineage_complete(&omitted_lineage));
        let mut duplicate_authority = lineage.clone();
        let authority_candidate = duplicate_authority
            .authority
            .artifact_objects
            .iter()
            .find(|object| object.role == "candidate")
            .expect("authority candidate")
            .clone();
        duplicate_authority
            .authority
            .artifact_objects
            .push(authority_candidate);
        assert!(!executable_artifact_lineage_complete(&duplicate_authority));
        let mut human_injection = lineage.clone();
        let human_object = human_injection
            .authority
            .artifact_objects
            .iter()
            .find(|object| object.role == "paper_source")
            .expect("authority paper source")
            .clone();
        human_injection.objects.push(human_object);
        assert!(!executable_artifact_lineage_complete(&human_injection));

        let challenge_path = "/api/agent-bridge/challenge-objects";
        let challenge_object = harness
            .challenge
            .bundle
            .objects
            .iter()
            .find(|object| object.object_key == "brief")
            .expect("challenge brief object");
        let challenge_query = challenge_object_query(
            &harness.challenge.bundle,
            &challenge_object.object_key,
            &challenge_object.digest,
            &harness.challenge.bundle.paper_project_id.to_string(),
            &harness.challenge.bundle.work_item_id.to_string(),
        );
        assert_eq!(
            challenge_query,
            format!(
                "bundle_hash={}&digest={}&object_key=brief&paper_id={}&work_item_id={}",
                encoded_digest(&harness.challenge.bundle.bundle_hash),
                encoded_digest(&challenge_object.digest),
                harness.challenge.bundle.paper_project_id,
                harness.challenge.bundle.work_item_id,
            ),
            "challenge query is not canonical key-order/lowercase-dashed UUID text",
        );
        let challenge_nonce = Uuid::new_v4();
        let challenge_headers = signed_agent_headers_for(
            &harness.agent_key,
            harness.binding_id,
            &harness.agent_id,
            &agent_key_id,
            &Method::GET,
            challenge_path,
            &challenge_query,
            challenge_nonce,
            &[],
        );
        assert_eq!(
            challenge_headers["x-paper-raid-agent-body-sha256"]
                .to_str()
                .expect("signed GET body digest header"),
            sha256_digest(&[]),
            "signed GET did not bind SHA-256(empty)",
        );
        let challenge_uri = format!("{challenge_path}?{challenge_query}");
        let (first_status, first_headers, first_body) = router_request(
            crate::app::router(harness.state.clone()),
            Method::GET,
            challenge_uri.clone(),
            challenge_headers.clone(),
            vec![],
        )
        .await;
        assert_eq!(first_status, StatusCode::OK, "mounted Challenge GET failed");
        assert_eq!(first_body, harness.challenge.object_bytes);
        assert_eq!(
            first_headers[header::CONTENT_TYPE]
                .to_str()
                .expect("Challenge Content-Type"),
            challenge_object.media_type
        );
        assert_eq!(
            first_headers[header::CACHE_CONTROL]
                .to_str()
                .expect("Challenge Cache-Control"),
            "private, no-store"
        );

        let restarted = AppState::connect(harness.config.clone())
            .await
            .expect("restart object Router BFF state");
        let (replay_status, replay_headers, replay_body) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            challenge_uri.clone(),
            challenge_headers.clone(),
            vec![],
        )
        .await;
        assert_eq!(replay_status, first_status);
        assert_eq!(replay_body, first_body, "restarted replay body drifted");
        assert_eq!(
            replay_headers[header::CONTENT_TYPE],
            first_headers[header::CONTENT_TYPE]
        );
        let request_uses = sqlx::query(
            "SELECT response_status,response_body FROM paper_raid_bff_agent_request_uses \
             WHERE binding_id=$1 AND nonce=$2",
        )
        .bind(harness.binding_id)
        .bind(challenge_nonce)
        .fetch_all(&restarted.pool)
        .await
        .expect("read Challenge request-use replay row");
        assert_eq!(
            request_uses.len(),
            1,
            "Challenge replay created another row"
        );
        assert_eq!(request_uses[0].get::<i32, _>("response_status"), 200);
        assert_eq!(
            request_uses[0].get::<Vec<u8>, _>("response_body"),
            first_body
        );

        // Keep the hostile row inside the catalog's 2-byte lower bound so the
        // request-use table accepts it, then prove the route independently
        // rejects drift from the Challenge authority's frozen byte length.
        let wrong_size_replay = if first_body.len() == 2 {
            vec![0_u8; 3]
        } else {
            vec![0_u8; 2]
        };
        sqlx::query(
            "UPDATE paper_raid_bff_agent_request_uses SET response_body=$3 \
             WHERE binding_id=$1 AND nonce=$2",
        )
        .bind(harness.binding_id)
        .bind(challenge_nonce)
        .bind(&wrong_size_replay)
        .execute(&restarted.pool)
        .await
        .expect("mutate Challenge replay body length");
        let (wrong_size_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            challenge_uri.clone(),
            challenge_headers.clone(),
            vec![],
        )
        .await;
        assert_eq!(
            wrong_size_status,
            StatusCode::CONFLICT,
            "Challenge replay accepted a body with a different frozen byte length",
        );
        sqlx::query(
            "UPDATE paper_raid_bff_agent_request_uses SET response_body=$3 \
             WHERE binding_id=$1 AND nonce=$2",
        )
        .bind(harness.binding_id)
        .bind(challenge_nonce)
        .bind(&first_body)
        .execute(&restarted.pool)
        .await
        .expect("restore Challenge replay body");

        let rebound_query = challenge_object_query(
            &harness.challenge.bundle,
            &challenge_object.object_key,
            &challenge_object.digest,
            &harness.challenge.bundle.paper_project_id.to_string(),
            &Uuid::new_v4().to_string(),
        );
        let rebound_headers = signed_agent_headers_for(
            &harness.agent_key,
            harness.binding_id,
            &harness.agent_id,
            &agent_key_id,
            &Method::GET,
            challenge_path,
            &rebound_query,
            challenge_nonce,
            &[],
        );
        let (rebound_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{challenge_path}?{rebound_query}"),
            rebound_headers,
            vec![],
        )
        .await;
        assert_eq!(
            rebound_status,
            StatusCode::CONFLICT,
            "one nonce was rebound to another canonical query",
        );

        let extra_query = format!("attacker=1&{challenge_query}");
        let extra_headers = signed_agent_headers_for(
            &harness.agent_key,
            harness.binding_id,
            &harness.agent_id,
            &agent_key_id,
            &Method::GET,
            challenge_path,
            &extra_query,
            Uuid::new_v4(),
            &[],
        );
        let (extra_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{challenge_path}?{extra_query}"),
            extra_headers,
            vec![],
        )
        .await;
        assert_eq!(extra_status, StatusCode::BAD_REQUEST);

        let duplicate_query =
            challenge_query.replacen("&paper_id=", "&object_key=brief&paper_id=", 1);
        let (duplicate_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{challenge_path}?{duplicate_query}"),
            challenge_headers.clone(),
            vec![],
        )
        .await;
        assert_eq!(duplicate_status, StatusCode::BAD_REQUEST);

        let simple_paper_id = harness
            .challenge
            .bundle
            .paper_project_id
            .simple()
            .to_string();
        let simple_uuid_query = challenge_object_query(
            &harness.challenge.bundle,
            &challenge_object.object_key,
            &challenge_object.digest,
            &simple_paper_id,
            &harness.challenge.bundle.work_item_id.to_string(),
        );
        let simple_uuid_headers = signed_agent_headers_for(
            &harness.agent_key,
            harness.binding_id,
            &harness.agent_id,
            &agent_key_id,
            &Method::GET,
            challenge_path,
            &simple_uuid_query,
            Uuid::new_v4(),
            &[],
        );
        let (simple_uuid_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{challenge_path}?{simple_uuid_query}"),
            simple_uuid_headers,
            vec![],
        )
        .await;
        assert_eq!(simple_uuid_status, StatusCode::BAD_REQUEST);

        let uppercase_uuid = harness
            .challenge
            .bundle
            .paper_project_id
            .to_string()
            .to_ascii_uppercase();
        let uppercase_uuid_query = challenge_object_query(
            &harness.challenge.bundle,
            &challenge_object.object_key,
            &challenge_object.digest,
            &uppercase_uuid,
            &harness.challenge.bundle.work_item_id.to_string(),
        );
        let uppercase_uuid_headers = signed_agent_headers_for(
            &harness.agent_key,
            harness.binding_id,
            &harness.agent_id,
            &agent_key_id,
            &Method::GET,
            challenge_path,
            &uppercase_uuid_query,
            Uuid::new_v4(),
            &[],
        );
        let (uppercase_uuid_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{challenge_path}?{uppercase_uuid_query}"),
            uppercase_uuid_headers,
            vec![],
        )
        .await;
        assert_eq!(uppercase_uuid_status, StatusCode::BAD_REQUEST);

        let ordered_parts = challenge_query.split('&').collect::<Vec<_>>();
        let out_of_order_query = format!(
            "{}&{}&{}&{}&{}",
            ordered_parts[1],
            ordered_parts[0],
            ordered_parts[2],
            ordered_parts[3],
            ordered_parts[4],
        );
        let (out_of_order_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{challenge_path}?{out_of_order_query}"),
            challenge_headers.clone(),
            vec![],
        )
        .await;
        assert_eq!(out_of_order_status, StatusCode::FORBIDDEN);

        let lowercase_percent_query = challenge_query.replace("%3A", "%3a");
        let (lowercase_percent_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{challenge_path}?{lowercase_percent_query}"),
            challenge_headers.clone(),
            vec![],
        )
        .await;
        assert_eq!(lowercase_percent_status, StatusCode::FORBIDDEN);

        let (nonempty_body_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            challenge_uri,
            challenge_headers,
            b"not-empty".to_vec(),
        )
        .await;
        assert_eq!(nonempty_body_status, StatusCode::FORBIDDEN);

        let review_bundle = &harness.bundles[0];
        let paper_source = review_bundle
            .authority
            .artifact_objects
            .iter()
            .find(|object| object.role == "paper_source")
            .expect("review paper source");
        let human_reads_before = cas_read_count(&harness, &paper_source.digest);
        let browser_query = format!(
            "assignment_id={}&bundle_hash={}&object_key={}&presentation=inline",
            review_bundle.assignment_id,
            encoded_digest(&review_bundle.bundle_hash),
            paper_source.object_key,
        );
        let browser_uri = format!(
            "/api/review/papers/{}/artifacts/{}?{}",
            review_bundle.paper_project_id, paper_source.digest, browser_query,
        );
        let browser_request_headers = browser_headers(&restarted, &harness.identity).await;
        let (browser_status, browser_response_headers, browser_body) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            browser_uri,
            browser_request_headers,
            vec![],
        )
        .await;
        assert_eq!(browser_status, StatusCode::OK);
        assert_eq!(browser_body, harness.paper_source_bytes);
        assert_eq!(
            browser_response_headers[header::CONTENT_TYPE]
                .to_str()
                .expect("Browser review Content-Type"),
            paper_source.media_type
        );
        assert!(browser_response_headers[header::CONTENT_DISPOSITION]
            .to_str()
            .expect("browser disposition")
            .starts_with("inline;"));
        let human_reads_after_browser = cas_read_count(&harness, &paper_source.digest);
        assert_eq!(human_reads_after_browser, human_reads_before + 1);

        let (review_task_id, _) = review_ids(review_bundle, None);
        let executable = review_bundle
            .objects
            .iter()
            .find(|object| object.role == "candidate")
            .expect("review candidate object");
        let executable_query = review_object_query(
            review_bundle,
            &executable.object_key,
            &executable.digest,
            review_task_id,
        );
        let review_path = "/api/agent-bridge/review-objects";
        let executable_headers = signed_agent_headers_for(
            &harness.agent_key,
            harness.binding_id,
            &harness.agent_id,
            &agent_key_id,
            &Method::GET,
            review_path,
            &executable_query,
            Uuid::new_v4(),
            &[],
        );
        let (executable_status, _, executable_body) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{review_path}?{executable_query}"),
            executable_headers,
            vec![],
        )
        .await;
        assert_eq!(executable_status, StatusCode::OK);
        assert_eq!(executable_body, harness.candidate_bytes);
        let executable_reads_after_success = cas_read_count(&harness, &executable.digest);

        let mut cross_release_queue =
            exact_review_queue_item(review_bundle, harness.identity.player_id);
        cross_release_queue["release_candidate_hash"] = json!(format!("sha256:{}", "e".repeat(64)));
        *harness
            .review_queue_override
            .lock()
            .expect("cross-release queue override lock") = Some(json!([cross_release_queue]));
        let cross_release_headers = signed_agent_headers_for(
            &harness.agent_key,
            harness.binding_id,
            &harness.agent_id,
            &agent_key_id,
            &Method::GET,
            review_path,
            &executable_query,
            Uuid::new_v4(),
            &[],
        );
        let (cross_release_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{review_path}?{executable_query}"),
            cross_release_headers,
            vec![],
        )
        .await;
        assert_eq!(cross_release_status, StatusCode::FORBIDDEN);

        let mut wrong_assignment_queue =
            exact_review_queue_item(review_bundle, harness.identity.player_id);
        wrong_assignment_queue["my_assignments"][0]["version"] =
            json!(review_bundle.assignment_version + 1);
        *harness
            .review_queue_override
            .lock()
            .expect("wrong-assignment queue override lock") = Some(json!([wrong_assignment_queue]));
        let wrong_assignment_headers = signed_agent_headers_for(
            &harness.agent_key,
            harness.binding_id,
            &harness.agent_id,
            &agent_key_id,
            &Method::GET,
            review_path,
            &executable_query,
            Uuid::new_v4(),
            &[],
        );
        let (wrong_assignment_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{review_path}?{executable_query}"),
            wrong_assignment_headers,
            vec![],
        )
        .await;
        assert_eq!(wrong_assignment_status, StatusCode::FORBIDDEN);

        let exact_queue = exact_review_queue_item(review_bundle, harness.identity.player_id);
        *harness
            .review_queue_override
            .lock()
            .expect("duplicate queue override lock") =
            Some(json!([exact_queue.clone(), exact_queue]));
        let duplicate_assignment_headers = signed_agent_headers_for(
            &harness.agent_key,
            harness.binding_id,
            &harness.agent_id,
            &agent_key_id,
            &Method::GET,
            review_path,
            &executable_query,
            Uuid::new_v4(),
            &[],
        );
        let (duplicate_assignment_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{review_path}?{executable_query}"),
            duplicate_assignment_headers,
            vec![],
        )
        .await;
        assert_eq!(duplicate_assignment_status, StatusCode::FORBIDDEN);
        *harness
            .review_queue_override
            .lock()
            .expect("clear queue override lock") = None;

        let wrong_task_query = review_object_query(
            review_bundle,
            &executable.object_key,
            &executable.digest,
            Uuid::new_v4(),
        );
        let wrong_task_headers = signed_agent_headers_for(
            &harness.agent_key,
            harness.binding_id,
            &harness.agent_id,
            &agent_key_id,
            &Method::GET,
            review_path,
            &wrong_task_query,
            Uuid::new_v4(),
            &[],
        );
        let (wrong_task_status, _, _) = router_request(
            crate::app::router(restarted.clone()),
            Method::GET,
            format!("{review_path}?{wrong_task_query}"),
            wrong_task_headers,
            vec![],
        )
        .await;
        assert_eq!(wrong_task_status, StatusCode::FORBIDDEN);
        assert_eq!(
            cas_read_count(&harness, &executable.digest),
            executable_reads_after_success,
            "queue/assignment/task mismatch reached executable CAS bytes",
        );

        let human_agent_query = review_object_query(
            review_bundle,
            &paper_source.object_key,
            &paper_source.digest,
            review_task_id,
        );
        let human_agent_headers = signed_agent_headers_for(
            &harness.agent_key,
            harness.binding_id,
            &harness.agent_id,
            &agent_key_id,
            &Method::GET,
            review_path,
            &human_agent_query,
            Uuid::new_v4(),
            &[],
        );
        let (human_agent_status, _, _) = router_request(
            crate::app::router(restarted),
            Method::GET,
            format!("{review_path}?{human_agent_query}"),
            human_agent_headers,
            vec![],
        )
        .await;
        assert_eq!(human_agent_status, StatusCode::NOT_FOUND);
        assert_eq!(
            cas_read_count(&harness, &paper_source.digest),
            human_reads_after_browser,
            "Agent audience read human paper bytes from CAS",
        );
    }

    #[tokio::test]
    async fn real_postgres_review_receipt_attempt_concurrency_replay_restart_and_integrity() {
        let Ok(database_url) = std::env::var("PAPER_RAID_BFF_TEST_DATABASE_URL") else {
            eprintln!("PAPER_RAID_BFF_TEST_DATABASE_URL is unset; review receipt PG gate skipped");
            return;
        };
        let harness = review_pg_harness(database_url, false).await;
        let initial_status = crate::db::agent_bridge_schema_status(&harness.state.pool).await;
        assert!(
            initial_status.schema_ready,
            "review receipt catalog is not ready"
        );
        assert!(
            initial_status.integrity_ok,
            "fresh review receipt integrity failed"
        );

        let binding = sqlx::query(
            "SELECT binding_id,agent_id FROM paper_raid_bff_agent_bridge_bindings
             WHERE subject_id=$1",
        )
        .bind(&harness.identity.subject_id)
        .fetch_one(&harness.state.pool)
        .await
        .expect("load review binding");
        let binding_id: Uuid = binding.get("binding_id");
        let agent_id: String = binding.get("agent_id");

        let first = review_request(
            &harness.bundles[0],
            binding_id,
            &agent_id,
            &harness.agent_key,
            1,
            900_000,
        );
        let (same_left, same_right) = tokio::join!(
            submit_review_receipt(harness.state.clone(), &harness.agent_key, first.clone(),),
            submit_review_receipt(harness.state.clone(), &harness.agent_key, first.clone(),),
        );
        assert!(
            same_left.0 == StatusCode::OK && same_right.0 == StatusCode::OK,
            "same receipt concurrent submit failed: left_status={} left_body={} \
             right_status={} right_body={}",
            same_left.0,
            String::from_utf8_lossy(&same_left.1),
            same_right.0,
            String::from_utf8_lossy(&same_right.1),
        );
        assert_eq!(
            same_left.1, same_right.1,
            "same receipt replay body drifted"
        );
        let first_body = same_left.1;
        let stored_value: Value =
            serde_json::from_slice(&first_body).expect("typed stored receipt response");
        let typed_first: ReviewExecutionReceiptV1 =
            serde_json::from_value(first["receipt"].clone()).expect("typed first receipt");
        let expected_receipt_hash =
            review_execution_receipt_hash(&typed_first).expect("first receipt hash");
        assert_eq!(
            stored_value,
            json!({
                "schema": "hepta.paper_raid.agent_bridge.review_receipt_result.v1",
                "receipt_id": typed_first.receipt_id,
                "task_id": typed_first.task_id,
                "attempt": typed_first.attempt,
                "receipt_hash": expected_receipt_hash,
                "status": "stored",
            }),
            "stored receipt response is not the immutable six-field contract",
        );
        let first_task = envelope_uuid(&first, "task_id");
        let first_receipt = envelope_uuid(&first, "receipt_id");
        let first_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM paper_raid_bff_review_execution_receipts
             WHERE task_id=$1 AND attempt=1",
        )
        .bind(first_task)
        .fetch_one(&harness.state.pool)
        .await
        .expect("count same-attempt rows");
        assert_eq!(first_count, 1);

        let pending_replay =
            submit_review_receipt(harness.state.clone(), &harness.agent_key, first.clone()).await;
        assert_eq!(pending_replay, (StatusCode::OK, first_body.clone()));
        for forbidden_attempt in [2_u64, 3] {
            let forbidden = review_request(
                &harness.bundles[0],
                binding_id,
                &agent_id,
                &harness.agent_key,
                forbidden_attempt,
                900_000,
            );
            assert_eq!(
                submit_review_receipt(harness.state.clone(), &harness.agent_key, forbidden,)
                    .await
                    .0,
                StatusCode::CONFLICT,
                "pending task accepted attempt {forbidden_attempt}",
            );
        }

        invalidate_for_test(
            &harness.state,
            &harness.identity,
            harness.bundles[0].paper_project_id,
            first_receipt,
        )
        .await;
        let invalidated_replay =
            submit_review_receipt(harness.state.clone(), &harness.agent_key, first.clone()).await;
        assert_eq!(invalidated_replay, (StatusCode::OK, first_body.clone()));
        let skipped = review_request(
            &harness.bundles[0],
            binding_id,
            &agent_id,
            &harness.agent_key,
            3,
            900_000,
        );
        assert_eq!(
            submit_review_receipt(harness.state.clone(), &harness.agent_key, skipped)
                .await
                .0,
            StatusCode::CONFLICT,
        );

        let restarted = AppState::connect(harness.config.clone())
            .await
            .expect("reconnect review BFF after invalidation");
        let projection = review_inbox_projection(
            restarted.clone(),
            &harness.agent_key,
            binding_id,
            &agent_id,
            harness.bundles[0].paper_project_id,
        )
        .await;
        let projected_task = &projection["papers"][0]["review_tasks"]["items"][0];
        assert_eq!(projected_task["task_id"], json!(first_task));
        assert_eq!(projected_task["attempt"], json!(2));
        assert_eq!(projected_task["state"], json!("pending"));
        let second = review_request(
            &harness.bundles[0],
            binding_id,
            &agent_id,
            &harness.agent_key,
            2,
            900_000,
        );
        let second_stored =
            submit_review_receipt(restarted.clone(), &harness.agent_key, second.clone()).await;
        assert_eq!(second_stored.0, StatusCode::OK);
        consume_for_test(&restarted.pool, &second).await;
        let consumed_replay =
            submit_review_receipt(restarted.clone(), &harness.agent_key, second.clone()).await;
        assert_eq!(consumed_replay, second_stored);

        let race_left = review_request(
            &harness.bundles[1],
            binding_id,
            &agent_id,
            &harness.agent_key,
            1,
            910_000,
        );
        let race_right = review_request(
            &harness.bundles[1],
            binding_id,
            &agent_id,
            &harness.agent_key,
            1,
            920_000,
        );
        let race_task = envelope_uuid(&race_left, "task_id");
        let (left, right) = tokio::join!(
            submit_review_receipt(restarted.clone(), &harness.agent_key, race_left),
            submit_review_receipt(restarted.clone(), &harness.agent_key, race_right),
        );
        let mut statuses = [left.0, right.0];
        statuses.sort_by_key(|status| status.as_u16());
        assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);
        let race_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM paper_raid_bff_review_execution_receipts
             WHERE task_id=$1 AND attempt=1",
        )
        .bind(race_task)
        .fetch_one(&restarted.pool)
        .await
        .expect("count competing receipt rows");
        assert_eq!(race_count, 1);

        let sequence_violations: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM (
                SELECT task_id,attempt,state,
                    lag(attempt) OVER (PARTITION BY task_id ORDER BY attempt) previous_attempt,
                    lead(attempt) OVER (PARTITION BY task_id ORDER BY attempt) next_attempt
                FROM paper_raid_bff_review_execution_receipts
             ) sequence
             WHERE (previous_attempt IS NULL AND attempt<>1)
                OR (previous_attempt IS NOT NULL AND attempt<>previous_attempt+1)
                OR (next_attempt IS NOT NULL AND state<>'invalidated')",
        )
        .fetch_one(&restarted.pool)
        .await
        .expect("scan receipt attempt sequence");
        assert_eq!(sequence_violations, 0);
        let multiple_live: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM (
                SELECT task_id FROM paper_raid_bff_review_execution_receipts
                WHERE state IN ('pending','consumed') GROUP BY task_id HAVING count(*)>1
             ) live",
        )
        .fetch_one(&restarted.pool)
        .await
        .expect("scan live receipt cardinality");
        assert_eq!(multiple_live, 0);
        let final_status = crate::db::agent_bridge_schema_status(&restarted.pool).await;
        assert!(final_status.schema_ready);
        assert!(final_status.integrity_ok);

        let dispositions = sqlx::query(
            "SELECT outcome,metadata->>'storage_disposition' AS disposition
             FROM paper_raid_bff_agent_bridge_audit
             WHERE action='review_execution_receipt' AND metadata->>'task_id'=$1",
        )
        .bind(first_task.to_string())
        .fetch_all(&restarted.pool)
        .await
        .expect("read review receipt audit");
        assert!(dispositions
            .iter()
            .all(|row| row.get::<String, _>("outcome") == "succeeded"));
        assert!(dispositions
            .iter()
            .any(|row| row.get::<String, _>("disposition") == "accepted"));
        assert!(dispositions
            .iter()
            .any(|row| row.get::<String, _>("disposition") == "replayed"));
        let invalidation_audit: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM paper_raid_bff_agent_bridge_audit
                WHERE action='review_execution_receipt_invalidated'
                  AND outcome='succeeded'
                  AND metadata->>'task_id'=$1
                  AND metadata->>'lifecycle_disposition'='invalidated'
             )",
        )
        .bind(first_task.to_string())
        .fetch_one(&restarted.pool)
        .await
        .expect("read review invalidation audit");
        assert!(invalidation_audit);

        // Drive the actual browser confirmation handler through ambiguous and authoritative
        // upstream outcomes.  429/401, a response timeout, and 5xx cannot prove whether Hepta
        // committed, so the exact frozen confirmation remains pending.  Only the documented
        // semantic rejection set may retire the attempt and expose attempt+1 to the Bridge.
        let classification = review_request(
            &harness.bundles[2],
            binding_id,
            &agent_id,
            &harness.agent_key,
            1,
            930_000,
        );
        let classification_stored = submit_review_receipt(
            restarted.clone(),
            &harness.agent_key,
            classification.clone(),
        )
        .await;
        assert_eq!(classification_stored.0, StatusCode::OK);
        let classification_receipt = envelope_uuid(&classification, "receipt_id");
        let classification_task = envelope_uuid(&classification, "task_id");
        let human_key = SigningKey::from_bytes(&[91_u8; 32]);
        let (classification_key, classification_confirmation) = prepare_confirmation_for_test(
            &restarted,
            &harness.identity,
            &harness.bundles[2],
            &classification,
            &human_key,
        )
        .await;
        let classification_call_start = harness
            .command_calls
            .lock()
            .expect("classification call lock")
            .len();
        harness
            .command_replies
            .lock()
            .expect("classification reply lock")
            .extend([
                ReviewCommandReply {
                    status: StatusCode::TOO_MANY_REQUESTS,
                    body: json!({"error": "ambiguous_rate_limit"}),
                    delay_before_response: Duration::ZERO,
                },
                ReviewCommandReply {
                    status: StatusCode::UNAUTHORIZED,
                    body: json!({"error": "ambiguous_auth_refresh"}),
                    delay_before_response: Duration::ZERO,
                },
                ReviewCommandReply {
                    status: StatusCode::OK,
                    body: json!({"result": "response_arrived_after_client_timeout"}),
                    delay_before_response: Duration::from_secs(9),
                },
                ReviewCommandReply {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    body: json!({"error": "ambiguous_server_failure"}),
                    delay_before_response: Duration::ZERO,
                },
                ReviewCommandReply {
                    status: StatusCode::UNPROCESSABLE_ENTITY,
                    body: json!({"error": "semantic_rejection"}),
                    delay_before_response: Duration::ZERO,
                },
            ]);
        for (expected_status, expected_lifecycle) in [
            (StatusCode::TOO_MANY_REQUESTS, "pending"),
            (StatusCode::UNAUTHORIZED, "pending"),
            (StatusCode::SERVICE_UNAVAILABLE, "pending"),
            (StatusCode::SERVICE_UNAVAILABLE, "pending"),
            (StatusCode::UNPROCESSABLE_ENTITY, "invalidated"),
        ] {
            let response = confirm_review_receipt(
                restarted.clone(),
                browser_headers(&restarted, &harness.identity).await,
                harness.bundles[2].paper_project_id,
                classification_receipt,
                &classification_confirmation,
            )
            .await;
            assert_eq!(
                response.0,
                expected_status,
                "confirmation classification failed: expected={expected_status} body={}",
                String::from_utf8_lossy(&response.2),
            );
            assert_eq!(
                receipt_lifecycle(&restarted.pool, classification_receipt).await,
                expected_lifecycle,
                "upstream status {expected_status} produced the wrong lifecycle",
            );
        }
        {
            let calls = harness
                .command_calls
                .lock()
                .expect("classification calls lock");
            let calls = &calls[classification_call_start..];
            assert_eq!(calls.len(), 5);
            assert!(calls.iter().all(|call| call.body == calls[0].body));
            assert!(calls.iter().all(|call| {
                call.idempotency_key == classification_key.to_string()
                    && call.nonce == classification_key.to_string()
            }));
        }
        let semantic_invalidation_audit: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM paper_raid_bff_agent_bridge_audit
                WHERE action='review_execution_receipt_invalidated'
                  AND outcome='succeeded'
                  AND metadata->>'task_id'=$1
                  AND metadata->>'reason'='authoritative_semantic_rejection'
                  AND metadata->>'upstream_status'='422'
             )",
        )
        .bind(classification_task.to_string())
        .fetch_one(&restarted.pool)
        .await
        .expect("read semantic review invalidation audit");
        assert!(semantic_invalidation_audit);

        // A delayed successful upstream response forces the response-uncertain timeout window:
        // the browser loses the rotated CSRF token, recovers it through the durable session, then
        // replays the exact confirmation.  The same upstream idempotency nonce and bytes must be
        // used, and the consumed response must remain byte-for-byte immutable on later replay.
        let recovery = review_request(
            &harness.bundles[3],
            binding_id,
            &agent_id,
            &harness.agent_key,
            1,
            940_000,
        );
        assert_eq!(
            submit_review_receipt(restarted.clone(), &harness.agent_key, recovery.clone())
                .await
                .0,
            StatusCode::OK,
        );
        let recovery_receipt = envelope_uuid(&recovery, "receipt_id");
        let (recovery_key, recovery_confirmation) = prepare_confirmation_for_test(
            &restarted,
            &harness.identity,
            &harness.bundles[3],
            &recovery,
            &human_key,
        )
        .await;
        let recovery_call_start = harness
            .command_calls
            .lock()
            .expect("recovery call lock")
            .len();
        harness
            .command_replies
            .lock()
            .expect("recovery reply lock")
            .extend([
                ReviewCommandReply {
                    status: StatusCode::CREATED,
                    body: json!({"result": "lost_response_recovered"}),
                    delay_before_response: Duration::from_secs(9),
                },
                ReviewCommandReply {
                    status: StatusCode::CREATED,
                    body: json!({"result": "lost_response_recovered"}),
                    delay_before_response: Duration::ZERO,
                },
            ]);
        let mut recovery_headers = browser_headers(&restarted, &harness.identity).await;
        let lost = confirm_review_receipt(
            restarted.clone(),
            recovery_headers.clone(),
            harness.bundles[3].paper_project_id,
            recovery_receipt,
            &recovery_confirmation,
        )
        .await;
        assert_eq!(lost.0, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            receipt_lifecycle(&restarted.pool, recovery_receipt).await,
            "pending",
        );
        recover_browser_csrf(&restarted, &mut recovery_headers).await;
        let recovered = confirm_review_receipt(
            restarted.clone(),
            recovery_headers.clone(),
            harness.bundles[3].paper_project_id,
            recovery_receipt,
            &recovery_confirmation,
        )
        .await;
        assert_eq!(recovered.0, StatusCode::CREATED);
        assert_eq!(
            receipt_lifecycle(&restarted.pool, recovery_receipt).await,
            "consumed",
        );
        recovery_headers.insert(
            CSRF_HEADER,
            recovered
                .1
                .get(CSRF_HEADER)
                .expect("recovered confirmation rotated csrf")
                .clone(),
        );
        let immutable_replay = confirm_review_receipt(
            restarted.clone(),
            recovery_headers,
            harness.bundles[3].paper_project_id,
            recovery_receipt,
            &recovery_confirmation,
        )
        .await;
        assert_eq!(immutable_replay.0, recovered.0);
        assert_eq!(immutable_replay.2, recovered.2);
        assert_eq!(
            immutable_replay
                .1
                .get("x-paper-raid-idempotent-replay")
                .and_then(|value| value.to_str().ok()),
            Some("true"),
        );
        {
            let calls = harness.command_calls.lock().expect("recovery calls lock");
            let calls = &calls[recovery_call_start..];
            assert_eq!(calls.len(), 2, "consumed browser replay reached upstream");
            assert_eq!(calls[0].body, calls[1].body);
            assert!(calls.iter().all(|call| {
                call.idempotency_key == recovery_key.to_string()
                    && call.nonce == recovery_key.to_string()
            }));
        }

        // Separate browser sessions may race the same human confirmation.  The receipt row lock
        // admits exactly one upstream call; the loser observes the committed immutable response.
        let concurrent = review_request(
            &harness.bundles[4],
            binding_id,
            &agent_id,
            &harness.agent_key,
            1,
            950_000,
        );
        assert_eq!(
            submit_review_receipt(restarted.clone(), &harness.agent_key, concurrent.clone())
                .await
                .0,
            StatusCode::OK,
        );
        let concurrent_receipt = envelope_uuid(&concurrent, "receipt_id");
        let (_, concurrent_confirmation) = prepare_confirmation_for_test(
            &restarted,
            &harness.identity,
            &harness.bundles[4],
            &concurrent,
            &human_key,
        )
        .await;
        let concurrent_call_start = harness
            .command_calls
            .lock()
            .expect("concurrent call lock")
            .len();
        harness
            .command_replies
            .lock()
            .expect("concurrent reply lock")
            .push_back(ReviewCommandReply {
                status: StatusCode::CREATED,
                body: json!({"result": "concurrent_confirmation"}),
                delay_before_response: Duration::ZERO,
            });
        let concurrent_left_headers = browser_headers(&restarted, &harness.identity).await;
        let concurrent_right_headers = browser_headers(&restarted, &harness.identity).await;
        let (concurrent_left, concurrent_right) = tokio::join!(
            confirm_review_receipt(
                restarted.clone(),
                concurrent_left_headers,
                harness.bundles[4].paper_project_id,
                concurrent_receipt,
                &concurrent_confirmation,
            ),
            confirm_review_receipt(
                restarted.clone(),
                concurrent_right_headers,
                harness.bundles[4].paper_project_id,
                concurrent_receipt,
                &concurrent_confirmation,
            ),
        );
        assert_eq!(concurrent_left.0, StatusCode::CREATED);
        assert_eq!(concurrent_right.0, StatusCode::CREATED);
        assert_eq!(concurrent_left.2, concurrent_right.2);
        assert_eq!(
            [concurrent_left.1, concurrent_right.1]
                .iter()
                .filter(|headers| {
                    headers
                        .get("x-paper-raid-idempotent-replay")
                        .and_then(|value| value.to_str().ok())
                        == Some("true")
                })
                .count(),
            1,
        );
        assert_eq!(
            harness
                .command_calls
                .lock()
                .expect("concurrent calls lock")
                .len(),
            concurrent_call_start + 1,
        );

        // Exercise the other executable assignment without duplicating the evaluator matrix.
        // The real inbox must bind `reproduce` to the finalized evaluation child; the signed
        // receipt, typed output, signing frame, upstream route and durable response replay must
        // preserve that same child across an invalidated attempt and BFF restarts.
        let reproducer = harness
            .reproducer_bundles
            .first()
            .expect("reproducer PG fixture");
        let reproducer_projection = review_inbox_projection(
            restarted.clone(),
            &harness.agent_key,
            binding_id,
            &agent_id,
            reproducer.bundle.paper_project_id,
        )
        .await;
        let reproducer_task = &reproducer_projection["papers"][0]["review_tasks"]["items"][0];
        assert_eq!(reproducer_task["role"], json!("reproducer"));
        assert_eq!(reproducer_task["kind"], json!("reproduce"));
        assert_eq!(
            reproducer_task["evaluation_id"],
            json!(reproducer.evaluation_id)
        );
        assert_eq!(reproducer_task["attempt"], json!(1));
        assert_eq!(reproducer_task["state"], json!("pending"));
        assert_eq!(reproducer_task["bundle"]["slot"], json!("reproducer"));
        assert_eq!(
            reproducer_task["bundle"]["execution"]["kind"],
            json!("reproduce")
        );

        let reproduction_first = reproduction_review_request(
            &reproducer.bundle,
            reproducer.evaluation_id,
            binding_id,
            &agent_id,
            &harness.agent_key,
            1,
            960_000,
        );
        assert_eq!(
            reproduction_first["receipt"]["task_id"],
            reproducer_task["task_id"]
        );
        assert_eq!(
            reproduction_first["receipt"]["evaluation_id"],
            json!(reproducer.evaluation_id)
        );
        assert_eq!(reproduction_first["receipt"]["kind"], json!("reproduce"));
        assert_eq!(
            reproduction_first["receipt"]["candidate_passed"],
            Value::Null
        );
        let typed_reproduction: ReviewReproductionExecutionResultV1 =
            serde_json::from_value(reproduction_first["output"].clone())
                .expect("typed reproduction PG output");
        assert_eq!(
            typed_reproduction.observed_metrics_micros.get("accuracy"),
            Some(&960_000)
        );
        assert_eq!(
            typed_reproduction
                .statistical_evidence
                .get("accuracy")
                .map(|evidence| evidence.interval_overlap_bps),
            Some(9_750)
        );
        let reproduction_first_stored = submit_review_receipt(
            restarted.clone(),
            &harness.agent_key,
            reproduction_first.clone(),
        )
        .await;
        assert_eq!(reproduction_first_stored.0, StatusCode::OK);
        assert_eq!(
            submit_review_receipt(
                restarted.clone(),
                &harness.agent_key,
                reproduction_first.clone(),
            )
            .await,
            reproduction_first_stored,
            "reproduction receipt replay drifted",
        );
        let reproduction_first_receipt = envelope_uuid(&reproduction_first, "receipt_id");
        invalidate_for_test(
            &restarted,
            &harness.identity,
            reproducer.bundle.paper_project_id,
            reproduction_first_receipt,
        )
        .await;

        let reproduction_restarted = AppState::connect(harness.config.clone())
            .await
            .expect("reconnect BFF before reproduction attempt 2");
        assert_eq!(
            submit_review_receipt(
                reproduction_restarted.clone(),
                &harness.agent_key,
                reproduction_first.clone(),
            )
            .await,
            reproduction_first_stored,
            "invalidated reproduction attempt lost immutable replay after restart",
        );
        let retry_projection = review_inbox_projection(
            reproduction_restarted.clone(),
            &harness.agent_key,
            binding_id,
            &agent_id,
            reproducer.bundle.paper_project_id,
        )
        .await;
        let retry_task = &retry_projection["papers"][0]["review_tasks"]["items"][0];
        assert_eq!(retry_task["task_id"], reproducer_task["task_id"]);
        assert_eq!(retry_task["evaluation_id"], json!(reproducer.evaluation_id));
        assert_eq!(retry_task["attempt"], json!(2));
        assert_eq!(retry_task["state"], json!("pending"));

        let reproduction_second = reproduction_review_request(
            &reproducer.bundle,
            reproducer.evaluation_id,
            binding_id,
            &agent_id,
            &harness.agent_key,
            2,
            961_000,
        );
        assert_eq!(
            submit_review_receipt(
                reproduction_restarted.clone(),
                &harness.agent_key,
                reproduction_second.clone(),
            )
            .await
            .0,
            StatusCode::OK,
        );
        let reproduction_second_receipt = envelope_uuid(&reproduction_second, "receipt_id");
        let mut reproducer_headers =
            browser_headers(&reproduction_restarted, &harness.identity).await;
        let signing = request_review_signing_frame(
            reproduction_restarted.clone(),
            reproducer_headers.clone(),
            reproducer.bundle.paper_project_id,
            reproduction_second_receipt,
            ReviewReceiptSigningFrameRequest {
                coi_attestation_hash: sha256_digest(b"pg-reproducer-coi-v1"),
                score_components: None,
                observable_hard_gates: None,
            },
        )
        .await;
        assert_eq!(
            signing.0,
            StatusCode::OK,
            "reproduction signing frame failed: {}",
            String::from_utf8_lossy(&signing.2),
        );
        reproducer_headers.insert(
            CSRF_HEADER,
            signing
                .1
                .get(CSRF_HEADER)
                .expect("reproducer signing frame rotated csrf")
                .clone(),
        );
        let reproduction_frame: Value =
            serde_json::from_slice(&signing.2).expect("reproduction signing frame JSON");
        assert_eq!(reproduction_frame["command"], json!("submit_reproduction"));
        assert_eq!(
            reproduction_frame["child_id"],
            json!(reproducer.evaluation_id)
        );
        assert_eq!(
            reproduction_frame["payload"]["observed_metrics_micros"],
            reproduction_second["output"]["observed_metrics_micros"]
        );
        assert_eq!(
            reproduction_frame["payload"]["statistical_evidence"],
            reproduction_second["output"]["statistical_evidence"]
        );
        assert_eq!(
            reproduction_frame["receipt_context"]["evaluation_id"],
            json!(reproducer.evaluation_id)
        );
        let reproduction_confirmation =
            confirmation_request_for_frame(&reproduction_frame, &harness.human_key);
        let reproduction_call_start = harness
            .command_calls
            .lock()
            .expect("reproduction command calls lock")
            .len();
        harness
            .command_replies
            .lock()
            .expect("reproduction command reply lock")
            .push_back(ReviewCommandReply {
                status: StatusCode::CREATED,
                body: json!({"result": "reproduction_confirmed"}),
                delay_before_response: Duration::ZERO,
            });
        let reproduction_confirmed = confirm_review_receipt(
            reproduction_restarted.clone(),
            reproducer_headers.clone(),
            reproducer.bundle.paper_project_id,
            reproduction_second_receipt,
            &reproduction_confirmation,
        )
        .await;
        assert_eq!(reproduction_confirmed.0, StatusCode::CREATED);
        assert_eq!(
            receipt_lifecycle(&reproduction_restarted.pool, reproduction_second_receipt).await,
            "consumed"
        );
        {
            let calls = harness
                .command_calls
                .lock()
                .expect("reproduction command calls lock");
            let calls = &calls[reproduction_call_start..];
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].operation, "create_paper_reproduction_v1");
            assert_eq!(
                calls[0].path,
                format!(
                    "/v2/hepta/papers/{}/evaluations/{}/reproductions",
                    reproducer.bundle.paper_project_id, reproducer.evaluation_id
                )
            );
            let body: Value =
                serde_json::from_slice(&calls[0].body).expect("typed reproduction upstream body");
            assert_eq!(
                body["reproducer_player_id"],
                json!(harness.identity.player_id)
            );
            assert_eq!(
                body["observed_metrics_micros"],
                reproduction_second["output"]["observed_metrics_micros"]
            );
            assert_eq!(body["idempotency_key"], json!(calls[0].idempotency_key));
            assert_eq!(calls[0].nonce, calls[0].idempotency_key);
        }

        reproducer_headers.insert(
            CSRF_HEADER,
            reproduction_confirmed
                .1
                .get(CSRF_HEADER)
                .expect("reproduction confirmation rotated csrf")
                .clone(),
        );
        let reproduction_consumed_restart = AppState::connect(harness.config.clone())
            .await
            .expect("reconnect BFF after consumed reproduction");
        let reproduction_confirmed_replay = confirm_review_receipt(
            reproduction_consumed_restart.clone(),
            reproducer_headers,
            reproducer.bundle.paper_project_id,
            reproduction_second_receipt,
            &reproduction_confirmation,
        )
        .await;
        assert_eq!(reproduction_confirmed_replay.0, reproduction_confirmed.0);
        assert_eq!(reproduction_confirmed_replay.2, reproduction_confirmed.2);
        assert_eq!(
            reproduction_confirmed_replay
                .1
                .get("x-paper-raid-idempotent-replay")
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
        assert_eq!(
            harness
                .command_calls
                .lock()
                .expect("reproduction replay calls lock")
                .len(),
            reproduction_call_start + 1,
            "consumed reproduction replay reached upstream after restart",
        );
        let forbidden_reproduction_attempt = reproduction_review_request(
            &reproducer.bundle,
            reproducer.evaluation_id,
            binding_id,
            &agent_id,
            &harness.agent_key,
            3,
            962_000,
        );
        assert_eq!(
            submit_review_receipt(
                reproduction_consumed_restart.clone(),
                &harness.agent_key,
                forbidden_reproduction_attempt,
            )
            .await
            .0,
            StatusCode::CONFLICT,
        );

        let confirm_final_status =
            crate::db::agent_bridge_schema_status(&reproduction_consumed_restart.pool).await;
        assert!(confirm_final_status.schema_ready);
        assert!(confirm_final_status.integrity_ok);
    }
}
