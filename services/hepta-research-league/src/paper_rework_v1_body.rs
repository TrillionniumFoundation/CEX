use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signature, Verifier};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use super::*;
use crate::paper_raid_contracts::{
    paper_rework_signing_bytes, PaperReworkSigningV1, PAPER_REWORK_V1,
};

pub const PAPER_REWORK_RECORD_SCHEMA_V1: &str = "hepta.paper_raid.rework_record.v1";
pub const PAPER_REWORK_RESUBMISSION_SCHEMA_V1: &str = "hepta.paper_raid.rework_resubmission.v1";
pub const PAPER_REWORK_LEASE_HOURS: i64 = 24;
const ZERO_SHA256_DIGEST: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StartPaperReworkRequestV1 {
    pub rework_id: Uuid,
    pub rejected_evaluation_id: Uuid,
    pub rejected_submission_id: Uuid,
    pub expected_paper_version: u64,
    pub rework_cycle: u64,
    pub author_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub reason_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperReworkRecordV1 {
    pub schema: String,
    pub rework_id: Uuid,
    pub paper_project_id: Uuid,
    pub rejected_evaluation_id: Uuid,
    pub rejected_submission_id: Uuid,
    pub rejected_revision_id: Uuid,
    pub rejected_release_candidate_hash: String,
    pub rejected_paper_bundle_hash: String,
    pub rejected_rework_content_commitment_sha256: String,
    pub rework_cycle: u64,
    pub author_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub reason_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub request_hash: String,
    pub rework_expires_at: DateTime<Utc>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperReworkResubmissionV1 {
    pub schema: String,
    pub rework_id: Uuid,
    pub paper_project_id: Uuid,
    pub rejected_submission_id: Uuid,
    pub replacement_submission_id: Uuid,
    pub replacement_revision_id: Uuid,
    pub replacement_release_candidate_hash: String,
    pub replacement_paper_bundle_hash: String,
    pub replacement_rework_content_commitment_sha256: String,
    pub replacement_review_round: u64,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperReworkStateV1 {
    pub rework: PaperReworkRecordV1,
    pub resubmission: Option<PaperReworkResubmissionV1>,
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route(
        "/v2/hepta/papers/:paper_id/reworks",
        get(get_paper_reworks).post(start_paper_rework),
    )
}

fn signing_claim(paper_id: Uuid, request: &StartPaperReworkRequestV1) -> PaperReworkSigningV1 {
    PaperReworkSigningV1 {
        schema: PAPER_REWORK_V1.to_string(),
        rework_id: request.rework_id,
        paper_project_id: paper_id,
        rejected_evaluation_id: request.rejected_evaluation_id,
        rejected_submission_id: request.rejected_submission_id,
        expected_paper_version: request.expected_paper_version,
        rework_cycle: request.rework_cycle,
        author_player_id: request.author_player_id,
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        reason_hash: request.reason_hash.clone(),
        signed_at_unix: request.signed_at_unix,
    }
}

fn validate_rework_request(
    paper_id: Uuid,
    request: &StartPaperReworkRequestV1,
) -> Result<Vec<u8>, ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest_v2("reason_hash", &request.reason_hash)?;
    validate_digest_v2("signing_public_key_hash", &request.signing_public_key_hash)?;
    validate_contract_text_api("signing_key_id", &request.signing_key_id)?;
    let claim = signing_claim(paper_id, request);
    paper_rework_signing_bytes(&claim)
        .map_err(|message| ApiError::bad_request("invalid_paper_rework_contract", message))
}

fn validate_signed_at(value: i64) -> Result<DateTime<Utc>, ApiError> {
    let signed_at = Utc.timestamp_opt(value, 0).single().ok_or_else(|| {
        ApiError::bad_request("invalid_signed_at", "signed timestamp is out of range")
    })?;
    let now = Utc::now();
    if signed_at > now + chrono::Duration::minutes(5)
        || signed_at < now - chrono::Duration::hours(24)
    {
        return Err(ApiError::forbidden(
            "signed_at_outside_window",
            "Paper rework signature is outside the accepted replay window",
        ));
    }
    Ok(signed_at)
}

fn verify_author_signature(
    player: &HumanPlayer,
    key_history: &HumanSigningKey,
    request: &StartPaperReworkRequestV1,
    frame: &[u8],
) -> Result<(), ApiError> {
    if player.status != HumanPlayerStatus::Active
        || player.player_id != request.author_player_id
        || player.signing_key_id != request.signing_key_id
        || player.signing_public_key != request.signing_public_key
        || player.signing_public_key_hash != request.signing_public_key_hash
        || key_history.player_id != player.player_id
        || key_history.signing_key_id != request.signing_key_id
        || key_history.signing_public_key != request.signing_public_key
        || key_history.signing_public_key_hash != request.signing_public_key_hash
        || key_history.status != HumanSigningKeyStatus::Active
        || key_history.retired_at.is_some()
        || key_history.revoked_at.is_some()
    {
        return Err(ApiError::forbidden(
            "paper_rework_author_key_inactive",
            "Paper rework requires the author's current active, unrevoked signing key",
        ));
    }
    let key = crate::decode_verifying_key(&request.signing_public_key)?;
    if crate::paper_raid_contracts::sha256_digest(&key.to_bytes())
        != request.signing_public_key_hash
    {
        return Err(ApiError::bad_request(
            "signing_public_key_hash_mismatch",
            "signing_public_key_hash does not match the supplied Ed25519 key",
        ));
    }
    let signature_bytes = BASE64.decode(&request.signature).map_err(|_| {
        ApiError::bad_request(
            "invalid_signature",
            "Paper rework signature must be canonical padded base64",
        )
    })?;
    if BASE64.encode(&signature_bytes) != request.signature {
        return Err(ApiError::bad_request(
            "invalid_signature",
            "Paper rework signature must be canonical padded base64",
        ));
    }
    let signature = Signature::from_slice(&signature_bytes).map_err(|_| {
        ApiError::bad_request(
            "invalid_signature",
            "Paper rework signature must decode to 64 Ed25519 bytes",
        )
    })?;
    key.verify(frame, &signature).map_err(|_| {
        ApiError::forbidden(
            "paper_rework_signature_failed",
            "Paper rework signature verification failed",
        )
    })
}

fn expected_rework_cycle<'a>(
    paper_id: Uuid,
    reworks: impl Iterator<Item = &'a PaperReworkRecordV1>,
) -> Result<u64, ApiError> {
    let next = reworks
        .filter(|record| record.paper_project_id == paper_id)
        .map(|record| record.rework_cycle)
        .max()
        .unwrap_or(1)
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("rework cycle overflow"))?;
    if next > JSON_SAFE_U64_MAX {
        return Err(ApiError::internal("rework cycle exceeds JSON-safe u64"));
    }
    Ok(next)
}

/// Identity-free content authority. UUIDs, signatures, author consents and
/// timestamps are intentionally excluded, so rotating identities cannot pass
/// as a scientific rework.
pub(super) fn rework_content_commitment_sha256(
    submission: &JointPaperSubmission,
) -> Result<String, ApiError> {
    let candidate = &submission.paper_bundle.release_candidate;
    let commitment = canonical_json_sha256(&json!({
        "schema": "hepta.paper_raid.rework_content_commitment.v1",
        "ruleset_hash": candidate.ruleset_hash,
        "challenge_snapshot_hash": candidate.challenge_snapshot_hash,
        "title": candidate.title,
        "abstract_text": candidate.abstract_text,
        "target_format": candidate.target_format,
        "source_manifest_hash": candidate.source_manifest_hash,
        "artifact_manifest_hash": candidate.artifact_manifest_hash,
        "bibliography_hash": candidate.bibliography_hash,
        "claim_evidence_graph_hash": candidate.claim_evidence_graph_hash,
        "collaboration_compact_hash": candidate.collaboration_compact_hash,
        "research_protocol_snapshot_hash": candidate.research_protocol_snapshot_hash,
        "ethics_disclosure_hash": candidate.ethics_disclosure_hash,
        "coi_disclosure_hash": candidate.coi_disclosure_hash,
        "contribution_ledger_hash": candidate.contribution_ledger_hash,
        "ai_disclosure_hash": candidate.ai_disclosure_hash,
        "license": candidate.license,
    }))
    .map_err(|message| {
        ApiError::internal(format!(
            "hash identity-free Paper rework content commitment: {message}"
        ))
    })?;
    if commitment == ZERO_SHA256_DIGEST {
        return Err(ApiError::internal(
            "identity-free Paper rework content commitment cannot be the zero digest",
        ));
    }
    Ok(commitment)
}

fn active_rework_memory(memory: &PaperRaidMemory, paper_id: Uuid) -> Option<&PaperReworkRecordV1> {
    memory.reworks.values().find(|record| {
        record.paper_project_id == paper_id
            && !memory.rework_resubmissions.contains_key(&record.rework_id)
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_rework_state(
    paper: &PaperProject,
    submission: &JointPaperSubmission,
    evaluation: &PaperEvaluation,
    evaluations: &[PaperEvaluation],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
    active_assignment: bool,
    active_rework: bool,
    expected_cycle: u64,
    request: &StartPaperReworkRequestV1,
) -> Result<(), ApiError> {
    if paper.version != request.expected_paper_version {
        return Err(version_conflict(
            "paper project",
            request.expected_paper_version,
            paper.version,
        ));
    }
    if paper.active_rework_id.is_some()
        || paper.active_rework_cycle.is_some()
        || paper.rework_expires_at.is_some()
    {
        return Err(ApiError::conflict(
            "paper_rework_already_active",
            "Paper already has an active server-owned rework lease",
        ));
    }
    if paper.phase != PaperPhase::SubmissionReady
        || paper.outcome != PaperChallengeOutcomeV1::SubmissionReady
        || paper.terminal_at.is_none()
        || submission.paper_project_id != paper.paper_project_id
        || submission.submission_id != request.rejected_submission_id
        || submission.status != JointSubmissionStatus::SubmissionReady
        || paper.current_revision_id != Some(submission.revision_id)
        || paper.release_candidate_revision_id != Some(submission.revision_id)
    {
        return Err(ApiError::conflict(
            "paper_rework_not_terminal_submission",
            "Paper rework requires the exact current terminal submission-ready PaperBundle",
        ));
    }
    if !submission
        .paper_bundle
        .author_consents
        .iter()
        .any(|consent| consent.player_id == request.author_player_id)
    {
        return Err(ApiError::forbidden(
            "paper_rework_author_not_frozen",
            "Paper rework must be signed by an author in the rejected immutable PaperBundle",
        ));
    }
    let latest = evaluations
        .iter()
        .filter(|candidate| {
            candidate.paper_project_id == paper.paper_project_id
                && candidate.submission_id == submission.submission_id
        })
        .max_by_key(|candidate| (candidate.version, candidate.evaluation_id))
        .ok_or_else(|| {
            ApiError::conflict(
                "paper_rework_rejected_evaluation_required",
                "Paper rework requires a terminal rejected evaluation",
            )
        })?;
    if latest.evaluation_id != evaluation.evaluation_id
        || evaluation.evaluation_id != request.rejected_evaluation_id
        || evaluation.submission_id != submission.submission_id
        || evaluation.release_candidate_hash != submission.release_candidate_hash
        || evaluation.paper_bundle_hash != submission.paper_bundle_hash
        || evaluation.status != PaperEvaluationStatus::Rejected
        || evaluations
            .iter()
            .any(|candidate| candidate.supersedes_evaluation_id == Some(evaluation.evaluation_id))
    {
        return Err(ApiError::conflict(
            "paper_rework_evaluation_not_final_rejected",
            "Paper rework requires the latest immutable rejected evaluation for the exact submission",
        ));
    }
    if appeals.iter().any(|appeal| {
        appeal.evaluation_id == evaluation.evaluation_id
            && !resolutions
                .iter()
                .any(|resolution| resolution.appeal_id == appeal.appeal_id)
    }) {
        return Err(ApiError::conflict(
            "paper_rework_open_appeal",
            "Paper rework cannot begin while the rejected evaluation has an open Appeal",
        ));
    }
    if active_assignment {
        return Err(ApiError::conflict(
            "paper_rework_review_assignment_active",
            "Paper rework cannot begin while the rejected submission has an active Review assignment",
        ));
    }
    if active_rework {
        return Err(ApiError::conflict(
            "paper_rework_already_active",
            "Paper already has an active replacement workflow",
        ));
    }
    if request.rework_cycle != expected_cycle {
        return Err(ApiError::conflict(
            "paper_rework_cycle_drift",
            format!("rework_cycle must be the server-derived generation {expected_cycle}"),
        ));
    }
    Ok(())
}

fn make_rework_record(
    paper_id: Uuid,
    submission: &JointPaperSubmission,
    request: &StartPaperReworkRequestV1,
    request_hash: String,
    now: DateTime<Utc>,
    rework_expires_at: DateTime<Utc>,
) -> Result<PaperReworkRecordV1, ApiError> {
    Ok(PaperReworkRecordV1 {
        schema: PAPER_REWORK_RECORD_SCHEMA_V1.to_string(),
        rework_id: request.rework_id,
        paper_project_id: paper_id,
        rejected_evaluation_id: request.rejected_evaluation_id,
        rejected_submission_id: request.rejected_submission_id,
        rejected_revision_id: submission.revision_id,
        rejected_release_candidate_hash: submission.release_candidate_hash.clone(),
        rejected_paper_bundle_hash: submission.paper_bundle_hash.clone(),
        rejected_rework_content_commitment_sha256: rework_content_commitment_sha256(submission)?,
        rework_cycle: request.rework_cycle,
        author_player_id: request.author_player_id,
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key: request.signing_public_key.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        reason_hash: request.reason_hash.clone(),
        signed_at_unix: request.signed_at_unix,
        signature: request.signature.clone(),
        request_hash,
        rework_expires_at,
        version: 1,
        created_at: now,
    })
}

async fn start_paper_rework(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<StartPaperReworkRequestV1>,
) -> Result<(StatusCode, Json<PaperReworkRecordV1>), ApiError> {
    const OPERATION: &str = "start_paper_rework_v1";
    let signing_frame = validate_rework_request(paper_id, &request)?;
    let body_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/reworks");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &body_hash,
    )?;
    if assertion.player_id != request.author_player_id {
        return Err(ApiError::forbidden(
            "paper_rework_assertion_mismatch",
            "Consumer assertion must identify the author signing the Paper rework",
        ));
    }

    if state.pool.is_none() {
        let mut guard = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&guard, OPERATION, &request.idempotency_key, &body_hash)?
        {
            return Ok(replay);
        }
        validate_signed_at(request.signed_at_unix)?;
        ensure_paper_finality_v2_source_unsealed_memory(&state, paper_id).await?;
        let mut next = guard.clone();
        if next.reworks.contains_key(&request.rework_id) {
            return Err(ApiError::conflict(
                "paper_rework_identity_conflict",
                "rework_id already exists",
            ));
        }
        let paper = next.papers.get(&paper_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "paper project does not exist")
        })?;
        let team = next
            .teams
            .get(&paper.team_id)
            .cloned()
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        assert_team_actor_memory(&next, &team, &assertion)?;
        let player = next
            .players
            .get(&request.author_player_id)
            .cloned()
            .ok_or_else(|| ApiError::internal("author player record is missing"))?;
        let key = next
            .human_signing_keys
            .get(&(request.author_player_id, request.signing_key_id.clone()))
            .cloned()
            .ok_or_else(|| {
                ApiError::forbidden(
                    "paper_rework_author_key_inactive",
                    "Paper rework signing key is absent from durable key history",
                )
            })?;
        verify_author_signature(&player, &key, &request, &signing_frame)?;
        let submission = next
            .submissions
            .get(&request.rejected_submission_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found(
                    "joint_submission_not_found",
                    "rejected submission does not exist",
                )
            })?;
        let evaluation = next
            .review
            .evaluations
            .get(&request.rejected_evaluation_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found(
                    "paper_evaluation_not_found",
                    "rejected evaluation does not exist",
                )
            })?;
        let evaluations = next
            .review
            .evaluations
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let appeals = next.review.appeals.values().cloned().collect::<Vec<_>>();
        let resolutions = next
            .review
            .resolutions
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let active_assignment = next.review.assignments.values().any(|assignment| {
            assignment.submission_id == submission.submission_id
                && review_v4::review_assignment_active(assignment, Utc::now())
        });
        let expected_cycle = expected_rework_cycle(paper_id, next.reworks.values())?;
        validate_rework_state(
            &paper,
            &submission,
            &evaluation,
            &evaluations,
            &appeals,
            &resolutions,
            active_assignment,
            active_rework_memory(&next, paper_id).is_some(),
            expected_cycle,
            &request,
        )?;
        let now = Utc::now();
        let rework_expires_at = now
            .checked_add_signed(chrono::Duration::hours(PAPER_REWORK_LEASE_HOURS))
            .ok_or_else(|| ApiError::internal("Paper rework lease expiry overflow"))?;
        let record = make_rework_record(
            paper_id,
            &submission,
            &request,
            body_hash.clone(),
            now,
            rework_expires_at,
        )?;
        let withdrawn = next
            .submissions
            .get_mut(&submission.submission_id)
            .expect("submission exists");
        withdrawn.status = JointSubmissionStatus::Withdrawn;
        let paper_mut = next.papers.get_mut(&paper_id).expect("paper exists");
        paper_mut.phase = PaperPhase::Drafting;
        paper_mut.outcome = PaperChallengeOutcomeV1::InProgress;
        paper_mut.outcome_reason = None;
        paper_mut.terminal_at = None;
        paper_mut.release_candidate_revision_id = None;
        paper_mut.active_rework_id = Some(record.rework_id);
        paper_mut.active_rework_cycle = Some(record.rework_cycle);
        paper_mut.rework_expires_at = Some(record.rework_expires_at);
        paper_mut.version = paper_mut
            .version
            .checked_add(1)
            .ok_or_else(|| ApiError::internal("paper version overflow"))?;
        paper_mut.updated_at = now;
        let paper_version = paper_mut.version;
        next.reworks.insert(record.rework_id, record.clone());
        push_memory_event(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.rework.started.v1",
            paper_id,
            paper_version,
            json!({
                "rework_id": record.rework_id,
                "paper_project_id": paper_id,
                "rejected_evaluation_id": record.rejected_evaluation_id,
                "withdrawn_submission_id": record.rejected_submission_id,
                "rework_cycle": record.rework_cycle,
                "author_player_id": record.author_player_id,
            }),
        )?;
        memory_remember(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            body_hash,
            StatusCode::CREATED,
            &record,
        )?;
        *guard = next;
        return Ok((StatusCode::CREATED, Json(record)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &body_hash).await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    validate_signed_at(request.signed_at_unix)?;
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let paper_row = sqlx::query(
        "select version,record_json from hepta_paper_projects
         where paper_project_id=$1 for update",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let mut paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    let actual_version = u64::try_from(paper_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("paper version is invalid"))?;
    if actual_version != paper.version {
        return Err(ApiError::internal("paper relational/JSON version diverged"));
    }
    assert_team_actor_postgres(&mut tx, paper.team_id, &assertion).await?;
    let player_row =
        sqlx::query("select record_json from hepta_human_players where player_id=$1 for share")
            .bind(request.author_player_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| ApiError::internal("author player record is missing"))?;
    let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
    let key_row = sqlx::query(
        "select record_json from hepta_human_signing_keys
         where player_id=$1 and signing_key_id=$2 for share",
    )
    .bind(request.author_player_id)
    .bind(&request.signing_key_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::forbidden(
            "paper_rework_author_key_inactive",
            "Paper rework signing key is absent from durable key history",
        )
    })?;
    let key: HumanSigningKey = decode_record(key_row.get("record_json"), "human signing key")?;
    verify_author_signature(&player, &key, &request, &signing_frame)?;
    let submission_row = sqlx::query(
        "select record_json from hepta_joint_paper_submissions
         where submission_id=$1 and paper_project_id=$2 for update",
    )
    .bind(request.rejected_submission_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "joint_submission_not_found",
            "rejected submission does not exist",
        )
    })?;
    let mut submission: JointPaperSubmission =
        decode_record(submission_row.get("record_json"), "joint paper submission")?;
    let evaluation_row = sqlx::query(
        "select record_json from hepta_paper_evaluations
         where evaluation_id=$1 and paper_project_id=$2 for share",
    )
    .bind(request.rejected_evaluation_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "paper_evaluation_not_found",
            "rejected evaluation does not exist",
        )
    })?;
    let evaluation: PaperEvaluation =
        decode_record(evaluation_row.get("record_json"), "paper evaluation")?;
    let evaluations = review_v4::load_review_records(
        &mut tx,
        "hepta_paper_evaluations",
        paper_id,
        "paper evaluation",
    )
    .await?;
    let appeals =
        review_v4::load_review_records(&mut tx, "hepta_paper_appeals", paper_id, "paper Appeal")
            .await?;
    let resolutions = review_v4::load_review_records(
        &mut tx,
        "hepta_paper_appeal_resolutions",
        paper_id,
        "Appeal resolution",
    )
    .await?;
    let active_assignment: bool = sqlx::query_scalar(
        "select exists(select 1 from hepta_paper_review_assignments
          where submission_id=$1 and status in ('claimed','pinned')
            and (status='pinned' or expires_at>clock_timestamp()))",
    )
    .bind(submission.submission_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let active_rework: bool = sqlx::query_scalar(
        "select exists(
           select 1 from hepta_paper_reworks w
           left join hepta_paper_rework_resubmissions s on s.rework_id=w.rework_id
           where w.paper_project_id=$1 and s.rework_id is null)",
    )
    .bind(paper_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let previous_cycle_value: i64 = sqlx::query_scalar(
        "select coalesce(max(rework_cycle),1)
         from hepta_paper_reworks where paper_project_id=$1",
    )
    .bind(paper_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let previous_cycle = u64::try_from(previous_cycle_value)
        .map_err(|_| ApiError::internal("rework cycle is invalid"))?;
    let expected_cycle = previous_cycle
        .checked_add(1)
        .filter(|cycle| *cycle <= JSON_SAFE_U64_MAX)
        .ok_or_else(|| ApiError::internal("rework cycle overflow"))?;
    validate_rework_state(
        &paper,
        &submission,
        &evaluation,
        &evaluations,
        &appeals,
        &resolutions,
        active_assignment,
        active_rework,
        expected_cycle,
        &request,
    )?;
    let now = collaboration_v3::postgres_transaction_now(&mut tx).await?;
    let rework_expires_at = now
        .checked_add_signed(chrono::Duration::hours(PAPER_REWORK_LEASE_HOURS))
        .ok_or_else(|| ApiError::internal("Paper rework lease expiry overflow"))?;
    let record = make_rework_record(
        paper_id,
        &submission,
        &request,
        body_hash.clone(),
        now,
        rework_expires_at,
    )?;
    let record_json = serde_json::to_value(&record)
        .map_err(|error| ApiError::internal(format!("encode Paper rework: {error}")))?;
    let inserted = sqlx::query(
        "insert into hepta_paper_reworks (
            rework_id,paper_project_id,rejected_evaluation_id,rejected_submission_id,
            rejected_revision_id,rejected_release_candidate_hash,rejected_paper_bundle_hash,
            rejected_rework_content_commitment_sha256,
            rework_cycle,author_player_id,signing_key_id,signing_public_key,
            signing_public_key_hash,reason_hash,signed_at_unix,request_hash,signature,
            rework_expires_at,version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,1,$19::jsonb,$20)",
    )
    .bind(record.rework_id)
    .bind(record.paper_project_id)
    .bind(record.rejected_evaluation_id)
    .bind(record.rejected_submission_id)
    .bind(record.rejected_revision_id)
    .bind(&record.rejected_release_candidate_hash)
    .bind(&record.rejected_paper_bundle_hash)
    .bind(&record.rejected_rework_content_commitment_sha256)
    .bind(i64::try_from(record.rework_cycle).map_err(|_| ApiError::internal("rework cycle overflow"))?)
    .bind(record.author_player_id)
    .bind(&record.signing_key_id)
    .bind(&record.signing_public_key)
    .bind(&record.signing_public_key_hash)
    .bind(&record.reason_hash)
    .bind(record.signed_at_unix)
    .bind(&record.request_hash)
    .bind(&record.signature)
    .bind(record.rework_expires_at)
    .bind(record_json)
    .bind(record.created_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = inserted {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "paper_rework_identity_conflict",
                "rework identity, rejected lineage, or rework cycle already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    submission.status = JointSubmissionStatus::Withdrawn;
    let submission_json = serde_json::to_value(&submission)
        .map_err(|error| ApiError::internal(format!("encode withdrawn submission: {error}")))?;
    let withdrawn = sqlx::query(
        "update hepta_joint_paper_submissions set status='withdrawn',record_json=$1::jsonb
         where submission_id=$2 and paper_project_id=$3 and status='submission_ready'",
    )
    .bind(submission_json)
    .bind(submission.submission_id)
    .bind(paper_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if withdrawn.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "paper_rework_submission_drift",
            "rejected submission changed concurrently",
        ));
    }
    paper.phase = PaperPhase::Drafting;
    paper.outcome = PaperChallengeOutcomeV1::InProgress;
    paper.outcome_reason = None;
    paper.terminal_at = None;
    paper.release_candidate_revision_id = None;
    paper.active_rework_id = Some(record.rework_id);
    paper.active_rework_cycle = Some(record.rework_cycle);
    paper.rework_expires_at = Some(record.rework_expires_at);
    paper.version = paper
        .version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("paper version overflow"))?;
    paper.updated_at = now;
    let paper_json = serde_json::to_value(&paper)
        .map_err(|error| ApiError::internal(format!("encode reworked Paper: {error}")))?;
    let updated = sqlx::query(
        "update hepta_paper_projects
         set phase='drafting',outcome='in_progress',outcome_reason=null,terminal_at=null,
             active_rework_id=$1,active_rework_cycle=$2,rework_expires_at=$3,
             version=$4,record_json=$5::jsonb,updated_at=$6
         where paper_project_id=$7 and version=$8",
    )
    .bind(paper.active_rework_id)
    .bind(
        i64::try_from(record.rework_cycle)
            .map_err(|_| ApiError::internal("rework cycle overflow"))?,
    )
    .bind(paper.rework_expires_at)
    .bind(i64::try_from(paper.version).map_err(|_| ApiError::internal("paper version overflow"))?)
    .bind(paper_json)
    .bind(paper.updated_at)
    .bind(paper_id)
    .bind(
        i64::try_from(request.expected_paper_version)
            .map_err(|_| ApiError::internal("paper version overflow"))?,
    )
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "aggregate_version_conflict",
            "paper project changed concurrently",
        ));
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.rework.started.v1",
        paper_id,
        paper.version,
        json!({
            "rework_id":record.rework_id,
            "paper_project_id":paper_id,
            "rejected_evaluation_id":record.rejected_evaluation_id,
            "withdrawn_submission_id":record.rejected_submission_id,
            "rework_cycle":record.rework_cycle,
            "author_player_id":record.author_player_id,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &body_hash,
        Some(record.rework_id),
        StatusCode::CREATED,
        &record,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn get_paper_reworks(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Vec<PaperReworkStateV1>>, ApiError> {
    let path = format!("/v2/hepta/papers/{paper_id}/reworks");
    let assertion = require_member_read_assertion(&headers, &state, "get_paper_reworks_v1", &path)?;
    if state.pool.is_none() {
        let memory = state.paper_raid.read().await;
        let paper = memory.papers.get(&paper_id).ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "paper project does not exist")
        })?;
        let team = memory
            .teams
            .get(&paper.team_id)
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        assert_team_actor_memory(&memory, team, &assertion)?;
        let mut records = memory
            .reworks
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .map(|record| PaperReworkStateV1 {
                rework: record.clone(),
                resubmission: memory.rework_resubmissions.get(&record.rework_id).cloned(),
            })
            .collect::<Vec<_>>();
        records.sort_by_key(|record| (record.rework.rework_cycle, record.rework.rework_id));
        return Ok(Json(records));
    }
    let pool = state.pool.as_ref().expect("checked PostgreSQL pool");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    let team_id = sqlx::query_scalar::<_, Uuid>(
        "select team_id from hepta_paper_projects where paper_project_id=$1",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    assert_team_actor_postgres(&mut tx, team_id, &assertion).await?;
    let rows = sqlx::query(
        "select w.record_json as rework_record,s.record_json as resubmission_record
         from hepta_paper_reworks w
         left join hepta_paper_rework_resubmissions s on s.rework_id=w.rework_id
         where w.paper_project_id=$1 order by w.rework_cycle,w.rework_id",
    )
    .bind(paper_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let records = rows
        .into_iter()
        .map(|row| {
            Ok(PaperReworkStateV1 {
                rework: decode_record(row.get("rework_record"), "Paper rework")?,
                resubmission: row
                    .get::<Option<serde_json::Value>, _>("resubmission_record")
                    .map(|value| decode_record(value, "Paper rework resubmission"))
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(records))
}

pub(super) fn prepare_rework_resubmission_memory(
    memory: &PaperRaidMemory,
    submission: &JointPaperSubmission,
    now: DateTime<Utc>,
) -> Result<Option<PaperReworkResubmissionV1>, ApiError> {
    if submission.status != JointSubmissionStatus::SubmissionReady {
        return Ok(None);
    }
    let Some(rework) = active_rework_memory(memory, submission.paper_project_id) else {
        return Ok(None);
    };
    validate_replacement(rework, submission)?;
    Ok(Some(PaperReworkResubmissionV1 {
        schema: PAPER_REWORK_RESUBMISSION_SCHEMA_V1.to_string(),
        rework_id: rework.rework_id,
        paper_project_id: rework.paper_project_id,
        rejected_submission_id: rework.rejected_submission_id,
        replacement_submission_id: submission.submission_id,
        replacement_revision_id: submission.revision_id,
        replacement_release_candidate_hash: submission.release_candidate_hash.clone(),
        replacement_paper_bundle_hash: submission.paper_bundle_hash.clone(),
        replacement_rework_content_commitment_sha256: rework_content_commitment_sha256(submission)?,
        replacement_review_round: 1,
        version: 1,
        created_at: now,
    }))
}

fn validate_replacement(
    rework: &PaperReworkRecordV1,
    submission: &JointPaperSubmission,
) -> Result<(), ApiError> {
    let replacement_content_commitment = rework_content_commitment_sha256(submission)?;
    if submission.paper_project_id != rework.paper_project_id
        || submission.revision_id == rework.rejected_revision_id
        || submission.submission_id == rework.rejected_submission_id
        || submission.release_candidate_hash == rework.rejected_release_candidate_hash
        || submission.paper_bundle_hash == rework.rejected_paper_bundle_hash
        || replacement_content_commitment == rework.rejected_rework_content_commitment_sha256
    {
        return Err(ApiError::conflict(
            "paper_rework_replacement_not_new",
            "Paper rework resubmission must change identity-free scientific content as well as revision, submission, release candidate, and PaperBundle identities",
        ));
    }
    Ok(())
}

pub(super) async fn prepare_rework_resubmission_postgres(
    tx: &mut Transaction<'_, Postgres>,
    submission: &JointPaperSubmission,
    now: DateTime<Utc>,
) -> Result<Option<PaperReworkResubmissionV1>, ApiError> {
    if submission.status != JointSubmissionStatus::SubmissionReady {
        return Ok(None);
    }
    let row = sqlx::query(
        "select w.record_json from hepta_paper_reworks w
         left join hepta_paper_rework_resubmissions s on s.rework_id=w.rework_id
         where w.paper_project_id=$1 and s.rework_id is null
         order by w.rework_cycle desc limit 2 for share of w",
    )
    .bind(submission.paper_project_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if row.len() > 1 {
        return Err(ApiError::internal(
            "Paper has multiple active rework records",
        ));
    }
    let Some(row) = row.first() else {
        return Ok(None);
    };
    let rework: PaperReworkRecordV1 = decode_record(row.get("record_json"), "Paper rework")?;
    validate_replacement(&rework, submission)?;
    Ok(Some(PaperReworkResubmissionV1 {
        schema: PAPER_REWORK_RESUBMISSION_SCHEMA_V1.to_string(),
        rework_id: rework.rework_id,
        paper_project_id: rework.paper_project_id,
        rejected_submission_id: rework.rejected_submission_id,
        replacement_submission_id: submission.submission_id,
        replacement_revision_id: submission.revision_id,
        replacement_release_candidate_hash: submission.release_candidate_hash.clone(),
        replacement_paper_bundle_hash: submission.paper_bundle_hash.clone(),
        replacement_rework_content_commitment_sha256: rework_content_commitment_sha256(submission)?,
        replacement_review_round: 1,
        version: 1,
        created_at: now,
    }))
}

pub(super) async fn insert_rework_resubmission_postgres(
    tx: &mut Transaction<'_, Postgres>,
    record: &PaperReworkResubmissionV1,
) -> Result<(), ApiError> {
    let record_json = serde_json::to_value(record).map_err(|error| {
        ApiError::internal(format!("encode Paper rework resubmission: {error}"))
    })?;
    sqlx::query(
        "insert into hepta_paper_rework_resubmissions (
            rework_id,paper_project_id,rejected_submission_id,replacement_submission_id,
            replacement_revision_id,replacement_release_candidate_hash,
            replacement_paper_bundle_hash,replacement_rework_content_commitment_sha256,
            replacement_review_round,version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,1,$10::jsonb,$11)",
    )
    .bind(record.rework_id)
    .bind(record.paper_project_id)
    .bind(record.rejected_submission_id)
    .bind(record.replacement_submission_id)
    .bind(record.replacement_revision_id)
    .bind(&record.replacement_release_candidate_hash)
    .bind(&record.replacement_paper_bundle_hash)
    .bind(&record.replacement_rework_content_commitment_sha256)
    .bind(
        i64::try_from(record.replacement_review_round)
            .map_err(|_| ApiError::internal("review round overflow"))?,
    )
    .bind(record_json)
    .bind(record.created_at)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    Ok(())
}

fn normalize_rework_catalog_sql(value: &str) -> String {
    value
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn rework_migration_function_body<'a>(
    migration: &'a str,
    function_name: &str,
) -> Result<&'a str, String> {
    let marker = format!("create or replace function {function_name}");
    let function_start = migration
        .find(&marker)
        .ok_or_else(|| format!("0050 canonical {function_name} function is missing"))?;
    let body_start = migration[function_start..]
        .find("as $function$\n")
        .map(|index| function_start + index + "as $function$\n".len())
        .ok_or_else(|| format!("0050 canonical {function_name} body start is missing"))?;
    let body_end = migration[body_start..]
        .find("\n$function$;")
        .map(|index| body_start + index)
        .ok_or_else(|| format!("0050 canonical {function_name} body end is missing"))?;
    Ok(&migration[body_start..body_end])
}

pub(crate) async fn verify_rework_migration_catalog(pool: &sqlx::PgPool) -> Result<(), String> {
    let tables: i64 = sqlx::query_scalar(
        "select count(*)::bigint from pg_class
         where oid in (
           to_regclass('public.hepta_paper_reworks'),
           to_regclass('public.hepta_paper_rework_resubmissions')
         ) and relkind='r'",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("inspect Paper rework tables: {error}"))?;
    if tables != 2 {
        return Err("Paper rework normalized tables are incomplete".to_string());
    }

    let columns: i64 = sqlx::query_scalar(
        "select count(*)::bigint from pg_attribute
         where attrelid='public.hepta_paper_projects'::regclass
           and attname in ('active_rework_id','active_rework_cycle','rework_expires_at')
           and attnum>0 and not attisdropped",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("inspect Paper rework lease columns: {error}"))?;
    if columns != 3 {
        return Err("Paper rework lease columns are incomplete".to_string());
    }

    let triggers = sqlx::query(
        "select t.tgname,
                rn.nspname || '.' || relation.relname as relation_name,
                fn.nspname || '.' || function_catalog.proname as function_name,
                t.tgtype::integer as trigger_type,
                t.tgenabled::text as enabled
         from pg_trigger t
         join pg_class relation on relation.oid=t.tgrelid
         join pg_namespace rn on rn.oid=relation.relnamespace
         join pg_proc function_catalog on function_catalog.oid=t.tgfoid
         join pg_namespace fn on fn.oid=function_catalog.pronamespace
         where tgname in (
           'hepta_validate_paper_rework_insert_trigger',
           'hepta_validate_paper_rework_resubmission_insert_trigger',
           'hepta_paper_reworks_immutable_trigger',
           'hepta_paper_rework_resubmissions_immutable_trigger',
           'hepta_joint_submission_rework_withdrawal_trigger',
           'hepta_paper_reworks_finality_v2_source_guard',
           'hepta_paper_rework_resubmissions_finality_v2_source_guard',
           'hepta_paper_reworks_truncate_guard',
           'hepta_paper_rework_resubmissions_truncate_guard',
           'hepta_paper_rework_finality_v2_lineage_guard'
         ) and not tgisinternal
         order by t.tgname,relation_name,function_name",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("inspect Paper rework ALWAYS triggers: {error}"))?;
    let expected_triggers = [
        (
            "hepta_joint_submission_rework_withdrawal_trigger",
            "public.hepta_joint_paper_submissions",
            "public.hepta_guard_joint_submission_rework_withdrawal",
            19_i32,
        ),
        (
            "hepta_paper_rework_finality_v2_lineage_guard",
            "public.hepta_paper_chain_finality_preparations_v2",
            "public.hepta_validate_paper_rework_finality_lineage",
            7_i32,
        ),
        (
            "hepta_paper_rework_resubmissions_finality_v2_source_guard",
            "public.hepta_paper_rework_resubmissions",
            "public.hepta_reject_paper_finality_v2_source_mutation",
            31_i32,
        ),
        (
            "hepta_paper_rework_resubmissions_immutable_trigger",
            "public.hepta_paper_rework_resubmissions",
            "public.hepta_reject_paper_rework_mutation",
            27_i32,
        ),
        (
            "hepta_paper_rework_resubmissions_truncate_guard",
            "public.hepta_paper_rework_resubmissions",
            "public.hepta_reject_paper_finality_v2_truncate",
            34_i32,
        ),
        (
            "hepta_paper_reworks_finality_v2_source_guard",
            "public.hepta_paper_reworks",
            "public.hepta_reject_paper_finality_v2_source_mutation",
            31_i32,
        ),
        (
            "hepta_paper_reworks_immutable_trigger",
            "public.hepta_paper_reworks",
            "public.hepta_reject_paper_rework_mutation",
            27_i32,
        ),
        (
            "hepta_paper_reworks_truncate_guard",
            "public.hepta_paper_reworks",
            "public.hepta_reject_paper_finality_v2_truncate",
            34_i32,
        ),
        (
            "hepta_validate_paper_rework_insert_trigger",
            "public.hepta_paper_reworks",
            "public.hepta_validate_paper_rework_insert",
            7_i32,
        ),
        (
            "hepta_validate_paper_rework_resubmission_insert_trigger",
            "public.hepta_paper_rework_resubmissions",
            "public.hepta_validate_paper_rework_resubmission_insert",
            7_i32,
        ),
    ];
    if triggers.len() != expected_triggers.len()
        || triggers
            .iter()
            .zip(expected_triggers)
            .any(|(actual, expected)| {
                actual.get::<String, _>("tgname") != expected.0
                    || actual.get::<String, _>("relation_name") != expected.1
                    || actual.get::<String, _>("function_name") != expected.2
                    || actual.get::<i32, _>("trigger_type") != expected.3
                    || actual.get::<String, _>("enabled") != "A"
            })
    {
        return Err(
            "Paper rework validation/immutability/finality guards are not globally unique, exact, and ALWAYS"
                .to_string(),
        );
    }

    let constraints = sqlx::query(
        "select constraint_catalog.conname,
                namespace.nspname || '.' || relation.relname as relation_name,
                constraint_catalog.contype::text as constraint_type,
                constraint_catalog.convalidated,
                constraint_catalog.condeferrable,
                constraint_catalog.connoinherit,
                coalesce(array_agg(attribute.attname order by attribute.attname)
                  filter (where attribute.attname is not null),array[]::name[])::text[]
                  as column_names,
                pg_get_constraintdef(constraint_catalog.oid,true) as definition
         from pg_constraint constraint_catalog
         join pg_class relation on relation.oid=constraint_catalog.conrelid
         join pg_namespace namespace on namespace.oid=relation.relnamespace
         left join lateral unnest(constraint_catalog.conkey) as key(attnum) on true
         left join pg_attribute attribute
           on attribute.attrelid=constraint_catalog.conrelid
          and attribute.attnum=key.attnum
         where conname in (
           'hepta_paper_reworks_pkey',
           'hepta_paper_rework_resubmissions_pkey',
           'hepta_paper_projects_active_rework_shape_check',
           'hepta_paper_reworks_content_commitment_nonzero_check',
           'hepta_rework_resubmission_content_commitment_nonzero_check'
         )
         group by constraint_catalog.oid,namespace.nspname,relation.relname
         order by constraint_catalog.conname,relation_name",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("inspect Paper rework constraints: {error}"))?;
    // Freeze PostgreSQL's exact catalog semantics: PRIMARY KEY constraints
    // have connoinherit=true, while CHECK constraints have it false.
    let expected_constraints: [(&str, &str, &str, &[&str], bool); 5] = [
        (
            "hepta_paper_projects_active_rework_shape_check",
            "public.hepta_paper_projects",
            "c",
            &[
                "active_rework_cycle",
                "active_rework_id",
                "rework_expires_at",
            ],
            false,
        ),
        (
            "hepta_paper_rework_resubmissions_pkey",
            "public.hepta_paper_rework_resubmissions",
            "p",
            &["rework_id"],
            true,
        ),
        (
            "hepta_paper_reworks_content_commitment_nonzero_check",
            "public.hepta_paper_reworks",
            "c",
            &["rejected_rework_content_commitment_sha256"],
            false,
        ),
        (
            "hepta_paper_reworks_pkey",
            "public.hepta_paper_reworks",
            "p",
            &["rework_id"],
            true,
        ),
        (
            "hepta_rework_resubmission_content_commitment_nonzero_check",
            "public.hepta_paper_rework_resubmissions",
            "c",
            &["replacement_rework_content_commitment_sha256"],
            false,
        ),
    ];
    if constraints.len() != expected_constraints.len()
        || constraints
            .iter()
            .zip(expected_constraints)
            .any(|(actual, expected)| {
                let columns = actual.get::<Vec<String>, _>("column_names");
                actual.get::<String, _>("conname") != expected.0
                    || actual.get::<String, _>("relation_name") != expected.1
                    || actual.get::<String, _>("constraint_type") != expected.2
                    || !actual.get::<bool, _>("convalidated")
                    || actual.get::<bool, _>("condeferrable")
                    || actual.get::<bool, _>("connoinherit") != expected.4
                    || columns.iter().map(String::as_str).collect::<Vec<_>>() != expected.3
                    || actual
                        .get::<String, _>("definition")
                        .to_ascii_lowercase()
                        .contains("or true")
            })
    {
        return Err(
            "Paper rework identity/lease constraints are not globally unique and exact".to_string(),
        );
    }

    let migration = include_str!("../../../migrations/0050_add_hepta_paper_rework.sql");
    let expected_functions = [
        (
            "hepta_paper_rework_content_projection",
            "public.hepta_paper_rework_content_projection(jsonb)",
            "jsonb",
            "submission_record jsonb",
            "sql",
            "i",
            false,
            true,
            "search_path=pg_catalog",
        ),
        (
            "hepta_paper_rework_content_commitment_sha256",
            "public.hepta_paper_rework_content_commitment_sha256(jsonb)",
            "text",
            "submission_record jsonb",
            "sql",
            "i",
            false,
            true,
            "search_path=pg_catalog",
        ),
        (
            "hepta_validate_paper_rework_insert",
            "public.hepta_validate_paper_rework_insert()",
            "trigger",
            "",
            "plpgsql",
            "v",
            true,
            false,
            "search_path=pg_catalog",
        ),
        (
            "hepta_validate_paper_rework_resubmission_insert",
            "public.hepta_validate_paper_rework_resubmission_insert()",
            "trigger",
            "",
            "plpgsql",
            "v",
            true,
            false,
            "search_path=pg_catalog",
        ),
        (
            "hepta_guard_joint_submission_rework_withdrawal",
            "public.hepta_guard_joint_submission_rework_withdrawal()",
            "trigger",
            "",
            "plpgsql",
            "v",
            false,
            false,
            "",
        ),
        (
            "hepta_reject_paper_rework_mutation",
            "public.hepta_reject_paper_rework_mutation()",
            "trigger",
            "",
            "plpgsql",
            "v",
            false,
            false,
            "",
        ),
        (
            "hepta_validate_paper_rework_finality_lineage",
            "public.hepta_validate_paper_rework_finality_lineage()",
            "trigger",
            "",
            "plpgsql",
            "v",
            true,
            false,
            "search_path=pg_catalog",
        ),
        (
            "hepta_guard_paper_challenge_ruleset_v1",
            "public.hepta_guard_paper_challenge_ruleset_v1()",
            "trigger",
            "",
            "plpgsql",
            "v",
            false,
            false,
            "",
        ),
    ];
    for (
        function_name,
        identity,
        result_type,
        arguments,
        language,
        volatility,
        security_definer,
        strict,
        config,
    ) in expected_functions
    {
        let globally_named: i64 =
            sqlx::query_scalar("select count(*)::bigint from pg_proc where proname=$1")
                .bind(function_name)
                .fetch_one(pool)
                .await
                .map_err(|error| format!("inspect global {function_name} identity: {error}"))?;
        if globally_named != 1 {
            return Err(format!(
                "Paper rework function {function_name} is not globally unique"
            ));
        }
        let function = sqlx::query(
            "select p.prosrc,p.prosecdef,p.provolatile::text as volatility,
                    p.proleakproof,p.proisstrict,p.proretset,p.prokind::text as function_kind,
                    p.pronargs,pg_get_function_result(p.oid) as result_type,
                    pg_get_function_arguments(p.oid) as arguments,
                    l.lanname,coalesce(array_to_string(p.proconfig,','),'') as config
             from pg_proc p
             join pg_language l on l.oid=p.prolang
             where p.oid=pg_catalog.to_regprocedure($1)",
        )
        .bind(identity)
        .fetch_one(pool)
        .await
        .map_err(|error| format!("inspect exact Paper rework function {identity}: {error}"))?;
        let expected_argument_count = if arguments.is_empty() { 0 } else { 1 };
        if function.get::<bool, _>("prosecdef") != security_definer
            || function.get::<String, _>("volatility") != volatility
            || function.get::<bool, _>("proleakproof")
            || function.get::<bool, _>("proisstrict") != strict
            || function.get::<bool, _>("proretset")
            || function.get::<String, _>("function_kind") != "f"
            || function.get::<i16, _>("pronargs") != expected_argument_count
            || function.get::<String, _>("result_type") != result_type
            || function.get::<String, _>("arguments") != arguments
            || function.get::<String, _>("lanname") != language
            || function.get::<String, _>("config") != config
        {
            return Err(format!(
                "Paper rework function {function_name} metadata is non-canonical"
            ));
        }
        let expected_body = rework_migration_function_body(migration, function_name)?;
        if normalize_rework_catalog_sql(&function.get::<String, _>("prosrc"))
            != normalize_rework_catalog_sql(expected_body)
        {
            return Err(format!(
                "Paper rework function {function_name} body is not the exact 0050 authority"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod migration_static_tests {
    use super::*;

    fn submission_fixture() -> JointPaperSubmission {
        let paper_id = Uuid::new_v4();
        let revision_id = Uuid::new_v4();
        JointPaperSubmission {
            submission_id: Uuid::new_v4(),
            paper_project_id: paper_id,
            revision_id,
            release_candidate_hash: format!("sha256:{}", "1".repeat(64)),
            paper_bundle_hash: format!("sha256:{}", "2".repeat(64)),
            status: JointSubmissionStatus::SubmissionReady,
            paper_bundle: PaperBundleV2 {
                schema: PAPER_BUNDLE_V2.to_string(),
                release_candidate: PaperReleaseCandidateV2 {
                    schema: PAPER_RELEASE_CANDIDATE_V2.to_string(),
                    paper_project_id: paper_id,
                    revision_id,
                    team_id: Uuid::new_v4(),
                    challenge_id: Uuid::new_v4(),
                    ruleset_hash: format!("sha256:{}", "3".repeat(64)),
                    challenge_snapshot_hash: format!("sha256:{}", "4".repeat(64)),
                    roster_version: 1,
                    title: "Evidence Audit".into(),
                    abstract_text: "Audit frozen claims".into(),
                    target_format: "paper".into(),
                    source_manifest_hash: format!("sha256:{}", "5".repeat(64)),
                    artifact_manifest_hash: format!("sha256:{}", "6".repeat(64)),
                    bibliography_hash: format!("sha256:{}", "7".repeat(64)),
                    claim_evidence_graph_hash: format!("sha256:{}", "8".repeat(64)),
                    section_materialization_root: None,
                    collaboration_compact_hash: format!("sha256:{}", "9".repeat(64)),
                    research_protocol_snapshot_hash: format!("sha256:{}", "a".repeat(64)),
                    ethics_disclosure_hash: format!("sha256:{}", "b".repeat(64)),
                    coi_disclosure_hash: format!("sha256:{}", "c".repeat(64)),
                    contribution_ledger_hash: format!("sha256:{}", "d".repeat(64)),
                    ai_disclosure_hash: format!("sha256:{}", "e".repeat(64)),
                    license: "CC-BY-4.0".into(),
                    authors: Vec::new(),
                },
                release_candidate_hash: format!("sha256:{}", "1".repeat(64)),
                author_consents: Vec::new(),
                paper_bundle_hash: format!("sha256:{}", "2".repeat(64)),
            },
            created_at: Utc::now(),
        }
    }

    #[test]
    fn migration_0050_never_reintroduces_a_subquery_partial_index() {
        let migration = include_str!("../../../migrations/0050_add_hepta_paper_rework.sql");
        assert!(!migration.contains("hepta_paper_one_active_rework_idx"));
        assert!(!migration.contains("where not exists (\n        select 1 from hepta_paper_rework"));
        assert!(migration.contains("interval '24 hours'"));
        assert!(migration.contains("rework_window_elapsed"));
        assert!(migration
            .contains("hepta_paper_rework_content_commitment_sha256(submission.record_json)"));
        assert!(migration
            .contains("hepta_paper_rework_content_commitment_sha256(replacement.record_json)"));
        assert!(migration
            .trim_start()
            .starts_with("-- Immutable Author rework lineage"));
        assert!(migration.contains("\nbegin;\n"));
        assert!(migration.trim_end().ends_with("commit;"));
        assert!(migration.contains(
            "revoke all on function public.hepta_guard_paper_challenge_ruleset_v1() from public"
        ));
    }

    #[test]
    fn replacement_must_change_identity_free_scientific_content() {
        let old = submission_fixture();
        let old_commitment = rework_content_commitment_sha256(&old).unwrap();
        let rework = PaperReworkRecordV1 {
            schema: PAPER_REWORK_RECORD_SCHEMA_V1.into(),
            rework_id: Uuid::new_v4(),
            paper_project_id: old.paper_project_id,
            rejected_evaluation_id: Uuid::new_v4(),
            rejected_submission_id: old.submission_id,
            rejected_revision_id: old.revision_id,
            rejected_release_candidate_hash: old.release_candidate_hash.clone(),
            rejected_paper_bundle_hash: old.paper_bundle_hash.clone(),
            rejected_rework_content_commitment_sha256: old_commitment,
            rework_cycle: 2,
            author_player_id: Uuid::new_v4(),
            signing_key_id: "key".into(),
            signing_public_key: "key".into(),
            signing_public_key_hash: format!("sha256:{}", "f".repeat(64)),
            reason_hash: format!("sha256:{}", "0".repeat(64)),
            signed_at_unix: 1,
            signature: "signature".into(),
            request_hash: format!("sha256:{}", "1".repeat(64)),
            rework_expires_at: Utc::now() + chrono::Duration::hours(24),
            version: 1,
            created_at: Utc::now(),
        };
        let mut replacement = old.clone();
        replacement.submission_id = Uuid::new_v4();
        replacement.revision_id = Uuid::new_v4();
        replacement.paper_bundle.release_candidate.revision_id = replacement.revision_id;
        replacement.release_candidate_hash = format!("sha256:{}", "a".repeat(64));
        replacement.paper_bundle_hash = format!("sha256:{}", "b".repeat(64));
        replacement.paper_bundle.release_candidate_hash =
            replacement.release_candidate_hash.clone();
        replacement.paper_bundle.paper_bundle_hash = replacement.paper_bundle_hash.clone();
        assert!(validate_replacement(&rework, &replacement).is_err());

        replacement
            .paper_bundle
            .release_candidate
            .claim_evidence_graph_hash = format!("sha256:{}", "c".repeat(64));
        assert!(validate_replacement(&rework, &replacement).is_ok());
    }

    #[test]
    fn integrity_hold_does_not_close_active_rework() {
        let submission = submission_fixture();
        let mut memory = PaperRaidMemory::default();
        let now = Utc::now();
        memory.reworks.insert(
            Uuid::new_v4(),
            PaperReworkRecordV1 {
                schema: PAPER_REWORK_RECORD_SCHEMA_V1.into(),
                rework_id: Uuid::new_v4(),
                paper_project_id: submission.paper_project_id,
                rejected_evaluation_id: Uuid::new_v4(),
                rejected_submission_id: Uuid::new_v4(),
                rejected_revision_id: Uuid::new_v4(),
                rejected_release_candidate_hash: format!("sha256:{}", "d".repeat(64)),
                rejected_paper_bundle_hash: format!("sha256:{}", "e".repeat(64)),
                rejected_rework_content_commitment_sha256: format!("sha256:{}", "f".repeat(64)),
                rework_cycle: 2,
                author_player_id: Uuid::new_v4(),
                signing_key_id: "key".into(),
                signing_public_key: "key".into(),
                signing_public_key_hash: format!("sha256:{}", "1".repeat(64)),
                reason_hash: format!("sha256:{}", "2".repeat(64)),
                signed_at_unix: now.timestamp(),
                signature: "signature".into(),
                request_hash: format!("sha256:{}", "3".repeat(64)),
                rework_expires_at: now + chrono::Duration::hours(24),
                version: 1,
                created_at: now,
            },
        );
        let mut held = submission;
        held.status = JointSubmissionStatus::IntegrityHold;
        assert!(prepare_rework_resubmission_memory(&memory, &held, now)
            .unwrap()
            .is_none());
        assert!(memory.rework_resubmissions.is_empty());
    }
}
