use std::{collections::HashMap, time::SystemTime};

use axum::{
    body::{to_bytes, Bytes},
    extract::{Path, Request, State},
    http::{header::CONTENT_LENGTH, HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use tokio::sync::TryAcquireError;
use trnm_finality_types::{
    CometBftAppHashFinalityReceiptV2, MAX_COMETBFT_TRUST_ANCHOR_V1_WIRE_BYTES,
};
use trnm_finality_verifier::{
    verify_cometbft_apphash_finality_receipt_v2_with_trust_anchor, ReceiptV2VerificationOutcome,
    ValidatedCometBftTrustAnchorV1, VerifiedCometBftDomainCommandV2, VerifiedCometBftReceiptV2,
};
use uuid::Uuid;

use crate::{
    paper_raid_v2::{
        JointPaperSubmission, JointSubmissionStatus, PaperPhase, PaperProject,
        ResearchSessionAuthorizationSetStatus,
    },
    push_event, require_service_token, sha256_hex,
    workflows::TrnmProjectionStatus,
    ApiError, AppState, EventEnvelope, PaperEvaluation, PaperEvaluationStatus, PaperReproduction,
    OPERATOR_TOKEN_HEADER, TRNM_TOKEN_HEADER,
};

pub const PAPER_TRNM_COMMAND_BINDING_SCHEMA_V1: &str = "hepta.paper_raid.trnm_command_binding.v1";
pub const PAPER_TRNM_EVALUATION_EXTERNAL_KEY_NAMESPACE_V1: &str = "hepta.paper_raid.evaluation";
pub const PAPER_TRNM_SUBMISSION_BINDING_DOMAIN_V1: &[u8] =
    b"HEPTA_PAPER_TRNM_SUBMISSION_BINDING_V1\0";
pub const PAPER_TRNM_MATCH_EVIDENCE_OBJECT_VERSION_V1: u64 = 1;
pub const PAPER_CHAIN_FINALITY_PROJECTION_SCHEMA_V1: &str =
    "hepta.paper_raid.chain_finality_projection.v1";
pub const PAPER_CHAIN_FINALITY_HISTORY_SCHEMA_V1: &str =
    "hepta.paper_raid.chain_finality_history.v1";
pub const PAPER_CHAIN_TRUST_ANCHOR_SCHEMA_V1: &str = "hepta.paper_raid.cometbft_trust_anchor.v1";
pub const TRNM_TRUST_ANCHOR_HASH_HEADER: &str = "x-hepta-trnm-trust-anchor-hash";

const PAPER_CHAIN_VERIFIED_EVENT_V1: &str = "hepta.paper_raid.chain_finality.verified.v1";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperTrnmCommandBindingV1 {
    pub schema: String,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub evaluation_id: Uuid,
    pub research_session_id: String,
    pub research_session_roster_version: u64,
    pub match_evidence_commitment_id: String,
    pub match_evidence_object_version: u64,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub submission_commitment_hash: String,
    pub tolerance_policy_hash: String,
    pub evaluation_signing_hash: String,
    pub reproduction_id: Uuid,
    pub reproduction_report_hash: String,
    pub evaluation_score_bps: u16,
    pub evaluation_accepted: bool,
    pub evaluation_completed_at_unix_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperChainFinalityHistoryV1 {
    pub schema: String,
    pub paper_project_id: Uuid,
    pub projections: Vec<PaperChainFinalityProjectionV1>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaperChainFinalityStatusV1 {
    PendingFinality,
    VerifiedFinality,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperChainFinalityProjectionV1 {
    pub schema: String,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub evaluation_id: Uuid,
    pub local_command_id: Uuid,
    pub command_idempotency_key: String,
    pub command_fingerprint: String,
    pub paper_binding_fingerprint: String,
    pub receipt_hash: String,
    pub trust_anchor_hash: String,
    pub chain_id: String,
    pub comet_tx_hash: String,
    pub transaction_index: u64,
    pub execution_height: u64,
    pub commitment_height: u64,
    pub commitment_header_hash: String,
    pub app_hash: String,
    pub status: PaperChainFinalityStatusV1,
    pub ranking_eligible: bool,
    pub reward_eligible: bool,
    pub score_eligible: bool,
    pub economic_eligible: bool,
    pub version: u64,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredPaperChainTrustAnchorV1 {
    pub schema: String,
    pub anchor_hash: String,
    pub canonical_sha256: String,
    pub chain_id: String,
    pub trusted_height: u64,
    pub admitted_at: DateTime<Utc>,
}

#[derive(Clone)]
struct MemoryTrustAnchor {
    record: StoredPaperChainTrustAnchorV1,
    canonical: Bytes,
}

#[derive(Clone)]
struct MemoryReceipt {
    canonical_sha256: String,
    trust_anchor_hash: String,
    canonical: Bytes,
    projection: PaperChainFinalityProjectionV1,
}

#[derive(Clone, Default)]
pub(crate) struct PaperChainFinalityMemory {
    trust_anchors: HashMap<String, MemoryTrustAnchor>,
    receipts: HashMap<String, MemoryReceipt>,
    projections: HashMap<Uuid, PaperChainFinalityProjectionV1>,
    inbox: HashMap<String, String>,
    pub(crate) preparations_v2:
        crate::paper_chain_finality_v2::PaperChainFinalityPreparationMemoryV2,
}

pub(crate) fn validated_trust_anchor_memory_v1(
    memory: &PaperChainFinalityMemory,
    anchor_hash: &str,
    pinned_anchor_hashes: &std::collections::HashSet<String>,
) -> Result<ValidatedCometBftTrustAnchorV1, ApiError> {
    if !pinned_anchor_hashes.contains(anchor_hash) {
        return Err(ApiError::forbidden(
            "trnm_trust_anchor_not_pinned",
            "stored trust anchor is absent from immutable server configuration",
        ));
    }
    let stored = memory.trust_anchors.get(anchor_hash).ok_or_else(|| {
        ApiError::not_found(
            "trnm_trust_anchor_not_admitted",
            "selected CometBFT trust anchor has not been admitted",
        )
    })?;
    let anchor = ValidatedCometBftTrustAnchorV1::from_canonical_bytes(&stored.canonical).map_err(
        |error| {
            ApiError::internal(format!(
                "stored canonical CometBFT trust anchor is invalid: {error:#}"
            ))
        },
    )?;
    if anchor.wire().anchor_hash_hex != anchor_hash {
        return Err(ApiError::internal(
            "stored CometBFT trust anchor bytes do not match their indexed hash",
        ));
    }
    Ok(anchor)
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/v2/hepta/operator/trnm/trust-anchors",
            post(ingest_trust_anchor),
        )
        .route(
            "/v2/hepta/papers/:paper_id/chain-finality",
            post(ingest_paper_chain_finality).get(get_paper_chain_finality),
        )
}

pub(crate) fn paper_binding_fingerprint(
    binding: &PaperTrnmCommandBindingV1,
) -> Result<String, ApiError> {
    validate_paper_binding_shape(binding)?;
    let canonical = serde_json::to_vec(binding)
        .map_err(|error| ApiError::internal(format!("encode Paper TRNM binding: {error}")))?;
    Ok(format!("sha256:{}", sha256_hex(&canonical)))
}

fn validate_paper_binding_shape(binding: &PaperTrnmCommandBindingV1) -> Result<(), ApiError> {
    if binding.schema != PAPER_TRNM_COMMAND_BINDING_SCHEMA_V1 {
        return Err(ApiError::bad_request(
            "unsupported_paper_trnm_binding_schema",
            format!("expected {PAPER_TRNM_COMMAND_BINDING_SCHEMA_V1}"),
        ));
    }
    for (field, value) in [
        ("release_candidate_hash", &binding.release_candidate_hash),
        ("paper_bundle_hash", &binding.paper_bundle_hash),
        (
            "submission_commitment_hash",
            &binding.submission_commitment_hash,
        ),
        ("tolerance_policy_hash", &binding.tolerance_policy_hash),
        ("evaluation_signing_hash", &binding.evaluation_signing_hash),
        (
            "reproduction_report_hash",
            &binding.reproduction_report_hash,
        ),
        (
            "match_evidence_commitment_id",
            &binding.match_evidence_commitment_id,
        ),
    ] {
        crate::validate_hash(field, value)?;
    }
    crate::paper_raid_v2::validate_logical_session_id(&binding.research_session_id)?;
    if binding.research_session_roster_version == 0 {
        return Err(ApiError::bad_request(
            "paper_trnm_binding_roster_version_invalid",
            "research_session_roster_version must be positive",
        ));
    }
    if binding.match_evidence_object_version != PAPER_TRNM_MATCH_EVIDENCE_OBJECT_VERSION_V1 {
        return Err(ApiError::bad_request(
            "paper_trnm_match_evidence_version_unsupported",
            format!(
                "match_evidence_object_version must be {PAPER_TRNM_MATCH_EVIDENCE_OBJECT_VERSION_V1}"
            ),
        ));
    }
    if binding.evaluation_score_bps > 10_000 {
        return Err(ApiError::bad_request(
            "paper_trnm_binding_score_out_of_range",
            "evaluation_score_bps must be at most 10000",
        ));
    }
    let expected_submission = paper_trnm_submission_commitment_hash(binding)?;
    if binding.submission_commitment_hash != expected_submission {
        return Err(ApiError::bad_request(
            "paper_trnm_submission_commitment_mismatch",
            "submission_commitment_hash does not bind the canonical Paper/submission/hash tuple",
        ));
    }
    Ok(())
}

pub(crate) fn paper_trnm_submission_commitment_hash(
    binding: &PaperTrnmCommandBindingV1,
) -> Result<String, ApiError> {
    let release_candidate = crate::decode_digest(&binding.release_candidate_hash)
        .map_err(|message| ApiError::bad_request("invalid_release_candidate_hash", message))?;
    let paper_bundle = crate::decode_digest(&binding.paper_bundle_hash)
        .map_err(|message| ApiError::bad_request("invalid_paper_bundle_hash", message))?;
    let mut bytes = Vec::with_capacity(128);
    bytes.extend_from_slice(PAPER_TRNM_SUBMISSION_BINDING_DOMAIN_V1);
    bytes.extend_from_slice(binding.paper_project_id.as_bytes());
    bytes.extend_from_slice(binding.submission_id.as_bytes());
    bytes.extend_from_slice(&release_candidate);
    bytes.extend_from_slice(&paper_bundle);
    Ok(format!("sha256:{}", sha256_hex(&bytes)))
}

pub fn paper_trnm_evaluation_external_key(
    evaluation_id: Uuid,
) -> Result<crate::trnm_v1::ExternalKey, String> {
    crate::trnm_v1::ExternalKey::from_uuid(
        PAPER_TRNM_EVALUATION_EXTERNAL_KEY_NAMESPACE_V1,
        &evaluation_id.to_string(),
    )
    .map_err(|error| format!("derive Paper evaluation external key: {error}"))
}

pub(crate) fn paper_trnm_match_evidence_object_ref(
    binding: &PaperTrnmCommandBindingV1,
) -> Result<crate::trnm_v1::ObjectRefV1, ApiError> {
    let commitment_id =
        crate::decode_digest(&binding.match_evidence_commitment_id).map_err(|message| {
            ApiError::bad_request("invalid_match_evidence_commitment_id", message)
        })?;
    Ok(crate::trnm_v1::ObjectRefV1::new(
        crate::trnm_v1::ResearchObjectKind::MatchEvidence,
        crate::trnm_v1::ExternalKey::from_bytes(commitment_id),
        binding.match_evidence_object_version,
    ))
}

pub(crate) fn validate_signed_paper_binding(
    signed_command: &crate::trnm_v1::SignedResearchCommandV1,
    binding: &PaperTrnmCommandBindingV1,
) -> Result<(), ApiError> {
    validate_paper_binding_shape(binding)?;
    let crate::trnm_v1::ResearchCommandV1::EvaluationCommitment(payload) = &signed_command.command
    else {
        return Err(ApiError::conflict(
            "paper_trnm_binding_command_kind_unsupported",
            "the first Paper Receipt V2 slice requires a signed EvaluationCommitmentV1",
        ));
    };
    let expected_evaluation_key =
        paper_trnm_evaluation_external_key(binding.evaluation_id).map_err(ApiError::internal)?;
    if payload.evaluation_id != expected_evaluation_key {
        return Err(ApiError::conflict(
            "paper_trnm_evaluation_key_mismatch",
            "signed EvaluationCommitment evaluation_id does not bind the Paper evaluation UUID",
        ));
    }
    if payload.match_evidence_ref != paper_trnm_match_evidence_object_ref(binding)? {
        return Err(ApiError::conflict(
            "paper_trnm_match_evidence_ref_mismatch",
            "signed EvaluationCommitment match_evidence_ref does not bind the exact completed Paper/Nakama session commitment and object version",
        ));
    }
    let submission_hash = crate::decode_digest(&binding.submission_commitment_hash)
        .map_err(|message| ApiError::bad_request("invalid_submission_commitment_hash", message))?;
    let rubric_hash = crate::decode_digest(&binding.tolerance_policy_hash)
        .map_err(|message| ApiError::bad_request("invalid_tolerance_policy_hash", message))?;
    let evaluation_hash = crate::decode_digest(&binding.evaluation_signing_hash)
        .map_err(|message| ApiError::bad_request("invalid_evaluation_signing_hash", message))?;
    let reproduction_hash = crate::decode_digest(&binding.reproduction_report_hash)
        .map_err(|message| ApiError::bad_request("invalid_reproduction_report_hash", message))?;
    if payload.submission_hash != submission_hash
        || payload.rubric_hash != rubric_hash
        || payload.evaluation_hash != evaluation_hash
        || payload.reproduction_hash != Some(reproduction_hash)
        || payload.score_bps != binding.evaluation_score_bps
        || payload.accepted != binding.evaluation_accepted
        || payload.completed_at_unix_s != binding.evaluation_completed_at_unix_s
    {
        return Err(ApiError::conflict(
            "paper_trnm_signed_semantics_mismatch",
            "signed EvaluationCommitment does not match the full immutable Paper binding tuple",
        ));
    }
    Ok(())
}

pub(crate) async fn validate_paper_binding_for_queue(
    state: &AppState,
    binding: &PaperTrnmCommandBindingV1,
) -> Result<(), ApiError> {
    validate_paper_binding_shape(binding)?;
    if let Some(pool) = &state.pool {
        let mut tx = pool.begin().await.map_err(ApiError::database)?;
        validate_paper_binding_postgres(&mut tx, binding, false, &state.security).await?;
        tx.commit().await.map_err(ApiError::database)
    } else {
        let memory = state.paper_raid.read().await;
        validate_paper_binding_memory(&memory, binding, &state.security)
    }
}

async fn ingest_trust_anchor(
    State(state): State<AppState>,
    request: Request,
) -> Result<(StatusCode, Json<StoredPaperChainTrustAnchorV1>), ApiError> {
    // Authentication deliberately precedes Content-Length inspection and body
    // polling so an unauthenticated peer cannot make the service buffer bytes.
    require_service_token(
        request.headers(),
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    let canonical = read_limited_body(request, MAX_COMETBFT_TRUST_ANCHOR_V1_WIRE_BYTES).await?;
    let validated =
        ValidatedCometBftTrustAnchorV1::from_canonical_bytes(&canonical).map_err(|error| {
            ApiError::bad_request(
                "invalid_canonical_trnm_trust_anchor",
                format!("canonical CometBFT trust anchor rejected: {error:#}"),
            )
        })?;
    let wire = validated.wire();
    if !state
        .security
        .pinned_trnm_cometbft_trust_anchor_hashes
        .contains(&wire.anchor_hash_hex)
    {
        return Err(ApiError::forbidden(
            "trnm_trust_anchor_not_pinned",
            "operator upload cannot expand trust; anchor hash is absent from immutable server configuration",
        ));
    }
    let admitted_at = DateTime::<Utc>::from((state.cometbft_local_verification_clock)());
    let record = StoredPaperChainTrustAnchorV1 {
        schema: PAPER_CHAIN_TRUST_ANCHOR_SCHEMA_V1.to_string(),
        anchor_hash: wire.anchor_hash_hex.clone(),
        canonical_sha256: format!("sha256:{}", sha256_hex(&canonical)),
        chain_id: wire.trusted_header.chain_id.clone(),
        trusted_height: wire.trusted_header.height,
        admitted_at,
    };
    if state.pool.is_some() {
        ingest_trust_anchor_postgres(&state, canonical, record).await
    } else {
        let mut memory = state.paper_chain_finality.write().await;
        if let Some(existing) = memory.trust_anchors.get(&record.anchor_hash) {
            if existing.canonical == canonical
                && existing.record.canonical_sha256 == record.canonical_sha256
                && existing.record.chain_id == record.chain_id
                && existing.record.trusted_height == record.trusted_height
            {
                return Ok((StatusCode::OK, Json(existing.record.clone())));
            }
            return Err(ApiError::conflict(
                "trnm_trust_anchor_conflict",
                "anchor hash is already stored with different canonical bytes or metadata",
            ));
        }
        memory.trust_anchors.insert(
            record.anchor_hash.clone(),
            MemoryTrustAnchor {
                record: record.clone(),
                canonical,
            },
        );
        Ok((StatusCode::CREATED, Json(record)))
    }
}

async fn ingest_trust_anchor_postgres(
    state: &AppState,
    canonical: Bytes,
    record: StoredPaperChainTrustAnchorV1,
) -> Result<(StatusCode, Json<StoredPaperChainTrustAnchorV1>), ApiError> {
    let pool = state.pool.as_ref().expect("PostgreSQL checked");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    let existing = sqlx::query(
        "select chain_id, trusted_height, canonical_anchor, canonical_sha256, admitted_at
         from hepta_trnm_cometbft_trust_anchors where anchor_hash=$1 for update",
    )
    .bind(&record.anchor_hash)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if let Some(existing) = existing {
        let existing_record = StoredPaperChainTrustAnchorV1 {
            schema: PAPER_CHAIN_TRUST_ANCHOR_SCHEMA_V1.to_string(),
            anchor_hash: record.anchor_hash.clone(),
            canonical_sha256: existing.get("canonical_sha256"),
            chain_id: existing.get("chain_id"),
            trusted_height: u64_from_i64(existing.get("trusted_height"), "trusted_height")?,
            admitted_at: existing.get("admitted_at"),
        };
        let existing_canonical: Vec<u8> = existing.get("canonical_anchor");
        if existing_canonical.as_slice() == canonical.as_ref()
            && existing_record.canonical_sha256 == record.canonical_sha256
            && existing_record.chain_id == record.chain_id
            && existing_record.trusted_height == record.trusted_height
        {
            tx.commit().await.map_err(ApiError::database)?;
            return Ok((StatusCode::OK, Json(existing_record)));
        }
        return Err(ApiError::conflict(
            "trnm_trust_anchor_conflict",
            "anchor hash is already stored with different canonical bytes or metadata",
        ));
    }
    sqlx::query(
        "insert into hepta_trnm_cometbft_trust_anchors (
            anchor_hash,chain_id,trusted_height,canonical_anchor,canonical_sha256,admitted_at
         ) values ($1,$2,$3,$4,$5,$6)",
    )
    .bind(&record.anchor_hash)
    .bind(&record.chain_id)
    .bind(i64_from_u64(record.trusted_height, "trusted_height")?)
    .bind(&canonical[..])
    .bind(&record.canonical_sha256)
    .bind(record.admitted_at)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn ingest_paper_chain_finality(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    request: Request,
) -> Result<(StatusCode, Json<PaperChainFinalityProjectionV1>), ApiError> {
    // Keep this order invariant: authenticate, validate trusted-anchor selector,
    // acquire bounded verification capacity, enforce the deployment byte limit
    // while streaming, and only then parse canonical JSON.
    require_service_token(
        request.headers(),
        TRNM_TOKEN_HEADER,
        &state.security.trnm_token,
        "trnm_auth_failed",
    )?;
    require_receipt_v2_enabled(&state)?;
    let trust_anchor_hash = trust_anchor_hash_header(request.headers())?;
    let _verification_permit = match state
        .paper_chain_verification_permits
        .clone()
        .try_acquire_owned()
    {
        Ok(permit) => permit,
        Err(TryAcquireError::NoPermits) => {
            return Err(ApiError::service_unavailable(
                "trnm_receipt_v2_verification_busy",
                "Receipt V2 verification capacity is busy; retry without changing the request",
            ));
        }
        Err(TryAcquireError::Closed) => {
            return Err(ApiError::internal(
                "Receipt V2 verification capacity is unavailable",
            ));
        }
    };
    let canonical =
        read_limited_body(request, state.security.trnm_receipt_v2_max_body_bytes).await?;
    let receipt =
        CometBftAppHashFinalityReceiptV2::from_canonical_bytes(&canonical).map_err(|error| {
            ApiError::bad_request(
                "invalid_canonical_trnm_receipt_v2",
                format!("canonical Receipt V2 rejected: {error:#}"),
            )
        })?;
    let canonical_sha256 = format!("sha256:{}", sha256_hex(&canonical));
    let verification_time = (state.cometbft_local_verification_clock)();
    if state.pool.is_some() {
        ingest_paper_chain_finality_postgres(
            &state,
            paper_id,
            trust_anchor_hash,
            canonical,
            canonical_sha256,
            receipt,
            verification_time,
        )
        .await
    } else {
        ingest_paper_chain_finality_memory(
            &state,
            paper_id,
            trust_anchor_hash,
            canonical,
            canonical_sha256,
            receipt,
            verification_time,
        )
        .await
    }
}

async fn get_paper_chain_finality(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<PaperChainFinalityProjectionV1>, ApiError> {
    require_service_token(
        &headers,
        TRNM_TOKEN_HEADER,
        &state.security.trnm_token,
        "trnm_auth_failed",
    )?;
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            "select record_json from hepta_paper_chain_finality_projections
             where paper_project_id=$1",
        )
        .bind(paper_id)
        .fetch_optional(pool)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| {
            ApiError::not_found(
                "paper_chain_finality_not_found",
                "Paper remains pending finality",
            )
        })?;
        Ok(Json(decode_record(
            row.get("record_json"),
            "Paper Chain finality projection",
        )?))
    } else {
        state
            .paper_chain_finality
            .read()
            .await
            .projections
            .get(&paper_id)
            .cloned()
            .map(Json)
            .ok_or_else(|| {
                ApiError::not_found(
                    "paper_chain_finality_not_found",
                    "Paper remains pending finality",
                )
            })
    }
}

#[allow(clippy::too_many_arguments)]
async fn ingest_paper_chain_finality_memory(
    state: &AppState,
    paper_id: Uuid,
    trust_anchor_hash: String,
    canonical: Bytes,
    canonical_sha256: String,
    receipt: CometBftAppHashFinalityReceiptV2,
    verification_time: SystemTime,
) -> Result<(StatusCode, Json<PaperChainFinalityProjectionV1>), ApiError> {
    let anchor_canonical = {
        let memory = state.paper_chain_finality.read().await;
        memory
            .trust_anchors
            .get(&trust_anchor_hash)
            .map(|anchor| anchor.canonical.clone())
            .ok_or_else(|| {
                ApiError::forbidden(
                    "trnm_trust_anchor_not_admitted",
                    "selected trust anchor is not present in authenticated local storage",
                )
            })?
    };
    let verified = verify_receipt(
        &receipt,
        &anchor_canonical,
        &trust_anchor_hash,
        &state.security.pinned_trnm_cometbft_trust_anchor_hashes,
        verification_time,
    )?;
    ingest_verified_paper_chain_finality_memory(
        state,
        paper_id,
        trust_anchor_hash,
        canonical,
        canonical_sha256,
        verified,
        DateTime::<Utc>::from(verification_time),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn ingest_verified_paper_chain_finality_memory(
    state: &AppState,
    paper_id: Uuid,
    trust_anchor_hash: String,
    canonical: Bytes,
    canonical_sha256: String,
    verified: VerifiedCometBftReceiptV2,
    verified_at: DateTime<Utc>,
) -> Result<(StatusCode, Json<PaperChainFinalityProjectionV1>), ApiError> {
    let verified_domain_command = require_legacy_research_domain(&verified)?;
    // Paper writes take this same lock. Holding it across the command and new
    // projection updates prevents an Appeal/integrity transition from racing
    // the finality decision in the in-memory test backend.
    let paper_memory = state.paper_raid.write().await;
    let mut league = state.inner.write().await;
    let mut finality = state.paper_chain_finality.write().await;
    let command = league
        .trnm_commands
        .values_mut()
        .find(|command| command.idempotency_key == verified.command_id)
        .ok_or_else(|| {
            ApiError::not_found(
                "queued_trnm_command_not_found",
                "Receipt domain command_id does not match a queued Hepta command",
            )
        })?;
    validate_verified_domain_command(command, verified_domain_command)?;
    if let Some(replay) = receipt_replay_memory(
        &finality,
        paper_id,
        &verified.receipt_hash_hex,
        &trust_anchor_hash,
        &canonical,
        &canonical_sha256,
    )? {
        return Ok((StatusCode::OK, Json(replay)));
    }
    if finality.projections.contains_key(&paper_id) {
        return Err(ApiError::conflict(
            "paper_chain_finality_conflict",
            "Paper already has a different immutable Chain finality projection",
        ));
    }
    validate_verified_command_binding(
        command,
        paper_id,
        &verified,
        &paper_memory,
        &state.security,
    )?;
    let projection = build_projection(
        paper_id,
        command,
        &verified,
        &trust_anchor_hash,
        verified_at,
    )?;
    command.status = TrnmProjectionStatus::VerifiedFinality;
    push_event(
        &mut league,
        PAPER_CHAIN_VERIFIED_EVENT_V1,
        paper_id.to_string(),
        projection_event_payload(&projection),
    );
    finality
        .inbox
        .insert(verified.receipt_hash_hex.clone(), canonical_sha256.clone());
    finality.receipts.insert(
        verified.receipt_hash_hex.clone(),
        MemoryReceipt {
            canonical_sha256,
            trust_anchor_hash,
            canonical,
            projection: projection.clone(),
        },
    );
    finality.projections.insert(paper_id, projection.clone());
    Ok((StatusCode::CREATED, Json(projection)))
}

fn receipt_replay_memory(
    memory: &PaperChainFinalityMemory,
    paper_id: Uuid,
    receipt_hash: &str,
    trust_anchor_hash: &str,
    canonical: &[u8],
    canonical_sha256: &str,
) -> Result<Option<PaperChainFinalityProjectionV1>, ApiError> {
    if let Some(existing) = memory.receipts.get(receipt_hash) {
        if existing.projection.paper_project_id == paper_id
            && existing.trust_anchor_hash == trust_anchor_hash
            && existing.canonical_sha256 == canonical_sha256
            && existing.canonical.as_ref() == canonical
            && memory.inbox.get(receipt_hash) == Some(&existing.canonical_sha256)
        {
            return Ok(Some(existing.projection.clone()));
        }
        return Err(ApiError::conflict(
            "trnm_receipt_v2_replay_conflict",
            "receipt hash was reused with different Paper, anchor, or canonical bytes",
        ));
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
async fn ingest_paper_chain_finality_postgres(
    state: &AppState,
    paper_id: Uuid,
    trust_anchor_hash: String,
    canonical: Bytes,
    canonical_sha256: String,
    receipt: CometBftAppHashFinalityReceiptV2,
    verification_time: SystemTime,
) -> Result<(StatusCode, Json<PaperChainFinalityProjectionV1>), ApiError> {
    let pool = state.pool.as_ref().expect("PostgreSQL checked");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    sqlx::query(
        "select pg_advisory_xact_lock(hashtext('hepta-paper-chain-finality-v1'), hashtext($1))",
    )
    .bind(paper_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let anchor_row = sqlx::query(
        "select canonical_anchor from hepta_trnm_cometbft_trust_anchors
         where anchor_hash=$1 for share",
    )
    .bind(&trust_anchor_hash)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::forbidden(
            "trnm_trust_anchor_not_admitted",
            "selected trust anchor is not present in authenticated local storage",
        )
    })?;
    let anchor_canonical: Vec<u8> = anchor_row.get("canonical_anchor");
    let verified = verify_receipt(
        &receipt,
        &anchor_canonical,
        &trust_anchor_hash,
        &state.security.pinned_trnm_cometbft_trust_anchor_hashes,
        verification_time,
    )?;
    let result = ingest_verified_paper_chain_finality_postgres(
        state,
        &mut tx,
        paper_id,
        &trust_anchor_hash,
        canonical.as_ref(),
        &canonical_sha256,
        &verified,
        DateTime::<Utc>::from(verification_time),
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
async fn ingest_verified_paper_chain_finality_postgres(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    trust_anchor_hash: &str,
    canonical: &[u8],
    canonical_sha256: &str,
    verified: &VerifiedCometBftReceiptV2,
    verified_at: DateTime<Utc>,
) -> Result<(StatusCode, Json<PaperChainFinalityProjectionV1>), ApiError> {
    let verified_domain_command = require_legacy_research_domain(verified)?;

    let state_row = sqlx::query(
        "select revision,state_json from hepta_league_state
         where state_key='primary' for update",
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    let revision: i64 = state_row.get("revision");
    let mut league: crate::LeagueState = decode_record(state_row.get("state_json"), "Hepta state")?;
    let command = league
        .trnm_commands
        .values_mut()
        .find(|command| command.idempotency_key == verified.command_id)
        .ok_or_else(|| {
            ApiError::not_found(
                "queued_trnm_command_not_found",
                "Receipt domain command_id does not match a queued Hepta command",
            )
        })?;
    validate_verified_domain_command(command, verified_domain_command)?;
    if let Some(replay) = receipt_replay_postgres(
        tx,
        paper_id,
        &verified.receipt_hash_hex,
        trust_anchor_hash,
        canonical,
        canonical_sha256,
    )
    .await?
    {
        return Ok((StatusCode::OK, Json(replay)));
    }
    if sqlx::query_scalar::<_, bool>(
        "select exists(select 1 from hepta_paper_chain_finality_projections
         where paper_project_id=$1)",
    )
    .bind(paper_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?
    {
        return Err(ApiError::conflict(
            "paper_chain_finality_conflict",
            "Paper already has a different immutable Chain finality projection",
        ));
    }
    let binding = validate_command_identity(command, paper_id, verified)?.clone();
    validate_paper_binding_postgres(tx, &binding, true, &state.security).await?;
    let projection = build_projection(paper_id, command, verified, trust_anchor_hash, verified_at)?;
    command.status = TrnmProjectionStatus::VerifiedFinality;
    push_event(
        &mut league,
        PAPER_CHAIN_VERIFIED_EVENT_V1,
        paper_id.to_string(),
        projection_event_payload(&projection),
    );
    let event = league
        .events
        .last()
        .cloned()
        .ok_or_else(|| ApiError::internal("Paper Chain finality event was not created"))?;
    let state_json = serde_json::to_value(&league)
        .map_err(|error| ApiError::internal(format!("encode Hepta state: {error}")))?;
    sqlx::query(
        "update hepta_league_state set revision=$1,state_json=$2::jsonb,updated_at=now()
         where state_key='primary' and revision=$3",
    )
    .bind(revision + 1)
    .bind(state_json)
    .bind(revision)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;

    let projection_json = serde_json::to_value(&projection).map_err(|error| {
        ApiError::internal(format!("encode Paper Chain finality projection: {error}"))
    })?;
    sqlx::query(
        "insert into hepta_paper_chain_finality_inbox (
            receipt_hash,canonical_sha256,anchor_hash,paper_project_id,received_at
         ) values ($1,$2,$3,$4,$5)",
    )
    .bind(&projection.receipt_hash)
    .bind(canonical_sha256)
    .bind(trust_anchor_hash)
    .bind(paper_id)
    .bind(projection.verified_at)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    sqlx::query(
        "insert into hepta_paper_chain_receipts (
            receipt_hash,paper_project_id,local_command_id,command_idempotency_key,
            command_fingerprint,paper_binding_fingerprint,anchor_hash,chain_id,
            execution_height,commitment_height,comet_tx_hash,app_hash,
            canonical_receipt,canonical_sha256,verified_at,record_json
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16::jsonb)",
    )
    .bind(&projection.receipt_hash)
    .bind(paper_id)
    .bind(projection.local_command_id)
    .bind(&projection.command_idempotency_key)
    .bind(&projection.command_fingerprint)
    .bind(&projection.paper_binding_fingerprint)
    .bind(trust_anchor_hash)
    .bind(&projection.chain_id)
    .bind(i64_from_u64(
        projection.execution_height,
        "execution_height",
    )?)
    .bind(i64_from_u64(
        projection.commitment_height,
        "commitment_height",
    )?)
    .bind(&projection.comet_tx_hash)
    .bind(&projection.app_hash)
    .bind(canonical)
    .bind(canonical_sha256)
    .bind(projection.verified_at)
    .bind(&projection_json)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    insert_finality_projection_postgres(tx, &projection, &projection_json).await?;
    insert_outbox_event(tx, &event).await?;
    Ok((StatusCode::CREATED, Json(projection)))
}

async fn insert_finality_projection_postgres(
    tx: &mut Transaction<'_, Postgres>,
    projection: &PaperChainFinalityProjectionV1,
    projection_json: &Value,
) -> Result<(), ApiError> {
    sqlx::query(
        "insert into hepta_paper_chain_finality_projections (
            paper_project_id,evaluation_id,local_command_id,receipt_hash,
            status,version,record_json,updated_at
         ) values ($1,$2,$3,$4,'verified_finality',1,$5::jsonb,$6)",
    )
    .bind(projection.paper_project_id)
    .bind(projection.evaluation_id)
    .bind(projection.local_command_id)
    .bind(&projection.receipt_hash)
    .bind(projection_json)
    .bind(projection.verified_at)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    Ok(())
}

async fn receipt_replay_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    receipt_hash: &str,
    trust_anchor_hash: &str,
    canonical: &[u8],
    canonical_sha256: &str,
) -> Result<Option<PaperChainFinalityProjectionV1>, ApiError> {
    let row = sqlx::query(
        "select r.paper_project_id,r.anchor_hash,r.canonical_receipt,r.canonical_sha256,
                p.record_json,i.canonical_sha256 as inbox_sha256
         from hepta_paper_chain_receipts r
         join hepta_paper_chain_finality_projections p on p.receipt_hash=r.receipt_hash
         join hepta_paper_chain_finality_inbox i on i.receipt_hash=r.receipt_hash
         where r.receipt_hash=$1 for update of r,p,i",
    )
    .bind(receipt_hash)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored_paper: Uuid = row.get("paper_project_id");
    let stored_anchor: String = row.get("anchor_hash");
    let stored_canonical: Vec<u8> = row.get("canonical_receipt");
    let stored_sha: String = row.get("canonical_sha256");
    let inbox_sha: String = row.get("inbox_sha256");
    if stored_paper != paper_id
        || stored_anchor != trust_anchor_hash
        || stored_canonical != canonical
        || stored_sha != canonical_sha256
        || inbox_sha != canonical_sha256
    {
        return Err(ApiError::conflict(
            "trnm_receipt_v2_replay_conflict",
            "receipt hash was reused with different Paper, anchor, or canonical bytes",
        ));
    }
    Ok(Some(decode_record(
        row.get("record_json"),
        "Paper Chain finality projection",
    )?))
}

fn verify_receipt(
    receipt: &CometBftAppHashFinalityReceiptV2,
    anchor_canonical: &[u8],
    trust_anchor_hash: &str,
    pinned_anchor_hashes: &std::collections::HashSet<String>,
    verification_time: SystemTime,
) -> Result<VerifiedCometBftReceiptV2, ApiError> {
    if !pinned_anchor_hashes.contains(trust_anchor_hash) {
        return Err(ApiError::forbidden(
            "trnm_trust_anchor_not_pinned",
            "stored trust anchor is absent from immutable server configuration",
        ));
    }
    let anchor = ValidatedCometBftTrustAnchorV1::from_canonical_bytes(anchor_canonical).map_err(
        |error| {
            ApiError::internal(format!(
                "stored canonical CometBFT trust anchor is invalid: {error:#}"
            ))
        },
    )?;
    if anchor.wire().anchor_hash_hex != trust_anchor_hash {
        return Err(ApiError::internal(
            "stored CometBFT trust anchor bytes do not match their indexed hash",
        ));
    }
    match verify_cometbft_apphash_finality_receipt_v2_with_trust_anchor(
        receipt,
        &anchor,
        verification_time,
    ) {
        ReceiptV2VerificationOutcome::Final(verified) => Ok(verified),
        ReceiptV2VerificationOutcome::StructuralInvalid { reason } => Err(ApiError::bad_request(
            "trnm_receipt_v2_structural_invalid",
            format!("Receipt V2 structural verification failed: {reason}"),
        )),
        ReceiptV2VerificationOutcome::Untrusted { reason } => Err(ApiError::forbidden(
            "trnm_receipt_v2_untrusted",
            format!("Receipt V2 trust verification failed: {reason}"),
        )),
        ReceiptV2VerificationOutcome::NotFinal { reason } => Err(ApiError::conflict(
            "trnm_receipt_v2_not_final",
            format!("Receipt V2 is not final: {reason}"),
        )),
    }
}

fn require_legacy_research_domain(
    verified: &VerifiedCometBftReceiptV2,
) -> Result<&crate::trnm_v1::SignedResearchCommandV1, ApiError> {
    match &verified.domain_command {
        VerifiedCometBftDomainCommandV2::ResearchV1(command) => Ok(command.as_ref()),
        VerifiedCometBftDomainCommandV2::PaperRaidFinalityV2(_)
        | VerifiedCometBftDomainCommandV2::PaperRaidFinalityV3(_) => Err(ApiError::conflict(
            "trnm_receipt_domain_lane_mismatch",
            "typed Paper Raid finality commands cannot enter the legacy Paper finality V1 lane",
        )),
    }
}

fn validate_verified_domain_command(
    command: &crate::workflows::TrnmCommand,
    verified_domain_command: &crate::trnm_v1::SignedResearchCommandV1,
) -> Result<(), ApiError> {
    if verified_domain_command != &command.signed_command {
        return Err(ApiError::conflict(
            "trnm_receipt_domain_command_mismatch",
            "verified Receipt V2 Research command differs from the exact locally queued signed command",
        ));
    }
    Ok(())
}

fn validate_verified_command_binding<'a>(
    command: &'a crate::workflows::TrnmCommand,
    paper_id: Uuid,
    verified: &VerifiedCometBftReceiptV2,
    memory: &crate::paper_raid_v2::PaperRaidMemory,
    security: &crate::SecurityConfig,
) -> Result<&'a PaperTrnmCommandBindingV1, ApiError> {
    let binding = validate_command_identity(command, paper_id, verified)?;
    validate_paper_binding_memory(memory, binding, security)?;
    Ok(binding)
}

fn validate_command_identity<'a>(
    command: &'a crate::workflows::TrnmCommand,
    paper_id: Uuid,
    verified: &VerifiedCometBftReceiptV2,
) -> Result<&'a PaperTrnmCommandBindingV1, ApiError> {
    if command.idempotency_key != verified.command_id {
        return Err(ApiError::conflict(
            "trnm_receipt_command_id_mismatch",
            "verified Receipt V2 command_id differs from queued command idempotency_key",
        ));
    }
    let expected_fingerprint = command
        .command_fingerprint
        .strip_prefix("sha256:")
        .ok_or_else(|| ApiError::internal("queued TRNM command fingerprint is not canonical"))?;
    if expected_fingerprint != verified.command_fingerprint_hex {
        return Err(ApiError::conflict(
            "trnm_receipt_command_fingerprint_mismatch",
            "verified Receipt V2 fingerprint differs from queued command fingerprint",
        ));
    }
    if command.status != TrnmProjectionStatus::PendingFinality {
        return Err(ApiError::conflict(
            "trnm_command_not_pending_finality",
            "only a queued pending_finality command can receive Receipt V2 finality",
        ));
    }
    let binding = command.paper_binding.as_ref().ok_or_else(|| {
        ApiError::conflict(
            "paper_trnm_binding_required",
            "legacy or unbound TRNM commands remain pending and cannot finalize a Paper",
        )
    })?;
    if binding.paper_project_id != paper_id {
        return Err(ApiError::conflict(
            "paper_trnm_binding_cross_paper",
            "queued command is immutably bound to a different Paper",
        ));
    }
    let expected_binding_fingerprint = paper_binding_fingerprint(binding)?;
    if command.paper_binding_fingerprint.as_deref() != Some(expected_binding_fingerprint.as_str()) {
        return Err(ApiError::conflict(
            "paper_trnm_binding_fingerprint_mismatch",
            "queued Paper binding no longer matches its immutable fingerprint",
        ));
    }
    Ok(binding)
}

pub(crate) fn validate_match_evidence_binding(
    binding: &PaperTrnmCommandBindingV1,
    submission: &JointPaperSubmission,
    authorization_set: &crate::paper_raid_v2::ResearchSessionAuthorizationSetV1,
    completion: &crate::paper_raid_contracts::SignedNakamaCompletionReceiptV1,
    latest_roster_version: Option<u64>,
    security: &crate::SecurityConfig,
) -> Result<(), ApiError> {
    if authorization_set.status != ResearchSessionAuthorizationSetStatus::Completed
        || authorization_set.session_id != binding.research_session_id
        || authorization_set.paper_project_id != binding.paper_project_id
        || authorization_set.roster_version != binding.research_session_roster_version
        || latest_roster_version != Some(binding.research_session_roster_version)
    {
        return Err(ApiError::conflict(
            "paper_trnm_authorization_epoch_mismatch",
            "Paper binding must reference the latest completed Research Session authorization epoch",
        ));
    }
    if completion.commitment_id != binding.match_evidence_commitment_id
        || completion.session_id != binding.research_session_id
        || completion.team_id != authorization_set.team_id
        || completion.paper_project_id != binding.paper_project_id
        || completion.challenge_id != authorization_set.challenge_id
        || completion.roster_version != binding.research_session_roster_version
        || completion.roster_root != authorization_set.roster_root
    {
        return Err(ApiError::conflict(
            "paper_trnm_match_evidence_mismatch",
            "stored MatchEvidence completion does not match the bound Paper authorization epoch",
        ));
    }
    if completion.terminal_facts.result_code != "paper_bundle_ready"
        || completion.terminal_facts.paper_bundle_hash != submission.paper_bundle_hash
        || completion.terminal_facts.paper_release_candidate_hash
            != submission.release_candidate_hash
        || completion.terminal_facts.contribution_ledger_hash
            != submission
                .paper_bundle
                .release_candidate
                .contribution_ledger_hash
    {
        return Err(ApiError::conflict(
            "paper_trnm_match_evidence_submission_mismatch",
            "stored MatchEvidence terminal facts do not bind the immutable Paper submission",
        ));
    }
    if completion.issuer_key_id != security.nakama_authorization_issuer_key_id {
        return Err(ApiError::conflict(
            "paper_trnm_match_evidence_issuer_mismatch",
            "stored MatchEvidence receipt issuer differs from the active pinned Hepta issuer",
        ));
    }
    if !security
        .trusted_nakama_research_authorities
        .contains_key(&completion.nakama_authority_key_id)
    {
        return Err(ApiError::conflict(
            "paper_trnm_match_evidence_authority_untrusted",
            "stored MatchEvidence receipt names an untrusted Nakama research authority",
        ));
    }
    crate::paper_raid_contracts::verify_nakama_completion_receipt(
        completion,
        &security.nakama_authorization_signing_key.verifying_key(),
    )
    .map_err(|message| {
        ApiError::conflict(
            "paper_trnm_match_evidence_signature_invalid",
            format!("stored MatchEvidence receipt signature is invalid: {message}"),
        )
    })
}

fn validate_paper_binding_memory(
    memory: &crate::paper_raid_v2::PaperRaidMemory,
    binding: &PaperTrnmCommandBindingV1,
    security: &crate::SecurityConfig,
) -> Result<(), ApiError> {
    let paper = memory
        .papers
        .get(&binding.paper_project_id)
        .ok_or_else(|| ApiError::not_found("paper_project_not_found", "Paper does not exist"))?;
    let submission = memory
        .submissions
        .get(&binding.submission_id)
        .ok_or_else(|| {
            ApiError::not_found(
                "joint_submission_not_found",
                "bound submission does not exist",
            )
        })?;
    let evaluation = memory
        .review
        .evaluations
        .get(&binding.evaluation_id)
        .ok_or_else(|| {
            ApiError::not_found(
                "paper_evaluation_not_found",
                "bound evaluation does not exist",
            )
        })?;
    let reproduction = memory
        .review
        .reproductions
        .get(&binding.reproduction_id)
        .ok_or_else(|| {
            ApiError::not_found(
                "paper_reproduction_not_found",
                "bound reproduction does not exist",
            )
        })?;
    let authorization_set = memory
        .research_session_authorization_sets
        .get(&(
            binding.research_session_id.clone(),
            binding.research_session_roster_version,
        ))
        .ok_or_else(|| {
            ApiError::not_found(
                "research_session_authorization_set_not_found",
                "bound Research Session authorization epoch does not exist",
            )
        })?;
    let completion = memory
        .research_session_completions
        .get(&(
            binding.research_session_id.clone(),
            binding.research_session_roster_version,
        ))
        .ok_or_else(|| {
            ApiError::not_found(
                "nakama_completion_not_found",
                "bound Paper/Nakama MatchEvidence completion does not exist",
            )
        })?;
    validate_current_binding_records(binding, paper, submission, evaluation, reproduction)?;
    validate_match_evidence_binding(
        binding,
        submission,
        authorization_set,
        completion,
        memory
            .research_session_authorization_sets
            .values()
            .filter(|set| set.session_id == binding.research_session_id)
            .map(|set| set.roster_version)
            .max(),
        security,
    )?;
    let open_appeal = memory.review.appeals.values().any(|appeal| {
        appeal.evaluation_id == binding.evaluation_id
            && !memory
                .review
                .resolutions
                .values()
                .any(|resolution| resolution.appeal_id == appeal.appeal_id)
    });
    if open_appeal {
        return Err(ApiError::conflict(
            "paper_chain_finality_open_appeal",
            "an unresolved Appeal holds Paper Chain finality",
        ));
    }
    if memory
        .review
        .evaluations
        .values()
        .any(|evaluation| evaluation.supersedes_evaluation_id == Some(binding.evaluation_id))
    {
        return Err(ApiError::conflict(
            "paper_chain_finality_evaluation_superseded",
            "bound evaluation was superseded and requires a new queued command",
        ));
    }
    if memory.review.reproductions.values().any(|reproduction| {
        reproduction.supersedes_reproduction_id == Some(binding.reproduction_id)
    }) {
        return Err(ApiError::conflict(
            "paper_chain_finality_reproduction_superseded",
            "bound reproduction was superseded and requires a new queued command",
        ));
    }
    Ok(())
}

async fn validate_paper_binding_postgres(
    tx: &mut Transaction<'_, Postgres>,
    binding: &PaperTrnmCommandBindingV1,
    exclusive_paper_lock: bool,
    security: &crate::SecurityConfig,
) -> Result<(), ApiError> {
    let paper_lock = if exclusive_paper_lock {
        "for update"
    } else {
        "for share"
    };
    let paper_sql = format!(
        "select record_json from hepta_paper_projects where paper_project_id=$1 {paper_lock}"
    );
    let paper_row = sqlx::query(&paper_sql)
        .bind(binding.paper_project_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("paper_project_not_found", "Paper does not exist"))?;
    let paper: PaperProject = decode_record(paper_row.get("record_json"), "Paper project")?;
    let submission_row = sqlx::query(
        "select record_json from hepta_joint_paper_submissions
         where submission_id=$1 and paper_project_id=$2 for share",
    )
    .bind(binding.submission_id)
    .bind(binding.paper_project_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "joint_submission_not_found",
            "bound submission does not exist",
        )
    })?;
    let submission: JointPaperSubmission =
        decode_record(submission_row.get("record_json"), "joint Paper submission")?;
    let evaluation_row = sqlx::query(
        "select record_json from hepta_paper_evaluations
         where evaluation_id=$1 and paper_project_id=$2 and submission_id=$3 for share",
    )
    .bind(binding.evaluation_id)
    .bind(binding.paper_project_id)
    .bind(binding.submission_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "paper_evaluation_not_found",
            "bound evaluation does not exist",
        )
    })?;
    let evaluation: PaperEvaluation =
        decode_record(evaluation_row.get("record_json"), "Paper evaluation")?;
    let reproduction_row = sqlx::query(
        "select record_json from hepta_paper_reproductions
         where reproduction_id=$1 and paper_project_id=$2 and evaluation_id=$3 for share",
    )
    .bind(binding.reproduction_id)
    .bind(binding.paper_project_id)
    .bind(binding.evaluation_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "paper_reproduction_not_found",
            "bound reproduction does not exist",
        )
    })?;
    let reproduction: PaperReproduction =
        decode_record(reproduction_row.get("record_json"), "Paper reproduction")?;
    validate_current_binding_records(binding, &paper, &submission, &evaluation, &reproduction)?;
    let authorization_row = sqlx::query(
        "select record_json from hepta_research_session_authorization_sets
         where session_id=$1 and roster_version=$2 for share",
    )
    .bind(&binding.research_session_id)
    .bind(i64_from_u64(
        binding.research_session_roster_version,
        "research_session_roster_version",
    )?)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "research_session_authorization_set_not_found",
            "bound Research Session authorization epoch does not exist",
        )
    })?;
    let authorization_set: crate::paper_raid_v2::ResearchSessionAuthorizationSetV1 = decode_record(
        authorization_row.get("record_json"),
        "Research Session authorization set",
    )?;
    let completion_row = sqlx::query(
        "select record_json from hepta_nakama_research_session_completions
         where session_id=$1 and roster_version=$2 and commitment_id=$3
           and paper_project_id=$4 for share",
    )
    .bind(&binding.research_session_id)
    .bind(i64_from_u64(
        binding.research_session_roster_version,
        "research_session_roster_version",
    )?)
    .bind(&binding.match_evidence_commitment_id)
    .bind(binding.paper_project_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "nakama_completion_not_found",
            "bound Paper/Nakama MatchEvidence completion does not exist",
        )
    })?;
    let completion: crate::paper_raid_contracts::SignedNakamaCompletionReceiptV1 = decode_record(
        completion_row.get("record_json"),
        "Nakama Research Session completion",
    )?;
    let latest_roster_version = sqlx::query_scalar::<_, Option<i64>>(
        "select max(roster_version) from hepta_research_session_authorization_sets
         where session_id=$1",
    )
    .bind(&binding.research_session_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .map(|value| u64_from_i64(value, "latest research_session_roster_version"))
    .transpose()?;
    validate_match_evidence_binding(
        binding,
        &submission,
        &authorization_set,
        &completion,
        latest_roster_version,
        security,
    )?;
    let open_appeal = sqlx::query_scalar::<_, bool>(
        "select exists(
            select 1 from hepta_paper_appeals a
            where a.evaluation_id=$1 and a.paper_project_id=$2
              and not exists (
                select 1 from hepta_paper_appeal_resolutions r
                where r.appeal_id=a.appeal_id and r.paper_project_id=a.paper_project_id
              )
         )",
    )
    .bind(binding.evaluation_id)
    .bind(binding.paper_project_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if open_appeal {
        return Err(ApiError::conflict(
            "paper_chain_finality_open_appeal",
            "an unresolved Appeal holds Paper Chain finality",
        ));
    }
    let superseded = sqlx::query_scalar::<_, bool>(
        "select exists(select 1 from hepta_paper_evaluations
         where supersedes_evaluation_id=$1 and paper_project_id=$2)",
    )
    .bind(binding.evaluation_id)
    .bind(binding.paper_project_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if superseded {
        return Err(ApiError::conflict(
            "paper_chain_finality_evaluation_superseded",
            "bound evaluation was superseded and requires a new queued command",
        ));
    }
    let reproduction_superseded = sqlx::query_scalar::<_, bool>(
        "select exists(select 1 from hepta_paper_reproductions
         where supersedes_reproduction_id=$1 and paper_project_id=$2)",
    )
    .bind(binding.reproduction_id)
    .bind(binding.paper_project_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if reproduction_superseded {
        return Err(ApiError::conflict(
            "paper_chain_finality_reproduction_superseded",
            "bound reproduction was superseded and requires a new queued command",
        ));
    }
    Ok(())
}

pub(crate) fn validate_current_binding_records(
    binding: &PaperTrnmCommandBindingV1,
    paper: &PaperProject,
    submission: &JointPaperSubmission,
    evaluation: &PaperEvaluation,
    reproduction: &PaperReproduction,
) -> Result<(), ApiError> {
    if paper.phase == PaperPhase::IntegrityHold
        || submission.status == JointSubmissionStatus::IntegrityHold
    {
        return Err(ApiError::conflict(
            "paper_chain_finality_integrity_hold",
            "Paper or submission is on integrity hold",
        ));
    }
    if paper.phase != PaperPhase::SubmissionReady
        || submission.status != JointSubmissionStatus::SubmissionReady
    {
        return Err(ApiError::conflict(
            "paper_chain_finality_not_ready",
            "Paper and submission must both remain submission_ready",
        ));
    }
    if submission.paper_project_id != binding.paper_project_id
        || evaluation.paper_project_id != binding.paper_project_id
        || evaluation.submission_id != binding.submission_id
        || reproduction.paper_project_id != binding.paper_project_id
        || reproduction.evaluation_id != binding.evaluation_id
    {
        return Err(ApiError::conflict(
            "paper_trnm_binding_record_mismatch",
            "Paper binding identifiers do not match current immutable records",
        ));
    }
    if submission.release_candidate_hash != binding.release_candidate_hash
        || submission.paper_bundle_hash != binding.paper_bundle_hash
        || evaluation.release_candidate_hash != binding.release_candidate_hash
        || evaluation.paper_bundle_hash != binding.paper_bundle_hash
        || evaluation.tolerance_policy_hash != binding.tolerance_policy_hash
        || evaluation.evaluation_signing_hash != binding.evaluation_signing_hash
        || reproduction.release_candidate_hash != binding.release_candidate_hash
        || reproduction.paper_bundle_hash != binding.paper_bundle_hash
        || reproduction.tolerance_policy_hash != binding.tolerance_policy_hash
        || reproduction.report_hash != binding.reproduction_report_hash
        || evaluation.paper_score.score_bps != binding.evaluation_score_bps
        || (evaluation.status == PaperEvaluationStatus::Accepted) != binding.evaluation_accepted
        || u64::try_from(evaluation.created_at.timestamp()).ok()
            != Some(binding.evaluation_completed_at_unix_s)
    {
        return Err(ApiError::conflict(
            "paper_trnm_binding_hash_mismatch",
            "Paper binding hashes do not match current immutable submission/evaluation records",
        ));
    }
    Ok(())
}

fn require_receipt_v2_enabled(state: &AppState) -> Result<(), ApiError> {
    if state.security.finality_mode != crate::FinalityMode::Verified {
        return Err(ApiError::conflict(
            "finality_pending_only",
            "Hepta is configured for pending_only finality; Receipt V2 writes are disabled",
        ));
    }
    if state
        .security
        .pinned_trnm_cometbft_trust_anchor_hashes
        .is_empty()
    {
        return Err(ApiError::internal(
            "verified Receipt V2 mode has no pinned CometBFT trust-anchor hashes",
        ));
    }
    Ok(())
}

fn build_projection(
    paper_id: Uuid,
    command: &crate::workflows::TrnmCommand,
    verified: &VerifiedCometBftReceiptV2,
    trust_anchor_hash: &str,
    verified_at: DateTime<Utc>,
) -> Result<PaperChainFinalityProjectionV1, ApiError> {
    let binding = command
        .paper_binding
        .as_ref()
        .ok_or_else(|| ApiError::internal("verified command lost its Paper binding"))?;
    let paper_binding_fingerprint = command
        .paper_binding_fingerprint
        .clone()
        .ok_or_else(|| ApiError::internal("verified command lost its Paper binding fingerprint"))?;
    Ok(PaperChainFinalityProjectionV1 {
        schema: PAPER_CHAIN_FINALITY_PROJECTION_SCHEMA_V1.to_string(),
        paper_project_id: paper_id,
        submission_id: binding.submission_id,
        evaluation_id: binding.evaluation_id,
        local_command_id: command.command_id,
        command_idempotency_key: command.idempotency_key.clone(),
        command_fingerprint: command.command_fingerprint.clone(),
        paper_binding_fingerprint,
        receipt_hash: verified.receipt_hash_hex.clone(),
        trust_anchor_hash: trust_anchor_hash.to_string(),
        chain_id: verified.chain_id.clone(),
        comet_tx_hash: verified.comet_tx_hash_hex.clone(),
        transaction_index: verified.transaction_index,
        execution_height: verified.execution_height,
        commitment_height: verified.commitment_height,
        commitment_header_hash: verified.commitment_header_hash_hex.clone(),
        app_hash: verified.app_hash_hex.clone(),
        status: PaperChainFinalityStatusV1::VerifiedFinality,
        ranking_eligible: false,
        reward_eligible: false,
        score_eligible: false,
        economic_eligible: false,
        version: 1,
        verified_at,
    })
}

fn projection_event_payload(projection: &PaperChainFinalityProjectionV1) -> Value {
    json!({
        "paper_project_id": projection.paper_project_id,
        "submission_id": projection.submission_id,
        "evaluation_id": projection.evaluation_id,
        "local_command_id": projection.local_command_id,
        "command_idempotency_key": projection.command_idempotency_key,
        "receipt_hash": projection.receipt_hash,
        "trust_anchor_hash": projection.trust_anchor_hash,
        "previous_status": PaperChainFinalityStatusV1::PendingFinality,
        "status": projection.status,
        "ranking_eligible": false,
        "reward_eligible": false,
        "score_eligible": false,
        "economic_eligible": false,
    })
}

async fn insert_outbox_event(
    tx: &mut Transaction<'_, Postgres>,
    event: &EventEnvelope,
) -> Result<(), ApiError> {
    sqlx::query(
        "insert into hepta_outbox (
            event_id,event_type,aggregate_id,aggregate_version,correlation_id,
            causation_id,idempotency_key,schema_version,producer,payload_hash,payload,occurred_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11::jsonb,$12)",
    )
    .bind(event.event_id)
    .bind(&event.event_type)
    .bind(&event.aggregate_id)
    .bind(i64_from_u64(event.aggregate_version, "aggregate_version")?)
    .bind(event.correlation_id)
    .bind(event.causation_id)
    .bind(&event.idempotency_key)
    .bind(&event.schema_version)
    .bind(&event.producer)
    .bind(&event.payload_hash)
    .bind(&event.payload)
    .bind(event.occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    Ok(())
}

fn trust_anchor_hash_header(headers: &HeaderMap) -> Result<String, ApiError> {
    let values = headers
        .get_all(TRNM_TRUST_ANCHOR_HASH_HEADER)
        .iter()
        .collect::<Vec<_>>();
    if values.is_empty() {
        return Err(ApiError::bad_request(
            "trnm_trust_anchor_hash_required",
            format!("{TRNM_TRUST_ANCHOR_HASH_HEADER} is required"),
        ));
    }
    if values.len() != 1 {
        return Err(ApiError::bad_request(
            "duplicate_trnm_trust_anchor_hash",
            "multiple trust-anchor hash headers are forbidden",
        ));
    }
    let value = values[0]
        .to_str()
        .map_err(|_| {
            ApiError::bad_request(
                "invalid_trnm_trust_anchor_hash",
                "trust anchor hash header must be visible ASCII",
            )
        })?
        .to_string();
    crate::validate_raw_sha256_hex("trust anchor hash", &value)
        .map_err(|message| ApiError::bad_request("invalid_trnm_trust_anchor_hash", message))?;
    Ok(value)
}

pub(crate) async fn read_limited_body(
    request: Request,
    limit: usize,
) -> Result<axum::body::Bytes, ApiError> {
    let content_lengths = request
        .headers()
        .get_all(CONTENT_LENGTH)
        .iter()
        .map(|value| value.to_str())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            ApiError::bad_request(
                "invalid_content_length",
                "Content-Length must be canonical visible ASCII",
            )
        })?;
    if content_lengths.len() > 1 {
        return Err(ApiError::bad_request(
            "invalid_content_length",
            "multiple Content-Length values are forbidden",
        ));
    }
    if let Some(content_length) = content_lengths.first() {
        if content_length.is_empty()
            || (content_length.len() > 1 && content_length.starts_with('0'))
            || content_length.bytes().any(|byte| !byte.is_ascii_digit())
        {
            return Err(ApiError::bad_request(
                "invalid_content_length",
                "Content-Length must be canonical decimal",
            ));
        }
        let content_length = content_length.parse::<usize>().map_err(|_| {
            ApiError::payload_too_large("Content-Length exceeds the supported platform range")
        })?;
        if content_length > limit {
            return Err(ApiError::payload_too_large(format!(
                "request body exceeds the {limit}-byte endpoint limit"
            )));
        }
    }
    to_bytes(request.into_body(), limit).await.map_err(|_| {
        ApiError::payload_too_large(format!(
            "request body could not be read within the {limit}-byte endpoint limit"
        ))
    })
}

fn decode_record<T: DeserializeOwned>(value: Value, label: &str) -> Result<T, ApiError> {
    serde_json::from_value(value)
        .map_err(|error| ApiError::internal(format!("decode {label}: {error}")))
}

fn i64_from_u64(value: u64, field: &str) -> Result<i64, ApiError> {
    i64::try_from(value)
        .map_err(|_| ApiError::bad_request("integer_out_of_range", format!("{field} exceeds i64")))
}

fn u64_from_i64(value: i64, field: &str) -> Result<u64, ApiError> {
    u64::try_from(value).map_err(|_| {
        ApiError::internal(format!(
            "stored {field} is negative or outside the u64 range"
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, sync::Arc, time::Duration};

    use axum::{
        body::{to_bytes as response_bytes, Body},
        http::Request as HttpRequest,
    };
    use ed25519_dalek::SigningKey;
    use serde_json::json;
    use sqlx::{Connection, PgConnection};
    use tower::ServiceExt;
    use trnm_research_protocol::{
        PaperRaidAppealStatusV2, PaperRaidAppealStatusV3, PaperRaidFinalityCommitmentV2,
        PaperRaidFinalityCommitmentV3, SignedPaperRaidFinalityCommandV2,
        SignedPaperRaidFinalityCommandV3,
    };

    use super::*;
    use crate::{
        app,
        trnm_v1::{
            AuthorityRole, EvaluationCommitmentV1, ExternalKey, FinalityReceiptV1,
            ObjectInclusionProofV1, ObjectRefV1, QuorumCertificateV1, ResearchCommandV1,
            ResearchObjectKind, SignedResearchCommandV1, TrnmCommandKind, FINALITY_RECEIPT_V1,
            OBJECT_INCLUSION_PROOF_V1, QUORUM_CERTIFICATE_V1,
        },
        workflows::TrnmCommand,
        FinalityMode, SecurityConfig, DEFAULT_TRNM_RECEIPT_V2_MAX_BODY_BYTES,
        DEFAULT_TRNM_RECEIPT_V2_MAX_IN_FLIGHT, MAX_TRNM_RECEIPT_V2_DEPLOYMENT_BODY_BYTES,
        MAX_TRNM_RECEIPT_V2_MAX_IN_FLIGHT,
    };

    const ANCHOR_FILE: &[u8] = include_bytes!(
        "../../../vendor/trnm-finality-verifier/fixtures/cometbft-trust-anchor-v1.json"
    );
    const RECEIPT_FILE: &[u8] = include_bytes!(
        "../../../vendor/trnm-finality-verifier/fixtures/cometbft-apphash-finality-receipt-v2.json"
    );
    const ANCHOR_HASH: &str = "88b73fc902dd554c35b9a44ff582ec6d76e59085a2e4fdf14292183f4b3846d5";
    const VERIFICATION_TIME: u64 = 1_786_034_510;

    fn canonical_fixture_payload(file: &'static [u8]) -> &'static [u8] {
        file.strip_suffix(b"\n")
            .expect("repository JSON fixture must end in one transport newline")
    }

    fn lower_hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;

        let mut encoded = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(&mut encoded, "{byte:02x}").expect("write lowercase hex");
        }
        encoded
    }

    fn verified_receipt_fixture() -> VerifiedCometBftReceiptV2 {
        let receipt = CometBftAppHashFinalityReceiptV2::from_canonical_bytes(
            canonical_fixture_payload(RECEIPT_FILE),
        )
        .expect("canonical Receipt V2 fixture");
        verify_receipt(
            &receipt,
            canonical_fixture_payload(ANCHOR_FILE),
            ANCHOR_HASH,
            &HashSet::from([ANCHOR_HASH.to_string()]),
            std::time::UNIX_EPOCH + Duration::from_secs(VERIFICATION_TIME),
        )
        .expect("Receipt V2 fixture verifies")
    }

    fn security() -> SecurityConfig {
        SecurityConfig::new("operator", "nakama")
            .with_trnm_token("trnm")
            .with_finality_mode(FinalityMode::Verified)
            .with_pinned_trnm_cometbft_trust_anchor_hash(ANCHOR_HASH)
            .expect("valid pinned anchor")
            .with_trusted_nakama_research_authority(
                "nakama-paper-raid-test-v1",
                SigningKey::from_bytes(&[0x75; 32])
                    .verifying_key()
                    .to_bytes(),
            )
            .expect("valid Nakama test completion authority")
    }

    fn security_with_receipt_v2_limits(
        max_body_bytes: usize,
        max_in_flight: usize,
    ) -> SecurityConfig {
        security()
            .with_trnm_receipt_v2_ingress_limits(max_body_bytes, max_in_flight)
            .expect("valid Receipt V2 ingress limits")
    }

    fn fixed_state() -> AppState {
        let mut state = AppState::new(security());
        state.cometbft_local_verification_clock =
            Arc::new(|| std::time::UNIX_EPOCH + Duration::from_secs(VERIFICATION_TIME));
        state
    }

    fn fixed_state_with_receipt_v2_limits(max_body_bytes: usize, max_in_flight: usize) -> AppState {
        let mut state = AppState::new(security_with_receipt_v2_limits(
            max_body_bytes,
            max_in_flight,
        ));
        state.cometbft_local_verification_clock =
            Arc::new(|| std::time::UNIX_EPOCH + Duration::from_secs(VERIFICATION_TIME));
        state
    }

    async fn raw_request(
        router: Router,
        uri: &str,
        token_header: Option<(&str, &str)>,
        body: Vec<u8>,
        content_length: Option<String>,
        anchor_headers: usize,
    ) -> (StatusCode, Value) {
        let mut builder = HttpRequest::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        if let Some((name, value)) = token_header {
            builder = builder.header(name, value);
        }
        if let Some(content_length) = content_length {
            builder = builder.header("content-length", content_length);
        }
        for _ in 0..anchor_headers {
            builder = builder.header(TRNM_TRUST_ANCHOR_HASH_HEADER, ANCHOR_HASH);
        }
        let response = router
            .oneshot(builder.body(Body::from(body)).expect("request"))
            .await
            .expect("response");
        let status = response.status();
        let body = response_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let value = serde_json::from_slice(&body)
            .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&body)}));
        (status, value)
    }

    async fn get_json(router: Router, uri: &str) -> (StatusCode, Value) {
        let response = router
            .oneshot(
                HttpRequest::builder()
                    .method("GET")
                    .uri(uri)
                    .header(TRNM_TOKEN_HEADER, "trnm")
                    .body(Body::empty())
                    .expect("GET request"),
            )
            .await
            .expect("GET response");
        let status = response.status();
        let body = response_bytes(response.into_body(), usize::MAX)
            .await
            .expect("GET response body");
        let value = serde_json::from_slice(&body)
            .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&body)}));
        (status, value)
    }

    fn legacy_v1_receipt(command: &TrnmCommand) -> FinalityReceiptV1 {
        FinalityReceiptV1 {
            protocol: FINALITY_RECEIPT_V1.to_string(),
            source_event_id: Uuid::new_v4(),
            command_id: command.command_id,
            command_fingerprint: command.command_fingerprint.clone(),
            chain_id: "legacy-paper-finality-test".to_string(),
            tx_hash: digest(0xa1),
            tx_index: 0,
            block_height: 1,
            block_hash: digest(0xa2),
            state_root: digest(0xa3),
            object_ref: ObjectRefV1::new(
                ResearchObjectKind::EvaluationCommitment,
                ExternalKey::from_bytes([0xa4; 32]),
                1,
            ),
            inclusion_proof: ObjectInclusionProofV1 {
                protocol: OBJECT_INCLUSION_PROOF_V1.to_string(),
                leaf_index: 0,
                sibling_hashes: Vec::new(),
            },
            validator_set_id: "legacy-validator-set".to_string(),
            quorum_certificate: QuorumCertificateV1 {
                protocol: QUORUM_CERTIFICATE_V1.to_string(),
                chain_id: "legacy-paper-finality-test".to_string(),
                validator_set_id: "legacy-validator-set".to_string(),
                block_height: 1,
                block_hash: digest(0xa2),
                state_root: digest(0xa3),
                signed_voting_power: 0,
                total_voting_power: 1,
                signatures: Vec::new(),
            },
            confirmations: 1,
            receipt_hash: digest(0xa5),
        }
    }

    fn legacy_live_receipt(command: &TrnmCommand) -> Value {
        json!({
            "source_event_id": Uuid::new_v4(),
            "receipt": {
                "schema": "trnm_finality_receipt_v1",
                "chain_id": "legacy-paper-finality-test",
                "command_id": command.command_id.to_string(),
                "domain_command_fingerprint_hex": command.command_fingerprint
                    .strip_prefix("sha256:")
                    .expect("canonical command fingerprint"),
                "transaction_hash_hex": "a1".repeat(32),
                "transaction_index": 0,
                "block_height": 1,
                "block_hash_hex": "a2".repeat(32),
                "block_header": {
                    "schema": "trnm_block_header_v1",
                    "chain_id": "legacy-paper-finality-test",
                    "height": 1,
                    "previous_block_hash_hex": "00".repeat(32),
                    "transaction_root_hex": "a3".repeat(32),
                    "state_root_hex": "a4".repeat(32),
                    "validator_set_id": "legacy-validator-set",
                    "timestamp_unix_ms": 1,
                },
                "state_root_hex": "a4".repeat(32),
                "transaction_root_hex": "a3".repeat(32),
                "object_ref": null,
                "transaction_inclusion_proof": {
                    "tree_domain": "trnm.transactions.v1",
                    "leaf_hash_hex": "a3".repeat(32),
                    "leaf_index": 0,
                    "leaf_count": 1,
                    "steps": [],
                },
                "object_inclusion_proof": null,
                "validator_set_id": "legacy-validator-set",
                "quorum_certificate": {
                    "validator_set_id": "legacy-validator-set",
                    "height": 1,
                    "block_hash_hex": "a2".repeat(32),
                    "signatures": [],
                },
                "receipt_hash_hex": "a5".repeat(32),
            },
        })
    }

    async fn legacy_lane_snapshot(state: &AppState, command_id: Uuid) -> Value {
        state
            .inspect(|league| {
                let command = league.trnm_commands.get(&command_id).ok_or_else(|| {
                    ApiError::internal("legacy finality guard test command disappeared")
                })?;
                Ok(json!({
                    "command_status": &command.status,
                    "legacy_projection": league.trnm_finality.get(&command_id),
                    "legacy_live_projection": league.trnm_live_finality.get(&command_id),
                    "legacy_inbox": &league.inbox_events,
                    "event_ids": league.events.iter().map(|event| event.event_id).collect::<Vec<_>>(),
                    "event_types": league.events.iter().map(|event| event.event_type.clone()).collect::<Vec<_>>(),
                }))
            })
            .await
            .expect("legacy lane snapshot")
    }

    async fn assert_legacy_lanes_reject(router: &Router, command: &TrnmCommand) {
        let (status, error) = raw_request(
            router.clone(),
            "/v1/hepta/trnm/finality",
            Some((TRNM_TOKEN_HEADER, "trnm")),
            serde_json::to_vec(&legacy_v1_receipt(command)).expect("legacy v1 receipt JSON"),
            None,
            0,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{error}");
        assert_eq!(error["code"], "paper_trnm_legacy_finality_forbidden");

        let (status, error) = raw_request(
            router.clone(),
            "/v1/hepta/trnm/finality/live",
            Some((TRNM_TOKEN_HEADER, "trnm")),
            serde_json::to_vec(&legacy_live_receipt(command)).expect("legacy live receipt JSON"),
            None,
            0,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{error}");
        assert_eq!(error["code"], "paper_trnm_legacy_finality_forbidden");
    }

    fn fixture_bound_command(binding: &PaperTrnmCommandBindingV1) -> TrnmCommand {
        let verified = verified_receipt_fixture();
        let VerifiedCometBftDomainCommandV2::ResearchV1(signed) = verified.domain_command else {
            panic!("repository Receipt V2 fixture must carry Research V1");
        };
        TrnmCommand {
            command_id: Uuid::new_v4(),
            kind: TrnmCommandKind::EvaluationCommitment,
            aggregate_id: signed.command.primary_object_ref().key.to_hex(),
            idempotency_key: verified.command_id,
            command_fingerprint: format!("sha256:{}", verified.command_fingerprint_hex),
            paper_binding: Some(binding.clone()),
            paper_binding_fingerprint: Some(
                paper_binding_fingerprint(binding).expect("Paper binding fingerprint"),
            ),
            signed_command: *signed,
            status: TrnmProjectionStatus::PendingFinality,
            created_at: Utc::now(),
        }
    }

    fn mismatched_fixture_bound_command(binding: &PaperTrnmCommandBindingV1) -> TrnmCommand {
        let mut command = fixture_bound_command(binding);
        command.signed_command = signed_command(binding, ExternalKey::from_bytes([0x42; 32]));
        command
    }

    async fn postgres_json_rows(pool: &sqlx::PgPool, query: &str) -> Vec<Value> {
        sqlx::query(query)
            .fetch_all(pool)
            .await
            .expect("Paper finality side-effect snapshot query")
            .into_iter()
            .map(|row| row.get("record"))
            .collect()
    }

    async fn paper_finality_side_effect_snapshot(state: &AppState) -> Value {
        if let Some(pool) = &state.pool {
            let league = sqlx::query_scalar::<_, Value>(
                "select to_jsonb(snapshot) from (
                    select state_key,revision,state_json,updated_at
                    from hepta_league_state where state_key='primary'
                 ) snapshot",
            )
            .fetch_one(pool)
            .await
            .expect("League state side-effect snapshot");
            return json!({
                "league": league,
                "paper_projects_with_finality_v2_seal": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_paper_projects order by paper_project_id
                     ) snapshot",
                ).await,
                "joint_paper_submissions": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_joint_paper_submissions order by submission_id
                     ) snapshot",
                ).await,
                "research_session_authorization_sets": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_research_session_authorization_sets
                        order by session_id,roster_version
                     ) snapshot",
                ).await,
                "nakama_research_session_completions": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_nakama_research_session_completions
                        order by commitment_id
                     ) snapshot",
                ).await,
                "paper_evaluations": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_paper_evaluations order by evaluation_id
                     ) snapshot",
                ).await,
                "paper_reproductions": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_paper_reproductions order by reproduction_id
                     ) snapshot",
                ).await,
                "paper_appeals": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_paper_appeals order by appeal_id
                     ) snapshot",
                ).await,
                "paper_appeal_resolutions": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_paper_appeal_resolutions order by resolution_id
                     ) snapshot",
                ).await,
                "trust_anchors": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_trnm_cometbft_trust_anchors order by anchor_hash
                     ) snapshot",
                ).await,
                "time_checkpoints_v1": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_trnm_cometbft_time_checkpoints_v1
                        order by chain_id,height,checkpoint_hash
                     ) snapshot",
                ).await,
                "finality_window_arms_v2": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_paper_chain_finality_window_arms_v2
                        order by paper_project_id,arm_id
                     ) snapshot",
                ).await,
                "finality_preparations_v2": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_paper_chain_finality_preparations_v2
                        order by paper_project_id,preparation_id
                     ) snapshot",
                ).await,
                "finality_inbox": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_paper_chain_finality_inbox order by receipt_hash
                     ) snapshot",
                ).await,
                "receipts": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_paper_chain_receipts order by receipt_hash
                     ) snapshot",
                ).await,
                "projections": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_paper_chain_finality_projections order by local_command_id
                     ) snapshot",
                ).await,
                "outbox": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_outbox order by event_id
                     ) snapshot",
                ).await,
                "inbox": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_inbox order by consumer,event_id
                     ) snapshot",
                ).await,
                "paper_raid_idempotency": postgres_json_rows(
                    pool,
                    "select to_jsonb(snapshot) as record from (
                        select * from hepta_paper_raid_idempotency order by operation,idempotency_key
                     ) snapshot",
                ).await,
            });
        }

        let league = {
            let league = state.inner.read().await;
            serde_json::to_value(&*league).expect("memory League state snapshot")
        };
        let paper_raid = state
            .paper_raid
            .read()
            .await
            .paper_finality_v1_side_effect_snapshot();
        let finality = state.paper_chain_finality.read().await;
        let mut trust_anchors = finality
            .trust_anchors
            .iter()
            .map(|(anchor_hash, anchor)| {
                json!({
                    "anchor_hash": anchor_hash,
                    "record": anchor.record,
                    "canonical_hex": lower_hex(anchor.canonical.as_ref()),
                })
            })
            .collect::<Vec<_>>();
        trust_anchors.sort_by(|left, right| {
            left["anchor_hash"]
                .as_str()
                .cmp(&right["anchor_hash"].as_str())
        });
        let mut receipts = finality
            .receipts
            .iter()
            .map(|(receipt_hash, receipt)| {
                json!({
                    "receipt_hash": receipt_hash,
                    "canonical_sha256": receipt.canonical_sha256,
                    "trust_anchor_hash": receipt.trust_anchor_hash,
                    "canonical_hex": lower_hex(receipt.canonical.as_ref()),
                    "projection": receipt.projection,
                })
            })
            .collect::<Vec<_>>();
        receipts.sort_by(|left, right| {
            left["receipt_hash"]
                .as_str()
                .cmp(&right["receipt_hash"].as_str())
        });
        let preparations_v2 = &finality.preparations_v2;
        let mut time_checkpoint_hash_by_chain_height = preparations_v2
            .time_checkpoint_hash_by_chain_height
            .iter()
            .map(|((chain_id, height), checkpoint_hash)| {
                json!({
                    "chain_id": chain_id,
                    "height": height,
                    "checkpoint_hash": checkpoint_hash,
                })
            })
            .collect::<Vec<_>>();
        time_checkpoint_hash_by_chain_height.sort_by(|left, right| {
            (left["chain_id"].as_str(), left["height"].as_u64())
                .cmp(&(right["chain_id"].as_str(), right["height"].as_u64()))
        });
        json!({
            "league": league,
            "paper_raid": paper_raid,
            "trust_anchors": trust_anchors,
            "finality_inbox": finality.inbox,
            "receipts": receipts,
            "projections": finality.projections,
            "preparations_v2": {
                "time_checkpoints_by_hash": preparations_v2.time_checkpoints_by_hash,
                "time_checkpoint_hash_by_chain_height": time_checkpoint_hash_by_chain_height,
                "arms_by_idempotency_key": preparations_v2.arms_by_idempotency_key,
                "arms_by_id": preparations_v2.arms_by_id,
                "arms_by_source_fingerprint": preparations_v2.arms_by_source_fingerprint,
                "by_idempotency_key": preparations_v2.by_idempotency_key,
                "by_commitment_id": preparations_v2.by_commitment_id,
                "by_paper_id": preparations_v2.by_paper_id,
            },
        })
    }

    async fn admit_fixture_anchor(router: &Router) {
        let (status, body) = raw_request(
            router.clone(),
            "/v2/hepta/operator/trnm/trust-anchors",
            Some((OPERATOR_TOKEN_HEADER, "operator")),
            canonical_fixture_payload(ANCHOR_FILE).to_vec(),
            None,
            0,
        )
        .await;
        assert!(
            matches!(status, StatusCode::CREATED | StatusCode::OK),
            "{body}"
        );
    }

    fn paper_raid_v2_domain_command() -> SignedPaperRaidFinalityCommandV2 {
        SignedPaperRaidFinalityCommandV2::sign(
            "trnm-comet-spike".to_string(),
            ExternalKey::from_bytes([0x91; 32]),
            "did:trnm:hepta-authority".to_string(),
            1,
            PaperRaidFinalityCommitmentV2 {
                commitment_id: ExternalKey::from_bytes([0x92; 32]),
                paper_project_id: ExternalKey::from_bytes([0x93; 32]),
                submission_id: ExternalKey::from_bytes([0x94; 32]),
                match_evidence_ref: ObjectRefV1::new(
                    ResearchObjectKind::MatchEvidence,
                    ExternalKey::from_bytes([0x95; 32]),
                    1,
                ),
                release_candidate_hash: [0x11; 32],
                paper_bundle_hash: [0x12; 32],
                submission_commitment_hash: [0x13; 32],
                author_consent_set_hash: [0x14; 32],
                tolerance_policy_hash: [0x15; 32],
                evaluation_id: ExternalKey::from_bytes([0x96; 32]),
                evaluation_hash: [0x16; 32],
                evaluation_score_bps: 8_500,
                evaluation_accepted: true,
                evaluation_completed_at_unix_s: 100,
                latest_reproduction_id: ExternalKey::from_bytes([0x97; 32]),
                latest_reproduction_hash: [0x17; 32],
                latest_reproduction_accepted: true,
                latest_reproduction_completed_at_unix_s: 110,
                evaluation_superseded_by: None,
                reproduction_superseded_by: None,
                appeal_status: PaperRaidAppealStatusV2::ClosedNoAppeal,
                appeal_id: None,
                appeal_resolution_hash: None,
                appeal_window_closes_at_unix_s: 120,
                settlement_policy_hash: [0x18; 32],
                scientific_finality: true,
                score_eligible: false,
                ranking_eligible: false,
                reward_eligible: false,
                economic_eligible: false,
                finalized_at_unix_s: 121,
            },
            &SigningKey::from_bytes(&[0x31; 32]),
        )
        .expect("valid Paper Raid finality V2 command")
    }

    fn paper_raid_v3_domain_command() -> SignedPaperRaidFinalityCommandV3 {
        SignedPaperRaidFinalityCommandV3::sign(
            "trnm-comet-spike".to_string(),
            ExternalKey::from_bytes([0xa1; 32]),
            "did:trnm:hepta-authority".to_string(),
            1,
            PaperRaidFinalityCommitmentV3 {
                commitment_id: ExternalKey::from_bytes([0xa2; 32]),
                paper_project_id: ExternalKey::from_bytes([0xa3; 32]),
                submission_id: ExternalKey::from_bytes([0xa4; 32]),
                match_evidence_ref: ObjectRefV1::new(
                    ResearchObjectKind::MatchEvidence,
                    ExternalKey::from_bytes([0xa5; 32]),
                    1,
                ),
                release_candidate_hash: [0x21; 32],
                paper_bundle_hash: [0x22; 32],
                submission_commitment_hash: [0x23; 32],
                author_consent_set_hash: [0x24; 32],
                tolerance_policy_hash: [0x25; 32],
                evaluation_id: ExternalKey::from_bytes([0xa6; 32]),
                evaluation_hash: [0x26; 32],
                evaluation_score_bps: 8_500,
                evaluation_accepted: true,
                evaluation_completed_at_unix_s: 100,
                latest_reproduction_id: ExternalKey::from_bytes([0xa7; 32]),
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
            &SigningKey::from_bytes(&[0x32; 32]),
        )
        .expect("valid Paper Raid finality V3 command")
    }

    fn synthetic_verified_domain(
        domain_command: VerifiedCometBftDomainCommandV2,
    ) -> VerifiedCometBftReceiptV2 {
        let (command_id, command_fingerprint_hex) = match &domain_command {
            VerifiedCometBftDomainCommandV2::ResearchV1(command) => (
                command.command_id.to_hex(),
                lower_hex(&command.command_fingerprint()),
            ),
            VerifiedCometBftDomainCommandV2::PaperRaidFinalityV2(command) => (
                command.command_id.to_hex(),
                lower_hex(&command.command_fingerprint()),
            ),
            VerifiedCometBftDomainCommandV2::PaperRaidFinalityV3(command) => (
                command.command_id.to_hex(),
                lower_hex(&command.command_fingerprint()),
            ),
        };
        VerifiedCometBftReceiptV2 {
            receipt_hash_hex: "b1".repeat(32),
            chain_id: "trnm-comet-spike".to_string(),
            command_id,
            command_fingerprint_hex,
            comet_tx_hash_hex: "b2".repeat(32),
            transaction_index: 0,
            applied_command_object_key_hex: "b3".repeat(32),
            execution_height: 7,
            commitment_height: 8,
            commitment_header_hash_hex: "b4".repeat(32),
            app_hash_hex: "b5".repeat(32),
            domain_command,
        }
    }

    fn synthetic_verified_paper_raid_domain(version: u8) -> VerifiedCometBftReceiptV2 {
        let domain_command = match version {
            2 => {
                let command = paper_raid_v2_domain_command();
                command
                    .validate()
                    .expect("synthetic Paper Raid V2 signature must verify");
                VerifiedCometBftDomainCommandV2::PaperRaidFinalityV2(Box::new(command))
            }
            3 => {
                let command = paper_raid_v3_domain_command();
                command
                    .validate()
                    .expect("synthetic Paper Raid V3 signature must verify");
                VerifiedCometBftDomainCommandV2::PaperRaidFinalityV3(Box::new(command))
            }
            _ => panic!("unsupported synthetic Paper Raid domain version"),
        };
        synthetic_verified_domain(domain_command)
    }

    async fn exercise_legacy_lane_ordering(state: AppState) {
        let binding =
            crate::paper_raid_v2::endpoint_tests::seed_paper_chain_finality_test(state.clone())
                .await;
        let command = fixture_bound_command(&binding);
        let verified_fixture = verified_receipt_fixture();
        assert!(matches!(
            &verified_fixture.domain_command,
            VerifiedCometBftDomainCommandV2::ResearchV1(receipt_command)
                if receipt_command.as_ref() == &command.signed_command
        ));
        state
            .transact(|league| {
                league
                    .trnm_commands
                    .insert(command.command_id, command.clone());
                Ok(())
            })
            .await
            .expect("seed fixture-bound command");
        let router = app(state.clone());

        let pending = legacy_lane_snapshot(&state, command.command_id).await;
        assert_eq!(pending["command_status"], "pending_finality");
        assert!(pending["legacy_projection"].is_null());
        assert!(pending["legacy_live_projection"].is_null());
        assert!(!pending["event_types"]
            .as_array()
            .expect("event types")
            .iter()
            .any(|event_type| matches!(
                event_type.as_str(),
                Some("trnm.receipt.projected.v2" | "trnm.live.receipt.projected.v1")
            )));
        assert_legacy_lanes_reject(&router, &command).await;
        assert_eq!(
            legacy_lane_snapshot(&state, command.command_id).await,
            pending,
            "legacy-before-v2 must not alter command, event, inbox, or projection state"
        );

        let (anchor_status, anchor) = raw_request(
            router.clone(),
            "/v2/hepta/operator/trnm/trust-anchors",
            Some((OPERATOR_TOKEN_HEADER, "operator")),
            canonical_fixture_payload(ANCHOR_FILE).to_vec(),
            None,
            0,
        )
        .await;
        assert_eq!(anchor_status, StatusCode::CREATED, "{anchor}");
        let finality_path = format!(
            "/v2/hepta/papers/{}/chain-finality",
            binding.paper_project_id
        );
        let (finality_status, projection) = raw_request(
            router.clone(),
            &finality_path,
            Some((TRNM_TOKEN_HEADER, "trnm")),
            canonical_fixture_payload(RECEIPT_FILE).to_vec(),
            None,
            1,
        )
        .await;
        assert_eq!(finality_status, StatusCode::CREATED, "{projection}");
        assert_eq!(projection["status"], "verified_finality");
        assert_eq!(
            projection["local_command_id"],
            command.command_id.to_string()
        );
        let after_created = paper_finality_side_effect_snapshot(&state).await;
        let (replay_status, replayed_projection) = raw_request(
            router.clone(),
            &finality_path,
            Some((TRNM_TOKEN_HEADER, "trnm")),
            canonical_fixture_payload(RECEIPT_FILE).to_vec(),
            None,
            1,
        )
        .await;
        assert_eq!(replay_status, StatusCode::OK, "{replayed_projection}");
        assert_eq!(replayed_projection, projection);
        assert_eq!(
            paper_finality_side_effect_snapshot(&state).await,
            after_created,
            "an exact Research V1 Receipt V2 replay must return the same projection without mutating reachable Paper finality V1 state",
        );

        let verified = legacy_lane_snapshot(&state, command.command_id).await;
        assert_eq!(verified["command_status"], "verified_finality");
        assert!(verified["legacy_projection"].is_null());
        assert!(verified["legacy_live_projection"].is_null());
        assert!(!verified["event_types"]
            .as_array()
            .expect("event types")
            .iter()
            .any(|event_type| matches!(
                event_type.as_str(),
                Some("trnm.receipt.projected.v2" | "trnm.live.receipt.projected.v1")
            )));
        let (get_status, stored_projection) = get_json(router.clone(), &finality_path).await;
        assert_eq!(get_status, StatusCode::OK, "{stored_projection}");
        assert_eq!(stored_projection, projection);

        assert_legacy_lanes_reject(&router, &command).await;
        assert_eq!(
            legacy_lane_snapshot(&state, command.command_id).await,
            verified,
            "v2-before-legacy must preserve the verified command and emit no legacy side effect"
        );
        let (get_status, replayed_projection) = get_json(router, &finality_path).await;
        assert_eq!(get_status, StatusCode::OK, "{replayed_projection}");
        assert_eq!(replayed_projection, stored_projection);

        state
            .transact(|league| {
                let queued = league
                    .trnm_commands
                    .get_mut(&command.command_id)
                    .ok_or_else(|| ApiError::internal("queued Research command disappeared"))?;
                queued.signed_command =
                    signed_command(&binding, ExternalKey::from_bytes([0x42; 32]));
                Ok(())
            })
            .await
            .expect("tamper queued signed Research command after exact replay");
        let after_queue_tamper = paper_finality_side_effect_snapshot(&state).await;
        let (tampered_replay_status, tampered_replay_error) = raw_request(
            app(state.clone()),
            &finality_path,
            Some((TRNM_TOKEN_HEADER, "trnm")),
            canonical_fixture_payload(RECEIPT_FILE).to_vec(),
            None,
            1,
        )
        .await;
        assert_eq!(
            tampered_replay_status,
            StatusCode::CONFLICT,
            "{tampered_replay_error}"
        );
        assert_eq!(
            tampered_replay_error["code"],
            "trnm_receipt_domain_command_mismatch"
        );
        assert_eq!(
            paper_finality_side_effect_snapshot(&state).await,
            after_queue_tamper,
            "the replay shortcut must not bypass exact equality with the queued signed Research command",
        );
        let (get_status, projection_after_tampered_replay) =
            get_json(app(state), &finality_path).await;
        assert_eq!(
            get_status,
            StatusCode::OK,
            "{projection_after_tampered_replay}"
        );
        assert_eq!(projection_after_tampered_replay, stored_projection);
    }

    async fn exercise_mismatched_research_zero_side_effects(state: AppState) {
        let binding =
            crate::paper_raid_v2::endpoint_tests::seed_paper_chain_finality_test(state.clone())
                .await;
        let command = mismatched_fixture_bound_command(&binding);
        state
            .transact(|league| {
                league
                    .trnm_commands
                    .insert(command.command_id, command.clone());
                Ok(())
            })
            .await
            .expect("seed mismatched fixture-bound command");
        let router = app(state.clone());
        admit_fixture_anchor(&router).await;
        let before = paper_finality_side_effect_snapshot(&state).await;
        let path = format!(
            "/v2/hepta/papers/{}/chain-finality",
            binding.paper_project_id
        );
        for _ in 0..2 {
            let (status, error) = raw_request(
                router.clone(),
                &path,
                Some((TRNM_TOKEN_HEADER, "trnm")),
                canonical_fixture_payload(RECEIPT_FILE).to_vec(),
                None,
                1,
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT, "{error}");
            assert_eq!(error["code"], "trnm_receipt_domain_command_mismatch");
        }
        assert_eq!(
            paper_finality_side_effect_snapshot(&state).await,
            before,
            "mismatched Research receipt must not alter business state, idempotency, events, projections, receipt inbox, or outbox",
        );
    }

    async fn exercise_paper_raid_lane_guard_zero_side_effects(state: AppState) {
        let binding =
            crate::paper_raid_v2::endpoint_tests::seed_paper_chain_finality_test(state.clone())
                .await;
        let command = fixture_bound_command(&binding);
        state
            .transact(|league| {
                league
                    .trnm_commands
                    .insert(command.command_id, command.clone());
                Ok(())
            })
            .await
            .expect("seed exact Research command before Paper Raid lane guard test");
        let before = paper_finality_side_effect_snapshot(&state).await;
        let canonical = Bytes::from_static(b"synthetic-post-verification-paper-raid-domain");
        let canonical_sha256 = format!("sha256:{}", sha256_hex(&canonical));
        let verified_at =
            DateTime::<Utc>::from(std::time::UNIX_EPOCH + Duration::from_secs(VERIFICATION_TIME));
        for version in [2, 3] {
            for _ in 0..2 {
                let verified = synthetic_verified_paper_raid_domain(version);
                let error = if let Some(pool) = &state.pool {
                    let mut tx = pool
                        .begin()
                        .await
                        .expect("begin Paper Raid typed-domain guard transaction");
                    let result = ingest_verified_paper_chain_finality_postgres(
                        &state,
                        &mut tx,
                        binding.paper_project_id,
                        ANCHOR_HASH,
                        canonical.as_ref(),
                        &canonical_sha256,
                        &verified,
                        verified_at,
                    )
                    .await;
                    let error = result.expect_err(
                        "Paper Raid domain must not enter PostgreSQL legacy Paper finality V1",
                    );
                    assert_eq!(error.status, StatusCode::CONFLICT);
                    assert_eq!(error.code, "trnm_receipt_domain_lane_mismatch");
                    tx.commit()
                        .await
                        .expect("commit expected Paper Raid typed-domain application rejection");
                    error
                } else {
                    ingest_verified_paper_chain_finality_memory(
                        &state,
                        binding.paper_project_id,
                        ANCHOR_HASH.to_string(),
                        canonical.clone(),
                        canonical_sha256.clone(),
                        verified,
                        verified_at,
                    )
                    .await
                    .expect_err("Paper Raid domain must not enter memory legacy Paper finality V1")
                };
                assert_eq!(error.status, StatusCode::CONFLICT);
                assert_eq!(error.code, "trnm_receipt_domain_lane_mismatch");
            }
        }
        assert_eq!(
            paper_finality_side_effect_snapshot(&state).await,
            before,
            "Paper Raid V2/V3 domain dispatch must precede mutation of every state surface reachable from the production memory/PG legacy finality V1 post-verification pipeline",
        );
    }

    #[tokio::test]
    async fn exact_research_accepts_and_legacy_lanes_stay_closed_in_memory() {
        exercise_legacy_lane_ordering(fixed_state()).await;
    }

    #[tokio::test]
    async fn exact_research_accepts_and_legacy_lanes_stay_closed_in_postgres() {
        let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
            eprintln!("HEPTA_TEST_DATABASE_URL unset; legacy finality PostgreSQL test skipped");
            return;
        };
        let mut lock = PgConnection::connect(&database_url)
            .await
            .expect("legacy finality PostgreSQL test lock");
        sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
            .execute(&mut lock)
            .await
            .expect("serialize Hepta PostgreSQL tests");
        let mut state = AppState::connect(&database_url, security())
            .await
            .expect("legacy finality PostgreSQL state");
        crate::paper_raid_v2::endpoint_tests::reset_postgres(&database_url).await;
        state.cometbft_local_verification_clock =
            Arc::new(|| std::time::UNIX_EPOCH + Duration::from_secs(VERIFICATION_TIME));

        exercise_legacy_lane_ordering(state).await;

        crate::paper_raid_v2::endpoint_tests::reset_postgres(&database_url).await;
        sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
            .execute(&mut lock)
            .await
            .expect("release Hepta PostgreSQL legacy finality lock");
    }

    #[tokio::test]
    async fn mismatched_research_rejects_twice_with_zero_side_effects_in_memory() {
        exercise_mismatched_research_zero_side_effects(fixed_state()).await;
    }

    #[tokio::test]
    async fn mismatched_research_rejects_twice_with_zero_side_effects_in_postgres() {
        let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
            eprintln!("HEPTA_TEST_DATABASE_URL unset; Research mismatch PostgreSQL test skipped");
            return;
        };
        let mut lock = PgConnection::connect(&database_url)
            .await
            .expect("Research mismatch PostgreSQL test lock");
        sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
            .execute(&mut lock)
            .await
            .expect("serialize Hepta PostgreSQL tests");
        let mut state = AppState::connect(&database_url, security())
            .await
            .expect("Research mismatch PostgreSQL state");
        crate::paper_raid_v2::endpoint_tests::reset_postgres(&database_url).await;
        state.cometbft_local_verification_clock =
            Arc::new(|| std::time::UNIX_EPOCH + Duration::from_secs(VERIFICATION_TIME));

        exercise_mismatched_research_zero_side_effects(state).await;

        crate::paper_raid_v2::endpoint_tests::reset_postgres(&database_url).await;
        sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
            .execute(&mut lock)
            .await
            .expect("release Research mismatch PostgreSQL lock");
    }

    #[tokio::test]
    async fn paper_raid_v2_v3_reject_before_all_memory_side_effects() {
        exercise_paper_raid_lane_guard_zero_side_effects(fixed_state()).await;
    }

    #[tokio::test]
    async fn paper_raid_v2_v3_reject_before_all_postgres_side_effects() {
        let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
            eprintln!("HEPTA_TEST_DATABASE_URL unset; Paper Raid lane PostgreSQL test skipped");
            return;
        };
        let mut lock = PgConnection::connect(&database_url)
            .await
            .expect("Paper Raid lane PostgreSQL test lock");
        sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
            .execute(&mut lock)
            .await
            .expect("serialize Hepta PostgreSQL tests");
        let state = AppState::connect(&database_url, security())
            .await
            .expect("Paper Raid lane PostgreSQL state");
        crate::paper_raid_v2::endpoint_tests::reset_postgres(&database_url).await;

        exercise_paper_raid_lane_guard_zero_side_effects(state).await;

        crate::paper_raid_v2::endpoint_tests::reset_postgres(&database_url).await;
        sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
            .execute(&mut lock)
            .await
            .expect("release Paper Raid lane PostgreSQL lock");
    }

    #[tokio::test]
    async fn trust_anchor_auth_precedes_body_read_and_exact_replay_is_stable() {
        let state = fixed_state();
        let router = app(state.clone());
        let oversized = (MAX_COMETBFT_TRUST_ANCHOR_V1_WIRE_BYTES + 1).to_string();
        let (status, _) = raw_request(
            router.clone(),
            "/v2/hepta/operator/trnm/trust-anchors",
            None,
            Vec::new(),
            Some(oversized.clone()),
            0,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "auth must run before size checks"
        );
        let (status, _) = raw_request(
            router.clone(),
            "/v2/hepta/operator/trnm/trust-anchors",
            Some((OPERATOR_TOKEN_HEADER, "operator")),
            Vec::new(),
            Some(oversized),
            0,
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

        let first = raw_request(
            router.clone(),
            "/v2/hepta/operator/trnm/trust-anchors",
            Some((OPERATOR_TOKEN_HEADER, "operator")),
            canonical_fixture_payload(ANCHOR_FILE).to_vec(),
            None,
            0,
        )
        .await;
        let replay = raw_request(
            router,
            "/v2/hepta/operator/trnm/trust-anchors",
            Some((OPERATOR_TOKEN_HEADER, "operator")),
            canonical_fixture_payload(ANCHOR_FILE).to_vec(),
            None,
            0,
        )
        .await;
        assert_eq!(first.0, StatusCode::CREATED);
        assert_eq!(replay.0, StatusCode::OK);
        assert_eq!(
            first.1, replay.1,
            "anchor replay must return stored admission time"
        );
    }

    #[tokio::test]
    async fn receipt_v2_auth_and_anchor_header_precede_body_limit() {
        let router = app(fixed_state());
        let uri = format!("/v2/hepta/papers/{}/chain-finality", Uuid::new_v4());
        let oversized = (DEFAULT_TRNM_RECEIPT_V2_MAX_BODY_BYTES + 1).to_string();

        let (status, error) = raw_request(
            router.clone(),
            &uri,
            None,
            Vec::new(),
            Some(oversized.clone()),
            1,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{error}");
        assert_eq!(error["code"], "trnm_auth_failed");

        let (status, error) = raw_request(
            router.clone(),
            &uri,
            Some((TRNM_TOKEN_HEADER, "trnm")),
            Vec::new(),
            Some(oversized.clone()),
            0,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(error["code"], "trnm_trust_anchor_hash_required");

        let (status, error) = raw_request(
            router,
            &uri,
            Some((TRNM_TOKEN_HEADER, "trnm")),
            Vec::new(),
            Some(oversized),
            1,
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{error}");
        assert_eq!(error["code"], "request_body_too_large");
    }

    #[tokio::test]
    async fn receipt_v2_deployment_cap_accepts_max_and_rejects_max_plus_one() {
        let receipt = canonical_fixture_payload(RECEIPT_FILE);
        let cap = receipt.len();
        let router = app(fixed_state_with_receipt_v2_limits(cap, 1));
        let uri = format!("/v2/hepta/papers/{}/chain-finality", Uuid::new_v4());

        let (status, error) = raw_request(
            router.clone(),
            &uri,
            Some((TRNM_TOKEN_HEADER, "trnm")),
            receipt.to_vec(),
            Some(cap.to_string()),
            1,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{error}");
        assert_eq!(error["code"], "trnm_trust_anchor_not_admitted");

        let (status, error) = raw_request(
            router.clone(),
            &uri,
            Some((TRNM_TOKEN_HEADER, "trnm")),
            receipt.to_vec(),
            Some((cap + 1).to_string()),
            1,
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{error}");
        assert_eq!(error["code"], "request_body_too_large");

        let (status, error) = raw_request(
            router,
            &uri,
            Some((TRNM_TOKEN_HEADER, "trnm")),
            vec![b' '; cap + 1],
            None,
            1,
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{error}");
        assert_eq!(error["code"], "request_body_too_large");
    }

    #[tokio::test]
    async fn receipt_v2_concurrent_permit_rejects_busy_before_body_read() {
        let state = fixed_state_with_receipt_v2_limits(1024, 2);
        assert_eq!(
            state.paper_chain_verification_permits.available_permits(),
            2
        );
        let first_held_permit = state
            .paper_chain_verification_permits
            .clone()
            .try_acquire_owned()
            .expect("reserve the first Receipt V2 verification permit");
        let second_held_permit = state
            .paper_chain_verification_permits
            .clone()
            .try_acquire_owned()
            .expect("reserve the second Receipt V2 verification permit");
        let router = app(state);
        let uri = format!("/v2/hepta/papers/{}/chain-finality", Uuid::new_v4());

        let (status, error) = raw_request(
            router.clone(),
            &uri,
            Some((TRNM_TOKEN_HEADER, "trnm")),
            Vec::new(),
            Some("1025".to_string()),
            1,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{error}");
        assert_eq!(error["code"], "trnm_receipt_v2_verification_busy");

        drop(first_held_permit);
        let (status, error) = raw_request(
            router,
            &uri,
            Some((TRNM_TOKEN_HEADER, "trnm")),
            Vec::new(),
            Some("1025".to_string()),
            1,
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{error}");
        assert_eq!(error["code"], "request_body_too_large");
        drop(second_held_permit);
    }

    #[test]
    fn receipt_v2_ingress_configuration_fails_closed_outside_bounds() {
        let defaults = SecurityConfig::new("operator", "nakama");
        assert_eq!(
            defaults.trnm_receipt_v2_max_body_bytes,
            DEFAULT_TRNM_RECEIPT_V2_MAX_BODY_BYTES
        );
        assert_eq!(
            defaults.trnm_receipt_v2_max_in_flight,
            DEFAULT_TRNM_RECEIPT_V2_MAX_IN_FLIGHT
        );
        assert!(SecurityConfig::new("operator", "nakama")
            .with_trnm_receipt_v2_ingress_limits(
                MAX_TRNM_RECEIPT_V2_DEPLOYMENT_BODY_BYTES,
                MAX_TRNM_RECEIPT_V2_MAX_IN_FLIGHT,
            )
            .is_ok());
        for (max_body_bytes, max_in_flight) in [
            (0, 1),
            (MAX_TRNM_RECEIPT_V2_DEPLOYMENT_BODY_BYTES + 1, 1),
            (1, 0),
            (1, MAX_TRNM_RECEIPT_V2_MAX_IN_FLIGHT + 1),
        ] {
            assert!(SecurityConfig::new("operator", "nakama")
                .with_trnm_receipt_v2_ingress_limits(max_body_bytes, max_in_flight)
                .is_err());
        }
        for raw in ["", "0", "01", "+1", " 1", "1 ", "five"] {
            assert!(crate::parse_bounded_positive_decimal("receipt_limit", raw, 4).is_err());
        }
        assert_eq!(
            crate::parse_bounded_positive_decimal("receipt_limit", "4", 4),
            Ok(4)
        );
        assert!(crate::parse_bounded_positive_decimal("receipt_limit", "5", 4).is_err());
    }

    #[tokio::test]
    async fn postgres_projection_persists_evaluation_binding() {
        let Ok(database_url) = std::env::var("HEPTA_TEST_DATABASE_URL") else {
            eprintln!("HEPTA_TEST_DATABASE_URL unset; finality projection PostgreSQL test skipped");
            return;
        };
        let mut lock = PgConnection::connect(&database_url)
            .await
            .expect("PostgreSQL finality projection test lock");
        sqlx::query("select pg_advisory_lock(hashtext('hepta-research-league-pg-tests'))")
            .execute(&mut lock)
            .await
            .expect("serialize Hepta PostgreSQL tests");
        let state = AppState::connect(&database_url, security())
            .await
            .expect("finality projection PostgreSQL state");
        crate::paper_raid_v2::endpoint_tests::reset_postgres(&database_url).await;
        let binding =
            crate::paper_raid_v2::endpoint_tests::seed_paper_chain_finality_test(state.clone())
                .await;
        let pool = state.pool.as_ref().expect("PostgreSQL pool");
        let projection = PaperChainFinalityProjectionV1 {
            schema: PAPER_CHAIN_FINALITY_PROJECTION_SCHEMA_V1.to_string(),
            paper_project_id: binding.paper_project_id,
            submission_id: binding.submission_id,
            evaluation_id: binding.evaluation_id,
            local_command_id: Uuid::new_v4(),
            command_idempotency_key: "finality-projection-postgres-command".to_string(),
            command_fingerprint: digest(0x81),
            paper_binding_fingerprint: digest(0x82),
            receipt_hash: "83".repeat(32),
            trust_anchor_hash: ANCHOR_HASH.to_string(),
            chain_id: "trnm-comet-spike".to_string(),
            comet_tx_hash: "84".repeat(32),
            transaction_index: 0,
            execution_height: 7,
            commitment_height: 8,
            commitment_header_hash: "85".repeat(32),
            app_hash: "86".repeat(32),
            status: PaperChainFinalityStatusV1::VerifiedFinality,
            ranking_eligible: false,
            reward_eligible: false,
            score_eligible: false,
            economic_eligible: false,
            version: 1,
            verified_at: Utc::now(),
        };
        let projection_json = serde_json::to_value(&projection).expect("projection JSON");
        sqlx::query(
            "insert into hepta_trnm_cometbft_trust_anchors (
                anchor_hash,chain_id,trusted_height,canonical_anchor,canonical_sha256,admitted_at
             ) values ($1,$2,1,$3,$1,now()) on conflict (anchor_hash) do nothing",
        )
        .bind(ANCHOR_HASH)
        .bind(&projection.chain_id)
        .bind(canonical_fixture_payload(ANCHOR_FILE))
        .execute(pool)
        .await
        .expect("trust anchor fixture");
        sqlx::query(
            "insert into hepta_paper_chain_receipts (
                receipt_hash,paper_project_id,local_command_id,command_idempotency_key,
                command_fingerprint,paper_binding_fingerprint,anchor_hash,chain_id,
                execution_height,commitment_height,comet_tx_hash,app_hash,
                canonical_receipt,canonical_sha256,verified_at,record_json
             ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16::jsonb)",
        )
        .bind(&projection.receipt_hash)
        .bind(projection.paper_project_id)
        .bind(projection.local_command_id)
        .bind(&projection.command_idempotency_key)
        .bind(&projection.command_fingerprint)
        .bind(&projection.paper_binding_fingerprint)
        .bind(&projection.trust_anchor_hash)
        .bind(&projection.chain_id)
        .bind(i64_from_u64(projection.execution_height, "execution_height").unwrap())
        .bind(i64_from_u64(projection.commitment_height, "commitment_height").unwrap())
        .bind(&projection.comet_tx_hash)
        .bind(&projection.app_hash)
        .bind(canonical_fixture_payload(RECEIPT_FILE))
        .bind(sha256_hex(canonical_fixture_payload(RECEIPT_FILE)))
        .bind(projection.verified_at)
        .bind(&projection_json)
        .execute(pool)
        .await
        .expect("receipt fixture");
        let mut tx = pool.begin().await.expect("projection transaction");
        insert_finality_projection_postgres(&mut tx, &projection, &projection_json)
            .await
            .expect("persist verified projection");
        tx.commit().await.expect("commit verified projection");
        let stored_evaluation_id: Uuid = sqlx::query_scalar(
            "select evaluation_id from hepta_paper_chain_finality_projections
             where local_command_id=$1",
        )
        .bind(projection.local_command_id)
        .fetch_one(pool)
        .await
        .expect("stored projection evaluation binding");
        assert_eq!(stored_evaluation_id, binding.evaluation_id);
        crate::paper_raid_v2::endpoint_tests::reset_postgres(&database_url).await;
        sqlx::query("select pg_advisory_unlock(hashtext('hepta-research-league-pg-tests'))")
            .execute(&mut lock)
            .await
            .expect("release Hepta PostgreSQL finality projection lock");
    }

    #[tokio::test]
    async fn raw_canonical_and_header_boundaries_fail_closed() {
        let router = app(fixed_state());
        let mut unknown_anchor: Value =
            serde_json::from_slice(canonical_fixture_payload(ANCHOR_FILE)).expect("anchor JSON");
        unknown_anchor["unknown"] = json!(true);
        let (status, _) = raw_request(
            router.clone(),
            "/v2/hepta/operator/trnm/trust-anchors",
            Some((OPERATOR_TOKEN_HEADER, "operator")),
            serde_json::to_vec(&unknown_anchor).expect("anchor bytes"),
            None,
            0,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let paper_id = Uuid::new_v4();
        let (status, _) = raw_request(
            router.clone(),
            &format!("/v2/hepta/papers/{paper_id}/chain-finality"),
            Some((TRNM_TOKEN_HEADER, "trnm")),
            canonical_fixture_payload(RECEIPT_FILE).to_vec(),
            None,
            2,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let mut unknown_receipt: Value =
            serde_json::from_slice(canonical_fixture_payload(RECEIPT_FILE)).expect("receipt JSON");
        unknown_receipt["unknown"] = json!(true);
        let (status, _) = raw_request(
            router,
            &format!("/v2/hepta/papers/{paper_id}/chain-finality"),
            Some((TRNM_TOKEN_HEADER, "trnm")),
            serde_json::to_vec(&unknown_receipt).expect("receipt bytes"),
            None,
            1,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn server_clock_expires_anchor_and_fixture_tamper_is_rejected() {
        let receipt = CometBftAppHashFinalityReceiptV2::from_canonical_bytes(
            canonical_fixture_payload(RECEIPT_FILE),
        )
        .expect("canonical receipt");
        let pinned = HashSet::from([ANCHOR_HASH.to_string()]);
        let verified = verify_receipt(
            &receipt,
            canonical_fixture_payload(ANCHOR_FILE),
            ANCHOR_HASH,
            &pinned,
            std::time::UNIX_EPOCH + Duration::from_secs(VERIFICATION_TIME),
        )
        .expect("fixture verifies at server policy time");
        assert_eq!(verified.command_id, receipt.command_id);
        let expired = verify_receipt(
            &receipt,
            canonical_fixture_payload(ANCHOR_FILE),
            ANCHOR_HASH,
            &pinned,
            std::time::UNIX_EPOCH + Duration::from_secs(VERIFICATION_TIME + 172_800),
        )
        .expect_err("expired anchor must fail closed");
        assert_eq!(expired.code, "trnm_receipt_v2_untrusted");

        let mut tampered: Value =
            serde_json::from_slice(canonical_fixture_payload(RECEIPT_FILE)).expect("receipt JSON");
        tampered["command_id"] = json!("00".repeat(32));
        assert!(CometBftAppHashFinalityReceiptV2::from_canonical_bytes(
            &serde_json::to_vec(&tampered).expect("tampered bytes")
        )
        .is_err());
    }

    fn digest(byte: u8) -> String {
        format!("sha256:{}", format!("{byte:02x}").repeat(32))
    }

    fn binding() -> PaperTrnmCommandBindingV1 {
        let mut binding = PaperTrnmCommandBindingV1 {
            schema: PAPER_TRNM_COMMAND_BINDING_SCHEMA_V1.to_string(),
            paper_project_id: Uuid::from_u128(0x100),
            submission_id: Uuid::from_u128(0x200),
            evaluation_id: Uuid::from_u128(0x300),
            research_session_id: "paper-finality-test-session".to_string(),
            research_session_roster_version: 1,
            match_evidence_commitment_id: digest(0x77),
            match_evidence_object_version: PAPER_TRNM_MATCH_EVIDENCE_OBJECT_VERSION_V1,
            release_candidate_hash: digest(0x11),
            paper_bundle_hash: digest(0x22),
            submission_commitment_hash: digest(0x33),
            tolerance_policy_hash: digest(0x44),
            evaluation_signing_hash: digest(0x55),
            reproduction_id: Uuid::from_u128(0x400),
            reproduction_report_hash: digest(0x66),
            evaluation_score_bps: 8_500,
            evaluation_accepted: true,
            evaluation_completed_at_unix_s: 1_753_449_600,
        };
        binding.submission_commitment_hash =
            paper_trnm_submission_commitment_hash(&binding).expect("submission commitment");
        binding
    }

    fn signed_command(
        binding: &PaperTrnmCommandBindingV1,
        command_id: ExternalKey,
    ) -> SignedResearchCommandV1 {
        let command = ResearchCommandV1::EvaluationCommitment(EvaluationCommitmentV1 {
            evaluation_id: ExternalKey::from_uuid(
                "hepta.paper_raid.evaluation",
                &binding.evaluation_id.to_string(),
            )
            .expect("evaluation key"),
            match_evidence_ref: paper_trnm_match_evidence_object_ref(binding)
                .expect("MatchEvidence object ref"),
            submission_hash: crate::decode_digest(&binding.submission_commitment_hash).unwrap(),
            rubric_hash: crate::decode_digest(&binding.tolerance_policy_hash).unwrap(),
            evaluation_hash: crate::decode_digest(&binding.evaluation_signing_hash).unwrap(),
            reproduction_hash: Some(
                crate::decode_digest(&binding.reproduction_report_hash).unwrap(),
            ),
            score_bps: binding.evaluation_score_bps,
            accepted: binding.evaluation_accepted,
            completed_at_unix_s: binding.evaluation_completed_at_unix_s,
        });
        SignedResearchCommandV1::sign(
            "trnm-comet-spike".to_string(),
            command_id,
            "did:trnm:hepta-authority".to_string(),
            AuthorityRole::HeptaAuthority,
            1,
            command,
            &SigningKey::from_bytes(&[0x42; 32]),
        )
        .expect("signed command")
    }

    #[test]
    fn paper_binding_is_signed_and_domain_id_never_uses_local_uuid() {
        let binding = binding();
        let domain_id = ExternalKey::from_bytes([0x88; 32]);
        let signed = signed_command(&binding, domain_id);
        validate_signed_paper_binding(&signed, &binding).expect("full signed binding");
        let mut tampered = binding.clone();
        tampered.evaluation_score_bps += 1;
        assert_eq!(
            validate_signed_paper_binding(&signed, &tampered)
                .expect_err("score sidecar tamper")
                .code,
            "paper_trnm_signed_semantics_mismatch"
        );

        let command = TrnmCommand {
            command_id: Uuid::new_v4(),
            kind: TrnmCommandKind::EvaluationCommitment,
            aggregate_id: signed.command.primary_object_ref().key.to_hex(),
            idempotency_key: domain_id.to_hex(),
            command_fingerprint: crate::trnm_v1::format_digest(&signed.command_fingerprint()),
            paper_binding: Some(binding.clone()),
            paper_binding_fingerprint: Some(
                paper_binding_fingerprint(&binding).expect("binding fingerprint"),
            ),
            signed_command: signed,
            status: TrnmProjectionStatus::PendingFinality,
            created_at: Utc::now(),
        };
        let verified = VerifiedCometBftReceiptV2 {
            receipt_hash_hex: "aa".repeat(32),
            chain_id: "trnm-comet-spike".to_string(),
            command_id: domain_id.to_hex(),
            command_fingerprint_hex: command
                .command_fingerprint
                .strip_prefix("sha256:")
                .unwrap()
                .to_string(),
            comet_tx_hash_hex: "bb".repeat(32),
            transaction_index: 0,
            applied_command_object_key_hex: "cc".repeat(32),
            execution_height: 7,
            commitment_height: 8,
            commitment_header_hash_hex: "dd".repeat(32),
            app_hash_hex: "ee".repeat(32),
            domain_command: VerifiedCometBftDomainCommandV2::ResearchV1(Box::new(
                command.signed_command.clone(),
            )),
        };
        assert_eq!(
            validate_command_identity(&command, binding.paper_project_id, &verified)
                .expect("domain command id binds idempotency key"),
            &binding
        );
        let mut wrong = verified;
        wrong.command_id = command.command_id.simple().to_string();
        assert_eq!(
            validate_command_identity(&command, binding.paper_project_id, &wrong)
                .expect_err("local UUID must never bind receipt command")
                .code,
            "trnm_receipt_command_id_mismatch"
        );
    }
}
