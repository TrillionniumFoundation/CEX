use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use trnm_finality_types::CometBftLightFinalityProofV1;
use trnm_finality_verifier::{
    verify_cometbft_light_finality_proof_v1_with_trust_anchor, ChainTimeVerificationOutcomeV1,
    ValidatedCometBftTrustAnchorV1,
};
use uuid::Uuid;

use crate::{
    paper_chain_finality_v1::{
        read_limited_body, validate_current_binding_records, validate_match_evidence_binding,
        validated_trust_anchor_memory_v1, PaperTrnmCommandBindingV1,
        PAPER_TRNM_COMMAND_BINDING_SCHEMA_V1, PAPER_TRNM_MATCH_EVIDENCE_OBJECT_VERSION_V1,
    },
    paper_raid_contracts::{canonical_json_sha256, paper_bundle_hash, PaperBundleAuthorConsentV2},
    paper_raid_v2::{
        AppealOutcome, JointPaperSubmission, PaperAppeal, PaperAppealResolution, PaperEvaluation,
        PaperEvaluationStatus, PaperProject, PaperReproduction, ReproductionStatus,
        ResearchSessionAuthorizationSetV1,
    },
    require_service_token, sha256_hex, ApiError, AppState, SecurityConfig, OPERATOR_TOKEN_HEADER,
};

pub const PAPER_TRNM_COMMAND_BINDING_SCHEMA_V2: &str = "hepta.paper_raid.trnm_command_binding.v2";
pub const PAPER_TRNM_FINALITY_PREPARATION_SCHEMA_V2: &str =
    "hepta.paper_raid.trnm_finality_preparation.v2";
pub const PAPER_TRNM_FINALITY_WINDOW_ARM_SCHEMA_V2: &str =
    "hepta.paper_raid.trnm_finality_window_arm.v2";
pub const PAPER_TRNM_CHAIN_TIME_CHECKPOINT_SCHEMA_V1: &str =
    "hepta.paper_raid.trnm_chain_time_checkpoint.v1";
pub const PAPER_SCIENTIFIC_FINALITY_POLICY_SCHEMA_V1: &str =
    "hepta.paper_raid.scientific_finality_policy.v1";
pub const PAPER_AUTHOR_CONSENT_SET_SCHEMA_V1: &str = "hepta.paper_raid.author_consent_set.v1";
pub const PAPER_TRNM_FINALITY_V2_COMMAND_LANE: &str = "awaiting_chain_verifier_upgrade";

const PREPARE_OPERATION_V2: &str = "prepare_paper_trnm_finality_v2";
const ARM_OPERATION_V2: &str = "arm_paper_trnm_finality_window_v2";
const MAX_PREPARE_PAPER_TRNM_FINALITY_V2_BODY_BYTES: usize = 16 * 1024;
const MAX_CHAIN_TIME_CHECKPOINT_BODY_BYTES: usize = 2 * 1024 * 1024;
const CHAIN_TIME_CHECKPOINT_HASH_DOMAIN_V1: &str =
    "hepta.paper_raid.trnm_chain_time_checkpoint_hash.v1";
const PAPER_TRNM_SUBMISSION_BINDING_DOMAIN_V2: &[u8] = b"HEPTA_PAPER_TRNM_SUBMISSION_BINDING_V2\0";
const PAPER_TRNM_COMMITMENT_ID_ZERO: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// This duration is a versioned product rule, not an environment override.
/// A resolved Appeal closes its own window early, but it never bypasses the
/// requirement that the latest evaluation and latest reproduction already
/// exist and are internally consistent.
pub const PAPER_NO_APPEAL_WINDOW_SECONDS_V1: i64 = 24 * 60 * 60;
/// A fresh Chain checkpoint may trail its local observation by this bounded
/// interval.  The complete allowance is added to the Appeal deadline, so
/// clock skew or normal checkpoint transport latency can only lengthen the
/// window.  A checkpoint outside the bound fails closed at arm time.
pub const PAPER_CHAIN_TIME_MAX_LAG_MS_V1: u64 = 15 * 60 * 1_000;
const PAPER_CHAIN_TIME_ADVISORY_LOCK_KEY_V1: i64 = 0x4850_5441_5449_4d45_i64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperScientificFinalityPolicyV1 {
    pub schema: String,
    pub no_appeal_window_seconds: i64,
    pub resolved_appeal_closes_window_early: bool,
    pub require_latest_evaluation: bool,
    pub require_latest_reproduction: bool,
    pub require_no_open_appeal: bool,
    pub scientific_finality_separate_from_settlement: bool,
    pub score_eligible: bool,
    pub ranking_eligible: bool,
    pub reward_eligible: bool,
    pub economic_eligible: bool,
}

pub fn paper_scientific_finality_policy_v1() -> PaperScientificFinalityPolicyV1 {
    PaperScientificFinalityPolicyV1 {
        schema: PAPER_SCIENTIFIC_FINALITY_POLICY_SCHEMA_V1.to_string(),
        no_appeal_window_seconds: PAPER_NO_APPEAL_WINDOW_SECONDS_V1,
        resolved_appeal_closes_window_early: true,
        require_latest_evaluation: true,
        require_latest_reproduction: true,
        require_no_open_appeal: true,
        scientific_finality_separate_from_settlement: true,
        score_eligible: false,
        ranking_eligible: false,
        reward_eligible: false,
        economic_eligible: false,
    }
}

pub(crate) fn paper_scientific_finality_policy_hash_v1() -> Result<String, ApiError> {
    canonical_json_sha256(&paper_scientific_finality_policy_v1()).map_err(|message| {
        ApiError::internal(format!(
            "encode frozen Paper scientific-finality policy: {message}"
        ))
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaperTrnmAppealStatusV2 {
    ClosedNoAppeal,
    ResolvedDenied,
    ResolvedUpheld,
}

impl PaperTrnmAppealStatusV2 {
    fn as_database_str(&self) -> &'static str {
        match self {
            Self::ClosedNoAppeal => "closed_no_appeal",
            Self::ResolvedDenied => "resolved_denied",
            Self::ResolvedUpheld => "resolved_upheld",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperTrnmCommandBindingV2 {
    pub schema: String,
    pub commitment_id: String,
    pub source_fingerprint: String,
    pub window_arm_id: Uuid,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub research_session_id: String,
    pub research_session_roster_version: u64,
    pub match_evidence_commitment_id: String,
    pub match_evidence_object_version: u64,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub submission_commitment_hash: String,
    pub author_consent_set_hash: String,
    pub tolerance_policy_hash: String,
    pub evaluation_id: Uuid,
    pub evaluation_signing_hash: String,
    pub evaluation_score_bps: u16,
    pub evaluation_accepted: bool,
    pub evaluation_completed_at_unix_s: u64,
    pub evaluation_supersedes_evaluation_id: Option<Uuid>,
    pub evaluation_superseded_by_evaluation_id: Option<Uuid>,
    pub latest_reproduction_id: Uuid,
    pub latest_reproduction_report_hash: String,
    pub latest_reproduction_accepted: bool,
    pub latest_reproduction_completed_at_unix_s: u64,
    pub reproduction_supersedes_reproduction_id: Option<Uuid>,
    pub reproduction_superseded_by_reproduction_id: Option<Uuid>,
    pub appeal_status: PaperTrnmAppealStatusV2,
    pub appeal_id: Option<Uuid>,
    pub appealed_evaluation_id: Option<Uuid>,
    pub appeal_resolution_id: Option<Uuid>,
    pub appeal_resolution_hash: Option<String>,
    pub start_checkpoint_hash: String,
    pub start_checkpoint_anchor_hash: String,
    pub start_checkpoint_chain_id: String,
    pub start_checkpoint_height: u64,
    pub start_checkpoint_header_hash: String,
    pub start_checkpoint_consensus_time_unix_ms: u64,
    pub final_checkpoint_hash: String,
    pub final_checkpoint_anchor_hash: String,
    pub final_checkpoint_chain_id: String,
    pub final_checkpoint_height: u64,
    pub final_checkpoint_header_hash: String,
    pub final_checkpoint_consensus_time_unix_ms: u64,
    pub max_chain_time_lag_ms: u64,
    pub appeal_window_closes_at_unix_ms: u64,
    pub appeal_window_closes_at_unix_s: u64,
    pub settlement_policy_hash: String,
    pub scientific_finality: bool,
    pub score_eligible: bool,
    pub ranking_eligible: bool,
    pub reward_eligible: bool,
    pub economic_eligible: bool,
    pub finalized_at_unix_s: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaperTrnmFinalityPreparationStatusV2 {
    AwaitingChainVerifierUpgrade,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperTrnmFinalityPreparationV2 {
    pub schema: String,
    pub preparation_id: Uuid,
    pub idempotency_key: String,
    pub request_hash: String,
    pub binding: PaperTrnmCommandBindingV2,
    pub binding_fingerprint: String,
    pub status: PaperTrnmFinalityPreparationStatusV2,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperTrnmFinalityWindowArmV2 {
    pub schema: String,
    pub arm_id: Uuid,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub evaluation_id: Uuid,
    pub latest_reproduction_id: Uuid,
    pub research_session_id: String,
    pub research_session_roster_version: u64,
    pub source_fingerprint: String,
    pub appeal_status: PaperTrnmAppealStatusV2,
    pub appeal_id: Option<Uuid>,
    pub appeal_resolution_id: Option<Uuid>,
    pub start_checkpoint: PaperTrnmChainTimeCheckpointV1,
    pub observed_max_checkpoint_height: u64,
    pub max_chain_time_lag_ms: u64,
    pub earliest_final_checkpoint_time_unix_ms: u64,
    pub idempotency_key: String,
    pub request_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperTrnmChainTimeCheckpointV1 {
    pub schema: String,
    pub checkpoint_hash: String,
    pub trust_anchor_hash: String,
    pub chain_id: String,
    pub height: u64,
    pub header_hash: String,
    pub consensus_time_unix_ms: u64,
    pub canonical_proof_sha256: String,
    pub locally_verified_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AdmitPaperTrnmChainTimeCheckpointV1Request {
    pub trust_anchor_hash: String,
    pub proof: CometBftLightFinalityProofV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArmPaperTrnmFinalityWindowV2Request {
    pub submission_id: Uuid,
    pub evaluation_id: Uuid,
    pub latest_reproduction_id: Uuid,
    pub research_session_id: String,
    pub research_session_roster_version: u64,
    pub start_checkpoint_hash: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreparePaperTrnmFinalityV2Request {
    pub arm_id: Uuid,
    pub submission_id: Uuid,
    pub evaluation_id: Uuid,
    pub latest_reproduction_id: Uuid,
    pub research_session_id: String,
    pub research_session_roster_version: u64,
    pub final_checkpoint_hash: String,
    pub idempotency_key: String,
}

#[derive(Clone, Default)]
pub(crate) struct PaperChainFinalityPreparationMemoryV2 {
    pub(crate) time_checkpoints_by_hash: HashMap<String, PaperTrnmChainTimeCheckpointV1>,
    pub(crate) time_checkpoint_hash_by_chain_height: HashMap<(String, u64), String>,
    pub(crate) arms_by_idempotency_key: HashMap<String, PaperTrnmFinalityWindowArmV2>,
    pub(crate) arms_by_id: HashMap<Uuid, PaperTrnmFinalityWindowArmV2>,
    pub(crate) arms_by_source_fingerprint: HashMap<String, PaperTrnmFinalityWindowArmV2>,
    pub(crate) by_idempotency_key: HashMap<String, PaperTrnmFinalityPreparationV2>,
    pub(crate) by_commitment_id: HashMap<String, PaperTrnmFinalityPreparationV2>,
    pub(crate) by_paper_id: HashMap<Uuid, PaperTrnmFinalityPreparationV2>,
}

#[derive(Clone)]
struct PaperFinalityFactsV2 {
    paper: PaperProject,
    submission: JointPaperSubmission,
    evaluations: Vec<PaperEvaluation>,
    reproductions: Vec<PaperReproduction>,
    appeals: Vec<PaperAppeal>,
    resolutions: Vec<PaperAppealResolution>,
    authorization_sets: Vec<ResearchSessionAuthorizationSetV1>,
    completions: Vec<crate::paper_raid_contracts::SignedNakamaCompletionReceiptV1>,
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/v2/hepta/operator/trnm/time-checkpoints",
            post(admit_paper_trnm_chain_time_checkpoint_v1),
        )
        .route(
            "/v2/hepta/papers/:paper_id/chain-finality-v2/arm",
            post(arm_paper_trnm_finality_window_v2),
        )
        .route(
            "/v2/hepta/papers/:paper_id/chain-finality-v2/prepare",
            post(prepare_paper_trnm_finality_v2),
        )
}

async fn admit_paper_trnm_chain_time_checkpoint_v1(
    State(state): State<AppState>,
    request: Request,
) -> Result<(StatusCode, Json<PaperTrnmChainTimeCheckpointV1>), ApiError> {
    require_service_token(
        request.headers(),
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    let body = read_limited_body(request, MAX_CHAIN_TIME_CHECKPOINT_BODY_BYTES).await?;
    let request: AdmitPaperTrnmChainTimeCheckpointV1Request = serde_json::from_slice(&body)
        .map_err(|error| {
            ApiError::bad_request(
                "invalid_paper_trnm_chain_time_checkpoint_request",
                format!("Paper Chain-time checkpoint JSON is invalid: {error}"),
            )
        })?;
    validate_anchor_hash(&request.trust_anchor_hash)?;
    let checkpoint_hash = chain_time_checkpoint_hash_v1(&request)?;
    let local_verification_time = (state.cometbft_local_verification_clock)();
    if state.finality_pool.is_some() {
        admit_paper_trnm_chain_time_checkpoint_postgres(
            &state,
            request,
            checkpoint_hash,
            local_verification_time,
        )
        .await
    } else {
        admit_paper_trnm_chain_time_checkpoint_memory(
            &state,
            request,
            checkpoint_hash,
            local_verification_time,
        )
        .await
    }
}

async fn admit_paper_trnm_chain_time_checkpoint_memory(
    state: &AppState,
    request: AdmitPaperTrnmChainTimeCheckpointV1Request,
    checkpoint_hash: String,
    local_verification_time: SystemTime,
) -> Result<(StatusCode, Json<PaperTrnmChainTimeCheckpointV1>), ApiError> {
    let mut finality = state.paper_chain_finality.write().await;
    if let Some(existing) = finality
        .preparations_v2
        .time_checkpoints_by_hash
        .get(&checkpoint_hash)
    {
        return Ok((StatusCode::OK, Json(existing.clone())));
    }
    let local_verification_time_unix_ms =
        system_time_unix_millis_v2(local_verification_time, "local verification time")?;
    enforce_local_verification_high_water_v1(
        finality
            .preparations_v2
            .time_checkpoints_by_hash
            .values()
            .map(|checkpoint| checkpoint.locally_verified_at_unix_ms)
            .max(),
        local_verification_time_unix_ms,
    )?;
    let anchor = validated_trust_anchor_memory_v1(
        &finality,
        &request.trust_anchor_hash,
        &state.security.pinned_trnm_cometbft_trust_anchor_hashes,
    )?;
    let checkpoint = verify_chain_time_checkpoint_v1(
        &request,
        &anchor,
        checkpoint_hash.clone(),
        local_verification_time,
        local_verification_time_unix_ms,
    )?;
    let latest_chain_checkpoint = finality
        .preparations_v2
        .time_checkpoints_by_hash
        .values()
        .filter(|stored| stored.chain_id == checkpoint.chain_id)
        .max_by_key(|stored| stored.height)
        .map(|stored| (stored.height, stored.consensus_time_unix_ms));
    validate_chain_time_checkpoint_progress_v1(latest_chain_checkpoint, &checkpoint)?;
    let chain_height_key = (checkpoint.chain_id.clone(), checkpoint.height);
    match finality
        .preparations_v2
        .time_checkpoint_hash_by_chain_height
        .entry(chain_height_key)
    {
        Entry::Occupied(_) => {
            return Err(ApiError::conflict(
                "paper_trnm_chain_time_height_conflict",
                "this Chain height already has a different authenticated time checkpoint",
            ));
        }
        Entry::Vacant(entry) => {
            entry.insert(checkpoint_hash.clone());
        }
    }
    finality
        .preparations_v2
        .time_checkpoints_by_hash
        .insert(checkpoint_hash, checkpoint.clone());
    Ok((StatusCode::CREATED, Json(checkpoint)))
}

async fn admit_paper_trnm_chain_time_checkpoint_postgres(
    state: &AppState,
    request: AdmitPaperTrnmChainTimeCheckpointV1Request,
    checkpoint_hash: String,
    local_verification_time: SystemTime,
) -> Result<(StatusCode, Json<PaperTrnmChainTimeCheckpointV1>), ApiError> {
    let pool = state.finality_pool.as_ref().expect("PostgreSQL checked");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    // Serializes checkpoint admission so the persisted local-clock high-water
    // and same-height conflict checks cannot race each other.
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(PAPER_CHAIN_TIME_ADVISORY_LOCK_KEY_V1)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let canonical_proof = serde_json::to_value(&request.proof).map_err(|error| {
        ApiError::internal(format!("encode Paper Chain-time checkpoint proof: {error}"))
    })?;
    if let Some(row) = sqlx::query(
        "select record_json,canonical_proof from hepta_trnm_cometbft_time_checkpoints_v1
         where checkpoint_hash=$1",
    )
    .bind(&checkpoint_hash)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    {
        let existing: PaperTrnmChainTimeCheckpointV1 =
            decode_record(row.get("record_json"), "Paper Chain-time checkpoint")?;
        let stored_proof: Value = row.get("canonical_proof");
        if stored_proof != canonical_proof
            || existing.trust_anchor_hash != request.trust_anchor_hash
        {
            return Err(ApiError::conflict(
                "paper_trnm_chain_time_checkpoint_replay_drift",
                "checkpoint hash does not reproduce its immutable proof and trust anchor",
            ));
        }
        tx.commit().await.map_err(ApiError::database)?;
        return Ok((StatusCode::OK, Json(existing)));
    }
    let local_verification_time_unix_ms =
        system_time_unix_millis_v2(local_verification_time, "local verification time")?;
    let local_high_water = sqlx::query(
        "select max(locally_verified_at_unix_ms) as high_water
         from hepta_trnm_cometbft_time_checkpoints_v1",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .get::<Option<i64>, _>("high_water")
    .map(|value| {
        u64::try_from(value)
            .map_err(|_| ApiError::internal("stored local verification high-water is negative"))
    })
    .transpose()?;
    enforce_local_verification_high_water_v1(local_high_water, local_verification_time_unix_ms)?;
    let anchor = validated_trust_anchor_postgres_v1(
        &mut tx,
        &request.trust_anchor_hash,
        &state.security.pinned_trnm_cometbft_trust_anchor_hashes,
    )
    .await?;
    let checkpoint = verify_chain_time_checkpoint_v1(
        &request,
        &anchor,
        checkpoint_hash.clone(),
        local_verification_time,
        local_verification_time_unix_ms,
    )?;
    let latest_chain_checkpoint = sqlx::query(
        "select height,consensus_time_unix_ms
         from hepta_trnm_cometbft_time_checkpoints_v1
         where chain_id=$1
         order by height desc
         limit 1",
    )
    .bind(&checkpoint.chain_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .map(|row| {
        let height = u64::try_from(row.get::<i64, _>("height"))
            .map_err(|_| ApiError::internal("stored Chain checkpoint height is negative"))?;
        let consensus_time_unix_ms = u64::try_from(row.get::<i64, _>("consensus_time_unix_ms"))
            .map_err(|_| {
                ApiError::internal("stored Chain checkpoint consensus time is negative")
            })?;
        Ok((height, consensus_time_unix_ms))
    })
    .transpose()?;
    validate_chain_time_checkpoint_progress_v1(latest_chain_checkpoint, &checkpoint)?;
    let record_json = serde_json::to_value(&checkpoint).map_err(|error| {
        ApiError::internal(format!("encode Paper Chain-time checkpoint: {error}"))
    })?;
    let insert = sqlx::query(
        "insert into hepta_trnm_cometbft_time_checkpoints_v1 (
            checkpoint_hash,trust_anchor_hash,chain_id,height,header_hash,
            consensus_time_unix_ms,canonical_proof,canonical_proof_sha256,
            locally_verified_at_unix_ms,record_json
         ) values ($1,$2,$3,$4,$5,$6,$7::jsonb,$8,$9,$10::jsonb)",
    )
    .bind(&checkpoint.checkpoint_hash)
    .bind(&checkpoint.trust_anchor_hash)
    .bind(&checkpoint.chain_id)
    .bind(i64_from_u64_v2(checkpoint.height, "checkpoint height")?)
    .bind(&checkpoint.header_hash)
    .bind(i64_from_u64_v2(
        checkpoint.consensus_time_unix_ms,
        "checkpoint consensus time",
    )?)
    .bind(&canonical_proof)
    .bind(&checkpoint.canonical_proof_sha256)
    .bind(i64_from_u64_v2(
        checkpoint.locally_verified_at_unix_ms,
        "local verification time",
    )?)
    .bind(&record_json)
    .execute(&mut *tx)
    .await;
    if let Err(error) = insert {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "paper_trnm_chain_time_height_conflict",
                "this Chain height already has a different authenticated time checkpoint",
            ));
        }
        return Err(ApiError::database(error));
    }
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(checkpoint)))
}

fn validate_chain_time_checkpoint_progress_v1(
    latest: Option<(u64, u64)>,
    candidate: &PaperTrnmChainTimeCheckpointV1,
) -> Result<(), ApiError> {
    let Some((latest_height, latest_consensus_time_unix_ms)) = latest else {
        return Ok(());
    };
    if candidate.height <= latest_height {
        return Err(ApiError::conflict(
            "paper_trnm_chain_time_checkpoint_height_replay",
            "new Chain-time checkpoints must strictly advance the highest admitted height",
        ));
    }
    if candidate.consensus_time_unix_ms < latest_consensus_time_unix_ms {
        return Err(ApiError::conflict(
            "paper_trnm_chain_time_checkpoint_time_regression",
            "new Chain-time checkpoints must not move consensus time backward",
        ));
    }
    Ok(())
}

fn chain_time_checkpoint_hash_v1(
    request: &AdmitPaperTrnmChainTimeCheckpointV1Request,
) -> Result<String, ApiError> {
    canonical_json_sha256(&json!({
        "domain": CHAIN_TIME_CHECKPOINT_HASH_DOMAIN_V1,
        "trust_anchor_hash": request.trust_anchor_hash,
        "proof": request.proof,
    }))
    .map_err(|message| ApiError::internal(format!("hash Chain-time checkpoint: {message}")))
}

fn verify_chain_time_checkpoint_v1(
    request: &AdmitPaperTrnmChainTimeCheckpointV1Request,
    anchor: &ValidatedCometBftTrustAnchorV1,
    checkpoint_hash: String,
    local_verification_time: SystemTime,
    locally_verified_at_unix_ms: u64,
) -> Result<PaperTrnmChainTimeCheckpointV1, ApiError> {
    let verified = match verify_cometbft_light_finality_proof_v1_with_trust_anchor(
        &request.proof,
        anchor,
        local_verification_time,
    ) {
        ChainTimeVerificationOutcomeV1::Verified(verified) => verified,
        ChainTimeVerificationOutcomeV1::StructuralInvalid { reason } => {
            return Err(ApiError::bad_request(
                "paper_trnm_chain_time_checkpoint_invalid",
                reason,
            ));
        }
        ChainTimeVerificationOutcomeV1::Untrusted { reason } => {
            return Err(ApiError::conflict(
                "paper_trnm_chain_time_checkpoint_untrusted",
                reason,
            ));
        }
        ChainTimeVerificationOutcomeV1::NotFinal { reason } => {
            return Err(ApiError::conflict(
                "paper_trnm_chain_time_checkpoint_not_final",
                reason,
            ));
        }
    };
    let canonical_proof_sha256 = canonical_json_sha256(&request.proof)
        .map_err(|message| ApiError::internal(format!("hash Chain-time proof: {message}")))?;
    Ok(PaperTrnmChainTimeCheckpointV1 {
        schema: PAPER_TRNM_CHAIN_TIME_CHECKPOINT_SCHEMA_V1.to_string(),
        checkpoint_hash,
        trust_anchor_hash: request.trust_anchor_hash.clone(),
        chain_id: verified.chain_id,
        height: verified.height,
        header_hash: verified.header_hash_hex,
        consensus_time_unix_ms: verified.consensus_time_unix_ms,
        canonical_proof_sha256,
        locally_verified_at_unix_ms,
    })
}

fn enforce_local_verification_high_water_v1(
    high_water_unix_ms: Option<u64>,
    local_verification_time_unix_ms: u64,
) -> Result<(), ApiError> {
    if high_water_unix_ms.is_some_and(|high_water| local_verification_time_unix_ms < high_water) {
        return Err(ApiError::conflict(
            "paper_trnm_local_verification_clock_rollback",
            "local light-client verification clock moved behind its persisted high-water",
        ));
    }
    Ok(())
}

async fn validated_trust_anchor_postgres_v1(
    tx: &mut Transaction<'_, Postgres>,
    anchor_hash: &str,
    pinned_anchor_hashes: &std::collections::HashSet<String>,
) -> Result<ValidatedCometBftTrustAnchorV1, ApiError> {
    if !pinned_anchor_hashes.contains(anchor_hash) {
        return Err(ApiError::forbidden(
            "trnm_trust_anchor_not_pinned",
            "stored trust anchor is absent from immutable server configuration",
        ));
    }
    let row = sqlx::query(
        "select canonical_anchor from hepta_trnm_cometbft_trust_anchors
         where anchor_hash=$1",
    )
    .bind(anchor_hash)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "trnm_trust_anchor_not_admitted",
            "selected CometBFT trust anchor has not been admitted",
        )
    })?;
    let canonical: Vec<u8> = row.get("canonical_anchor");
    let anchor =
        ValidatedCometBftTrustAnchorV1::from_canonical_bytes(&canonical).map_err(|error| {
            ApiError::internal(format!(
                "stored canonical CometBFT trust anchor is invalid: {error:#}"
            ))
        })?;
    if anchor.wire().anchor_hash_hex != anchor_hash {
        return Err(ApiError::internal(
            "stored CometBFT trust anchor bytes do not match their indexed hash",
        ));
    }
    Ok(anchor)
}

fn load_chain_time_checkpoint_memory_v1(
    finality: &crate::paper_chain_finality_v1::PaperChainFinalityMemory,
    checkpoint_hash: &str,
    pinned_anchor_hashes: &std::collections::HashSet<String>,
) -> Result<(PaperTrnmChainTimeCheckpointV1, u64, u64), ApiError> {
    crate::validate_hash("checkpoint_hash", checkpoint_hash)?;
    let checkpoint = finality
        .preparations_v2
        .time_checkpoints_by_hash
        .get(checkpoint_hash)
        .cloned()
        .ok_or_else(|| {
            ApiError::not_found(
                "paper_trnm_chain_time_checkpoint_not_found",
                "selected authenticated Chain-time checkpoint has not been admitted",
            )
        })?;
    if !pinned_anchor_hashes.contains(&checkpoint.trust_anchor_hash) {
        return Err(ApiError::forbidden(
            "trnm_trust_anchor_not_pinned",
            "checkpoint trust anchor is absent from immutable server configuration",
        ));
    }
    let max_height = finality
        .preparations_v2
        .time_checkpoints_by_hash
        .values()
        .filter(|candidate| candidate.chain_id == checkpoint.chain_id)
        .map(|candidate| candidate.height)
        .max()
        .ok_or_else(|| ApiError::internal("checkpoint index is internally inconsistent"))?;
    let local_high_water = finality
        .preparations_v2
        .time_checkpoints_by_hash
        .values()
        .map(|candidate| candidate.locally_verified_at_unix_ms)
        .max()
        .ok_or_else(|| ApiError::internal("checkpoint high-water is internally inconsistent"))?;
    Ok((checkpoint, max_height, local_high_water))
}

async fn load_chain_time_checkpoint_postgres_v1(
    tx: &mut Transaction<'_, Postgres>,
    checkpoint_hash: &str,
    pinned_anchor_hashes: &std::collections::HashSet<String>,
) -> Result<(PaperTrnmChainTimeCheckpointV1, u64, u64), ApiError> {
    crate::validate_hash("checkpoint_hash", checkpoint_hash)?;
    let row = sqlx::query(
        "select record_json from hepta_trnm_cometbft_time_checkpoints_v1
         where checkpoint_hash=$1",
    )
    .bind(checkpoint_hash)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "paper_trnm_chain_time_checkpoint_not_found",
            "selected authenticated Chain-time checkpoint has not been admitted",
        )
    })?;
    let checkpoint: PaperTrnmChainTimeCheckpointV1 =
        decode_record(row.get("record_json"), "Paper Chain-time checkpoint")?;
    if checkpoint.checkpoint_hash != checkpoint_hash {
        return Err(ApiError::internal(
            "stored Chain-time checkpoint record does not match its indexed hash",
        ));
    }
    if !pinned_anchor_hashes.contains(&checkpoint.trust_anchor_hash) {
        return Err(ApiError::forbidden(
            "trnm_trust_anchor_not_pinned",
            "checkpoint trust anchor is absent from immutable server configuration",
        ));
    }
    let max_height = sqlx::query(
        "select max(height) as max_height from hepta_trnm_cometbft_time_checkpoints_v1
         where chain_id=$1",
    )
    .bind(&checkpoint.chain_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .get::<Option<i64>, _>("max_height")
    .ok_or_else(|| ApiError::internal("checkpoint index is internally inconsistent"))?;
    let max_height = u64::try_from(max_height)
        .map_err(|_| ApiError::internal("stored checkpoint height is negative"))?;
    let local_high_water = sqlx::query(
        "select max(locally_verified_at_unix_ms) as high_water
         from hepta_trnm_cometbft_time_checkpoints_v1",
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .get::<Option<i64>, _>("high_water")
    .ok_or_else(|| ApiError::internal("checkpoint high-water is internally inconsistent"))?;
    let local_high_water = u64::try_from(local_high_water)
        .map_err(|_| ApiError::internal("stored checkpoint high-water is negative"))?;
    Ok((checkpoint, max_height, local_high_water))
}

pub(crate) fn ensure_paper_finality_v2_source_unsealed_memory(
    finality: &crate::paper_chain_finality_v1::PaperChainFinalityMemory,
    paper_id: Uuid,
) -> Result<(), ApiError> {
    if finality.preparations_v2.by_paper_id.contains_key(&paper_id) {
        return Err(ApiError::conflict(
            "paper_chain_finality_v2_source_sealed",
            "Paper evaluation, reproduction, and Appeal facts are sealed by an immutable V2 finality preparation",
        ));
    }
    Ok(())
}

pub(crate) async fn lock_paper_finality_v2_source_unsealed_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<(), ApiError> {
    let row = sqlx::query(
        "select finality_v2_seal_epoch from hepta_paper_projects
         where paper_project_id=$1 for update",
    )
    .bind(paper_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("paper_project_not_found", "Paper does not exist"))?;
    let seal_epoch: i16 = row.get("finality_v2_seal_epoch");
    if seal_epoch != 0 {
        return Err(ApiError::conflict(
            "paper_chain_finality_v2_source_sealed",
            "Paper evaluation, reproduction, and Appeal facts are sealed by an immutable V2 finality preparation",
        ));
    }
    Ok(())
}

async fn arm_paper_trnm_finality_window_v2(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    request: Request,
) -> Result<(StatusCode, Json<PaperTrnmFinalityWindowArmV2>), ApiError> {
    require_service_token(
        request.headers(),
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    let body = read_limited_body(request, MAX_PREPARE_PAPER_TRNM_FINALITY_V2_BODY_BYTES).await?;
    let request: ArmPaperTrnmFinalityWindowV2Request =
        serde_json::from_slice(&body).map_err(|error| {
            ApiError::bad_request(
                "invalid_paper_trnm_v2_window_arm_request",
                format!("Paper V2 window-arm JSON is invalid: {error}"),
            )
        })?;
    validate_idempotency_key(&request.idempotency_key)?;
    crate::validate_hash("start_checkpoint_hash", &request.start_checkpoint_hash)?;
    crate::paper_raid_v2::validate_logical_session_id(&request.research_session_id)?;
    if request.research_session_roster_version == 0 {
        return Err(ApiError::bad_request(
            "paper_trnm_roster_version_invalid",
            "research_session_roster_version must be positive",
        ));
    }
    let request_hash = canonical_json_sha256(&request).map_err(|message| {
        ApiError::internal(format!("encode V2 window-arm request: {message}"))
    })?;
    let local_verification_time = (state.cometbft_local_verification_clock)();

    if state.finality_pool.is_some() {
        arm_paper_trnm_finality_window_postgres(
            &state,
            paper_id,
            request,
            request_hash,
            local_verification_time,
        )
        .await
    } else {
        arm_paper_trnm_finality_window_memory(
            &state,
            paper_id,
            request,
            request_hash,
            local_verification_time,
        )
        .await
    }
}

async fn arm_paper_trnm_finality_window_memory(
    state: &AppState,
    paper_id: Uuid,
    request: ArmPaperTrnmFinalityWindowV2Request,
    request_hash: String,
    local_verification_time: SystemTime,
) -> Result<(StatusCode, Json<PaperTrnmFinalityWindowArmV2>), ApiError> {
    let paper = state.paper_raid.read().await;
    let mut finality = state.paper_chain_finality.write().await;
    if let Some(existing) = finality
        .preparations_v2
        .arms_by_idempotency_key
        .get(&request.idempotency_key)
    {
        if existing.request_hash == request_hash && existing.paper_project_id == paper_id {
            return Ok((StatusCode::OK, Json(existing.clone())));
        }
        return Err(ApiError::conflict(
            "paper_trnm_v2_window_arm_idempotency_conflict",
            "idempotency key was reused with a different Paper window-arm request",
        ));
    }
    let (checkpoint, max_checkpoint_height, local_high_water) =
        load_chain_time_checkpoint_memory_v1(
            &finality,
            &request.start_checkpoint_hash,
            &state.security.pinned_trnm_cometbft_trust_anchor_hashes,
        )?;
    let local_verification_time_unix_ms =
        system_time_unix_millis_v2(local_verification_time, "local verification time")?;
    enforce_local_verification_high_water_v1(
        Some(local_high_water),
        local_verification_time_unix_ms,
    )?;
    validate_fresh_start_checkpoint(
        &checkpoint,
        max_checkpoint_height,
        local_verification_time_unix_ms,
    )?;
    let facts = finality_facts_memory(&paper, paper_id, request.submission_id)?;
    let source = validate_source_candidate_v2(&facts, &request, &state.security)?;
    let source_fingerprint = source_fingerprint_v2(&facts, &request)?;
    if finality
        .preparations_v2
        .arms_by_source_fingerprint
        .contains_key(&source_fingerprint)
    {
        return Err(ApiError::conflict(
            "paper_trnm_v2_window_arm_exists",
            "the exact finality source facts already have an immutable Chain-time arm",
        ));
    }
    let arm = build_window_arm_v2(
        paper_id,
        request,
        request_hash,
        source_fingerprint,
        source.appeal,
        checkpoint,
        max_checkpoint_height,
    )?;
    finality
        .preparations_v2
        .arms_by_id
        .insert(arm.arm_id, arm.clone());
    finality
        .preparations_v2
        .arms_by_source_fingerprint
        .insert(arm.source_fingerprint.clone(), arm.clone());
    finality
        .preparations_v2
        .arms_by_idempotency_key
        .insert(arm.idempotency_key.clone(), arm.clone());
    Ok((StatusCode::CREATED, Json(arm)))
}

async fn arm_paper_trnm_finality_window_postgres(
    state: &AppState,
    paper_id: Uuid,
    request: ArmPaperTrnmFinalityWindowV2Request,
    request_hash: String,
    local_verification_time: SystemTime,
) -> Result<(StatusCode, Json<PaperTrnmFinalityWindowArmV2>), ApiError> {
    let pool = state.finality_pool.as_ref().expect("PostgreSQL checked");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    lock_finality_idempotency_key_postgres(&mut tx, ARM_OPERATION_V2, &request.idempotency_key)
        .await?;
    if let Some(row) = sqlx::query(
        "select paper_project_id,request_hash,record_json
         from hepta_paper_chain_finality_window_arms_v2
         where idempotency_key=$1",
    )
    .bind(&request.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    {
        let stored_paper_id: Uuid = row.get("paper_project_id");
        let stored_hash: String = row.get("request_hash");
        if stored_paper_id == paper_id && stored_hash == request_hash {
            let stored: PaperTrnmFinalityWindowArmV2 =
                decode_record(row.get("record_json"), "Paper V2 window arm")?;
            tx.commit().await.map_err(ApiError::database)?;
            return Ok((StatusCode::OK, Json(stored)));
        }
        return Err(ApiError::conflict(
            "paper_trnm_v2_window_arm_idempotency_conflict",
            "idempotency key was reused with a different Paper window-arm request",
        ));
    }

    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(PAPER_CHAIN_TIME_ADVISORY_LOCK_KEY_V1)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    lock_paper_anchor_for_finality_v2(&mut tx, paper_id).await?;
    let (checkpoint, max_checkpoint_height, local_high_water) =
        load_chain_time_checkpoint_postgres_v1(
            &mut tx,
            &request.start_checkpoint_hash,
            &state.security.pinned_trnm_cometbft_trust_anchor_hashes,
        )
        .await?;
    let local_verification_time_unix_ms =
        system_time_unix_millis_v2(local_verification_time, "local verification time")?;
    enforce_local_verification_high_water_v1(
        Some(local_high_water),
        local_verification_time_unix_ms,
    )?;
    validate_fresh_start_checkpoint(
        &checkpoint,
        max_checkpoint_height,
        local_verification_time_unix_ms,
    )?;
    let facts = finality_facts_postgres(&mut tx, paper_id, request.submission_id).await?;
    let source = validate_source_candidate_v2(&facts, &request, &state.security)?;
    let source_fingerprint = source_fingerprint_v2(&facts, &request)?;
    let arm = build_window_arm_v2(
        paper_id,
        request.clone(),
        request_hash.clone(),
        source_fingerprint,
        source.appeal,
        checkpoint,
        max_checkpoint_height,
    )?;
    let record_json = serde_json::to_value(&arm)
        .map_err(|error| ApiError::internal(format!("encode Paper V2 window arm: {error}")))?;
    let insert = sqlx::query(
        "insert into hepta_paper_chain_finality_window_arms_v2 (
            arm_id,paper_project_id,submission_id,evaluation_id,latest_reproduction_id,
            research_session_id,research_session_roster_version,source_fingerprint,
            appeal_status,appeal_id,appeal_resolution_id,
            start_checkpoint_hash,start_anchor_hash,start_chain_id,start_height,
            start_header_hash,start_consensus_time_unix_ms,observed_max_checkpoint_height,
            max_chain_time_lag_ms,earliest_final_checkpoint_time_unix_ms,
            idempotency_key,request_hash,record_json
         ) values (
            $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,
            $19,$20,$21,$22,$23::jsonb
         )",
    )
    .bind(arm.arm_id)
    .bind(arm.paper_project_id)
    .bind(arm.submission_id)
    .bind(arm.evaluation_id)
    .bind(arm.latest_reproduction_id)
    .bind(&arm.research_session_id)
    .bind(i64_from_u64_v2(
        arm.research_session_roster_version,
        "research session roster version",
    )?)
    .bind(&arm.source_fingerprint)
    .bind(arm.appeal_status.as_database_str())
    .bind(arm.appeal_id)
    .bind(arm.appeal_resolution_id)
    .bind(&arm.start_checkpoint.checkpoint_hash)
    .bind(&arm.start_checkpoint.trust_anchor_hash)
    .bind(&arm.start_checkpoint.chain_id)
    .bind(i64_from_u64_v2(
        arm.start_checkpoint.height,
        "start checkpoint height",
    )?)
    .bind(&arm.start_checkpoint.header_hash)
    .bind(i64_from_u64_v2(
        arm.start_checkpoint.consensus_time_unix_ms,
        "start checkpoint time",
    )?)
    .bind(i64_from_u64_v2(
        arm.observed_max_checkpoint_height,
        "observed max checkpoint height",
    )?)
    .bind(i64_from_u64_v2(
        arm.max_chain_time_lag_ms,
        "max Chain time lag",
    )?)
    .bind(i64_from_u64_v2(
        arm.earliest_final_checkpoint_time_unix_ms,
        "earliest final checkpoint time",
    )?)
    .bind(&arm.idempotency_key)
    .bind(&arm.request_hash)
    .bind(&record_json)
    .execute(&mut *tx)
    .await;
    if let Err(error) = insert {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "paper_trnm_v2_window_arm_conflict",
                "the exact source facts or idempotency key already have a Chain-time arm",
            ));
        }
        return Err(ApiError::database(error));
    }
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(arm)))
}

async fn lock_finality_idempotency_key_postgres(
    tx: &mut Transaction<'_, Postgres>,
    operation: &str,
    idempotency_key: &str,
) -> Result<(), ApiError> {
    // The immutable arm/preparation primary tables are also the replay source.
    // A transaction-scoped advisory lock serializes the operation-scoped key
    // before the primary-record lookup without requiring a shared writable
    // idempotency table or UPDATE privilege merely for SELECT ... FOR UPDATE.
    sqlx::query(
        "select pg_advisory_xact_lock(
            hashtextextended($1::text || E'\\n' || $2::text, 0)
         )",
    )
    .bind(operation)
    .bind(idempotency_key)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    Ok(())
}

fn build_window_arm_v2(
    paper_id: Uuid,
    request: ArmPaperTrnmFinalityWindowV2Request,
    request_hash: String,
    source_fingerprint: String,
    appeal: ResolvedAppealFactsV2,
    checkpoint: PaperTrnmChainTimeCheckpointV1,
    max_checkpoint_height: u64,
) -> Result<PaperTrnmFinalityWindowArmV2, ApiError> {
    let policy_delay_ms = match appeal.status {
        PaperTrnmAppealStatusV2::ClosedNoAppeal => u64::try_from(PAPER_NO_APPEAL_WINDOW_SECONDS_V1)
            .map_err(|_| ApiError::internal("Paper Appeal window policy is negative"))?
            .checked_mul(1_000)
            .ok_or_else(|| ApiError::internal("Paper Appeal window milliseconds overflow"))?,
        PaperTrnmAppealStatusV2::ResolvedDenied | PaperTrnmAppealStatusV2::ResolvedUpheld => 0,
    };
    let earliest_final_checkpoint_time_unix_ms = checkpoint
        .consensus_time_unix_ms
        .checked_add(PAPER_CHAIN_TIME_MAX_LAG_MS_V1)
        .and_then(|value| value.checked_add(policy_delay_ms))
        .ok_or_else(|| ApiError::internal("Paper Chain-time Appeal deadline overflow"))?;
    Ok(PaperTrnmFinalityWindowArmV2 {
        schema: PAPER_TRNM_FINALITY_WINDOW_ARM_SCHEMA_V2.to_string(),
        arm_id: Uuid::new_v4(),
        paper_project_id: paper_id,
        submission_id: request.submission_id,
        evaluation_id: request.evaluation_id,
        latest_reproduction_id: request.latest_reproduction_id,
        research_session_id: request.research_session_id,
        research_session_roster_version: request.research_session_roster_version,
        source_fingerprint,
        appeal_status: appeal.status,
        appeal_id: appeal.appeal_id,
        appeal_resolution_id: appeal.resolution_id,
        start_checkpoint: checkpoint,
        observed_max_checkpoint_height: max_checkpoint_height,
        max_chain_time_lag_ms: PAPER_CHAIN_TIME_MAX_LAG_MS_V1,
        earliest_final_checkpoint_time_unix_ms,
        idempotency_key: request.idempotency_key,
        request_hash,
    })
}

fn validate_fresh_start_checkpoint(
    checkpoint: &PaperTrnmChainTimeCheckpointV1,
    max_checkpoint_height: u64,
    local_verification_time_unix_ms: u64,
) -> Result<(), ApiError> {
    if checkpoint.height != max_checkpoint_height {
        return Err(ApiError::conflict(
            "paper_trnm_v2_start_checkpoint_not_latest",
            "window arm must use the highest authenticated Chain-time checkpoint admitted by this service",
        ));
    }
    if checkpoint.consensus_time_unix_ms > local_verification_time_unix_ms {
        return Err(ApiError::conflict(
            "paper_trnm_v2_start_checkpoint_from_future",
            "Chain checkpoint is ahead of the local light-client verification clock",
        ));
    }
    if local_verification_time_unix_ms - checkpoint.consensus_time_unix_ms
        > PAPER_CHAIN_TIME_MAX_LAG_MS_V1
    {
        return Err(ApiError::conflict(
            "paper_trnm_v2_start_checkpoint_stale",
            "Chain checkpoint is too old to arm a rights-preserving Appeal window",
        ));
    }
    Ok(())
}

fn validate_prepare_request_matches_arm(
    request: &PreparePaperTrnmFinalityV2Request,
    paper_id: Uuid,
    arm: &PaperTrnmFinalityWindowArmV2,
) -> Result<(), ApiError> {
    if arm.schema != PAPER_TRNM_FINALITY_WINDOW_ARM_SCHEMA_V2
        || arm.paper_project_id != paper_id
        || arm.submission_id != request.submission_id
        || arm.evaluation_id != request.evaluation_id
        || arm.latest_reproduction_id != request.latest_reproduction_id
        || arm.research_session_id != request.research_session_id
        || arm.research_session_roster_version != request.research_session_roster_version
        || arm.max_chain_time_lag_ms != PAPER_CHAIN_TIME_MAX_LAG_MS_V1
    {
        return Err(ApiError::conflict(
            "paper_trnm_v2_window_arm_binding_mismatch",
            "preparation selectors do not match the immutable Chain-time window arm",
        ));
    }
    Ok(())
}

fn validate_final_checkpoint(
    arm: &PaperTrnmFinalityWindowArmV2,
    final_checkpoint: &PaperTrnmChainTimeCheckpointV1,
) -> Result<(), ApiError> {
    if final_checkpoint.chain_id != arm.start_checkpoint.chain_id {
        return Err(ApiError::conflict(
            "paper_trnm_v2_checkpoint_chain_mismatch",
            "start and final Chain checkpoints belong to different chains",
        ));
    }
    if final_checkpoint.height <= arm.observed_max_checkpoint_height
        || final_checkpoint.height <= arm.start_checkpoint.height
    {
        return Err(ApiError::conflict(
            "paper_trnm_v2_final_checkpoint_not_causal",
            "final checkpoint must be admitted after the arm and advance Chain height",
        ));
    }
    if final_checkpoint.consensus_time_unix_ms < arm.earliest_final_checkpoint_time_unix_ms {
        return Err(ApiError::conflict(
            "paper_chain_finality_appeal_window_open",
            format!(
                "Paper scientific-finality policy holds until Chain time {}ms",
                arm.earliest_final_checkpoint_time_unix_ms
            ),
        ));
    }
    Ok(())
}

async fn lock_paper_anchor_for_finality_v2(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<(), ApiError> {
    if let Err(error) =
        sqlx::query("select public.hepta_assert_paper_finality_v2_source_unsealed($1)")
            .bind(paper_id)
            .execute(&mut **tx)
            .await
    {
        if error
            .as_database_error()
            .is_some_and(|database| database.code().as_deref() == Some("55000"))
        {
            return Err(ApiError::conflict(
                "paper_chain_finality_v2_source_sealed",
                "Paper source facts already have an immutable V2 finality preparation",
            ));
        }
        return Err(ApiError::database(error));
    }
    let row = sqlx::query(
        "select finality_v2_seal_epoch from hepta_paper_projects
         where paper_project_id=$1",
    )
    .bind(paper_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("paper_project_not_found", "Paper does not exist"))?;
    let seal_epoch: i16 = row.get("finality_v2_seal_epoch");
    if seal_epoch != 0 {
        return Err(ApiError::conflict(
            "paper_chain_finality_v2_source_sealed",
            "Paper source facts already have an immutable V2 finality preparation",
        ));
    }
    Ok(())
}

async fn load_window_arm_postgres(
    tx: &mut Transaction<'_, Postgres>,
    arm_id: Uuid,
) -> Result<PaperTrnmFinalityWindowArmV2, ApiError> {
    let row = sqlx::query(
        "select record_json from hepta_paper_chain_finality_window_arms_v2
         where arm_id=$1",
    )
    .bind(arm_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "paper_trnm_v2_window_arm_not_found",
            "the referenced immutable Chain-time window arm does not exist",
        )
    })?;
    decode_record(row.get("record_json"), "Paper V2 window arm")
}

async fn prepare_paper_trnm_finality_v2(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    request: Request,
) -> Result<(StatusCode, Json<PaperTrnmFinalityPreparationV2>), ApiError> {
    // Authentication precedes Content-Length inspection and body polling.  The
    // preparation is operator-only and must not let an unauthenticated peer
    // spend JSON parsing or buffering resources.
    require_service_token(
        request.headers(),
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    let body = read_limited_body(request, MAX_PREPARE_PAPER_TRNM_FINALITY_V2_BODY_BYTES).await?;
    let request: PreparePaperTrnmFinalityV2Request =
        serde_json::from_slice(&body).map_err(|error| {
            ApiError::bad_request(
                "invalid_paper_trnm_v2_preparation_request",
                format!("Paper V2 preparation JSON is invalid: {error}"),
            )
        })?;
    validate_idempotency_key(&request.idempotency_key)?;
    crate::validate_hash("final_checkpoint_hash", &request.final_checkpoint_hash)?;
    crate::paper_raid_v2::validate_logical_session_id(&request.research_session_id)?;
    if request.research_session_roster_version == 0 {
        return Err(ApiError::bad_request(
            "paper_trnm_roster_version_invalid",
            "research_session_roster_version must be positive",
        ));
    }
    let request_hash = canonical_json_sha256(&request).map_err(|message| {
        ApiError::internal(format!("encode V2 preparation request: {message}"))
    })?;
    if state.finality_pool.is_some() {
        prepare_paper_trnm_finality_postgres(&state, paper_id, request, request_hash).await
    } else {
        prepare_paper_trnm_finality_memory(&state, paper_id, request, request_hash).await
    }
}

async fn prepare_paper_trnm_finality_memory(
    state: &AppState,
    paper_id: Uuid,
    request: PreparePaperTrnmFinalityV2Request,
    request_hash: String,
) -> Result<(StatusCode, Json<PaperTrnmFinalityPreparationV2>), ApiError> {
    // Keep the same paper -> finality lock order as Receipt V2 projection.
    // Reversing these locks can deadlock a preparation against a concurrent
    // Paper finality write in the in-memory backend.
    let paper = state.paper_raid.read().await;
    let mut finality = state.paper_chain_finality.write().await;
    if let Some(existing) = finality
        .preparations_v2
        .by_idempotency_key
        .get(&request.idempotency_key)
    {
        if existing.request_hash == request_hash && existing.binding.paper_project_id == paper_id {
            return Ok((StatusCode::OK, Json(existing.clone())));
        }
        return Err(ApiError::conflict(
            "paper_trnm_v2_idempotency_conflict",
            "idempotency key was reused with a different Paper finality request",
        ));
    }
    if finality.preparations_v2.by_paper_id.contains_key(&paper_id) {
        return Err(ApiError::conflict(
            "paper_trnm_v2_preparation_exists",
            "the Paper already has an immutable V2 finality preparation",
        ));
    }
    let arm = finality
        .preparations_v2
        .arms_by_id
        .get(&request.arm_id)
        .cloned()
        .ok_or_else(|| {
            ApiError::not_found(
                "paper_trnm_v2_window_arm_not_found",
                "the referenced immutable Chain-time window arm does not exist",
            )
        })?;
    validate_prepare_request_matches_arm(&request, paper_id, &arm)?;
    let (final_checkpoint, _, _) = load_chain_time_checkpoint_memory_v1(
        &finality,
        &request.final_checkpoint_hash,
        &state.security.pinned_trnm_cometbft_trust_anchor_hashes,
    )?;
    validate_final_checkpoint(&arm, &final_checkpoint)?;
    let facts = finality_facts_memory(&paper, paper_id, request.submission_id)?;
    let source_fingerprint = source_fingerprint_v2(&facts, &request)?;
    if source_fingerprint != arm.source_fingerprint {
        return Err(ApiError::conflict(
            "paper_trnm_v2_window_arm_stale",
            "Paper finality source facts changed after the Chain-time window was armed",
        ));
    }
    let binding = derive_binding_v2(&facts, &request, &arm, &final_checkpoint, &state.security)?;
    let preparation = build_preparation(
        request.idempotency_key,
        request_hash,
        binding,
        final_checkpoint.consensus_time_unix_ms,
    )?;
    if let Some(existing) = finality
        .preparations_v2
        .by_commitment_id
        .get(&preparation.binding.commitment_id)
    {
        if existing.binding == preparation.binding {
            return Err(ApiError::conflict(
                "paper_trnm_v2_duplicate_preparation",
                "the immutable finality facts were already prepared under another idempotency key",
            ));
        }
        return Err(ApiError::conflict(
            "paper_trnm_v2_commitment_conflict",
            "commitment id collides with different immutable finality facts",
        ));
    }
    finality.preparations_v2.by_commitment_id.insert(
        preparation.binding.commitment_id.clone(),
        preparation.clone(),
    );
    finality
        .preparations_v2
        .by_paper_id
        .insert(paper_id, preparation.clone());
    finality
        .preparations_v2
        .by_idempotency_key
        .insert(preparation.idempotency_key.clone(), preparation.clone());
    Ok((StatusCode::CREATED, Json(preparation)))
}

async fn prepare_paper_trnm_finality_postgres(
    state: &AppState,
    paper_id: Uuid,
    request: PreparePaperTrnmFinalityV2Request,
    request_hash: String,
) -> Result<(StatusCode, Json<PaperTrnmFinalityPreparationV2>), ApiError> {
    let pool = state.finality_pool.as_ref().expect("PostgreSQL checked");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    lock_finality_idempotency_key_postgres(&mut tx, PREPARE_OPERATION_V2, &request.idempotency_key)
        .await?;
    if let Some(row) = sqlx::query(
        "select paper_project_id,request_hash,record_json
         from hepta_paper_chain_finality_preparations_v2
         where idempotency_key=$1",
    )
    .bind(&request.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    {
        let stored_paper_id: Uuid = row.get("paper_project_id");
        let stored_hash: String = row.get("request_hash");
        if stored_paper_id == paper_id && stored_hash == request_hash {
            let stored: PaperTrnmFinalityPreparationV2 =
                decode_record(row.get("record_json"), "Paper V2 preparation")?;
            tx.commit().await.map_err(ApiError::database)?;
            return Ok((StatusCode::OK, Json(stored)));
        }
        return Err(ApiError::conflict(
            "paper_trnm_v2_idempotency_conflict",
            "idempotency key was reused with a different Paper finality request",
        ));
    }

    lock_paper_anchor_for_finality_v2(&mut tx, paper_id).await?;
    let arm = load_window_arm_postgres(&mut tx, request.arm_id).await?;
    validate_prepare_request_matches_arm(&request, paper_id, &arm)?;
    let (final_checkpoint, _, _) = load_chain_time_checkpoint_postgres_v1(
        &mut tx,
        &request.final_checkpoint_hash,
        &state.security.pinned_trnm_cometbft_trust_anchor_hashes,
    )
    .await?;
    validate_final_checkpoint(&arm, &final_checkpoint)?;
    let facts = finality_facts_postgres(&mut tx, paper_id, request.submission_id).await?;
    let source_fingerprint = source_fingerprint_v2(&facts, &request)?;
    if source_fingerprint != arm.source_fingerprint {
        return Err(ApiError::conflict(
            "paper_trnm_v2_window_arm_stale",
            "Paper finality source facts changed after the Chain-time window was armed",
        ));
    }
    let binding = derive_binding_v2(&facts, &request, &arm, &final_checkpoint, &state.security)?;
    let preparation = build_preparation(
        request.idempotency_key.clone(),
        request_hash.clone(),
        binding,
        final_checkpoint.consensus_time_unix_ms,
    )?;
    if sqlx::query_scalar::<_, bool>(
        "select exists(
            select 1 from hepta_paper_chain_finality_preparations_v2
            where paper_project_id=$1
         )",
    )
    .bind(preparation.binding.paper_project_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?
    {
        return Err(ApiError::conflict(
            "paper_trnm_v2_preparation_exists",
            "the Paper already has an immutable V2 finality preparation",
        ));
    }
    let record_json = serde_json::to_value(&preparation)
        .map_err(|error| ApiError::internal(format!("encode Paper V2 preparation: {error}")))?;
    let insert = sqlx::query(
        "insert into hepta_paper_chain_finality_preparations_v2 (
            preparation_id,paper_project_id,submission_id,evaluation_id,latest_reproduction_id,
            arm_id,source_fingerprint,final_checkpoint_hash,
            final_anchor_hash,final_chain_id,final_height,
            final_header_hash,final_consensus_time_unix_ms,request_hash,
            research_session_id,research_session_roster_version,
            match_evidence_commitment_id,appeal_status,appeal_id,appealed_evaluation_id,
            appeal_resolution_id,commitment_id,binding_fingerprint,idempotency_key,status,
            scientific_finality,score_eligible,ranking_eligible,reward_eligible,economic_eligible,
            record_json,created_at
         ) values (
            $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
            $18,$19,$20,$21,$22,$23,$24,'awaiting_chain_verifier_upgrade',
            $25,$26,$27,$28,$29,$30::jsonb,$31
         )",
    )
    .bind(preparation.preparation_id)
    .bind(preparation.binding.paper_project_id)
    .bind(preparation.binding.submission_id)
    .bind(preparation.binding.evaluation_id)
    .bind(preparation.binding.latest_reproduction_id)
    .bind(preparation.binding.window_arm_id)
    .bind(&preparation.binding.source_fingerprint)
    .bind(&preparation.binding.final_checkpoint_hash)
    .bind(&preparation.binding.final_checkpoint_anchor_hash)
    .bind(&preparation.binding.final_checkpoint_chain_id)
    .bind(i64_from_u64_v2(
        preparation.binding.final_checkpoint_height,
        "final checkpoint height",
    )?)
    .bind(&preparation.binding.final_checkpoint_header_hash)
    .bind(i64_from_u64_v2(
        preparation.binding.final_checkpoint_consensus_time_unix_ms,
        "final checkpoint time",
    )?)
    .bind(&preparation.request_hash)
    .bind(&preparation.binding.research_session_id)
    .bind(i64_from_u64_v2(
        preparation.binding.research_session_roster_version,
        "research session roster version",
    )?)
    .bind(&preparation.binding.match_evidence_commitment_id)
    .bind(preparation.binding.appeal_status.as_database_str())
    .bind(preparation.binding.appeal_id)
    .bind(preparation.binding.appealed_evaluation_id)
    .bind(preparation.binding.appeal_resolution_id)
    .bind(&preparation.binding.commitment_id)
    .bind(&preparation.binding_fingerprint)
    .bind(&preparation.idempotency_key)
    .bind(preparation.binding.scientific_finality)
    .bind(preparation.binding.score_eligible)
    .bind(preparation.binding.ranking_eligible)
    .bind(preparation.binding.reward_eligible)
    .bind(preparation.binding.economic_eligible)
    .bind(&record_json)
    .bind(preparation.created_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = insert {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "paper_trnm_v2_preparation_conflict",
                "the immutable commitment, binding, or idempotency key was already prepared",
            ));
        }
        return Err(ApiError::database(error));
    }
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(preparation)))
}

fn finality_facts_memory(
    memory: &crate::paper_raid_v2::PaperRaidMemory,
    paper_id: Uuid,
    submission_id: Uuid,
) -> Result<PaperFinalityFactsV2, ApiError> {
    let paper =
        memory.papers.get(&paper_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "Paper does not exist")
        })?;
    let submission = memory
        .submissions
        .get(&submission_id)
        .filter(|submission| submission.paper_project_id == paper_id)
        .cloned()
        .ok_or_else(|| {
            ApiError::not_found(
                "joint_submission_not_found",
                "Paper-bound submission does not exist",
            )
        })?;
    Ok(PaperFinalityFactsV2 {
        paper,
        submission,
        evaluations: memory
            .review
            .evaluations
            .values()
            .filter(|evaluation| evaluation.paper_project_id == paper_id)
            .cloned()
            .collect(),
        reproductions: memory
            .review
            .reproductions
            .values()
            .filter(|reproduction| reproduction.paper_project_id == paper_id)
            .cloned()
            .collect(),
        appeals: memory
            .review
            .appeals
            .values()
            .filter(|appeal| appeal.paper_project_id == paper_id)
            .cloned()
            .collect(),
        resolutions: memory
            .review
            .resolutions
            .values()
            .filter(|resolution| resolution.paper_project_id == paper_id)
            .cloned()
            .collect(),
        authorization_sets: memory
            .research_session_authorization_sets
            .values()
            .filter(|set| set.paper_project_id == paper_id)
            .cloned()
            .collect(),
        completions: memory
            .research_session_completions
            .values()
            .filter(|completion| completion.paper_project_id == paper_id)
            .cloned()
            .collect(),
    })
}

async fn finality_facts_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    submission_id: Uuid,
) -> Result<PaperFinalityFactsV2, ApiError> {
    // The finality capability locks the persistent Paper anchor before this
    // function is called. Every source writer takes that same anchor before
    // its first mutation, so these immutable-in-transaction reads do not need
    // row-lock clauses (and therefore do not require UPDATE privilege).
    let paper: PaperProject = fetch_one_record(
        tx,
        "select record_json from hepta_paper_projects where paper_project_id=$1",
        paper_id,
        "Paper",
    )
    .await?;
    let submission_row = sqlx::query(
        "select record_json from hepta_joint_paper_submissions
         where submission_id=$1 and paper_project_id=$2",
    )
    .bind(submission_id)
    .bind(paper_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "joint_submission_not_found",
            "Paper-bound submission does not exist",
        )
    })?;
    let submission = decode_record(submission_row.get("record_json"), "joint submission")?;
    Ok(PaperFinalityFactsV2 {
        paper,
        submission,
        evaluations: fetch_records_for_paper(
            tx,
            "select record_json from hepta_paper_evaluations where paper_project_id=$1",
            paper_id,
            "Paper evaluation",
        )
        .await?,
        reproductions: fetch_records_for_paper(
            tx,
            "select record_json from hepta_paper_reproductions where paper_project_id=$1",
            paper_id,
            "Paper reproduction",
        )
        .await?,
        appeals: fetch_records_for_paper(
            tx,
            "select record_json from hepta_paper_appeals where paper_project_id=$1",
            paper_id,
            "Paper appeal",
        )
        .await?,
        resolutions: fetch_records_for_paper(
            tx,
            "select record_json from hepta_paper_appeal_resolutions where paper_project_id=$1",
            paper_id,
            "Paper appeal resolution",
        )
        .await?,
        authorization_sets: fetch_records_for_paper(
            tx,
            "select record_json from hepta_research_session_authorization_sets where paper_project_id=$1",
            paper_id,
            "Research Session authorization set",
        )
        .await?,
        completions: fetch_records_for_paper(
            tx,
            "select record_json from hepta_nakama_research_session_completions where paper_project_id=$1",
            paper_id,
            "Nakama completion",
        )
        .await?,
    })
}

async fn fetch_one_record<T: DeserializeOwned>(
    tx: &mut Transaction<'_, Postgres>,
    sql: &str,
    id: Uuid,
    label: &'static str,
) -> Result<T, ApiError> {
    let row = sqlx::query(sql)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("paper_project_not_found", "Paper does not exist"))?;
    decode_record(row.get("record_json"), label)
}

async fn fetch_records_for_paper<T: DeserializeOwned>(
    tx: &mut Transaction<'_, Postgres>,
    sql: &str,
    paper_id: Uuid,
    label: &'static str,
) -> Result<Vec<T>, ApiError> {
    sqlx::query(sql)
        .bind(paper_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), label))
        .collect()
}

trait FinalitySourceSelectorV2 {
    fn submission_id(&self) -> Uuid;
    fn evaluation_id(&self) -> Uuid;
    fn latest_reproduction_id(&self) -> Uuid;
    fn research_session_id(&self) -> &str;
    fn research_session_roster_version(&self) -> u64;
}

impl FinalitySourceSelectorV2 for ArmPaperTrnmFinalityWindowV2Request {
    fn submission_id(&self) -> Uuid {
        self.submission_id
    }

    fn evaluation_id(&self) -> Uuid {
        self.evaluation_id
    }

    fn latest_reproduction_id(&self) -> Uuid {
        self.latest_reproduction_id
    }

    fn research_session_id(&self) -> &str {
        &self.research_session_id
    }

    fn research_session_roster_version(&self) -> u64 {
        self.research_session_roster_version
    }
}

impl FinalitySourceSelectorV2 for PreparePaperTrnmFinalityV2Request {
    fn submission_id(&self) -> Uuid {
        self.submission_id
    }

    fn evaluation_id(&self) -> Uuid {
        self.evaluation_id
    }

    fn latest_reproduction_id(&self) -> Uuid {
        self.latest_reproduction_id
    }

    fn research_session_id(&self) -> &str {
        &self.research_session_id
    }

    fn research_session_roster_version(&self) -> u64 {
        self.research_session_roster_version
    }
}

struct PaperFinalitySourceCandidateV2<'a> {
    evaluation: &'a PaperEvaluation,
    reproduction: &'a PaperReproduction,
    authorization_set: &'a ResearchSessionAuthorizationSetV1,
    completion: &'a crate::paper_raid_contracts::SignedNakamaCompletionReceiptV1,
    latest_roster_version: Option<u64>,
    appeal: ResolvedAppealFactsV2,
}

fn validate_source_candidate_v2<'a, R: FinalitySourceSelectorV2>(
    facts: &'a PaperFinalityFactsV2,
    request: &R,
    security: &SecurityConfig,
) -> Result<PaperFinalitySourceCandidateV2<'a>, ApiError> {
    let evaluation = require_unique_latest_evaluation(facts, request)?;
    let reproduction = require_unique_latest_reproduction(facts, request, evaluation)?;
    let authorization_set = facts
        .authorization_sets
        .iter()
        .find(|set| {
            set.session_id == request.research_session_id()
                && set.roster_version == request.research_session_roster_version()
        })
        .ok_or_else(|| {
            ApiError::not_found(
                "research_session_authorization_set_not_found",
                "bound Research Session authorization epoch does not exist",
            )
        })?;
    let completion = facts
        .completions
        .iter()
        .find(|completion| {
            completion.session_id == request.research_session_id()
                && completion.roster_version == request.research_session_roster_version()
        })
        .ok_or_else(|| {
            ApiError::not_found(
                "nakama_completion_not_found",
                "bound Research Session completion does not exist",
            )
        })?;
    let latest_roster_version = facts
        .authorization_sets
        .iter()
        .filter(|set| set.session_id == request.research_session_id())
        .map(|set| set.roster_version)
        .max();
    let appeal = resolve_appeal_facts(facts, evaluation)?;
    let legacy = PaperTrnmCommandBindingV1 {
        schema: PAPER_TRNM_COMMAND_BINDING_SCHEMA_V1.to_string(),
        paper_project_id: facts.paper.paper_project_id,
        submission_id: facts.submission.submission_id,
        evaluation_id: evaluation.evaluation_id,
        research_session_id: request.research_session_id().to_string(),
        research_session_roster_version: request.research_session_roster_version(),
        match_evidence_commitment_id: completion.commitment_id.clone(),
        match_evidence_object_version: PAPER_TRNM_MATCH_EVIDENCE_OBJECT_VERSION_V1,
        release_candidate_hash: facts.submission.release_candidate_hash.clone(),
        paper_bundle_hash: facts.submission.paper_bundle_hash.clone(),
        submission_commitment_hash: String::new(),
        tolerance_policy_hash: evaluation.tolerance_policy_hash.clone(),
        evaluation_signing_hash: evaluation.evaluation_signing_hash.clone(),
        reproduction_id: reproduction.reproduction_id,
        reproduction_report_hash: reproduction.report_hash.clone(),
        evaluation_score_bps: evaluation.paper_score.score_bps,
        evaluation_accepted: evaluation.status == PaperEvaluationStatus::Accepted,
        evaluation_completed_at_unix_s: positive_unix(evaluation.created_at, "evaluation")?,
    };
    let mut legacy = legacy;
    legacy.submission_commitment_hash =
        crate::paper_chain_finality_v1::paper_trnm_submission_commitment_hash(&legacy)?;
    validate_current_binding_records(
        &legacy,
        &facts.paper,
        &facts.submission,
        evaluation,
        reproduction,
    )?;
    validate_match_evidence_binding(
        &legacy,
        &facts.submission,
        authorization_set,
        completion,
        latest_roster_version,
        security,
    )?;
    Ok(PaperFinalitySourceCandidateV2 {
        evaluation,
        reproduction,
        authorization_set,
        completion,
        latest_roster_version,
        appeal,
    })
}

fn source_fingerprint_v2<R: FinalitySourceSelectorV2>(
    facts: &PaperFinalityFactsV2,
    request: &R,
) -> Result<String, ApiError> {
    let mut evaluations = facts.evaluations.clone();
    evaluations.sort_by_key(|record| record.evaluation_id);
    let mut reproductions = facts.reproductions.clone();
    reproductions.sort_by_key(|record| record.reproduction_id);
    let mut appeals = facts.appeals.clone();
    appeals.sort_by_key(|record| record.appeal_id);
    let mut resolutions = facts.resolutions.clone();
    resolutions.sort_by_key(|record| record.resolution_id);
    let mut authorization_sets = facts.authorization_sets.clone();
    authorization_sets.sort_by(|left, right| {
        (&left.session_id, left.roster_version).cmp(&(&right.session_id, right.roster_version))
    });
    let mut completions = facts.completions.clone();
    completions.sort_by(|left, right| {
        (&left.session_id, left.roster_version).cmp(&(&right.session_id, right.roster_version))
    });
    canonical_json_sha256(&json!({
        "domain":"hepta.paper_raid.trnm_finality_source_fingerprint.v2",
        "paper":facts.paper,
        "submission":facts.submission,
        "evaluations":evaluations,
        "reproductions":reproductions,
        "appeals":appeals,
        "resolutions":resolutions,
        "authorization_sets":authorization_sets,
        "completions":completions,
        "selector":{
            "submission_id":request.submission_id(),
            "evaluation_id":request.evaluation_id(),
            "latest_reproduction_id":request.latest_reproduction_id(),
            "research_session_id":request.research_session_id(),
            "research_session_roster_version":request.research_session_roster_version(),
        }
    }))
    .map_err(|message| ApiError::internal(format!("encode V2 source fingerprint: {message}")))
}

fn derive_binding_v2(
    facts: &PaperFinalityFactsV2,
    request: &PreparePaperTrnmFinalityV2Request,
    arm: &PaperTrnmFinalityWindowArmV2,
    final_checkpoint: &PaperTrnmChainTimeCheckpointV1,
    security: &SecurityConfig,
) -> Result<PaperTrnmCommandBindingV2, ApiError> {
    let source = validate_source_candidate_v2(facts, request, security)?;
    let evaluation = source.evaluation;
    let reproduction = source.reproduction;
    let appeal = source.appeal;

    let evaluation_completed_at_unix_s = positive_unix(evaluation.created_at, "evaluation")?;
    let reproduction_completed_at_unix_s = positive_unix(reproduction.created_at, "reproduction")?;
    let submission_commitment_hash = paper_submission_commitment_hash_v2(
        facts.paper.paper_project_id,
        facts.submission.submission_id,
        &facts.submission.release_candidate_hash,
        &facts.submission.paper_bundle_hash,
    )?;
    let author_consent_set_hash = author_consent_set_hash_v1(&facts.submission)?;
    let appeal_window_closes_at_unix_ms = arm.earliest_final_checkpoint_time_unix_ms;
    let appeal_window_closes_at_unix_s =
        ceil_millis_to_seconds(appeal_window_closes_at_unix_ms, "appeal window close")?;
    let finalized_at_unix_s = ceil_millis_to_seconds(
        final_checkpoint.consensus_time_unix_ms,
        "final checkpoint time",
    )?;
    let settlement_policy_hash = paper_scientific_finality_policy_hash_v1()?;
    let mut binding = PaperTrnmCommandBindingV2 {
        schema: PAPER_TRNM_COMMAND_BINDING_SCHEMA_V2.to_string(),
        commitment_id: PAPER_TRNM_COMMITMENT_ID_ZERO.to_string(),
        source_fingerprint: arm.source_fingerprint.clone(),
        window_arm_id: arm.arm_id,
        paper_project_id: facts.paper.paper_project_id,
        submission_id: facts.submission.submission_id,
        research_session_id: request.research_session_id.clone(),
        research_session_roster_version: request.research_session_roster_version,
        match_evidence_commitment_id: source.completion.commitment_id.clone(),
        match_evidence_object_version: PAPER_TRNM_MATCH_EVIDENCE_OBJECT_VERSION_V1,
        release_candidate_hash: facts.submission.release_candidate_hash.clone(),
        paper_bundle_hash: facts.submission.paper_bundle_hash.clone(),
        submission_commitment_hash,
        author_consent_set_hash,
        tolerance_policy_hash: evaluation.tolerance_policy_hash.clone(),
        evaluation_id: evaluation.evaluation_id,
        evaluation_signing_hash: evaluation.evaluation_signing_hash.clone(),
        evaluation_score_bps: evaluation.paper_score.score_bps,
        evaluation_accepted: evaluation.status == PaperEvaluationStatus::Accepted,
        evaluation_completed_at_unix_s,
        evaluation_supersedes_evaluation_id: evaluation.supersedes_evaluation_id,
        evaluation_superseded_by_evaluation_id: None,
        latest_reproduction_id: reproduction.reproduction_id,
        latest_reproduction_report_hash: reproduction.report_hash.clone(),
        latest_reproduction_accepted: reproduction.status == ReproductionStatus::Reproduced,
        latest_reproduction_completed_at_unix_s: reproduction_completed_at_unix_s,
        reproduction_supersedes_reproduction_id: reproduction.supersedes_reproduction_id,
        reproduction_superseded_by_reproduction_id: None,
        appeal_status: appeal.status,
        appeal_id: appeal.appeal_id,
        appealed_evaluation_id: appeal.appealed_evaluation_id,
        appeal_resolution_id: appeal.resolution_id,
        appeal_resolution_hash: appeal.resolution_hash,
        start_checkpoint_hash: arm.start_checkpoint.checkpoint_hash.clone(),
        start_checkpoint_anchor_hash: arm.start_checkpoint.trust_anchor_hash.clone(),
        start_checkpoint_chain_id: arm.start_checkpoint.chain_id.clone(),
        start_checkpoint_height: arm.start_checkpoint.height,
        start_checkpoint_header_hash: arm.start_checkpoint.header_hash.clone(),
        start_checkpoint_consensus_time_unix_ms: arm.start_checkpoint.consensus_time_unix_ms,
        final_checkpoint_hash: final_checkpoint.checkpoint_hash.clone(),
        final_checkpoint_anchor_hash: final_checkpoint.trust_anchor_hash.clone(),
        final_checkpoint_chain_id: final_checkpoint.chain_id.clone(),
        final_checkpoint_height: final_checkpoint.height,
        final_checkpoint_header_hash: final_checkpoint.header_hash.clone(),
        final_checkpoint_consensus_time_unix_ms: final_checkpoint.consensus_time_unix_ms,
        max_chain_time_lag_ms: arm.max_chain_time_lag_ms,
        appeal_window_closes_at_unix_ms,
        appeal_window_closes_at_unix_s,
        settlement_policy_hash,
        scientific_finality: true,
        score_eligible: false,
        ranking_eligible: false,
        reward_eligible: false,
        economic_eligible: false,
        finalized_at_unix_s,
    };
    let legacy = legacy_binding_for_shared_validation(&binding)?;
    validate_current_binding_records(
        &legacy,
        &facts.paper,
        &facts.submission,
        evaluation,
        reproduction,
    )?;
    validate_match_evidence_binding(
        &legacy,
        &facts.submission,
        source.authorization_set,
        source.completion,
        source.latest_roster_version,
        security,
    )?;
    binding.commitment_id = binding_commitment_id_v2(&binding)?;
    validate_binding_v2(&binding)?;
    Ok(binding)
}

fn require_unique_latest_evaluation<'a, R: FinalitySourceSelectorV2>(
    facts: &'a PaperFinalityFactsV2,
    request: &R,
) -> Result<&'a PaperEvaluation, ApiError> {
    let candidates = facts
        .evaluations
        .iter()
        .filter(|evaluation| evaluation.submission_id == request.submission_id())
        .collect::<Vec<_>>();
    let latest = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            !candidates.iter().any(|successor| {
                successor.supersedes_evaluation_id == Some(candidate.evaluation_id)
            })
        })
        .collect::<Vec<_>>();
    if latest.len() != 1 {
        return Err(ApiError::conflict(
            "paper_trnm_latest_evaluation_ambiguous",
            "Paper finality requires exactly one unsuperseded evaluation",
        ));
    }
    if latest[0].evaluation_id != request.evaluation_id() {
        return Err(ApiError::conflict(
            "paper_trnm_evaluation_superseded",
            "requested evaluation is not the unique latest evaluation",
        ));
    }
    Ok(latest[0])
}

fn require_unique_latest_reproduction<'a, R: FinalitySourceSelectorV2>(
    facts: &'a PaperFinalityFactsV2,
    request: &R,
    evaluation: &PaperEvaluation,
) -> Result<&'a PaperReproduction, ApiError> {
    let candidates = facts
        .reproductions
        .iter()
        .filter(|reproduction| reproduction.evaluation_id == evaluation.evaluation_id)
        .collect::<Vec<_>>();
    let latest = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            !candidates.iter().any(|successor| {
                successor.supersedes_reproduction_id == Some(candidate.reproduction_id)
            })
        })
        .collect::<Vec<_>>();
    if latest.len() != 1 {
        return Err(ApiError::conflict(
            "paper_trnm_latest_reproduction_ambiguous",
            "Paper finality requires exactly one unsuperseded reproduction",
        ));
    }
    if latest[0].reproduction_id != request.latest_reproduction_id() {
        return Err(ApiError::conflict(
            "paper_trnm_reproduction_superseded",
            "requested reproduction is not the unique latest reproduction",
        ));
    }
    Ok(latest[0])
}

#[derive(Clone)]
struct ResolvedAppealFactsV2 {
    status: PaperTrnmAppealStatusV2,
    appeal_id: Option<Uuid>,
    appealed_evaluation_id: Option<Uuid>,
    resolution_id: Option<Uuid>,
    resolution_hash: Option<String>,
}

fn resolve_appeal_facts(
    facts: &PaperFinalityFactsV2,
    evaluation: &PaperEvaluation,
) -> Result<ResolvedAppealFactsV2, ApiError> {
    let lineage = evaluation_lineage(facts, evaluation)?;
    let lineage_ids = lineage
        .iter()
        .map(|record| record.evaluation_id)
        .collect::<HashSet<_>>();
    let lineage_appeals = facts
        .appeals
        .iter()
        .filter(|appeal| lineage_ids.contains(&appeal.evaluation_id))
        .collect::<Vec<_>>();
    let lineage_appeal_ids = lineage_appeals
        .iter()
        .map(|appeal| appeal.appeal_id)
        .collect::<HashSet<_>>();

    if facts.resolutions.iter().any(|resolution| {
        resolution.superseding_evaluation_id == Some(evaluation.evaluation_id)
            && !lineage_appeal_ids.contains(&resolution.appeal_id)
    }) {
        return Err(ApiError::conflict(
            "paper_trnm_appeal_lineage_mismatch",
            "a resolution outside the final evaluation lineage claims the latest evaluation",
        ));
    }

    let mut resolved = Vec::with_capacity(lineage_appeals.len());
    for appeal in lineage_appeals {
        let resolutions = facts
            .resolutions
            .iter()
            .filter(|resolution| resolution.appeal_id == appeal.appeal_id)
            .collect::<Vec<_>>();
        if resolutions.is_empty() {
            return Err(ApiError::conflict(
                "paper_chain_finality_open_appeal",
                "an unresolved Appeal anywhere in the final evaluation lineage holds Paper scientific finality",
            ));
        }
        if resolutions.len() != 1 || resolutions[0].evaluation_id != appeal.evaluation_id {
            return Err(ApiError::conflict(
                "paper_trnm_appeal_lineage_ambiguous",
                "Appeal resolution lineage is missing, duplicated, or bound to another evaluation",
            ));
        }
        resolved.push((appeal, resolutions[0]));
    }

    if resolved.len() > 1 {
        return Err(ApiError::conflict(
            "paper_trnm_appeal_lineage_ambiguous",
            "Paper finality supports exactly one resolved Appeal in an evaluation lineage",
        ));
    }

    if let Some((appeal, resolution)) = resolved.first().copied() {
        let status = match resolution.outcome {
            AppealOutcome::Denied
                if appeal.evaluation_id == evaluation.evaluation_id
                    && evaluation.supersedes_evaluation_id.is_none()
                    && resolution.superseding_evaluation_id.is_none() =>
            {
                PaperTrnmAppealStatusV2::ResolvedDenied
            }
            AppealOutcome::Upheld
                if appeal.evaluation_id != evaluation.evaluation_id
                    && evaluation.supersedes_evaluation_id == Some(appeal.evaluation_id)
                    && resolution.superseding_evaluation_id == Some(evaluation.evaluation_id) =>
            {
                PaperTrnmAppealStatusV2::ResolvedUpheld
            }
            _ => {
                return Err(ApiError::conflict(
                    "paper_trnm_appeal_lineage_mismatch",
                    "only a denied final evaluation or an exact upheld direct replacement can reach Paper finality",
                ));
            }
        };
        return Ok(ResolvedAppealFactsV2 {
            status,
            appeal_id: Some(appeal.appeal_id),
            appealed_evaluation_id: Some(appeal.evaluation_id),
            resolution_id: Some(resolution.resolution_id),
            resolution_hash: Some(resolution.decision_hash.clone()),
        });
    }

    if evaluation.supersedes_evaluation_id.is_some() {
        return Err(ApiError::conflict(
            "paper_trnm_appeal_lineage_mismatch",
            "an evaluation replacement can reach finality only through its exact upheld Appeal",
        ));
    }
    Ok(ResolvedAppealFactsV2 {
        status: PaperTrnmAppealStatusV2::ClosedNoAppeal,
        appeal_id: None,
        appealed_evaluation_id: None,
        resolution_id: None,
        resolution_hash: None,
    })
}

fn evaluation_lineage<'a>(
    facts: &'a PaperFinalityFactsV2,
    latest: &'a PaperEvaluation,
) -> Result<Vec<&'a PaperEvaluation>, ApiError> {
    let by_id = facts
        .evaluations
        .iter()
        .filter(|record| record.submission_id == latest.submission_id)
        .map(|record| (record.evaluation_id, record))
        .collect::<HashMap<_, _>>();
    let mut lineage = Vec::new();
    let mut seen = HashSet::new();
    let mut current = latest;
    loop {
        if !seen.insert(current.evaluation_id) {
            return Err(ApiError::conflict(
                "paper_trnm_evaluation_lineage_cycle",
                "evaluation supersession lineage contains a cycle",
            ));
        }
        lineage.push(current);
        let Some(parent_id) = current.supersedes_evaluation_id else {
            break;
        };
        current = by_id.get(&parent_id).copied().ok_or_else(|| {
            ApiError::conflict(
                "paper_trnm_evaluation_lineage_missing",
                "latest evaluation supersedes an evaluation outside its exact submission lineage",
            )
        })?;
    }
    Ok(lineage)
}

fn legacy_binding_for_shared_validation(
    binding: &PaperTrnmCommandBindingV2,
) -> Result<PaperTrnmCommandBindingV1, ApiError> {
    let mut legacy = PaperTrnmCommandBindingV1 {
        schema: PAPER_TRNM_COMMAND_BINDING_SCHEMA_V1.to_string(),
        paper_project_id: binding.paper_project_id,
        submission_id: binding.submission_id,
        evaluation_id: binding.evaluation_id,
        research_session_id: binding.research_session_id.clone(),
        research_session_roster_version: binding.research_session_roster_version,
        match_evidence_commitment_id: binding.match_evidence_commitment_id.clone(),
        match_evidence_object_version: binding.match_evidence_object_version,
        release_candidate_hash: binding.release_candidate_hash.clone(),
        paper_bundle_hash: binding.paper_bundle_hash.clone(),
        submission_commitment_hash: String::new(),
        tolerance_policy_hash: binding.tolerance_policy_hash.clone(),
        evaluation_signing_hash: binding.evaluation_signing_hash.clone(),
        reproduction_id: binding.latest_reproduction_id,
        reproduction_report_hash: binding.latest_reproduction_report_hash.clone(),
        evaluation_score_bps: binding.evaluation_score_bps,
        evaluation_accepted: binding.evaluation_accepted,
        evaluation_completed_at_unix_s: binding.evaluation_completed_at_unix_s,
    };
    legacy.submission_commitment_hash =
        crate::paper_chain_finality_v1::paper_trnm_submission_commitment_hash(&legacy)?;
    Ok(legacy)
}

fn paper_submission_commitment_hash_v2(
    paper_project_id: Uuid,
    submission_id: Uuid,
    release_candidate_hash: &str,
    paper_bundle_hash: &str,
) -> Result<String, ApiError> {
    let release = crate::decode_digest(release_candidate_hash)
        .map_err(|message| ApiError::bad_request("invalid_release_candidate_hash", message))?;
    let bundle = crate::decode_digest(paper_bundle_hash)
        .map_err(|message| ApiError::bad_request("invalid_paper_bundle_hash", message))?;
    let mut bytes = Vec::with_capacity(128);
    bytes.extend_from_slice(PAPER_TRNM_SUBMISSION_BINDING_DOMAIN_V2);
    bytes.extend_from_slice(paper_project_id.as_bytes());
    bytes.extend_from_slice(submission_id.as_bytes());
    bytes.extend_from_slice(&release);
    bytes.extend_from_slice(&bundle);
    Ok(format!("sha256:{}", sha256_hex(&bytes)))
}

#[derive(Serialize)]
struct CanonicalAuthorConsentSetV1<'a> {
    schema: &'static str,
    release_candidate_hash: &'a str,
    author_consents: Vec<PaperBundleAuthorConsentV2>,
}

fn author_consent_set_hash_v1(submission: &JointPaperSubmission) -> Result<String, ApiError> {
    if paper_bundle_hash(&submission.paper_bundle).map_err(|message| {
        ApiError::conflict(
            "paper_trnm_paper_bundle_invalid",
            format!("stored PaperBundle is invalid: {message}"),
        )
    })? != submission.paper_bundle_hash
    {
        return Err(ApiError::conflict(
            "paper_trnm_paper_bundle_hash_mismatch",
            "stored PaperBundle bytes do not match the submission hash",
        ));
    }
    let mut author_consents = submission.paper_bundle.author_consents.clone();
    author_consents.sort_by_key(|consent| consent.author_order);
    canonical_json_sha256(&CanonicalAuthorConsentSetV1 {
        schema: PAPER_AUTHOR_CONSENT_SET_SCHEMA_V1,
        release_candidate_hash: &submission.release_candidate_hash,
        author_consents,
    })
    .map_err(|message| ApiError::internal(format!("encode author consent set: {message}")))
}

pub(crate) fn binding_commitment_id_v2(
    binding: &PaperTrnmCommandBindingV2,
) -> Result<String, ApiError> {
    let mut preimage = binding.clone();
    preimage.commitment_id = PAPER_TRNM_COMMITMENT_ID_ZERO.to_string();
    canonical_json_sha256(&json!({
        "domain":"hepta.paper_raid.trnm_finality_commitment_id.v2",
        "binding":preimage,
    }))
    .map_err(|message| ApiError::internal(format!("derive V2 commitment id: {message}")))
}

pub(crate) fn validate_binding_v2(binding: &PaperTrnmCommandBindingV2) -> Result<(), ApiError> {
    if binding.schema != PAPER_TRNM_COMMAND_BINDING_SCHEMA_V2 {
        return Err(ApiError::bad_request(
            "unsupported_paper_trnm_binding_schema",
            format!("expected {PAPER_TRNM_COMMAND_BINDING_SCHEMA_V2}"),
        ));
    }
    for (field, value) in [
        ("commitment_id", &binding.commitment_id),
        ("source_fingerprint", &binding.source_fingerprint),
        ("start_checkpoint_hash", &binding.start_checkpoint_hash),
        ("final_checkpoint_hash", &binding.final_checkpoint_hash),
        (
            "match_evidence_commitment_id",
            &binding.match_evidence_commitment_id,
        ),
        ("release_candidate_hash", &binding.release_candidate_hash),
        ("paper_bundle_hash", &binding.paper_bundle_hash),
        (
            "submission_commitment_hash",
            &binding.submission_commitment_hash,
        ),
        ("author_consent_set_hash", &binding.author_consent_set_hash),
        ("tolerance_policy_hash", &binding.tolerance_policy_hash),
        ("evaluation_signing_hash", &binding.evaluation_signing_hash),
        (
            "latest_reproduction_report_hash",
            &binding.latest_reproduction_report_hash,
        ),
        ("settlement_policy_hash", &binding.settlement_policy_hash),
    ] {
        crate::validate_hash(field, value)?;
    }
    if let Some(hash) = &binding.appeal_resolution_hash {
        crate::validate_hash("appeal_resolution_hash", hash)?;
    }
    validate_raw_hex_32(
        "start_checkpoint_anchor_hash",
        &binding.start_checkpoint_anchor_hash,
    )?;
    validate_raw_hex_32(
        "start_checkpoint_header_hash",
        &binding.start_checkpoint_header_hash,
    )?;
    validate_raw_hex_32(
        "final_checkpoint_anchor_hash",
        &binding.final_checkpoint_anchor_hash,
    )?;
    validate_raw_hex_32(
        "final_checkpoint_header_hash",
        &binding.final_checkpoint_header_hash,
    )?;
    if binding.commitment_id == PAPER_TRNM_COMMITMENT_ID_ZERO
        || binding.match_evidence_object_version != PAPER_TRNM_MATCH_EVIDENCE_OBJECT_VERSION_V1
        || binding.research_session_roster_version == 0
        || binding.evaluation_score_bps > 10_000
        || (binding.evaluation_accepted && binding.evaluation_score_bps == 0)
        || !binding.scientific_finality
        || binding.score_eligible
        || binding.ranking_eligible
        || binding.reward_eligible
        || binding.economic_eligible
        || binding.evaluation_superseded_by_evaluation_id.is_some()
        || binding.reproduction_superseded_by_reproduction_id.is_some()
        || binding.start_checkpoint_chain_id.is_empty()
        || binding.final_checkpoint_chain_id != binding.start_checkpoint_chain_id
        || binding.start_checkpoint_hash == binding.final_checkpoint_hash
        || binding.start_checkpoint_height == 0
        || binding.final_checkpoint_height <= binding.start_checkpoint_height
        || binding.max_chain_time_lag_ms != PAPER_CHAIN_TIME_MAX_LAG_MS_V1
    {
        return Err(ApiError::conflict(
            "paper_trnm_v2_finality_invariant_failed",
            "V2 binding violates scientific-finality or fail-closed eligibility invariants",
        ));
    }
    let policy_delay_ms = match binding.appeal_status {
        PaperTrnmAppealStatusV2::ClosedNoAppeal => u64::try_from(PAPER_NO_APPEAL_WINDOW_SECONDS_V1)
            .map_err(|_| ApiError::internal("Paper Appeal window policy is negative"))?
            .checked_mul(1_000)
            .ok_or_else(|| ApiError::internal("Paper Appeal window milliseconds overflow"))?,
        PaperTrnmAppealStatusV2::ResolvedDenied | PaperTrnmAppealStatusV2::ResolvedUpheld => 0,
    };
    let expected_window_close = binding
        .start_checkpoint_consensus_time_unix_ms
        .checked_add(binding.max_chain_time_lag_ms)
        .and_then(|value| value.checked_add(policy_delay_ms))
        .ok_or_else(|| ApiError::internal("Paper Chain-time Appeal deadline overflow"))?;
    if binding.evaluation_completed_at_unix_s == 0
        || binding.latest_reproduction_completed_at_unix_s < binding.evaluation_completed_at_unix_s
        || binding.appeal_window_closes_at_unix_s < binding.latest_reproduction_completed_at_unix_s
        || binding.finalized_at_unix_s < binding.appeal_window_closes_at_unix_s
        || binding.final_checkpoint_consensus_time_unix_ms < binding.appeal_window_closes_at_unix_ms
        || binding.start_checkpoint_consensus_time_unix_ms
            >= binding.final_checkpoint_consensus_time_unix_ms
        || binding.appeal_window_closes_at_unix_ms != expected_window_close
        || ceil_millis_to_seconds(
            binding.appeal_window_closes_at_unix_ms,
            "appeal window close",
        )? != binding.appeal_window_closes_at_unix_s
        || ceil_millis_to_seconds(
            binding.final_checkpoint_consensus_time_unix_ms,
            "final checkpoint time",
        )? != binding.finalized_at_unix_s
    {
        return Err(ApiError::conflict(
            "paper_trnm_v2_timestamp_regression",
            "V2 binding timestamps regress or use a future Appeal window",
        ));
    }
    match binding.appeal_status {
        PaperTrnmAppealStatusV2::ClosedNoAppeal => {
            if binding.appeal_id.is_some()
                || binding.appealed_evaluation_id.is_some()
                || binding.appeal_resolution_id.is_some()
                || binding.appeal_resolution_hash.is_some()
                || binding.evaluation_supersedes_evaluation_id.is_some()
            {
                return Err(ApiError::conflict(
                    "paper_trnm_v2_appeal_inconsistent",
                    "closed_no_appeal must not carry Appeal identity or resolution fields",
                ));
            }
        }
        PaperTrnmAppealStatusV2::ResolvedDenied => {
            if binding.appeal_id.is_none()
                || binding.appealed_evaluation_id != Some(binding.evaluation_id)
                || binding.appeal_resolution_id.is_none()
                || binding.appeal_resolution_hash.is_none()
                || binding.evaluation_supersedes_evaluation_id.is_some()
            {
                return Err(ApiError::conflict(
                    "paper_trnm_v2_appeal_inconsistent",
                    "resolved_denied must bind the final evaluation and complete resolution",
                ));
            }
        }
        PaperTrnmAppealStatusV2::ResolvedUpheld => {
            let appealed = binding.appealed_evaluation_id.ok_or_else(|| {
                ApiError::conflict(
                    "paper_trnm_v2_appeal_inconsistent",
                    "resolved_upheld requires appealed_evaluation_id",
                )
            })?;
            if binding.appeal_id.is_none()
                || binding.appeal_resolution_id.is_none()
                || binding.appeal_resolution_hash.is_none()
                || appealed == binding.evaluation_id
                || binding.evaluation_supersedes_evaluation_id != Some(appealed)
            {
                return Err(ApiError::conflict(
                    "paper_trnm_v2_appeal_inconsistent",
                    "resolved_upheld must bind the exact latest superseding evaluation",
                ));
            }
        }
    }
    if binding.commitment_id != binding_commitment_id_v2(binding)? {
        return Err(ApiError::conflict(
            "paper_trnm_v2_commitment_id_mismatch",
            "commitment_id does not bind the exact immutable V2 tuple",
        ));
    }
    Ok(())
}

fn build_preparation(
    idempotency_key: String,
    request_hash: String,
    binding: PaperTrnmCommandBindingV2,
    final_checkpoint_consensus_time_unix_ms: u64,
) -> Result<PaperTrnmFinalityPreparationV2, ApiError> {
    let binding_fingerprint = canonical_json_sha256(&binding)
        .map_err(|message| ApiError::internal(format!("encode V2 binding: {message}")))?;
    Ok(PaperTrnmFinalityPreparationV2 {
        schema: PAPER_TRNM_FINALITY_PREPARATION_SCHEMA_V2.to_string(),
        preparation_id: Uuid::new_v4(),
        idempotency_key,
        request_hash,
        binding,
        binding_fingerprint,
        status: PaperTrnmFinalityPreparationStatusV2::AwaitingChainVerifierUpgrade,
        created_at: datetime_from_unix_millis(
            final_checkpoint_consensus_time_unix_ms,
            "final checkpoint time",
        )?,
    })
}

fn positive_unix(value: DateTime<Utc>, field: &'static str) -> Result<u64, ApiError> {
    u64::try_from(value.timestamp()).map_err(|_| {
        ApiError::conflict(
            "paper_trnm_v2_timestamp_invalid",
            format!("{field} must be at or after the Unix epoch"),
        )
    })
}

fn ceil_millis_to_seconds(value: u64, field: &'static str) -> Result<u64, ApiError> {
    value
        .checked_add(999)
        .map(|rounded| rounded / 1_000)
        .ok_or_else(|| ApiError::internal(format!("{field} milliseconds overflow")))
}

fn datetime_from_unix_millis(value: u64, field: &'static str) -> Result<DateTime<Utc>, ApiError> {
    let value = i64::try_from(value)
        .map_err(|_| ApiError::internal(format!("{field} exceeds i64 milliseconds")))?;
    DateTime::<Utc>::from_timestamp_millis(value)
        .ok_or_else(|| ApiError::internal(format!("{field} is outside chrono range")))
}

fn system_time_unix_millis_v2(value: SystemTime, field: &'static str) -> Result<u64, ApiError> {
    u64::try_from(
        value
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                ApiError::conflict(
                    "paper_trnm_v2_local_verification_clock_invalid",
                    format!("{field} predates the Unix epoch"),
                )
            })?
            .as_millis(),
    )
    .map_err(|_| ApiError::internal(format!("{field} exceeds u64 milliseconds")))
}

fn i64_from_u64_v2(value: u64, field: &'static str) -> Result<i64, ApiError> {
    i64::try_from(value).map_err(|_| ApiError::internal(format!("{field} exceeds i64")))
}

fn validate_raw_hex_32(field: &'static str, value: &str) -> Result<(), ApiError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ApiError::bad_request(
            "invalid_chain_digest",
            format!("{field} must be 64 lowercase hexadecimal characters"),
        ));
    }
    Ok(())
}

fn validate_anchor_hash(value: &str) -> Result<(), ApiError> {
    validate_raw_hex_32("trust_anchor_hash", value)
}

fn validate_idempotency_key(value: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(ApiError::bad_request(
            "invalid_idempotency_key",
            "idempotency_key must contain 1-128 canonical ASCII token bytes",
        ));
    }
    Ok(())
}

fn decode_record<T: DeserializeOwned>(value: Value, label: &str) -> Result<T, ApiError> {
    serde_json::from_value(value)
        .map_err(|error| ApiError::internal(format!("decode stored {label}: {error}")))
}
