use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    decode_digest,
    paper_raid_contracts::{
        canonical_json_sha256, frozen_challenge_material_authority_hash,
        FrozenChallengeMaterialAuthorityV1, FROZEN_CHALLENGE_MATERIAL_AUTHORITY_V1,
    },
    push_event, require_service_token, ApiError, AppState, ChallengeStatus, ChallengeTemplateV1,
    LeagueState, ResearchChallenge, OPERATOR_TOKEN_HEADER,
};

pub const ACTIVATION_REQUEST_V1: &str = "hepta.challenge_pack.activation_request.v1";
pub const ACTIVATION_EVIDENCE_V1: &str = "hepta.challenge_pack.activation_evidence.v1";
pub const ACTIVATION_RECORD_V1: &str = "hepta.challenge_pack.activation_record.v1";
pub const CAS_ACTIVATION_RECEIPT_V1: &str = "hepta.challenge_pack.cas_activation_receipt.v1";
pub const CAS_ACTIVATION_CATALOG_PATCH_V2: &str =
    "hepta.challenge_pack.activation_catalog_patch.v2";
pub const CURRENT_CANDIDATE_BINDING_V2: &str = "trnm.paper-raid.current-candidate-binding.v2";
pub const STRICT_REVIEW_EVIDENCE_V1: &str = "trnm.paper-raid.strict-review-evidence.v1";
pub const STRICT_REVIEW_TERMINAL_BUNDLE_BINDING_V1: &str =
    "trnm.paper-raid.strict-review-terminal-bundle-binding.v1";

const EVIDENCE_AUDIT_TEMPLATE: &str = "evidence-audit";
const EVIDENCE_AUDIT_PACK_ID: &str = "paper-raid-evidence-audit-seeded-v1";
const EVIDENCE_AUDIT_TITLE: &str = "Paper Raid: Evidence and Citation Audit";
const EVIDENCE_AUDIT_DESCRIPTION_MARKER: &str = "paper-raid-alpha-template-evidence-audit-v1";
const EVIDENCE_AUDIT_RULESET_VERSION: &str = "paper-raid-evidence-audit-v1";
const EVIDENCE_AUDIT_RULESET_HASH: &str =
    "sha256:54a740273a82d56938a20db1669b236e8b3724defffe32fb91768632f7994453";
const EVIDENCE_AUDIT_DATASET_MANIFEST_HASH: &str =
    "sha256:1363088a76b2dd5c77b04b2620a4806100f1125c482a3e6413691d1a8c372a75";
const EVIDENCE_AUDIT_EVALUATOR_MANIFEST_HASH: &str =
    "sha256:7d7f0096261132ceda30e66586e49129aaa99bb979772ceec9b2940eda21261e";
const EVIDENCE_AUDIT_PACK_MANIFEST_HASH: &str =
    "sha256:69a695a6a75dd71e2c53c7a832298ab2ecbcd4086fa4ef653914dd2e5770b37c";
const EVIDENCE_AUDIT_SOURCE_CATALOG_HASH: &str =
    "sha256:fc649c1f55ef484bc2f8baf279dec1c668eaa610691ac4234a74df1d90aaa6bb";

const CANDIDATE_PENDING_EVIDENCE: &str = "immutable_candidate_pending_evidence";
const CANDIDATE_VERIFIED: &str = "verified_candidate";
const TRACKED_IMAGE_LOCK_UNBOUND: &str = "unbound";
const RELEASE_IMAGE_LOCK_LOCKED: &str = "locked";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengePackCandidateBindingV2 {
    pub schema: String,
    pub state: String,
    pub integration_base_revision: String,
    pub integration_source_tree: String,
    pub hepta_base_revision: String,
    pub hepta_source_tree: String,
    pub component_pins_authoritative: bool,
    pub working_tree_clean: bool,
    pub tracked_image_lock_status: String,
    pub source_fileset_sha256: String,
    pub release_image_lock_status: String,
    pub release_id: String,
    pub release_image_lock_sha256: String,
    pub release_provenance_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengePackActivationEvidenceV1 {
    pub schema: String,
    pub source_catalog_sha256: String,
    pub pack_manifest_sha256: String,
    pub cas_activation_receipt_schema: String,
    pub cas_activation_receipt_sha256: String,
    pub cas_activation_catalog_patch_schema: String,
    pub cas_activation_catalog_patch_sha256: String,
    pub cas_all_packs_verified: bool,
    pub cas_scoped_readback: bool,
    pub cas_pack_count: u16,
    pub cas_object_count: u16,
    pub strict_review_evidence_schema: String,
    pub strict_review_evidence_sha256: String,
    pub strict_review_chain_proof_manifest_sha256: String,
    pub strict_review_chain_proof_fileset_sha256: String,
    pub strict_review_terminal_bundle_schema: String,
    pub strict_review_terminal_bundle_sha256: String,
    pub cross_paper_denial_receipt_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActivateChallengePackRequestV1 {
    pub schema: String,
    pub template: String,
    pub pack_id: String,
    pub expected_status: ChallengeStatus,
    pub requested_status: ChallengeStatus,
    pub ruleset_version: String,
    pub ruleset_hash: String,
    pub dataset_manifest_hash: String,
    pub evaluator_manifest_hash: String,
    pub candidate: ChallengePackCandidateBindingV2,
    pub evidence: ChallengePackActivationEvidenceV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengePackActivationRecordV1 {
    pub schema: String,
    pub activation_id: Uuid,
    pub challenge_id: Uuid,
    pub request_sha256: String,
    pub request: ActivateChallengePackRequestV1,
    pub previous_status: ChallengeStatus,
    pub activated_status: ChallengeStatus,
    pub activated_at: DateTime<Utc>,
}

struct ActivationMutation {
    status: StatusCode,
    record: ChallengePackActivationRecordV1,
    applied: bool,
}

pub(crate) fn router() -> Router<AppState> {
    Router::new().route(
        "/v1/hepta/operator/challenges/:challenge_id/pack-activation",
        get(get_activation).post(activate_challenge_pack),
    )
}

async fn activate_challenge_pack(
    State(state): State<AppState>,
    Path(challenge_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<ActivateChallengePackRequestV1>,
) -> Result<(StatusCode, Json<ChallengePackActivationRecordV1>), ApiError> {
    require_service_token(
        &headers,
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    validate_activation_request(&request)?;
    let request_sha256 = canonical_json_sha256(&json!({
        "challenge_id": challenge_id,
        "request": request,
    }))
    .map_err(|message| {
        ApiError::bad_request(
            "challenge_pack_activation_request_not_canonical",
            format!("activation request is not canonical: {message}"),
        )
    })?;
    let result = state
        .activate_challenge_pack_transaction(challenge_id, request, request_sha256)
        .await?;
    Ok((result.status, Json(result.record)))
}

async fn get_activation(
    State(state): State<AppState>,
    Path(challenge_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ChallengePackActivationRecordV1>, ApiError> {
    require_service_token(
        &headers,
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    state
        .inspect(|league| {
            league
                .challenge_pack_activations
                .get(&challenge_id)
                .cloned()
                .map(Json)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "challenge_pack_activation_not_found",
                        format!("challenge {challenge_id} has no Challenge Pack activation"),
                    )
                })
        })
        .await
}

impl AppState {
    async fn activate_challenge_pack_transaction(
        &self,
        challenge_id: Uuid,
        request: ActivateChallengePackRequestV1,
        request_sha256: String,
    ) -> Result<ActivationMutation, ApiError> {
        let Some(pool) = &self.pool else {
            let mut state = self.inner.write().await;
            return apply_activation(
                &mut state,
                challenge_id,
                request,
                request_sha256,
                Utc::now(),
            );
        };

        let mut tx = pool.begin().await.map_err(ApiError::database)?;
        sqlx::query("select pg_advisory_xact_lock(hashtext('hepta-research-league-state-v1'))")
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
        let row = sqlx::query(
            "select revision, state_json from hepta_league_state
             where state_key='primary' for update",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        let revision: i64 = row.get("revision");
        let mut league: LeagueState = serde_json::from_value(row.get("state_json"))
            .map_err(|error| ApiError::internal(format!("decode Hepta state: {error}")))?;
        // PostgreSQL persists timestamptz at microsecond precision.  Use the
        // database clock itself so the immutable JSON projection and the
        // relational value are the same bytes on every insert, rather than
        // intermittently depending on a process-clock nanosecond remainder.
        let activated_at: DateTime<Utc> = sqlx::query_scalar("select clock_timestamp()")
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
        let event_offset = league.events.len();
        let result = apply_activation(
            &mut league,
            challenge_id,
            request,
            request_sha256,
            activated_at,
        )?;
        if !result.applied {
            tx.rollback().await.map_err(ApiError::database)?;
            return Ok(result);
        }

        let state_json = serde_json::to_value(&league)
            .map_err(|error| ApiError::internal(format!("encode Hepta state: {error}")))?;
        let update = sqlx::query(
            "update hepta_league_state
             set revision=$1,state_json=$2::jsonb,updated_at=now()
             where state_key='primary' and revision=$3
               and state_json #>> array['challenges',$4::text,'status']='draft'",
        )
        .bind(revision + 1)
        .bind(state_json)
        .bind(revision)
        .bind(challenge_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        if update.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "challenge_pack_activation_status_drift",
                "Challenge did not remain in the exact durable draft state",
            ));
        }

        persist_activation(&mut tx, &result.record).await?;
        for event in &league.events[event_offset..] {
            sqlx::query(
                "insert into hepta_outbox (
                    event_id,event_type,aggregate_id,aggregate_version,
                    correlation_id,causation_id,idempotency_key,schema_version,
                    producer,payload_hash,payload,occurred_at
                 ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11::jsonb,$12)
                 on conflict (event_id) do nothing",
            )
            .bind(event.event_id)
            .bind(&event.event_type)
            .bind(&event.aggregate_id)
            .bind(event.aggregate_version as i64)
            .bind(event.correlation_id)
            .bind(event.causation_id)
            .bind(&event.idempotency_key)
            .bind(&event.schema_version)
            .bind(&event.producer)
            .bind(&event.payload_hash)
            .bind(&event.payload)
            .bind(event.occurred_at)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
        }
        tx.commit().await.map_err(ApiError::database)?;
        Ok(result)
    }
}

fn apply_activation(
    league: &mut LeagueState,
    challenge_id: Uuid,
    request: ActivateChallengePackRequestV1,
    request_sha256: String,
    activated_at: DateTime<Utc>,
) -> Result<ActivationMutation, ApiError> {
    let challenge = league.challenges.get(&challenge_id).ok_or_else(|| {
        ApiError::not_found(
            "challenge_not_found",
            format!("challenge {challenge_id} does not exist"),
        )
    })?;
    verify_challenge_binding(challenge, &request)?;

    if let Some(existing) = league.challenge_pack_activations.get(&challenge_id) {
        if existing.request_sha256 != request_sha256 || existing.request != request {
            return Err(ApiError::conflict(
                "challenge_pack_activation_replay_conflict",
                "Challenge already has a different immutable activation request",
            ));
        }
        if challenge.status != ChallengeStatus::Open
            || existing.previous_status != ChallengeStatus::Draft
            || existing.activated_status != ChallengeStatus::Open
        {
            return Err(ApiError::conflict(
                "challenge_pack_activation_status_drift",
                "Challenge activation record and durable status diverged",
            ));
        }
        return Ok(ActivationMutation {
            status: StatusCode::OK,
            record: existing.clone(),
            applied: false,
        });
    }

    if challenge.status != ChallengeStatus::Draft {
        return Err(ApiError::conflict(
            "challenge_pack_activation_status_drift",
            "Challenge must be in the exact draft state before activation",
        ));
    }
    if league
        .challenge_pack_activations
        .values()
        .any(|record| record.request_sha256 == request_sha256)
    {
        return Err(ApiError::conflict(
            "challenge_pack_activation_request_reused",
            "Activation request digest is already bound to another Challenge",
        ));
    }

    let record = ChallengePackActivationRecordV1 {
        schema: ACTIVATION_RECORD_V1.to_string(),
        activation_id: Uuid::new_v4(),
        challenge_id,
        request_sha256,
        request,
        previous_status: ChallengeStatus::Draft,
        activated_status: ChallengeStatus::Open,
        activated_at,
    };
    league
        .challenges
        .get_mut(&challenge_id)
        .expect("Challenge exists after immutable lookup")
        .status = ChallengeStatus::Open;
    league
        .challenge_pack_activations
        .insert(challenge_id, record.clone());
    push_event(
        league,
        "hepta.challenge_pack.activated.v1",
        challenge_id.to_string(),
        json!({
            "schema": ACTIVATION_RECORD_V1,
            "activation_id": record.activation_id,
            "challenge_id": challenge_id,
            "request_sha256": record.request_sha256,
            "template": record.request.template,
            "pack_id": record.request.pack_id,
            "ruleset_hash": record.request.ruleset_hash,
            "dataset_manifest_hash": record.request.dataset_manifest_hash,
            "evaluator_manifest_hash": record.request.evaluator_manifest_hash,
            "release_provenance_sha256": record.request.candidate.release_provenance_sha256,
            "cas_activation_receipt_sha256": record.request.evidence.cas_activation_receipt_sha256,
            "strict_review_evidence_sha256": record.request.evidence.strict_review_evidence_sha256,
            "strict_review_chain_proof_manifest_sha256": record.request.evidence.strict_review_chain_proof_manifest_sha256,
            "strict_review_chain_proof_fileset_sha256": record.request.evidence.strict_review_chain_proof_fileset_sha256,
            "strict_review_terminal_bundle_schema": record.request.evidence.strict_review_terminal_bundle_schema,
            "strict_review_terminal_bundle_sha256": record.request.evidence.strict_review_terminal_bundle_sha256,
            "cross_paper_denial_receipt_sha256": record.request.evidence.cross_paper_denial_receipt_sha256,
            "activated_at": record.activated_at,
        }),
    );
    Ok(ActivationMutation {
        status: StatusCode::CREATED,
        record,
        applied: true,
    })
}

fn validate_activation_request(request: &ActivateChallengePackRequestV1) -> Result<(), ApiError> {
    require_exact("schema", &request.schema, ACTIVATION_REQUEST_V1)?;
    require_exact("template", &request.template, EVIDENCE_AUDIT_TEMPLATE)?;
    require_exact("pack_id", &request.pack_id, EVIDENCE_AUDIT_PACK_ID)?;
    if request.expected_status != ChallengeStatus::Draft
        || request.requested_status != ChallengeStatus::Open
    {
        return Err(invalid_contract(
            "expected_status/requested_status must be the exact draft-to-open transition",
        ));
    }
    require_exact(
        "ruleset_version",
        &request.ruleset_version,
        EVIDENCE_AUDIT_RULESET_VERSION,
    )?;
    require_exact(
        "ruleset_hash",
        &request.ruleset_hash,
        EVIDENCE_AUDIT_RULESET_HASH,
    )?;
    require_exact(
        "dataset_manifest_hash",
        &request.dataset_manifest_hash,
        EVIDENCE_AUDIT_DATASET_MANIFEST_HASH,
    )?;
    require_exact(
        "evaluator_manifest_hash",
        &request.evaluator_manifest_hash,
        EVIDENCE_AUDIT_EVALUATOR_MANIFEST_HASH,
    )?;

    let candidate = &request.candidate;
    require_exact(
        "candidate.schema",
        &candidate.schema,
        CURRENT_CANDIDATE_BINDING_V2,
    )?;
    if !matches!(
        candidate.state.as_str(),
        CANDIDATE_PENDING_EVIDENCE | CANDIDATE_VERIFIED
    ) {
        return Err(invalid_contract(
            "candidate.state is not an admissible immutable candidate state",
        ));
    }
    for (field, value) in [
        (
            "candidate.integration_base_revision",
            candidate.integration_base_revision.as_str(),
        ),
        (
            "candidate.integration_source_tree",
            candidate.integration_source_tree.as_str(),
        ),
        (
            "candidate.hepta_base_revision",
            candidate.hepta_base_revision.as_str(),
        ),
        (
            "candidate.hepta_source_tree",
            candidate.hepta_source_tree.as_str(),
        ),
    ] {
        validate_git_oid(field, value)?;
    }
    if !candidate.component_pins_authoritative || !candidate.working_tree_clean {
        return Err(invalid_contract(
            "candidate must bind authoritative component pins and a clean committed worktree",
        ));
    }
    require_exact(
        "candidate.tracked_image_lock_status",
        &candidate.tracked_image_lock_status,
        TRACKED_IMAGE_LOCK_UNBOUND,
    )?;
    require_exact(
        "candidate.release_image_lock_status",
        &candidate.release_image_lock_status,
        RELEASE_IMAGE_LOCK_LOCKED,
    )?;
    validate_release_id(&candidate.release_id)?;
    for (field, value) in [
        (
            "candidate.source_fileset_sha256",
            candidate.source_fileset_sha256.as_str(),
        ),
        (
            "candidate.release_image_lock_sha256",
            candidate.release_image_lock_sha256.as_str(),
        ),
        (
            "candidate.release_provenance_sha256",
            candidate.release_provenance_sha256.as_str(),
        ),
    ] {
        validate_canonical_digest(field, value)?;
    }

    let evidence = &request.evidence;
    require_exact("evidence.schema", &evidence.schema, ACTIVATION_EVIDENCE_V1)?;
    require_exact(
        "evidence.source_catalog_sha256",
        &evidence.source_catalog_sha256,
        EVIDENCE_AUDIT_SOURCE_CATALOG_HASH,
    )?;
    require_exact(
        "evidence.pack_manifest_sha256",
        &evidence.pack_manifest_sha256,
        EVIDENCE_AUDIT_PACK_MANIFEST_HASH,
    )?;
    require_exact(
        "evidence.cas_activation_receipt_schema",
        &evidence.cas_activation_receipt_schema,
        CAS_ACTIVATION_RECEIPT_V1,
    )?;
    require_exact(
        "evidence.cas_activation_catalog_patch_schema",
        &evidence.cas_activation_catalog_patch_schema,
        CAS_ACTIVATION_CATALOG_PATCH_V2,
    )?;
    require_exact(
        "evidence.strict_review_evidence_schema",
        &evidence.strict_review_evidence_schema,
        STRICT_REVIEW_EVIDENCE_V1,
    )?;
    require_exact(
        "evidence.strict_review_terminal_bundle_schema",
        &evidence.strict_review_terminal_bundle_schema,
        STRICT_REVIEW_TERMINAL_BUNDLE_BINDING_V1,
    )?;
    for (field, value) in [
        (
            "evidence.cas_activation_receipt_sha256",
            evidence.cas_activation_receipt_sha256.as_str(),
        ),
        (
            "evidence.cas_activation_catalog_patch_sha256",
            evidence.cas_activation_catalog_patch_sha256.as_str(),
        ),
        (
            "evidence.strict_review_evidence_sha256",
            evidence.strict_review_evidence_sha256.as_str(),
        ),
        (
            "evidence.strict_review_chain_proof_manifest_sha256",
            evidence.strict_review_chain_proof_manifest_sha256.as_str(),
        ),
        (
            "evidence.strict_review_chain_proof_fileset_sha256",
            evidence.strict_review_chain_proof_fileset_sha256.as_str(),
        ),
        (
            "evidence.strict_review_terminal_bundle_sha256",
            evidence.strict_review_terminal_bundle_sha256.as_str(),
        ),
        (
            "evidence.cross_paper_denial_receipt_sha256",
            evidence.cross_paper_denial_receipt_sha256.as_str(),
        ),
    ] {
        validate_canonical_digest(field, value)?;
    }
    if !evidence.cas_all_packs_verified
        || !evidence.cas_scoped_readback
        || evidence.cas_pack_count != 3
        || evidence.cas_object_count != 26
    {
        return Err(invalid_contract(
            "CAS evidence must prove exact scoped readback of all 3 packs and 26 deduplicated objects",
        ));
    }
    Ok(())
}

/// Project the append-only activation into the Paper's immutable challenge snapshot.
///
/// This deliberately does not read a current catalog: the activation request already binds the
/// exact source pack manifest and the exact dataset/evaluator manifest digests.  Any state drift
/// fails Paper creation instead of silently falling back to player-selected material.
pub(crate) fn freeze_challenge_material_authority(
    challenge: &ResearchChallenge,
    activation: &ChallengePackActivationRecordV1,
    challenge_snapshot_hash: &str,
) -> Result<FrozenChallengeMaterialAuthorityV1, ApiError> {
    decode_digest(challenge_snapshot_hash).map_err(|message| {
        ApiError::internal(format!(
            "invalid challenge snapshot hash before material freeze: {message}"
        ))
    })?;
    if activation.schema != ACTIVATION_RECORD_V1
        || activation.challenge_id != challenge.challenge_id
        || activation.previous_status != ChallengeStatus::Draft
        || activation.activated_status != ChallengeStatus::Open
        || challenge.status != ChallengeStatus::Open
    {
        return Err(ApiError::internal(
            "Challenge Pack activation record cannot authorize this open Challenge snapshot",
        ));
    }
    validate_activation_request(&activation.request).map_err(|error| {
        ApiError::internal(format!(
            "stored Challenge Pack activation request is invalid: {}",
            error.message
        ))
    })?;
    verify_challenge_binding(challenge, &activation.request).map_err(|error| {
        ApiError::internal(format!(
            "stored Challenge Pack activation binding is invalid: {}",
            error.message
        ))
    })?;
    let request_sha256 = canonical_json_sha256(&json!({
        "challenge_id": challenge.challenge_id,
        "request": activation.request,
    }))
    .map_err(|message| {
        ApiError::internal(format!("rehash Challenge Pack activation: {message}"))
    })?;
    if request_sha256 != activation.request_sha256 {
        return Err(ApiError::internal(
            "stored Challenge Pack activation request hash mismatch",
        ));
    }
    let mut authority = FrozenChallengeMaterialAuthorityV1 {
        schema: FROZEN_CHALLENGE_MATERIAL_AUTHORITY_V1.to_string(),
        authority_hash: String::new(),
        activation_id: activation.activation_id,
        activation_request_sha256: activation.request_sha256.clone(),
        challenge_id: challenge.challenge_id,
        challenge_snapshot_hash: challenge_snapshot_hash.to_string(),
        template: activation.request.template.clone(),
        pack_id: activation.request.pack_id.clone(),
        pack_manifest_hash: activation.request.evidence.pack_manifest_sha256.clone(),
        ruleset_version: activation.request.ruleset_version.clone(),
        ruleset_hash: activation.request.ruleset_hash.clone(),
        dataset_manifest_hash: activation.request.dataset_manifest_hash.clone(),
        evaluator_manifest_hash: activation.request.evaluator_manifest_hash.clone(),
    };
    authority.authority_hash =
        frozen_challenge_material_authority_hash(&authority).map_err(|message| {
            ApiError::internal(format!("freeze Challenge Pack authority: {message}"))
        })?;
    Ok(authority)
}

fn verify_challenge_binding(
    challenge: &ResearchChallenge,
    request: &ActivateChallengePackRequestV1,
) -> Result<(), ApiError> {
    if challenge.title != EVIDENCE_AUDIT_TITLE
        || !challenge
            .description
            .contains(EVIDENCE_AUDIT_DESCRIPTION_MARKER)
        || !challenge.description.contains("template=evidence-audit")
        || challenge.ruleset_version != request.ruleset_version
        || challenge.ruleset_hash != request.ruleset_hash
        || challenge.dataset_manifest_hash != request.dataset_manifest_hash
        || challenge.evaluator_manifest_hash != request.evaluator_manifest_hash
    {
        return Err(ApiError::conflict(
            "challenge_pack_activation_binding_mismatch",
            "Challenge snapshot does not exactly match the frozen Evidence Audit catalog entry",
        ));
    }
    let ruleset = challenge.ruleset.as_ref().ok_or_else(|| {
        ApiError::conflict(
            "challenge_pack_activation_binding_mismatch",
            "Evidence Audit activation requires the authoritative typed ruleset",
        )
    })?;
    if ruleset.template != ChallengeTemplateV1::EvidenceAudit
        || ruleset.canonical_hash().map_err(|message| {
            ApiError::conflict(
                "challenge_pack_activation_binding_mismatch",
                format!("typed Challenge ruleset is invalid: {message}"),
            )
        })? != EVIDENCE_AUDIT_RULESET_HASH
    {
        return Err(ApiError::conflict(
            "challenge_pack_activation_binding_mismatch",
            "Challenge typed ruleset does not exactly match Evidence Audit",
        ));
    }
    Ok(())
}

fn require_exact(field: &str, value: &str, expected: &str) -> Result<(), ApiError> {
    if value != expected {
        return Err(invalid_contract(format!(
            "{field} must equal the frozen value {expected}"
        )));
    }
    Ok(())
}

fn validate_canonical_digest(field: &str, value: &str) -> Result<(), ApiError> {
    decode_digest(value).map_err(|message| invalid_contract(format!("{field}: {message}")))?;
    if value == format!("sha256:{}", "0".repeat(64)) {
        return Err(invalid_contract(format!(
            "{field} must not be the all-zero SHA-256 sentinel"
        )));
    }
    Ok(())
}

fn validate_git_oid(field: &str, value: &str) -> Result<(), ApiError> {
    if value.len() != 40
        || value.bytes().all(|byte| byte == b'0')
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_contract(format!(
            "{field} must contain exactly 40 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_release_id(value: &str) -> Result<(), ApiError> {
    let mut bytes = value.bytes();
    let first = bytes.next().ok_or_else(|| {
        invalid_contract("candidate.release_id must be a non-empty canonical identifier")
    })?;
    if value.len() > 128
        || !(first.is_ascii_lowercase() || first.is_ascii_digit())
        || !bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err(invalid_contract(
            "candidate.release_id must match [a-z0-9][a-z0-9._-]{0,127}",
        ));
    }
    Ok(())
}

fn invalid_contract(message: impl Into<String>) -> ApiError {
    ApiError::bad_request("challenge_pack_activation_contract_invalid", message)
}

async fn persist_activation(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    record: &ChallengePackActivationRecordV1,
) -> Result<(), ApiError> {
    let request = &record.request;
    let candidate = &request.candidate;
    let evidence = &request.evidence;
    let record_json = serde_json::to_value(record)
        .map_err(|error| ApiError::internal(format!("encode activation record: {error}")))?;
    sqlx::query(
        "insert into hepta_challenge_pack_activations_v1 (
            activation_id,challenge_id,request_sha256,pack_id,template,
            ruleset_version,ruleset_hash,dataset_manifest_hash,evaluator_manifest_hash,
            candidate_state,integration_base_revision,integration_source_tree,
            hepta_base_revision,hepta_source_tree,source_fileset_sha256,release_id,
            release_image_lock_sha256,release_provenance_sha256,source_catalog_sha256,
            pack_manifest_sha256,cas_activation_receipt_sha256,
            cas_activation_catalog_patch_sha256,strict_review_evidence_sha256,
            strict_review_chain_proof_manifest_sha256,
            strict_review_chain_proof_fileset_sha256,
            strict_review_terminal_bundle_schema,strict_review_terminal_bundle_sha256,
            cross_paper_denial_receipt_sha256,activated_at,record_json
         ) values (
            $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,
            $20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30::jsonb
         )",
    )
    .bind(record.activation_id)
    .bind(record.challenge_id)
    .bind(&record.request_sha256)
    .bind(&request.pack_id)
    .bind(&request.template)
    .bind(&request.ruleset_version)
    .bind(&request.ruleset_hash)
    .bind(&request.dataset_manifest_hash)
    .bind(&request.evaluator_manifest_hash)
    .bind(&candidate.state)
    .bind(&candidate.integration_base_revision)
    .bind(&candidate.integration_source_tree)
    .bind(&candidate.hepta_base_revision)
    .bind(&candidate.hepta_source_tree)
    .bind(&candidate.source_fileset_sha256)
    .bind(&candidate.release_id)
    .bind(&candidate.release_image_lock_sha256)
    .bind(&candidate.release_provenance_sha256)
    .bind(&evidence.source_catalog_sha256)
    .bind(&evidence.pack_manifest_sha256)
    .bind(&evidence.cas_activation_receipt_sha256)
    .bind(&evidence.cas_activation_catalog_patch_sha256)
    .bind(&evidence.strict_review_evidence_sha256)
    .bind(&evidence.strict_review_chain_proof_manifest_sha256)
    .bind(&evidence.strict_review_chain_proof_fileset_sha256)
    .bind(&evidence.strict_review_terminal_bundle_schema)
    .bind(&evidence.strict_review_terminal_bundle_sha256)
    .bind(&evidence.cross_paper_denial_receipt_sha256)
    .bind(record.activated_at)
    .bind(record_json)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    Ok(())
}

fn normalize_activation_catalog_sql(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    let mut in_single_quoted_literal = false;
    let mut in_double_quoted_identifier = false;
    let mut pending_space = false;

    while let Some(character) = characters.next() {
        if in_single_quoted_literal {
            normalized.push(character);
            if character == '\'' {
                if characters.peek() == Some(&'\'') {
                    normalized.push(characters.next().expect("peeked escaped quote"));
                } else {
                    in_single_quoted_literal = false;
                }
            }
            continue;
        }

        if in_double_quoted_identifier {
            normalized.push(character);
            if character == '"' {
                if characters.peek() == Some(&'"') {
                    normalized.push(characters.next().expect("peeked escaped identifier quote"));
                } else {
                    in_double_quoted_identifier = false;
                }
            }
            continue;
        }

        if character.is_whitespace() {
            pending_space = !normalized.is_empty();
            continue;
        }

        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }

        match character {
            '\'' => {
                normalized.push(character);
                in_single_quoted_literal = true;
            }
            '"' => {
                normalized.push(character);
                in_double_quoted_identifier = true;
            }
            _ => normalized.extend(character.to_lowercase()),
        }
    }

    normalized
}

fn compact_activation_constraint_definition(value: &str) -> String {
    let normalized = normalize_activation_catalog_sql(value);
    let characters = normalized.chars().collect::<Vec<_>>();
    let mut compact = String::with_capacity(normalized.len());
    let mut index = 0;
    let mut in_single_quoted_literal = false;
    let mut in_double_quoted_identifier = false;

    while index < characters.len() {
        let character = characters[index];

        if in_single_quoted_literal {
            compact.push(character);
            if character == '\'' {
                if characters.get(index + 1) == Some(&'\'') {
                    compact.push('\'');
                    index += 1;
                } else {
                    in_single_quoted_literal = false;
                }
            }
            index += 1;
            continue;
        }

        if in_double_quoted_identifier {
            if character == '"' {
                if characters.get(index + 1) == Some(&'"') {
                    compact.push('"');
                    index += 1;
                } else {
                    in_double_quoted_identifier = false;
                }
            } else {
                compact.push(character);
            }
            index += 1;
            continue;
        }

        match character {
            '\'' => {
                compact.push(character);
                in_single_quoted_literal = true;
                index += 1;
            }
            '"' => {
                in_double_quoted_identifier = true;
                index += 1;
            }
            '(' | ')' if !in_double_quoted_identifier => index += 1,
            character if character.is_whitespace() => index += 1,
            ':' if characters.get(index..index + 6)
                == Some([':', ':', 't', 'e', 'x', 't'].as_slice()) =>
            {
                index += 6;
            }
            _ => {
                compact.push(character);
                index += 1;
            }
        }
    }

    compact
}

fn activation_evidence_digest_constraint_definition_is_exact(value: &str) -> bool {
    const DIGEST_COLUMNS: [&str; 10] = [
        "source_fileset_sha256",
        "release_image_lock_sha256",
        "release_provenance_sha256",
        "cas_activation_receipt_sha256",
        "cas_activation_catalog_patch_sha256",
        "strict_review_evidence_sha256",
        "strict_review_chain_proof_manifest_sha256",
        "strict_review_chain_proof_fileset_sha256",
        "strict_review_terminal_bundle_sha256",
        "cross_paper_denial_receipt_sha256",
    ];
    let zero = format!("sha256:{}", "0".repeat(64));
    let expected = format!(
        "check{}and{}",
        DIGEST_COLUMNS
            .iter()
            .map(|column| format!("{column}~'^sha256:[0-9a-f]{{64}}$'"))
            .collect::<Vec<_>>()
            .join("and"),
        DIGEST_COLUMNS
            .iter()
            .map(|column| format!("{column}<>'{zero}'"))
            .collect::<Vec<_>>()
            .join("and")
    );
    compact_activation_constraint_definition(value) == expected
}

fn activation_migration_function_body<'a>(
    migration: &'a str,
    function_name: &str,
) -> Result<&'a str, String> {
    let qualified_marker = format!("create or replace function public.{function_name}");
    let historical_marker = format!("create or replace function {function_name}");
    let function_start = migration
        .find(&qualified_marker)
        .or_else(|| migration.find(&historical_marker))
        .ok_or_else(|| format!("canonical {function_name} function is missing"))?;
    let body_start = migration[function_start..]
        .find("as $$\n")
        .map(|index| function_start + index + "as $$\n".len())
        .ok_or_else(|| format!("canonical {function_name} body start is missing"))?;
    let body_end = migration[body_start..]
        .find("\n$$;")
        .map(|index| body_start + index)
        .ok_or_else(|| format!("canonical {function_name} body end is missing"))?;
    Ok(&migration[body_start..body_end])
}

pub(crate) async fn verify_migration_catalog(pool: &PgPool) -> Result<(), String> {
    let table_exists: bool = sqlx::query_scalar(
        "select to_regclass('public.hepta_challenge_pack_activations_v1') is not null",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("inspect Challenge Pack activation table: {error}"))?;
    if !table_exists {
        return Err("Challenge Pack activation table is missing".to_string());
    }
    let chain_proof_columns = sqlx::query(
        "select attribute.attname,
                pg_catalog.format_type(attribute.atttypid,attribute.atttypmod) as data_type,
                attribute.attnotnull,
                attribute.attgenerated::text as generated,
                attribute.attidentity::text as identity,
                attribute_default.oid is not null as has_default
         from pg_attribute attribute
         left join pg_attrdef attribute_default
           on attribute_default.adrelid=attribute.attrelid
          and attribute_default.adnum=attribute.attnum
         where attribute.attrelid='public.hepta_challenge_pack_activations_v1'::regclass
           and attribute.attname in (
             'strict_review_chain_proof_manifest_sha256',
             'strict_review_chain_proof_fileset_sha256',
             'strict_review_terminal_bundle_schema',
             'strict_review_terminal_bundle_sha256'
           )
           and attribute.attnum > 0 and not attribute.attisdropped
         order by attribute.attname",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("inspect activation Chain proof columns: {error}"))?;
    let expected_chain_proof_columns = [
        "strict_review_chain_proof_fileset_sha256",
        "strict_review_chain_proof_manifest_sha256",
        "strict_review_terminal_bundle_schema",
        "strict_review_terminal_bundle_sha256",
    ];
    if chain_proof_columns.len() != expected_chain_proof_columns.len()
        || chain_proof_columns
            .iter()
            .zip(expected_chain_proof_columns)
            .any(|(actual, expected_name)| {
                actual.get::<String, _>("attname") != expected_name
                    || actual.get::<String, _>("data_type") != "text"
                    || !actual.get::<bool, _>("attnotnull")
                    || !actual.get::<String, _>("generated").is_empty()
                    || !actual.get::<String, _>("identity").is_empty()
                    || actual.get::<bool, _>("has_default")
            })
    {
        return Err("Challenge Pack activation Chain proof columns are not exact".to_string());
    }
    let triggers = sqlx::query(
        "select t.tgname,
                relation_namespace.nspname || '.' || relation.relname as relation_name,
                function_namespace.nspname || '.' || function_catalog.proname as function_name,
                t.tgtype::integer as trigger_type,
                t.tgenabled::text as enabled
         from pg_trigger t
         join pg_class relation on relation.oid=t.tgrelid
         join pg_namespace relation_namespace on relation_namespace.oid=relation.relnamespace
         join pg_proc function_catalog on function_catalog.oid=t.tgfoid
         join pg_namespace function_namespace
           on function_namespace.oid=function_catalog.pronamespace
         where t.tgname in (
           'hepta_challenge_pack_activation_validate_guard',
           'hepta_challenge_pack_activation_immutable_guard',
           'hepta_challenge_pack_activation_truncate_guard'
         ) and not t.tgisinternal
         order by t.tgname,relation_name,function_name",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("inspect Challenge Pack activation triggers: {error}"))?;
    let expected_triggers = [
        (
            "hepta_challenge_pack_activation_immutable_guard",
            "public.hepta_challenge_pack_activations_v1",
            "public.hepta_reject_challenge_pack_activation_mutation_v1",
            27_i32,
        ),
        (
            "hepta_challenge_pack_activation_truncate_guard",
            "public.hepta_challenge_pack_activations_v1",
            "public.hepta_reject_challenge_pack_activation_mutation_v1",
            34_i32,
        ),
        (
            "hepta_challenge_pack_activation_validate_guard",
            "public.hepta_challenge_pack_activations_v1",
            "public.hepta_validate_challenge_pack_activation_v1",
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
            "Challenge Pack activation guards are not globally unique, exact, and ALWAYS"
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
         where constraint_catalog.conname in (
           'hepta_challenge_pack_activations_v1_pkey',
           'hepta_challenge_pack_activations_challenge_key',
           'hepta_challenge_pack_activations_request_key',
           'hepta_challenge_pack_activations_non_nil_id_check',
           'hepta_challenge_pack_activations_request_hash_check',
           'hepta_challenge_pack_activations_identity_check',
           'hepta_challenge_pack_activations_candidate_state_check',
           'hepta_challenge_pack_activations_git_pins_check',
           'hepta_challenge_pack_activations_release_id_check',
           'hepta_challenge_pack_activations_evidence_digests_check'
         )
         group by constraint_catalog.oid,namespace.nspname,relation.relname
         order by constraint_catalog.conname,relation_name",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("inspect Challenge Pack activation constraints: {error}"))?;
    // PostgreSQL sets connoinherit for PRIMARY KEY/UNIQUE constraints even
    // though inheritance semantics only apply to CHECK constraints.  Freeze
    // that exact catalog representation instead of incorrectly requiring the
    // CHECK-constraint value for every constraint type.
    let expected_constraints: [(&str, &str, &[&str], bool); 10] = [
        (
            "hepta_challenge_pack_activations_candidate_state_check",
            "c",
            &["candidate_state"],
            false,
        ),
        (
            "hepta_challenge_pack_activations_challenge_key",
            "u",
            &["challenge_id"],
            true,
        ),
        (
            "hepta_challenge_pack_activations_evidence_digests_check",
            "c",
            &[
                "cas_activation_catalog_patch_sha256",
                "cas_activation_receipt_sha256",
                "cross_paper_denial_receipt_sha256",
                "release_image_lock_sha256",
                "release_provenance_sha256",
                "source_fileset_sha256",
                "strict_review_chain_proof_fileset_sha256",
                "strict_review_chain_proof_manifest_sha256",
                "strict_review_evidence_sha256",
                "strict_review_terminal_bundle_sha256",
            ],
            false,
        ),
        (
            "hepta_challenge_pack_activations_git_pins_check",
            "c",
            &[
                "hepta_base_revision",
                "hepta_source_tree",
                "integration_base_revision",
                "integration_source_tree",
            ],
            false,
        ),
        (
            "hepta_challenge_pack_activations_identity_check",
            "c",
            &[
                "dataset_manifest_hash",
                "evaluator_manifest_hash",
                "pack_id",
                "pack_manifest_sha256",
                "ruleset_hash",
                "ruleset_version",
                "source_catalog_sha256",
                "template",
            ],
            false,
        ),
        (
            "hepta_challenge_pack_activations_non_nil_id_check",
            "c",
            &["activation_id"],
            false,
        ),
        (
            "hepta_challenge_pack_activations_release_id_check",
            "c",
            &["release_id"],
            false,
        ),
        (
            "hepta_challenge_pack_activations_request_hash_check",
            "c",
            &["request_sha256"],
            false,
        ),
        (
            "hepta_challenge_pack_activations_request_key",
            "u",
            &["request_sha256"],
            true,
        ),
        (
            "hepta_challenge_pack_activations_v1_pkey",
            "p",
            &["activation_id"],
            true,
        ),
    ];
    if constraints.len() != expected_constraints.len()
        || constraints
            .iter()
            .zip(expected_constraints)
            .any(|(actual, expected)| {
                let columns = actual.get::<Vec<String>, _>("column_names");
                let name = actual.get::<String, _>("conname");
                let definition = actual.get::<String, _>("definition");
                actual.get::<String, _>("conname") != expected.0
                    || actual.get::<String, _>("relation_name")
                        != "public.hepta_challenge_pack_activations_v1"
                    || actual.get::<String, _>("constraint_type") != expected.1
                    || !actual.get::<bool, _>("convalidated")
                    || actual.get::<bool, _>("condeferrable")
                    || actual.get::<bool, _>("connoinherit") != expected.3
                    || columns.iter().map(String::as_str).collect::<Vec<_>>() != expected.2
                    || (name == "hepta_challenge_pack_activations_evidence_digests_check"
                        && !activation_evidence_digest_constraint_definition_is_exact(&definition))
                    || definition.to_ascii_lowercase().contains("or true")
            })
    {
        return Err(
            "Challenge Pack activation constraints are not globally unique and exact".to_string(),
        );
    }
    let migration_0049 =
        include_str!("../../../migrations/0049_add_hepta_challenge_pack_activation.sql");
    let migration_0054 =
        include_str!("../../../migrations/0054_bind_challenge_pack_activation_chain_proof.sql");
    for (function_name, identity, config, authority) in [
        (
            "hepta_validate_challenge_pack_activation_v1",
            "public.hepta_validate_challenge_pack_activation_v1()",
            "search_path=pg_catalog, public",
            migration_0054,
        ),
        (
            "hepta_reject_challenge_pack_activation_mutation_v1",
            "public.hepta_reject_challenge_pack_activation_mutation_v1()",
            "search_path=pg_catalog",
            migration_0049,
        ),
    ] {
        let globally_named: i64 =
            sqlx::query_scalar("select count(*)::bigint from pg_proc where proname=$1")
                .bind(function_name)
                .fetch_one(pool)
                .await
                .map_err(|error| format!("inspect global {function_name} identity: {error}"))?;
        if globally_named != 1 {
            return Err(format!(
                "Challenge Pack activation function {function_name} is not globally unique"
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
        .map_err(|error| format!("inspect exact activation function {identity}: {error}"))?;
        if function.get::<bool, _>("prosecdef")
            || function.get::<String, _>("volatility") != "v"
            || function.get::<bool, _>("proleakproof")
            || function.get::<bool, _>("proisstrict")
            || function.get::<bool, _>("proretset")
            || function.get::<String, _>("function_kind") != "f"
            || function.get::<i16, _>("pronargs") != 0
            || function.get::<String, _>("result_type") != "trigger"
            || !function.get::<String, _>("arguments").is_empty()
            || function.get::<String, _>("lanname") != "plpgsql"
            || function.get::<String, _>("config") != config
        {
            return Err(format!(
                "Challenge Pack activation function {function_name} metadata is non-canonical"
            ));
        }
        let expected_body = activation_migration_function_body(authority, function_name)?;
        if normalize_activation_catalog_sql(&function.get::<String, _>("prosrc"))
            != normalize_activation_catalog_sql(expected_body)
        {
            return Err(format!(
                "Challenge Pack activation function {function_name} body is not the exact migration authority"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) struct PostgresActivationFixture {
    pub challenge_id: Uuid,
    pub request: ActivateChallengePackRequestV1,
    pub record: ChallengePackActivationRecordV1,
}

#[cfg(test)]
pub(crate) fn fixture_activation_request(challenge_id: Uuid) -> ActivateChallengePackRequestV1 {
    ActivateChallengePackRequestV1 {
        schema: ACTIVATION_REQUEST_V1.to_string(),
        template: EVIDENCE_AUDIT_TEMPLATE.to_string(),
        pack_id: EVIDENCE_AUDIT_PACK_ID.to_string(),
        expected_status: ChallengeStatus::Draft,
        requested_status: ChallengeStatus::Open,
        ruleset_version: EVIDENCE_AUDIT_RULESET_VERSION.to_string(),
        ruleset_hash: EVIDENCE_AUDIT_RULESET_HASH.to_string(),
        dataset_manifest_hash: EVIDENCE_AUDIT_DATASET_MANIFEST_HASH.to_string(),
        evaluator_manifest_hash: EVIDENCE_AUDIT_EVALUATOR_MANIFEST_HASH.to_string(),
        candidate: ChallengePackCandidateBindingV2 {
            schema: CURRENT_CANDIDATE_BINDING_V2.to_string(),
            state: CANDIDATE_PENDING_EVIDENCE.to_string(),
            integration_base_revision: "1".repeat(40),
            integration_source_tree: "2".repeat(40),
            hepta_base_revision: "3".repeat(40),
            hepta_source_tree: "4".repeat(40),
            component_pins_authoritative: true,
            working_tree_clean: true,
            tracked_image_lock_status: TRACKED_IMAGE_LOCK_UNBOUND.to_string(),
            source_fileset_sha256: format!("sha256:{}", "5".repeat(64)),
            release_image_lock_status: RELEASE_IMAGE_LOCK_LOCKED.to_string(),
            release_id: format!("hepta-postgres-activation-{}", challenge_id.simple()),
            release_image_lock_sha256: format!("sha256:{}", "6".repeat(64)),
            release_provenance_sha256: format!("sha256:{}", "7".repeat(64)),
        },
        evidence: ChallengePackActivationEvidenceV1 {
            schema: ACTIVATION_EVIDENCE_V1.to_string(),
            source_catalog_sha256: EVIDENCE_AUDIT_SOURCE_CATALOG_HASH.to_string(),
            pack_manifest_sha256: EVIDENCE_AUDIT_PACK_MANIFEST_HASH.to_string(),
            cas_activation_receipt_schema: CAS_ACTIVATION_RECEIPT_V1.to_string(),
            cas_activation_receipt_sha256: format!("sha256:{}", "8".repeat(64)),
            cas_activation_catalog_patch_schema: CAS_ACTIVATION_CATALOG_PATCH_V2.to_string(),
            cas_activation_catalog_patch_sha256: format!("sha256:{}", "9".repeat(64)),
            cas_all_packs_verified: true,
            cas_scoped_readback: true,
            cas_pack_count: 3,
            cas_object_count: 26,
            strict_review_evidence_schema: STRICT_REVIEW_EVIDENCE_V1.to_string(),
            strict_review_evidence_sha256: format!("sha256:{}", "a".repeat(64)),
            strict_review_chain_proof_manifest_sha256: format!("sha256:{}", "c".repeat(64)),
            strict_review_chain_proof_fileset_sha256: format!("sha256:{}", "d".repeat(64)),
            strict_review_terminal_bundle_schema: STRICT_REVIEW_TERMINAL_BUNDLE_BINDING_V1
                .to_string(),
            strict_review_terminal_bundle_sha256: format!("sha256:{}", "e".repeat(64)),
            cross_paper_denial_receipt_sha256: format!("sha256:{}", "b".repeat(64)),
        },
    }
}

#[cfg(test)]
pub(crate) async fn seed_postgres_activation_fixture(
    state: &AppState,
) -> Result<PostgresActivationFixture, String> {
    let ruleset = serde_json::from_str(
        r#"{
          "schema":"hepta.challenge.ruleset.v1",
          "template":"evidence-audit",
          "duration_seconds":2700,
          "grace_seconds":900,
          "phase_gates":[
            {"transition":"preregistering_to_researching","requirements":[{"kind":"work_items","minimum":1},{"kind":"artifact_manifests","minimum":1}]},
            {"transition":"researching_to_experimenting","requirements":[{"kind":"evidence_cards","minimum":2},{"kind":"citations","minimum":2},{"kind":"claims","minimum":2}]},
            {"transition":"experimenting_to_drafting","requirements":[{"kind":"artifact_manifests","minimum":1}]},
            {"transition":"drafting_to_integrity_review","requirements":[{"kind":"all_work_items_terminal","minimum":1},{"kind":"section_revisions","minimum":1},{"kind":"paper_revisions","minimum":1}]},
            {"transition":"integrity_review_to_reproducing","requirements":[{"kind":"approving_section_reviews","minimum":1},{"kind":"section_merges","minimum":1}]},
            {"transition":"reproducing_to_author_approval","requirements":[{"kind":"paper_revision_covers_section_merges","minimum":1}]}
          ],
          "victory_requirements":[
            {"kind":"accepted_work_items","minimum":1},{"kind":"evidence_cards","minimum":2},
            {"kind":"citations","minimum":2},{"kind":"claims","minimum":2},
            {"kind":"release_candidate","minimum":1},{"kind":"all_author_consents","minimum":1},
            {"kind":"paper_revision_covers_section_merges","minimum":1}
          ],
          "gameplay":{
            "difficulty":"introductory",
            "objective":"Audit core claims, citations, licenses, and artifact provenance.",
            "risk":"Citation mismatch and unsupported core claims.",
            "modifiers":["core-claim-coverage","license-audit","provenance-chain"],
            "victory_summary":"Every core claim is evidence-bound and every citation and provenance hard gate passes.",
            "role_resources":{"captain_focus":2,"evidence_focus":4,"experiment_focus":2,"run_budget":2,"retained_failure_focus_refund":1}
          }
        }"#,
    )
    .map_err(|error| format!("decode PostgreSQL Evidence Audit test ruleset: {error}"))?;
    let challenge_id = Uuid::new_v4();
    let challenge = ResearchChallenge {
        challenge_id,
        title: EVIDENCE_AUDIT_TITLE.to_string(),
        description: format!("template=evidence-audit; {EVIDENCE_AUDIT_DESCRIPTION_MARKER}"),
        ruleset_version: EVIDENCE_AUDIT_RULESET_VERSION.to_string(),
        ruleset_hash: EVIDENCE_AUDIT_RULESET_HASH.to_string(),
        dataset_manifest_hash: EVIDENCE_AUDIT_DATASET_MANIFEST_HASH.to_string(),
        evaluator_manifest_hash: EVIDENCE_AUDIT_EVALUATOR_MANIFEST_HASH.to_string(),
        ruleset: Some(ruleset),
        status: ChallengeStatus::Draft,
        created_at: Utc::now(),
    };
    state
        .transact(|league| {
            league.challenges.insert(challenge_id, challenge);
            Ok(())
        })
        .await
        .map_err(|error| format!("seed PostgreSQL Evidence Audit Challenge: {error:?}"))?;

    let request = fixture_activation_request(challenge_id);
    validate_activation_request(&request)
        .map_err(|error| format!("validate PostgreSQL activation fixture request: {error:?}"))?;
    let request_sha256 = canonical_json_sha256(&json!({
        "challenge_id": challenge_id,
        "request": request,
    }))
    .map_err(|error| format!("hash PostgreSQL activation fixture request: {error}"))?;
    let result = state
        .activate_challenge_pack_transaction(challenge_id, request.clone(), request_sha256)
        .await
        .map_err(|error| format!("activate PostgreSQL Evidence Audit fixture: {error:?}"))?;
    if result.status != StatusCode::CREATED || !result.applied {
        return Err("PostgreSQL activation fixture was not newly applied".to_string());
    }
    Ok(PostgresActivationFixture {
        challenge_id,
        request,
        record: result.record,
    })
}

#[cfg(test)]
pub(crate) async fn verify_postgres_activation_recovery_and_guards(
    recovered: &AppState,
    migration_pool: &PgPool,
    fixture: PostgresActivationFixture,
) -> Result<(), String> {
    let runtime_pool = recovered
        .pool
        .as_ref()
        .ok_or_else(|| "recovered activation state has no runtime pool".to_string())?;
    let request_sha256 = canonical_json_sha256(&json!({
        "challenge_id": fixture.challenge_id,
        "request": fixture.request,
    }))
    .map_err(|error| format!("hash recovered activation request: {error}"))?;
    let replay = recovered
        .activate_challenge_pack_transaction(fixture.challenge_id, fixture.request, request_sha256)
        .await
        .map_err(|error| format!("replay recovered activation request: {error:?}"))?;
    if replay.status != StatusCode::OK || replay.applied || replay.record != fixture.record {
        return Err("restarted activation did not return the exact immutable replay".to_string());
    }

    let status: Option<String> = sqlx::query_scalar(
        "select state_json #>> array['challenges',$1::text,'status']
         from hepta_league_state where state_key='primary'",
    )
    .bind(fixture.challenge_id.to_string())
    .fetch_one(runtime_pool)
    .await
    .map_err(|error| format!("inspect recovered Challenge JSON status: {error}"))?;
    if status.as_deref() != Some("open") {
        return Err(format!(
            "UUID-keyed Challenge JSON status predicate did not persist open: {status:?}"
        ));
    }
    let activation_rows: i64 = sqlx::query_scalar(
        "select count(*)::bigint from hepta_challenge_pack_activations_v1
         where activation_id=$1 and challenge_id=$2 and request_sha256=$3",
    )
    .bind(fixture.record.activation_id)
    .bind(fixture.challenge_id)
    .bind(&fixture.record.request_sha256)
    .fetch_one(runtime_pool)
    .await
    .map_err(|error| format!("inspect normalized activation row: {error}"))?;
    if activation_rows != 1 {
        return Err("runtime activation did not insert one normalized row".to_string());
    }
    let event_rows: i64 = sqlx::query_scalar(
        "select count(*)::bigint from hepta_outbox
         where event_type='hepta.challenge_pack.activated.v1' and aggregate_id=$1",
    )
    .bind(fixture.challenge_id.to_string())
    .fetch_one(runtime_pool)
    .await
    .map_err(|error| format!("inspect activation outbox event: {error}"))?;
    if event_rows != 1 {
        return Err(format!(
            "activation/replay emitted {event_rows} outbox events instead of one"
        ));
    }
    let runtime_insert: bool = sqlx::query_scalar(
        "select has_table_privilege(current_user,
          'public.hepta_challenge_pack_activations_v1','INSERT')",
    )
    .fetch_one(runtime_pool)
    .await
    .map_err(|error| format!("inspect runtime activation INSERT authority: {error}"))?;
    if !runtime_insert {
        return Err("runtime role lacks activation INSERT authority".to_string());
    }
    let hostile_activation_id = Uuid::new_v4();
    let hostile_challenge_id = Uuid::new_v4();
    let hostile_request_sha256 = format!("sha256:{}", "f".repeat(64));
    let mut hostile_record = serde_json::to_value(&fixture.record)
        .map_err(|error| format!("encode hostile activation record: {error}"))?;
    hostile_record["activation_id"] = json!(hostile_activation_id);
    hostile_record["challenge_id"] = json!(hostile_challenge_id);
    hostile_record["request_sha256"] = json!(hostile_request_sha256);
    let hostile_evidence = hostile_record
        .get_mut("request")
        .and_then(|request| request.get_mut("evidence"))
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| "hostile activation evidence fixture is not an object".to_string())?;
    hostile_evidence.remove("strict_review_evidence_schema");
    hostile_evidence.insert(
        "unexpected_schema_alias".to_string(),
        json!(STRICT_REVIEW_EVIDENCE_V1),
    );
    let hostile_error = sqlx::query(
        "insert into hepta_challenge_pack_activations_v1 (
           activation_id,challenge_id,request_sha256,pack_id,template,
           ruleset_version,ruleset_hash,dataset_manifest_hash,evaluator_manifest_hash,
           candidate_state,integration_base_revision,integration_source_tree,
           hepta_base_revision,hepta_source_tree,source_fileset_sha256,release_id,
           release_image_lock_sha256,release_provenance_sha256,source_catalog_sha256,
           pack_manifest_sha256,cas_activation_receipt_sha256,
           cas_activation_catalog_patch_sha256,strict_review_evidence_sha256,
           strict_review_chain_proof_manifest_sha256,
           strict_review_chain_proof_fileset_sha256,
           strict_review_terminal_bundle_schema,strict_review_terminal_bundle_sha256,
           cross_paper_denial_receipt_sha256,activated_at,record_json
         )
         select $1::uuid,$2::uuid,$3::text,pack_id,template,
           ruleset_version,ruleset_hash,dataset_manifest_hash,evaluator_manifest_hash,
           candidate_state,integration_base_revision,integration_source_tree,
           hepta_base_revision,hepta_source_tree,source_fileset_sha256,release_id,
           release_image_lock_sha256,release_provenance_sha256,source_catalog_sha256,
           pack_manifest_sha256,cas_activation_receipt_sha256,
           cas_activation_catalog_patch_sha256,strict_review_evidence_sha256,
           strict_review_chain_proof_manifest_sha256,
           strict_review_chain_proof_fileset_sha256,
           strict_review_terminal_bundle_schema,strict_review_terminal_bundle_sha256,
           cross_paper_denial_receipt_sha256,activated_at,$4::jsonb
         from hepta_challenge_pack_activations_v1 where activation_id=$5::uuid",
    )
    .bind(hostile_activation_id)
    .bind(hostile_challenge_id)
    .bind(&hostile_request_sha256)
    .bind(hostile_record)
    .bind(fixture.record.activation_id)
    .execute(runtime_pool)
    .await
    .expect_err("missing fixed evidence schema plus unknown replacement unexpectedly inserted");
    if hostile_error
        .as_database_error()
        .and_then(|database| database.code())
        .is_none_or(|code| code.as_ref() != "P0001")
    {
        return Err(format!(
            "missing fixed evidence schema was not rejected by the exact-shape trigger: {hostile_error}"
        ));
    }
    for (operation, query) in [
        (
            "update",
            "update hepta_challenge_pack_activations_v1 set record_json=record_json where activation_id=$1",
        ),
        (
            "delete",
            "delete from hepta_challenge_pack_activations_v1 where activation_id=$1",
        ),
    ] {
        let error = sqlx::query(query)
            .bind(fixture.record.activation_id)
            .execute(runtime_pool)
            .await
            .expect_err("append-only activation mutation unexpectedly succeeded");
        if error
            .as_database_error()
            .and_then(|database| database.code())
            .is_none_or(|code| code.as_ref() != "P0001")
        {
            return Err(format!(
                "activation {operation} was not rejected by the append-only trigger: {error}"
            ));
        }
    }
    let truncate_error = sqlx::query("truncate table hepta_challenge_pack_activations_v1")
        .execute(migration_pool)
        .await
        .expect_err("append-only activation truncate unexpectedly succeeded");
    if truncate_error
        .as_database_error()
        .and_then(|database| database.code())
        .is_none_or(|code| code.as_ref() != "P0001")
    {
        return Err(format!(
            "activation truncate was not rejected by the append-only trigger: {truncate_error}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod activation_catalog_static_tests {
    use super::*;

    #[test]
    fn migration_0049_is_atomic_exact_and_always_guarded() {
        let migration =
            include_str!("../../../migrations/0049_add_hepta_challenge_pack_activation.sql");
        assert!(migration.trim_start().starts_with("begin;"));
        assert!(migration.trim_end().ends_with("commit;"));
        for trigger_name in [
            "hepta_challenge_pack_activation_validate_guard",
            "hepta_challenge_pack_activation_immutable_guard",
            "hepta_challenge_pack_activation_truncate_guard",
        ] {
            assert_eq!(
                migration
                    .matches(&format!("create trigger {trigger_name}"))
                    .count(),
                1
            );
            assert_eq!(
                migration
                    .matches(&format!("enable always trigger {trigger_name}"))
                    .count(),
                1
            );
        }
        let validate = activation_migration_function_body(
            migration,
            "hepta_validate_challenge_pack_activation_v1",
        )
        .expect("0049 validation function body");
        for exact_binding in [
            "hepta.challenge_pack.activation_record.v1",
            "hepta.challenge_pack.activation_request.v1",
            "trnm.paper-raid.current-candidate-binding.v2",
            "hepta.challenge_pack.activation_evidence.v1",
            "trnm.paper-raid.strict-review-evidence.v1",
            "jsonb_typeof(candidate_json -> 'component_pins_authoritative')",
            "jsonb_typeof(candidate_json -> 'working_tree_clean')",
            "jsonb_typeof(evidence_json -> 'cas_all_packs_verified')",
            "jsonb_typeof(evidence_json -> 'cas_scoped_readback')",
            "jsonb_typeof(evidence_json -> 'cas_pack_count')",
            "jsonb_typeof(evidence_json -> 'cas_object_count')",
            "cas_pack_count', '')::integer <> 3",
            "cas_object_count', '')::integer <> 26",
        ] {
            assert!(validate.contains(exact_binding));
        }
        assert!(migration
            .contains("sha256:0000000000000000000000000000000000000000000000000000000000000000"));
        assert!(migration.contains("0000000000000000000000000000000000000000"));
        assert!(!normalize_activation_catalog_sql(validate).contains("or true"));
        let reject = activation_migration_function_body(
            migration,
            "hepta_reject_challenge_pack_activation_mutation_v1",
        )
        .expect("0049 immutable function body");
        assert!(reject.contains("append-only"));
    }

    #[test]
    fn migration_0054_is_atomic_exact_and_chain_proof_bound() {
        let migration =
            include_str!("../../../migrations/0054_bind_challenge_pack_activation_chain_proof.sql");
        assert!(migration.trim_start().starts_with("begin;"));
        assert!(migration.trim_end().ends_with("commit;"));
        assert_eq!(migration.to_ascii_lowercase().matches("begin;").count(), 1);
        assert_eq!(migration.to_ascii_lowercase().matches("commit;").count(), 1);
        assert!(!migration.to_ascii_lowercase().contains("not valid"));
        assert!(!migration.contains("create trigger"));
        for column in [
            "strict_review_chain_proof_manifest_sha256",
            "strict_review_chain_proof_fileset_sha256",
            "strict_review_terminal_bundle_schema",
            "strict_review_terminal_bundle_sha256",
        ] {
            assert!(migration.contains(&format!("add column if not exists {column} text")));
            assert!(migration.contains(&format!("alter column {column} set not null")));
        }
        let validate = activation_migration_function_body(
            migration,
            "hepta_validate_challenge_pack_activation_v1",
        )
        .expect("0054 validation function body");
        assert!(migration
            .contains("Existing activation records are not exact 0054 Chain proof records"));
        for exact_binding in [
            "jsonb_object_keys(evidence_json)) <> 18",
            "trnm.paper-raid.strict-review-terminal-bundle-binding.v1",
            "strict_review_chain_proof_manifest_sha256",
            "strict_review_chain_proof_fileset_sha256",
            "strict_review_terminal_bundle_sha256",
        ] {
            assert!(validate.contains(exact_binding));
        }
        for fixed_binding in [
            "schema'\n            is distinct from 'hepta.challenge_pack.activation_record.v1'",
            "schema'\n            is distinct from 'hepta.challenge_pack.activation_request.v1'",
            "schema'\n            is distinct from 'trnm.paper-raid.current-candidate-binding.v2'",
            "schema'\n            is distinct from 'hepta.challenge_pack.activation_evidence.v1'",
            "cas_activation_receipt_schema'\n            is distinct from 'hepta.challenge_pack.cas_activation_receipt.v1'",
            "strict_review_evidence_schema'\n            is distinct from 'trnm.paper-raid.strict-review-evidence.v1'",
        ] {
            assert!(validate.contains(fixed_binding));
        }
        assert!(!normalize_activation_catalog_sql(validate).contains("or true"));
        let case_mutated_schema = validate.replacen(
            "trnm.paper-raid.strict-review-terminal-bundle-binding.v1",
            "TRNM.paper-raid.strict-review-terminal-bundle-binding.v1",
            1,
        );
        assert_ne!(
            normalize_activation_catalog_sql(validate),
            normalize_activation_catalog_sql(&case_mutated_schema)
        );
        assert!(migration.contains("jsonb_object_keys(\n                        activation.record_json #> '{request,evidence}'"));
        assert!(migration
            .contains("is distinct from activation.strict_review_chain_proof_manifest_sha256"));
        assert!(
            migration.contains("is distinct from activation.strict_review_terminal_bundle_sha256")
        );

        let digest_columns = [
            "source_fileset_sha256",
            "release_image_lock_sha256",
            "release_provenance_sha256",
            "cas_activation_receipt_sha256",
            "cas_activation_catalog_patch_sha256",
            "strict_review_evidence_sha256",
            "strict_review_chain_proof_manifest_sha256",
            "strict_review_chain_proof_fileset_sha256",
            "strict_review_terminal_bundle_sha256",
            "cross_paper_denial_receipt_sha256",
        ];
        let zero = format!("sha256:{}", "0".repeat(64));
        let exact_definition = format!(
            "CHECK ({} AND {})",
            digest_columns
                .iter()
                .map(|column| format!("{column} ~ '^sha256:[0-9a-f]{{64}}$'::text"))
                .collect::<Vec<_>>()
                .join(" AND "),
            digest_columns
                .iter()
                .map(|column| format!("{column} <> '{zero}'::text"))
                .collect::<Vec<_>>()
                .join(" AND ")
        );
        assert!(activation_evidence_digest_constraint_definition_is_exact(
            &exact_definition
        ));
        assert!(!activation_evidence_digest_constraint_definition_is_exact(
            &format!("{exact_definition} OR TRUE")
        ));
        assert!(!activation_evidence_digest_constraint_definition_is_exact(
            &exact_definition.replace(
                "strict_review_terminal_bundle_sha256 ~ '^sha256:[0-9a-f]{64}$'::text",
                "strict_review_terminal_bundle_sha256 IS NOT NULL"
            )
        ));
        assert!(!activation_evidence_digest_constraint_definition_is_exact(
            &exact_definition.replacen("[0-9a-f]", "[0-9A-F]", 1)
        ));
        assert!(!activation_evidence_digest_constraint_definition_is_exact(
            &exact_definition.replacen("'^sha256:", "'^ sha256:", 1)
        ));
    }

    #[test]
    fn activation_request_rejects_zero_digest_and_git_sentinels() {
        let challenge_id = Uuid::new_v4();
        for digest_field in [
            "strict_review_evidence_sha256",
            "strict_review_chain_proof_manifest_sha256",
            "strict_review_chain_proof_fileset_sha256",
            "strict_review_terminal_bundle_sha256",
        ] {
            let mut request = fixture_activation_request(challenge_id);
            let zero = format!("sha256:{}", "0".repeat(64));
            match digest_field {
                "strict_review_evidence_sha256" => {
                    request.evidence.strict_review_evidence_sha256 = zero
                }
                "strict_review_chain_proof_manifest_sha256" => {
                    request.evidence.strict_review_chain_proof_manifest_sha256 = zero
                }
                "strict_review_chain_proof_fileset_sha256" => {
                    request.evidence.strict_review_chain_proof_fileset_sha256 = zero
                }
                "strict_review_terminal_bundle_sha256" => {
                    request.evidence.strict_review_terminal_bundle_sha256 = zero
                }
                _ => unreachable!(),
            }
            assert!(validate_activation_request(&request).is_err());
        }

        let mut request = fixture_activation_request(challenge_id);
        request.evidence.strict_review_terminal_bundle_schema = "unexpected".to_string();
        assert!(validate_activation_request(&request).is_err());

        let mut request = fixture_activation_request(challenge_id);
        request.candidate.hepta_base_revision = "0".repeat(40);
        assert!(validate_activation_request(&request).is_err());
    }
}
